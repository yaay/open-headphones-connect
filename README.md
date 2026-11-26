# Open Headphones Connect

An unofficial, open-source companion for Sony headphones on Windows 11 and macOS, and on the Mac
for Nothing and CMF earbuds and headphones too. Not made by or affiliated with Sony or Nothing.

[![Build](https://github.com/yaay/open-headphones-connect/actions/workflows/ci.yml/badge.svg)](https://github.com/yaay/open-headphones-connect/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/yaay/open-headphones-connect)](https://github.com/yaay/open-headphones-connect/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

<p align="center">
  <a href="docs/connect-card.mp4"><img src="docs/connect-card.webp" alt="The connect card: the headphones spin into view, then the battery level and noise mode appear" width="300"></a>
  &nbsp;&nbsp;
  <img src="docs/panel.png" alt="The tray panel: noise control, playback, equalizer and headphone settings" width="300">
</p>

## Download

### Windows

<a href="https://apps.microsoft.com/detail/9NXQ80TSQD95?mode=direct"><img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get it from Microsoft" width="200"></a>

Windows 11, 64-bit. The Microsoft Store version installs without warnings and keeps itself up
to date.

Or get the installer from the [latest release](https://github.com/yaay/open-headphones-connect/releases/latest);
no administrator rights needed. It is not code-signed, so Windows shows "Windows protected your
PC" the first time: choose *More info*, then *Run anyway*.

Pair the headphones in Windows first. The app then lives in the notification area: left-click
its icon for the panel, right-click to quit.

### Mac

Coming to the Mac App Store, where it is in Apple's review. Until then you can build it
yourself: macOS 15 or later, with Rust and Xcode's command-line tools, run `macos/run.sh app`.
[The Mac app's notes](macos/README.md) have the details.

Pair the headphones in System Settings first. The app then lives in the menu bar: click its
icon for the panel.

## What it does

- **Connect card** with a 3D model of your headphones, battery level and noise mode whenever
  they connect
- **Battery** in the tray icon or the menu bar, with a notification when it runs low (Windows)
- **Noise control**: noise cancelling, wind noise reduction, ambient sound
- **Playback**: volume, play / pause, skip (Windows)
- **Equalizer** and the headphones' own settings: DSEE, Speak-to-Chat, touch panel, connecting
  to two devices, voice guidance, automatic power-off
- **NC Optimizer**, **the custom button** and **the devices** your headphones are connected to,
  with switching between them (Mac)
- **One-click connect / disconnect**, and starting at login; on the Mac, ⌥⌘A switches the
  noise mode from anywhere
- **Pairing from a card** (Mac): supported headphones in pairing mode nearby, not yet paired
  with the Mac, get a card with a Pair button, as AirPods do

## Supported headphones

| Headphones | Status | Notes |
| --- | --- | --- |
| **WH-1000XM6** | ✅ Works | |
| **WH-1000XM4** | ✅ Works | |
| **WH-1000XM3** | ✅ Works | |
| **ULT WEAR** | ⚠️ Mostly works | The equalizer and the touch panel switch are fixed in 0.2.1, not yet confirmed ([a user's report](https://github.com/yaay/open-headphones-connect/issues/1)) |
| WH-1000XM5 | ❔ Untested | |
| WH-1000XM2 | ❔ Untested | |
| WF-1000XM5, WF-1000XM4, WF-1000XM3 | ❔ Untested | |
| LinkBuds, LinkBuds S, Fit, Open, Clip | ❔ Untested | |
| WH-CH720N, WH-XB900N | ❔ Untested | |
| Nothing Ear (1), (2), (3), (a), Ear; Headphone (1) | ❔ Untested | Mac only: battery, noise control with its strength, EQ presets, in-ear detection, low latency |
| **CMF Buds Pro 2** | ⚠️ Mostly works | Mac only. Tried on real earbuds: battery for each bud, noise control and its strength, pausing when taken off, their colour on the card; the EQ presets and the two switches are not yet confirmed |
| CMF Buds 2, Buds, Buds Pro, Headphone Pro | ❔ Untested | Mac only, as above |
| Headphones from other brands | ❌ Not supported | Sony, Nothing and CMF only |

"Works" means everything in the list above: the connect card, battery, noise control, playback,
the equalizer and the headphones' settings.

Is yours untested? Try it anyway, then [tell us how it went](https://github.com/yaay/open-headphones-connect/issues/new?template=headphones-report.yml).
That is how headphones move up this table.

## More

- [Supported headphones and reporting](docs/headphones.md)
- [Your own 3D model on the connect card](docs/custom-model.md)
- [Building and how it works](docs/development.md)
- [The Mac app: building, packaging and how it works](macos/README.md)
- [Changelog](CHANGELOG.md)

## Credits

Built on the protocol work of [SonyHeadphonesClient](https://github.com/Plutoberth/SonyHeadphonesClient)
by Plutoberth, [Sony Tray](https://github.com/abhiinhii/sony-tray) by abhiinhii, and
[SonyHeadphonesClient](https://github.com/mos9527/SonyHeadphonesClient) by mos9527, Amr Satrio
and contributors. The WH-1000XM5 on the connect card is the 3D model
[Sony Wh-1000xm5](https://sketchfab.com/3d-models/sony-wh-1000xm5-5d8aea0a780b49fa89c9c205912414e3) by Zalaneke, used under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

Nothing and CMF support is written from what these MIT-licensed projects found on real
hardware: [something-x](https://github.com/SoaOaoS/something-x),
[nothinx](https://github.com/faustyu1/nothinx),
[omarchy-headphones](https://github.com/ncr/omarchy-headphones),
[nothing-toolbox](https://github.com/marc-alexis-com/nothing-toolbox),
[nothing-headphone-cli](https://github.com/LikelyLucid/nothing-headphone-cli) and
[nothing_helper](https://github.com/Kuksenok-i-s/nothing_helper).

The Nothing Headphone (1) model on the Mac's connect card is by
[Mehdi](https://www.instagram.com/mhdy.3d6/), used with his permission.

MIT licensed, apart from that model; see [LICENSE](LICENSE). Sony and its product names are trademarks of Sony Group
Corporation, used here only to say what this software works with.
