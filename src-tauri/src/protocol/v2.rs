//! The command tables "v2" headsets speak (WH-1000XM5/XM6, WF-1000XM4/XM5,
//! the LinkBuds family, ULT Wear, WH-CH720N…).
//!
//! The first port came from abhiinhii/sony-tray. The function table, the
//! noise-control variants and everything past battery, noise and equalizer
//! follow libmdr in mos9527/SonyHeadphonesClient (MIT), and are checked
//! against that project's recordings of real headsets (see `fixtures/`).
//!
//! Two things differ from the first generation and are easy to trip over:
//! on/off settings are sent as 0 = on, 1 = off, and a headset describes
//! itself with a list of functions per table rather than one list.

use super::framing::TYPE_DATA_MDR_NO2;
use super::{
    battery, codec_name, BatterySlot, EqState, Event, NoiseMode, NoiseState, Playback, Switch, Transport,
};

pub const SERVICE_UUID: u128 = 0x956C7B26_D49A_4BA8_B03F_B17D393CB6E2;
pub const MIN_AMBIENT_LEVEL: u8 = 1;
pub const MAX_AMBIENT_LEVEL: u8 = 20;
pub const MAX_VOLUME: u8 = 30;

/// Table 1 function ids, as listed by `CONNECT_RET_SUPPORT_FUNCTION`.
pub mod function {
    pub const CODEC_INDICATOR: u8 = 0x12;
    pub const BATTERY_LEVEL: u8 = 0x20;
    pub const LEFT_RIGHT_BATTERY_LEVEL: u8 = 0x21;
    pub const CRADLE_BATTERY_LEVEL: u8 = 0x22;
    pub const POWER_OFF: u8 = 0x23;
    pub const AUTO_POWER_OFF: u8 = 0x24;
    pub const AUTO_POWER_OFF_WITH_WEARING_DETECTION: u8 = 0x25;
    /// The three battery functions again, in the variant that also reports
    /// the level at which the headset warns.
    pub const BATTERY_LEVEL_WITH_THRESHOLD: u8 = 0x28;
    pub const PRESET_EQ: u8 = 0x50;
    pub const PRESET_EQ_NONCUSTOMIZABLE: u8 = 0x52;
    pub const PRESET_EQ_AND_ULT_MODE: u8 = 0x53;
    pub const PRESET_EQ_AND_ERRORCODE: u8 = 0x57;
    pub const FIXED_MESSAGE: u8 = 0x90;
    pub const PLAYBACK_CONTROLLERS: [u8; 4] = [0xA1, 0xA2, 0xA3, 0xA4];
    pub const GENERAL_SETTINGS: [u8; 4] = [0xD1, 0xD2, 0xD3, 0xD4];
    pub const UPSCALING_AUTO_OFF: u8 = 0xE2;
    pub const PLAYBACK_CONTROL_BY_WEARING: u8 = 0xF1;
    pub const SMART_TALKING_MODE_TYPE1: u8 = 0xF2;
    pub const SMART_TALKING_MODE_TYPE2: u8 = 0xFC;
}

// Inquired types: the second byte of most messages, naming the feature.
const DEVICE_MODEL_NAME: u8 = 0x01;
const DEVICE_FW_VERSION: u8 = 0x02;
const DEVICE_SERIES_AND_COLOUR: u8 = 0x03;
const COMMON_AUDIO_CODEC: u8 = 0x02;
const POWER_SWITCH_OFF: u8 = 0x03;
const POWER_AUTO_OFF: u8 = 0x04;
const POWER_AUTO_OFF_WEARING: u8 = 0x05;
const EQ_PRESET: u8 = 0x00;
const NOISE_BUTTON_TOGGLE: u8 = 0x30;
const ALERT_FIXED_MESSAGE: u8 = 0x00;
const ALERT_POSITIVE_NEGATIVE: u8 = 0x01;
const PLAY_CONTROL: u8 = 0x01;
const PLAY_MUSIC_VOLUME: u8 = 0x20;
const GS_BOOLEAN: u8 = 0x00;
const AUDIO_UPSCALING: u8 = 0x01;
const SYSTEM_PLAYBACK_CONTROL_BY_WEARING: u8 = 0x01;
const SYSTEM_SMART_TALKING_TYPE1: u8 = 0x02;
const SYSTEM_SMART_TALKING_TYPE2: u8 = 0x0C;
const ENGLISH: u8 = 0x01;
/// `ValueChangeStatus::CHANGED`: the value is final, not mid-drag.
const CHANGED: u8 = 0x01;

/// On/off settings travel inverted in this generation.
fn on_off(on: bool) -> u8 {
    if on {
        0x00
    } else {
        0x01
    }
}

// ----- noise control ----------------------------------------------------------

/// How the noise-cancelling part of a message is expressed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Canceller {
    /// 0 = off, 1 = on.
    OnOff(usize),
    /// 0 = off, 1 = single microphone (wind noise reduction), 2 = dual.
    Graded(usize),
}

const NC_SINGLE: u8 = 0x01;
const NC_DUAL: u8 = 0x02;

/// Where each field sits in one variant of the noise-control message. Sony
/// has a dozen variants (`NcAsmInquiredType`); a headset uses exactly one.
///
/// The message is always `[command, variant, changeStatus, effect, …]`; the
/// numbers here are positions in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NoiseLayout {
    /// The `NcAsmInquiredType` value.
    pub kind: u8,
    len: usize,
    /// 0 = noise cancelling, 1 = ambient sound.
    mode: Option<usize>,
    canceller: Option<Canceller>,
    ambient_on: Option<usize>,
    voice: Option<usize>,
    level: Option<usize>,
    /// Wind noise reduction is offered as a mode of its own.
    wind: bool,
}

/// Function id to noise-control variant, richest first.
const NOISE_FUNCTIONS: [(u8, u8); 12] = [
    (0x6D, 0x19),
    (0x6B, 0x17),
    (0x6C, 0x18),
    (0x6A, 0x16),
    (0x68, 0x15),
    (0x65, 0x14),
    (0x64, 0x13),
    (0x63, 0x12),
    (0x62, 0x11),
    (0x61, 0x01),
    (0x67, 0x22),
    (0x66, 0x21),
];

