import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { backend, simulator } from "./backend";
import { Card } from "./Card";
import { Headphones3D, prepareHeadphones3D } from "./Headphones3D";
import { Panel } from "./Panel";
import {
  EMPTY_SNAPSHOT,
  headsetOf,
  type Accent,
  type SettingsView,
  type ShowRequest,
  type Snapshot,
} from "./types";

const DEFAULT_SETTINGS: SettingsView = {
  showConnectCard: true,
  lowBatteryAlert: true,
  finish: "black",
  startWithWindows: false,
  version: "",
};

// What the card shows when previewed from settings with nothing connected.
const SAMPLE: Snapshot = {
  ...EMPTY_SNAPSHOT,
  phase: "connected",
  deviceName: "WH-1000XM3",
  battery: { single: { level: 80, charging: false }, left: null, right: null, case: null },
  noise: { mode: "noise_cancelling", ambientLevel: 10, focusOnVoice: false },
};

export function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(EMPTY_SNAPSHOT);
  const [settings, setSettings] = useState<SettingsView>(DEFAULT_SETTINGS);
  const [accent, setAccent] = useState<Accent | null>(null);
  const [request, setRequest] = useState<ShowRequest | null>(null);
  // The nonce of the request the window is currently showing.
  const [shown, setShown] = useState<number | null>(null);
  const surface = useRef<HTMLDivElement>(null);
  // What the card is about to show, for the effect that waits on its 3D model.
  const shownOnCard = useRef<Snapshot>(EMPTY_SNAPSHOT);

  useEffect(() => {
    const state = backend.getState();
    state.then(setSnapshot);
    backend.getSettings().then(async (loaded) => {
      setSettings(loaded);
      // Get the 3D headphones ready before they are first needed. (Not in the
      // browser-only model viewer, which picks its own colour.)
      if (!new URLSearchParams(location.search).has("model")) {
        prepareHeadphones3D(loaded.finish, headsetOf(await state));
      }
    });
    backend.getAccent().then(setAccent);
    const stops = [
      backend.onState(setSnapshot),
      backend.onShow((next) => {
        // Settings may have changed since the page loaded (another launch).
        backend.getSettings().then(setSettings);
        setRequest(next);
      }),
      backend.onHidden(() => setShown(null)),
    ];
    backend.ready();
    return () => stops.forEach((stop) => stop());
  }, []);

  // Once the requested view is laid out, tell the app how tall it is so the
  // window can be sized and slid into place.
  useLayoutEffect(() => {
    if (!request || !surface.current) return;
    let current = true;
    const present = () => {
      if (!current || !surface.current) return;
      const height = Math.ceil(surface.current.getBoundingClientRect().height);
      backend.present(request.mode, request.nonce, height).then((accepted) => {
        if (current && accepted) setShown(request.nonce);
      });
    };
    if (request.mode === "card") {
      // The card waits for its 3D headphones, but not for ever.
      const patience = new Promise((resolve) => setTimeout(resolve, 4000));
      void Promise.race([prepareHeadphones3D(settings.finish, headsetOf(shownOnCard.current)), patience]).then(present);
    } else {
      present();
    }
    return () => {
      current = false;
    };
    // `settings.finish` is read when the request arrives; a later change of
    // colour must not present the same request twice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);

  // Keep the window fitted while its content grows or shrinks.
  const visible = request !== null && shown === request.nonce;
  useEffect(() => {
    if (!visible || !surface.current) return;
    const observer = new ResizeObserver(([entry]) => {
      backend.resize(Math.ceil(entry.target.getBoundingClientRect().height));
    });
    observer.observe(surface.current);
    return () => observer.disconnect();
  }, [visible]);

  const style = accent
    ? ({ "--accent-on-dark": accent.onDark, "--accent-on-light": accent.onLight } as CSSProperties)
    : undefined;
  const cardSnapshot = request?.preview && snapshot.phase !== "connected" ? SAMPLE : snapshot;
  shownOnCard.current = cardSnapshot;

  // Browser-only: `?model=black&yaw=-0.5` shows the 3D model large and still.
  const inspect = backend.simulated ? new URLSearchParams(location.search) : null;
  if (inspect?.has("model")) {
    const yaw = inspect.get("yaw");
    return (
      <div className="stage stage-simulated">
        <Headphones3D
          className="inspect"
          finish={inspect.get("model") === "silver" ? "silver" : "black"}
          headset={inspect.get("headset")}
          play
          fixedYaw={yaw === null ? undefined : Number(yaw)}
          onUnavailable={() => {}}
        />
      </div>
    );
  }

  return (
    <div className={`stage ${backend.simulated ? "stage-simulated" : ""}`} style={style}>
      {simulator && <SimulatorBar />}
      <div ref={surface} className="surface">
        {request?.mode === "card" && (
          <Card key={request.nonce} snapshot={cardSnapshot} finish={settings.finish} visible={visible} />
        )}
        {request?.mode === "panel" && (
          <Panel
            snapshot={snapshot}
            settings={settings}
            onSettings={(next) => {
              // A new colour means a new model; build it before the next card.
              if (next.finish !== settings.finish) prepareHeadphones3D(next.finish);
              setSettings(next);
            }}
          />
        )}
      </div>
    </div>
  );
}

/** Browser-only controls for trying each state without headphones. */
function SimulatorBar() {
  const sim = simulator!;
  const off = { ...EMPTY_SNAPSHOT, deviceName: "WH-1000XM3" };
  const actions: Array<[string, () => void]> = [
    ["Connect card", () => (sim.set(sim.connected), sim.show("card"))],
    ["Panel", () => (sim.set(sim.connected), sim.show("panel"))],
    ...sim.models.map(([label, model]): [string, () => void] => [label, () => (sim.set(model), sim.show("panel"))]),
    ["Low battery", () => sim.set({ battery: { ...sim.connected.battery, single: { level: 14, charging: false } } })],
    ["Charging", () => sim.set({ battery: { ...sim.connected.battery, single: { level: 46, charging: true } } })],
    ["Disconnected", () => (sim.set({ ...off, phase: "disconnected" }), sim.show("panel"))],
    ["Not paired", () => (sim.set({ ...EMPTY_SNAPSHOT, phase: "no_device" }), sim.show("panel"))],
    ["Bluetooth off", () => (sim.set({ ...EMPTY_SNAPSHOT, phase: "bluetooth_off" }), sim.show("panel"))],
  ];
  useEffect(() => {
    // Deferred so the app's own listeners (registered in a parent effect,
    // which runs after this one) are in place first.
    const wanted = new URLSearchParams(location.search).get("view");
    const timer = setTimeout(() => sim.show(wanted === "panel" ? "panel" : "card"), 50);
    return () => clearTimeout(timer);
  }, [sim]);
  return (
    <nav className="simulator" aria-label="Simulator">
      <span>Simulated headphones</span>
      {actions.map(([label, run]) => (
        <button key={label} type="button" onClick={run}>
          {label}
        </button>
      ))}
    </nav>
  );
}
