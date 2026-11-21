//! The headset session: one thread that owns the Bluetooth link, keeps a
//! snapshot of what the headset reports, and carries out UI commands.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::bluetooth::link::{Incoming, Link};
use crate::bluetooth::{self, audio, SonyDevice};
use crate::protocol::framing::{pack, Frame, TYPE_ACK, TYPE_DATA_MDR, TYPE_DATA_MDR_NO2};
use crate::protocol::{
    eq_preset_is_editable, v1, v2, Battery, BatterySlot, EqState, Event, Generation, NoiseMode,
    NoiseState, Playback, Switch, Transport, EQ_MANUAL,
};

const ACK_TIMEOUT: Duration = Duration::from_millis(1000);
/// How long a question may go unanswered before the next one is asked.
const REPLY_TIMEOUT: Duration = Duration::from_millis(700);
const SEND_ATTEMPTS: usize = 3;
const REFRESH_EVERY: Duration = Duration::from_secs(60);
const RESCAN_EVERY: Duration = Duration::from_secs(30);
/// How long after the user flips a switch a question from the headset is
/// taken to be about that switch.
const CONSENT_WINDOW: Duration = Duration::from_secs(15);

/// With `OHC_TRACE=1` in the environment every message to and from the
/// headset is written to the log: the way to see what an untested model says.
fn tracing() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("OHC_TRACE").is_some_and(|value| value != "0"))
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Before the first Bluetooth check has finished.
    #[default]
    Starting,
    BluetoothOff,
    NoDevice,
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Pending {
    ConnectingAudio,
    Disconnecting,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Batteries {
    pub single: Option<Battery>,
    pub left: Option<Battery>,
    pub right: Option<Battery>,
    pub case: Option<Battery>,
}

impl Batteries {
    /// The number shown in the tray: the headset, or the weaker earbud.
    pub fn headline(&self) -> Option<Battery> {
        self.single.or(match (self.left, self.right) {
            (Some(l), Some(r)) => Some(if l.level <= r.level { l } else { r }),
            (l, r) => l.or(r),
        })
    }
}

/// One of the headset's on/off settings and where it stands.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct SwitchState {
    pub id: Switch,
    pub on: bool,
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub phase: Phase,
    /// The name Windows knows the headset by, which the owner can change.
    pub device_name: Option<String>,
    /// The model as the headset itself reports it, and Sony's colour code
    /// for this unit.
    pub model: Option<String>,
    pub colour: Option<u8>,
    pub battery: Batteries,
    pub noise: Option<NoiseState>,
    pub noise_modes: Vec<NoiseMode>,
    pub min_ambient_level: u8,
    pub max_ambient_level: u8,
    pub eq: Option<EqState>,
    /// The presets this model has, where it lists them. Empty: not listed.
    pub eq_presets: Vec<u8>,
    /// The frequency of each band in hertz, where the headset says.
    pub eq_frequencies: Vec<u32>,
    pub firmware: Option<String>,
    pub codec: Option<String>,
    pub can_power_off: bool,
    pub focus_on_voice_available: bool,
    /// Headset volume, 0 to `max_volume`. None where it cannot be set from here.
    pub volume: Option<u8>,
    pub max_volume: u8,
    /// Present when play, pause and skip can be sent through the headset.
    pub playback: Option<Playback>,
    /// The on/off settings this headset has, in a fixed order.
    pub switches: Vec<SwitchState>,
    /// The automatic power-off choice in effect, with the choices on offer.
    pub auto_power_off: Option<u8>,
    pub auto_power_off_options: Vec<u8>,
    pub pending: Option<Pending>,
    /// A problem worth telling the user about, already phrased for them.
    pub notice: Option<String>,
    /// Goes up by one each time the headset connects afresh (switched on,
    /// back in range), but not when the control link merely recovers from a
    /// hiccup. The connect card is shown whenever this changes.
    pub connections: u64,
}

impl Snapshot {
    pub fn switch(&self, id: Switch) -> Option<bool> {
        self.switches.iter().find(|s| s.id == id).map(|s| s.on)
    }

    fn set_switch(&mut self, id: Switch, on: bool) {
        match self.switches.iter_mut().find(|s| s.id == id) {
            Some(existing) => existing.on = on,
            None => {
                self.switches.push(SwitchState { id, on });
                self.switches.sort_by_key(|s| s.id);
            }
        }
    }
}

/// Tells a fresh connection (the headset was switched on) from the control
/// link recovering after a brief hiccup.
///
/// It does not rely on catching Windows in the act of reporting the headset
/// as disconnected: a quick off-and-on can slip between two looks. A link
/// that stayed down for a few seconds counts as the headset having been away.
#[derive(Debug)]
pub struct Arrival {
    away: bool,
    lost_at: Option<Instant>,
}

impl Arrival {
    /// A link that comes back sooner than this was a hiccup. Switching a
    /// headset off and on again takes a good deal longer.
    pub const HICCUP: Duration = Duration::from_secs(3);

    pub fn new() -> Self {
        // The first connection after the app starts is always a fresh one.
        Arrival { away: true, lost_at: None }
    }

    /// Windows reports the headset as not connected, or Bluetooth as off.
    pub fn gone(&mut self) {
        self.away = true;
    }

    /// A working link went down at `now`.
    pub fn lost(&mut self, now: Instant) {
        self.lost_at.get_or_insert(now);
    }

    /// A link is working again at `now`. True if this is a fresh connection.
    pub fn back(&mut self, now: Instant) -> bool {
        let long_gap = self.lost_at.take().is_some_and(|at| now.duration_since(at) >= Self::HICCUP);
        let fresh = self.away || long_gap;
        self.away = false;
        fresh
    }
}

impl Default for Arrival {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetNoise(NoiseState),
    SetEqPreset(u8),
    SetEqBands { clear_bass: Option<i8>, bands: Vec<i8> },
    SetVolume(u8),
    Transport(Transport),
    SetSwitch(Switch, bool),
    SetAutoPowerOff(u8),
    /// Raised by the session itself when the headset asks a question.
    AnswerAlert { message: u8, yes: bool },
    PowerOff,
    Connect,
    Disconnect,
}

enum Msg {
    Command(Command),
    Frame(u64, Frame),
    Closed(u64, Option<String>),
}

#[derive(Clone)]
pub struct Handle {
    tx: Sender<Msg>,
}

impl Handle {
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(Msg::Command(command));
    }
}

/// Starts the session thread. `on_change` runs on that thread every time the
/// snapshot changes.
pub fn spawn(on_change: impl Fn(&Snapshot) + Send + 'static) -> Handle {
    let (tx, rx) = mpsc::channel();
    let handle = Handle { tx: tx.clone() };
    thread::Builder::new()
        .name("headset-session".into())
        .spawn(move || {
            Session {
                rx,
                tx,
                on_change: Box::new(on_change),
                snap: Snapshot::default(),
                published: None,
                device: None,
                queue: VecDeque::new(),
                hold_off: false,
                next_link_id: 0,
                arrival: Arrival::new(),
                connections: 0,
                consent_until: None,
            }
            .run()
        })
        .expect("failed to start the headset session thread");
    handle
}

