import { useEffect, useState } from "react";
import { backend } from "./backend";
import { BatteryGlyph, Button, Glyph, IconButton, NoiseIcon, Slider, Toggle } from "./controls";
import { Headphones } from "./Headphones";
import { headphonePortrait } from "./Headphones3D";
import {
  AUTO_POWER_OFF_LABEL,
  DEFAULT_EQ_PRESETS,
  EQ_PRESET_LABEL,
  NOISE_LABEL,
  SWITCH_INFO,
  frequencyLabel,
  headlineBattery,
  headsetOf,
  type Battery,
  type EqState,
  type Finish,
  type NoiseMode,
  type NoiseState,
  type SettingsView,
  type Snapshot,
} from "./types";

interface PanelProps {
  snapshot: Snapshot;
  settings: SettingsView;
  onSettings: (next: SettingsView) => void;
}

/** The control panel opened from the tray icon. */
export function Panel({ snapshot, settings, onSettings }: PanelProps) {
  const [page, setPage] = useState<"main" | "settings">("main");

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (page === "settings") setPage("main");
      else backend.dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [page]);

  if (page === "settings") {
    return (
      <SettingsPage
        snapshot={snapshot}
        settings={settings}
        onSettings={onSettings}
        onBack={() => setPage("main")}
      />
    );
  }

  return (
    <div className="panel">
      <Header snapshot={snapshot} finish={settings.finish} onOpenSettings={() => setPage("settings")} />
      {snapshot.notice && (
        <p className="notice" role="status">
          <Glyph name="info" />
          <span>{snapshot.notice}</span>
        </p>
      )}
      <Body snapshot={snapshot} />
    </div>
  );
}

// ----- header ----------------------------------------------------------------

const SLOT_LABEL = { left: "Left", right: "Right", case: "Case" } as const;

function BatteryLine({ battery, label }: { battery: Battery; label?: string }) {
  return (
    <span className="battery-line">
      <BatteryGlyph battery={battery} />
      <span>
        {label ? `${label} ` : ""}
        {battery.level}%{battery.charging ? ", charging" : ""}
      </span>
    </span>
  );
}

function statusLine(snapshot: Snapshot): string {
  if (snapshot.pending === "connecting_audio") return "Connecting…";
  if (snapshot.pending === "disconnecting") return "Disconnecting…";
  switch (snapshot.phase) {
    case "starting":
      return "Looking for your headphones…";
    case "bluetooth_off":
      return "Bluetooth is off";
    case "no_device":
      return "Not paired yet";
    case "disconnected":
      return "Not connected";
    case "connecting":
      return "Connecting…";
    case "connected":
      return snapshot.codec ? `Connected, ${snapshot.codec}` : "Connected";
  }
}

function Header(props: { snapshot: Snapshot; finish: Finish; onOpenSettings: () => void }) {
  const { snapshot } = props;
  const connected = snapshot.phase === "connected";
  const headline = connected ? headlineBattery(snapshot) : null;
  const busy = snapshot.phase === "connecting" || snapshot.phase === "starting" || snapshot.pending !== null;
  const { single, left, right, case: charger } = snapshot.battery;

  return (
    <header className="header">
      <HeaderArt finish={props.finish} headset={headsetOf(snapshot)} battery={headline} connected={connected} />
      <div className="header-text">
        <h1 className="header-name">{snapshot.deviceName ?? "Sony headphones"}</h1>
        <p className="header-status">{statusLine(snapshot)}</p>
        {connected && (
          <div className="header-batteries">
            {single && <BatteryLine battery={single} />}
            {left && <BatteryLine battery={left} label={SLOT_LABEL.left} />}
            {right && <BatteryLine battery={right} label={SLOT_LABEL.right} />}
            {charger && <BatteryLine battery={charger} label={SLOT_LABEL.case} />}
          </div>
        )}
      </div>
      <IconButton glyph="settings" label="Settings" className="header-settings" onClick={props.onOpenSettings} />
      {busy && <div className="progress" role="progressbar" aria-label="Working" />}
    </header>
  );
}

// Rendered at twice the size it is shown, so it stays sharp on dense screens.
const ART_PIXELS = 176;

/**
 * The picture beside the name: a still of the same 3D model the connect card
 * uses. Falls back to the flat drawing where 3D is not available.
 */
