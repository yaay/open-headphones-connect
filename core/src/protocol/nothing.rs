//! The control protocol of Nothing and CMF earbuds and headphones.
//!
//! `55 | control u16 | command u16 | length u16 | fsn | payload | CRC u16`,
//! little-endian throughout, over the same kind of serial (RFCOMM) channel as
//! Sony's. Written from what these MIT-licensed projects found and showed on
//! real hardware: SoaOaoS/something-x, faustyu1/nothinx, ncr/omarchy-headphones,
//! marc-alexis-com/nothing-toolbox, LikelyLucid/nothing-headphone-cli,
//! Kuksenok-i-s/nothing_helper and smaller ones. No code was copied from any.

use super::{Battery, BatterySlot, NoiseMode};
use serde::{Deserialize, Serialize};

/// The service the control channel is offered under, on every model.
pub const SERVICE_UUID: u128 = 0xAEAC4A03_DFF5_498F_843A_34487CF133EB;

const START: u8 = 0x55;
/// What the computer puts in every frame's control word: a CRC follows, the
/// frame may be one of several, and it is for earbuds. Devices answer all
/// the variants clients send; this is the one most of them use.
const CONTROL: u16 = 0x0160;
const HAS_CRC: u16 = 0x0020;
/// Larger lengths are taken for noise, not for a frame.
const MAX_PAYLOAD: usize = 4096;

/// The commands this app sends, and the unasked-for messages it reads. An
/// answer is its request with the top bit cleared: `C007` is answered by
/// `4007`, and `F00F` by `700F`.
pub mod command {
    pub const GET_PROTOCOL_VERSION: u16 = 0xC001;
    pub const ACTIVATE: u16 = 0xF001;
    pub const GET_DEVICE_INFO: u16 = 0xC006;
    pub const GET_BATTERY: u16 = 0xC007;
    pub const GET_EXTRA_FEATURES: u16 = 0xC00E;
    pub const SET_EXTRA_FEATURE: u16 = 0xF004;
    pub const GET_MODEL: u16 = 0xC01C;
    pub const GET_NOISE: u16 = 0xC01E;
    pub const SET_NOISE: u16 = 0xF00F;
    /// The equalizer presets of Nothing's own earbuds and headphones.
    pub const GET_EQ_PRESET: u16 = 0xC01F;
    pub const SET_EQ_PRESET: u16 = 0xF010;
    pub const GET_LOW_LATENCY: u16 = 0xC041;
    pub const SET_LOW_LATENCY: u16 = 0xF040;
    pub const GET_FIRMWARE: u16 = 0xC042;
    /// The "listening modes" that CMF earbuds have in place of presets.
    pub const GET_LISTENING_MODE: u16 = 0xC050;
    pub const SET_LISTENING_MODE: u16 = 0xF01D;
    pub const BATTERY_CHANGED: u16 = 0xE001;
    pub const NOISE_CHANGED: u16 = 0xE003;
}

/// The command that answers `request`.
pub fn answer_to(request: u16) -> u16 {
    request & 0x7FFF
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub control: u16,
    pub command: u16,
    /// Echoes the request's number in an answer; 0 (or sometimes 0xFF) in a
    /// message the device sends unasked.
    pub fsn: u8,
    pub payload: Vec<u8>,
}

/// CRC-16/MODBUS: polynomial 0x8005 reflected, starting from 0xFFFF.
pub fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0xFFFF, |mut crc, &byte| {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
        crc
    })
}

/// A request, ready to send. Always with a CRC: some earbuds quietly ignore
/// settings for the rest of a session once a frame without one arrived.
pub fn pack(command: u16, fsn: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 10);
    frame.push(START);
    frame.extend_from_slice(&CONTROL.to_le_bytes());
    frame.extend_from_slice(&command.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    frame.push(fsn);
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&crc16(&frame).to_le_bytes());
    frame
}

/// Turns what comes off the channel, in pieces of any size, into frames.
#[derive(Default)]
pub struct Reassembler {
    buffer: Vec<u8>,
}