/// Starts a made-up headset of the given model instead of looking for a real
/// one, for trying the app without headphones (`--demo WH-1000XM5`). Nothing
/// here touches Bluetooth; the controls change the made-up state.
pub fn spawn_demo(model: String, on_change: impl Fn(&Snapshot) + Send + 'static) -> Handle {
    let (tx, rx) = mpsc::channel();
    let handle = Handle { tx };
    thread::Builder::new()
        .name("demo-session".into())
        .spawn(move || {
            let mut snap = demo_snapshot(&model, 1);
            on_change(&Snapshot {
                phase: Phase::Connecting,
                device_name: Some(model.clone()),
                ..Snapshot::default()
            });
            // As long as a real headset takes to answer.
            thread::sleep(Duration::from_millis(900));
            on_change(&snap);
            for message in rx {
                if let Msg::Command(command) = message {
                    apply_demo(&mut snap, &model, command);
                    on_change(&snap);
                }
            }
        })
        .expect("failed to start the demo session thread");
    handle
}

fn demo_snapshot(model: &str, connections: u64) -> Snapshot {
    let battery = |level| Some(Battery { level, charging: false });
    let mut snap = Snapshot {
        phase: Phase::Connected,
        device_name: Some(model.into()),
        model: Some(model.into()),
        battery: if bluetooth::is_earbuds(model) {
            Batteries { left: battery(90), right: battery(100), case: battery(60), ..Batteries::default() }
        } else {
            Batteries { single: battery(80), ..Batteries::default() }
        },
        noise: Some(NoiseState { mode: NoiseMode::NoiseCancelling, ambient_level: 12, focus_on_voice: false }),
        noise_modes: vec![NoiseMode::NoiseCancelling, NoiseMode::Ambient, NoiseMode::Off],
        min_ambient_level: 1,
        max_ambient_level: 20,
        focus_on_voice_available: true,
        eq: Some(EqState { preset: 0x00, clear_bass: Some(0), bands: vec![0; 5], range: 10 }),
        firmware: Some("demo".into()),
        codec: Some("LDAC".into()),
        can_power_off: true,
        volume: Some(21),
        max_volume: 30,
        playback: Some(Playback::Paused),
        auto_power_off: Some(0x00),
        auto_power_off_options: vec![0x00, 0x01, 0x02, 0x03, 0x11],
        connections,
        ..Snapshot::default()
    };
    for (id, on) in [
        (Switch::Upscaling, true),
        (Switch::SpeakToChat, false),
        (Switch::TouchPanel, true),
        (Switch::VoiceGuidance, true),
    ] {
        snap.set_switch(id, on);
    }
    snap
}

fn apply_demo(snap: &mut Snapshot, model: &str, command: Command) {
    let connected = snap.phase == Phase::Connected;
    match command {
        Command::Connect if !connected => *snap = demo_snapshot(model, snap.connections + 1),
        Command::Disconnect | Command::PowerOff if connected => {
            *snap = Snapshot {
                phase: Phase::Disconnected,
                device_name: Some(model.into()),
                connections: snap.connections,
                ..Snapshot::default()
            };
        }
        _ if !connected => {}
        Command::SetNoise(state) => snap.noise = Some(state),
        Command::SetEqPreset(preset) => {
            if let Some(eq) = snap.eq.as_mut() {
                eq.preset = preset;
            }
        }
        Command::SetEqBands { clear_bass, bands } => {
            if let Some(eq) = snap.eq.as_mut() {
                eq.preset = EQ_MANUAL;
                eq.clear_bass = clear_bass;
                eq.bands = bands;
            }
        }
        Command::SetVolume(volume) => snap.volume = Some(volume.min(snap.max_volume)),
        Command::Transport(Transport::Play) => snap.playback = Some(Playback::Playing),
        Command::Transport(Transport::Pause) => snap.playback = Some(Playback::Paused),
        Command::SetSwitch(id, on) => snap.set_switch(id, on),
        Command::SetAutoPowerOff(choice) => snap.auto_power_off = Some(choice),
        Command::Transport(_) | Command::AnswerAlert { .. } | Command::Connect => {}
        Command::Disconnect | Command::PowerOff => {}
    }
}

enum LinkError {
    /// The socket is gone; the reason is for the log.
    Closed(String),
    NoAck,
}

impl LinkError {
    fn describe(&self) -> String {
        match self {
            LinkError::Closed(why) => why.clone(),
            LinkError::NoAck => "the headset stopped acknowledging commands".into(),
        }
    }
}

type LinkResult<T> = Result<T, LinkError>;

/// What is known about a first-generation headset on this link.
#[derive(Default)]
struct V1Link {
    /// What the headset says it can do. Empty if it never answered, in which
    /// case the essentials are tried anyway and the extras are left alone.
    functions: Vec<u8>,
    /// Which noise-control message it uses, if it has noise control.
    noise: Option<v1::NoiseKind>,
    wind_mode: bool,
    earbuds: bool,
}

impl V1Link {
    /// Whether to use a function: yes if advertised, and also if the headset
    /// never said what it supports.
    fn may_have(&self, function: u8) -> bool {
        self.functions.is_empty() || self.functions.contains(&function)
    }

    fn has(&self, function: u8) -> bool {
        self.functions.contains(&function)
    }
}

/// What is known about a second-generation headset on this link.
#[derive(Default)]
struct V2Link {
    caps: v2::Capabilities,
    /// The last noise-control message received: the next change starts from it.
    noise_raw: Option<Vec<u8>>,
    eq_layout: v2::EqLayout,
    /// The last equalizer values received, on the headset's own scale.
    eq_raw: Option<(u8, Vec<u8>)>,
    voice_language: Option<u8>,
}

enum Dialect {
    V1(V1Link),
    V2(V2Link),
}

struct Conn {
    link: Link,
    id: u64,
    seq: u8,
    acked: bool,
    /// The frame type and command number of the answer being waited for.
    awaiting: Option<(u8, u8)>,
    answered: bool,
    got_protocol_info: bool,
    has_table2: bool,
    support_functions: Option<Vec<u8>>,
    support_functions2: Option<Vec<u8>>,
    /// Numbered general settings that turned out to be switches the app knows.
    general: Vec<(u8, Switch)>,
    /// General settings the headset has described so far.
    described: Vec<u8>,
    dialect: Dialect,
}

/// The message that flips `id`, and the frame type it travels in. `None`
/// where this headset has no such switch.
fn switch_command(conn: &Conn, id: Switch, on: bool) -> Option<(u8, Vec<u8>)> {
    let slot = conn.general.iter().find(|(_, switch)| *switch == id).map(|(slot, _)| *slot);
    Some(match (&conn.dialect, id) {
        (Dialect::V1(_), Switch::TouchPanel | Switch::Multipoint | Switch::Sidetone) => {
            (TYPE_DATA_MDR, v1::set_general_setting(slot?, on))
        }
        (Dialect::V1(_), Switch::Upscaling) => (TYPE_DATA_MDR, v1::set_upscaling(on)),
        (Dialect::V1(_), Switch::SpeakToChat) => (TYPE_DATA_MDR, v1::set_speak_to_chat(on)),
        (Dialect::V1(_), Switch::PauseWhenRemoved) => (TYPE_DATA_MDR, v1::set_pause_when_removed(on)),
        (Dialect::V1(_), Switch::VoiceGuidance) => (TYPE_DATA_MDR_NO2, v1::set_voice_guidance(on)),
        (Dialect::V2(_), Switch::TouchPanel | Switch::Multipoint | Switch::Sidetone) => {
            (TYPE_DATA_MDR, v2::set_general_setting(slot?, on))
        }
        (Dialect::V2(_), Switch::Upscaling) => (TYPE_DATA_MDR, v2::set_upscaling(on)),
        (Dialect::V2(link), Switch::SpeakToChat) => {
            (TYPE_DATA_MDR, v2::set_speak_to_chat(link.caps.speak_to_chat?, on))
        }
        (Dialect::V2(_), Switch::PauseWhenRemoved) => (TYPE_DATA_MDR, v2::set_pause_when_removed(on)),
        (Dialect::V2(link), Switch::VoiceGuidance) => (
            TYPE_DATA_MDR_NO2,
            v2::set_voice_guidance(link.caps.voice_guidance?, on, link.voice_language),
        ),
    })
}

