export type Phase =
  | "starting"
  | "bluetooth_off"
  | "no_device"
  | "disconnected"
  | "connecting"
  | "connected";

export type NoiseMode = "off" | "noise_cancelling" | "wind" | "ambient";

export interface Battery {
  level: number;
  charging: boolean;
}

export interface NoiseState {
  mode: NoiseMode;
  ambientLevel: number;
  focusOnVoice: boolean;
}

export type Playback = "unknown" | "playing" | "paused" | "stopped";
export type Transport = "play" | "pause" | "next" | "previous";

export interface EqState {
  preset: number;
  /** null on headsets without a Clear Bass control. */
  clearBass: number | null;
  bands: number[];
  /** Every value runs from minus this to plus this. */
  range: number;
}

/** An on/off setting stored in the headphones. Which exist depends on the model. */
export type SwitchId =
  | "upscaling"
  | "speak_to_chat"
  | "pause_when_removed"
  | "touch_panel"
  | "multipoint"
  | "sidetone"
  | "voice_guidance";

export interface SwitchState {
  id: SwitchId;
  on: boolean;
}

export interface Snapshot {
  phase: Phase;
  /** The name Windows knows the headphones by. */
  deviceName: string | null;
  /** The model and Sony's colour code, as the headphones report them. */
  model: string | null;
  colour: number | null;
  battery: {
    single: Battery | null;
    left: Battery | null;
    right: Battery | null;
    case: Battery | null;
  };
  noise: NoiseState | null;
  noiseModes: NoiseMode[];
  minAmbientLevel: number;
  /** 0 where ambient sound has no level to set. */
  maxAmbientLevel: number;
  eq: EqState | null;
  /** The presets this model has; empty where it does not list them. */
  eqPresets: number[];
  /** The frequency of each band in hertz; empty where the model does not say. */
  eqFrequencies: number[];
  firmware: string | null;
  codec: string | null;
  canPowerOff: boolean;
  focusOnVoiceAvailable: boolean;
  /** Headset volume, 0 to maxVolume; null where it cannot be set from here. */
  volume: number | null;
  maxVolume: number;
  /** Non-null when play, pause and skip can be sent through the headset. */
  playback: Playback | null;
  /** The on/off settings this model has, in the order to show them. */
  switches: SwitchState[];
  /** The automatic power-off choice in effect, and the choices on offer. */
  autoPowerOff: number | null;
  autoPowerOffOptions: number[];
  pending: "connecting_audio" | "disconnecting" | null;
  notice: string | null;
  /** Goes up each time the headset connects afresh. */
  connections: number;
}

export type Finish = "black" | "silver";

export interface Settings {
  showConnectCard: boolean;
  lowBatteryAlert: boolean;
  finish: Finish;
}

export interface SettingsView extends Settings {
  startWithWindows: boolean;
  version: string;
}

export interface Accent {
  base: string;
  onDark: string;
  onLight: string;
}

/**
 * How a user-supplied 3D model should be placed. The app expects headphones
 * standing upright and facing the viewer; these settings turn a model that
 * was saved some other way round.
 */
export interface ModelSettings {
  /** Degrees about X, Y and Z, applied before the model is centred. */
  rotation?: [number, number, number];
  /** How large it appears on the card; 1 is the default framing. */
  scale?: number;
  /** Surface roughness override, 0 (glossy) to 1 (matte). */
  roughness?: number;
  /** Lighting strength relative to the built-in model; 1 leaves it alone. */
  brightness?: number;
  /** Colour correction: red, green and blue multipliers for every surface. */
  tint?: [number, number, number];
  /** Names of materials whose texture has see-through areas (printed labels). */
  cutout?: string[];
}

export interface CustomModel {
  bytes: ArrayBuffer;
  settings: ModelSettings;
}

export type Mode = "card" | "panel";

export interface ShowRequest {
  mode: Mode;
  nonce: number;
  preview: boolean;
}

