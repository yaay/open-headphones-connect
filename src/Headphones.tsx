import { useEffect, useState, type ReactNode } from "react";
import type { Finish } from "./types";

// When the gauge starts filling, measured from the moment the card appears.
// The headband has finished drawing and the ear cups have settled by then.
export const GAUGE_DELAY_MS = 950;
export const GAUGE_FILL_MS = 1100;

interface Props {
  finish: Finish;
  /** Battery percentage drawn along the headband; null hides the gauge. */
  level: number | null;
  charging?: boolean;
  /** Play the connect sequence instead of drawing the finished state. */
  entrance?: boolean;
  /** Greyed out while nothing is connected. */
  dimmed?: boolean;
  /** Rendered in the open space between the ear cups. */
  children?: ReactNode;
  className?: string;
}

// A front-on pair of over-ear headphones. The headband doubles as the battery
// gauge: a thin light strip that fills from the left cup to the right.
const BAND_LEFT = "M120 26 A74 74 0 0 0 46 100 V114";
const BAND_RIGHT = "M120 26 A74 74 0 0 1 194 100 V114";
const GAUGE = "M46 114 V100 A74 74 0 0 1 194 100 V114";

export function Headphones({ finish, level, charging, entrance, dimmed, children, className }: Props) {
  // Start empty, then let the CSS transition carry the strip to its level.
  const [filled, setFilled] = useState(!entrance);
  useEffect(() => {
    if (!entrance) {
      setFilled(true);
      return;
    }
    const timer = setTimeout(() => setFilled(true), GAUGE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [entrance]);

  const shown = level === null || !filled ? 0 : Math.max(0, Math.min(100, level));
  const tone = level !== null && level <= 20 && !charging ? "low" : charging ? "charging" : "normal";
  const classes = ["hp", `hp-${finish}`, entrance && "hp-entrance", dimmed && "hp-dimmed", className]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={classes}>
      <svg viewBox="0 0 240 190" aria-hidden="true" focusable="false">
        <defs>
          <linearGradient id={`hp-cup-${finish}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="var(--hp-cup-top)" />
            <stop offset="1" stopColor="var(--hp-cup-bottom)" />
          </linearGradient>
        </defs>

        <g className="hp-band" fill="none" strokeLinecap="round">
          <path className="hp-rim" d={BAND_LEFT} pathLength={100} strokeWidth="16" />
          <path className="hp-rim" d={BAND_RIGHT} pathLength={100} strokeWidth="16" />
          <path d={BAND_LEFT} pathLength={100} stroke="var(--hp-band)" strokeWidth="14" />
          <path d={BAND_RIGHT} pathLength={100} stroke="var(--hp-band)" strokeWidth="14" />
          <path
            className="hp-cushion"
            d="M88 39 A66 66 0 0 1 152 39"
            stroke="var(--hp-cushion)"
            strokeWidth="9"
          />
        </g>

        {level !== null && (
          <g className={`hp-gauge hp-gauge-${tone}`} fill="none" strokeLinecap="round">
            <path className="hp-gauge-track" d={GAUGE} strokeWidth="3.5" />
            <path
              className="hp-gauge-fill"
              d={GAUGE}
              pathLength={100}
              strokeWidth="3.5"
              strokeDasharray="100 100"
              style={{ strokeDashoffset: 100 - shown, opacity: shown > 0 ? 1 : 0 }}
            />
          </g>
        )}

        <g className="hp-cup hp-cup-left">
          <rect x="58" y="106" width="17" height="62" rx="8.5" fill="var(--hp-pad)" />
          <rect x="24" y="98" width="42" height="78" rx="21" fill={`url(#hp-cup-${finish})`} />
          <rect className="hp-rim" x="24.5" y="98.5" width="41" height="77" rx="20.5" fill="none" />
          <rect x="38" y="90" width="16" height="14" rx="4" fill="var(--hp-band)" />
          <rect x="38" y="99" width="16" height="1.6" fill="var(--hp-trim)" />
        </g>
        <g className="hp-cup hp-cup-right">
          <rect x="165" y="106" width="17" height="62" rx="8.5" fill="var(--hp-pad)" />
          <rect x="174" y="98" width="42" height="78" rx="21" fill={`url(#hp-cup-${finish})`} />
          <rect className="hp-rim" x="174.5" y="98.5" width="41" height="77" rx="20.5" fill="none" />
          <rect x="186" y="90" width="16" height="14" rx="4" fill="var(--hp-band)" />
          <rect x="186" y="99" width="16" height="1.6" fill="var(--hp-trim)" />
        </g>

        {entrance && (
          <g className="hp-waves" fill="none" strokeLinecap="round" strokeWidth="2.5">
            <path className="hp-wave hp-wave-left" d="M16 116 A30 30 0 0 0 16 158" />
            <path className="hp-wave hp-wave-left hp-wave-far" d="M7 106 A44 44 0 0 0 7 168" />
            <path className="hp-wave hp-wave-right" d="M224 116 A30 30 0 0 1 224 158" />
            <path className="hp-wave hp-wave-right hp-wave-far" d="M233 106 A44 44 0 0 1 233 168" />
          </g>
        )}
      </svg>
      {children && <div className="hp-inner">{children}</div>}
    </div>
  );
}

/** Counts up to `target` in step with the gauge filling. */
export function useCountUp(target: number | null, entrance: boolean): number | null {
  const [value, setValue] = useState<number | null>(entrance ? 0 : target);

  useEffect(() => {
    if (target === null) {
      setValue(null);
      return;
    }
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!entrance || reduced) {
      setValue(target);
      return;
    }
    setValue(0);
    let frame = 0;
    const startAt = performance.now() + GAUGE_DELAY_MS;
    const tick = (now: number) => {
      const t = Math.min(1, Math.max(0, (now - startAt) / GAUGE_FILL_MS));
      // Same ease-out curve as the gauge's CSS transition.
      setValue(Math.round(target * (1 - Math.pow(1 - t, 3))));
      if (t < 1) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [target, entrance]);

  return value;
}