struct Session {
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    on_change: Box<dyn Fn(&Snapshot) + Send>,
    snap: Snapshot,
    published: Option<Snapshot>,
    device: Option<SonyDevice>,
    queue: VecDeque<Command>,
    /// Set after the user disconnects, so the still-up radio link is not
    /// immediately reopened.
    hold_off: bool,
    next_link_id: u64,
    arrival: Arrival,
    connections: u64,
    /// Set when the user flips a switch that the headset will ask about
    /// before obeying; a question arriving before this time is answered yes.
    consent_until: Option<Instant>,
}

impl Session {
    fn publish(&mut self) {
        self.snap.connections = self.connections;
        if self.published.as_ref() != Some(&self.snap) {
            self.published = Some(self.snap.clone());
            (self.on_change)(&self.snap);
        }
    }

    fn set_phase(&mut self, phase: Phase) {
        self.snap.phase = phase;
        self.publish();
    }

    /// Forgets everything the headset told us.
    fn clear_live(&mut self) {
        let keep_name = self.snap.device_name.take();
        let keep_notice = self.snap.notice.take();
        self.snap = Snapshot {
            phase: self.snap.phase,
            device_name: keep_name,
            notice: keep_notice,
            ..Snapshot::default()
        };
    }

    fn run(mut self) {
        bluetooth::init_thread();
        let mut last_scan: Option<Instant> = None;
        let mut failures: u32 = 0;
        loop {
            if !bluetooth::radio_is_on() {
                self.device = None;
                self.arrival.gone();
                self.snap = Snapshot { phase: Phase::BluetoothOff, ..Snapshot::default() };
                self.publish();
                self.idle(Duration::from_secs(3));
                continue;
            }

            let stale = last_scan.is_none_or(|at| at.elapsed() > RESCAN_EVERY);
            let offline = self.device.as_ref().is_none_or(|d| !d.is_connected());
            if stale && offline {
                self.device = bluetooth::find_sony_device();
                last_scan = Some(Instant::now());
            }
            let Some(device) = self.device.as_ref() else {
                self.arrival.gone();
                self.snap = Snapshot { phase: Phase::NoDevice, ..Snapshot::default() };
                self.publish();
                self.idle(Duration::from_secs(4));
                continue;
            };
            self.snap.device_name = Some(device.name.clone());

            let connected = device.is_connected();
            if !connected {
                self.hold_off = false;
                failures = 0;
            }
            if !connected || self.hold_off {
                self.arrival.gone();
                self.set_phase(Phase::Disconnected);
                if let Some(Command::Connect) = self.idle(Duration::from_millis(1500)) {
                    self.hold_off = false;
                    self.connect_audio();
                }
                continue;
            }

            self.snap.notice = None;
            self.set_phase(Phase::Connecting);
            let device = self.device.as_ref().expect("checked above");
            match Link::open(device) {
                Ok((link, generation)) => {
                    crate::log!("Control link open ({generation:?}) to {}", device.name);
                    let ended = self.serve(link, generation);
                    crate::log!("Control link ended: {ended}");
                    let was_working = self.snap.phase == Phase::Connected;
                    if was_working {
                        self.arrival.lost(Instant::now());
                    }
                    failures = if was_working { 0 } else { failures + 1 };
                }
                Err(e) => {
                    failures += 1;
                    crate::log!("Could not open the control link: {e}");
                }
            }
            self.clear_live();
            if self.hold_off {
                continue;
            }
            if failures >= 3 {
                self.snap.notice = Some(
                    "Connected for audio, but the headphones aren't answering control requests. \
                     Close the Sony app on other devices, or turn the headphones off and on."
                        .into(),
                );
            }
            self.set_phase(Phase::Connecting);
            let backoff = Duration::from_millis(800 * 2u64.pow(failures.min(4)));
            if let Some(Command::Disconnect) = self.idle(backoff) {
                self.disconnect_audio();
            }
        }
    }

    /// Waits while nothing is connected. Returns early for the two commands
    /// that make sense in that state; everything else is dropped.
    fn idle(&mut self, duration: Duration) -> Option<Command> {
        let deadline = Instant::now() + duration;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(Msg::Command(c @ (Command::Connect | Command::Disconnect))) => return Some(c),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    fn audio_target(&self) -> Option<audio::Target<'_>> {
        self.device.as_ref().map(|d| audio::Target {
            container_id: d.container_id,
            name: &d.name,
            address_hex: d.address_hex(),
        })
    }

    fn connect_audio(&mut self) {
        self.snap.pending = Some(Pending::ConnectingAudio);
        self.snap.notice = None;
        self.publish();
        let result = self.audio_target().map(|t| audio::connect(&t));
        let name = self.snap.device_name.clone().unwrap_or_else(|| "the headphones".into());
        match result {
            Some(Ok(())) => {
                let deadline = Instant::now() + Duration::from_secs(10);
                while Instant::now() < deadline
                    && !self.device.as_ref().is_some_and(|d| d.is_connected())
                {
                    thread::sleep(Duration::from_millis(250));
                }
                if !self.device.as_ref().is_some_and(|d| d.is_connected()) {
                    self.snap.notice =
                        Some(format!("Couldn't reach {name}. Check that it's switched on and nearby."));
                }
            }
            Some(Err(message)) => self.snap.notice = Some(message),
            None => {}
        }
        self.snap.pending = None;
        self.publish();
    }

    fn disconnect_audio(&mut self) {
        self.snap.pending = Some(Pending::Disconnecting);
        self.publish();
        if let Some(Err(message)) = self.audio_target().map(|t| audio::disconnect(&t)) {
            crate::log!("Disconnect request failed: {message}");
        }
        self.hold_off = true;
        self.snap.pending = None;
    }

    // ----- a live link -----------------------------------------------------

