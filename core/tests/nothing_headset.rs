//! Runs the whole session against a stand-in CMF Buds Pro 2 that answers the
//! way the captured devices in protocol/nothing.rs's sources did, then changes
//! its settings and checks what reached the earbuds. No Bluetooth involved.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ohc_core::platform::{Bluetooth, Headset, Incoming, Link};
use ohc_core::protocol::nothing::{answer_to, command, pack, NcLevel, Reassembler};
use ohc_core::protocol::{Battery, Generation, NoiseMode, NoiseState, Switch};
use ohc_core::session::{self, Command, Phase, Snapshot};

/// What the stand-in earbuds hold.
struct Buds {
    noise: u8,
    listening_mode: u8,
    low_latency: bool,
    in_ear: bool,
    /// Every request that arrived, by command.
    heard: Vec<u16>,
}

type Listener = Box<dyn FnMut(Incoming) + Send>;

#[derive(Clone)]
struct Channel {
    buds: Arc<Mutex<Buds>>,
    reassembler: Arc<Mutex<Reassembler>>,
    listener: Arc<Mutex<Option<Listener>>>,
}

impl Channel {
    fn say(&self, command: u16, fsn: u8, payload: &[u8]) {
        if let Some(listener) = self.listener.lock().unwrap().as_mut() {
            // In two pieces, as a serial channel may well deliver it.
            let bytes = pack(command, fsn, payload);
            let (first, second) = bytes.split_at(bytes.len() / 2);
            listener(Incoming::Bytes(first.to_vec()));
            listener(Incoming::Bytes(second.to_vec()));
        }
    }
}

impl Link for Channel {
    fn listen(&self, on_incoming: Listener) {
        *self.listener.lock().unwrap() = Some(on_incoming);
    }

    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        for frame in self.reassembler.lock().unwrap().feed(bytes) {
            let mut buds = self.buds.lock().unwrap();
            buds.heard.push(frame.command);
            let p = &frame.payload;
            let answer: Option<Vec<u8>> = match frame.command {
                command::GET_PROTOCOL_VERSION => Some(b"1.0.0\n".to_vec()),
                command::GET_DEVICE_INFO => Some(b"\x022,2,1.0.1.74\n3,2,1.0.1.74\n".to_vec()),
                // Left 85 %, right 15 % and charging, case 85 %.
                command::GET_BATTERY => Some(vec![3, 2, 0x55, 3, 0x8F, 4, 0x55]),
                // Level first, then the mode, as this model sends it.
                command::GET_NOISE => Some(vec![2, buds.noise.min(4).max(1), 0, 1, buds.noise, 0]),
                command::GET_FIRMWARE => Some(b"1.0.1.74\0".to_vec()),
                command::GET_EXTRA_FEATURES => Some(vec![2, 1, u8::from(buds.in_ear), 2, 0]),
                command::GET_LOW_LATENCY => Some(vec![if buds.low_latency { 1 } else { 2 }]),
                command::GET_LISTENING_MODE => Some(vec![buds.listening_mode]),
                command::SET_NOISE => {
                    buds.noise = p[1];
                    Some(Vec::new())
                }
                command::SET_LISTENING_MODE => {
                    buds.listening_mode = p[0];
                    Some(Vec::new())
                }
                command::SET_LOW_LATENCY => {
                    buds.low_latency = p[0] == 1;
                    Some(vec![p[0]])
                }
                command::SET_EXTRA_FEATURE => {
                    buds.in_ear = p[2] == 1;
                    Some(vec![0])
                }
                // Activation and the rest go unanswered, as on this model.
                _ => None,
            };
            let noise = buds.noise;
            drop(buds);
            if let Some(answer) = answer {
                self.say(answer_to(frame.command), frame.fsn, &answer);
            }
            // A change of noise mode is announced too, unasked.
            if frame.command == command::SET_NOISE {
                self.say(command::NOISE_CHANGED, 0, &[1, noise, 0]);
            }
        }
        Ok(())
    }
}

struct Computer(Channel);

impl Bluetooth for Computer {
    fn radio_is_on(&self) -> bool {
        true
    }

    fn find_headset(&self) -> Option<Box<dyn Headset>> {
        Some(Box::new(Earbuds(self.0.clone())))
    }
}

struct Earbuds(Channel);