impl Reassembler {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            match self.buffer.iter().position(|&b| b == START) {
                Some(start) => drop(self.buffer.drain(..start)),
                None => {
                    self.buffer.clear();
                    break;
                }
            }
            if self.buffer.len() < 8 {
                break;
            }
            let control = u16::from_le_bytes([self.buffer[1], self.buffer[2]]);
            let length = usize::from(u16::from_le_bytes([self.buffer[5], self.buffer[6]]));
            let crc = control & HAS_CRC != 0;
            let total = 8 + length + if crc { 2 } else { 0 };
            if length > MAX_PAYLOAD {
                // A stray 0x55, not a frame: look for the next one.
                self.buffer.remove(0);
                continue;
            }
            if self.buffer.len() < total {
                break;
            }
            let body = &self.buffer[..8 + length];
            let payload = &body[8..];
            // Most devices check the whole frame; CMF Buds Pro 2 firmware only
            // its payload. Either will do.
            let intact = !crc || {
                let sent = u16::from_le_bytes([self.buffer[total - 2], self.buffer[total - 1]]);
                sent == crc16(body) || sent == crc16(payload)
            };
            if !intact {
                self.buffer.remove(0);
                continue;
            }
            frames.push(Frame {
                control,
                command: u16::from_le_bytes([self.buffer[3], self.buffer[4]]),
                fsn: self.buffer[7],
                payload: payload.to_vec(),
            });
            self.buffer.drain(..total);
        }
        frames
    }
}

/// How strongly noise cancelling works.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NcLevel {
    High,
    Mid,
    Low,
    /// Follows how loud it is around you.
    Adaptive,
}

impl NcLevel {
    pub const ALL: [NcLevel; 4] = [NcLevel::High, NcLevel::Mid, NcLevel::Low, NcLevel::Adaptive];

    fn code(self) -> u8 {
        match self {
            NcLevel::High => 1,
            NcLevel::Mid => 2,
            NcLevel::Low => 3,
            NcLevel::Adaptive => 4,
        }
    }

    fn from_code(code: u8) -> Option<NcLevel> {
        NcLevel::ALL.into_iter().find(|level| level.code() == code)
    }
}

const NOISE_OFF: u8 = 5;
const NOISE_TRANSPARENCY: u8 = 7;

/// Noise control as the device reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Noise {
    pub mode: NoiseMode,
    /// The level in use, or the one last used while noise cancelling is not on.
    /// `None` where the device did not say.
    pub level: Option<NcLevel>,
    /// Whether the device has noise-cancelling levels at all: false where it
    /// reports no last level, or 0.
    pub has_levels: bool,
}

/// The answer to [`command::GET_NOISE`], or a [`command::NOISE_CHANGED`]:
/// `(type, value, 0)` triplets in no fixed order. Type 1 is the mode, type 2
/// the last noise-cancelling level.
pub fn parse_noise(payload: &[u8]) -> Option<Noise> {
    let mut mode = None;
    let mut last = None;
    for entry in payload.chunks_exact(3) {
        match entry[0] {
            1 => mode = Some(entry[1]),
            2 => last = Some(entry[1]),
            _ => {}
        }
    }
    let mode = mode?;
    let (mode, level) = match mode {
        // 0 is off on older models, 0xFE transparency.
        0 | NOISE_OFF => (NoiseMode::Off, None),
        NOISE_TRANSPARENCY | 0xFE => (NoiseMode::Ambient, None),
        code => (NoiseMode::NoiseCancelling, Some(NcLevel::from_code(code)?)),
    };
    let last = last.and_then(NcLevel::from_code);
    Some(Noise { mode, level: level.or(last), has_levels: level.is_some() || last.is_some() })
}

/// Turns noise cancelling on at `level`, or switches to transparency or off.
pub fn set_noise(mode: NoiseMode, level: NcLevel) -> Vec<u8> {
    let code = match mode {
        NoiseMode::NoiseCancelling => level.code(),
        NoiseMode::Ambient => NOISE_TRANSPARENCY,
        NoiseMode::Off | NoiseMode::Wind => NOISE_OFF,
    };
    vec![1, code, 0]
}