impl NoiseLayout {
    /// The layout of one `NcAsmInquiredType`. Only 0x17 (WH-1000XM5,
    /// WF-1000XM5…), 0x19 and 0x22 have been seen working on real headsets;
    /// the rest follow Sony's own message tables.
    pub fn of(kind: u8) -> Option<NoiseLayout> {
        use Canceller::{Graded, OnOff};
        let l = |len, mode, canceller, ambient_on, voice, level, wind| NoiseLayout {
            kind,
            len,
            mode,
            canceller,
            ambient_on,
            voice,
            level,
            wind,
        };
        Some(match kind {
            0x01 => l(5, None, Some(OnOff(4)), None, None, None, false),
            0x11 => l(7, None, Some(OnOff(4)), Some(6), Some(5), None, false),
            0x12 => l(7, None, Some(Graded(4)), Some(6), Some(5), None, true),
            0x13 => l(7, None, Some(OnOff(4)), None, Some(5), Some(6), false),
            0x14 => l(7, None, Some(Graded(4)), None, Some(5), Some(6), true),
            0x15 => l(8, Some(4), Some(Graded(5)), None, Some(6), Some(7), false),
            0x16 => l(8, Some(4), Some(Graded(5)), None, Some(6), Some(7), true),
            0x17 | 0x18 => l(7, Some(4), None, None, Some(5), Some(6), false),
            // Two more bytes follow: automatic ambient sound and its sensitivity.
            0x19 => l(9, Some(4), None, None, Some(5), Some(6), false),
            0x21 => l(6, None, None, Some(5), Some(4), None, false),
            0x22 => l(6, None, None, None, Some(4), Some(5), false),
            _ => return None,
        })
    }

    /// The variant a headset uses, from its table 1 function list.
    pub fn for_functions(functions: &[u8]) -> Option<NoiseLayout> {
        NOISE_FUNCTIONS
            .iter()
            .find(|(function, _)| functions.contains(function))
            .and_then(|(_, kind)| NoiseLayout::of(*kind))
    }

    fn has_cancelling(&self) -> bool {
        self.mode.is_some() || self.canceller.is_some()
    }

    fn has_ambient(&self) -> bool {
        self.mode.is_some() || self.ambient_on.is_some() || self.level.is_some()
    }

    pub fn modes(&self) -> Vec<NoiseMode> {
        let mut modes = Vec::new();
        if self.has_cancelling() {
            modes.push(NoiseMode::NoiseCancelling);
            if self.wind {
                modes.push(NoiseMode::Wind);
            }
        }
        if self.has_ambient() {
            modes.push(NoiseMode::Ambient);
        }
        modes.push(NoiseMode::Off);
        modes
    }

    pub fn has_level(&self) -> bool {
        self.level.is_some()
    }

    pub fn has_voice_focus(&self) -> bool {
        self.voice.is_some()
    }

    /// Reads a `NCASM_RET_PARAM` or `NCASM_NTFY_PARAM` message.
    pub fn read(&self, p: &[u8]) -> Option<NoiseState> {
        if p.len() < self.len || p[1] != self.kind {
            return None;
        }
        let cancelling = match self.canceller {
            Some(Canceller::OnOff(at)) => (p[at] != 0).then_some(NoiseMode::NoiseCancelling),
            Some(Canceller::Graded(at)) => match p[at] {
                0 => None,
                NC_SINGLE if self.wind => Some(NoiseMode::Wind),
                _ => Some(NoiseMode::NoiseCancelling),
            },
            None => None,
        };
        let mode = if p[3] == 0 {
            NoiseMode::Off
        } else if let Some(at) = self.mode {
            if p[at] == 1 {
                NoiseMode::Ambient
            } else {
                cancelling.unwrap_or(NoiseMode::NoiseCancelling)
            }
        } else if let Some(mode) = cancelling {
            mode
        } else if let Some(at) = self.ambient_on {
            if p[at] != 0 {
                NoiseMode::Ambient
            } else {
                NoiseMode::Off
            }
        } else if self.has_ambient() {
            NoiseMode::Ambient
        } else {
            NoiseMode::Off
        };
        Some(NoiseState {
            mode,
            ambient_level: self.level.map_or(0, |at| p[at]),
            focus_on_voice: self.voice.is_some_and(|at| p[at] == 1),
        })
    }

    /// Builds `NCASM_SET_PARAM`. `last` is the headset's most recent message
    /// of this kind: fields this app has no control for (automatic ambient
    /// sound, its sensitivity, the cancelling flavour) are sent back as they
    /// were rather than reset.
    pub fn write(&self, last: Option<&[u8]>, state: NoiseState) -> Vec<u8> {
        let mut out = match last {
            Some(last) if last.len() >= self.len && last[1] == self.kind => last.to_vec(),
            _ => vec![0; self.len],
        };
        out[0] = 0x68;
        out[1] = self.kind;
        out[2] = CHANGED;
        out[3] = (state.mode != NoiseMode::Off) as u8;
        if state.mode == NoiseMode::Off {
            return out;
        }
        let ambient = state.mode == NoiseMode::Ambient;
        if let Some(at) = self.mode {
            out[at] = ambient as u8;
        }
        match self.canceller {
            // Without a mode byte, the canceller being off is what says "ambient".
            Some(Canceller::OnOff(at) | Canceller::Graded(at)) if ambient => {
                if self.mode.is_none() {
                    out[at] = 0;
                }
            }
            Some(Canceller::OnOff(at)) => out[at] = 1,
            Some(Canceller::Graded(at)) => {
                out[at] = match (self.wind, state.mode) {
                    (true, NoiseMode::Wind) => NC_SINGLE,
                    (true, _) => NC_DUAL,
                    // No wind mode to choose: keep whichever flavour was in use.
                    (false, _) if out[at] != 0 => out[at],
                    (false, _) => NC_DUAL,
                };
            }
            None => {}
        }
        if let Some(at) = self.ambient_on {
            out[at] = ambient as u8;
        }
        if let Some(at) = self.voice {
            out[at] = state.focus_on_voice as u8;
        }
        if let (Some(at), true) = (self.level, ambient) {
            out[at] = state.ambient_level.clamp(MIN_AMBIENT_LEVEL, MAX_AMBIENT_LEVEL);
        }
        out
    }
}

pub fn get_noise(kind: u8) -> Vec<u8> {
    vec![0x66, kind]
}

// ----- what a headset offers --------------------------------------------------

/// What a headset offers, worked out from its two function lists.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Capabilities {
    pub noise: Option<NoiseLayout>,
    /// Battery inquiries to poll.
    pub battery_inquiries: Vec<u8>,
    /// The equalizer variant, as the type to ask its description with.
    pub eq: Option<u8>,
    pub power_off: bool,
    pub playback: bool,
    pub codec: bool,
    pub alerts: bool,
    pub upscaling: bool,
    /// The automatic power-off variant, as its inquired type.
    pub auto_power_off: Option<u8>,
    /// Speak-to-Chat, as its inquired type.
    pub speak_to_chat: Option<u8>,
    pub pause_when_removed: bool,
    /// Numbered general settings the headset has.
    pub general_settings: Vec<u8>,
    /// Voice guidance, as its inquired type on table 2.
    pub voice_guidance: Option<u8>,
}