function HeaderArt(props: { finish: Finish; headset: string | null; battery: Battery | null; connected: boolean }) {
  // undefined: still rendering; null: 3D is not available.
  const [picture, setPicture] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    let current = true;
    void headphonePortrait(props.finish, ART_PIXELS, props.headset).then((made) => {
      if (current) setPicture(made);
    });
    return () => {
      current = false;
    };
  }, [props.finish, props.headset]);

  if (picture === null) {
    return (
      <Headphones
        className="header-art"
        finish={props.finish}
        level={props.battery?.level ?? null}
        charging={props.battery?.charging}
        dimmed={!props.connected}
      />
    );
  }
  return (
    <div className={`header-art header-photo ${props.connected ? "" : "header-photo-off"}`}>
      {picture && <img src={picture} alt="" />}
    </div>
  );
}

// ----- body ------------------------------------------------------------------

function Body({ snapshot }: { snapshot: Snapshot }) {
  switch (snapshot.phase) {
    case "starting":
    case "connecting":
      return null;
    case "bluetooth_off":
      return (
        <section className="empty">
          <p>Turn Bluetooth on to use your headphones with this PC.</p>
          <Button accent onClick={backend.openBluetoothSettings}>
            Open Bluetooth settings
          </Button>
        </section>
      );
    case "no_device":
      return (
        <section className="empty">
          <p>
            Pair your Sony headphones with this PC first. Hold their power button for 7 seconds
            until the light flashes blue, then add them in Bluetooth settings.
          </p>
          <Button accent onClick={backend.openBluetoothSettings}>
            Open Bluetooth settings
          </Button>
        </section>
      );
    case "disconnected":
      return (
        <section className="empty">
          <p>Switch the headphones on, then connect.</p>
          <Button accent busy={snapshot.pending === "connecting_audio"} onClick={backend.connect}>
            {snapshot.pending === "connecting_audio" ? "Connecting…" : "Connect"}
          </Button>
        </section>
      );
    case "connected":
      return (
        <>
          {snapshot.noise && <NoiseControl snapshot={snapshot} noise={snapshot.noise} />}
          <Playback snapshot={snapshot} />
          {snapshot.eq && <Equalizer eq={snapshot.eq} snapshot={snapshot} />}
          <HeadphoneSettings snapshot={snapshot} />
          <footer className="footer">
            <Button onClick={backend.disconnect} busy={snapshot.pending === "disconnecting"}>
              Disconnect
            </Button>
            {snapshot.canPowerOff && (
              <Button onClick={backend.powerOff}>
                <Glyph name="power" />
                Power off
              </Button>
            )}
          </footer>
        </>
      );
  }
}

const TILE_LABEL: Record<NoiseMode, string> = {
  noise_cancelling: "Cancelling",
  wind: "Wind",
  ambient: "Ambient",
  off: "Off",
};

function NoiseControl({ snapshot, noise }: { snapshot: Snapshot; noise: NoiseState }) {
  const apply = (next: Partial<NoiseState>) => {
    const merged = { ...noise, ...next };
    backend.setNoise(merged.mode, merged.ambientLevel, merged.focusOnVoice);
  };
  const shownLevel = noise.ambientLevel - snapshot.minAmbientLevel + 1;
  // Some models have ambient sound without a level, or without a voice mode.
  const hasLevel = snapshot.maxAmbientLevel > 0;

  return (
    <section className="section" aria-labelledby="noise-title">
      <h2 id="noise-title" className="section-title">
        Noise control
      </h2>
      <div className="tiles" role="radiogroup" aria-labelledby="noise-title">
        {snapshot.noiseModes.map((mode) => (
          <button
            key={mode}
            type="button"
            role="radio"
            aria-checked={noise.mode === mode}
            title={NOISE_LABEL[mode]}
            className="tile"
            onClick={() => apply({ mode })}
          >
            <NoiseIcon mode={mode} />
            <span>{TILE_LABEL[mode]}</span>
          </button>
        ))}
      </div>
      {noise.mode === "ambient" && (hasLevel || snapshot.focusOnVoiceAvailable) && (
        <div className="ambient">
          {hasLevel && (
            <div className="ambient-level">
              <span id="ambient-label">Ambient level</span>
              <Slider
                label="Ambient level"
                min={snapshot.minAmbientLevel}
                max={snapshot.maxAmbientLevel}
                value={noise.ambientLevel}
                onChange={(ambientLevel) => apply({ ambientLevel })}
              />
              <output className="ambient-value">{shownLevel}</output>
            </div>
          )}
          {snapshot.focusOnVoiceAvailable && (
            <Toggle
              label="Focus on voice"
              hint="Lets voices through while cutting other noise"
              checked={noise.focusOnVoice}
              onChange={(focusOnVoice) => apply({ focusOnVoice })}
            />
          )}
        </div>
      )}
    </section>
  );
}

