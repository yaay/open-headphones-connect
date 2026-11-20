//! The first-generation protocol, as spoken by the WH-1000XM3 (and the XM2,
//! XM4, WF-1000XM3 and others of that era).
//!
//! The noise-control command goes back to Plutoberth/SonyHeadphonesClient.
//! Everything else here (capability discovery, playback, DSEE, the general
//! settings, automatic power-off, voice guidance) follows the message tables
//! of mos9527/SonyHeadphonesClient (`libmdr`, MIT), whose maintainers verified
//! them on a WH-1000XM3 with firmware 4.5.2. A recording of that session is
//! replayed by the tests at the bottom of this file.
//!
//! Models with only noise cancelling or only ambient sound, Speak-to-Chat and
//! pausing when taken off (WH-1000XM4) follow the same tables but have not
//! been tried on a real headset.

use super::framing::TYPE_DATA_MDR_NO2;
use super::{
    battery, codec_name, BatterySlot, EqState, Event, NoiseCapability, NoiseMode, NoiseState,
    Playback, Switch, Transport,
};

pub const SERVICE_UUID: u128 = 0x96CC203E_5068_46AD_B32D_E316F5E069BA;

/// Ambient levels start at 1; the headset reports 0 only outside ambient mode.
pub const MIN_AMBIENT_LEVEL: u8 = 1;
/// Used until the headset has said how many levels it really has.
pub const DEFAULT_MAX_AMBIENT_LEVEL: u8 = 20;
pub const MAX_VOLUME: u8 = 30;

/// Entries of the list a headset returns when asked what it can do.
pub mod function {
    pub const BATTERY_LEVEL: u8 = 0x11;
    pub const LEFT_RIGHT_BATTERY_LEVEL: u8 = 0x15;
    pub const CRADLE_BATTERY_LEVEL: u8 = 0x18;
    pub const POWER_OFF: u8 = 0x21;
    pub const VOICE_GUIDANCE: u8 = 0x39;
    pub const PRESET_EQ: u8 = 0x51;
    pub const PRESET_EQ_NONCUSTOMIZABLE: u8 = 0x53;
    pub const NOISE_CANCELLING: u8 = 0x61;
    pub const NOISE_CANCELLING_AND_AMBIENT_SOUND_MODE: u8 = 0x62;
    pub const AMBIENT_SOUND_MODE: u8 = 0x63;
    pub const PLAYBACK_CONTROLLER: u8 = 0xA1;
    pub const GENERAL_SETTINGS: [u8; 3] = [0xD1, 0xD2, 0xD3];
    pub const UPSCALING: u8 = 0xE2;
    pub const CONTROL_BY_WEARING: u8 = 0xF3;
    pub const AUTO_POWER_OFF: u8 = 0xF4;
    pub const SMART_TALKING_MODE: u8 = 0xF5;
}

/// Which of the three noise-control messages a headset uses
/// (`NcAsmInquiredType`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NoiseKind {
    CancellingOnly = 0x01,
    /// Noise cancelling and ambient sound: the WH-1000X line.
    #[default]
    Both = 0x02,
    AmbientOnly = 0x03,
}

impl NoiseKind {
    /// The kind a headset uses, from its function list.
    pub fn for_functions(functions: &[u8]) -> Option<NoiseKind> {
        if functions.contains(&function::NOISE_CANCELLING_AND_AMBIENT_SOUND_MODE) {
            Some(NoiseKind::Both)
        } else if functions.contains(&function::NOISE_CANCELLING) {
            Some(NoiseKind::CancellingOnly)
        } else if functions.contains(&function::AMBIENT_SOUND_MODE) {
            Some(NoiseKind::AmbientOnly)
        } else {
            None
        }
    }
}

const NC_ONLY: u8 = NoiseKind::CancellingOnly as u8;
const NC_AND_ASM: u8 = NoiseKind::Both as u8;
const ASM_ONLY: u8 = NoiseKind::AmbientOnly as u8;
// NcAsmEffect
const EFFECT_OFF: u8 = 0x00;
const EFFECT_ADJUSTMENT_COMPLETION: u8 = 0x11;
// NcAsmSettingType
const NC_SETTING_ON_OFF: u8 = 0x00;
const NC_SETTING_DUAL_SINGLE_OFF: u8 = 0x02;
// NcDualSingleValue
const NC_OFF: u8 = 0x00;
const NC_SINGLE: u8 = 0x01; // wind noise reduction
const NC_DUAL: u8 = 0x02; // full noise cancelling
// AsmSettingType
const ASM_ON_OFF: u8 = 0x00;
const ASM_LEVEL_ADJUSTMENT: u8 = 0x01;
const ASM_ID_VOICE: u8 = 0x01;

const EQ_INQUIRY: u8 = 0x01;
const PLAYBACK_CONTROLLER: u8 = 0x01;
const PLAYBACK_VOLUME: u8 = 0x20;
const AUDIO_UPSCALING: u8 = 0x02;
const SYSTEM_CONTROL_BY_WEARING: u8 = 0x03;
const SYSTEM_AUTO_POWER_OFF: u8 = 0x04;
const SYSTEM_SMART_TALKING: u8 = 0x05;
const GS_BOOLEAN: u8 = 0x01;
const ALERT_FIXED_MESSAGE: u8 = 0x01;
const ALERT_POSITIVE_NEGATIVE: u8 = 0x01;
const VOICE_GUIDANCE_SETTING: u8 = 0x01;
const VOICE_GUIDANCE_ON_OFF: u8 = 0x01;

