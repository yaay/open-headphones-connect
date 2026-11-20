import { useEffect, useLayoutEffect, useRef, useState } from "react";
import * as THREE from "three";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { backend } from "./backend";
import { buildHeadphones, loadHeadphones, type HeadphoneModel } from "./headphoneModel";
import type { CustomModel, Finish } from "./types";

/** How long the spin-in takes before the model settles into its resting pose. */
export const SETTLE_MS = 1700;

// The view the model comes to rest in: standing upright, turned a little to
// one side so the outside of one ear cup and the pad of the other both show.
const REST_YAW = 0.62;
const REST_PITCH = 0.06;
// How much of the canvas the resting model may fill. The canvas is taller
// than the card's stage, hence the modest height.
const FILL_HEIGHT = 0.66;
const FILL_WIDTH = 0.74;
const CANVAS_WIDTH = 336;
const CANVAS_HEIGHT = 296;
const FIELD_OF_VIEW = 24;
// It arrives from most of a turn away.
const SPIN = Math.PI * 1.35;

const easeOutQuint = (t: number) => 1 - (1 - t) ** 5;
const easeOutBack = (t: number) => 1 + 2.2 * (t - 1) ** 3 + 1.2 * (t - 1) ** 2;
const clamp01 = (t: number) => Math.min(1, Math.max(0, t));

/**
 * The renderer, lights and model. Setting these up takes around a second
 * (shader compilation, mostly), so one stage is built ahead of time and kept
 * for the life of the app; the card borrows its canvas each time it appears.
 */
interface Stage {
  /** What it shows: a colour of the built-in model, or the user's own model. */
  key: string;
  canvas: HTMLCanvasElement;
  fit(width: number, height: number): void;
  /** Draws the pose for `elapsed` ms into the entrance, or a fixed angle. */
  draw(elapsed: number, fixedYaw?: number): void;
  /** A still picture of the model at rest, as a PNG data URL. */
  portrait(pixels: number): string;
  dispose(): void;
}

interface StageOptions {
  /** Lighting strength; 1 is the default. */
  brightness?: number;
  /** Size of the model in the frame; 1 is the default. */
  zoom?: number;
}

/** Distance at which a box of this size fills the given share of the view. */
function framingDistance(size: THREE.Vector3, aspect: number, fillWidth: number, fillHeight: number): number {
  const reach = Math.tan(THREE.MathUtils.degToRad(FIELD_OF_VIEW / 2));
  const forHeight = size.y / fillHeight / (2 * reach);
  const forWidth = size.x / fillWidth / (2 * reach * aspect);
  return Math.max(forHeight, forWidth) + size.z / 2;
}

