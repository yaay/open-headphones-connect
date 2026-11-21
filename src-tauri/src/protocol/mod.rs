//! Sony "MDR" control protocol: framing plus the two command tables.
//!
//! * [`v1`] is the table the WH-1000XM3 speaks (also XM2, XM4, WF-1000XM3…),
//!   reverse-engineered by Plutoberth/SonyHeadphonesClient.
//! * [`v2`] is the table newer models speak (XM5, XM6, LinkBuds…), first
//!   ported from abhiinhii/sony-tray.
//!
//! Both were filled out from libmdr in mos9527/SonyHeadphonesClient.

pub mod framing;
pub mod v1;
pub mod v2;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Generation {
    V1,
    V2,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NoiseMode {
    Off,
    NoiseCancelling,
    Wind,
    Ambient,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NoiseState {
    pub mode: NoiseMode,
    pub ambient_level: u8,
    pub focus_on_voice: bool,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BatterySlot {
    Single,
    Left,
    Right,
    Case,
}

/// Band values are on the user scale (0 = flat).
#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EqState {
    pub preset: u8,
    /// `None` on devices without a Clear Bass control.
    pub clear_bass: Option<i8>,
    pub bands: Vec<i8>,
    /// Every value runs from minus this to plus this.
    pub range: i8,
}

/// An on/off setting stored in the headset. Which ones exist depends on the
/// model; the headset says so when it connects.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Switch {
    /// DSEE upscaling.
    Upscaling,
    SpeakToChat,
    PauseWhenRemoved,
    TouchPanel,
    /// Staying connected to two devices at once.
    Multipoint,
    /// Hearing your own voice during calls.
    Sidetone,
    VoiceGuidance,
}

impl Switch {
    /// Headsets expose some switches as numbered "general settings" that
    /// introduce themselves by name. This maps the names known so far.
    pub fn from_general_setting(name: &str) -> Option<Switch> {
        match name {
            "TOUCH_PANEL_SETTING" => Some(Switch::TouchPanel),
            "MULTIPOINT_SETTING" => Some(Switch::Multipoint),
            "SIDETONE_SETTING" => Some(Switch::Sidetone),
            _ => None,
        }
    }
}

/// How a headset's noise control is laid out, as the headset describes it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NoiseCapability {
    pub modes: Vec<NoiseMode>,
    /// Full noise cancelling and wind noise reduction are separate modes.
    pub wind_mode: bool,
    pub max_ambient_level: u8,
    pub focus_on_voice: bool,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Playback {
    /// The headset can relay playback commands but has not said what is playing.
    Unknown,
    Playing,
    Paused,
    Stopped,
}

/// A playback command, relayed by the headset to whatever is playing.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Play,
    Pause,
    Next,
    Previous,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event {
    /// `table2` says whether the headset has a second command table.
    ProtocolInfo { table2: bool },
    /// The functions of the first command table, and of the second.
    SupportFunctions(Vec<u8>),
    SupportFunctions2(Vec<u8>),
    ModelName(String),
    /// Sony's colour code for this unit (1 black, 2 white, 3 silver…).
    ModelColour(u8),
    /// `None`: that earbud or case is not in use right now.
    Battery(BatterySlot, Option<Battery>),
    Noise(NoiseState),
    /// The noise-control message as received, for headsets whose next
    /// change has to start from it.
    NoiseRaw(Vec<u8>),
    NoiseCapability(NoiseCapability),
    Eq(EqState),
    /// An equalizer message as received, minus its two header bytes: the
    /// preset, on some headsets an ULT mode, then the values on the
    /// headset's own scale. Only what the headset says about its equalizer
    /// ([`Event::EqLayout`], its function list) explains how to read it.
    EqRaw(Vec<u8>),
    EqLayout { values: u8, steps: u8, presets: Vec<u8> },
    /// The frequency of each band, in hertz.
    EqFrequencies(Vec<u32>),
    Firmware(String),
    Codec(&'static str),
    Volume(u8),
    Playback(Playback),
    Switch(Switch, bool),
    /// The automatic power-off choice in effect, and the choices on offer.
    AutoPowerOff(u8),
    AutoPowerOffOptions(Vec<u8>),
    /// What one of the headset's numbered "general settings" is.
    GeneralSettingInfo { slot: u8, name: String, boolean: bool },
    GeneralSetting { slot: u8, on: bool },
    VoiceGuidanceLanguage(u8),
    /// The headset is asking a question and waits for a reply.
    Alert { message: u8, wants_choice: bool },
    /// A message that is understood and deliberately not used.
    Ignored,
}

pub const EQ_MANUAL: u8 = 0xA0;

/// Manual and the numbered custom slots; the named presets are fixed.
pub fn eq_preset_is_editable(preset: u8) -> bool {
    matches!(preset, 0xA0..=0xA5)
}

pub(crate) fn codec_name(code: u8) -> Option<&'static str> {
    Some(match code {
        0x01 => "SBC",
        0x02 => "AAC",
        0x10 => "LDAC",
        0x20 => "aptX",
        0x21 => "aptX HD",
        0x30 => "LC3",
        _ => return None,
    })
}

pub(crate) fn battery(level: u8, charging: u8) -> Battery {
    Battery { level: level.min(100), charging: charging == 1 }
}