// ----- startup ----------------------------------------------------------------

pub fn get_protocol_info() -> Vec<u8> {
    vec![0x00, 0x00]
}

/// Asks which functions the headset has (see [`function`]).
pub fn get_support_functions() -> Vec<u8> {
    vec![0x06, 0x00]
}

pub fn get_model_name() -> Vec<u8> {
    vec![0x04, 0x01]
}

pub fn get_firmware_version() -> Vec<u8> {
    vec![0x04, 0x02]
}

pub fn get_series_and_colour() -> Vec<u8> {
    vec![0x04, 0x03]
}

pub fn get_battery(slot: BatterySlot) -> Vec<u8> {
    let kind = match slot {
        BatterySlot::Single => 0x00,
        BatterySlot::Left | BatterySlot::Right => 0x01,
        BatterySlot::Case => 0x02,
    };
    vec![0x10, kind]
}

pub fn get_codec() -> Vec<u8> {
    vec![0x18, 0x00]
}

pub fn power_off() -> Vec<u8> {
    vec![0x22, 0x00, 0x01]
}

// ----- noise control ----------------------------------------------------------

/// Asks how the headset's noise control is laid out: which modes it has and
/// how many ambient levels.
pub fn get_noise_capability(kind: NoiseKind) -> Vec<u8> {
    vec![0x60, kind as u8]
}

pub fn get_noise(kind: NoiseKind) -> Vec<u8> {
    vec![0x66, kind as u8]
}

/// `wind_mode` says whether the headset distinguishes full noise cancelling
/// from wind noise reduction (the XM3 does) or only has noise cancelling
/// on and off.
pub fn set_noise(kind: NoiseKind, state: NoiseState, wind_mode: bool) -> Vec<u8> {
    let on = state.mode != NoiseMode::Off;
    match kind {
        NoiseKind::Both => {}
        // [cmd, kind, settingType (on/off), value]
        NoiseKind::CancellingOnly => return vec![0x68, NC_ONLY, 0x00, on as u8],
        // [cmd, kind, effect, asmSettingType, asmId, asmLevel]
        NoiseKind::AmbientOnly => {
            return vec![
                0x68,
                ASM_ONLY,
                if on { EFFECT_ADJUSTMENT_COMPLETION } else { EFFECT_OFF },
                ASM_LEVEL_ADJUSTMENT,
                state.focus_on_voice as u8,
                state.ambient_level.max(MIN_AMBIENT_LEVEL),
            ];
        }
    }
    let cancelling = match (state.mode, wind_mode) {
        (NoiseMode::NoiseCancelling, true) => NC_DUAL,
        (NoiseMode::Wind, true) => NC_SINGLE,
        (NoiseMode::NoiseCancelling | NoiseMode::Wind, false) => 0x01,
        (NoiseMode::Ambient | NoiseMode::Off, _) => NC_OFF,
    };
    // The level only goes out with ambient sound; in the cancelling modes the
    // headset itself reports 0.
    let level = match state.mode {
        NoiseMode::Ambient | NoiseMode::Off => state.ambient_level.max(MIN_AMBIENT_LEVEL),
        NoiseMode::NoiseCancelling | NoiseMode::Wind => 0,
    };
    vec![
        0x68,
        NC_AND_ASM,
        if state.mode == NoiseMode::Off { EFFECT_OFF } else { EFFECT_ADJUSTMENT_COMPLETION },
        if wind_mode { NC_SETTING_DUAL_SINGLE_OFF } else { NC_SETTING_ON_OFF },
        cancelling,
        ASM_LEVEL_ADJUSTMENT,
        state.focus_on_voice as u8,
        level,
    ]
}

// ----- equalizer --------------------------------------------------------------

pub fn get_eq() -> Vec<u8> {
    vec![0x56, EQ_INQUIRY]
}

pub fn set_eq_preset(preset: u8) -> Vec<u8> {
    vec![0x58, EQ_INQUIRY, preset, 0x00]
}

/// Clear Bass plus five bands (400 Hz, 1 k, 2.5 k, 6.3 k, 16 k), each −10…+10.
/// `0xFF` leaves the active preset slot unchanged.
pub fn set_eq_bands(clear_bass: i8, bands: &[i8; 5]) -> Vec<u8> {
    let wire = |v: i8| (v.clamp(-10, 10) + 10) as u8;
    let mut out = vec![0x58, EQ_INQUIRY, 0xFF, 0x06, wire(clear_bass)];
    out.extend(bands.iter().map(|b| wire(*b)));
    out
}

// ----- playback ---------------------------------------------------------------

pub fn get_volume() -> Vec<u8> {
    vec![0xA6, PLAYBACK_CONTROLLER, PLAYBACK_VOLUME]
}

pub fn set_volume(volume: u8) -> Vec<u8> {
    vec![0xA8, PLAYBACK_CONTROLLER, PLAYBACK_VOLUME, volume.min(MAX_VOLUME)]
}