    fn serve(&mut self, link: Link, generation: Generation) -> String {
        self.next_link_id += 1;
        let id = self.next_link_id;
        let tx = self.tx.clone();
        link.spawn_reader(move |incoming| {
            let _ = tx.send(match incoming {
                Incoming::Frame(frame) => Msg::Frame(id, frame),
                Incoming::Closed(why) => Msg::Closed(id, why),
            });
        });

        let name = self.snap.device_name.clone().unwrap_or_default();
        let mut conn = Conn {
            link,
            id,
            seq: 0,
            acked: false,
            awaiting: None,
            answered: false,
            got_protocol_info: false,
            has_table2: false,
            support_functions: None,
            support_functions2: None,
            general: Vec::new(),
            described: Vec::new(),
            dialect: match generation {
                // Guesses from the name, replaced by what the headset reports.
                Generation::V1 => Dialect::V1(V1Link {
                    wind_mode: bluetooth::has_wind_mode(&name),
                    earbuds: bluetooth::is_earbuds(&name),
                    ..V1Link::default()
                }),
                Generation::V2 => Dialect::V2(V2Link::default()),
            },
        };
        self.queue.clear();
        self.consent_until = None;

        if let Err(e) = self.handshake(&mut conn) {
            return e.describe();
        }
        self.snap.notice = None;
        if self.arrival.back(Instant::now()) {
            self.connections += 1;
            crate::log!("Headphones connected (connection {} since the app started)", self.connections);
        }
        self.set_phase(Phase::Connected);

        // The connect card can appear now; the remaining settings fill in
        // behind it.
        if let Err(e) = self.learn_extras(&mut conn) {
            return e.describe();
        }
        self.publish();

        let mut last_refresh = Instant::now();
        loop {
            while let Some(command) = self.next_command() {
                match self.execute(&mut conn, command) {
                    Ok(true) => {}
                    Ok(false) => return "ended on request".into(),
                    Err(e) => return e.describe(),
                }
                self.publish();
            }
            if last_refresh.elapsed() > REFRESH_EVERY {
                last_refresh = Instant::now();
                if let Err(e) = self.refresh(&mut conn) {
                    return e.describe();
                }
            }
            if let Err(e) = self.pump(&mut conn, Duration::from_secs(1), true, |_, _| false) {
                return e.describe();
            }
            if !self.device.as_ref().is_some_and(|d| d.is_connected()) {
                return "the headset disconnected".into();
            }
        }
    }

    /// The essentials: enough to show the battery and the noise mode.
    fn handshake(&mut self, conn: &mut Conn) -> LinkResult<()> {
        match conn.dialect {
            Dialect::V1(_) => self.handshake_v1(conn)?,
            Dialect::V2(_) => self.handshake_v2(conn)?,
        }
        for query in self.state_queries(conn, true) {
            self.send_optional(conn, &query)?;
        }
        // Give the answers a moment to land so the first thing shown is
        // complete. Not every headset has noise control to wait for.
        self.pump(conn, Duration::from_millis(1200), false, |_, snap| {
            snap.battery.headline().is_some() && (snap.noise.is_some() || snap.noise_modes.is_empty())
        })?;
        Ok(())
    }

    fn handshake_v1(&mut self, conn: &mut Conn) -> LinkResult<()> {
        self.send(conn, &v1::get_protocol_info())?;
        // The original client never waited for this reply, so it is welcome
        // but not required.
        self.pump(conn, Duration::from_millis(1500), false, |c, _| c.got_protocol_info)?;

        // Ask the headset what it can do rather than guessing from its name.
        self.send_optional(conn, &v1::get_support_functions())?;
        self.pump(conn, Duration::from_millis(1500), false, |c, _| c.support_functions.is_some())?;
        let functions = conn.support_functions.clone().unwrap_or_default();
        if functions.is_empty() {
            crate::log!("The headset did not list its functions; going by its name");
        } else {
            crate::log!("Headset functions: {functions:02x?}");
        }

        let Dialect::V1(link) = &mut conn.dialect else { return Ok(()) };
        // A headset that listed nothing is assumed to be of the WH-1000X kind.
        link.noise =
            if functions.is_empty() { Some(v1::NoiseKind::Both) } else { v1::NoiseKind::for_functions(&functions) };
        link.functions = functions;
        let kind = link.noise;
        // Used until the headset describes its own noise control just below.
        self.snap.noise_modes = match (kind, link.wind_mode) {
            (None, _) => Vec::new(),
            (Some(v1::NoiseKind::CancellingOnly), _) => vec![NoiseMode::NoiseCancelling, NoiseMode::Off],
            (Some(v1::NoiseKind::AmbientOnly), _) => vec![NoiseMode::Ambient, NoiseMode::Off],
            (Some(v1::NoiseKind::Both), true) => {
                vec![NoiseMode::NoiseCancelling, NoiseMode::Wind, NoiseMode::Ambient, NoiseMode::Off]
            }
            (Some(v1::NoiseKind::Both), false) => {
                vec![NoiseMode::NoiseCancelling, NoiseMode::Ambient, NoiseMode::Off]
            }
        };
        let ambient = matches!(kind, Some(v1::NoiseKind::Both | v1::NoiseKind::AmbientOnly));
        self.snap.min_ambient_level = if ambient { v1::MIN_AMBIENT_LEVEL } else { 0 };
        self.snap.max_ambient_level = if ambient { v1::DEFAULT_MAX_AMBIENT_LEVEL } else { 0 };
        self.snap.focus_on_voice_available = ambient;
        self.snap.can_power_off = link.has(v1::function::POWER_OFF);
        if let Some(kind) = kind {
            self.send_optional(conn, &v1::get_noise_capability(kind))?;
        }
        Ok(())
    }

    fn handshake_v2(&mut self, conn: &mut Conn) -> LinkResult<()> {
        self.send(conn, &v2::get_protocol_info())?;
        if !self.pump(conn, Duration::from_secs(3), false, |c, _| c.got_protocol_info)? {
            return Err(LinkError::Closed("no protocol reply from the headset".into()));
        }
        // These headsets list their functions per command table.
        self.send(conn, &v2::get_support_functions())?;
        self.pump(conn, Duration::from_secs(3), false, |c, _| c.support_functions.is_some())?;
        if conn.has_table2 {
            self.send_optional_as(conn, TYPE_DATA_MDR_NO2, &v2::get_support_functions())?;
            self.pump(conn, Duration::from_millis(1500), false, |c, _| c.support_functions2.is_some())?;
        }
        let table1 = conn.support_functions.clone().unwrap_or_default();
        let table2 = conn.support_functions2.clone().unwrap_or_default();
        if table1.is_empty() {
            crate::log!("The headset did not list its functions; assuming the common kind");
        } else {
            crate::log!("Headset functions: {table1:02x?}, second table: {table2:02x?}");
        }

        let caps = v2::Capabilities::resolve(&table1, &table2);
        self.snap.noise_modes = caps.noise.map(|layout| layout.modes()).unwrap_or_default();
        let levels = caps.noise.is_some_and(|layout| layout.has_level());
        self.snap.min_ambient_level = if levels { v2::MIN_AMBIENT_LEVEL } else { 0 };
        self.snap.max_ambient_level = if levels { v2::MAX_AMBIENT_LEVEL } else { 0 };
        self.snap.focus_on_voice_available = caps.noise.is_some_and(|layout| layout.has_voice_focus());
        self.snap.can_power_off = caps.power_off;
        conn.dialect = Dialect::V2(V2Link { caps, ..V2Link::default() });
        Ok(())
    }

    /// Everything beyond the essentials, for headsets that said they have it.
    fn learn_extras(&mut self, conn: &mut Conn) -> LinkResult<()> {
        match &conn.dialect {
            Dialect::V1(link) => {
                let functions = link.functions.clone();
                self.learn_extras_v1(conn, &functions)
            }
            Dialect::V2(link) => {
                let caps = link.caps.clone();
                self.learn_extras_v2(conn, &caps)
            }
        }
    }