impl Capabilities {
    /// `table1` and `table2` are the function ids each table listed.
    ///
    /// A headset that listed nothing at all is treated as the most common
    /// kind (one battery, noise cancelling with an ambient level), so that a
    /// lost reply does not leave the panel empty.
    pub fn resolve(table1: &[u8], table2: &[u8]) -> Self {
        use function::*;
        let has = |id: u8| table1.contains(&id);
        if table1.is_empty() {
            return Capabilities {
                noise: NoiseLayout::of(0x17),
                battery_inquiries: vec![0x00],
                eq: Some(EQ_PRESET),
                ..Capabilities::default()
            };
        }
        let mut battery_inquiries = Vec::new();
        for kind in 0u8..=2 {
            if has(BATTERY_LEVEL + kind) {
                battery_inquiries.push(kind);
            } else if has(BATTERY_LEVEL_WITH_THRESHOLD + kind) {
                battery_inquiries.push(kind + 8);
            }
        }
        let eq = [
            (PRESET_EQ, 0x00),
            (PRESET_EQ_NONCUSTOMIZABLE, 0x02),
            (PRESET_EQ_AND_ULT_MODE, 0x03),
            (PRESET_EQ_AND_ERRORCODE, 0x04),
        ]
        .iter()
        .find(|(function, _)| has(*function))
        .map(|(_, kind)| *kind);
        let voice_guidance = [(0x40, 0x00), (0x41, 0x01), (0x42, 0x01), (0x44, 0x02), (0x45, 0x03)]
            .iter()
            .find(|(function, _)| table2.contains(function))
            .map(|(_, kind)| *kind);
        Capabilities {
            noise: NoiseLayout::for_functions(table1),
            battery_inquiries,
            eq,
            power_off: has(POWER_OFF),
            playback: PLAYBACK_CONTROLLERS.iter().any(|id| has(*id)),
            codec: has(CODEC_INDICATOR),
            alerts: has(FIXED_MESSAGE),
            upscaling: has(UPSCALING_AUTO_OFF),
            auto_power_off: if has(AUTO_POWER_OFF) {
                Some(POWER_AUTO_OFF)
            } else if has(AUTO_POWER_OFF_WITH_WEARING_DETECTION) {
                Some(POWER_AUTO_OFF_WEARING)
            } else {
                None
            },
            speak_to_chat: if has(SMART_TALKING_MODE_TYPE2) {
                Some(SYSTEM_SMART_TALKING_TYPE2)
            } else if has(SMART_TALKING_MODE_TYPE1) {
                Some(SYSTEM_SMART_TALKING_TYPE1)
            } else {
                None
            },
            pause_when_removed: has(PLAYBACK_CONTROL_BY_WEARING),
            general_settings: GENERAL_SETTINGS.iter().copied().filter(|id| has(*id)).collect(),
            voice_guidance,
        }
    }
}

// ----- identity ---------------------------------------------------------------

pub fn get_protocol_info() -> Vec<u8> {
    vec![0x00, 0x00]
}

/// The same request lists either table's functions, depending on the frame
/// type it is sent with.
pub fn get_support_functions() -> Vec<u8> {
    vec![0x06, 0x00]
}

pub fn get_model_name() -> Vec<u8> {
    vec![0x04, DEVICE_MODEL_NAME]
}

pub fn get_firmware_version() -> Vec<u8> {
    vec![0x04, DEVICE_FW_VERSION]
}

pub fn get_series_and_colour() -> Vec<u8> {
    vec![0x04, DEVICE_SERIES_AND_COLOUR]
}

pub fn get_codec() -> Vec<u8> {
    vec![0x12, COMMON_AUDIO_CODEC]
}

// ----- power ------------------------------------------------------------------

pub fn get_battery(inquiry: u8) -> Vec<u8> {
    vec![0x22, inquiry]
}

pub fn power_off() -> Vec<u8> {
    vec![0x24, POWER_SWITCH_OFF, 0x01]
}

pub fn get_auto_power_off_options(kind: u8) -> Vec<u8> {
    vec![0x20, kind]
}

pub fn get_auto_power_off(kind: u8) -> Vec<u8> {
    vec![0x26, kind]
}

/// The choices a headset is assumed to have until it lists its own.
pub fn default_auto_power_off_options(kind: u8) -> Vec<u8> {
    if kind == POWER_AUTO_OFF_WEARING {
        vec![0x10, 0x11]
    } else {
        vec![0x00, 0x01, 0x02, 0x03, 0x11]
    }
}

/// The message carries the choice now in effect and, separately, the timed
/// choice to go back to.
pub fn set_auto_power_off(kind: u8, choice: u8) -> Vec<u8> {
    let timed = if choice < 0x10 { choice } else { 0x00 };
    vec![0x28, kind, choice, timed]
}

// ----- equalizer --------------------------------------------------------------

/// How a headset's equalizer values map to what the sliders show.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct EqLayout {
    /// How many values a message carries, per the headset. 0 = not said.
    pub values: u8,
    /// How many positions each value has (13 means −6…+6). 0 = not said.
    pub steps: u8,
    /// How many of the values are frequency bands. 0 = not said.
    pub frequencies: u8,
}

impl EqLayout {
    /// Half the range: what a raw value of "flat" is.
    fn offset(&self, raw: usize) -> u8 {
        if self.values as usize == raw && self.steps >= 3 && self.steps % 2 == 1 {
            (self.steps - 1) / 2
        } else if raw == 6 {
            // Sony's two fixed layouts: Clear Bass + 5 bands, or 10 bands.
            10
        } else {
            6
        }
    }

    /// Whether the first value is Clear Bass rather than a band.
    fn leads_with_clear_bass(&self, raw: usize) -> bool {
        if self.frequencies != 0 {
            raw == self.frequencies as usize + 1
        } else {
            raw == 6
        }
    }

    pub fn decode(&self, preset: u8, raw: &[u8]) -> EqState {
        let offset = self.offset(raw.len()) as i16;
        let user = |value: &u8| (*value as i16 - offset).clamp(-offset, offset) as i8;
        let bass = self.leads_with_clear_bass(raw.len()) && !raw.is_empty();
        EqState {
            preset,
            clear_bass: bass.then(|| user(&raw[0])),
            bands: raw[bass as usize..].iter().map(user).collect(),
            range: offset as i8,
        }
    }

    pub fn encode(&self, preset: u8, clear_bass: Option<i8>, bands: &[i8]) -> Option<Vec<u8>> {
        let count = bands.len() + clear_bass.is_some() as usize;
        if bands.is_empty() || count > 16 {
            return None;
        }
        let offset = self.offset(count) as i16;
        let raw = |value: i8| ((value as i16).clamp(-offset, offset) + offset) as u8;
        let mut out = vec![0x58, EQ_PRESET, preset, count as u8];
        out.extend(clear_bass.map(raw));
        out.extend(bands.iter().map(|band| raw(*band)));
        Some(out)
    }
}

/// Asks which presets the equalizer has and how its values are scaled.
/// `kind` comes from [`Capabilities::eq`].
pub fn get_eq_layout(kind: u8) -> Vec<u8> {
    vec![0x50, kind, ENGLISH]
}

/// Asks which frequency each band is.
pub fn get_eq_frequencies() -> Vec<u8> {
    vec![0x5A, EQ_PRESET]
}

pub fn get_eq() -> Vec<u8> {
    vec![0x56, EQ_PRESET]
}

pub fn set_eq_preset(preset: u8) -> Vec<u8> {
    vec![0x58, EQ_PRESET, preset, 0x00]
}

