import { useRef, type CSSProperties, type ReactNode } from "react";
import type { Battery, NoiseMode } from "./types";

// Glyphs from Segoe Fluent Icons, which ships with Windows 11.
export const GLYPH = {
  settings: "",
  back: "",
  close: "",
  chevronDown: "",
  chevronUp: "",
  bluetooth: "",
  power: "",
  info: "",
  play: "",
  pause: "",
  previous: "",
  next: "",
  volume: "",
} as const;

export function Glyph({ name, className }: { name: keyof typeof GLYPH; className?: string }) {
  return (
    <span className={`glyph ${className ?? ""}`} aria-hidden="true">
      {GLYPH[name]}
    </span>
  );
}

export function IconButton(props: { glyph: keyof typeof GLYPH; label: string; onClick: () => void; className?: string }) {
  return (
    <button
      type="button"
      className={`icon-button ${props.className ?? ""}`}
      aria-label={props.label}
      title={props.label}
      onClick={(event) => {
        event.stopPropagation();
        props.onClick();
      }}
    >
      <Glyph name={props.glyph} />
    </button>
  );
}

export function Button(props: {
  children: ReactNode;
  onClick: () => void;
  accent?: boolean;
  disabled?: boolean;
  busy?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      className={`button ${props.accent ? "button-accent" : ""} ${props.className ?? ""}`}
      disabled={props.disabled || props.busy}
      onClick={props.onClick}
    >
      {props.busy && <span className="ring" aria-hidden="true" />}
      {props.children}
    </button>
  );
}

export function Toggle(props: { checked: boolean; onChange: (checked: boolean) => void; label: string; hint?: string }) {
  return (
    <label className="toggle-row">
      <span className="toggle-text">
        <span>{props.label}</span>
        {props.hint && <span className="hint">{props.hint}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={props.checked}
        aria-label={props.label}
        className="toggle"
        onClick={() => props.onChange(!props.checked)}
      >
        <span className="toggle-knob" />
      </button>
    </label>
  );
}

interface SliderProps {
  value: number;
  min: number;
  max: number;
  label: string;
  vertical?: boolean;
  /** Called continuously while dragging, at most every 80 ms. */
  onChange: (value: number) => void;
}

/** A Fluent slider. Drags are throttled so the headset is not flooded. */
export function Slider({ value, min, max, label, vertical, onChange }: SliderProps) {
  const lastSent = useRef(0);
  const trailing = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const percent = max === min ? 0 : ((value - min) / (max - min)) * 100;

  const send = (next: number) => {
    clearTimeout(trailing.current);
    const wait = 80 - (Date.now() - lastSent.current);
    if (wait <= 0) {
      lastSent.current = Date.now();
      onChange(next);
    } else {
      trailing.current = setTimeout(() => {
        lastSent.current = Date.now();
        onChange(next);
      }, wait);
    }
  };

  return (
    <input
      type="range"
      className={`slider ${vertical ? "slider-vertical" : ""}`}
      aria-label={label}
      min={min}
      max={max}
      step={1}
      value={value}
      style={{ "--pos": `${percent}%` } as CSSProperties}
      onChange={(event) => send(Number(event.currentTarget.value))}
    />
  );
}

export function BatteryGlyph({ battery }: { battery: Battery }) {
  const low = battery.level <= 20 && !battery.charging;
  const width = Math.max(1.5, (battery.level / 100) * 15);
  return (
    <svg
      className={`battery ${low ? "battery-low" : ""} ${battery.charging ? "battery-charging" : ""}`}
      viewBox="0 0 24 14"
      aria-hidden="true"
    >
      <rect x="0.75" y="1.75" width="19.5" height="10.5" rx="3" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <rect x="21.5" y="4.75" width="2" height="4.5" rx="1" fill="currentColor" />
      <rect className="battery-level" x="3" y="4" width={width} height="6" rx="1.2" />
      {battery.charging && <path className="battery-bolt" d="M11.6 2.6 7.4 7.6h2.9l-1 3.8 4.3-5h-3z" />}
    </svg>
  );
}

export function NoiseIcon({ mode }: { mode: NoiseMode }) {
  const common = {
    viewBox: "0 0 20 20",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.5,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    className: "noise-icon",
  };
  switch (mode) {
    case "noise_cancelling":
      // A sound wave flattening out.
      return (
        <svg {...common}>
          <path d="M2 10c1-5 2.4-5 3.4 0s2.4 5 3.4 0" />
          <path d="M11.5 10H18" />
        </svg>
      );
    case "wind":
      return (
        <svg {...common}>
          <path d="M2.5 7.5h8.2a2.3 2.3 0 1 0-2.2-3" />
          <path d="M2.5 11h12.2a2.5 2.5 0 1 1-2.4 3.2" />
          <path d="M2.5 14.5h5" />
        </svg>
      );
    case "ambient":
      // Sound reaching in from both sides.
      return (
        <svg {...common}>
          <circle cx="10" cy="10" r="1.6" fill="currentColor" stroke="none" />
          <path d="M6.6 6.9a4.6 4.6 0 0 0 0 6.2M13.4 6.9a4.6 4.6 0 0 1 0 6.2" />
          <path d="M3.9 4.6a8 8 0 0 0 0 10.8M16.1 4.6a8 8 0 0 1 0 10.8" />
        </svg>
      );
    case "off":
      return (
        <svg {...common}>
          <circle cx="10" cy="10" r="6.5" />
          <path d="M5.5 14.5l9-9" />
        </svg>
      );
  }
}