/** Volume and track controls, relayed by the headset to whatever is playing. */
function Playback({ snapshot }: { snapshot: Snapshot }) {
  const { playback, volume, maxVolume } = snapshot;
  if (playback === null && volume === null) return null;
  const playing = playback === "playing";

  return (
    <section className="section" aria-labelledby="playback-title">
      <div className="section-row">
        <h2 id="playback-title" className="section-title">
          Playback
        </h2>
        {playback !== null && (
          <div className="transport">
            <IconButton glyph="previous" label="Previous track" onClick={() => backend.transport("previous")} />
            <IconButton
              glyph={playing ? "pause" : "play"}
              label={playing ? "Pause" : "Play"}
              onClick={() => backend.transport(playing ? "pause" : "play")}
            />
            <IconButton glyph="next" label="Next track" onClick={() => backend.transport("next")} />
          </div>
        )}
      </div>
      {volume !== null && maxVolume > 0 && (
        <div className="volume">
          <Glyph name="volume" />
          <Slider label="Volume" min={0} max={maxVolume} value={volume} onChange={backend.setVolume} />
          <output className="volume-value">{Math.round((volume / maxVolume) * 100)}%</output>
        </div>
      )}
    </section>
  );
}

/** Settings stored in the headphones themselves. Closed until asked for. */
function HeadphoneSettings({ snapshot }: { snapshot: Snapshot }) {
  const [open, setOpen] = useState(() => localStorage.getItem("headphone-settings-open") === "1");
  const { switches, autoPowerOff, autoPowerOffOptions } = snapshot;
  const powerOff = autoPowerOff !== null && autoPowerOffOptions.length > 0;
  if (switches.length === 0 && !powerOff) return null;

  const toggle = () => {
    localStorage.setItem("headphone-settings-open", open ? "0" : "1");
    setOpen(!open);
  };

  return (
    <section className="section" aria-labelledby="headphone-settings-title">
      <div className="section-row">
        <h2 id="headphone-settings-title" className="section-title">
          Headphone settings
        </h2>
        <IconButton
          glyph={open ? "chevronUp" : "chevronDown"}
          label={open ? "Hide headphone settings" : "Show headphone settings"}
          onClick={toggle}
        />
      </div>
      {open && (
        <div className="settings-list">
          {switches.map(({ id, on }) => (
            <Toggle
              key={id}
              label={SWITCH_INFO[id].label}
              hint={SWITCH_INFO[id].hint}
              checked={on}
              onChange={(next) => backend.setSwitch(id, next)}
            />
          ))}
          {powerOff && (
            <label className="setting-row">
              <span className="toggle-text">
                <span>Automatic power-off</span>
                <span className="hint">Switches off by itself to save the battery</span>
              </span>
              <select
                className="select"
                value={autoPowerOff}
                onChange={(event) => backend.setAutoPowerOff(Number(event.currentTarget.value))}
              >
                {autoPowerOffOptions.map((choice) => (
                  <option key={choice} value={choice}>
                    {AUTO_POWER_OFF_LABEL[choice] ?? `Option ${choice}`}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
      )}
    </section>
  );
}

const SIX_BAND_LABELS = ["400", "1k", "2.5k", "6.3k", "16k"];
const TEN_BAND_LABELS = ["31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];

function Equalizer({ eq, snapshot }: { eq: EqState; snapshot: Snapshot }) {
  const [open, setOpen] = useState(() => localStorage.getItem("eq-open") === "1");
  const toggle = () => {
    localStorage.setItem("eq-open", open ? "0" : "1");
    setOpen(!open);
  };

  const hasBands = eq.bands.length > 0;
  const range = eq.range > 0 ? eq.range : eq.clearBass === null ? 6 : 10;
  // The model's own band frequencies where it gives them, else Sony's usual two layouts.
  const labels =
    snapshot.eqFrequencies.length === eq.bands.length
      ? snapshot.eqFrequencies.map(frequencyLabel)
      : eq.bands.length === 10
        ? TEN_BAND_LABELS
        : SIX_BAND_LABELS;
  const presets = snapshot.eqPresets.length > 0 ? snapshot.eqPresets : DEFAULT_EQ_PRESETS;
  const known = presets.includes(eq.preset);
  const setBand = (index: number, value: number) =>
    backend.setEqBands(eq.clearBass, eq.bands.map((band, i) => (i === index ? value : band)));

  return (
    <section className="section" aria-labelledby="eq-title">
      <div className="section-row">
        <h2 id="eq-title" className="section-title">
          Equalizer
        </h2>
        <select
          className="select"
          aria-label="Equalizer preset"
          value={eq.preset}
          onChange={(event) => backend.setEqPreset(Number(event.currentTarget.value))}
        >
          {!known && <option value={eq.preset}>{EQ_PRESET_LABEL[eq.preset] ?? "Custom"}</option>}
          {presets.map((code) => (
            <option key={code} value={code}>
              {EQ_PRESET_LABEL[code] ?? `Preset ${code}`}
            </option>
          ))}
        </select>
        {hasBands && (
          <IconButton
            glyph={open ? "chevronUp" : "chevronDown"}
            label={open ? "Hide sliders" : "Adjust sliders"}
            onClick={toggle}
          />
        )}
      </div>
      {open && hasBands && (
        <div className={`bands ${eq.bands.length > 6 ? "bands-ten" : ""}`}>
          {eq.clearBass !== null && (
            <Band
              label="Bass"
              title="Clear Bass"
              value={eq.clearBass}
              range={range}
              onChange={(value) => backend.setEqBands(value, eq.bands)}
            />
          )}
          {eq.bands.map((band, index) => (
            <Band
              key={index}
              label={labels[index] ?? String(index + 1)}
              title={`${labels[index] ?? index + 1} Hz`}
              value={band}
              range={range}
              onChange={(value) => setBand(index, value)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function Band(props: { label: string; title: string; value: number; range: number; onChange: (v: number) => void }) {
  return (
    <div className="band" title={props.title}>
      <output className="band-value">{props.value > 0 ? `+${props.value}` : props.value}</output>
      <Slider
        vertical
        label={props.title}
        min={-props.range}
        max={props.range}
        value={props.value}
        onChange={props.onChange}
      />
      <span className="band-label">{props.label}</span>
    </div>
  );
}

// ----- settings --------------------------------------------------------------

function SettingsPage(props: {
  snapshot: Snapshot;
  settings: SettingsView;
  onSettings: (next: SettingsView) => void;
  onBack: () => void;
}) {
  const { settings, onSettings } = props;
  const [error, setError] = useState<string | null>(null);

  const save = (patch: Partial<SettingsView>) => {
    const next = { ...settings, ...patch };
    onSettings(next);
    backend.updateSettings({
      showConnectCard: next.showConnectCard,
      lowBatteryAlert: next.lowBatteryAlert,
      finish: next.finish,
    });
  };

  const setStartup = (enabled: boolean) => {
    setError(null);
    onSettings({ ...settings, startWithWindows: enabled });
    backend.setStartWithWindows(enabled).catch((reason) => {
      onSettings({ ...settings, startWithWindows: !enabled });
      setError(String(reason));
    });
  };

  return (
    <div className="panel">
      <header className="page-header">
        <IconButton glyph="back" label="Back" onClick={props.onBack} />
        <h1 className="page-title">Settings</h1>
      </header>

      <section className="section settings-group">
        <Toggle
          label="Start with Windows"
          hint="Runs quietly in the notification area"
          checked={settings.startWithWindows}
          onChange={setStartup}
        />
        <Toggle
          label="Show a card when headphones connect"
          checked={settings.showConnectCard}
          onChange={(showConnectCard) => save({ showConnectCard })}
        />
        <Toggle
          label="Warn when the battery reaches 20%"
          checked={settings.lowBatteryAlert}
          onChange={(lowBatteryAlert) => save({ lowBatteryAlert })}
        />
        {error && (
          <p className="notice" role="alert">
            <Glyph name="info" />
            <span>{error}</span>
          </p>
        )}
      </section>

      <section className="section" aria-labelledby="finish-title">
        <div className="section-row">
          <h2 id="finish-title" className="section-title">
            Headphone colour
          </h2>
          <div className="segments" role="radiogroup" aria-labelledby="finish-title">
            {(["black", "silver"] as const).map((finish) => (
              <button
                key={finish}
                type="button"
                role="radio"
                aria-checked={settings.finish === finish}
                className="segment"
                onClick={() => save({ finish })}
              >
                {finish === "black" ? "Black" : "Silver"}
              </button>
            ))}
          </div>
        </div>
      </section>

      <footer className="footer">
        <Button onClick={backend.previewCard}>Preview connect card</Button>
        <Button onClick={backend.quit}>Quit</Button>
      </footer>

      <p className="about">
        Open Headphones Connect {settings.version}
        {props.snapshot.firmware ? `. Headphone firmware ${props.snapshot.firmware}` : ""}. An
        unofficial, open-source app: not made by or affiliated with Sony. Protocol work by the
        SonyHeadphonesClient and Sony Tray projects.
      </p>
    </div>
  );
}