// ----- playback ---------------------------------------------------------------

pub fn get_volume() -> Vec<u8> {
    vec![0xA6, PLAY_MUSIC_VOLUME]
}

pub fn set_volume(volume: u8) -> Vec<u8> {
    vec![0xA8, PLAY_MUSIC_VOLUME, volume.min(MAX_VOLUME)]
}

pub fn get_playback() -> Vec<u8> {
    vec![0xA2, PLAY_CONTROL]
}

pub fn transport(control: Transport) -> Vec<u8> {
    let code = match control {
        Transport::Pause => 0x01,
        Transport::Next => 0x02,
        Transport::Previous => 0x03,
        Transport::Play => 0x07,
    };
    vec![0xA4, PLAY_CONTROL, 0x00, code]
}

// ----- settings ---------------------------------------------------------------

pub fn get_upscaling() -> Vec<u8> {
    vec![0xE6, AUDIO_UPSCALING]
}

/// Unlike the other settings this one is a mode: 0 = off, 1 = automatic.
pub fn set_upscaling(on: bool) -> Vec<u8> {
    vec![0xE8, AUDIO_UPSCALING, on as u8]
}

pub fn get_pause_when_removed() -> Vec<u8> {
    vec![0xF6, SYSTEM_PLAYBACK_CONTROL_BY_WEARING]
}

pub fn set_pause_when_removed(on: bool) -> Vec<u8> {
    vec![0xF8, SYSTEM_PLAYBACK_CONTROL_BY_WEARING, on_off(on)]
}

/// `kind` comes from [`Capabilities::speak_to_chat`].
pub fn get_speak_to_chat(kind: u8) -> Vec<u8> {
    vec![0xF6, kind]
}

/// The last byte keeps the "preview" mode, used while setting it up, off.
pub fn set_speak_to_chat(kind: u8, on: bool) -> Vec<u8> {
    vec![0xF8, kind, on_off(on), on_off(false)]
}

pub fn get_general_setting_info(slot: u8) -> Vec<u8> {
    vec![0xD0, slot, ENGLISH]
}

pub fn get_general_setting(slot: u8) -> Vec<u8> {
    vec![0xD6, slot]
}

pub fn set_general_setting(slot: u8, on: bool) -> Vec<u8> {
    vec![0xD8, slot, GS_BOOLEAN, on_off(on)]
}

/// Asks the headset to announce changes that need the user's agreement,
/// such as the reconnect that switching multipoint brings.
pub fn enable_alerts() -> Vec<u8> {
    vec![0x94, ALERT_FIXED_MESSAGE, 0x00]
}

pub fn answer_alert(message: u8, yes: bool) -> Vec<u8> {
    vec![0x98, ALERT_FIXED_MESSAGE, message, yes as u8]
}

// Table 2: these go out in a `DATA_MDR_NO2` frame.

/// `kind` comes from [`Capabilities::voice_guidance`].
pub fn get_voice_guidance(kind: u8) -> Vec<u8> {
    vec![0x46, kind]
}

/// Only the variant with a language of its own (0x02) repeats the language.
pub fn set_voice_guidance(kind: u8, on: bool, language: Option<u8>) -> Vec<u8> {
    let mut out = vec![0x48, kind, on_off(on)];
    if kind == 0x02 {
        out.push(language.unwrap_or(ENGLISH));
    }
    out
}

// ----- incoming ---------------------------------------------------------------

/// Parses a reply or a spontaneous notification. `kind` is the frame type,
/// which selects the command table. An empty result means the message is not
/// known at all; [`Event::Ignored`] marks one that is known and unused.
pub fn parse(kind: u8, p: &[u8]) -> Vec<Event> {
    if p.len() < 2 {
        return Vec::new();
    }
    if kind == TYPE_DATA_MDR_NO2 {
        return parse_table2(p);
    }
    let one = |event: Option<Event>| vec![event.unwrap_or(Event::Ignored)];
    let some = |events: Vec<Event>| if events.is_empty() { vec![Event::Ignored] } else { events };
    match p[0] {
        0x01 => one(parse_protocol_info(p)),
        0x05 => one(parse_device_info(p)),
        0x07 => one(parse_support_functions(p).map(Event::SupportFunctions)),
        0x13 | 0x15 => one(parse_codec(p)),
        0x21 => one(parse_auto_power_off_options(p)),
        0x23 | 0x25 => some(parse_battery(p)),
        0x27 | 0x29 => one(parse_auto_power_off(p)),
        0x51 => one(parse_eq_layout(p)),
        0x57 | 0x59 => one(parse_eq(p)),
        0x5B => one(parse_eq_frequencies(p)),
        0x67 | 0x69 => some(parse_noise(p)),
        0x99 => one(parse_alert(p)),
        0xA3 | 0xA5 => one(parse_playback(p)),
        0xA7 | 0xA9 => one(parse_volume(p)),
        0xD1 => one(parse_general_setting_info(p)),
        0xD7 | 0xD9 => one(parse_general_setting(p)),
        0xE7 | 0xE9 => one(parse_audio(p)),
        0xF7 | 0xF9 => one(parse_system(p)),
        // Known and not used: the remaining capability, status and
        // notification messages of each group (connection, common, power,
        // firmware update, LE Audio, equalizer, noise control, sensing,
        // optimizer, alerts, playback, spatial audio, logging, general
        // settings, audio, system).
        0x03 | 0x11 | 0x19 => vec![Event::Ignored],
        0x31..=0x3F | 0x41..=0x4D | 0x53 | 0x55 => vec![Event::Ignored],
        0x61 | 0x63 | 0x65 | 0x71..=0x7B | 0x81..=0x89 => vec![Event::Ignored],
        0x91 | 0x93 | 0x95 | 0xA1 | 0xB1..=0xB9 | 0xC9 => vec![Event::Ignored],
        0xD3 | 0xD5 | 0xE1 | 0xE3 | 0xE5 | 0xF1 | 0xF3 | 0xF5 | 0xFB | 0xFD => vec![Event::Ignored],
        _ => Vec::new(),
    }
}

fn parse_table2(p: &[u8]) -> Vec<Event> {
    match p[0] {
        0x07 => vec![parse_support_functions(p).map_or(Event::Ignored, Event::SupportFunctions2)],
        // [cmd, variant, onOff, language]
        0x47 | 0x49 if p.len() >= 3 && p[1] <= 0x03 => {
            let mut events = vec![Event::Switch(Switch::VoiceGuidance, p[2] == on_off(true))];
            if let (0x02, Some(language)) = (p[1], p.get(3)) {
                events.push(Event::VoiceGuidanceLanguage(*language));
            }
            events
        }
        // Power, paired devices, voice guidance, safe listening, LE Audio,
        // party speakers and the second system group.
        0x21..=0x29 | 0x31..=0x3D | 0x41..=0x49 | 0x53..=0x5B => vec![Event::Ignored],
        0x61..=0x69 | 0x71..=0x79 | 0xF1..=0xFD => vec![Event::Ignored],
        _ => Vec::new(),
    }
}

