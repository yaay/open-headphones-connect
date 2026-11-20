// A 3D model of the WH-1000XM3, built from code and proportioned against
// photographs of the real headphones:
//
//   * a padded crown cushion that ends in a metal collar on each side,
//   * flat plastic arms running down from the collars to the folding hinge,
//   * the broad hanger plate below each hinge, which sweeps into a crescent
//     arm around the front of the ear cup,
//   * egg-shaped cup housings with a flat, slightly raised touch disc,
//   * a copper-rimmed microphone slot above the disc,
//   * thick ear pads, and cups that splay outward at the top when not worn.
//
// Units are roughly centimetres. Each side is built for the right ear and
// mirrored for the left. +X is outward from the right ear, +Y up, +Z forward.

import * as THREE from "three";
import { MeshoptDecoder } from "three/examples/jsm/libs/meshopt_decoder.module.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import type { Finish, ModelSettings } from "./types";

const rad = (degrees: number) => (degrees * Math.PI) / 180;
const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const smooth = (t: number) => {
  const c = Math.min(1, Math.max(0, t));
  return c * c * (3 - 2 * c);
};

// ----- sweeping a cross-section along a path ---------------------------------

interface Ring {
  p: THREE.Vector3;
  /** Thickness direction. */
  n: THREE.Vector3;
  /** Width direction. */
  b: THREE.Vector3;
  halfWidth: number;
  halfThickness: number;
}

/** Adds a rounded nose to both ends of a strip, so it does not end in a flat cut. */
function roundEnds(rings: Ring[]): Ring[] {
  const nose = (end: Ring, next: Ring): Ring[] => {
    const along = end.p.clone().sub(next.p).normalize();
    const extra: Ring[] = [];
    for (let j = 1; j <= 6; j++) {
      const angle = (j / 6) * (Math.PI / 2);
      const shrink = end.halfThickness * (1 - Math.cos(angle));
      extra.push({
        ...end,
        p: end.p.clone().addScaledVector(along, end.halfThickness * Math.sin(angle)),
        halfWidth: Math.max(0.01, end.halfWidth - shrink),
        halfThickness: Math.max(0.01, end.halfThickness - shrink),
      });
    }
    return extra;
  };
  const last = rings.length - 1;
  return [...nose(rings[0], rings[1]).reverse(), ...rings, ...nose(rings[last], rings[last - 1])];
}

/**
 * Sweeps a rounded-rectangle cross-section through a series of rings.
 * `squareness` 2 is an ellipse; higher values approach a sharp rectangle.
 */
