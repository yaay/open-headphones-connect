# The Mac app

A first version of the app exists: the menu-bar item, its panel and the connect card, on top
of a bridge that puts the shared session behind Swift, and a script that packages it for other
people's Macs. This page records what has been decided and what is known, so the work can go
on without repeating the research. Only what [The probe](#the-probe),
[The bridge](#the-bridge), [The panel](#the-panel), [The card](#the-card),
[Packaging](#packaging) and [The Mac App Store](#the-mac-app-store) say has been tried on
macOS.

## What it should be

The same thing the Windows app is: a card with a 3D model of the headphones when they connect,
and a small panel from the menu bar for battery, noise control, the equalizer and the headset's
switches.

- A native Swift app (SwiftUI), living in the menu bar. Not the Windows interface in a web
  view.
- Public macOS APIs only. The connect card is the app's own floating window, not Apple's
  system banner: that one sits in a private framework, breaks with macOS updates and rules out
  the Mac App Store.
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
card and its model (a first version); then packaging (done, without notarisation).
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

`macos/app/` is the app: SwiftUI, for macOS 15 and later.

- `App.swift`: the item in the menu bar (a `MenuBarExtra` in window style, with the battery
  beside the icon as in the Windows tray) and the command-line flags below.
- `Panel.swift`: the panel. `Settings.swift`: the app's own settings in its ⋯ menu.
- `Headset.swift`: the snapshot as Swift types, and the commands as JSON.

```sh
macos/run.sh app                              # with your headphones
macos/run.sh app --demo WH-1000XM5            # with made-up ones; Bluetooth is left alone
macos/run.sh app --demo WF-1000XM5 --render panel.png [card] [dark]
                                              # a picture of the panel or the card, then quit
```

`--demo` is the core's demo mode, as on Windows. `--render` draws the panel off screen, which
is how to look at a change without a headset or a screen; `-eq-open YES`,
`-headphone-settings-open YES` and `-devices-open YES` after it open the sections that are
closed until asked for.

Everything in the panel is a stock control in its stock style: SwiftUI's, except the
equalizer's band sliders, which are AppKit's `NSSlider`, SwiftUI having no upright slider on the
Mac. On macOS 26 their fill, and Clear Bass's, grows from 0 both ways. Tried on screen: the
sliders drag and set the equalizer as they should. Top to bottom: a still
of the card's 3D model (`models/portrait.png`, made by `models/portrait.swift`) with the
headphones' name, state and a gauge for each battery; noise control as a button for each
mode, with the ambient level, voice focus and the NC Optimizer under it; the equalizer, an
upright slider for each band with tick marks and a scale beside them, and Clear Bass across the
bottom; the devices the headphones are paired with; the settings kept in the
headphones, the job of each button among them; and a foot with Disconnect and a ⋯ menu.

The ⋯ menu has Power off (which asks first, as the Mac does before shutting down), Bluetooth
settings and Quit, and the app's own settings as items that tick on and off: open at login
(through `SMAppService`; where macOS wants the user's say, it opens Login Items in System
Settings), a card when the headphones connect, and a global ⌥⌘A that switches between noise
cancelling and ambient sound, registered as a hot key so that it needs no permission. "Copy
Diagnostics Report" puts what the app knows on the clipboard for a problem report, leaving out
names and addresses.

Every control shows what the session reports and sends a command when changed; the session
shows a change at once, so nothing is kept in the panel itself. A slider or fader follows the
pointer while it is dragged and the headset otherwise.

Three things beyond what the Windows panel has, all of them in `core`, so that it can have
them too:

- **The NC Optimizer** of the WH-1000XM3 and XM4 (`Command::StartOptimizer`, the snapshot's
  `optimizer`). A run plays tones in the wearer's ears, so the panel asks before starting one.
- **What a button or touch surface does** (`Command::SetAssignment`, `assignments`). On the
  WH-1000XM3 that is its custom button, which the headset describes as a general setting with
  three choices. Second-generation headsets have their own messages for it.
- **The devices the headphones are paired with** (`devices`, and `Command::ConnectDevice`,
  `DisconnectDevice` and `SelectSource`), on second-generation headsets with multipoint.

Tried with a real WH-1000XM3: the panel shows its four noise modes, equalizer, three switches,
automatic power-off, what its custom button does, and its optimizer as idle and last run at
1.0 atm. Its controls work on it, a run of the optimizer and changing what the custom button
does among them.

Not tried on a headset: everything about paired devices and second-generation keys, which
rests on the recorded WF-1000XM5 and LinkBuds Clip sessions for reading, and on libmdr's and
acouplet's agreeing accounts for writing. The message that lists which jobs a
second-generation key can have is in no recording at all.

Two things a menu-bar window does that are easy to trip over:

- It takes the least height its content will accept. A scroller accepts none, so a panel
  built around one collapses to its head and foot. The panel measures its controls and gives
  the scroller that height outright, or the screen's if that is less. `--render` sizes its
  window the same way, so this shows off screen too.
- Controls in a window that is not in front lose their colour, and so does anything drawn off
  screen: in `--render` the mode in effect and switches that are on come out grey.

Checked against Apple's Human Interface Guidelines (Designing for macOS, The menu bar,
Disclosure controls, Menus, Buttons, Writing, Panels, Notifications), November 2026:

- Section titles open their sections as well as the triangle beside them. SwiftUI's
  `DisclosureGroup` on the Mac answers only the triangle, so each title is a button too.
- Menu items are verb phrases in title style without articles, with an ellipsis where more
  input follows ("Power Off Headphones…") or another app opens ("Open Bluetooth Settings…"),
  and icons for all of a group's items or none.
- Labels of controls are in sentence style and section headings in title style, throughout.
- Power off asks first, with a destructive button.

Three places where the app departs from the guidelines, on purpose for now:

- The menu bar extra opens a window-style panel, not a menu. The guidelines allow that when
  the functionality is too complex for a menu, as sliders and an equalizer are.
- The app lives only in the menu bar. The guidelines would have people choose whether the
  menu bar extra is there, from the app's settings, with the app reachable otherwise (a Dock
  icon and menu).
- The card is the app's own notification-like window, which does not follow Focus as a
  system notification would.

When the user has refused the app Bluetooth, the session cannot tell: it sees Bluetooth as
off, or nothing paired. The panel asks `CBCentralManager.authorization` instead and says so,
with a button to the right page of System Settings. That screen has only been drawn
(`--render FILE refused`), not met.

Left out on purpose: volume and playback, which a Mac has its own controls for. Not there yet:

- A warning when the battery runs low.
- Trying the app's own settings: they are built, not yet used. Opening at login may want the
  app in Applications and signed with a certificate.
- The same three additions in the Windows panel.
- At first launch the menu-bar item does not respond until the permission question is
  answered: the first IOBluetooth call happens on the main thread.

## The card

`macos/app/Card.swift` is the card: a SwiftUI `Window` with no frame, floating above other
windows under the menu bar at the right, where notifications appear, with the headphones'
name, the 3D model spinning into place, the battery and the noise mode. It stays seven
seconds, longer while the pointer rests on it, and a click sends it away. It appears whenever
the session counts a fresh connection, as on Windows, which includes the app starting with the
headphones already on. The menu-bar item is what opens it, being the one view that is always
there.

On macOS 26 and later the card's ground is Liquid Glass, as a notification's is; on macOS 15
it is the frosted material with a shadow. Everything else takes on Liquid Glass by itself on
macOS 26, being stock controls, menus and windows; the panel has no glass of its own, since
Apple advises against glass on glass.

Opened while another app is in use, the card does not take the focus from it (tried with
Safari in front). Seen on screen, card and panel look as they should. It needs macOS 15, for
the frameless, floating, placed window and for `RealityView`; on macOS 14 the card would have
to be an AppKit panel.

The model is drawn with RealityKit, since SceneKit is on its way out. The entrance, the
resting pose, the lights and the framing are those of `src/Headphones3D.tsx`, number for
number.

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

`macos/run.sh app --demo WH-1000XM3 --render card.png card` draws the card off screen, with
the still in place of the 3D model and without its glass, neither of which can be drawn that
way.

Not there yet:

- The WH-1000XM5 model in `src/assets/models/`: every headset gets the built-in one.
- The silver finish, and the user's own models (`docs/custom-model.md`).
- Opening the panel by clicking the card: a `MenuBarExtra` cannot be opened from code.
- `register(forConnectNotifications:)`: the card follows the session, which notices a headset
  a second or two after macOS does.
- The battery counting up, and the text rising in.

## Nothing and CMF

Nothing and CMF earbuds and headphones speak a protocol of their own over the same kind of serial
channel (`core/src/protocol/nothing.rs`; the session side is `core/src/session/nothing_link.rs`).
It was written from MIT-licensed projects that tested it on real devices; the README credits
them. The Mac finds the channel through its service record
(`AEAC4A03-DFF5-498F-843A-34487CF133EB`), and the bridge now hands the session raw bytes, which it
puts together as Sony or Nothing frames.

- Battery for each earbud and the case (the case only reports while open, so its last level
  stays), noise cancelling with its strength, transparency, off, EQ presets (Nothing's own, or
  CMF's "listening modes"), in-ear detection (as "Pause when taken off") and low latency.
- Requests go at least 150 ms apart, as some earbuds drop one that follows too closely, and every
  setting is read back after it is acknowledged.
- `--demo "CMF Buds Pro 2"` (or any Nothing or CMF name) shows made-up ones.
- A model can have its own picture: `headphones-<name>.usdz` for the card's 3D view and
  `portrait-<name>.png` for the panel, or `card-<name>.mov` or `.mp4`, a clip the card plays
  instead (muted, once, the card staying until it ends). A clip on a plain ground needs no
  cut-out: the card takes the colour of its first frame's corner, and the clip's edges fade into
  it. The glow behind a 3D model waits for the model, so it never shows alone.
- A model can take a second to read (the CMF Buds Pro 2's, 1.3 s), so the app reads the one of
  the headphones used last as it starts, and opens the card only once its model is in memory.
  A finish the headphones report (`finishes` in `Card.swift`: so far the orange CMF Buds Pro 2,
  colour 7) tints the model and picks the panel's still, `portrait-<name>-<colour>.png`. `<name>` is the model name in lower
  case with dashes: `cmf-buds-pro-2`, `headphone-1`, `cmf-headphone-pro`. Models without them get
  the general ones. `models/blender_usdz.py` says how the Nothing and CMF ones were made.
- `core/tests/nothing_headset.rs` runs the whole session against a stand-in CMF Buds Pro 2.

Not tried on real hardware yet. Open questions: whether presets 1 and 3 are Voice and More Bass
(Headphone (1)) or the other way round (one report from an Ear (2)); which preset command
CMF Buds 2 and CMF Headphone Pro use (the session asks both ways); and whether every model takes
"off" as 5. Without a service record the app does not yet try the known channels (15, 16, 28).
Not done: custom EQ curves, gestures, spatial audio, bass boost, multipoint, and Windows (its
link would have to hand over raw bytes and look for the same service).

## The pairing card

Headphones in pairing mode nearby, supported and not yet paired with the Mac, get the card with
"Not paired with this Mac" and one big Pair button (`app/Pairing.swift`). Every card has a close
button in its corner; closing an offer turns it down, and silences that pair until the app
starts again. "Offer to Pair New Headphones" in the ⋯ menu turns the offers off. `--demo X --offer "WH-1000XM3"` shows the card for made-up ones.

- **Noticing.** Headphones in pairing mode say so over Bluetooth LE: Google Fast Pair (service
  data `FE2C` holding a three-byte model id; anything longer is an account filter, sent when not
  in pairing mode) and Microsoft Swift Pair (manufacturer data `0006 03 …`). The app listens in
  scans of eight seconds in a row, since a device is reported once a scan. A WH-1000XM3 sends
  Fast Pair `0BC95C` under the name `LE_WH-1000XM3`: Sony's LE names carry that prefix, which
  is dropped. An advertisement that names nothing leads to one short search, two minutes apart
  at most.
- **Pairing.** Clicking Pair runs a classic search (`IOBluetoothDeviceInquiry`, audio devices
  only) with the LE listening paused, for up to 45 seconds: a ten-second pass missed the
  WH-1000XM3 once and found it after nine seconds another time. Then `IOBluetoothDevicePair`,
  `openConnection`, and `rescan` to the session until it has them. On the WH-1000XM3: found
  twelve seconds after the click, paired in two, connected two later.
- **What is trusted.** An advertisement is anybody's word, so it only ever leads to an offer.
  The device paired with has to answer the search itself, as an audio device, under a supported
  headphone's name that matches the offer (and under the advertisement's address, where it gave
  one). macOS asked for no confirmation when the WH-1000XM3 paired, as headphones have nothing
  to confirm on; a device that does ask, for a number or a code, is refused.
- **Buttons.** The card's two buttons are fills drawn by the app: Pair a capsule in the system's
  blue, which keeps its place while the pairing is under way with the spinner in it, and close
  the system's `xmark.circle.fill`. Stock buttons are Liquid Glass on macOS 26, and
  inside the card's own glass they drew all of the card's text upside down; the glass button
  styles over a card whose glass was moved behind its content looked wrong too. Fills also keep
  their colour in a window that is not the active one, which the card never is.
- `macos/run.sh probe scan 20` and `probe inquire 8` show what the app would hear and find.

Not tried: Swift Pair headphones, Nothing's and CMF's (their Fast Pair ids are from an app's
tables), and headphones that announce nothing, which are never noticed. Not done: Windows, which
shows its own Swift Pair notification.

## Packaging

```sh
macos/run.sh package
```

builds the app optimised, for Apple silicon and Intel in one file, with its icon, the model
and the licence inside, and puts it in `macos/build/Open-Headphones-Connect-<version>.dmg`
beside a shortcut to Applications. The version is the one in `package.json`, so the usual
version change covers the Mac app too. It needs Rust and Xcode's command-line tools, and
fetches Rust's Intel half the first time.

The app is signed ad hoc and with the hardened runtime, which Apple asks for before it will
notarise anything. It is not notarised, so whoever downloads it meets what the Windows
installer meets with SmartScreen. On first opening, macOS says it cannot check the app; they
then go to System Settings > Privacy & Security, scroll to the message about Open Headphones
Connect, and choose Open Anyway. After that macOS asks once whether it may use Bluetooth.

Found while building it:

- Bluetooth needs no entitlement under the hardened runtime. The packaged app, started that
  way, connected to a WH-1000XM3 and showed its panel.
- Only one copy at a time can hold the headset's control channel. A second one finds the
  headset and then stays at "Connecting…" until the first is quit.
- Because the signature is ad hoc, macOS takes each new version for a new app and asks for
  Bluetooth again after an update.

Not tried:

- The Intel half. It is built, and the build checks that it compiles, but there was no Intel
  Mac or Rosetta to run it on.
- Opening a downloaded copy on another Mac. The steps above are what macOS is known to do
  with an app that is not notarised, not something watched here; here `spctl` rejects it.
- Notarising, which takes away the Open Anyway step. It needs the paid Apple Developer
  Program, and on the Mac that builds: a Developer ID Application certificate (Xcode >
  Settings > Accounts > Manage Certificates), and Apple's password for the notary service
  stored once with `xcrun notarytool store-credentials ohc-notary`. Then

  ```sh
  SIGN="Developer ID Application" NOTARY=ohc-notary macos/run.sh package
  ```

  signs the app and the disk image, sends the image to Apple, waits, and staples the answer
  to it.


## The Mac App Store

```sh
macos/run.sh store
```

builds the same universal app sandboxed, which the store insists on, and puts it in the
installer package the store takes, `macos/build/Open-Headphones-Connect-<version>.pkg`. As it
stands that is for trying: signed ad hoc, it cannot be uploaded. For the store it wants three
things from the paid Apple Developer Program, none of which belongs in this repository:

```sh
SIGN="Apple Distribution" INSTALLER="3rd Party Mac Developer Installer" \
    PROFILE=~/Downloads/OpenHeadphonesConnect.provisionprofile BUILD=2 macos/run.sh store
```

- `SIGN` and `INSTALLER` are the two certificates (Xcode > Settings > Accounts > Manage
  Certificates: Apple Distribution, and Mac Installer Distribution).
- `PROFILE` is a Mac App Store Connect provisioning profile for the app's identifier,
  `dev.openheadphonesconnect.desktop`, made at developer.apple.com once the identifier is
  registered there. It is copied into the app, and the app and team identifiers in it go
  into the entitlements. Without one the store still takes the app, but TestFlight does not.
- `BUILD` is the build number, which has to go up with every upload of the same version.

The package then goes up with Apple's Transporter app, to an app record made in App Store
Connect.

Tried, with a WH-1000XM3:

- The sandboxed app needs two entitlements and no more: `com.apple.security.app-sandbox` and
  `com.apple.security.device.bluetooth`. Signed with those it finds the headset, opens the
  control channel and shows its panel, as it does outside the sandbox.
- Disconnect and Connect work from inside the sandbox (tried with the session tool).
- The package holds the app intact: expanded again, its signature verifies.
- `Info.plist` has the category and the export-compliance answer the store asks for, and the
  icon has every size.

- Signed for the store with the two certificates and a Mac App Store Connect profile, the
  build goes through: the app carries the profile and the four entitlements, both signatures
  verify, and the package is signed by the installer certificate.

- The first upload was turned down (error 91109): the profile, downloaded with a browser, had
  macOS's quarantine mark on it, and its copy in the app kept the mark. The build now takes
  such marks off everything in the app before signing.

  The second upload, through Apple's Transporter app, went through.

Not tried: review.

Two things review is likely to raise:

- Reviewers have no Sony headphones, and without any the app shows only "Not paired yet".
  The usual answer is a screen recording of it at work, in the notes for the reviewer.
- Whether an unofficial app for Sony's headphones, with their name in its listing, is
  acceptable (guideline 5.2) is Apple's call and not predictable from here.

## Prior work

Both are MIT-licensed; credit them in `LICENSE` and the README if code is taken from either.

- [xm5-control-macos](https://github.com/Maadlou/xm5-control-macos) by Mohamed Emad: a small
  SwiftUI menu-bar app for the WH-1000XM5, WH-1000XM6 and WF-1000XM5. The clearest reference for
  the IOBluetooth calls above (`Sources/SonyHeadphonesController.swift`).
- [acouplet](https://github.com/baglayan/acouplet) by Meriç Bağlayan, built on the former: both
  protocol generations, many more settings, and experimental LDAC playback through its own audio
  driver. Its banners use Apple's private `SystemBannerUI` framework, with a plain panel as the
  fallback (`Sources/SonyNoiseModeHUD.swift`).