    fn learn_extras_v1(&mut self, conn: &mut Conn, functions: &[u8]) -> LinkResult<()> {
        let has = |function: u8| functions.contains(&function);

        self.send_optional(conn, &v1::get_series_and_colour())?;
        if has(v1::function::PLAYBACK_CONTROLLER) {
            self.snap.max_volume = v1::MAX_VOLUME;
            self.snap.playback = Some(Playback::Unknown);
            self.send_optional(conn, &v1::get_volume())?;
            self.send_optional(conn, &v1::get_playback())?;
        }
        if has(v1::function::UPSCALING) {
            self.send_optional(conn, &v1::get_upscaling())?;
        }
        if has(v1::function::SMART_TALKING_MODE) {
            self.send_optional(conn, &v1::get_speak_to_chat())?;
        }
        if has(v1::function::CONTROL_BY_WEARING) {
            self.send_optional(conn, &v1::get_pause_when_removed())?;
        }
        if has(v1::function::AUTO_POWER_OFF) {
            self.send_optional(conn, &v1::get_auto_power_off_options())?;
            self.send_optional(conn, &v1::get_auto_power_off())?;
        }
        for slot in v1::function::GENERAL_SETTINGS {
            if has(slot) {
                self.learn_general_setting(
                    conn,
                    slot,
                    &v1::get_general_setting_info(slot),
                    &v1::get_general_setting(slot),
                )?;
            }
        }
        if has(v1::function::VOICE_GUIDANCE) {
            self.send_optional_as(conn, TYPE_DATA_MDR_NO2, &v1::get_voice_guidance())?;
        }
        Ok(())
    }

    fn learn_extras_v2(&mut self, conn: &mut Conn, caps: &v2::Capabilities) -> LinkResult<()> {
        self.send_optional(conn, &v2::get_firmware_version())?;
        self.send_optional(conn, &v2::get_series_and_colour())?;
        if caps.alerts {
            self.tell_optional(conn, &v2::enable_alerts())?;
        }
        if caps.codec {
            self.send_optional(conn, &v2::get_codec())?;
        }
        if caps.playback {
            self.snap.max_volume = v2::MAX_VOLUME;
            self.snap.playback = Some(Playback::Unknown);
            self.send_optional(conn, &v2::get_volume())?;
            self.send_optional(conn, &v2::get_playback())?;
        }
        if let Some(kind) = caps.eq {
            // How the values are scaled comes first, so that they are read
            // right the first time.
            self.send_optional(conn, &v2::get_eq_layout(kind))?;
            self.send_optional(conn, &v2::get_eq_frequencies())?;
            self.send_optional(conn, &v2::get_eq())?;
        }
        if caps.upscaling {
            self.send_optional(conn, &v2::get_upscaling())?;
        }
        if let Some(kind) = caps.speak_to_chat {
            self.send_optional(conn, &v2::get_speak_to_chat(kind))?;
        }
        if caps.pause_when_removed {
            self.send_optional(conn, &v2::get_pause_when_removed())?;
        }
        if let Some(kind) = caps.auto_power_off {
            // Stands until the headset lists its own choices.
            self.snap.auto_power_off_options = v2::default_auto_power_off_options(kind);
            self.send_optional(conn, &v2::get_auto_power_off_options(kind))?;
            self.send_optional(conn, &v2::get_auto_power_off(kind))?;
        }
        for slot in caps.general_settings.iter().copied() {
            self.learn_general_setting(
                conn,
                slot,
                &v2::get_general_setting_info(slot),
                &v2::get_general_setting(slot),
            )?;
        }
        if let Some(kind) = caps.voice_guidance {
            self.send_optional_as(conn, TYPE_DATA_MDR_NO2, &v2::get_voice_guidance(kind))?;
        }
        Ok(())
    }

    /// Each numbered general setting has to say what it is before it can be
    /// used; its value is only worth asking for if it is a switch the app knows.
    fn learn_general_setting(
        &mut self,
        conn: &mut Conn,
        slot: u8,
        describe: &[u8],
        read: &[u8],
    ) -> LinkResult<()> {
        self.send_optional(conn, describe)?;
        self.pump(conn, Duration::from_millis(600), false, |c, _| c.described.contains(&slot))?;
        if conn.general.iter().any(|(known, _)| *known == slot) {
            self.send_optional(conn, read)?;
        }
        Ok(())
    }

    fn state_queries(&self, conn: &Conn, everything: bool) -> Vec<Vec<u8>> {
        let mut queries = Vec::new();
        match &conn.dialect {
            Dialect::V1(link) => {
                let listed = !link.functions.is_empty();
                let split = if listed {
                    link.has(v1::function::LEFT_RIGHT_BATTERY_LEVEL)
                } else {
                    link.earbuds
                };
                if split {
                    queries.push(v1::get_battery(BatterySlot::Left));
                }
                if link.has(v1::function::CRADLE_BATTERY_LEVEL) || (!listed && link.earbuds) {
                    queries.push(v1::get_battery(BatterySlot::Case));
                }
                if link.has(v1::function::BATTERY_LEVEL) || (!listed && !link.earbuds) {
                    queries.push(v1::get_battery(BatterySlot::Single));
                }
                if let Some(kind) = link.noise {
                    queries.push(v1::get_noise(kind));
                }
                if everything {
                    // Asked this early because the connect card picks its 3D
                    // model by it.
                    queries.insert(0, v1::get_model_name());
                    if link.may_have(v1::function::PRESET_EQ)
                        || link.has(v1::function::PRESET_EQ_NONCUSTOMIZABLE)
                    {
                        queries.push(v1::get_eq());
                    }
                    queries.push(v1::get_firmware_version());
                }
                queries.push(v1::get_codec());
            }
            Dialect::V2(link) => {
                queries.extend(link.caps.battery_inquiries.iter().map(|inquiry| v2::get_battery(*inquiry)));
                if let Some(layout) = link.caps.noise {
                    queries.push(v2::get_noise(layout.kind));
                }
                if queries.is_empty() {
                    // Something has to be asked, if only to see that the link is alive.
                    queries.push(v2::get_protocol_info());
                }
                if everything {
                    queries.insert(0, v2::get_model_name());
                }
            }
        }
        queries
    }

    fn refresh(&mut self, conn: &mut Conn) -> LinkResult<()> {
        // The first query doubles as a liveness check, so it must be acknowledged.
        let mut queries = self.state_queries(conn, false).into_iter();
        if let Some(first) = queries.next() {
            self.ask_as(conn, TYPE_DATA_MDR, &first)?;
        }
        for query in queries {
            self.send_optional(conn, &query)?;
        }
        Ok(())
    }

    fn next_command(&mut self) -> Option<Command> {
        loop {
            let command = self.queue.pop_front()?;
            // While a slider is dragged only the newest value matters.
            let superseded = match &command {
                Command::SetNoise(_) => self.queue.iter().any(|c| matches!(c, Command::SetNoise(_))),
                Command::SetVolume(_) => self.queue.iter().any(|c| matches!(c, Command::SetVolume(_))),
                Command::SetEqBands { .. } => self
                    .queue
                    .iter()
                    .any(|c| matches!(c, Command::SetEqBands { .. } | Command::SetEqPreset(_))),
                _ => false,
            };
            if !superseded {
                return Some(command);
            }
        }
    }

