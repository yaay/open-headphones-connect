# Supported headphones

| Headphones | Protocol | Status |
| --- | --- | --- |
| WH-1000XM3 | first generation | Tested (firmware 4.5.2) |
| WH-1000XM4 | first generation | Tested |
| WH-1000XM6 | second generation | Tested |
| ULT WEAR (WH-ULT900N) | second generation | [Reported by a user](https://github.com/yaay/open-headphones-connect/issues/1) on firmware 2.1.0: works; the equalizer and the touch panel switch are fixed in 0.2.1, not yet confirmed |
| WH-1000XM2, WF-1000XM3, WH-H900N, WH-XB900N | first generation | Same command table as the WH-1000XM3, not tried yet |
| WF-1000XM5, LinkBuds Clip | second generation | Reading checked against recordings of real units, not tried yet |
| WH-1000XM5, WF-1000XM4, other LinkBuds, WH-CH720N | second generation | Same command tables, not tried yet |

Each headset is asked what it has when it connects, and the panel shows only that: earbuds get
left, right and case batteries, and open earbuds without noise control get no noise section.

## What has actually been tried

- **WH-1000XM3, WH-1000XM4 and WH-1000XM6:** tested by the maintainer and working: the connect
  card, battery, noise control, playback, the equalizer and the headphone settings.
- **ULT WEAR:** one user's report on version 0.2.0. The connect card, battery, noise control
  with ambient level and focus on voice, playback and connect / disconnect all worked. The
  equalizer showed a single band and could not be changed, because this model's equalizer
  messages carry an ULT mode the app did not read. The touch panel switch had no effect, because
  the headphones ask for confirmation and the app declined. Both are fixed in 0.2.1 from the
  report's trace, and wait for someone with the headphones to confirm. DSEE could not be
  confirmed either way.

Everything else in the table rests on Sony's message tables as published in libmdr, and on
replaying sessions recorded from real WF-1000XM5 and LinkBuds Clip earbuds
(`core/src/protocol/fixtures/`). Of the twelve noise-control layouts Sony defines, the kind
the WH-1000XM5 and ULT WEAR use is confirmed on a real ULT WEAR; most of the others are
implemented from the tables alone. The same goes for first-generation models with only noise cancelling or only
ambient sound.

## Good to know

- Only one program can hold the headphones' control connection at a time. If the panel says the
  headphones aren't answering, close Sony's app on your phone.
- Switching "connect to two devices" makes the headphones drop and re-establish their Bluetooth
  connection; the app answers the headset's "go ahead?" for you when you flip that switch.
- The WH-1000XM3 cannot be powered off remotely, so that button is hidden for it.

## Tell us how yours behaves

If your headphones are not marked as tested above, a report is the most useful contribution
there is, whether it works or not. [Open a headphones report](https://github.com/yaay/open-headphones-connect/issues/new?template=headphones-report.yml)
and attach a trace:

1. Quit the app (right-click the tray icon).
2. Start it from PowerShell with tracing on:

   ```powershell
   $env:OHC_TRACE = "1"; & "$env:LOCALAPPDATA\Open Headphones Connect\open-headphones-connect.exe"
   ```

3. Connect the headphones, try the controls that misbehave, then attach
   `%LocalAppData%\dev.openheadphonesconnect.desktop\logs\app.log`.

With the Microsoft Store version, start it with
`$env:OHC_TRACE = "1"; open-headphones-connect.exe` instead, and the log is
`%LocalAppData%\Packages\17085Yehia.OpenHeadphonesConnect_86j4vst6mgpyr\LocalState\logs\app.log`.

The trace holds every message exchanged with the headphones: the model name, firmware version
and settings, and nothing about you or your PC. Messages the app does not understand are
written to the same log even without tracing.