function sweep(rings: Ring[], around = 32, squareness = 4): THREE.BufferGeometry {
  const positions: number[] = [];
  const indices: number[] = [];
  const power = 2 / squareness;
  for (const ring of rings) {
    for (let k = 0; k < around; k++) {
      const angle = (k / around) * Math.PI * 2;
      const c = Math.cos(angle);
      const s = Math.sin(angle);
      const x = ring.halfWidth * Math.sign(c) * Math.abs(c) ** power;
      const y = ring.halfThickness * Math.sign(s) * Math.abs(s) ** power;
      positions.push(
        ring.p.x + ring.b.x * x + ring.n.x * y,
        ring.p.y + ring.b.y * x + ring.n.y * y,
        ring.p.z + ring.b.z * x + ring.n.z * y,
      );
    }
  }
  for (let i = 0; i < rings.length - 1; i++) {
    for (let k = 0; k < around; k++) {
      const a = i * around + k;
      const b = i * around + ((k + 1) % around);
      indices.push(a, a + around, b, b, a + around, b + around);
    }
  }
  for (const [ring, flip] of [[0, false], [rings.length - 1, true]] as const) {
    const centre = positions.length / 3;
    positions.push(rings[ring].p.x, rings[ring].p.y, rings[ring].p.z);
    for (let k = 0; k < around; k++) {
      const a = ring * around + k;
      const b = ring * around + ((k + 1) % around);
      if (flip) indices.push(centre, a, b);
      else indices.push(centre, b, a);
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

const Z = new THREE.Vector3(0, 0, 1);

// ----- the headband ----------------------------------------------------------

// The band follows an ellipse, seen from the front. Angles are measured from
// the crown: 0 at the top, positive toward the right ear.
const BAND_RX = 9.0;
const BAND_RY = 9.3;
const BAND_Y = 6.0;
const CUSHION_END = rad(51); // where the padded crown meets the collar
const COLLAR_END = rad(58);
const ARM_END = rad(95); // the folding hinge

function bandRing(phi: number, inset: number, halfWidth: number, halfThickness: number): Ring {
  const a = Math.PI / 2 - phi;
  const tangent = new THREE.Vector3(-BAND_RX * Math.sin(a), BAND_RY * Math.cos(a), 0).normalize();
  const outward = new THREE.Vector3(tangent.y, -tangent.x, 0);
  return {
    p: new THREE.Vector3(BAND_RX * Math.cos(a), BAND_Y + BAND_RY * Math.sin(a), 0).addScaledVector(outward, -inset),
    n: outward,
    b: Z,
    halfWidth,
    halfThickness,
  };
}

/** The leatherette-wrapped pad across the top of the head. */
function crownCushion(): THREE.BufferGeometry {
  const rings: Ring[] = [];
  const steps = 64;
  for (let i = 0; i <= steps; i++) {
    const phi = mix(-CUSHION_END, CUSHION_END, i / steps);
    // Slightly fuller in the middle than where it tucks into the collars.
    const fullness = 1 - 0.14 * (Math.abs(phi) / CUSHION_END) ** 2;
    rings.push(bandRing(phi, 0.36, 1.8 * fullness, 0.68 * fullness));
  }
  return sweep(rings, 40, 3.2);
}

/** The metal collar between the cushion and an arm. */
function collar(): THREE.BufferGeometry {
  const rings: Ring[] = [];
  for (let i = 0; i <= 6; i++) {
    rings.push(bandRing(mix(CUSHION_END - rad(0.6), COLLAR_END, i / 6), 0.2, 1.62, 0.5));
  }
  return sweep(rings, 40, 7);
}

/** The flat arm from the collar down to the hinge. */
function arm(): THREE.BufferGeometry {
  const rings: Ring[] = [];
  const steps = 30;
  for (let i = 0; i <= steps; i++) {
    const u = i / steps;
    const phi = mix(COLLAR_END - rad(0.6), ARM_END, u);
    rings.push(bandRing(phi, 0.06, mix(1.5, 1.42, u), mix(0.36, 0.32, u)));
  }
  return sweep(rings, 36, 6);
}

/** The knuckle where the arm folds, sitting on top of the hanger plate. */
function hinge(): THREE.BufferGeometry {
  const rings: Ring[] = [];
  for (let i = 0; i <= 8; i++) {
    const u = i / 8;
    rings.push(bandRing(mix(ARM_END - rad(1.5), ARM_END + rad(6), u), 0.02, 1.46, 0.3 + 0.16 * Math.sin(Math.PI * u)));
  }
  return sweep(roundEnds(rings), 36, 5);
}

// ----- an ear cup ------------------------------------------------------------

const CUP_RY = 4.85; // half-height of the housing
const CUP_RZ = 4.35; // half-width, front to back
const CUP_DEPTH = 2.45;
const DISC_Y = -0.62; // the touch disc sits low in the housing
const DISC_RY = 3.36;
const DISC_RZ = 3.3;

/** The housing narrows a little toward the top, like an egg. */
const egg = (sine: number) => 1 - 0.09 * smooth(sine);

/** A point on the housing outline, scaled by `f`. 0 is front, 90 top, 180 back. */
function outline(degrees: number, f = 1): [number, number] {
  const a = rad(degrees);
  const sine = Math.sin(a);
  return [f * CUP_RZ * Math.cos(a) * egg(sine), f * CUP_RY * sine];
}

/** The cup housing: an egg-shaped puck with a flat face and a soft shoulder. */
function housing(): THREE.BufferGeometry {
  const d = CUP_DEPTH;
  const profile = new THREE.SplineCurve([
    new THREE.Vector2(0, d),
    new THREE.Vector2(0.6, d),
    new THREE.Vector2(0.8, d * 0.975),
    new THREE.Vector2(0.93, d * 0.86),
    new THREE.Vector2(0.99, d * 0.62),
    new THREE.Vector2(1.0, d * 0.3),
    new THREE.Vector2(0.985, 0.04),
    new THREE.Vector2(0.93, -0.1),
  ]).getPoints(56);
  profile[0].x = 0;
  const geometry = new THREE.LatheGeometry(profile, 88);
  geometry.rotateZ(-Math.PI / 2); // spin axis: up -> outward
  const position = geometry.getAttribute("position");
  for (let i = 0; i < position.count; i++) {
    const y = position.getY(i);
    const z = position.getZ(i);
    const sine = Math.max(0, y); // the radius is still 1 here
    position.setY(i, y * CUP_RY);
    position.setZ(i, z * CUP_RZ * egg(sine));
  }
  geometry.computeVertexNormals();
  return geometry;
}

/** The touch panel: a flat disc standing slightly proud of the housing. */
function touchDisc(): THREE.BufferGeometry {
  const profile = [
    new THREE.Vector2(0, 0.2),
    new THREE.Vector2(0.955, 0.2),
    new THREE.Vector2(0.985, 0.17),
    new THREE.Vector2(1.0, 0.1),
    new THREE.Vector2(1.0, -0.3),
  ];
  const geometry = new THREE.LatheGeometry(profile, 88);
  geometry.rotateZ(-Math.PI / 2);
  geometry.scale(1, DISC_RY, DISC_RZ);
  geometry.computeVertexNormals();
  return geometry;
}

/**
 * The hanger: the broad plate under the hinge and the crescent arm it becomes,
 * wrapping from the top of the cup around the front to just below the middle.
 */
function hanger(): THREE.BufferGeometry {
  const shape = new THREE.Shape();
  // Shape coordinates are (-z, y): the extrusion is then turned to face outward.
  const on = (degrees: number, f: number) => {
    const [z, y] = outline(degrees, f);
    return new THREE.Vector2(-z, y);
  };
  const NECK_Y = CUP_RY + 2.1;
  const NECK_HALF = 1.55;
  const INNER = 0.83;

  shape.moveTo(-NECK_HALF, NECK_Y);
  // Front edge of the plate, flaring out to meet the cup's outline.
  const flare = on(52, 1);
  shape.bezierCurveTo(-NECK_HALF, NECK_Y - 1.3, -1.9, CUP_RY + 0.45, flare.x, flare.y);
  for (let a = 49; a >= -30; a -= 3) shape.lineTo(on(a, 1).x, on(a, 1).y);
  // The rounded tip of the crescent.
  const tip = on(-37, 0.9);
  const tipEnd = on(-30, INNER);
  shape.quadraticCurveTo(tip.x, tip.y, tipEnd.x, tipEnd.y);
  // Back up along the inside of the crescent, over the top of the touch disc.
  for (let a = -27; a <= 96; a += 3) shape.lineTo(on(a, INNER).x, on(a, INNER).y);
  // The lower edge of the plate runs out to the back of the cup.
  const sweepMid = on(126, 0.83);
  const sweepEnd = on(152, 1);
  shape.quadraticCurveTo(sweepMid.x, sweepMid.y, sweepEnd.x, sweepEnd.y);
  for (let a = 149; a >= 128; a -= 3) shape.lineTo(on(a, 1).x, on(a, 1).y);
  // Back edge of the plate, up to the neck.
  shape.bezierCurveTo(1.9, CUP_RY + 0.45, NECK_HALF, NECK_Y - 1.3, NECK_HALF, NECK_Y);
  shape.closePath();

  const geometry = new THREE.ExtrudeGeometry(shape, {
    depth: 0.62,
    bevelEnabled: true,
    bevelThickness: 0.1,
    bevelSize: 0.1,
    bevelSegments: 4,
    curveSegments: 10,
  });
  geometry.rotateY(Math.PI / 2); // extrusion: toward the viewer -> outward
  geometry.translate(CUP_DEPTH - 0.52, 0, 0);
  return geometry;
}

/** The microphone opening: a short slot, used for both its rim and its hole. */
function slot(length: number, radius: number, depth: number): THREE.BufferGeometry {
  const shape = new THREE.Shape();
  const half = length / 2 - radius;
  shape.absarc(half, 0, radius, -Math.PI / 2, Math.PI / 2, false);
  shape.absarc(-half, 0, radius, Math.PI / 2, (3 * Math.PI) / 2, false);
  shape.closePath();
  const geometry = new THREE.ExtrudeGeometry(shape, { depth, bevelEnabled: false, curveSegments: 12 });
  geometry.rotateY(Math.PI / 2);
  return geometry;
}

// ----- putting it together ---------------------------------------------------

interface Palette {
  body: number;
  cushion: number;
  cloth: number;
  collar: number;
  accent: number;
}

const PALETTES: Record<Finish, Palette> = {
  black: { body: 0x141617, cushion: 0x101112, cloth: 0x040405, collar: 0x30353a, accent: 0xc47a45 },
  silver: { body: 0xb9b0a1, cushion: 0xbfb6a7, cloth: 0x6a6358, collar: 0x9c9383, accent: 0xb5873f },
};

// The cups hang square to the band, as in the product shots (off the head the
// real ones tip inward on their pivots; that hides the hanger's shape).
const CUP_SPLAY = 0;
// Where the inner face of a cup housing sits, so the hanger's neck lands
// under the hinge at the end of the arm.
const CUP_AT = new THREE.Vector3(6.72, -1.6, 0);
const TOP = BAND_Y + BAND_RY + 0.3;
const BOTTOM = CUP_AT.y - CUP_RY - 0.6;

/** Overall height of the model, for framing it. */
export const MODEL_HEIGHT = TOP - BOTTOM;

export interface HeadphoneModel {
  object: THREE.Group;
  dispose(): void;
}

export function buildHeadphones(finish: Finish): HeadphoneModel {
  const palette = PALETTES[finish];
  const disposables = new Set<{ dispose(): void }>();
  const keep = <T extends { dispose(): void }>(item: T): T => (disposables.add(item), item);

  // Swept and extruded parts are drawn double-sided, so their lighting does
  // not depend on which way the triangles happen to wind.
  const side = THREE.DoubleSide;
  const body = keep(
    new THREE.MeshPhysicalMaterial({
      color: palette.body,
      roughness: 0.56,
      metalness: 0.04,
      clearcoat: 0.12,
      clearcoatRoughness: 0.7,
      side,
    }),
  );
  // The touch disc is the same colour with a slightly silkier surface.
  const disc = keep(
    new THREE.MeshPhysicalMaterial({
      color: palette.body,
      roughness: 0.4,
      metalness: 0.05,
      clearcoat: 0.2,
      clearcoatRoughness: 0.5,
      side,
    }),
  );
  const cushion = keep(
    new THREE.MeshPhysicalMaterial({
      color: palette.cushion,
      roughness: 0.68,
      sheen: 0.55,
      sheenRoughness: 0.5,
      sheenColor: new THREE.Color(finish === "black" ? 0x4c4f52 : 0xfff3df),
      side,
    }),
  );
  const cloth = keep(new THREE.MeshStandardMaterial({ color: palette.cloth, roughness: 0.95, side }));
  const metal = keep(new THREE.MeshStandardMaterial({ color: palette.collar, roughness: 0.34, metalness: 0.9, side }));
  const accent = keep(new THREE.MeshStandardMaterial({ color: palette.accent, roughness: 0.26, metalness: 1, side }));

  const mesh = (geometry: THREE.BufferGeometry, material: THREE.Material) => new THREE.Mesh(keep(geometry), material);

  const root = new THREE.Group();
  root.add(mesh(crownCushion(), cushion));

  const collarGeometry = collar();
  const armGeometry = arm();
  const hingeGeometry = hinge();
  const housingGeometry = housing();
  const discGeometry = touchDisc();
  const hangerGeometry = hanger();
  const padGeometry = new THREE.TorusGeometry(1, 0.38, 32, 88).rotateY(Math.PI / 2);
  const clothGeometry = new THREE.CircleGeometry(0.72, 56).rotateY(-Math.PI / 2);
  const slotRim = slot(1.4, 0.25, 0.1);
  const slotHole = slot(1.14, 0.13, 0.12);

  for (const sideSign of [1, -1]) {
    const half = new THREE.Group();
    half.scale.x = sideSign;
    half.add(mesh(collarGeometry, metal), mesh(armGeometry, body), mesh(hingeGeometry, body));

    const cup = new THREE.Group();
    cup.position.copy(CUP_AT);
    cup.rotation.z = CUP_SPLAY;

    const face = mesh(discGeometry, disc);
    face.position.set(CUP_DEPTH, DISC_Y, 0);

    const pad = mesh(padGeometry, cushion);
    pad.scale.set(3.4, (CUP_RY * 1.02) / 1.38, CUP_RZ / 1.38);
    pad.position.x = -1.15;

    const speaker = mesh(clothGeometry, cloth);
    speaker.scale.set(1, CUP_RY * 0.7, CUP_RZ * 0.68);
    speaker.position.x = -1.0;

    // The microphone slot sits between the top of the disc and the hanger.
    const port = new THREE.Group();
    port.position.set(CUP_DEPTH - 0.02, DISC_Y + DISC_RY + 0.5, 0.75);
    port.rotation.x = rad(-10);
    port.add(mesh(slotRim, accent), mesh(slotHole, cloth));

    // The hanger plate is left blank: the real one carries the maker's name,
    // which is not this project's to reproduce.
    cup.add(mesh(housingGeometry, body), mesh(hangerGeometry, body), face, pad, speaker, port);
    half.add(cup);
    root.add(half);
  }

  // Centre the model on its own height.
  root.position.y = -(TOP + BOTTOM) / 2;
  const object = new THREE.Group();
  object.add(root);

  return { object, dispose: () => disposables.forEach((item) => item.dispose()) };
}

// ----- a model supplied by the user ------------------------------------------

/**
 * Loads a GLB file the user has added and fits it into the same space as the
 * built-in model: centred, and scaled to the same height. `settings` can turn
 * it to face the viewer and adjust how its surfaces look.
 */
export async function loadHeadphones(bytes: ArrayBuffer, settings: ModelSettings = {}): Promise<HeadphoneModel> {
  const loader = new GLTFLoader();
  loader.setMeshoptDecoder(MeshoptDecoder);
  const { scene } = await loader.parseAsync(bytes, "");

  const cutout = new Set(settings.cutout ?? []);
  const materials = new Set<THREE.Material>();
  const geometries = new Set<THREE.BufferGeometry>();
  scene.traverse((node) => {
    const mesh = node as THREE.Mesh;
    if (!mesh.isMesh) return;
    geometries.add(mesh.geometry);
    for (const material of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) {
      materials.add(material);
      const surface = material as THREE.MeshStandardMaterial;
      if (settings.roughness !== undefined && surface.isMeshStandardMaterial) surface.roughness = settings.roughness;
      if (settings.tint && surface.color) surface.color.multiply(new THREE.Color(...settings.tint));
      if (cutout.has(material.name)) {
        // Printed labels: a texture that is see-through everywhere but the ink.
        material.transparent = true;
        material.alphaTest = 0.04;
        material.depthWrite = false;
        mesh.renderOrder = 1;
      }
    }
  });

  const turned = new THREE.Group();
  const [x, y, z] = settings.rotation ?? [0, 0, 0];
  turned.rotation.set(rad(x), rad(y), rad(z));
  turned.add(scene);
  turned.updateMatrixWorld(true);
  const box = new THREE.Box3().setFromObject(turned);
  const size = box.getSize(new THREE.Vector3());
  turned.position.sub(box.getCenter(new THREE.Vector3()));

  const object = new THREE.Group();
  object.scale.setScalar(MODEL_HEIGHT / Math.max(size.y, 1e-6));
  object.add(turned);

  return {
    object,
    dispose() {
      geometries.forEach((geometry) => geometry.dispose());
      materials.forEach((material) => {
        const surface = material as THREE.MeshStandardMaterial;
        surface.map?.dispose();
        surface.normalMap?.dispose();
        surface.roughnessMap?.dispose();
        surface.metalnessMap?.dispose();
        material.dispose();
      });
    },
  };
}
