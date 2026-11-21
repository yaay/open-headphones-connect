// Everything the page asks of the app goes through this one object. Inside
// the real window it talks to Rust; in a plain browser (`npm run dev`) it is
// replaced by a simulated pair of headphones so the UI can be worked on
// without hardware.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  EMPTY_SNAPSHOT,
  type Accent,
  type CustomModel,
  type Mode,
  type ModelSettings,
  type NoiseMode,
  type Settings,
  type SettingsView,
  type SwitchId,
  type ShowRequest,
  type Snapshot,
  type Transport,
} from "./types";

type Unsubscribe = () => void;

export interface Backend {
  readonly simulated: boolean;
  getState(): Promise<Snapshot>;
  onState(handler: (snapshot: Snapshot) => void): Unsubscribe;
  onShow(handler: (request: ShowRequest) => void): Unsubscribe;
  onHidden(handler: () => void): Unsubscribe;
  ready(): void;
  present(mode: Mode, nonce: number, height: number): Promise<boolean>;
  resize(height: number): void;
  dismiss(): void;
  openPanel(): void;
  previewCard(): void;
  getSettings(): Promise<SettingsView>;
  updateSettings(settings: Settings): void;
  setStartWithWindows(enabled: boolean): Promise<void>;
  getAccent(): Promise<Accent | null>;
  /** Writes a line to the app's log file. */
  note(message: string): void;
  /** The user's own 3D model for the connect card, if they have added one. */
  /** Which of the user's own 3D models to show for this headset, if any. */
  findCustomModel(headset: string | null): Promise<CustomModel | null>;
  readCustomModel(name: string | null): Promise<ArrayBuffer | null>;
  setNoise(mode: NoiseMode, ambientLevel: number, focusOnVoice: boolean): void;
  setEqPreset(preset: number): void;
  setEqBands(clearBass: number | null, bands: number[]): void;
  setVolume(volume: number): void;
  transport(control: Transport): void;
  setSwitch(id: SwitchId, on: boolean): void;
  setAutoPowerOff(choice: number): void;
  connect(): void;
  disconnect(): void;
  powerOff(): void;
  openBluetoothSettings(): void;
  quit(): void;
}

// Listeners register asynchronously. `ready` waits for all of them, so the app
// never sends a `show` that nobody is listening for yet.
const registrations: Promise<unknown>[] = [];

function subscribe<T>(event: string, handler: (payload: T) => void): Unsubscribe {
  const pending = listen<T>(event, (e) => handler(e.payload));
  registrations.push(pending);
  return () => void pending.then((stop) => stop());
}

function parseModelSettings(text: string | null): ModelSettings {
  if (!text) return {};
  try {
    return JSON.parse(text) as ModelSettings;
  } catch (error) {
    console.error("model.json could not be read", error);
    return {};
  }
}

const fire = (command: string, args?: Record<string, unknown>) =>
  void invoke(command, args).catch((error) => console.error(command, error));

