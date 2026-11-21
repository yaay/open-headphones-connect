// 3D models that ship with the app, for headphones it has a likeness of.
// Everything else gets the model generated in `headphoneModel.ts`.

import xm5 from "./assets/models/wh-1000xm5.glb?url";
import type { ModelSettings } from "./types";

export interface BundledModel {
  key: string;
  /** Tested against the model name the headphones report. */
  matches: RegExp;
  url: string;
  settings: ModelSettings;
}

export const BUNDLED_MODELS: BundledModel[] = [
  {
    // "Sony Wh-1000xm5" by Zalaneke (https://sketchfab.com/Zalanekee), CC BY 4.0;
    // converted to glTF with its materials adjusted. See LICENSE.
    key: "wh-1000xm5",
    // The WH-1000XM6 is close enough in outline to borrow it.
    matches: /WH-?1000X ?M[56](?!\d)/i,
    url: xm5,
    settings: { rotation: [0, 90, 0], brightness: 1.25 },
  },
];

export function bundledModelFor(headset: string | null): BundledModel | null {
  return (headset && BUNDLED_MODELS.find((model) => model.matches.test(headset))) || null;
}