function createStage(key: string, look: Finish, model: HeadphoneModel, options: StageOptions = {}): Stage {
  const brightness = options.brightness ?? 1;
  const zoom = options.zoom ?? 1;
  const canvas = document.createElement("canvas");
  const renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = (look === "black" ? 1.05 : 0.8) * brightness;

  const scene = new THREE.Scene();
  const pmrem = new THREE.PMREMGenerator(renderer);
  const studio = new RoomEnvironment();
  const environment = pmrem.fromScene(studio, 0.03);
  scene.environment = environment.texture;
  scene.environmentIntensity = look === "black" ? 0.9 : 0.55;

  // A key light from the upper front left and a rim light from behind, so
  // dark headphones keep a bright edge against a dark card.
  const key_ = new THREE.DirectionalLight(0xffffff, look === "black" ? 3.4 : 1.5);
  key_.position.set(-10, 16, 18);
  const rim = new THREE.DirectionalLight(0xdfe9ff, look === "black" ? 3.4 : 1.0);
  rim.position.set(14, 8, -16);
  scene.add(key_, rim);

  const camera = new THREE.PerspectiveCamera(FIELD_OF_VIEW, 1, 1, 400);

  const pivot = new THREE.Group();
  // Tilt first, then turn: the turn stays about the model's own upright axis.
  pivot.rotation.order = "ZXY";
  pivot.add(model.object);
  scene.add(pivot);

  // Measure the model in its resting pose and place the camera so it fills
  // the same share of the frame whatever its proportions. The spare room
  // above and below keeps it clear of the canvas edge while it rises in.
  pivot.rotation.set(REST_PITCH, REST_YAW, 0, "ZXY");
  pivot.updateMatrixWorld(true);
  const rest = new THREE.Box3().setFromObject(pivot, true);
  const restSize = rest.getSize(new THREE.Vector3());
  const restCentre = rest.getCenter(new THREE.Vector3());
  const frameCard = () => {
    const distance = framingDistance(restSize, camera.aspect, FILL_WIDTH * zoom, FILL_HEIGHT * zoom);
    camera.position.set(restCentre.x, restCentre.y + distance * 0.035, distance);
    camera.lookAt(restCentre.x, restCentre.y, 0);
  };

  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const pose = (elapsed: number, fixedYaw?: number) => {
    pivot.rotation.z = 0;
    if (fixedYaw !== undefined) {
      pivot.rotation.set(REST_PITCH, fixedYaw, 0, "ZXY");
      pivot.position.y = 0;
      pivot.scale.setScalar(1);
      return;
    }
    const t = reducedMotion ? 1 : clamp01(elapsed / SETTLE_MS);
    const turn = easeOutQuint(t);
    const idle = reducedMotion ? 0 : Math.max(0, elapsed - SETTLE_MS) / 1000;
    pivot.rotation.y = REST_YAW - (1 - turn) * SPIN + 0.17 * Math.sin(idle * 0.75);
    pivot.rotation.x = REST_PITCH + (1 - turn) * 0.35 + 0.02 * Math.sin(idle * 0.9 + 1);
    pivot.position.y = -(1 - turn) * 3.2 + 0.1 * Math.sin(idle * 1.1);
    const grow = reducedMotion ? 1 : clamp01(elapsed / (SETTLE_MS * 0.62));
    pivot.scale.setScalar(0.62 + 0.38 * easeOutBack(grow));
  };

  const fit = (width: number, height: number) => {
    if (width <= 0 || height <= 0) return;
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    frameCard();
    camera.updateProjectionMatrix();
  };

  const portrait = (pixels: number) => {
    const before = renderer.getSize(new THREE.Vector2());
    const ratio = renderer.getPixelRatio();
    renderer.setPixelRatio(1);
    renderer.setSize(pixels, pixels, false);
    pose(0, REST_YAW);
    camera.aspect = 1;
    const distance = framingDistance(restSize, 1, 0.94, 0.94);
    camera.position.set(restCentre.x, restCentre.y + distance * 0.035, distance);
    camera.lookAt(restCentre.x, restCentre.y, 0);
    camera.updateProjectionMatrix();
    // A small picture needs a little more light to read clearly.
    const exposure = renderer.toneMappingExposure;
    renderer.toneMappingExposure = exposure * 1.2;
    renderer.render(scene, camera);
    // Read back in the same task as the draw, before the buffer is cleared.
    const picture = canvas.toDataURL("image/png");
    renderer.toneMappingExposure = exposure;
    renderer.setPixelRatio(ratio);
    fit(before.x, before.y);
    return picture;
  };

  // Compile the shaders now, at the size the card uses, rather than on the
  // first visible frame.
  fit(CANVAS_WIDTH, CANVAS_HEIGHT);
  pose(0);
  renderer.compile(scene, camera);
  renderer.render(scene, camera);

  return {
    key,
    canvas,
    fit,
    draw(elapsed, fixedYaw) {
      pose(elapsed, fixedYaw);
      renderer.render(scene, camera);
    },
    portrait,
    dispose() {
      model.dispose();
      environment.dispose();
      studio.dispose();
      pmrem.dispose();
      renderer.dispose();
      renderer.forceContextLoss();
    },
  };
}

let stage: Stage | null = null;
let building: { finish: Finish; done: Promise<Stage | null> } | null = null;
let unavailable = false;
let ownModel: Promise<CustomModel | null> | null = null;