const real: Backend = {
  simulated: false,
  getState: () => invoke("get_state"),
  onState: (handler) => subscribe("state", handler),
  onShow: (handler) => subscribe("show", handler),
  onHidden: (handler) => subscribe<null>("hidden", () => handler()),
  ready: () => void Promise.allSettled(registrations).then(() => fire("frontend_ready")),
  present: (mode, nonce, height) => invoke("present", { mode, nonce, height }),
  resize: (height) => fire("resize", { height }),
  dismiss: () => fire("dismiss"),
  openPanel: () => fire("open_panel"),
  previewCard: () => fire("preview_card"),
  getSettings: () => invoke("get_settings"),
  updateSettings: (settings) => fire("update_settings", { settings }),
  setStartWithWindows: (enabled) => invoke("set_start_with_windows", { enabled }),
  getAccent: () => invoke("get_accent"),
  note: (message) => fire("log_note", { message }),
  findCustomModel: async (headset) => {
    const found = await invoke<{ name: string | null; settings: string | null } | null>("custom_model_info", {
      headset,
    });
    return found && { name: found.name, settings: parseModelSettings(found.settings) };
  },
  readCustomModel: async (name) => {
    const bytes = await invoke<ArrayBuffer>("custom_model", { name });
    return bytes && bytes.byteLength > 0 ? bytes : null;
  },
  setNoise: (mode, ambientLevel, focusOnVoice) => fire("set_noise", { mode, ambientLevel, focusOnVoice }),
  setEqPreset: (preset) => fire("set_eq_preset", { preset }),
  setEqBands: (clearBass, bands) => fire("set_eq_bands", { clearBass, bands }),
  setVolume: (volume) => fire("set_volume", { volume }),
  transport: (control) => fire("transport", { control }),
  setSwitch: (id, on) => fire("set_switch", { id, on }),
  setAutoPowerOff: (choice) => fire("set_auto_power_off", { choice }),
  connect: () => fire("connect"),
  disconnect: () => fire("disconnect"),
  powerOff: () => fire("power_off"),
  openBluetoothSettings: () => fire("open_bluetooth_settings"),
  quit: () => fire("quit"),
};

// ----- simulated headphones --------------------------------------------------

const CONNECTED: Snapshot = {
  ...EMPTY_SNAPSHOT,
  phase: "connected",
  deviceName: "WH-1000XM3",
  battery: { single: { level: 80, charging: false }, left: null, right: null, case: null },
  noise: { mode: "noise_cancelling", ambientLevel: 12, focusOnVoice: false },
  noiseModes: ["noise_cancelling", "wind", "ambient", "off"],
  minAmbientLevel: 1,
  maxAmbientLevel: 20,
  focusOnVoiceAvailable: true,
  volume: 21,
  maxVolume: 30,
  playback: "paused",
  model: "WH-1000XM3",
  colour: 0x01,
  switches: [
    { id: "upscaling", on: true },
    { id: "touch_panel", on: true },
    { id: "voice_guidance", on: true },
  ],
  autoPowerOff: 0x00,
  autoPowerOffOptions: [0x00, 0x01, 0x02, 0x03, 0x11],
  eq: { preset: 0x16, clearBass: 7, bands: [0, 0, 0, 0, 0], range: 10 },
  firmware: "4.5.2",
  codec: "LDAC",
  // As on a real WH-1000XM3, which cannot be switched off remotely.
  canPowerOff: false,
};

// Two newer models, as recordings of real ones show them, to see how the
// panel adapts: earbuds with a case, and open earbuds without noise control.
const EARBUDS: Snapshot = {
  ...CONNECTED,
  deviceName: "WF-1000XM5",
  model: "WF-1000XM5",
  colour: 0x03,
  battery: {
    single: null,
    left: { level: 90, charging: false },
    right: { level: 100, charging: false },
    case: { level: 53, charging: false },
  },
  noise: { mode: "ambient", ambientLevel: 20, focusOnVoice: false },
  noiseModes: ["noise_cancelling", "ambient", "off"],
  volume: 12,
  switches: [
    { id: "upscaling", on: true },
    { id: "speak_to_chat", on: false },
    { id: "pause_when_removed", on: true },
    { id: "multipoint", on: true },
    { id: "sidetone", on: false },
    { id: "voice_guidance", on: true },
  ],
  autoPowerOff: 0x10,
  autoPowerOffOptions: [0x10, 0x11],
  eq: { preset: 0xa0, clearBass: 10, bands: [1, 0, 3, -1, 2], range: 10 },
  firmware: "6.1.0",
  codec: "AAC",
  canPowerOff: true,
};