/// A length byte followed by that many characters.
fn text(p: &[u8], at: usize) -> Option<(String, usize)> {
    let len = *p.get(at)? as usize;
    let bytes = p.get(at + 1..at + 1 + len)?;
    Some((String::from_utf8_lossy(bytes).trim().to_string(), at + 1 + len))
}

// [cmd, 0, version ×4, table1, table2] with 0 = supported
fn parse_protocol_info(p: &[u8]) -> Option<Event> {
    if p.len() < 8 || p[1] != 0x00 {
        return None;
    }
    Some(Event::ProtocolInfo { table2: p[7] == 0x00 })
}

fn parse_device_info(p: &[u8]) -> Option<Event> {
    match p[1] {
        DEVICE_MODEL_NAME => Some(Event::ModelName(text(p, 2)?.0)),
        DEVICE_FW_VERSION => Some(Event::Firmware(text(p, 2)?.0)),
        // [cmd, type, series, colour]
        DEVICE_SERIES_AND_COLOUR => Some(Event::ModelColour(*p.get(3)?)),
        _ => None,
    }
}

// [cmd, 0, count, (function, priority)…]
fn parse_support_functions(p: &[u8]) -> Option<Vec<u8>> {
    if p.len() < 3 || p[1] != 0x00 {
        return None;
    }
    let pairs = p.get(3..3 + p[2] as usize * 2)?;
    Some(pairs.chunks(2).map(|pair| pair[0]).collect())
}

fn parse_codec(p: &[u8]) -> Option<Event> {
    if p.len() < 3 || p[1] != COMMON_AUDIO_CODEC {
        return None;
    }
    codec_name(p[2]).map(Event::Codec)
}

/// An earbud or case that is not in use reports level 0 and not charging,
/// which would otherwise read as an empty battery.
fn presence(level: u8, charging: u8) -> Option<super::Battery> {
    (level != 0 || charging == 1).then(|| battery(level, charging))
}

// The variants "with threshold" (+8) append the warning level, unused here.
fn parse_battery(p: &[u8]) -> Vec<Event> {
    match (p[1], p.len()) {
        (0x00 | 0x08, 4..) => vec![Event::Battery(BatterySlot::Single, Some(battery(p[2], p[3])))],
        (0x01 | 0x09, 6..) => vec![
            Event::Battery(BatterySlot::Left, presence(p[2], p[3])),
            Event::Battery(BatterySlot::Right, presence(p[4], p[5])),
        ],
        (0x02 | 0x0A, 4..) => vec![Event::Battery(BatterySlot::Case, presence(p[2], p[3]))],
        _ => Vec::new(),
    }
}

// [cmd, variant, count, choice…]
fn parse_auto_power_off_options(p: &[u8]) -> Option<Event> {
    if !matches!(p[1], POWER_AUTO_OFF | POWER_AUTO_OFF_WEARING) {
        return None;
    }
    let count = *p.get(2)? as usize;
    Some(Event::AutoPowerOffOptions(p.get(3..3 + count)?.to_vec()))
}

// [cmd, variant, choice in effect, timed choice to return to]
fn parse_auto_power_off(p: &[u8]) -> Option<Event> {
    if p.len() < 3 || !matches!(p[1], POWER_AUTO_OFF | POWER_AUTO_OFF_WEARING) {
        return None;
    }
    Some(Event::AutoPowerOff(p[2]))
}

// [cmd, variant, values, steps, count, (presetId, nameLen, name…)…]
fn parse_eq_layout(p: &[u8]) -> Option<Event> {
    if p.len() < 5 || p[1] > 0x04 {
        return None;
    }
    let mut presets = Vec::new();
    let mut at = 5;
    for _ in 0..p[4] {
        presets.push(*p.get(at)?);
        at = text(p, at + 1)?.1;
    }
    Some(Event::EqLayout { values: p[2], steps: p[3], presets })
}

// [cmd, variant, preset, count, value…]
fn parse_eq(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != EQ_PRESET {
        return None;
    }
    let values = p.get(4..4 + p[3] as usize)?.to_vec();
    Some(Event::EqRaw { preset: p[2], values })
}

// [cmd, variant, count, (unit, value ×2)…] with unit 1 = Hz, 2 = kHz
fn parse_eq_frequencies(p: &[u8]) -> Option<Event> {
    if p.len() < 3 || p[1] != EQ_PRESET {
        return None;
    }
    let bands = p.get(3..3 + p[2] as usize * 3)?;
    Some(Event::EqFrequencies(
        bands
            .chunks(3)
            .filter_map(|band| {
                let value = u16::from_be_bytes([band[1], band[2]]) as u32;
                match band[0] {
                    0x01 => Some(value),
                    0x02 => Some(value * 1000),
                    _ => None,
                }
            })
            .collect(),
    ))
}

fn parse_noise(p: &[u8]) -> Vec<Event> {
    if p[1] == NOISE_BUTTON_TOGGLE {
        return Vec::new();
    }
    match NoiseLayout::of(p[1]).and_then(|layout| layout.read(p)) {
        // The message itself is kept too: it is the starting point for the
        // next change.
        Some(state) => vec![Event::Noise(state), Event::NoiseRaw(p.to_vec())],
        None => Vec::new(),
    }
}

// [cmd, type, message, action]
fn parse_alert(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != ALERT_FIXED_MESSAGE {
        return None;
    }
    Some(Event::Alert { message: p[2], wants_choice: p[3] == ALERT_POSITIVE_NEGATIVE })
}

// [cmd, type, availability, playbackStatus, musicOrCall]
fn parse_playback(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] > 0x03 {
        return None;
    }
    Some(Event::Playback(match p[3] {
        0x01 => Playback::Playing,
        0x02 => Playback::Paused,
        0x03 => Playback::Stopped,
        _ => Playback::Unknown,
    }))
}

// [cmd, type, value]; the other types carry track names and call volume.
fn parse_volume(p: &[u8]) -> Option<Event> {
    if p.len() < 3 || p[1] != PLAY_MUSIC_VOLUME {
        return None;
    }
    Some(Event::Volume(p[2].min(MAX_VOLUME)))
}

// [cmd, slot, settingType, stringFormat, nameLen, name…, summaryLen, summary…]
fn parse_general_setting_info(p: &[u8]) -> Option<Event> {
    let (name, _) = text(p, 4)?;
    Some(Event::GeneralSettingInfo { slot: p[1], name, boolean: p[2] == GS_BOOLEAN })
}

// [cmd, slot, settingType, value]
fn parse_general_setting(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[2] != GS_BOOLEAN {
        return None;
    }
    Some(Event::GeneralSetting { slot: p[1], on: p[3] == on_off(true) })
}

// [cmd, type, value]
fn parse_audio(p: &[u8]) -> Option<Event> {
    if p.len() < 3 || p[1] != AUDIO_UPSCALING {
        return None;
    }
    Some(Event::Switch(Switch::Upscaling, p[2] == 0x01))
}

