import { useEffect, useRef, useState } from "react";
import { backend } from "./backend";
import { BatteryGlyph, Glyph, IconButton } from "./controls";
import { Headphones, useCountUp } from "./Headphones";
import { Headphones3D } from "./Headphones3D";
import { headlineBattery, type Finish, type Snapshot } from "./types";

const STAY_MS = 7000;
const LEAVE_MS = 160;

function status(snapshot: Snapshot): string {
  const noise = snapshot.noise;
  if (!noise) return "Connected";
  switch (noise.mode) {
    case "noise_cancelling":
      return "Noise cancelling is on";
    case "wind":
      return "Wind noise reduction is on";
    case "ambient":
      // Some models have ambient sound without a level.
      return snapshot.maxAmbientLevel > 0
        ? `Ambient sound, level ${noise.ambientLevel - snapshot.minAmbientLevel + 1}`
        : "Ambient sound is on";
    case "off":
      return "Noise control is off";
  }
}

/**
 * The card that slides up when the headphones connect: their name, a 3D
 * model that spins into place, then the battery level and noise mode.
 */
export function Card(props: { snapshot: Snapshot; finish: Finish; visible: boolean }) {
  const { snapshot, finish, visible } = props;
  const battery = headlineBattery(snapshot);
  const percent = useCountUp(battery?.level ?? null, visible);
  const [leaving, setLeaving] = useState(false);
  // Falls back to the flat drawing where 3D is not available.
  const [flat, setFlat] = useState(false);
  const hovering = useRef(false);

  const leave = (then: () => void) => {
    setLeaving(true);
    setTimeout(then, LEAVE_MS);
  };

  // Stay for a few seconds, longer if the pointer is resting on the card.
  useEffect(() => {
    if (!visible) return;
    let timer: ReturnType<typeof setTimeout>;
    const arm = (delay: number) => {
      timer = setTimeout(() => {
        if (hovering.current) arm(1200);
        else leave(() => backend.dismiss());
      }, delay);
    };
    arm(STAY_MS);
    return () => clearTimeout(timer);
  }, [visible]);

  return (
    <div
      className={`card ${visible ? "card-in" : ""} ${leaving ? "card-leaving" : ""}`}
      role="button"
      tabIndex={0}
      title="Open controls"
      onPointerEnter={() => (hovering.current = true)}
      onPointerLeave={() => (hovering.current = false)}
      onClick={() => backend.openPanel()}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") backend.openPanel();
        if (event.key === "Escape") backend.dismiss();
      }}
    >
      <IconButton
        glyph="close"
        label="Close"
        className="card-close"
        onClick={() => leave(() => backend.dismiss())}
      />

      <h1 className="card-name">{snapshot.deviceName ?? "Sony headphones"}</h1>

      <div className="card-stage">
        {flat ? (
          <Headphones
            key={visible ? "playing" : "waiting"}
            className="card-flat"
            finish={finish}
            level={battery?.level ?? null}
            charging={battery?.charging}
            entrance={visible}
          />
        ) : (
          <Headphones3D className="card-model" finish={finish} play={visible} onUnavailable={() => setFlat(true)} />
        )}
        <div className="card-shadow" />
      </div>

      <div className="card-details">
        {battery && percent !== null && (
          <p className="card-battery" aria-label={`Battery ${battery.level}%`}>
            <BatteryGlyph battery={{ ...battery, level: percent }} />
            <span>{percent}%</span>
            {battery.charging && <span className="card-charging">Charging</span>}
          </p>
        )}
        <p className="card-status">
          <Glyph name="bluetooth" className="card-status-glyph" />
          {status(snapshot)}
        </p>
      </div>
    </div>
  );
}
