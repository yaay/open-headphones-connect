//! A live link to Nothing or CMF earbuds or headphones: what `serve` is for
//! Sony's. The rest of the session (finding the device, telling a fresh
//! connection from a hiccup, the snapshot, the commands) is shared.

use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::{Duration, Instant};

use super::{tracing, Batteries, Command, LinkError, LinkResult, Msg, Phase, Session, Snapshot, REFRESH_EVERY};
use crate::platform::{Incoming, Link};
use crate::protocol::nothing::{self, answer_to, command, EqFamily, Frame, Model, NcLevel};
use crate::protocol::{Battery, BatterySlot, EqState, NoiseMode, NoiseState, Switch};

/// Some earbuds drop a request that arrives right behind another.
const SPACING: Duration = Duration::from_millis(150);
/// How long a question waits. One that is never answered is about something
/// the device does not have.
const ANSWER_TIMEOUT: Duration = Duration::from_millis(1200);
/// An acknowledgement carries no state, so a setting is read back after it.
const ACK_TIMEOUT: Duration = Duration::from_millis(400);

struct NothingConn {
    link: Box<dyn Link>,
    id: u64,
    fsn: u8,
    last_sent: Option<Instant>,
    model: Option<Model>,
    eq: Option<EqFamily>,
    /// The noise-cancelling level to go back to when noise cancelling is
    /// turned on again.
    level: NcLevel,
}

impl Session {
    pub(super) fn serve_nothing(&mut self, link: Box<dyn Link>) -> String {
        self.next_link_id += 1;
        let id = self.next_link_id;
        let tx = self.tx.clone();
        let mut reassembler = nothing::Reassembler::default();
        link.listen(Box::new(move |incoming| match incoming {
            Incoming::Bytes(bytes) => {
                for frame in reassembler.feed(&bytes) {
                    let _ = tx.send(Msg::Nothing(id, frame));
                }
            }
            // Only Sony frames come whole.
            Incoming::Frame(_) => {}
            Incoming::Closed(why) => {
                let _ = tx.send(Msg::Closed(id, why));
            }
        }));

        let model = Model::from_name(self.snap.device_name.as_deref().unwrap_or_default());
        let mut conn = NothingConn {
            link,
            id,
            fsn: 0,
            last_sent: None,
            eq: model.as_ref().and_then(|model| model.eq),
            model,
            level: NcLevel::High,
        };
        self.queue.clear();

        if let Err(e) = self.nothing_handshake(&mut conn) {
            return e.describe();
        }
        self.snap.model = conn.model.as_ref().map(|model| model.name.to_string());
        self.snap.notice = None;
        if self.arrival.back(Instant::now()) {
            self.connections += 1;
            crate::log!("Headphones connected (connection {} since the app started)", self.connections);
        }
        self.set_phase(Phase::Connected);

        // The card can appear now; the settings fill in behind it.
        if let Err(e) = self.nothing_extras(&mut conn) {
            return e.describe();
        }
        self.publish();

        let mut last_refresh = Instant::now();
        loop {
            while let Some(command) = self.next_command() {
                match self.nothing_execute(&mut conn, command) {
                    Ok(true) => {}
                    Ok(false) => return "ended on request".into(),
                    Err(e) => return e.describe(),
                }
                self.publish();
            }
            // The device announces changes itself; this is a backstop.
            if last_refresh.elapsed() > REFRESH_EVERY {
                last_refresh = Instant::now();
                for (request, payload) in [(command::GET_BATTERY, vec![]), (command::GET_NOISE, vec![3])] {
                    if let Err(e) = self.nothing_send(&mut conn, request, &payload) {
                        return e.describe();
                    }
                }
            }
            if let Err(e) = self.nothing_pump(&mut conn, Duration::from_secs(1), true, None) {
                return e.describe();
            }
            if !self.device.as_ref().is_some_and(|d| d.is_connected()) {
                return "the headset disconnected".into();
            }
        }
    }

