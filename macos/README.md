# The Mac app

Not built yet. This page records what has been decided and what is known, so the work can start
on a Mac without repeating the research. Nothing here has been tried on macOS.

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

What is missing for Swift is a bridge: a small Rust crate with a C interface that implements
the traits by calling into Swift, hands each `Snapshot` over as JSON, and accepts commands.
`Snapshot` already serialises to the JSON the Windows interface reads (its shape is in
`src/types.ts`); `session::Command` has no serialised form yet.

The session calls the traits from its own thread and expects them to block, while IOBluetooth
wants the main run loop. The bridge has to hop between the two.

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

- Whether the WH-1000XM3 answers the service lookup on macOS. On Windows it keeps the control
  service out of the public list, and only a direct request by id works. Neither existing Mac
  client lists the XM3 as tested.
- Whether a second question sent before the first answer loses that answer on macOS too (the
  session already waits, so this should need nothing).

A sensible order: a command-line probe that opens the channel and prints the reply to one
question; then the bridge; then the menu-bar panel; then the card and its model; then packaging.
Without an Apple Developer ID the app cannot be notarised, and people who download it have to
allow it by hand in System Settings.

## Prior work

Both are MIT-licensed; credit them in `LICENSE` and the README if code is taken from either.

- [xm5-control-macos](https://github.com/Maadlou/xm5-control-macos) by Mohamed Emad: a small
  SwiftUI menu-bar app for the WH-1000XM5, WH-1000XM6 and WF-1000XM5. The clearest reference for
  the IOBluetooth calls above (`Sources/SonyHeadphonesController.swift`).
- [acouplet](https://github.com/baglayan/acouplet) by Meriç Bağlayan, built on the former: both
  protocol generations, many more settings, and experimental LDAC playback through its own audio
  driver. Its banners use Apple's private `SystemBannerUI` framework, with a plain panel as the
  fallback (`Sources/SonyNoiseModeHUD.swift`).