impl Headset for Earbuds {
    fn name(&self) -> &str {
        "CMF Buds Pro 2"
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn open(&self) -> Result<(Box<dyn Link>, Generation), String> {
        Ok((Box::new(self.0.clone()), Generation::Nothing))
    }

    fn connect_audio(&self) -> Result<(), String> {
        Ok(())
    }

    fn disconnect_audio(&self) -> Result<(), String> {
        Ok(())
    }
}

fn wait_for(snapshots: &mpsc::Receiver<Snapshot>, last: &mut Snapshot, done: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(last) {
        let left = deadline.saturating_duration_since(Instant::now());
        *last = snapshots.recv_timeout(left).unwrap_or_else(|_| panic!("the session got no further than {last:?}"));
    }
}

#[test]
fn stand_in_cmf_buds_connect_report_and_take_settings() {
    let buds = Arc::new(Mutex::new(Buds {
        noise: 1,
        listening_mode: 0,
        low_latency: false,
        in_ear: true,
        heard: Vec::new(),
    }));
    let channel = Channel { buds: buds.clone(), reassembler: Arc::default(), listener: Arc::default() };
    let (seen, snapshots) = mpsc::channel::<Snapshot>();
    let session = session::spawn(Computer(channel), move |snapshot| {
        let _ = seen.send(snapshot.clone());
    });

    let mut last = Snapshot::default();
    wait_for(&snapshots, &mut last, |s| {
        s.phase == Phase::Connected && s.firmware.is_some() && s.eq.is_some() && s.switches.len() == 2
    });
    assert_eq!(last.model.as_deref(), Some("CMF Buds Pro 2"));
    assert_eq!(last.firmware.as_deref(), Some("1.0.1.74"));
    assert_eq!(last.connections, 1);
    assert_eq!(last.battery.left, Some(Battery { level: 85, charging: false }));
    assert_eq!(last.battery.right, Some(Battery { level: 15, charging: true }));
    assert_eq!(last.battery.case, Some(Battery { level: 85, charging: false }));
    assert_eq!(last.noise.map(|n| n.mode), Some(NoiseMode::NoiseCancelling));
    assert_eq!(last.noise_modes, [NoiseMode::NoiseCancelling, NoiseMode::Ambient, NoiseMode::Off]);
    assert_eq!((last.nc_level, last.nc_levels.as_slice()), (Some(NcLevel::High), &NcLevel::ALL[..]));
    assert_eq!(last.eq.as_ref().map(|eq| eq.preset), Some(0));
    assert_eq!(last.eq_preset_names[..3], ["Balanced", "Rock", "Electronic"]);
    assert_eq!(last.switch(Switch::PauseWhenRemoved), Some(true));
    assert_eq!(last.switch(Switch::LowLatency), Some(false));
    // It asked for the protocol version first, and started the earbuds.
    assert_eq!(buds.lock().unwrap().heard[..2], [command::GET_PROTOCOL_VERSION, command::ACTIVATE]);

    session.send(Command::SetNcLevel(NcLevel::Adaptive));
    wait_for(&snapshots, &mut last, |s| s.nc_level == Some(NcLevel::Adaptive));
    session.send(Command::SetEqPreset(3));
    session.send(Command::SetSwitch(Switch::LowLatency, true));
    session.send(Command::SetSwitch(Switch::PauseWhenRemoved, false));
    session.send(Command::SetNoise(NoiseState { mode: NoiseMode::Ambient, ambient_level: 0, focus_on_voice: false }));
    wait_for(&snapshots, &mut last, |s| {
        s.noise.map(|n| n.mode) == Some(NoiseMode::Ambient)
            && s.eq.as_ref().map(|eq| eq.preset) == Some(3)
            && s.switch(Switch::LowLatency) == Some(true)
            && s.switch(Switch::PauseWhenRemoved) == Some(false)
    });
    // The snapshot shows a change at once; the request follows a moment later.
    let deadline = Instant::now() + Duration::from_secs(5);
    let held = || {
        let buds = buds.lock().unwrap();
        (buds.noise, buds.listening_mode, buds.low_latency, buds.in_ear)
    };
    while held() != (7, 3, true, false) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(held(), (7, 3, true, false));
    // The level stays known while transparency is on, for turning noise cancelling back on.
    assert_eq!(last.nc_level, Some(NcLevel::Adaptive));
}