pub fn get_playback() -> Vec<u8> {
    vec![0xA2, PLAYBACK_CONTROLLER]
}

/// The headset passes this on to whatever is playing, as its own buttons do.
pub fn transport(control: Transport) -> Vec<u8> {
    let code = match control {
        Transport::Pause => 0x01,
        Transport::Next => 0x02,
        Transport::Previous => 0x03,
        Transport::Play => 0x07,
    };
    vec![0xA4, PLAYBACK_CONTROLLER, 0x00, code]
}

// ----- sound and system settings ------------------------------------------------

/// DSEE HX, Sony's upscaling of compressed audio.
pub fn get_upscaling() -> Vec<u8> {
    vec![0xE6, AUDIO_UPSCALING]
}

pub fn set_upscaling(on: bool) -> Vec<u8> {
    vec![0xE8, AUDIO_UPSCALING, 0x00, on as u8]
}

/// Which automatic power-off choices the headset offers.
pub fn get_auto_power_off_options() -> Vec<u8> {
    vec![0xF0, SYSTEM_AUTO_POWER_OFF]
}

pub fn get_auto_power_off() -> Vec<u8> {
    vec![0xF6, SYSTEM_AUTO_POWER_OFF]
}

/// `choice` is one of the ids from [`get_auto_power_off_options`]:
/// 0x00 5 min, 0x01 30 min, 0x02 1 h, 0x03 3 h, 0x10 when taken off, 0x11 never.
pub fn set_auto_power_off(choice: u8) -> Vec<u8> {
    vec![0xF8, SYSTEM_AUTO_POWER_OFF, 0x01, choice, 0x00]
}

/// Asks what one of the headset's "general settings" is. `slot` is one of
/// [`function::GENERAL_SETTINGS`]; the reply names it.
pub fn get_general_setting_info(slot: u8) -> Vec<u8> {
    vec![0xD0, slot, 0x00]
}

pub fn get_general_setting(slot: u8) -> Vec<u8> {
    vec![0xD6, slot]
}

pub fn set_general_setting(slot: u8, on: bool) -> Vec<u8> {
    vec![0xD8, slot, GS_BOOLEAN, on as u8]
}

/// Speak-to-Chat: pausing the music when the wearer starts talking.
pub fn get_speak_to_chat() -> Vec<u8> {
    vec![0xF6, SYSTEM_SMART_TALKING]
}

/// The 0x01 says this is the mode itself, not its try-out ("preview") mode.
pub fn set_speak_to_chat(on: bool) -> Vec<u8> {
    vec![0xF8, SYSTEM_SMART_TALKING, 0x01, on as u8]
}

/// Pausing when the headphones are taken off.
pub fn get_pause_when_removed() -> Vec<u8> {
    vec![0xF6, SYSTEM_CONTROL_BY_WEARING]
}

/// This one is written with the "extended" set command, as in libmdr.
pub fn set_pause_when_removed(on: bool) -> Vec<u8> {
    vec![0xFC, SYSTEM_CONTROL_BY_WEARING, 0x00, on as u8]
}

/// Answers a question the headset raised with [`Event::Alert`].
pub fn answer_alert(message: u8, yes: bool) -> Vec<u8> {
    vec![0x98, ALERT_FIXED_MESSAGE, message, yes as u8]
}

// Voice guidance lives in the second command table: the same command numbers
// mean something else there, so these travel in `TYPE_DATA_MDR_NO2` frames.

pub fn get_voice_guidance() -> Vec<u8> {
    vec![0x46, VOICE_GUIDANCE_SETTING, VOICE_GUIDANCE_ON_OFF]
}

pub fn set_voice_guidance(on: bool) -> Vec<u8> {
    vec![0x48, VOICE_GUIDANCE_SETTING, VOICE_GUIDANCE_ON_OFF, on as u8]
}

// ----- reading what the headset says --------------------------------------------

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
    match p[0] {
        0x01 => vec![Event::ProtocolInfo { table2: true }],
        0x05 => one(parse_device_info(p)),
        0x07 => one(parse_support_functions(p)),
        0x11 | 0x13 => parse_battery(p),
        0x19 | 0x1B => one(parse_codec(p)),
        0x57 | 0x59 => one(parse_eq(p)),
        0x61 => one(parse_noise_capability(p)),
        0x67 | 0x69 => one(parse_noise(p)),
        0x99 => one(parse_alert(p)),
        0xA3 | 0xA5 => one(parse_playback(p)),
        0xA7 | 0xA9 => one(parse_volume(p)),
        0xD1 => one(parse_general_setting_info(p)),
        0xD7 | 0xD9 => one(parse_general_setting(p)),
        0xE7 | 0xE9 => one(parse_upscaling(p)),
        0xF1 => one(parse_auto_power_off_options(p)),
        0xF7 | 0xF9 => one(parse_system(p)),
        // Known and not used: identity and update details, the DSEE and
        // optimizer indicators, capability and status replies for functions
        // handled above.
        0x03 | 0x15 | 0x17 | 0x1D | 0x25 | 0x27 | 0x29 | 0x2F | 0x35 | 0x37 => vec![Event::Ignored],
        0x41 | 0x43 | 0x45 | 0x47 | 0x49 => vec![Event::Ignored],
        0x51 | 0x53 | 0x55 | 0x5B | 0x63 | 0x65 | 0x71 => vec![Event::Ignored],
        0x81 | 0x83 | 0x85 | 0x87 | 0x89 | 0x91 | 0xA1 | 0xC9 => vec![Event::Ignored],
        0xD3 | 0xD5 | 0xE1 | 0xE3 | 0xE5 | 0xF3 | 0xF5 | 0xFB | 0xFD => vec![Event::Ignored],
        _ => Vec::new(),
    }
}