// [cmd, type, onOff, …]
fn parse_system(p: &[u8]) -> Option<Event> {
    let on = *p.get(2)? == on_off(true);
    match p[1] {
        SYSTEM_PLAYBACK_CONTROL_BY_WEARING => Some(Event::Switch(Switch::PauseWhenRemoved, on)),
        SYSTEM_SMART_TALKING_TYPE1 | SYSTEM_SMART_TALKING_TYPE2 => Some(Event::Switch(Switch::SpeakToChat, on)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::framing::{unpack, TYPE_ACK, TYPE_DATA_MDR};
    use super::super::Battery;
    use super::*;

    const T1: u8 = TYPE_DATA_MDR;
    const T2: u8 = TYPE_DATA_MDR_NO2;

    fn noise(mode: NoiseMode, level: u8, voice: bool) -> NoiseState {
        NoiseState { mode, ambient_level: level, focus_on_voice: voice }
    }

    #[test]
    fn works_out_what_an_xm5_style_headset_offers() {
        let caps = Capabilities::resolve(&[0x6B, 0x20, 0x57, 0x23, 0xA1, 0xE2, 0x24, 0xFC, 0xF1, 0xD1], &[0x41]);
        assert_eq!(caps.noise.map(|n| n.kind), Some(0x17));
        assert_eq!(caps.battery_inquiries, [0x00]);
        assert_eq!(caps.eq, Some(0x04));
        assert!(caps.power_off && caps.playback && caps.upscaling && caps.pause_when_removed);
        assert_eq!(caps.auto_power_off, Some(0x04));
        assert_eq!(caps.speak_to_chat, Some(0x0C));
        assert_eq!(caps.general_settings, [0xD1]);
        assert_eq!(caps.voice_guidance, Some(0x01));
    }

    #[test]
    fn earbuds_get_threshold_batteries_and_no_single_battery() {
        let caps = Capabilities::resolve(&[0x6D, 0x29, 0x2A], &[]);
        assert_eq!(caps.noise.map(|n| n.kind), Some(0x19));
        assert_eq!(caps.battery_inquiries, [0x09, 0x0A]);
        assert_eq!(caps.eq, None);
        assert_eq!(caps.voice_guidance, None);
    }

    #[test]
    fn a_headset_that_lists_nothing_is_treated_as_the_common_kind() {
        let caps = Capabilities::resolve(&[], &[]);
        assert_eq!(caps.noise.map(|n| n.kind), Some(0x17));
        assert_eq!(caps.battery_inquiries, [0x00]);
        assert!(!caps.power_off && !caps.playback);
    }

    #[test]
    fn each_noise_variant_offers_the_modes_it_has() {
        use NoiseMode::*;
        let modes = |kind| NoiseLayout::of(kind).unwrap().modes();
        assert_eq!(modes(0x01), [NoiseCancelling, Off]);
        assert_eq!(modes(0x11), [NoiseCancelling, Ambient, Off]);
        assert_eq!(modes(0x12), [NoiseCancelling, Wind, Ambient, Off]);
        assert_eq!(modes(0x16), [NoiseCancelling, Wind, Ambient, Off]);
        assert_eq!(modes(0x17), [NoiseCancelling, Ambient, Off]);
        assert_eq!(modes(0x19), [NoiseCancelling, Ambient, Off]);
        assert_eq!(modes(0x21), [Ambient, Off]);
        assert_eq!(modes(0x22), [Ambient, Off]);
        assert!(NoiseLayout::of(0x30).is_none());
        let levels = |kind| NoiseLayout::of(kind).unwrap().has_level();
        assert!(levels(0x17) && levels(0x22) && !levels(0x01) && !levels(0x11) && !levels(0x21));
    }

    #[test]
    fn noise_commands_follow_each_variant_layout() {
        let write = |kind, state| NoiseLayout::of(kind).unwrap().write(None, state);
        let ambient = noise(NoiseMode::Ambient, 12, true);
        let cancelling = noise(NoiseMode::NoiseCancelling, 12, false);
        // XM5: [cmd, type, changed, effect, mode, voice, level]
        assert_eq!(write(0x17, ambient), [0x68, 0x17, 0x01, 0x01, 0x01, 0x01, 12]);
        assert_eq!(write(0x17, cancelling), [0x68, 0x17, 0x01, 0x01, 0x00, 0x00, 0]);
        assert_eq!(write(0x19, ambient), [0x68, 0x19, 0x01, 0x01, 0x01, 0x01, 12, 0x00, 0x00]);
        // Ambient-only: [cmd, type, changed, effect, voice, level]
        assert_eq!(write(0x22, ambient), [0x68, 0x22, 0x01, 0x01, 0x01, 12]);
        assert_eq!(write(0x21, ambient), [0x68, 0x21, 0x01, 0x01, 0x01, 0x01]);
        // No mode byte: the canceller being on or off says which mode it is.
        assert_eq!(write(0x13, cancelling), [0x68, 0x13, 0x01, 0x01, 0x01, 0x00, 0]);
        assert_eq!(write(0x13, ambient), [0x68, 0x13, 0x01, 0x01, 0x00, 0x01, 12]);
        assert_eq!(write(0x11, ambient), [0x68, 0x11, 0x01, 0x01, 0x00, 0x01, 0x01]);
        assert_eq!(write(0x01, cancelling), [0x68, 0x01, 0x01, 0x01, 0x01]);
        // Wind noise reduction is the single-microphone flavour.
        assert_eq!(write(0x16, noise(NoiseMode::Wind, 5, false)), [0x68, 0x16, 0x01, 0x01, 0x00, 0x01, 0x00, 0]);
        assert_eq!(write(0x16, cancelling), [0x68, 0x16, 0x01, 0x01, 0x00, 0x02, 0x00, 0]);
        assert_eq!(write(0x14, noise(NoiseMode::Wind, 5, false)), [0x68, 0x14, 0x01, 0x01, 0x01, 0x00, 0]);
    }

    #[test]
    fn every_noise_variant_reads_back_what_it_wrote() {
        for (_, kind) in NOISE_FUNCTIONS {
            let layout = NoiseLayout::of(kind).unwrap();
            for mode in layout.modes() {
                let level = if layout.has_level() && mode == NoiseMode::Ambient { 7 } else { 0 };
                let voice = layout.has_voice_focus() && mode == NoiseMode::Ambient;
                let state = noise(mode, level, voice);
                let mut message = layout.write(None, state);
                message[0] = 0x67;
                assert_eq!(layout.read(&message), Some(state), "variant {kind:#04x}, {mode:?}");
            }
        }
    }

    #[test]
    fn a_noise_change_keeps_the_fields_this_app_does_not_control() {
        let layout = NoiseLayout::of(0x19).unwrap();
        // Ambient at 20 with automatic ambient sound on and high sensitivity.
        let last = [0x69, 0x19, 0x01, 0x01, 0x01, 0x00, 20, 0x01, 0x01];
        assert_eq!(
            layout.write(Some(&last), noise(NoiseMode::NoiseCancelling, 20, false)),
            [0x68, 0x19, 0x01, 0x01, 0x00, 0x00, 20, 0x01, 0x01]
        );
        // Switching off touches nothing but the effect byte.
        assert_eq!(
            layout.write(Some(&last), noise(NoiseMode::Off, 3, true)),
            [0x68, 0x19, 0x01, 0x00, 0x01, 0x00, 20, 0x01, 0x01]
        );
        // A message of another variant is not a starting point.
        assert_eq!(
            NoiseLayout::of(0x17).unwrap().write(Some(&last), noise(NoiseMode::Ambient, 4, false)),
            [0x68, 0x17, 0x01, 0x01, 0x01, 0x00, 4]
        );
    }

    #[test]
    fn noise_replies_yield_the_state_and_the_message() {
        let message = [0x69, 0x17, 0x01, 0x01, 0x01, 0x00, 15];
        assert_eq!(
            parse(T1, &message),
            [Event::Noise(noise(NoiseMode::Ambient, 15, false)), Event::NoiseRaw(message.to_vec())]
        );
        assert_eq!(
            parse(T1, &[0x67, 0x17, 0x01, 0x00, 0x01, 0x00, 15])[0],
            Event::Noise(noise(NoiseMode::Off, 15, false))
        );
        // Which mode the button cycles through: known, not used.
        assert_eq!(parse(T1, &[0x67, 0x30, 0x01]), [Event::Ignored]);
    }

    #[test]
    fn on_off_settings_are_inverted() {
        assert_eq!(set_pause_when_removed(true), [0xF8, 0x01, 0x00]);
        assert_eq!(set_speak_to_chat(0x0C, true), [0xF8, 0x0C, 0x00, 0x01]);
        assert_eq!(set_general_setting(0xD2, false), [0xD8, 0xD2, 0x00, 0x01]);
        assert_eq!(set_voice_guidance(0x01, true, None), [0x48, 0x01, 0x00]);
        assert_eq!(set_voice_guidance(0x02, false, Some(0x0B)), [0x48, 0x02, 0x01, 0x0B]);
        // DSEE is a mode instead: 1 = automatic.
        assert_eq!(set_upscaling(true), [0xE8, 0x01, 0x01]);
        assert_eq!(parse(T1, &[0xF7, 0x0C, 0x01, 0x01]), [Event::Switch(Switch::SpeakToChat, false)]);
        assert_eq!(parse(T1, &[0xF9, 0x01, 0x00]), [Event::Switch(Switch::PauseWhenRemoved, true)]);
        assert_eq!(parse(T1, &[0xD7, 0xD2, 0x00, 0x00]), [Event::GeneralSetting { slot: 0xD2, on: true }]);
        assert_eq!(parse(T1, &[0xE7, 0x01, 0x00]), [Event::Switch(Switch::Upscaling, false)]);
        assert_eq!(
            parse(T2, &[0x47, 0x02, 0x00, 0x0B]),
            [Event::Switch(Switch::VoiceGuidance, true), Event::VoiceGuidanceLanguage(0x0B)]
        );
    }

    #[test]
    fn builds_the_playback_and_power_commands() {
        assert_eq!(set_volume(99), [0xA8, 0x20, 30]);
        assert_eq!(transport(Transport::Next), [0xA4, 0x01, 0x00, 0x02]);
        assert_eq!(power_off(), [0x24, 0x03, 0x01]);
        // "When taken off" and "never" keep five minutes as the timed choice.
        assert_eq!(set_auto_power_off(0x05, 0x10), [0x28, 0x05, 0x10, 0x00]);
        assert_eq!(set_auto_power_off(0x04, 0x02), [0x28, 0x04, 0x02, 0x02]);
        assert_eq!(answer_alert(0x07, true), [0x98, 0x00, 0x07, 0x01]);
    }

    #[test]
    fn earbuds_not_in_use_have_no_battery_reading() {
        let b = |level, charging| Some(Battery { level, charging });
        assert_eq!(
            parse(T1, &[0x23, 0x09, 0, 0, 100, 0, 20, 20]),
            [Event::Battery(BatterySlot::Left, None), Event::Battery(BatterySlot::Right, b(100, false))]
        );
        assert_eq!(parse(T1, &[0x25, 0x0A, 53, 1, 30]), [Event::Battery(BatterySlot::Case, b(53, true))]);
        assert_eq!(parse(T1, &[0x23, 0x00, 70, 0]), [Event::Battery(BatterySlot::Single, b(70, false))]);
        // A headset at 0% is empty, not absent.
        assert_eq!(parse(T1, &[0x23, 0x00, 0, 0]), [Event::Battery(BatterySlot::Single, b(0, false))]);
    }

    #[test]
    fn equalizer_values_follow_the_headsets_own_scale() {
        // Ten bands of 13 positions: −6…+6.
        let ten = EqLayout { values: 10, steps: 13, frequencies: 10 };
        let state = ten.decode(0xA0, &[6, 6, 6, 6, 6, 6, 6, 6, 6, 12]);
        assert_eq!(state.clear_bass, None);
        assert_eq!(state.bands, [0, 0, 0, 0, 0, 0, 0, 0, 0, 6]);
        assert_eq!(state.range, 6);
        assert_eq!(ten.encode(0xA0, None, &[0; 10]).unwrap()[..5], [0x58, 0x00, 0xA0, 0x0A, 6]);
        // Nothing said: six values are Clear Bass and five bands at −10…+10.
        let unknown = EqLayout::default();
        let state = unknown.decode(0xA0, &[20, 11, 10, 13, 9, 12]);
        assert_eq!((state.clear_bass, state.bands.as_slice(), state.range), (Some(10), &[1, 0, 3, -1, 2][..], 10));
        assert_eq!(
            unknown.encode(0xA0, Some(2), &[0, 1, 2, 3, 4]).unwrap(),
            [0x58, 0x00, 0xA0, 0x06, 12, 10, 11, 12, 13, 14]
        );
        // Five frequencies and six values: the first value is Clear Bass.
        let six = EqLayout { values: 6, steps: 21, frequencies: 5 };
        assert_eq!(six.decode(0x00, &[10; 6]).clear_bass, Some(0));
        // Out-of-range values are pulled in rather than wrapped.
        assert_eq!(ten.encode(0xA0, None, &[99, -99, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap()[4..6], [12, 0]);
        assert!(ten.encode(0xA0, None, &[]).is_none());
    }

    #[test]
    fn ignores_truncated_payloads_and_reports_unknown_ones_as_such() {
        assert!(parse(T1, &[0x67]).is_empty());
        assert_eq!(parse(T1, &[0x23, 0x09, 50]), [Event::Ignored]);
        assert_eq!(parse(T1, &[0x69, 0x17, 0x01]), [Event::Ignored]);
        assert_eq!(parse(T1, &[0x51, 0x00, 0x0A, 0x0D, 0x03, 0x00]), [Event::Ignored]);
        assert!(parse(T1, &[0x0F, 0x00]).is_empty());
        assert!(parse(T2, &[0x0F, 0x00]).is_empty());
    }

    // ----- real headsets ---------------------------------------------------------

    /// Every event a recorded headset produced, in order.
    fn replay(recording: &str) -> Vec<Event> {
        let mut events = Vec::new();
        for line in recording.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let raw: Vec<u8> =
                (0..line.len()).step_by(2).map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap()).collect();
            let frame = unpack(&raw).unwrap_or_else(|| panic!("a recorded frame did not unpack: {line}"));
            if frame.kind == TYPE_ACK {
                continue;
            }
            let parsed = parse(frame.kind, &frame.payload);
            assert!(!parsed.is_empty(), "a recorded message is not known: {:02x?}", frame.payload);
            events.extend(parsed);
        }
        events
    }

    fn functions(events: &[Event]) -> (Vec<u8>, Vec<u8>) {
        let table1 = events.iter().find_map(|e| match e {
            Event::SupportFunctions(list) => Some(list.clone()),
            _ => None,
        });
        let table2 = events.iter().find_map(|e| match e {
            Event::SupportFunctions2(list) => Some(list.clone()),
            _ => None,
        });
        (table1.expect("no table 1 function list"), table2.expect("no table 2 function list"))
    }

    const WF_1000XM5: &str = include_str!("fixtures/wf-1000xm5-6.1.0.txt");
    const LINKBUDS_CLIP: &str = include_str!("fixtures/wf-lc900-2.0.3.txt");
    const LINKBUDS_CLIP_EQ: &str = include_str!("fixtures/wf-lc900-2.0.3-equalizer.txt");
    const LINKBUDS_CLIP_LISTENING: &str = include_str!("fixtures/wf-lc900-2.0.3-listening.txt");

    #[test]
    fn understands_everything_the_recorded_headsets_say() {
        for (name, recording, least) in [
            ("WF-1000XM5", WF_1000XM5, 18),
            ("LinkBuds Clip", LINKBUDS_CLIP, 14),
            ("LinkBuds Clip, equalizer", LINKBUDS_CLIP_EQ, 14),
            ("LinkBuds Clip, listening modes", LINKBUDS_CLIP_LISTENING, 14),
        ] {
            let events = replay(recording);
            let used = events.iter().filter(|e| **e != Event::Ignored).count();
            assert!(used >= least, "{name}: only {used} recorded messages were put to use");
        }
    }

    #[test]
    fn reads_a_real_wf_1000xm5() {
        let events = replay(WF_1000XM5);
        let has = |event: Event| events.contains(&event);
        assert!(has(Event::ProtocolInfo { table2: true }));
        assert!(has(Event::ModelName("WF-1000XM5".into())));
        assert!(has(Event::Firmware("6.1.0".into())));
        assert!(has(Event::ModelColour(0x03)));
        assert!(has(Event::Codec("AAC")));

        let (table1, table2) = functions(&events);
        let caps = Capabilities::resolve(&table1, &table2);
        assert_eq!(caps.noise.map(|n| n.kind), Some(0x17));
        // Earbuds: left and right, and the case, each with a warning level.
        assert_eq!(caps.battery_inquiries, [0x09, 0x0A]);
        assert_eq!(caps.eq, Some(0x00));
        assert!(caps.power_off && caps.playback && caps.codec && caps.upscaling && caps.pause_when_removed);
        assert_eq!(caps.auto_power_off, Some(0x05));
        assert_eq!(caps.speak_to_chat, Some(0x0C));
        assert_eq!(caps.general_settings, [0xD1, 0xD2]);
        assert_eq!(caps.voice_guidance, Some(0x01));

        // Ambient sound selected at level 20, switched off for now.
        assert!(has(Event::Noise(noise(NoiseMode::Off, 20, false))));
        // The left earbud was in its case; the right one and the case report.
        assert!(has(Event::Battery(BatterySlot::Left, None)));
        assert!(has(Event::Battery(BatterySlot::Right, Some(Battery { level: 100, charging: false }))));
        assert!(has(Event::Battery(BatterySlot::Case, Some(Battery { level: 53, charging: false }))));
        assert!(has(Event::EqRaw { preset: 0xA0, values: vec![20, 11, 10, 13, 9, 12] }));
        assert!(has(Event::Volume(12)));
        assert!(has(Event::Playback(Playback::Paused)));
        assert!(has(Event::AutoPowerOff(0x10)));
        assert!(has(Event::Switch(Switch::SpeakToChat, false)));
        assert!(has(Event::Switch(Switch::PauseWhenRemoved, true)));
        assert!(has(Event::Switch(Switch::Upscaling, true)));
        assert!(has(Event::Switch(Switch::VoiceGuidance, true)));
        assert!(has(Event::GeneralSettingInfo { slot: 0xD1, name: "SIDETONE_SETTING".into(), boolean: true }));
        assert!(has(Event::GeneralSettingInfo { slot: 0xD2, name: "MULTIPOINT_SETTING".into(), boolean: true }));
        assert!(has(Event::GeneralSetting { slot: 0xD1, on: false }));
        assert!(has(Event::GeneralSetting { slot: 0xD2, on: true }));
    }

    #[test]
    fn reads_real_linkbuds_clip_which_have_no_noise_control() {
        let events = replay(LINKBUDS_CLIP_EQ);
        let has = |event: Event| events.contains(&event);
        assert!(has(Event::ModelName("LinkBuds Clip".into())));
        assert!(has(Event::Firmware("2.0.3".into())));

        let (table1, table2) = functions(&events);
        let caps = Capabilities::resolve(&table1, &table2);
        assert_eq!(caps.noise, None);
        assert!(!events.iter().any(|e| matches!(e, Event::Noise(_))));
        assert_eq!(caps.battery_inquiries, [0x09, 0x0A]);
        assert_eq!(caps.eq, Some(0x00));
        assert!(caps.power_off && caps.playback && caps.upscaling);
        assert_eq!(caps.auto_power_off, None);
        assert_eq!(caps.speak_to_chat, None);
        assert_eq!(caps.general_settings, [0xD2, 0xD3]);
        assert_eq!(caps.voice_guidance, Some(0x01));

        // Ten bands of −6…+6 and the presets this model has.
        assert!(has(Event::EqLayout {
            values: 10,
            steps: 13,
            presets: vec![0x00, 0x30, 0x31, 0x32, 0x33, 0xA0, 0xA1, 0xA2]
        }));
        assert!(has(Event::EqRaw { preset: 0x00, values: vec![6; 10] }));
        assert!(has(Event::Volume(30)));
        assert!(has(Event::GeneralSettingInfo { slot: 0xD2, name: "MULTIPOINT_SETTING".into(), boolean: true }));
        // Tap sensitivity is a list of choices, not a switch.
        assert!(has(Event::GeneralSettingInfo {
            slot: 0xD3,
            name: "TAP_SENSITIVITY_SETTING".into(),
            boolean: false
        }));
        assert!(!events.iter().any(|e| matches!(e, Event::GeneralSetting { slot: 0xD3, .. })));
    }
}