/// The answer to [`command::GET_BATTERY`], or a [`command::BATTERY_CHANGED`]:
/// a count, then each part's id and level. The top bit of a level means
/// charging. The case only reports while it is open.
pub fn parse_battery(payload: &[u8]) -> Vec<(BatterySlot, Battery)> {
    let Some((&count, parts)) = payload.split_first() else { return Vec::new() };
    parts
        .chunks_exact(2)
        .take(usize::from(count))
        .filter_map(|part| {
            let slot = match part[0] {
                2 => BatterySlot::Left,
                3 => BatterySlot::Right,
                4 => BatterySlot::Case,
                6 => BatterySlot::Single,
                _ => return None,
            };
            Some((slot, Battery { level: (part[1] & 0x7F).min(100), charging: part[1] & 0x80 != 0 }))
        })
        .collect()
}

/// Settings stored as numbered on/off features. 1 is in-ear detection.
pub const IN_EAR_DETECTION: u8 = 1;

/// The answer to [`command::GET_EXTRA_FEATURES`]: a count, then id and value
/// pairs.
pub fn parse_extra_features(payload: &[u8]) -> Vec<(u8, bool)> {
    let Some((&count, pairs)) = payload.split_first() else { return Vec::new() };
    pairs.chunks_exact(2).take(usize::from(count)).map(|pair| (pair[0], pair[1] != 0)).collect()
}

pub fn set_extra_feature(id: u8, on: bool) -> Vec<u8> {
    vec![1, id, u8::from(on)]
}

/// The answer to [`command::GET_LOW_LATENCY`]: 1 on, 2 off, and 0 on a
/// device where it was never set. Some answer with a leading 0.
pub fn parse_low_latency(payload: &[u8]) -> Option<bool> {
    let value = match payload {
        [0, value] => *value,
        [value, ..] => *value,
        [] => return None,
    };
    Some(value == 1)
}

pub fn set_low_latency(on: bool) -> Vec<u8> {
    vec![if on { 1 } else { 2 }]
}

/// Text answers (firmware, protocol version) end in a newline or NUL.
pub fn parse_text(payload: &[u8]) -> String {
    String::from_utf8_lossy(payload).trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string()
}

/// How a model's equalizer is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqFamily {
    /// Nothing's earbuds and headphones: [`command::GET_EQ_PRESET`].
    Presets,
    /// CMF earbuds: [`command::GET_LISTENING_MODE`].
    ListeningModes,
}

impl EqFamily {
    /// The presets, by the value the device uses, with their names. A custom
    /// curve is left out: this app does not edit one.
    // ponytail: values 1 and 3 are Voice and More Bass on Headphone (1) and in
    // the official app's tables, but the other way round in one source tested
    // on an Ear (2). Confirm with an owner of an Ear.
    pub fn presets(self) -> &'static [(u8, &'static str)] {
        match self {
            EqFamily::Presets => &[(0, "Balanced"), (1, "Voice"), (2, "More Treble"), (3, "More Bass")],
            EqFamily::ListeningModes => &[
                (0, "Balanced"),
                (1, "Rock"),
                (2, "Electronic"),
                (3, "Pop"),
                (4, "Enhance Vocals"),
                (5, "Classical"),
            ],
        }
    }

    pub fn get(self) -> u16 {
        match self {
            EqFamily::Presets => command::GET_EQ_PRESET,
            EqFamily::ListeningModes => command::GET_LISTENING_MODE,
        }
    }

    /// The command and payload that choose `preset`.
    pub fn set(self, preset: u8) -> (u16, Vec<u8>) {
        match self {
            EqFamily::Presets => (command::SET_EQ_PRESET, vec![preset]),
            EqFamily::ListeningModes => (command::SET_LISTENING_MODE, vec![preset, 0]),
        }
    }
}

/// What can be told from a Nothing or CMF device's name: its proper name,
/// whether it is a pair of earbuds, how its equalizer works, and which
/// noise-cancelling levels it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: &'static str,
    pub earbuds: bool,
    /// `None` where no source says; the session then asks both ways.
    pub eq: Option<EqFamily>,
    pub levels: &'static [NcLevel],
}

