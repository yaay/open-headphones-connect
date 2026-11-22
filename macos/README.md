# The Mac app

A first version of the app exists: the menu-bar item, its panel and the connect card, on top
of a bridge that puts the shared session behind Swift. Packaging is still to come. This page
records what has been decided and what is known, so the work can go on without repeating the
research. Only what [The probe](#the-probe), [The bridge](#the-bridge),
[The panel](#the-panel) and [The card](#the-card) say has been tried on macOS.

## What it should be

The same thing the Windows app is: a card with a 3D model of the headphones when they connect,
and a small panel from the menu bar for battery, noise control, the equalizer and the headset's
switches.

- A native Swift app (SwiftUI and AppKit), living in the menu bar. Not the Windows interface in
  a web view.
- Public macOS APIs only. The connect card is the app's own floating panel (a borderless,
  non-activating `NSPanel`), not Apple's system banner: that one sits in a private framework,
  breaks with macOS updates and rules out the Mac App Store.
- The protocol and the session come from `core/`, the Rust crate the Windows app uses, built
  into the Mac app as a static library. One implementation, one set of tests.
- Only artwork with a licence that allows redistribution. The WH-1000XM5 model in
  `src/assets/models/` (CC BY 4.0, credited in `LICENSE`) can be converted for SceneKit or
  RealityKit. No Sony product photographs.

## How `core` plugs in

`core/src/platform.rs` has three traits: `Bluetooth` (is the radio on, find the paired
headset), `Headset` (name, is it connected, open the control channel, connect and disconnect
audio) and `Link` (send bytes, deliver what arrives). `session::spawn` takes a `Bluetooth` and a
callback, runs on its own thread, and calls back with a `Snapshot` whenever anything changes.
`core/tests/recorded_headset.rs` implements all three in about a hundred lines.

Swift reaches all that through the bridge in `macos/bridge/`: see [The bridge](#the-bridge).

## Bluetooth on macOS

Classic Bluetooth serial channels go through IOBluetooth, as both existing Mac clients do:

- `IOBluetoothDevice.pairedDevices()` lists candidates; `core::models::generation_from_name`
  recognises Sony names.
- The control service is found by id: `96CC203E-5068-46AD-B32D-E316F5E069BA` for the
  WH-1000XM3 generation, `956C7B26-D49A-4BA8-B03F-B17D393CB6E2` for the XM5 generation. Look the
  service record up with `getServiceRecord(for:)`, take its channel with
  `getRFCOMMChannelID`, open it with `openRFCOMMChannelAsync(_:withChannelID:delegate:)`.
- Received bytes go through `core::protocol::framing::Reassembler` to become frames.
- `IOBluetoothDevice.register(forConnectNotifications:selector:)` reports a connecting device
  at once, which is what the card should hang on rather than polling.
- The app needs `NSBluetoothAlwaysUsageDescription` in its `Info.plist`, and the
  `com.apple.security.device.bluetooth` entitlement if it is sandboxed.

## To find out first

- Whether a second question sent before the first answer loses that answer on macOS too (the
  session already waits, so this should need nothing).

A sensible order: a command-line probe that opens the channel and prints the reply to one
question (done); then the bridge (done); then the menu-bar panel (a first version); then the
card and its model (a first version); then packaging.
Without an Apple Developer ID the app cannot be notarised, and people who download it have to
allow it by hand in System Settings.

## The probe

`macos/probe/main.swift` is one file that needs nothing but Xcode's command-line tools.
`macos/run.sh` builds it into a throwaway app bundle and runs it:

```sh
macos/run.sh probe                            # macOS asks for Bluetooth after every build
SIGN="Apple Development" macos/run.sh probe   # with your certificate, it asks once
```

It lists the paired devices. For each connected one with a Sony name it says whether macOS has
a record of either control service, before and after asking the headset for its services, and
prints the headset's list. Last, it opens the channel, asks for the protocol info and prints
the raw reply. It changes nothing on the headset.

What it found with a WH-1000XM3 (firmware 4.5.2) on macOS 27:

- The control service is not hidden as it is on Windows. `getServiceRecord(for:)` returns it
  from what macOS already remembers, before any query, and the headset's own list has it too
  (channel 15, no name). The calls under "Bluetooth on macOS" work as written.
- The reply to the protocol info question is `01 00 40 10`, byte for byte what the recorded
  session in `core/src/protocol/fixtures/wh-1000xm3-4.5.2.txt` has.
- `performSDPQuery(_:uuids:)`, which asks for particular services, had not reported back after
  15 seconds the one time it was tried. The plain `performSDPQuery(_:)` did, and is enough.

And about the Bluetooth permission:

- macOS charges a command-line tool's Bluetooth use to the program that started it. If that
  program has no `NSBluetoothAlwaysUsageDescription`, the tool is killed at its first
  IOBluetooth call without a word (exit code 134; the crash report in
  `~/Library/Logs/DiagnosticReports` names the missing key). Hence the bundle.
- The first IOBluetooth call (`IOBluetoothHostController.default()`) does not return until the
  permission question is answered. The app must not make it where a blocked main thread hurts.
- A build signed ad hoc is asked again after every rebuild. Signed with a development
  certificate, the answer holds across rebuilds.

Not tried: a second-generation headset, and running the bare binary from Terminal (whose
`Info.plist` has no usage description either).

## The bridge

`macos/bridge/` has both halves:

- `src/lib.rs`, a Rust crate built as a static library. It implements the three traits by
  calling functions Swift supplies, hands each `Snapshot` over as JSON (the shape in
  `src/types.ts`), and reads commands from JSON: `"connect"`, `{"set_volume": 12}`,
  `{"set_switch": ["upscaling", true]}`. That form is `Command`'s own, in
  `core/src/session.rs`, where a test pins it.
- `ohc_bridge.h`, the C interface between the two, which says who calls what from which
  thread.
- `HeadsetSession.swift`, which supplies those functions over IOBluetooth. The app needs two
  calls: `HeadsetSession.start { json in … }` and `HeadsetSession.send(json)`.

The session calls the traits from its own thread and expects them to block, while IOBluetooth
is used from the main thread and reports back there. So every function Swift supplies hops to
the main thread for the IOBluetooth call, and where it then has to wait for a report (the
channel opening, a service query finishing) it waits on the session's thread. The main thread
never waits for the session.

`macos/session/main.swift` runs it all and prints every snapshot; it needs Rust as well:

```sh
macos/run.sh session                          # for ten seconds
macos/run.sh session 30 '{"set_volume": 12}'  # for thirty, sending that halfway through
OHC_TRACE=1 macos/run.sh session              # with every message to and from the headset
```

With the WH-1000XM3 the session connects and reports what it does on Windows: model, firmware,
battery, noise control, equalizer, codec, volume, playback, switches and automatic power-off.
A command sent through it (the noise mode the headset was already in) reached the headset and
was answered. Disconnect (`closeConnection()`) drops the headset, and Connect
(`openConnection(_:)`) brings it back as the Mac's sound output.

With two Sony headsets paired and neither connected, the first one macOS lists is not
necessarily the one just disconnected, and Connect went to the wrong headset. The bridge now
offers the session the one used last (`recentAccessDate()`).

Not tried:

- The headset going away while the channel is open.
- A headset whose services macOS does not remember, for which the bridge asks first.
- A second-generation headset.

## The panel

`macos/app/` is the app: SwiftUI, for macOS 14 and later.

- `App.swift`: the item in the menu bar (a `MenuBarExtra` in window style, with the battery
  beside the icon as in the Windows tray) and the command-line flags below.
- `Panel.swift`: the panel, section for section what `src/Panel.tsx` is, in stock controls.
- `Headset.swift`: the snapshot as Swift types, and the commands as JSON.

```sh
macos/run.sh app                              # with your headphones
macos/run.sh app --demo WH-1000XM5            # with made-up ones; Bluetooth is left alone
macos/run.sh app --demo WF-1000XM5 --render panel.png [dark]
                                              # a picture of the panel, then quit
```

`--demo` is the core's demo mode, as on Windows. `--render` draws the panel off screen with
everything in it opened up, which is how to look at a change without a headset or a screen.

Every control shows what the session reports and sends a command when changed; the session
shows a change at once, so nothing is kept in the panel itself. A slider follows the pointer
while it is dragged and the headset otherwise.

Drawn with `--render` from a real WH-1000XM3, the panel shows that headset's four noise modes,
equalizer, three switches and automatic power-off, and its controls work on that headset.

Left out on purpose: volume and playback, which a Mac has its own controls for. Not there yet:

- The settings page (start at login, the connect card's switch, the low-battery warning).
- A picture of the headphones in the header, which has a symbol for now.
- Saying so when Bluetooth permission was refused. The panel then reads as Bluetooth being off.
- At first launch the menu-bar item does not respond until the permission question is
  answered: the first IOBluetooth call happens on the main thread.

## The card

`macos/app/Card.swift` is the card: a borderless, non-activating `NSPanel` that slides in under
the menu bar at the right, where notifications appear, with the headphones' name, the 3D model
spinning into place, the battery and the noise mode. It stays seven seconds, longer while the
pointer rests on it, and a click sends it away. It appears whenever the session counts a fresh
connection, as on Windows, which includes the app starting with the headphones already on.
Seen on screen with a WH-1000XM3, it looks and moves as it should.

The model is drawn with RealityKit (an `ARView`, which works without AR on a Mac), since
SceneKit is on its way out. The entrance, the resting pose, the lights and the framing are
those of `src/Headphones3D.tsx`, number for number.

RealityKit loads USDZ, so `macos/app/models/export.ts` writes the model built in
`src/headphoneModel.ts` out as `headphones.usdz`, which is committed:

```sh
npm ci && node macos/app/models/export.ts
```

Three things the conversion has to do, each found by a wrong picture:

- USDZ has no mirrored parts, and the left ear is the right one mirrored. The script moves
  every part's placement into its shape and turns a mirrored part's triangles back round.
- The model's surfaces are drawn from both sides, and three.js's exporter leaves that out. The
  script adds USD's `doubleSided`; RealityKit does not go by it, so `Card.swift` also switches
  face culling off for every material. Without that, one ear cup has no outside.
- The exporter writes text. `usdcat` and `usdzip`, which macOS comes with, make it binary, a
  quarter of the size.

`macos/run.sh app --demo WH-1000XM3 --render card.png card [dark]` draws the card off screen,
model included.

Not there yet:

- The WH-1000XM5 model in `src/assets/models/`: every headset gets the built-in one.
- The silver finish, and the user's own models (`docs/custom-model.md`).
- Opening the panel by clicking the card: a `MenuBarExtra` cannot be opened from code.
- The switch for the card in settings, and `register(forConnectNotifications:)`: the card
  follows the session, which notices a headset a second or two after macOS does.
- The battery counting up, and the text rising in.

## Prior work

Both are MIT-licensed; credit them in `LICENSE` and the README if code is taken from either.

- [xm5-control-macos](https://github.com/Maadlou/xm5-control-macos) by Mohamed Emad: a small
  SwiftUI menu-bar app for the WH-1000XM5, WH-1000XM6 and WF-1000XM5. The clearest reference for
  the IOBluetooth calls above (`Sources/SonyHeadphonesController.swift`).
- [acouplet](https://github.com/baglayan/acouplet) by Meriç Bağlayan, built on the former: both
  protocol generations, many more settings, and experimental LDAC playback through its own audio
  driver. Its banners use Apple's private `SystemBannerUI` framework, with a plain panel as the
  fallback (`Sources/SonyNoiseModeHUD.swift`).
