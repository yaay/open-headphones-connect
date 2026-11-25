// Writes the 3D models the Mac app ships, in the format it can load (USDZ), from the same
// sources the Windows app draws: the model built in src/headphoneModel.ts for now.
//
//     npm ci && node macos/app/models/export.ts
//
// The results are committed, so the Mac app builds without Node. Run this again, on a Mac (it
// uses the USD tools macOS comes with), when the model changes.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import * as THREE from "three";
import { USDZExporter } from "three/examples/jsm/exporters/USDZExporter.js";
import { buildHeadphones } from "../../../src/headphoneModel.ts";

const model = buildHeadphones("black").object;
model.updateMatrixWorld(true);

// USDZ has no mirrored parts, and the left ear is the right one mirrored. So every part's
// placement goes into its shape instead, with a mirrored part's triangles turned back the
// right way round.
const scene = new THREE.Scene();
model.traverse((part) => {
  if (!(part instanceof THREE.Mesh)) return;
  const shape = part.geometry.clone().applyMatrix4(part.matrixWorld);
  if (!shape.index) shape.setIndex([...Array(shape.attributes.position.count).keys()]);
  if (part.matrixWorld.determinant() < 0) {
    const corners = shape.index!;
    for (let i = 0; i < corners.count; i += 3) {
      const first = corners.getX(i);
      corners.setX(i, corners.getX(i + 2));
      corners.setX(i + 2, first);
    }
  }
  scene.add(new THREE.Mesh(shape, part.material));
});

// The exporter says, once for every material, that it cannot write double-sided ones.
const warn = console.warn;
console.warn = () => {};
const exported = await new USDZExporter().parseAsync(scene);
console.warn = warn;

// So that is added here: the model's surfaces are drawn from both sides (see `side` in
// headphoneModel.ts), and USD does have a word for it. The file also goes from text to
// binary on the way, a quarter of the size.
const work = mkdtempSync(join(tmpdir(), "ohc-model-"));
const at = (name: string) => join(work, name);
writeFileSync(at("exported.usdz"), exported);
execFileSync("usdcat", ["--flatten", at("exported.usdz"), "-o", at("flat.usda")]);
const flat = readFileSync(at("flat.usda"), "utf8");
const meshes = /def Mesh "[^"]*"\s*\{/g;
writeFileSync(at("flat.usda"), flat.replace(meshes, (open) => `${open}\n uniform bool doubleSided = 1`));
execFileSync("usdcat", [at("flat.usda"), "-o", at("headphones.usdc")]);
const file = fileURLToPath(new URL("headphones.usdz", import.meta.url));
rmSync(file, { force: true });
execFileSync("usdzip", [file, "headphones.usdc"], { cwd: work });
rmSync(work, { recursive: true });
console.log(`${file}: ${flat.match(meshes)?.length} parts, ${readFileSync(file).byteLength} bytes`);
