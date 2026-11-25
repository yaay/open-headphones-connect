//! Sony "MDR" control protocol: framing plus the two command tables.
//!
//! * [`v1`] is the table the WH-1000XM3 speaks (also XM2, XM4, WF-1000XM3…),
//!   reverse-engineered by Plutoberth/SonyHeadphonesClient.
//! * [`v2`] is the table newer models speak (XM5, XM6, LinkBuds…), first
//!   ported from abhiinhii/sony-tray.
//!
//! Both were filled out from libmdr in mos9527/SonyHeadphonesClient.
//!
//! [`nothing`] is the protocol of Nothing and CMF earbuds and headphones, which has nothing to
//! do with Sony's.

pub mod framing;
pub mod nothing;
pub mod v1;
pub mod v2;

use serde::{Deserialize, Serialize};

/// Which protocol the control channel speaks: one of Sony's two command tables, or Nothing's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Generation {
    V1,
    V2,
    /// Nothing and CMF earbuds and headphones, which speak a protocol of their own.
    Nothing,
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
    /// Less delay between picture and sound, for games and video.
    LowLatency,
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

/// Where the NC Optimizer is in a run.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum OptimizerPhase {
    #[default]
    Idle,
    /// Measuring how the headset sits on this wearer.
    MeasuringFit,
    MeasuringPressure,
    Optimizing,
    Finished,
}

/// The NC Optimizer: a run in which the headset plays test tones, measures
/// how it sits on the wearer and the air pressure around it, and tunes its
/// noise cancelling to both.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Optimizer {
    pub phase: OptimizerPhase,
    /// False while the headset will not start a run.
    pub available: bool,
    /// About how long a run takes, in seconds. 0 where the headset did not say.
    pub seconds: u8,
    /// Whether the wearer's fit has been measured at some time.
    pub fit_measured: bool,
    /// The air pressure last measured, in tenths of an atmosphere.
    pub pressure: Option<u8>,
}

/// A button or touch surface on the headset whose job can be chosen.
///
/// Keys and jobs go by short names so that an interface can label the ones it
/// knows and still show the rest: see [`assign`].
#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub key: String,
    /// A button to press, as against a surface to tap.
    pub button: bool,
    /// What it does now, and everything it can be given to do.
    pub function: String,
    pub options: Vec<String>,
}

/// The names [`Assignment`] uses, and the numbers behind them.
pub mod assign {
    const KEYS: [(u8, &str); 6] =
        [(0x00, "left"), (0x01, "right"), (0x02, "custom"), (0x03, "c"), (0x04, "nc_amb"), (0x05, "nc_amb")];

    /// `AssignableSettingsPreset`: the jobs a key can have. The same numbers
    /// in both generations, the second having added to them.
    const PRESETS: [(u8, &str); 15] = [
        (0x00, "noise_control"),
        (0x10, "volume"),
        (0x20, "playback"),
        (0x21, "tracks"),
        (0x30, "voice_assistant"),
        (0x31, "google_assistant"),
        (0x32, "amazon_alexa"),
        (0x33, "tencent_xiaowei"),
        (0x35, "noise_control_and_quick_access"),
        (0x36, "quick_access"),
        (0x38, "teams"),
        (0x45, "noise_control_and_microphone"),
        (0x46, "listening_mode_and_quick_access"),
        (0x47, "noise_control_and_listening_mode"),
        (0xFF, "none"),
    ];

    pub fn key_name(key: u8) -> String {
        KEYS.iter().find(|(id, _)| *id == key).map_or_else(|| format!("key_{key:02x}"), |(_, name)| (*name).into())
    }

    pub fn preset_name(preset: u8) -> String {
        PRESETS
            .iter()
            .find(|(id, _)| *id == preset)
            .map_or_else(|| format!("preset_{preset:02x}"), |(_, name)| (*name).into())
    }

    /// The number behind a name [`preset_name`] gave.
    pub fn preset_id(name: &str) -> Option<u8> {
        PRESETS
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(id, _)| *id)
            .or_else(|| u8::from_str_radix(name.strip_prefix("preset_")?, 16).ok())
    }

    /// The job behind one choice of the WH-1000XM3's button setting, which
    /// names its choices in words: `ASSIGNABLE_KEY_ELEM_GOOGLE_ASSISTANT`.
    pub fn element_name(element: &str) -> String {
        match element.strip_prefix("ASSIGNABLE_KEY_ELEM_").unwrap_or(element) {
            "NCASM" => "noise_control".into(),
            other => other.to_ascii_lowercase(),
        }
    }
}

/// What kind of thing a paired device is, as far as its Bluetooth class says.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Computer,
    Phone,
    Other,
}

/// A device the headset is paired with. A headset with multipoint can be
/// connected to two at once, and plays the sound of one of them.
#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    /// As the headset writes it: `AA:BB:CC:DD:EE:FF`.
    pub address: String,
    pub name: String,
    pub kind: DeviceKind,
    pub connected: bool,
    /// Whether the sound is this device's right now.
    pub source: bool,
}

/// A second-generation key as the headset describes it: what it is, and the
/// jobs (`AssignableSettingsPreset` numbers) it can be given.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AssignableKey {
    pub key: u8,
    pub button: bool,
    pub presets: Vec<u8>,
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
    /// A general setting that is one choice among several, each named, and
    /// which of them is chosen.
    GeneralSettingList { slot: u8, name: String, options: Vec<String> },
    GeneralSettingChoice { slot: u8, index: u8 },
    /// About how long a run of the NC Optimizer takes, in seconds.
    OptimizerTime(u8),
    OptimizerStatus { available: bool, phase: OptimizerPhase },
    OptimizerResult { fit_measured: bool, pressure: Option<u8> },
    /// The keys whose job can be chosen, and the job each has, in the same
    /// order.
    AssignableKeys(Vec<AssignableKey>),
    AssignedPresets(Vec<u8>),
    PairedDevices(Vec<PairedDevice>),
    /// How connecting to, disconnecting from or switching to a paired device
    /// went, as far as the headset has got with it.
    DeviceAction { address: String, done: bool, failed: bool },
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