const OPEN_EARBUDS: Snapshot = {
  ...CONNECTED,
  deviceName: "LinkBuds Clip",
  model: "LinkBuds Clip",
  colour: 0x01,
  battery: { single: null, left: null, right: { level: 64, charging: false }, case: { level: 60, charging: false } },
  noise: null,
  noiseModes: [],
  minAmbientLevel: 0,
  maxAmbientLevel: 0,
  focusOnVoiceAvailable: false,
  volume: 30,
  switches: [
    { id: "upscaling", on: false },
    { id: "multipoint", on: true },
    { id: "voice_guidance", on: true },
  ],
  autoPowerOff: null,
  autoPowerOffOptions: [],
  eq: { preset: 0x00, clearBass: null, bands: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0], range: 6 },
  eqPresets: [0x00, 0x30, 0x31, 0x32, 0x33, 0xa0, 0xa1, 0xa2],
  eqFrequencies: [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000],
  firmware: "2.0.3",
  codec: "SBC",
  canPowerOff: true,
};

// A newer over-ear model, to see the card with a model of its own.
const NEWER_OVER_EAR: Snapshot = {
  ...CONNECTED,
  deviceName: "WH-1000XM5",
  model: "WH-1000XM5",
  noiseModes: ["noise_cancelling", "ambient", "off"],
  switches: [
    { id: "upscaling", on: true },
    { id: "speak_to_chat", on: false },
    { id: "pause_when_removed", on: true },
    { id: "touch_panel", on: true },
    { id: "multipoint", on: false },
    { id: "voice_guidance", on: true },
  ],
  firmware: "2.3.1",
  canPowerOff: true,
};

const PRESET_CURVES: Record<number, [number, number[]]> = {
  0x00: [0, [0, 0, 0, 0, 0]],
  0x10: [0, [0, 2, 4, 6, 8]],
  0x11: [6, [2, 0, 1, 4, 6]],
  0x12: [2, [-1, -2, -3, -5, -7]],
  0x13: [-2, [-1, -2, -4, -6, -8]],
  0x14: [-2, [3, 6, 5, 2, 0]],
  0x15: [0, [0, 0, 2, 5, 8]],
  0x16: [7, [0, 0, 0, 0, 0]],
  0x17: [-10, [4, 7, 6, 2, -4]],
};

export interface Simulator {
  set(patch: Partial<Snapshot>): void;
  show(mode: Mode, preview?: boolean): void;
  connected: Snapshot;
  /** Other models to try the panel with. */
  models: Array<[string, Snapshot]>;
}