    /// Returns `Ok(false)` when the link should be closed afterwards.
    fn execute(&mut self, conn: &mut Conn, command: Command) -> LinkResult<bool> {
        match command {
            Command::SetNoise(state) if self.snap.noise_modes.contains(&state.mode) => {
                let (min, max) = (self.snap.min_ambient_level, self.snap.max_ambient_level);
                let state = NoiseState {
                    ambient_level: if max == 0 { 0 } else { state.ambient_level.clamp(min, max) },
                    focus_on_voice: state.focus_on_voice && self.snap.focus_on_voice_available,
                    ..state
                };
                let payload = match &conn.dialect {
                    Dialect::V1(link) => v1::set_noise(link.noise.unwrap_or_default(), state, link.wind_mode),
                    Dialect::V2(link) => match link.caps.noise {
                        Some(layout) => layout.write(link.noise_raw.as_deref(), state),
                        None => return Ok(true),
                    },
                };
                self.snap.noise = Some(state);
                self.publish();
                self.send(conn, &payload)?;
            }
            Command::SetEqPreset(preset) if self.snap.eq.is_some() => {
                let (set, get) = match &conn.dialect {
                    Dialect::V1(_) => (v1::set_eq_preset(preset), v1::get_eq()),
                    Dialect::V2(_) => (v2::set_eq_preset(preset), v2::get_eq()),
                };
                if let Some(eq) = self.snap.eq.as_mut() {
                    eq.preset = preset;
                }
                self.publish();
                self.send(conn, &set)?;
                self.send_optional(conn, &get)?;
            }
            Command::SetEqBands { clear_bass, bands } => {
                let Some(shown) = self.snap.eq.clone() else { return Ok(true) };
                let preset = if eq_preset_is_editable(shown.preset) { shown.preset } else { EQ_MANUAL };
                let range = shown.range.max(1);
                let clear_bass = clear_bass.map(|bass| bass.clamp(-range, range));
                let bands: Vec<i8> = bands.iter().map(|band| (*band).clamp(-range, range)).collect();
                let payload = match &conn.dialect {
                    Dialect::V1(_) => {
                        let Ok(five) = <[i8; 5]>::try_from(bands.as_slice()) else { return Ok(true) };
                        if preset != shown.preset {
                            // Fixed presets cannot be edited; move to Manual first.
                            self.send(conn, &v1::set_eq_preset(preset))?;
                        }
                        v1::set_eq_bands(clear_bass.unwrap_or(0), &five)
                    }
                    Dialect::V2(link) => match link.eq_layout.encode(preset, clear_bass, &bands) {
                        Some(payload) => payload,
                        None => return Ok(true),
                    },
                };
                self.snap.eq = Some(EqState { preset, clear_bass, bands, range: shown.range });
                self.publish();
                self.send(conn, &payload)?;
            }
            // The settings below exist only where the headset offered them,
            // which is what their presence in the snapshot records.
            Command::SetVolume(volume) if self.snap.playback.is_some() => {
                let volume = volume.min(self.snap.max_volume);
                self.snap.volume = Some(volume);
                self.publish();
                let payload = match &conn.dialect {
                    Dialect::V1(_) => v1::set_volume(volume),
                    Dialect::V2(_) => v2::set_volume(volume),
                };
                self.send(conn, &payload)?;
            }
            Command::Transport(control) if self.snap.playback.is_some() => {
                let payload = match &conn.dialect {
                    Dialect::V1(_) => v1::transport(control),
                    Dialect::V2(_) => v2::transport(control),
                };
                self.send(conn, &payload)?;
                // Shown at once; the headset corrects it if it knows better.
                match control {
                    Transport::Play => self.snap.playback = Some(Playback::Playing),
                    Transport::Pause => self.snap.playback = Some(Playback::Paused),
                    Transport::Next | Transport::Previous => {}
                }
            }
            Command::SetSwitch(id, on) if self.snap.switch(id).is_some() => {
                let Some((kind, payload)) = switch_command(conn, id, on) else { return Ok(true) };
                if id == Switch::Multipoint {
                    // The headset asks before it reconnects for this one, and
                    // the user has just said what they want.
                    self.consent_until = Some(Instant::now() + CONSENT_WINDOW);
                }
                self.snap.set_switch(id, on);
                self.publish();
                self.send_as(conn, kind, &payload)?;
            }
            Command::SetAutoPowerOff(choice) if self.snap.auto_power_off_options.contains(&choice) => {
                let payload = match &conn.dialect {
                    Dialect::V1(_) => v1::set_auto_power_off(choice),
                    Dialect::V2(link) => match link.caps.auto_power_off {
                        Some(kind) => v2::set_auto_power_off(kind, choice),
                        None => return Ok(true),
                    },
                };
                self.snap.auto_power_off = Some(choice);
                self.publish();
                self.send(conn, &payload)?;
            }
            Command::AnswerAlert { message, yes } => {
                let payload = match &conn.dialect {
                    Dialect::V1(_) => v1::answer_alert(message, yes),
                    Dialect::V2(_) => v2::answer_alert(message, yes),
                };
                self.tell_optional(conn, &payload)?;
            }
            Command::PowerOff if self.snap.can_power_off => {
                let payload = match &conn.dialect {
                    Dialect::V1(_) => v1::power_off(),
                    Dialect::V2(_) => v2::power_off(),
                };
                self.send(conn, &payload)?;
                // The headset drops the link itself; stay out of its way.
                self.hold_off = true;
                return Ok(false);
            }
            Command::Disconnect => {
                self.disconnect_audio();
                return Ok(false);
            }
            // Connect means nothing while connected, and the guarded settings
            // above fall through to here when the headset lacks them.
            Command::Connect
            | Command::SetNoise(_)
            | Command::SetEqPreset(_)
            | Command::SetVolume(_)
            | Command::Transport(_)
            | Command::SetSwitch(..)
            | Command::SetAutoPowerOff(_)
            | Command::PowerOff => {}
        }
        Ok(true)
    }

    /// Shows the equalizer as last received, read with the layout known now.
    fn show_eq(&mut self, conn: &Conn) {
        if let Dialect::V2(V2Link { eq_layout, eq_raw: Some((preset, values)), .. }) = &conn.dialect {
            self.snap.eq = Some(eq_layout.decode(*preset, values));
        }
    }