fn parse_table2(p: &[u8]) -> Vec<Event> {
    match p[0] {
        0x47 | 0x49 if p.len() >= 4 && p[1] == VOICE_GUIDANCE_SETTING && p[2] == VOICE_GUIDANCE_ON_OFF => {
            vec![Event::Switch(Switch::VoiceGuidance, p[3] == 1)]
        }
        0x31..=0x3D | 0x41..=0x49 => vec![Event::Ignored],
        _ => Vec::new(),
    }
}

// [cmd, 1 model | 2 firmware, len, text…] or [cmd, 3, series, colour]
fn parse_device_info(p: &[u8]) -> Option<Event> {
    if p.len() < 4 {
        return None;
    }
    if p[1] == 0x03 {
        return Some(Event::ModelColour(p[3]));
    }
    let len = (p[2] as usize).min(p.len() - 3);
    let text: String =
        p[3..3 + len].iter().filter(|b| b.is_ascii_graphic() || **b == b' ').map(|b| *b as char).collect();
    let text = text.trim().to_string();
    match p[1] {
        _ if text.is_empty() => None,
        0x01 => Some(Event::ModelName(text)),
        0x02 => Some(Event::Firmware(text)),
        _ => None,
    }
}

// [cmd, fixed, count, function…]
fn parse_support_functions(p: &[u8]) -> Option<Event> {
    let count = *p.get(2)? as usize;
    Some(Event::SupportFunctions(p.get(3..3 + count)?.to_vec()))
}

fn parse_battery(p: &[u8]) -> Vec<Event> {
    match (p[1], p.len()) {
        (0x00, 4..) => vec![Event::Battery(BatterySlot::Single, Some(battery(p[2], p[3])))],
        (0x01, 6..) => vec![
            Event::Battery(BatterySlot::Left, Some(battery(p[2], p[3]))),
            Event::Battery(BatterySlot::Right, Some(battery(p[4], p[5]))),
        ],
        (0x02, 4..) => vec![Event::Battery(BatterySlot::Case, Some(battery(p[2], p[3])))],
        _ => vec![Event::Ignored],
    }
}

fn parse_codec(p: &[u8]) -> Option<Event> {
    if p.len() < 3 || p[1] != 0x00 {
        return None;
    }
    codec_name(p[2]).map(Event::Codec)
}

fn parse_eq(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != EQ_INQUIRY {
        return None;
    }
    let preset = p[2];
    let count = p[3] as usize;
    if count == 6 && p.len() >= 10 {
        let user = |b: u8| b as i8 - 10;
        Some(Event::Eq(EqState {
            preset,
            clear_bass: Some(user(p[4])),
            bands: p[5..10].iter().map(|b| user(*b)).collect(),
            range: 10,
        }))
    } else {
        Some(Event::Eq(EqState { preset, clear_bass: None, bands: Vec::new(), range: 10 }))
    }
}

// Noise cancelling and ambient sound:
//   [cmd, kind, ncSettingType, ncSteps, asmSettingType, count, (asmId, levels)…]
// Ambient sound only:
//   [cmd, kind, asmSettingType, count, (asmId, levels)…]
// Noise cancelling only:
//   [cmd, kind, ncSettingType]
fn parse_noise_capability(p: &[u8]) -> Option<Event> {
    let (wind_mode, cancelling, ambient) = match p[1] {
        NC_AND_ASM => (*p.get(2)? == NC_SETTING_DUAL_SINGLE_OFF, true, p.get(4..)?),
        ASM_ONLY => (false, false, p.get(2..)?),
        NC_ONLY => (false, true, &[][..]),
        _ => return None,
    };
    // `ambient` is [asmSettingType, count, (asmId, levels)…]
    let kinds = match ambient {
        [_, count, rest @ ..] => rest.get(..*count as usize * 2)?,
        _ => &[][..],
    };
    let normal_levels = kinds.chunks(2).find(|k| k[0] != ASM_ID_VOICE).map(|k| k[1]);
    let voice_levels = kinds.chunks(2).find(|k| k[0] == ASM_ID_VOICE).map(|k| k[1]);
    let mut modes = Vec::new();
    if cancelling {
        modes.push(NoiseMode::NoiseCancelling);
    }
    if wind_mode {
        modes.push(NoiseMode::Wind);
    }
    if !kinds.is_empty() {
        modes.push(NoiseMode::Ambient);
    }
    modes.push(NoiseMode::Off);
    let max_ambient_level = if kinds.is_empty() || ambient.first() == Some(&ASM_ON_OFF) {
        // Ambient sound without a level, or none at all.
        0
    } else {
        normal_levels.or(voice_levels).filter(|l| *l > 0).unwrap_or(DEFAULT_MAX_AMBIENT_LEVEL)
    };
    Some(Event::NoiseCapability(NoiseCapability {
        modes,
        wind_mode,
        max_ambient_level,
        focus_on_voice: voice_levels.is_some(),
    }))
}