    /// Enough to show the battery and the noise mode.
    fn nothing_handshake(&mut self, conn: &mut NothingConn) -> LinkResult<()> {
        // An Ear (2) wants to be asked its protocol version and then told to
        // start; the others do not mind being asked.
        let version = self.nothing_ask(conn, command::GET_PROTOCOL_VERSION, &[])?;
        if version.is_some() {
            self.nothing_ask(conn, command::ACTIVATE, &[])?;
        }
        // Some devices answer nothing until asked for their details.
        self.nothing_ask(conn, command::GET_DEVICE_INFO, &[])?;
        let battery = self.nothing_ask(conn, command::GET_BATTERY, &[])?;
        let noise = self.nothing_ask(conn, command::GET_NOISE, &[3])?;
        // Before the card appears, which shows the headphones in their colour.
        self.nothing_ask(conn, command::GET_COLOUR, &[])?;
        if version.is_none() && battery.is_none() && noise.is_none() {
            return Err(LinkError::Closed("no answer from the headset".into()));
        }
        Ok(())
    }

    fn nothing_extras(&mut self, conn: &mut NothingConn) -> LinkResult<()> {
        self.nothing_ask(conn, command::GET_FIRMWARE, &[])?;
        self.nothing_ask(conn, command::GET_EXTRA_FEATURES, &[])?;
        self.nothing_ask(conn, command::GET_LOW_LATENCY, &[])?;
        match conn.eq {
            Some(family) => {
                self.nothing_ask(conn, family.get(), &[])?;
            }
            // Where no source says which way the model chooses presets, the
            // one it answers is the one it has.
            None => {
                for family in [EqFamily::Presets, EqFamily::ListeningModes] {
                    if self.nothing_ask(conn, family.get(), &[])?.is_some() {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns `Ok(false)` when the link should be closed afterwards.
    fn nothing_execute(&mut self, conn: &mut NothingConn, command: Command) -> LinkResult<bool> {
        let (request, payload, read_back) = match command {
            Command::SetNoise(state) if self.snap.noise_modes.contains(&state.mode) => {
                self.snap.noise = Some(NoiseState { mode: state.mode, ambient_level: 0, focus_on_voice: false });
                (command::SET_NOISE, nothing::set_noise(state.mode, conn.level), (command::GET_NOISE, vec![3]))
            }
            Command::SetNcLevel(level) if self.snap.nc_levels.contains(&level) => {
                conn.level = level;
                self.snap.nc_level = Some(level);
                self.snap.noise =
                    Some(NoiseState { mode: NoiseMode::NoiseCancelling, ambient_level: 0, focus_on_voice: false });
                (command::SET_NOISE, nothing::set_noise(NoiseMode::NoiseCancelling, level), (command::GET_NOISE, vec![3]))
            }
            Command::SetEqPreset(preset) if self.snap.eq_presets.contains(&preset) => {
                let Some(family) = conn.eq else { return Ok(true) };
                if let Some(eq) = self.snap.eq.as_mut() {
                    eq.preset = preset;
                }
                let (request, payload) = family.set(preset);
                (request, payload, (family.get(), vec![]))
            }
            Command::SetSwitch(Switch::PauseWhenRemoved, on) if self.snap.switch(Switch::PauseWhenRemoved).is_some() => {
                self.snap.set_switch(Switch::PauseWhenRemoved, on);
                (
                    command::SET_EXTRA_FEATURE,
                    nothing::set_extra_feature(nothing::IN_EAR_DETECTION, on),
                    (command::GET_EXTRA_FEATURES, vec![]),
                )
            }
            Command::SetSwitch(Switch::LowLatency, on) if self.snap.switch(Switch::LowLatency).is_some() => {
                self.snap.set_switch(Switch::LowLatency, on);
                (command::SET_LOW_LATENCY, nothing::set_low_latency(on), (command::GET_LOW_LATENCY, vec![]))
            }
            Command::Disconnect => {
                self.disconnect_audio();
                return Ok(false);
            }
            // Not on these devices, or not done by this app yet.
            _ => return Ok(true),
        };
        self.publish();
        self.nothing_send(conn, request, &payload)?;
        self.nothing_pump(conn, ACK_TIMEOUT, false, Some(request))?;
        self.nothing_ask(conn, read_back.0, &read_back.1)?;
        Ok(true)
    }

    /// Takes in one frame from the device: an answer, an acknowledgement or a
    /// change it announces.
    fn on_nothing(&mut self, conn: &mut NothingConn, frame: Frame) {
        if tracing() {
            crate::log!("< {:04X} {:02x?}", frame.command, frame.payload);
        }
        let answer = |request| frame.command == answer_to(request);
        if answer(command::GET_BATTERY) || frame.command == command::BATTERY_CHANGED {
            // A part that does not report keeps its last level: the case, for
            // one, only reports while it is open.
            for (slot, battery) in nothing::parse_battery(&frame.payload) {
                let held = &mut self.snap.battery;
                *match slot {
                    BatterySlot::Single => &mut held.single,
                    BatterySlot::Left => &mut held.left,
                    BatterySlot::Right => &mut held.right,
                    BatterySlot::Case => &mut held.case,
                } = Some(battery);
            }
        } else if answer(command::GET_NOISE) || frame.command == command::NOISE_CHANGED {
            let Some(noise) = nothing::parse_noise(&frame.payload) else { return };
            if self.snap.noise_modes.is_empty() {
                self.snap.nc_levels = match &conn.model {
                    Some(model) => model.levels.to_vec(),
                    None if noise.has_levels => NcLevel::ALL.to_vec(),
                    None => Vec::new(),
                };
                self.snap.noise_modes = if self.snap.nc_levels.is_empty() && noise.mode != NoiseMode::NoiseCancelling {
                    vec![NoiseMode::Ambient, NoiseMode::Off]
                } else {
                    vec![NoiseMode::NoiseCancelling, NoiseMode::Ambient, NoiseMode::Off]
                };
            }
            self.snap.noise = Some(NoiseState { mode: noise.mode, ambient_level: 0, focus_on_voice: false });
            if let Some(level) = noise.level {
                conn.level = level;
                if self.snap.nc_levels.contains(&level) {
                    self.snap.nc_level = Some(level);
                }
            }
        } else if answer(command::GET_EQ_PRESET) || answer(command::GET_LISTENING_MODE) {
            let family =
                if answer(command::GET_EQ_PRESET) { EqFamily::Presets } else { EqFamily::ListeningModes };
            let Some(&preset) = frame.payload.first() else { return };
            conn.eq = Some(family);
            let mut presets: Vec<(u8, String)> =
                family.presets().iter().map(|(value, name)| (*value, (*name).to_string())).collect();
            if !presets.iter().any(|(value, _)| *value == preset) {
                presets.push((preset, "Custom".into()));
            }
            self.snap.eq_presets = presets.iter().map(|(value, _)| *value).collect();
            self.snap.eq_preset_names = presets.into_iter().map(|(_, name)| name).collect();
            self.snap.eq = Some(EqState { preset, clear_bass: None, bands: Vec::new(), range: 0 });
        } else if answer(command::GET_EXTRA_FEATURES) {
            for (id, on) in nothing::parse_extra_features(&frame.payload) {
                if id == nothing::IN_EAR_DETECTION {
                    self.snap.set_switch(Switch::PauseWhenRemoved, on);
                }
            }
        } else if answer(command::GET_LOW_LATENCY) {
            if let Some(on) = nothing::parse_low_latency(&frame.payload) {
                self.snap.set_switch(Switch::LowLatency, on);
            }
        } else if answer(command::GET_COLOUR) {
            self.snap.colour = frame.payload.first().copied();
        } else if answer(command::GET_FIRMWARE) {
            self.snap.firmware = Some(nothing::parse_text(&frame.payload)).filter(|text| !text.is_empty());
        }
    }

    fn nothing_send(&mut self, conn: &mut NothingConn, request: u16, payload: &[u8]) -> LinkResult<()> {
        if let Some(last) = conn.last_sent {
            thread::sleep(SPACING.saturating_sub(last.elapsed()));
        }
        conn.fsn = if conn.fsn == u8::MAX { 1 } else { conn.fsn + 1 };
        if tracing() {
            crate::log!("> {request:04X} {payload:02x?}");
        }
        conn.link.send(&nothing::pack(request, conn.fsn, payload)).map_err(LinkError::Closed)?;
        conn.last_sent = Some(Instant::now());
        Ok(())
    }

    /// Sends a request and waits for its answer, which it returns. `None`
    /// when none came.
    fn nothing_ask(&mut self, conn: &mut NothingConn, request: u16, payload: &[u8]) -> LinkResult<Option<Vec<u8>>> {
        self.nothing_send(conn, request, payload)?;
        self.nothing_pump(conn, ANSWER_TIMEOUT, false, Some(request))
    }

    /// Takes in what arrives until the answer to `request` does (which it
    /// returns), or `timeout` passes, or, when `yield_on_command`, a command
    /// comes in.
    fn nothing_pump(
        &mut self,
        conn: &mut NothingConn,
        timeout: Duration,
        yield_on_command: bool,
        request: Option<u16>,
    ) -> LinkResult<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            match self.rx.recv_timeout(left) {
                Ok(Msg::Nothing(id, frame)) if id == conn.id => {
                    let answer = request.filter(|&r| frame.command == answer_to(r)).map(|_| frame.payload.clone());
                    self.on_nothing(conn, frame);
                    if self.snap.phase == Phase::Connected {
                        self.publish();
                    }
                    if answer.is_some() {
                        return Ok(answer);
                    }
                }
                Ok(Msg::Closed(id, why)) if id == conn.id => {
                    return Err(LinkError::Closed(why.unwrap_or_else(|| "the headset closed the link".into())));
                }
                Ok(Msg::Command(command)) => {
                    self.queue.push_back(command);
                    if yield_on_command {
                        return Ok(None);
                    }
                }
                Ok(_) => {} // left over from an earlier link
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(LinkError::Closed("the app is shutting down".into()));
                }
            }
        }
    }
}

/// A made-up Nothing or CMF device for `--demo "CMF Buds Pro 2"` and the like.
pub(super) fn demo_snapshot(name: &str, model: &Model, connections: u64) -> Snapshot {
    let battery = |level| Some(Battery { level, charging: false });
    let family = model.eq.unwrap_or(EqFamily::Presets);
    let mut snap = Snapshot {
        phase: Phase::Connected,
        device_name: Some(name.into()),
        model: Some(model.name.into()),
        battery: if model.earbuds {
            Batteries { left: battery(90), right: battery(100), case: battery(60), ..Batteries::default() }
        } else {
            Batteries { single: battery(80), ..Batteries::default() }
        },
        noise: Some(NoiseState {
            mode: if model.levels.is_empty() { NoiseMode::Ambient } else { NoiseMode::NoiseCancelling },
            ambient_level: 0,
            focus_on_voice: false,
        }),
        noise_modes: if model.levels.is_empty() {
            vec![NoiseMode::Ambient, NoiseMode::Off]
        } else {
            vec![NoiseMode::NoiseCancelling, NoiseMode::Ambient, NoiseMode::Off]
        },
        nc_level: model.levels.first().copied(),
        nc_levels: model.levels.to_vec(),
        eq: Some(EqState { preset: 0, clear_bass: None, bands: Vec::new(), range: 0 }),
        eq_presets: family.presets().iter().map(|(value, _)| *value).collect(),
        eq_preset_names: family.presets().iter().map(|(_, name)| (*name).to_string()).collect(),
        firmware: Some("demo".into()),
        connections,
        ..Snapshot::default()
    };
    snap.set_switch(Switch::PauseWhenRemoved, true);
    snap.set_switch(Switch::LowLatency, false);
    snap
}
