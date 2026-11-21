# Your own 3D model on the connect card

The connect card shows a 3D model of the headphones. A few models of headphones have one made
for them that ships with the app; the rest get a plain one generated in code. You can replace
either with any model of your own by placing two files in
`%AppData%\dev.openheadphonesconnect.desktop\`:

- `model.glb`: the model, as binary glTF (Meshopt compression and WebP textures are supported).
- `model.json` (optional): how to place it.

```json
{
  "rotation": [0, -46.3, 0],
  "scale": 1,
  "roughness": 0.62,
  "brightness": 1.3,
  "tint": [0.88, 0.94, 1.0],
  "cutout": ["lambert6"]
}
```

If you installed the app from the Microsoft Store, the folder is
`%LocalAppData%\Packages\17085Yehia.OpenHeadphonesConnect_86j4vst6mgpyr\LocalState\` instead,
here and everywhere below.

| Setting | What it does |
| --- | --- |
| `rotation` | Degrees about X, Y and Z, to stand the model upright facing the viewer |
| `scale` | Larger or smaller than the automatic framing; 1 leaves it alone |
| `roughness` | Surface roughness, 0 (glossy) to 1 (matte) |
| `brightness` | Lighting strength; 1 leaves it alone |
| `tint` | Red, green and blue multipliers applied to every surface |
| `cutout` | Names of materials whose texture is transparent apart from printed labels |

Restart the app after changing either file. The same model is used for the small picture in the
panel header. The colour option in settings applies to the built-in model only.

## A different model for each pair of headphones

`model.glb` is shown for whatever connects. To give one model of headphones its own picture,
add a `models` folder next to it and name the files after the headphones, exactly as the app
shows their model name:

```
%AppData%\dev.openheadphonesconnect.desktop\
  model.glb                  shown for everything else
  model.json
  models\
    WH-1000XM5.glb           shown when a WH-1000XM5 connects
    WH-1000XM5.json
    WH-1000XM6.glb
```

Each file can have its own `.json` beside it. The most specific model wins:

1. your file in `models` named after the connected headphones,
2. the model the app ships for those headphones, if it has one,
3. your `model.glb`,
4. the generated model.

A large model is worth slimming first, which also makes the card appear sooner:

```powershell
npx @gltf-transform/cli optimize in.gltf model.glb --compress meshopt --simplify-ratio 0.2 --simplify-error 0.0004 --texture-compress webp --texture-size 1024 --palette false
```

The model is read from your settings folder at run time. It is never copied into the app or
this repository, so what you use there is between you and the model's author: check its licence
before sharing a build that depends on it. While developing, the same files in `local/`
(git-ignored), including `local/models/`, are picked up by `npm run dev` and `npm run app:dev`.