export const EMPTY_SNAPSHOT: Snapshot = {
  phase: "starting",
  deviceName: null,
  model: null,
  colour: null,
  battery: { single: null, left: null, right: null, case: null },
  noise: null,
  noiseModes: [],
  minAmbientLevel: 0,
  maxAmbientLevel: 0,
  eq: null,
  eqPresets: [],
  eqFrequencies: [],
  firmware: null,
  codec: null,
  canPowerOff: false,
  focusOnVoiceAvailable: false,
  volume: null,
  maxVolume: 0,
  playback: null,
  switches: [],
  autoPowerOff: null,
  autoPowerOffOptions: [],
  pending: null,
  notice: null,
  connections: 0,
};

/** The battery to headline: the headset, or the weaker earbud. */
export function headlineBattery(snapshot: Snapshot): Battery | null {
  const { single, left, right } = snapshot.battery;
  if (single) return single;
  if (left && right) return left.level <= right.level ? left : right;
  return left ?? right;
}

export const NOISE_LABEL: Record<NoiseMode, string> = {
  noise_cancelling: "Noise cancelling",
  wind: "Wind noise reduction",
  ambient: "Ambient sound",
  off: "Off",
};

/** The headset's automatic power-off choices, by the id it uses for each. */
export const AUTO_POWER_OFF_LABEL: Record<number, string> = {
  0x00: "After 5 minutes",
  0x01: "After 30 minutes",
  0x02: "After 1 hour",
  0x03: "After 3 hours",
  0x04: "After 15 minutes",
  0x10: "When taken off",
  0x11: "Never",
};

/** What each on/off setting is called, and what it does. */
export const SWITCH_INFO: Record<SwitchId, { label: string; hint: string }> = {
  upscaling: { label: "DSEE upscaling", hint: "Restores detail lost in compressed music" },
  speak_to_chat: { label: "Speak-to-Chat", hint: "Pauses the music when you start talking" },
  pause_when_removed: { label: "Pause when taken off", hint: "Playback stops when you take them off" },
  touch_panel: { label: "Touch panel", hint: "Taps and swipes on the right ear cup" },
  multipoint: { label: "Connect to two devices", hint: "The headphones reconnect when this changes" },
  sidetone: { label: "Hear your voice on calls", hint: "Plays your own voice back while you talk" },
  voice_guidance: { label: "Voice guidance", hint: "Spoken prompts for power, battery and pairing" },
};

/** Sony's equalizer presets, by the id the headphones use for each. */
export const EQ_PRESET_LABEL: Record<number, string> = {
  0x00: "Off",
  0x01: "Rock",
  0x02: "Pop",
  0x03: "Jazz",
  0x04: "Dance",
  0x05: "EDM",
  0x06: "R&B / Hip-hop",
  0x07: "Acoustic",
  0x10: "Bright",
  0x11: "Excited",
  0x12: "Mellow",
  0x13: "Relaxed",
  0x14: "Vocal",
  0x15: "Treble boost",
  0x16: "Bass boost",
  0x17: "Speech",
  0x20: "Gaming",
  0x21: "FPS 1",
  0x22: "FPS 2",
  0x23: "FPS 3",
  0x30: "Heavy",
  0x31: "Clear",
  0x32: "Hard",
  0x33: "Soft",
  0xa0: "Manual",
  0xa1: "Custom 1",
  0xa2: "Custom 2",
  0xa3: "Custom 3",
  0xa4: "Custom 4",
  0xa5: "Custom 5",
};

/** The presets offered when a model does not list its own. */
export const DEFAULT_EQ_PRESETS = [0x00, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0xa0, 0xa1, 0xa2];

/** A band's frequency as it is printed under its slider: 400, 1k, 2.5k. */
export function frequencyLabel(hertz: number): string {
  return hertz >= 1000 ? `${Number((hertz / 1000).toFixed(1))}k` : String(hertz);
}
