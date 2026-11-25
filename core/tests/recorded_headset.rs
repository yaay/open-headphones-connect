//! Runs the whole session against a stand-in headset that answers from a
//! recording of a real WH-1000XM3. No Bluetooth is involved, so this passes
//! on any operating system, and it doubles as the smallest complete example
//! of the three `platform` traits an app has to supply.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ohc_core::platform::{Bluetooth, Headset, Incoming, Link};
use ohc_core::protocol::framing::{pack, unpack, Frame, Reassembler, TYPE_ACK};
use ohc_core::protocol::{Generation, NoiseMode, OptimizerPhase};
use ohc_core::session::{self, Phase, Snapshot};

/// Everything the recorded headset said, acknowledgements left out.
fn recording() -> Vec<Frame> {
    include_str!("../src/protocol/fixtures/wh-1000xm3-4.5.2.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let raw: Vec<u8> =
                (0..line.len()).step_by(2).map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap()).collect();
            unpack(&raw).expect("a recorded frame did not unpack")
        })
        .filter(Frame::is_data)
        .collect()
}

struct Computer;

impl Bluetooth for Computer {
    fn radio_is_on(&self) -> bool {
        true
    }

    fn find_headset(&self) -> Option<Box<dyn Headset>> {
        Some(Box::new(RecordedXm3))
    }
}

struct RecordedXm3;

impl Headset for RecordedXm3 {
    fn name(&self) -> &str {
        "WH-1000XM3"
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn open(&self) -> Result<(Box<dyn Link>, Generation), String> {
        let link = Channel { said: recording(), reassembler: Mutex::default(), listener: Arc::default() };
        Ok((Box::new(link), Generation::V1))
    }

    fn connect_audio(&self) -> Result<(), String> {
        Ok(())
    }

    fn disconnect_audio(&self) -> Result<(), String> {
        Ok(())
    }
}

type Listener = Box<dyn FnMut(Incoming) + Send>;

struct Channel {
    said: Vec<Frame>,
    reassembler: Mutex<Reassembler>,
    listener: Arc<Mutex<Option<Listener>>>,
}

impl Channel {
    fn deliver(&self, frame: Frame) {
        if let Some(listener) = self.listener.lock().unwrap().as_mut() {
            listener(Incoming::Frame(frame));
        }
    }
}

impl Link for Channel {
    fn listen(&self, on_incoming: Listener) {
        *self.listener.lock().unwrap() = Some(on_incoming);
    }

    /// Acknowledges every message and, for a question, replays what the real
    /// headset answered: the recorded message whose command number is one
    /// above the question's and that is about the same thing.
    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        let frames = self.reassembler.lock().unwrap().feed(bytes);
        for frame in frames.into_iter().filter(Frame::is_data) {
            self.deliver(Frame { kind: TYPE_ACK, seq: 1u8.wrapping_sub(frame.seq), payload: Vec::new() });
            let answer = self.said.iter().find(|said| {
                said.kind == frame.kind
                    && said.payload.first() == frame.payload.first().map(|command| command.wrapping_add(1)).as_ref()
                    && said.payload.get(1) == frame.payload.get(1)
            });
            if let Some(answer) = answer {
                self.deliver(answer.clone());
            }
        }
        Ok(())
    }
}

#[test]
fn a_recorded_xm3_connects_and_reports_what_the_real_one_did() {
    // The frames this test builds by hand have to survive the real framing.
    assert_eq!(unpack(&pack(TYPE_ACK, 1, &[])).map(|frame| frame.seq), Some(1));

    let (seen, snapshots) = mpsc::channel::<Snapshot>();
    let _session = session::spawn(Computer, move |snapshot| {
        let _ = seen.send(snapshot.clone());
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = Snapshot::default();
    while !(last.phase == Phase::Connected
        && last.firmware.is_some()
        && last.eq.is_some()
        && last.optimizer.is_some_and(|optimizer| optimizer.pressure.is_some())
        && !last.assignments.is_empty())
    {
        let left = deadline.saturating_duration_since(Instant::now());
        last = snapshots.recv_timeout(left).unwrap_or_else(|_| panic!("the session got no further than {last:?}"));
    }

    assert_eq!(last.device_name.as_deref(), Some("WH-1000XM3"));
    assert_eq!(last.model.as_deref(), Some("WH-1000XM3"));
    assert_eq!(last.firmware.as_deref(), Some("4.5.2"));
    assert_eq!(last.connections, 1);
    assert!(last.battery.single.is_some(), "no battery level in {last:?}");
    assert!(last.noise.is_some(), "no noise mode in {last:?}");
    assert_eq!(
        last.noise_modes,
        [NoiseMode::NoiseCancelling, NoiseMode::Wind, NoiseMode::Ambient, NoiseMode::Off]
    );
    // A WH-1000XM3 does not list power-off among its functions.
    assert!(!last.can_power_off);

    // Its NC Optimizer: idle, about thirteen seconds a run, last run at 0.8 atm.
    let optimizer = last.optimizer.expect("no optimizer");
    assert_eq!(optimizer.phase, OptimizerPhase::Idle);
    assert_eq!((optimizer.seconds, optimizer.fit_measured, optimizer.pressure), (13, true, Some(8)));
    // Its custom button, set to noise control, with the two assistants on offer.
    assert_eq!(last.assignments.len(), 1);
    assert_eq!((last.assignments[0].key.as_str(), last.assignments[0].button), ("custom", true));
    assert_eq!(last.assignments[0].function, "noise_control");
    assert_eq!(last.assignments[0].options, ["noise_control", "google_assistant", "amazon_alexa"]);
    // It has no multipoint, so no list of devices.
    assert!(last.devices.is_empty());
}
