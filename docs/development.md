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
cd core; cargo test                # protocol and session tests; these run on any system
cd src-tauri; cargo test --lib     # tray icon, settings and app tests
```

If your system drive is short of space, or the machine's Windows SDK is incomplete, dot-source
`scripts\env.ps1` first; the comments at its top explain what it can do.

## The Microsoft Store package

The Store version is the same program inside an MSIX package:

```powershell
.\packaging\build-msix.ps1             # builds the app and the .msix to upload
.\packaging\build-msix.ps1 -Register   # also installs it on this PC to try (needs Developer Mode)
.\packaging\build-msix.ps1 -Unregister # removes that test install
```

The package is not signed; the Store signs it. Its identity comes from
`packaging\store-identity.json` (git-ignored; copy `store-identity.example.json` and fill in the
values from Partner Center > Product identity). Quit the ordinary app before trying the
packaged one, as only one copy runs at a time.

Two things differ inside the package, both in `src-tauri/src/startup.rs`: starting with Windows
goes through a startup task declared in `packaging/AppxManifest.xml`, and settings, your own 3D
models and the log live in `%LocalAppData%\Packages\<package family>\LocalState\`.

### Sending a version to the Store

`.github/workflows/store.yml` does this without anyone opening Partner Center. When a GitHub
release is published, it builds the package for x64 and for ARM64 from that release's tag, puts
the two into one bundle and submits it with Microsoft's
[Store Developer CLI](https://learn.microsoft.com/windows/apps/publish/msstore-dev-cli/overview).
Certification then takes its usual time. Run by hand from the Actions tab, it leaves the upload
as a draft in Partner Center unless told to send it. Either way it replaces a submission that
was started in Partner Center and not finished.

It needs five repository secrets (Settings > Secrets and variables > Actions):

| Secret | What it is |
| --- | --- |
| `PARTNER_CENTER_TENANT_ID` | The Microsoft Entra tenant linked to the Partner Center account |
| `PARTNER_CENTER_CLIENT_ID` | The Entra app registration that has the Manager role in Partner Center |
| `PARTNER_CENTER_CLIENT_SECRET` | A client secret of that app registration. It expires; renew it before it does |
| `PARTNER_CENTER_SELLER_ID` | The Seller ID from Partner Center's account settings |
| `STORE_IDENTITY_JSON` | The contents of `packaging\store-identity.json` |

Microsoft's [guide](https://learn.microsoft.com/windows/apps/publish/msstore-dev-cli/github-actions)
walks through creating the first four.

## Windows on ARM

The app builds natively for ARM PCs as well (`aarch64-pc-windows-msvc`). The GitHub build does
that on an ARM machine with every push: it runs the tests there, starts the app in demo mode to
see the connect card come up, and keeps the installer and the program as the run's
`windows-arm64` artifact. A release ships that installer for ARM.

To build it on your own PC you need Visual Studio's "MSVC ARM64 build tools" component and
`rustup target add aarch64-pc-windows-msvc`:

```powershell
npx tauri build --target aarch64-pc-windows-msvc    # the installer
.\packaging\build-msix.ps1 -Arch arm64              # the Store package
```

The Store package can also be made from a program built elsewhere, which is how the GitHub
build's program gets your own Store identity:

```powershell
.\packaging\build-msix.ps1 -Arch arm64 -Exe .\open-headphones-connect.exe
```

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
core/                   everything that is not tied to one operating system
  src/protocol/framing.rs   packet framing, escaping, checksum
  src/protocol/v1.rs        commands for the WH-1000XM3 generation
  src/protocol/v2.rs        commands for the XM5 generation
  src/protocol/fixtures/    recorded sessions of real headsets, replayed by the tests
  src/session.rs            connection state machine
  src/platform.rs           what the session needs from the system's Bluetooth
  src/models.rs             what a headset's name alone tells
  tests/                    the whole session run against a recorded WH-1000XM3
src-tauri/src/          the Windows app
  bluetooth/mod.rs      finding the paired headset
  bluetooth/link.rs     the RFCOMM control socket
  bluetooth/probe.rs    the --probe diagnostic
  bluetooth/audio.rs    connect / disconnect audio
  flyout.rs             window placement and slide-in
  tray.rs               tray icon drawing
  lib.rs                app wiring and commands
macos/                  the Mac app so far, and notes for the rest (macos/README.md)
  app/                  the menu-bar item, its panel and the connect card, in SwiftUI
  probe/                the same kind of diagnostic as --probe
  bridge/               the session behind a C interface, and the Swift that gives it IOBluetooth
  session/              runs the session through the bridge and prints what it reports
  run.sh                builds and runs the app or either tool, and packages the app
```

`core` knows nothing about Windows. The session reaches the headset through three small traits
in `core/src/platform.rs`, which the Windows app implements in `src-tauri/src/bluetooth/`.
`core/tests/recorded_headset.rs` is the shortest complete example of them.

## Two things that cost time to find out

- The XM3 keeps its control service out of the public Bluetooth service list, so the WinRT
  RFCOMM API cannot find it. The app connects with a plain Bluetooth socket instead, which asks
  the headset for that service by id.
- Asked a second question before it has answered the first, the XM3 drops the first answer
  without a word. The app therefore waits for each answer before asking the next question.

## Contributing

Reports from other headphone models come first: see [Supported headphones](headphones.md). For
code, open an issue before a large change so the direction can be agreed; small fixes can go
straight to a pull request. The two test commands above and `npx tsc --noEmit` should pass; the
GitHub build runs all three, and the `core` tests on macOS as well.

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