fn parse_noise(p: &[u8]) -> Option<Event> {
    match (p[1], p.len()) {
        (NC_AND_ASM, 8..) => {}
        // [cmd, kind, settingType, value]
        (NC_ONLY, 4..) => {
            let mode = if p[3] == 0 { NoiseMode::Off } else { NoiseMode::NoiseCancelling };
            return Some(Event::Noise(NoiseState { mode, ambient_level: 0, focus_on_voice: false }));
        }
        // [cmd, kind, effect, asmSettingType, asmId, asmLevel]
        (ASM_ONLY, 6..) => {
            let mode = if p[2] == EFFECT_OFF { NoiseMode::Off } else { NoiseMode::Ambient };
            return Some(Event::Noise(NoiseState {
                mode,
                ambient_level: p[5],
                focus_on_voice: p[4] == ASM_ID_VOICE,
            }));
        }
        _ => return None,
    }
    // [cmd, kind, effect, ncSettingType, ncValue, asmSettingType, asmId, asmLevel]
    let mode = if p[2] == EFFECT_OFF {
        NoiseMode::Off
    } else {
        match (p[3], p[4]) {
            (_, NC_DUAL) => NoiseMode::NoiseCancelling,
            (NC_SETTING_DUAL_SINGLE_OFF, NC_SINGLE) => NoiseMode::Wind,
            // Headsets without a wind mode report plain on/off: 1 = on.
            (_, NC_SINGLE) => NoiseMode::NoiseCancelling,
            _ => NoiseMode::Ambient,
        }
    };
    Some(Event::Noise(NoiseState { mode, ambient_level: p[7], focus_on_voice: p[6] == ASM_ID_VOICE }))
}

// [cmd, inquiry, messageType, actionType]
fn parse_alert(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != ALERT_FIXED_MESSAGE {
        return None;
    }
    Some(Event::Alert { message: p[2], wants_choice: p[3] == ALERT_POSITIVE_NEGATIVE })
}

// [cmd, inquiry, status, playbackStatus]
fn parse_playback(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != PLAYBACK_CONTROLLER {
        return None;
    }
    Some(Event::Playback(match p[3] {
        0x01 => Playback::Playing,
        0x02 => Playback::Paused,
        0x03 => Playback::Stopped,
        _ => Playback::Unknown,
    }))
}

// [cmd, inquiry, dataType, value]; the other data types are track names.
fn parse_volume(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != PLAYBACK_CONTROLLER || p[2] != PLAYBACK_VOLUME {
        return None;
    }
    Some(Event::Volume(p[3].min(MAX_VOLUME)))
}

// [cmd, slot, stringFormat, nameLen, name…, summaryLen, summary…, settingType, …]
fn parse_general_setting_info(p: &[u8]) -> Option<Event> {
    let name_len = *p.get(3)? as usize;
    let name = p.get(4..4 + name_len)?;
    let summary_len = *p.get(4 + name_len)? as usize;
    let setting_type = *p.get(5 + name_len + summary_len)?;
    Some(Event::GeneralSettingInfo {
        slot: p[1],
        name: String::from_utf8_lossy(name).into_owned(),
        boolean: setting_type == GS_BOOLEAN,
    })
}

// [cmd, slot, settingType, value]
fn parse_general_setting(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[2] != GS_BOOLEAN {
        return None;
    }
    Some(Event::GeneralSetting { slot: p[1], on: p[3] == 1 })
}

// [cmd, inquiry, settingType, value]
fn parse_upscaling(p: &[u8]) -> Option<Event> {
    if p.len() < 4 || p[1] != AUDIO_UPSCALING {
        return None;
    }
    Some(Event::Switch(Switch::Upscaling, p[3] != 0))
}

// [cmd, inquiry, count, choice…]
fn parse_auto_power_off_options(p: &[u8]) -> Option<Event> {
    if p[1] != SYSTEM_AUTO_POWER_OFF {
        return None;
    }
    let count = *p.get(2)? as usize;
    Some(Event::AutoPowerOffOptions(p.get(3..3 + count)?.to_vec()))
}