    fn apply(&mut self, conn: &mut Conn, event: Event) {
        match event {
            Event::ProtocolInfo { table2 } => {
                conn.got_protocol_info = true;
                conn.has_table2 = table2;
            }
            Event::SupportFunctions(functions) => conn.support_functions = Some(functions),
            Event::SupportFunctions2(functions) => conn.support_functions2 = Some(functions),
            Event::ModelName(name) => self.snap.model = Some(name),
            Event::ModelColour(colour) => self.snap.colour = Some(colour),
            Event::Battery(slot, battery) => {
                let b = &mut self.snap.battery;
                match slot {
                    BatterySlot::Single => b.single = battery,
                    BatterySlot::Left => b.left = battery,
                    BatterySlot::Right => b.right = battery,
                    BatterySlot::Case => b.case = battery,
                }
            }
            Event::NoiseCapability(capability) => {
                self.snap.noise_modes = capability.modes;
                self.snap.min_ambient_level = capability.max_ambient_level.min(v1::MIN_AMBIENT_LEVEL);
                self.snap.max_ambient_level = capability.max_ambient_level;
                self.snap.focus_on_voice_available = capability.focus_on_voice;
                if let Dialect::V1(link) = &mut conn.dialect {
                    link.wind_mode = capability.wind_mode;
                }
            }
            Event::Noise(mut state) => {
                let (min, max) = (self.snap.min_ambient_level, self.snap.max_ambient_level);
                if max == 0 {
                    // This headset's ambient sound has no level.
                    state.ambient_level = 0;
                } else {
                    // Outside ambient mode the headset may report level 0,
                    // which is not a level. Keep the one the slider was last
                    // on, so that switching back to ambient restores it.
                    if state.ambient_level < min {
                        state.ambient_level = self
                            .snap
                            .noise
                            .map(|n| n.ambient_level)
                            .filter(|level| *level >= min)
                            .unwrap_or(max.div_ceil(2).max(min));
                    }
                    state.ambient_level = state.ambient_level.min(max);
                }
                self.snap.noise = Some(state);
            }
            Event::NoiseRaw(message) => {
                if let Dialect::V2(link) = &mut conn.dialect {
                    link.noise_raw = Some(message);
                }
            }
            Event::Eq(mut eq) => {
                if eq.bands.is_empty() {
                    if let Some(previous) = &self.snap.eq {
                        eq.bands = previous.bands.clone();
                        eq.clear_bass = previous.clear_bass;
                    }
                }
                self.snap.eq = Some(eq);
            }
            Event::EqRaw { preset, values } => {
                if let Dialect::V2(link) = &mut conn.dialect {
                    link.eq_raw = Some((preset, values));
                }
                self.show_eq(conn);
            }
            Event::EqLayout { values, steps, presets } => {
                if let Dialect::V2(link) = &mut conn.dialect {
                    link.eq_layout.values = values;
                    link.eq_layout.steps = steps;
                }
                self.snap.eq_presets = presets;
                self.show_eq(conn);
            }
            Event::EqFrequencies(frequencies) => {
                if let Dialect::V2(link) = &mut conn.dialect {
                    link.eq_layout.frequencies = frequencies.len().min(u8::MAX as usize) as u8;
                }
                self.snap.eq_frequencies = frequencies;
                self.show_eq(conn);
            }
            Event::Firmware(version) => self.snap.firmware = Some(version),
            Event::Codec(codec) => self.snap.codec = Some(codec.into()),
            Event::Volume(volume) => self.snap.volume = Some(volume),
            Event::Playback(playback) => self.snap.playback = Some(playback),
            Event::Switch(id, on) => self.snap.set_switch(id, on),
            Event::AutoPowerOff(choice) => {
                self.snap.auto_power_off = Some(choice);
                if !self.snap.auto_power_off_options.contains(&choice) {
                    // A choice the list missed is still a choice the headset has.
                    self.snap.auto_power_off_options.push(choice);
                    self.snap.auto_power_off_options.sort_unstable();
                }
            }
            Event::AutoPowerOffOptions(options) => {
                let current = self.snap.auto_power_off;
                self.snap.auto_power_off_options = options;
                if let Some(choice) = current.filter(|c| !self.snap.auto_power_off_options.contains(c)) {
                    self.snap.auto_power_off_options.push(choice);
                    self.snap.auto_power_off_options.sort_unstable();
                }
            }
            Event::GeneralSettingInfo { slot, name, boolean } => {
                conn.described.push(slot);
                match Switch::from_general_setting(&name) {
                    Some(switch) if boolean => conn.general.push((slot, switch)),
                    _ => crate::log!("General setting {slot:#04x} is {name}, which the app has no control for"),
                }
            }
            Event::GeneralSetting { slot, on } => {
                if let Some((_, switch)) = conn.general.iter().find(|(known, _)| *known == slot) {
                    self.snap.set_switch(*switch, on);
                }
            }
            Event::VoiceGuidanceLanguage(language) => {
                if let Dialect::V2(link) = &mut conn.dialect {
                    link.voice_language = Some(language);
                }
            }
            Event::Alert { message, wants_choice } => {
                // The headset waits for an answer. A plain notice is
                // acknowledged. A yes/no question gets a yes only when it
                // follows a switch the user has just flipped knowing it brings
                // a reconnect. Anything else is declined, after which the XM3
                // (its questions come with the touch panel and the equalizer)
                // applies the change anyway without dropping the connection.
                let agreed = self.consent_until.take().is_some_and(|until| Instant::now() < until);
                let yes = !wants_choice || agreed;
                crate::log!(
                    "The headset raised alert {message:#04x} (asks for a choice: {wants_choice}); answering {}",
                    if yes { "yes" } else { "no" }
                );
                self.queue.push_back(Command::AnswerAlert { message, yes });
            }
            Event::Ignored => {}
        }
    }

    fn on_frame(&mut self, conn: &mut Conn, frame: Frame) -> LinkResult<()> {
        if frame.is_ack() {
            // The acknowledgement carries the sequence number to use next.
            conn.seq = frame.seq;
            conn.acked = true;
            return Ok(());
        }
        if !frame.is_data() {
            return Ok(());
        }
        conn.link.send(&pack(TYPE_ACK, 1u8.wrapping_sub(frame.seq), &[])).map_err(LinkError::Closed)?;
        if tracing() {
            crate::log!("< table {} {:02x?}", if frame.kind == TYPE_DATA_MDR_NO2 { 2 } else { 1 }, frame.payload);
        }
        if conn.awaiting.is_some_and(|(kind, command)| frame.kind == kind && frame.payload.first() == Some(&command)) {
            conn.answered = true;
        }
        let events = match &conn.dialect {
            Dialect::V1(_) => v1::parse(frame.kind, &frame.payload),
            Dialect::V2(_) => v2::parse(frame.kind, &frame.payload),
        };
        if events.is_empty() {
            crate::log!("Unhandled message {:02x?}", frame.payload);
        }
        for event in events {
            self.apply(conn, event);
        }
        if self.snap.phase == Phase::Connected {
            self.publish();
        }
        Ok(())
    }