impl Model {
    /// From the Bluetooth name, which owners may have changed: `None` then.
    pub fn from_name(name: &str) -> Option<Model> {
        use EqFamily::*;
        use NcLevel::*;
        const ALL: &[NcLevel] = &NcLevel::ALL;
        let upper = name.to_ascii_uppercase();
        // Longest names first, so that "CMF BUDS PRO 2" is not taken for "CMF BUDS".
        let table: [(&str, &str, bool, Option<EqFamily>, &[NcLevel]); 14] = [
            ("CMF BUDS PRO 2", "CMF Buds Pro 2", true, Some(ListeningModes), ALL),
            ("CMF HEADPHONE PRO", "CMF Headphone Pro", false, None, ALL),
            ("CMF NECKBAND PRO", "CMF Neckband Pro", true, None, ALL),
            ("CMF BUDS PRO", "CMF Buds Pro", true, None, &[High, Mid, Low]),
            ("CMF BUDS 2", "CMF Buds 2", true, None, ALL),
            ("CMF BUDS", "CMF Buds", true, Some(ListeningModes), &[High]),
            ("HEADPHONE (1)", "Headphone (1)", false, Some(Presets), ALL),
            ("EAR (STICK)", "Ear (stick)", true, Some(Presets), &[]),
            ("EAR (OPEN)", "Ear (open)", true, Some(Presets), &[]),
            ("EAR (1)", "Ear (1)", true, Some(Presets), &[High, Low]),
            ("EAR (2)", "Ear (2)", true, Some(Presets), ALL),
            ("EAR (3)", "Ear (3)", true, Some(Presets), ALL),
            ("EAR (A)", "Ear (a)", true, Some(Presets), ALL),
            ("NOTHING EAR", "Ear", true, Some(Presets), ALL),
        ];
        table.into_iter().find(|entry| upper.contains(entry.0)).map(|(_, name, earbuds, eq, levels)| Model {
            name,
            earbuds,
            eq,
            levels,
        })
    }
}

