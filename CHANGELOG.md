# Changelog

Notable changes to Open Headphones Connect, newest first.

## Unreleased

- Mac: Nothing and CMF earbuds and headphones (Ear, Headphone (1), CMF Buds and others): battery
  for each earbud and the case, noise cancelling with its strength (high, mid, low, adaptive),
  transparency, EQ presets, in-ear detection and low-latency mode. Not yet tried on real
  hardware; reports welcome.
- Mac: the connect card and the panel show each model's own picture where the app has one: 3D
  models of the CMF Buds Pro 2 (lid open, buds out) and the Nothing Headphone (1), and a clip of
  the CMF Headphone Pro.

## 0.2.1 (2026-11-21)

- ULT WEAR: the equalizer now works. Its messages carry an ULT mode that the app did not read, so
  it showed a single band and could not be changed ([#1](https://github.com/yaay/open-headphones-connect/issues/1)).
- ULT WEAR and other newer models: the touch panel switch now takes effect. The headphones ask
  for confirmation before changing it, and the app used to answer no.

## 0.2.0 (2026-11-21)

- The connect card shows a model of the headphones that are connected: the WH-1000XM5 and
  WH-1000XM6 get a realistic one ("Sony Wh-1000xm5" by Zalaneke, CC BY 4.0).
- Your own 3D models can be per headphone model: a file named after the headphones in a
  `models` folder wins over the general `model.glb`.
- Demo mode: `--demo WH-1000XM5` shows made-up headphones of any model, without Bluetooth.

## 0.1.0 (2026-11-21)

The first public release.

- Connect card with a 3D model, battery level and the active noise mode, shown every time the
  headphones connect.
- Battery percentage in the tray icon, with a notification at 20%.
- Noise control: noise cancelling, wind noise reduction, ambient sound with level and focus on
  voice, and off, for every noise-control layout in Sony's message tables.
- Playback: volume, play / pause and skipping tracks.
- Equalizer with the presets, bands and range each headset describes.
- Headphone settings: DSEE upscaling, Speak-to-Chat, pause when taken off, touch panel,
  connecting to two devices, voice guidance and automatic power-off.
- One-click connect / disconnect of the headphones' audio.
- Your own 3D model on the connect card, loaded from your settings folder.
- Start with Windows.

Tested on a WH-1000XM3 only. Second-generation models (WH-1000XM5 and later, WF-1000XM4 and
later, LinkBuds) are implemented from Sony's message tables and checked against recordings of
real WF-1000XM5 and LinkBuds Clip earbuds, but have never been connected to this app.