    /// Processes incoming messages until `done` holds or `timeout` passes.
    /// Returns whether `done` was reached.
    fn pump(
        &mut self,
        conn: &mut Conn,
        timeout: Duration,
        yield_on_command: bool,
        done: impl Fn(&Conn, &Snapshot) -> bool,
    ) -> LinkResult<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if done(conn, &self.snap) {
                return Ok(true);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            match self.rx.recv_timeout(left) {
                Ok(Msg::Frame(id, frame)) if id == conn.id => self.on_frame(conn, frame)?,
                Ok(Msg::Closed(id, why)) if id == conn.id => {
                    return Err(LinkError::Closed(why.unwrap_or_else(|| "the headset closed the link".into())));
                }
                Ok(Msg::Command(command)) => {
                    self.queue.push_back(command);
                    if yield_on_command {
                        return Ok(false);
                    }
                }
                Ok(_) => {} // left over from an earlier link
                Err(RecvTimeoutError::Timeout) => return Ok(false),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(LinkError::Closed("the app is shutting down".into()));
                }
            }
        }
    }

    fn send(&mut self, conn: &mut Conn, payload: &[u8]) -> LinkResult<()> {
        self.send_as(conn, TYPE_DATA_MDR, payload)
    }

    /// `kind` is the frame type, which selects the command table.
    fn send_as(&mut self, conn: &mut Conn, kind: u8, payload: &[u8]) -> LinkResult<()> {
        if tracing() {
            crate::log!("> table {} {payload:02x?}", if kind == TYPE_DATA_MDR_NO2 { 2 } else { 1 });
        }
        for _ in 0..SEND_ATTEMPTS {
            conn.acked = false;
            conn.link.send(&pack(kind, conn.seq, payload)).map_err(LinkError::Closed)?;
            // The XM3 often answers a request before acknowledging it; the
            // pump takes replies and acknowledgements in whichever order.
            if self.pump(conn, ACK_TIMEOUT, false, |c, _| c.acked)? {
                return Ok(());
            }
        }
        Err(LinkError::NoAck)
    }

    /// Sends a question and waits for its answer, which carries the command
    /// number one above the question's.
    ///
    /// The waiting matters: a WH-1000XM3 that is asked the next question
    /// while still preparing an answer drops that answer without a word. An
    /// answer that never comes costs [`REPLY_TIMEOUT`] and is not an error.
    fn ask_as(&mut self, conn: &mut Conn, kind: u8, payload: &[u8]) -> LinkResult<()> {
        conn.awaiting = Some((kind, payload[0].wrapping_add(1)));
        conn.answered = false;
        let outcome = match self.send_as(conn, kind, payload) {
            Ok(()) => self.pump(conn, REPLY_TIMEOUT, false, |c, _| c.answered).map(|answered| {
                if !answered {
                    crate::log!("No reply to query {payload:02x?}");
                }
            }),
            Err(e) => Err(e),
        };
        conn.awaiting = None;
        outcome
    }

    /// For questions a headset may not support: an unacknowledged one is
    /// logged and skipped instead of ending the link.
    fn send_optional(&mut self, conn: &mut Conn, payload: &[u8]) -> LinkResult<()> {
        self.send_optional_as(conn, TYPE_DATA_MDR, payload)
    }

    fn send_optional_as(&mut self, conn: &mut Conn, kind: u8, payload: &[u8]) -> LinkResult<()> {
        match self.ask_as(conn, kind, payload) {
            Err(LinkError::NoAck) => {
                crate::log!("No acknowledgement for query {payload:02x?}");
                Ok(())
            }
            other => other,
        }
    }

    /// For instructions that bring no answer and that a headset may ignore.
    fn tell_optional(&mut self, conn: &mut Conn, payload: &[u8]) -> LinkResult<()> {
        match self.send(conn, payload) {
            Err(LinkError::NoAck) => {
                crate::log!("No acknowledgement for {payload:02x?}");
                Ok(())
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headline_battery_prefers_the_headset_then_the_weaker_bud() {
        let b = |level| Some(Battery { level, charging: false });
        assert_eq!(Batteries { single: b(80), ..Default::default() }.headline(), b(80));
        assert_eq!(Batteries { left: b(60), right: b(40), ..Default::default() }.headline(), b(40));
        assert_eq!(Batteries { right: b(40), ..Default::default() }.headline(), b(40));
        assert_eq!(Batteries { case: b(90), ..Default::default() }.headline(), None);
    }

    #[test]
    fn snapshot_serializes_with_the_names_the_ui_expects() {
        let snap = Snapshot {
            phase: Phase::Connected,
            device_name: Some("WH-1000XM3".into()),
            noise: Some(NoiseState { mode: NoiseMode::NoiseCancelling, ambient_level: 9, focus_on_voice: false }),
            ..Snapshot::default()
        };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["phase"], "connected");
        assert_eq!(json["deviceName"], "WH-1000XM3");
        assert_eq!(json["noise"]["mode"], "noise_cancelling");
        assert_eq!(json["noise"]["ambientLevel"], 9);
        assert_eq!(json["maxAmbientLevel"], 0);
        assert_eq!(json["connections"], 0);
        assert_eq!(json["eqPresets"], serde_json::json!([]));
        assert_eq!(json["model"], serde_json::Value::Null);
    }

    #[test]
    fn switches_keep_a_fixed_order_and_serialize_by_name() {
        let mut snap = Snapshot::default();
        assert_eq!(snap.switch(Switch::Upscaling), None);
        snap.set_switch(Switch::VoiceGuidance, true);
        snap.set_switch(Switch::Multipoint, false);
        snap.set_switch(Switch::Upscaling, true);
        snap.set_switch(Switch::Multipoint, true);
        let order: Vec<Switch> = snap.switches.iter().map(|s| s.id).collect();
        assert_eq!(order, [Switch::Upscaling, Switch::Multipoint, Switch::VoiceGuidance]);
        assert_eq!(snap.switch(Switch::Multipoint), Some(true));
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["switches"][1], serde_json::json!({ "id": "multipoint", "on": true }));
        assert_eq!(json["switches"][0]["id"], "upscaling");
    }

    #[test]
    fn the_demo_headset_obeys_the_controls_and_reconnects() {
        let mut snap = demo_snapshot("WH-1000XM5", 1);
        assert_eq!((snap.phase, snap.model.as_deref(), snap.connections), (Phase::Connected, Some("WH-1000XM5"), 1));
        assert!(snap.battery.single.is_some() && snap.battery.left.is_none());
        apply_demo(&mut snap, "WH-1000XM5", Command::SetVolume(99));
        apply_demo(&mut snap, "WH-1000XM5", Command::SetSwitch(Switch::SpeakToChat, true));
        assert_eq!(snap.volume, Some(30));
        assert_eq!(snap.switch(Switch::SpeakToChat), Some(true));

        apply_demo(&mut snap, "WH-1000XM5", Command::Disconnect);
        assert_eq!((snap.phase, snap.noise, snap.connections), (Phase::Disconnected, None, 1));
        // Nothing but Connect means anything while disconnected.
        apply_demo(&mut snap, "WH-1000XM5", Command::SetVolume(5));
        assert_eq!(snap.volume, None);
        // Connecting again counts as a fresh connection, so the card shows again.
        apply_demo(&mut snap, "WH-1000XM5", Command::Connect);
        assert_eq!((snap.phase, snap.connections), (Phase::Connected, 2));

        // Earbuds get a battery each and a case.
        let buds = demo_snapshot("WF-1000XM5", 1);
        assert!(buds.battery.single.is_none() && buds.battery.left.is_some() && buds.battery.case.is_some());
    }

    #[test]
    fn the_first_connection_is_fresh() {
        let mut arrival = Arrival::new();
        assert!(arrival.back(Instant::now()));
    }

    #[test]
    fn a_link_that_recovers_quickly_is_not_a_new_connection() {
        let start = Instant::now();
        let mut arrival = Arrival::new();
        assert!(arrival.back(start));
        arrival.lost(start + Duration::from_secs(60));
        assert!(!arrival.back(start + Duration::from_millis(61_600)));
    }

    #[test]
    fn switching_off_and_on_is_a_new_connection_even_if_windows_never_said_disconnected() {
        // What the log showed on a real WH-1000XM3: link lost, two failed
        // attempts, working again 18 seconds later.
        let start = Instant::now();
        let mut arrival = Arrival::new();
        assert!(arrival.back(start));
        arrival.lost(start + Duration::from_secs(100));
        assert!(arrival.back(start + Duration::from_secs(118)));
        // and the same again the next time
        arrival.lost(start + Duration::from_secs(300));
        assert!(arrival.back(start + Duration::from_secs(310)));
    }

    #[test]
    fn seeing_the_headset_disconnected_makes_the_next_link_fresh_however_soon() {
        let start = Instant::now();
        let mut arrival = Arrival::new();
        assert!(arrival.back(start));
        arrival.lost(start + Duration::from_secs(10));
        arrival.gone();
        assert!(arrival.back(start + Duration::from_secs(11)));
    }

    #[test]
    fn failed_attempts_while_down_do_not_restart_the_clock() {
        let start = Instant::now();
        let mut arrival = Arrival::new();
        assert!(arrival.back(start));
        arrival.lost(start + Duration::from_secs(10));
        arrival.lost(start + Duration::from_secs(14)); // a later report of the same outage
        assert!(arrival.back(start + Duration::from_secs(15)));
    }
}
