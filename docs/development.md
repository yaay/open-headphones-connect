# Building and how it works

Built with Tauri 2: a Rust core that speaks Sony's Bluetooth control protocol, and a small React
UI styled after Windows 11 flyouts.

## Building

Needs Node 20+, Rust (the pinned toolchain installs itself through rustup) and the Visual Studio
C++ build tools with a Windows SDK.

```powershell
npm install
npm run app:dev          # run with live reload
npm run app:build        # release build and installer
```

`npm run dev` on its own opens the UI in a browser with simulated headphones (an XM3, an XM5,
earbuds, and open earbuds without noise control), which is the quick way to work on the design.

The built app has a demo mode too. It shows made-up headphones of whatever model you name,
without touching Bluetooth, so you can see the connect card and the panel for headphones you do
not own. Quit the running app first, then:

```powershell
& "$env:LOCALAPPDATA\Open Headphones Connect\open-headphones-connect.exe" --demo WH-1000XM5
```

Disconnect and Connect in the demo's panel replay the connect card.

```powershell
cd src-tauri; cargo test --lib     # protocol, session, tray icon and settings tests
```

If your system drive is short of space, or the machine's Windows SDK is incomplete, dot-source
`scripts\env.ps1` first; the comments at its top explain what it can do.

## Seeing what a headset says

A debug build has a probe that asks every question directly and prints the raw answers:

```powershell
cd src-tauri
cargo run -- --probe          # read-only: paired devices, services, every query and its reply
cargo run -- --probe-write    # the same, then re-sends the noise mode the headset is already in
```

Starting the app with `OHC_TRACE=1` in the environment writes every message in both directions
to `%LocalAppData%\dev.openheadphonesconnect.desktop\logs\app.log`.

## Layout

```
src/                    React UI
  Card.tsx              the connect card
  Headphones3D.tsx      3D renderer and the spin-in animation
  headphoneModel.ts     the built-in model, generated from code, and the loader for your own
  Panel.tsx             tray panel: noise control, playback, equalizer, settings
  Headphones.tsx        flat drawing, used only where 3D is not available
  backend.ts            bridge to Rust, plus the browser simulator
src-tauri/src/
  protocol/framing.rs   packet framing, escaping, checksum
  protocol/v1.rs        commands for the WH-1000XM3 generation
  protocol/v2.rs        commands for the XM5 generation
  protocol/fixtures/    recorded sessions of real headsets, replayed by the tests
  bluetooth/mod.rs      finding the paired headset
  bluetooth/link.rs     the RFCOMM control socket
  bluetooth/probe.rs    the --probe diagnostic
  bluetooth/audio.rs    connect / disconnect audio
  session.rs            connection state machine
  flyout.rs             window placement and slide-in
  tray.rs               tray icon drawing
  lib.rs                app wiring and commands
```

## Two things that cost time to find out

- The XM3 keeps its control service out of the public Bluetooth service list, so the WinRT
  RFCOMM API cannot find it. The app connects with a plain Bluetooth socket instead, which asks
  the headset for that service by id.
- Asked a second question before it has answered the first, the XM3 drops the first answer
  without a word. The app therefore waits for each answer before asking the next question.

## Contributing

Reports from other headphone models come first: see [Supported headphones](headphones.md). For
code, open an issue before a large change so the direction can be agreed; small fixes can go
straight to a pull request. `cargo test --lib` and `npx tsc --noEmit` should pass, and the
GitHub build runs both.

## Where the protocol knowledge comes from

- [SonyHeadphonesClient](https://github.com/Plutoberth/SonyHeadphonesClient) by Plutoberth (MIT):
  the original reverse engineering of the protocol and the WH-1000XM3 noise-control command.
- [Sony Tray](https://github.com/abhiinhii/sony-tray) by abhiinhii (MIT): the first version of
  the second-generation commands, and the session design.
- [SonyHeadphonesClient](https://github.com/mos9527/SonyHeadphonesClient) by mos9527, Amr Satrio
  and contributors (MIT): the message tables of both generations for capabilities, noise-control
  layouts, playback, the equalizer, DSEE, general settings, Speak-to-Chat, automatic power-off and
  voice guidance, and the recorded WH-1000XM3, WF-1000XM5 and LinkBuds Clip sessions that the
  tests replay.

## The 3D models

- Most headphones get a model generated in code (`src/headphoneModel.ts`), proportioned against
  photos of the WH-1000XM3. It carries no logo.
- The WH-1000XM5 and WH-1000XM6 get [Sony Wh-1000xm5](https://sketchfab.com/3d-models/sony-wh-1000xm5-5d8aea0a780b49fa89c9c205912414e3)
  by Zalaneke, used under CC BY 4.0, converted to glTF with its materials adjusted
  (`src/assets/models/`, listed in `src/bundledModels.ts`). To add a model for other headphones,
  it must come with a licence that allows redistribution; add the file and an entry there, and
  credit its author in `LICENSE`.

No product photographs are bundled.