fn parse_system(p: &[u8]) -> Option<Event> {
    if p.len() < 4 {
        return None;
    }
    match p[1] {
        // [cmd, inquiry, parameterType, activeChoice, selectedTime]
        SYSTEM_AUTO_POWER_OFF => Some(Event::AutoPowerOff(p[3])),
        // [cmd, inquiry, settingType, value]
        SYSTEM_CONTROL_BY_WEARING => Some(Event::Switch(Switch::PauseWhenRemoved, p[3] == 1)),
        // The same, except that a notification may be about the try-out
        // ("preview") mode instead, marked with 0x02.
        SYSTEM_SMART_TALKING if !(p[0] == 0xF9 && p[2] == 0x02) => {
            Some(Event::Switch(Switch::SpeakToChat, p[3] == 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::framing::{unpack, TYPE_ACK, TYPE_DATA_MDR};
    use super::super::Battery;
    use super::*;

    const T1: u8 = TYPE_DATA_MDR;

    fn noise(mode: NoiseMode, level: u8, voice: bool) -> NoiseState {
        NoiseState { mode, ambient_level: level, focus_on_voice: voice }
    }

    #[test]
    fn noise_commands_match_the_xm3_layout() {
        assert_eq!(
            set_noise(NoiseKind::Both, noise(NoiseMode::NoiseCancelling, 12, false), true),
            [0x68, 0x02, 0x11, 0x02, 0x02, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            set_noise(NoiseKind::Both, noise(NoiseMode::Wind, 12, false), true),
            [0x68, 0x02, 0x11, 0x02, 0x01, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            set_noise(NoiseKind::Both, noise(NoiseMode::Ambient, 12, true), true),
            [0x68, 0x02, 0x11, 0x02, 0x00, 0x01, 0x01, 0x0C]
        );
        assert_eq!(
            set_noise(NoiseKind::Both, noise(NoiseMode::Off, 12, false), true),
            [0x68, 0x02, 0x00, 0x02, 0x00, 0x01, 0x00, 0x0C]
        );
    }

    #[test]
    fn headsets_without_a_wind_mode_get_plain_on_and_off() {
        assert_eq!(
            set_noise(NoiseKind::Both, noise(NoiseMode::NoiseCancelling, 5, false), false),
            [0x68, 0x02, 0x11, 0x00, 0x01, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            set_noise(NoiseKind::Both, noise(NoiseMode::Ambient, 5, false), false),
            [0x68, 0x02, 0x11, 0x00, 0x00, 0x01, 0x00, 0x05]
        );
    }

    #[test]
    fn ambient_level_zero_is_never_sent_in_ambient_mode() {
        // 0 is what the headset reports outside ambient mode, not a level.
        assert_eq!(set_noise(NoiseKind::Both, noise(NoiseMode::Ambient, 0, false), true)[7], 1);
        assert_eq!(set_noise(NoiseKind::Both, noise(NoiseMode::Ambient, 20, false), true)[7], 20);
    }

    #[test]
    fn noise_replies_round_trip() {
        for state in [
            noise(NoiseMode::NoiseCancelling, 0, false),
            noise(NoiseMode::Wind, 0, false),
            noise(NoiseMode::Ambient, 7, true),
            noise(NoiseMode::Ambient, 20, false),
            noise(NoiseMode::Off, 7, false),
        ] {
            let mut reply = set_noise(NoiseKind::Both, state, true);
            reply[0] = 0x69; // the headset echoes the same layout as a notification
            assert_eq!(parse(T1, &reply), [Event::Noise(state)]);
        }
    }

    #[test]
    fn parses_single_and_split_batteries() {
        let b = |level, charging| Some(Battery { level, charging });
        assert_eq!(parse(T1, &[0x11, 0x00, 80, 0]), [Event::Battery(BatterySlot::Single, b(80, false))]);
        assert_eq!(
            parse(T1, &[0x13, 0x01, 60, 1, 55, 0]),
            [Event::Battery(BatterySlot::Left, b(60, true)), Event::Battery(BatterySlot::Right, b(55, false))]
        );
        assert_eq!(parse(T1, &[0x11, 0x02, 100, 1]), [Event::Battery(BatterySlot::Case, b(100, true))]);
    }

    #[test]
    fn equalizer_bands_use_a_plus_ten_offset() {
        assert_eq!(set_eq_bands(3, &[-10, 0, 2, 10, -4]), [0x58, 0x01, 0xFF, 0x06, 13, 0, 10, 12, 20, 6]);
        assert_eq!(
            parse(T1, &[0x57, 0x01, 0x16, 0x06, 17, 10, 10, 10, 10, 10]),
            [Event::Eq(EqState { preset: 0x16, clear_bass: Some(7), bands: vec![0; 5], range: 10 })]
        );
        assert_eq!(set_eq_preset(0x16), [0x58, 0x01, 0x16, 0x00]);
    }

    #[test]
    fn builds_the_playback_and_settings_commands() {
        assert_eq!(set_volume(21), [0xA8, 0x01, 0x20, 21]);
        assert_eq!(set_volume(200), [0xA8, 0x01, 0x20, 30]);
        assert_eq!(transport(Transport::Play), [0xA4, 0x01, 0x00, 0x07]);
        assert_eq!(transport(Transport::Pause), [0xA4, 0x01, 0x00, 0x01]);
        assert_eq!(transport(Transport::Next), [0xA4, 0x01, 0x00, 0x02]);
        assert_eq!(transport(Transport::Previous), [0xA4, 0x01, 0x00, 0x03]);
        assert_eq!(set_upscaling(true), [0xE8, 0x02, 0x00, 0x01]);
        assert_eq!(set_auto_power_off(0x11), [0xF8, 0x04, 0x01, 0x11, 0x00]);
        assert_eq!(set_general_setting(0xD2, false), [0xD8, 0xD2, 0x01, 0x00]);
        assert_eq!(set_voice_guidance(true), [0x48, 0x01, 0x01, 0x01]);
        assert_eq!(answer_alert(0x01, false), [0x98, 0x01, 0x01, 0x00]);
    }

    #[test]
    fn the_same_command_number_means_different_things_in_each_table() {
        // 0x47 is a surround reply in table 1 and voice guidance in table 2.
        assert_eq!(parse(T1, &[0x47, 0x01, 0x01, 0x01]), [Event::Ignored]);
        assert_eq!(
            parse(TYPE_DATA_MDR_NO2, &[0x47, 0x01, 0x01, 0x01]),
            [Event::Switch(Switch::VoiceGuidance, true)]
        );
    }

    #[test]
    fn headsets_with_only_noise_cancelling_or_only_ambient_sound() {
        use NoiseMode::*;
        assert_eq!(NoiseKind::for_functions(&[0x11, 0x61]), Some(NoiseKind::CancellingOnly));
        assert_eq!(NoiseKind::for_functions(&[0x63]), Some(NoiseKind::AmbientOnly));
        assert_eq!(NoiseKind::for_functions(&[0x61, 0x62]), Some(NoiseKind::Both));
        assert_eq!(NoiseKind::for_functions(&[0x11]), None);

        // Noise cancelling only: on or off, nothing else.
        assert_eq!(get_noise(NoiseKind::CancellingOnly), [0x66, 0x01]);
        assert_eq!(
            set_noise(NoiseKind::CancellingOnly, noise(NoiseCancelling, 0, false), false),
            [0x68, 0x01, 0x00, 0x01]
        );
        assert_eq!(set_noise(NoiseKind::CancellingOnly, noise(Off, 0, false), false), [0x68, 0x01, 0x00, 0x00]);
        assert_eq!(parse(T1, &[0x67, 0x01, 0x00, 0x01]), [Event::Noise(noise(NoiseCancelling, 0, false))]);
        assert_eq!(
            parse(T1, &[0x61, 0x01, 0x00]),
            [Event::NoiseCapability(NoiseCapability {
                modes: vec![NoiseCancelling, Off],
                wind_mode: false,
                max_ambient_level: 0,
                focus_on_voice: false,
            })]
        );

        // Ambient sound only, here with 20 levels and a voice mode.
        assert_eq!(
            set_noise(NoiseKind::AmbientOnly, noise(Ambient, 9, true), false),
            [0x68, 0x03, 0x11, 0x01, 0x01, 9]
        );
        assert_eq!(
            parse(T1, &[0x69, 0x03, 0x11, 0x01, 0x01, 9]),
            [Event::Noise(noise(Ambient, 9, true))]
        );
        assert_eq!(parse(T1, &[0x67, 0x03, 0x00, 0x01, 0x00, 9]), [Event::Noise(noise(Off, 9, false))]);
        assert_eq!(
            parse(T1, &[0x61, 0x03, 0x01, 0x02, 0x00, 20, 0x01, 20]),
            [Event::NoiseCapability(NoiseCapability {
                modes: vec![Ambient, Off],
                wind_mode: false,
                max_ambient_level: 20,
                focus_on_voice: true,
            })]
        );
        // Ambient sound that is only on or off has no level to show.
        assert_eq!(
            parse(T1, &[0x61, 0x03, 0x00, 0x01, 0x00, 1]),
            [Event::NoiseCapability(NoiseCapability {
                modes: vec![Ambient, Off],
                wind_mode: false,
                max_ambient_level: 0,
                focus_on_voice: false,
            })]
        );
    }

    #[test]
    fn speak_to_chat_and_pausing_when_taken_off() {
        assert_eq!(set_speak_to_chat(true), [0xF8, 0x05, 0x01, 0x01]);
        assert_eq!(set_pause_when_removed(false), [0xFC, 0x03, 0x00, 0x00]);
        assert_eq!(parse(T1, &[0xF7, 0x05, 0x00, 0x01]), [Event::Switch(Switch::SpeakToChat, true)]);
        assert_eq!(parse(T1, &[0xF9, 0x05, 0x01, 0x00]), [Event::Switch(Switch::SpeakToChat, false)]);
        // A notification about the try-out mode is not the setting itself.
        assert_eq!(parse(T1, &[0xF9, 0x05, 0x02, 0x01]), [Event::Ignored]);
        assert_eq!(parse(T1, &[0xF7, 0x03, 0x00, 0x01]), [Event::Switch(Switch::PauseWhenRemoved, true)]);
        assert_eq!(parse(T1, &[0xF7, 0x04, 0x01, 0x11, 0x00]), [Event::AutoPowerOff(0x11)]);
    }

    #[test]
    fn reads_the_model_name_and_colour() {
        let mut name = vec![0x05, 0x01, 10];
        name.extend(b"WH-1000XM4");
        assert_eq!(parse(T1, &name), [Event::ModelName("WH-1000XM4".into())]);
        assert_eq!(parse(T1, &[0x05, 0x03, 0x30, 0x03]), [Event::ModelColour(0x03)]);
        assert_eq!(get_model_name(), [0x04, 0x01]);
        assert_eq!(get_series_and_colour(), [0x04, 0x03]);
    }

    #[test]
    fn ignores_truncated_payloads_and_reports_unknown_ones_as_such() {
        assert!(parse(T1, &[]).is_empty());
        assert_eq!(parse(T1, &[0x69, 0x02, 0x11]), [Event::Ignored]);
        assert!(parse(T1, &[0xB7, 0x00, 0x01]).is_empty());
    }

    // ----- a real WH-1000XM3 ---------------------------------------------------

    /// Every event the recorded headset produced, in order.
    fn replay() -> Vec<Event> {
        let recording = include_str!("fixtures/wh-1000xm3-4.5.2.txt");
        let mut events = Vec::new();
        let mut frames = 0;
        for line in recording.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let raw: Vec<u8> =
                (0..line.len()).step_by(2).map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap()).collect();
            let frame = unpack(&raw).unwrap_or_else(|| panic!("a recorded frame did not unpack: {line}"));
            frames += 1;
            if frame.kind == TYPE_ACK {
                continue;
            }
            let parsed = parse(frame.kind, &frame.payload);
            assert!(!parsed.is_empty(), "a recorded message is not known: {:02x?}", frame.payload);
            events.extend(parsed);
        }
        assert_eq!(frames, 187);
        events
    }

    #[test]
    fn understands_everything_a_real_xm3_says() {
        let events = replay();
        let used = events.iter().filter(|e| **e != Event::Ignored).count();
        assert!(used > 40, "only {used} recorded messages were put to use");
    }

    #[test]
    fn reads_the_xm3s_capabilities_from_the_recording() {
        let events = replay();
        let functions = events
            .iter()
            .find_map(|e| if let Event::SupportFunctions(f) = e { Some(f.clone()) } else { None })
            .expect("the function list");
        assert_eq!(functions.len(), 19);
        for wanted in [
            function::BATTERY_LEVEL,
            function::NOISE_CANCELLING_AND_AMBIENT_SOUND_MODE,
            function::PRESET_EQ,
            function::PLAYBACK_CONTROLLER,
            function::UPSCALING,
            function::AUTO_POWER_OFF,
            function::VOICE_GUIDANCE,
        ] {
            assert!(functions.contains(&wanted), "function {wanted:#04x} is missing");
        }
        // The XM3 cannot be switched off remotely, and says so.
        assert!(!functions.contains(&function::POWER_OFF));

        assert!(events.contains(&Event::NoiseCapability(NoiseCapability {
            modes: vec![NoiseMode::NoiseCancelling, NoiseMode::Wind, NoiseMode::Ambient, NoiseMode::Off],
            wind_mode: true,
            max_ambient_level: 20,
            focus_on_voice: true,
        })));
        assert!(events.contains(&Event::AutoPowerOffOptions(vec![0x00, 0x01, 0x02, 0x03, 0x11])));
        assert!(events.contains(&Event::GeneralSettingInfo {
            slot: 0xD2,
            name: "TOUCH_PANEL_SETTING".into(),
            boolean: true,
        }));
        assert!(events.contains(&Event::GeneralSettingInfo {
            slot: 0xD1,
            name: "ASSIGNABLE_KEY_SETTING".into(),
            boolean: false,
        }));
    }

    #[test]
    fn reads_the_xm3s_state_from_the_recording() {
        let events = replay();
        let first = |pick: fn(&Event) -> bool| events.iter().find(|e| pick(e)).cloned();

        assert_eq!(first(|e| matches!(e, Event::Firmware(_))), Some(Event::Firmware("4.5.2".into())));
        assert_eq!(
            first(|e| matches!(e, Event::Battery(..))),
            Some(Event::Battery(BatterySlot::Single, Some(Battery { level: 100, charging: false })))
        );
        assert_eq!(first(|e| matches!(e, Event::Codec(_))), Some(Event::Codec("LDAC")));
        assert_eq!(
            first(|e| matches!(e, Event::Noise(_))),
            Some(Event::Noise(noise(NoiseMode::NoiseCancelling, 0, false)))
        );
        assert_eq!(
            first(|e| matches!(e, Event::Eq(_))),
            Some(Event::Eq(EqState { preset: 0x00, clear_bass: Some(0), bands: vec![0; 5], range: 10 }))
        );
        assert_eq!(first(|e| matches!(e, Event::Volume(_))), Some(Event::Volume(21)));
        assert_eq!(
            first(|e| matches!(e, Event::Switch(Switch::Upscaling, _))),
            Some(Event::Switch(Switch::Upscaling, true))
        );
        assert_eq!(Switch::from_general_setting("TOUCH_PANEL_SETTING"), Some(Switch::TouchPanel));
        assert_eq!(first(|e| matches!(e, Event::AutoPowerOff(_))), Some(Event::AutoPowerOff(0x00)));
        assert_eq!(
            first(|e| matches!(e, Event::Switch(Switch::VoiceGuidance, _))),
            Some(Event::Switch(Switch::VoiceGuidance, true))
        );
        assert_eq!(
            first(|e| matches!(e, Event::GeneralSetting { slot: 0xD2, .. })),
            Some(Event::GeneralSetting { slot: 0xD2, on: true })
        );
        // Changing the equalizer made the headset ask a yes/no question.
        assert!(events.contains(&Event::Alert { message: 0x01, wants_choice: true }));
        // The volume was stepped through during the recording.
        let volumes = events.iter().filter(|e| matches!(e, Event::Volume(_))).count();
        assert!(volumes > 10);
        // The noise mode was switched off and back on from the client.
        assert!(events.contains(&Event::Noise(noise(NoiseMode::Off, 0, false))));
    }
}