/// Whether a Bluetooth device of this name is a Nothing or CMF one.
pub fn is_nothing_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("NOTHING ") || upper.starts_with("CMF ") || upper.starts_with("EAR (") || upper.starts_with("HEADPHONE (")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        text.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn one(text: &str) -> Frame {
        let frames = Reassembler::default().feed(&hex(text));
        assert_eq!(frames.len(), 1, "{text}");
        frames.into_iter().next().unwrap()
    }

    #[test]
    fn the_crc_is_modbus() {
        assert_eq!(crc16(b"123456789"), 0x4B37);
    }

    #[test]
    fn requests_match_captured_ones() {
        assert_eq!(pack(command::GET_BATTERY, 1, &[]), hex("55 60 01 07 C0 00 00 01 AC DF"));
        assert_eq!(pack(command::GET_NOISE, 1, &[3]), hex("55 60 01 1E C0 01 00 01 03 9C 89"));
        assert_eq!(pack(command::GET_DEVICE_INFO, 1, &[]), hex("55 60 01 06 C0 00 00 01 91 1F"));
        assert_eq!(
            pack(command::SET_NOISE, 1, &set_noise(NoiseMode::Ambient, NcLevel::High)),
            hex("55 60 01 0F F0 03 00 01 01 07 00 FA 77")
        );
        assert_eq!(pack(command::SET_LOW_LATENCY, 1, &set_low_latency(true)), hex("55 60 01 40 F0 01 00 01 01 50 32"));
        assert_eq!(pack(command::SET_LOW_LATENCY, 1, &set_low_latency(false)), hex("55 60 01 40 F0 01 00 01 02 10 33"));
    }

    #[test]
    fn answers_and_events_are_read() {
        let battery = one("55 60 01 07 40 05 00 01 02 02 64 03 64 32 89");
        assert_eq!(battery.command, answer_to(command::GET_BATTERY));
        assert_eq!(
            parse_battery(&battery.payload),
            [
                (BatterySlot::Left, Battery { level: 100, charging: false }),
                (BatterySlot::Right, Battery { level: 100, charging: false })
            ]
        );
        let headphones = one("55 60 01 07 40 03 00 01 01 06 0F 0B 8E");
        assert_eq!(parse_battery(&headphones.payload), [(BatterySlot::Single, Battery { level: 15, charging: false })]);

        // An event with a CRC, then one without.
        let event = one("55 20 01 01 E0 07 00 00 03 02 64 03 64 04 55 2D 75");
        assert_eq!((event.command, event.fsn), (command::BATTERY_CHANGED, 0));
        assert_eq!(parse_battery(&event.payload)[2], (BatterySlot::Case, Battery { level: 85, charging: false }));
        let noise = one("55 00 03 03 E0 06 00 00 01 05 00 02 04 00");
        assert_eq!(
            parse_noise(&noise.payload),
            Some(Noise { mode: NoiseMode::Off, level: Some(NcLevel::Adaptive), has_levels: true })
        );
    }

    #[test]
    fn noise_is_read_by_type_in_any_order() {
        let answer = one("55 60 01 1E 40 06 00 01 01 01 00 02 01 00 47 EA");
        assert_eq!(
            parse_noise(&answer.payload),
            Some(Noise { mode: NoiseMode::NoiseCancelling, level: Some(NcLevel::High), has_levels: true })
        );
        // CMF Buds Pro 2 puts the level first.
        assert_eq!(
            parse_noise(&[2, 4, 0, 1, 7, 0]),
            Some(Noise { mode: NoiseMode::Ambient, level: Some(NcLevel::Adaptive), has_levels: true })
        );
        // Some firmware sends the mode alone.
        assert_eq!(parse_noise(&[1, 7, 0]), Some(Noise { mode: NoiseMode::Ambient, level: None, has_levels: false }));
        assert_eq!(parse_noise(&[1, 0, 0]).map(|n| n.mode), Some(NoiseMode::Off));
    }

    #[test]
    fn a_crc_over_the_payload_alone_is_accepted() {
        // CMF Buds Pro 2: listening mode 0, its CRC covering only the payload.
        let frame = one("55 20 01 50 40 01 00 00 00 BF 40");
        assert_eq!((frame.command, frame.payload), (answer_to(command::GET_LISTENING_MODE), vec![0]));
    }

    #[test]
    fn frames_split_glued_or_behind_noise_come_out_whole() {
        let mut stream = hex("12 55 34");
        stream.extend(hex("55 60 01 07 40 03 00 01 01 06 0F 0B 8E"));
        stream.extend(hex("55 60 01 0F 70 00 00 01 6B DE"));
        let mut reassembler = Reassembler::default();
        let mut frames = Vec::new();
        for piece in stream.chunks(5) {
            frames.extend(reassembler.feed(piece));
        }
        let commands: Vec<u16> = frames.iter().map(|f| f.command).collect();
        assert_eq!(commands, [0x4007, 0x700F]);
    }

    #[test]
    fn settings_are_read() {
        assert_eq!(parse_low_latency(&[1]), Some(true));
        assert_eq!(parse_low_latency(&[2]), Some(false));
        assert_eq!(parse_low_latency(&[0, 1]), Some(true));
        assert_eq!(parse_low_latency(&[0]), Some(false));
        assert_eq!(parse_extra_features(&[2, 1, 1, 2, 0]), [(1, true), (2, false)]);
        assert_eq!(parse_text(b"1.0.1.74\0"), "1.0.1.74");
        // The in-ear switch, as captured from an Ear (2) (there with fsn 0x26).
        assert_eq!(
            pack(command::SET_EXTRA_FEATURE, 0x26, &set_extra_feature(IN_EAR_DETECTION, true)),
            hex("55 60 01 04 F0 03 00 26 01 01 01 73 10")
        );
    }

    #[test]
    fn models_are_told_apart_by_name() {
        let model = |name| Model::from_name(name).map(|m| m.name);
        assert_eq!(model("CMF Buds Pro 2"), Some("CMF Buds Pro 2"));
        assert_eq!(model("CMF Buds"), Some("CMF Buds"));
        assert_eq!(model("Nothing ear (1)"), Some("Ear (1)"));
        assert_eq!(model("Nothing Headphone (1)"), Some("Headphone (1)"));
        assert_eq!(model("Nothing Ear"), Some("Ear"));
        assert_eq!(model("Yehia's buds"), None);
        assert!(!Model::from_name("CMF Headphone Pro").unwrap().earbuds);
        assert!(is_nothing_name("CMF Buds 2") && is_nothing_name("Ear (a)") && !is_nothing_name("WH-1000XM3"));
    }
}