function simulate(): Backend & { sim: Simulator } {
  let snapshot: Snapshot = CONNECTED;
  let settings: SettingsView = {
    showConnectCard: true,
    lowBatteryAlert: true,
    finish: "black",
    startWithWindows: false,
    version: "0.2.0",
  };
  let nonce = 0;
  const stateHandlers = new Set<(s: Snapshot) => void>();
  const showHandlers = new Set<(r: ShowRequest) => void>();
  const hiddenHandlers = new Set<() => void>();

  const set = (patch: Partial<Snapshot>) => {
    snapshot = { ...snapshot, ...patch };
    stateHandlers.forEach((h) => h(snapshot));
  };
  const show = (mode: Mode, preview = false) => {
    nonce += 1;
    showHandlers.forEach((h) => h({ mode, nonce, preview }));
  };
  const hide = () => {
    nonce += 1;
    hiddenHandlers.forEach((h) => h());
  };
  const add = <T,>(handlers: Set<T>, handler: T): Unsubscribe => {
    handlers.add(handler);
    return () => void handlers.delete(handler);
  };

  return {
    simulated: true,
    sim: {
      set,
      show,
      connected: CONNECTED,
      models: [
        ["XM5", NEWER_OVER_EAR],
        ["Earbuds", EARBUDS],
        ["Open earbuds", OPEN_EARBUDS],
      ],
    },
    getState: async () => snapshot,
    onState: (handler) => add(stateHandlers, handler),
    onShow: (handler) => add(showHandlers, handler),
    onHidden: (handler) => add(hiddenHandlers, handler),
    ready: () => {},
    present: async (_mode, requested) => requested === nonce,
    resize: () => {},
    dismiss: hide,
    openPanel: () => show("panel"),
    previewCard: () => show("card", true),
    getSettings: async () => settings,
    updateSettings: (next) => {
      settings = { ...settings, ...next };
    },
    setStartWithWindows: async (enabled) => {
      settings = { ...settings, startWithWindows: enabled };
    },
    getAccent: async () => null,
    note: (message) => console.info(message),
    // While developing, models placed in `local/` are served at the site
    // root: `model.glb` for every headset, `models/<headset>.glb` for one.
    findCustomModel: async (headset) => {
      const served = async (path: string, kind: string) => {
        try {
          const reply = await fetch(path);
          return reply.ok && (reply.headers.get("content-type") ?? "").includes(kind) ? reply : null;
        } catch {
          return null;
        }
      };
      const stems = headset ? [`/models/${encodeURIComponent(headset)}`, "/model"] : ["/model"];
      for (const stem of stems) {
        if (!(await served(`${stem}.glb`, "gltf"))) continue;
        const notes = await served(`${stem}.json`, "json");
        return { name: stem === "/model" ? null : headset, settings: parseModelSettings(notes && (await notes.text())) };
      }
      return null;
    },
    readCustomModel: async (name) => {
      try {
        const reply = await fetch(name ? `/models/${encodeURIComponent(name)}.glb` : "/model.glb");
        return reply.ok ? await reply.arrayBuffer() : null;
      } catch {
        return null;
      }
    },
    setNoise: (mode, ambientLevel, focusOnVoice) => set({ noise: { mode, ambientLevel, focusOnVoice } }),
    setEqPreset: (preset) => {
      const curve = PRESET_CURVES[preset];
      const previous = snapshot.eq ?? CONNECTED.eq!;
      const fits = curve && previous.clearBass !== null && curve[1].length === previous.bands.length;
      set({ eq: fits ? { ...previous, preset, clearBass: curve[0], bands: curve[1] } : { ...previous, preset } });
    },
    setEqBands: (clearBass, bands) => {
      const preset = snapshot.eq && snapshot.eq.preset >= 0xa0 ? snapshot.eq.preset : 0xa0;
      set({ eq: { preset, clearBass, bands, range: snapshot.eq?.range ?? 10 } });
    },
    setVolume: (volume) => set({ volume }),
    transport: (control) => {
      if (control === "play") set({ playback: "playing" });
      if (control === "pause") set({ playback: "paused" });
    },
    setSwitch: (id, on) =>
      set({ switches: snapshot.switches.map((item) => (item.id === id ? { ...item, on } : item)) }),
    setAutoPowerOff: (autoPowerOff) => set({ autoPowerOff }),
    connect: () => {
      set({ pending: "connecting_audio", notice: null });
      setTimeout(() => set({ ...EMPTY_SNAPSHOT, phase: "connecting", deviceName: "WH-1000XM3" }), 900);
      setTimeout(() => set(CONNECTED), 2200);
    },
    disconnect: () => set({ ...EMPTY_SNAPSHOT, phase: "disconnected", deviceName: "WH-1000XM3" }),
    powerOff: () => set({ ...EMPTY_SNAPSHOT, phase: "disconnected", deviceName: "WH-1000XM3" }),
    openBluetoothSettings: () => {},
    quit: () => {},
  };
}

const inApp = "__TAURI_INTERNALS__" in window;

if (inApp) {
  // Page errors end up in the app's log file, where they can be found later.
  const report = (message: string) => void invoke("report_error", { message }).catch(() => {});
  window.addEventListener("error", (event) => report(`${event.message} (${event.filename}:${event.lineno})`));
  window.addEventListener("unhandledrejection", (event) => report(`Unhandled rejection: ${String(event.reason)}`));
}
const simulated = inApp ? null : simulate();

export const backend: Backend = simulated ?? real;
export const simulator: Simulator | null = simulated?.sim ?? null;
