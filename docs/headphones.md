# Supported headphones

| Headphones | Protocol | Status |
| --- | --- | --- |
| WH-1000XM3 | first generation | Tested on real hardware (firmware 4.5.2) |
| WH-1000XM2, WH-1000XM4, WF-1000XM3, WH-H900N, WH-XB900N | first generation | Same command table, untested |
| WF-1000XM5, LinkBuds Clip | second generation | Reading checked against recordings of real units; never connected here |
| WH-1000XM5 / XM6, WF-1000XM4, other LinkBuds, ULT Wear, WH-CH720N | second generation | Same command tables, untested |

Each headset is asked what it has when it connects, and the panel shows only that: earbuds get
left, right and case batteries, and open earbuds without noise control get no noise section.

## What has actually been tried

**Only the WH-1000XM3 has been connected to this app.** On it the following has been confirmed
against the headset itself: opening the control link, reading battery, noise mode, equalizer,
firmware and codec, the connect card on every reconnect, the playback controls and the headphone
settings. Not yet exercised on hardware: switching between noise modes, changing the equalizer,
and the audio connect / disconnect button.

For second-generation headsets, what the app reads is tested by replaying sessions recorded from
real WF-1000XM5 and LinkBuds Clip earbuds (`src-tauri/src/protocol/fixtures/`). What it writes
follows Sony's message tables as published in libmdr, and has not been sent to a real headset
from this app. Of the twelve noise-control layouts Sony defines, libmdr drives three (the XM5
kind, the adaptive kind and ambient-only); the other nine are implemented from the tables alone.
The same goes for first-generation models with only noise cancelling or only ambient sound, and
for Speak-to-Chat and pause when taken off on the WH-1000XM4.

## Good to know

- Only one program can hold the headphones' control connection at a time. If the panel says the
  headphones aren't answering, close Sony's app on your phone.
- Switching "connect to two devices" makes the headphones drop and re-establish their Bluetooth
  connection; the app answers the headset's "go ahead?" for you when you flip that switch.
- The WH-1000XM3 cannot be powered off remotely, so that button is hidden for it.

## Tell us how yours behaves

If you own anything other than a WH-1000XM3, a report is the most useful contribution there is,
whether it works or not. [Open a headphones report](https://github.com/yaay/open-headphones-connect/issues/new?template=headphones-report.yml)
and attach a trace:

1. Quit the app (right-click the tray icon).
2. Start it from PowerShell with tracing on:

   ```powershell
   $env:OHC_TRACE = "1"; & "$env:LOCALAPPDATA\Open Headphones Connect\open-headphones-connect.exe"
   ```

3. Connect the headphones, try the controls that misbehave, then attach
   `%LocalAppData%\dev.openheadphonesconnect.desktop\logs\app.log`.

The trace holds every message exchanged with the headphones: the model name, firmware version
and settings, and nothing about you or your PC. Messages the app does not understand are
written to the same log even without tracing.