/** The user's own model is fetched once and reused. */
function customModel(): Promise<CustomModel | null> {
  ownModel ??= backend.getCustomModel().catch(() => null);
  return ownModel;
}

async function build(finish: Finish): Promise<Stage | null> {
  let own = await customModel();
  let model: HeadphoneModel | null = null;
  if (own) {
    if (stage?.key === "own") return stage;
    try {
      model = await loadHeadphones(own.bytes, own.settings);
    } catch (error) {
      // A file that cannot be read should not cost the user the whole card.
      console.error("The custom 3D model could not be loaded; using the built-in one", error);
      ownModel = Promise.resolve(null);
      own = null;
    }
  }
  if (!own && stage?.key === finish) return stage;
  try {
    const next = own
      ? createStage("own", "black", model!, { brightness: own.settings.brightness, zoom: own.settings.scale })
      : createStage(finish, finish, buildHeadphones(finish));
    backend.note(
      own
        ? `3D headphones: your own model (${Math.round(own.bytes.byteLength / 1024)} KB)`
        : `3D headphones: built-in, ${finish}`,
    );
    stage?.dispose();
    stage = next;
  } catch (error) {
    console.error("3D is unavailable, using the flat drawing", error);
    stage = null;
    unavailable = true;
  }
  return stage;
}

/**
 * Gets the 3D headphones ready and resolves once they can be shown (with null
 * if this machine cannot draw 3D). Called at startup so the first connect
 * card does not have to wait, and again by anything that needs the stage.
 */
export function prepareHeadphones3D(finish: Finish): Promise<unknown> {
  if (unavailable) return Promise.resolve(null);
  if (building?.finish !== finish) building = { finish, done: build(finish) };
  return building.done;
}

const portraits = new Map<string, string>();

/**
 * A still picture of the headphones at rest, for places too small for the
 * live model. Null if this machine cannot draw 3D.
 */
export async function headphonePortrait(finish: Finish, pixels: number): Promise<string | null> {
  await prepareHeadphones3D(finish);
  if (!stage) return null;
  const name = `${stage.key}@${pixels}`;
  let picture = portraits.get(name);
  if (!picture) {
    picture = stage.portrait(pixels);
    portraits.set(name, picture);
  }
  return picture;
}

interface Props {
  finish: Finish;
  /** Starts the entrance. Until then the first frame is held. */
  play: boolean;
  className?: string;
  /** Called if this machine cannot draw 3D, so a flat drawing can be used. */
  onUnavailable: () => void;
  /** Holds the model still at this angle (radians). For inspecting the model. */
  fixedYaw?: number;
}

/**
 * A live 3D render of the headphones. They spin up from below, settle into a
 * three-quarter view, then keep turning very slightly, as if on a display
 * stand.
 */
export function Headphones3D({ finish, play, className, onUnavailable, fixedYaw }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState<Stage | null>(null);
  const report = useRef(onUnavailable);
  report.current = onUnavailable;

  // Started in a layout effect so that, when the stage is already built, its
  // canvas is in place before the app is told the card can be shown.
  useLayoutEffect(() => {
    let attached: Stage | null = null;
    let cancelled = false;
    void prepareHeadphones3D(finish).then(() => {
      if (cancelled || !host.current) return;
      if (!stage) {
        report.current();
        return;
      }
      attached = stage;
      host.current.appendChild(stage.canvas);
      stage.fit(host.current.clientWidth, host.current.clientHeight);
      stage.draw(0, fixedYaw);
      setShown(stage);
    });
    return () => {
      cancelled = true;
      attached?.canvas.remove();
      setShown(null);
    };
  }, [finish, fixedYaw]);

  useEffect(() => {
    if (!play || !shown) return;
    const startedAt = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      shown.draw(now - startedAt, fixedYaw);
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [play, shown, fixedYaw]);

  return <div ref={host} className={className} aria-hidden="true" />;
}
