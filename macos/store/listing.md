# Mac App Store listing

A draft of the text for App Store Connect, English (U.S.). Each heading below is the name of a
field there, with its limit. It says only what the Mac app does today: no playback or volume
controls and no low-battery warning, which the Windows listing has.

## Name (30)

Open Headphones Connect

## Subtitle (30)

Battery, noise control, EQ

## Promotional Text (170)

Your Sony wireless headphones in the menu bar: battery at a glance, noise control, the equalizer and their settings, and a card that greets them when they connect.

## Description (4000)

Open Headphones Connect brings your Sony wireless headphones to the Mac's menu bar.

When your headphones connect, a small card slides in with a 3D picture of them, the battery level and the noise mode, then gets out of your way. After that, everything lives in an icon in the menu bar: click it to see the battery, switch between noise cancelling and ambient sound, adjust the equalizer, or change settings you would otherwise need your phone for.

What you can do:
• See the battery level at a glance, right in the menu bar
• Switch between noise cancelling, ambient sound and off, with the ambient level and focus on voice
• Run the NC Optimizer, which tunes noise cancelling to you and to the air pressure around you
• Pick an equalizer preset or shape your own curve
• See which devices the headphones are connected to, and switch which one plays
• Choose what the custom button does
• Connect, disconnect or power off the headphones with one click
• Change headphone settings: Speak-to-Chat, touch panel, DSEE upscaling, voice guidance, automatic power-off
• Switch the noise mode from anywhere with Option-Command-A, and open the app at login
• Pair new headphones from a card: put them in pairing mode near your Mac and click Pair

Headphones it works with:
• WH-1000XM3: tested on the Mac
• WH-1000XM4 and WH-1000XM6: tested with the Windows version of this app, which shares its headphone code
• ULT WEAR, WH-1000XM5, WF-1000XM4 and XM5, LinkBuds and other recent Sony models: expected to work, not yet tested on the Mac
The app shows only the controls your headphones actually have. Headphones from other brands are not supported.

Private by design: the app talks only to your headphones. It does not connect to the internet, and has no account, ads or tracking.

Free and open source. The code, the full list of supported headphones and the place to report problems are at https://github.com/yaay/open-headphones-connect

Open Headphones Connect is an unofficial, community-made app. It is not made by, affiliated with or endorsed by Sony. Your headphones must already be paired with your Mac over Bluetooth.

Sony, WH-1000XM, LinkBuds, ULT WEAR and DSEE are trademarks of Sony Group Corporation.

## Keywords (100)

headphones,noise cancelling,equalizer,battery,ambient sound,bluetooth,menu bar,anc,eq,earbuds

No Sony names here on purpose: Apple's guideline 2.3.7 is against trademarked terms in keywords.
The description may say what the app works with; keywords are where it gets apps rejected.

## What's New in This Version

• NC Optimizer, the custom button, and the devices your headphones are connected to, with switching between them
• Switch the noise mode from anywhere with Option-Command-A
• Open at login
• Pair new headphones from a card, the way AirPods pair
• A redesigned panel, and a livelier card when your headphones connect
• Recovers by itself when macOS restarts Bluetooth

## Support URL

https://github.com/yaay/open-headphones-connect/issues

## Marketing URL

https://github.com/yaay/open-headphones-connect

## Privacy Policy URL

https://github.com/yaay/open-headphones-connect/blob/main/docs/privacy.md

## Copyright

2026 Open Headphones Connect contributors

## Screenshots

Upload in this order. All are 2880 x 1800, one of the four sizes the store takes for a Mac, and
have no transparency in them:

1. `screenshot-1.jpg`: the card as the headphones connect
2. `screenshot-2.jpg`: noise control, with ambient sound chosen
3. `screenshot-3.jpg`: the equalizer with its sliders open
4. `screenshot-4.jpg`: the headphone settings open

`screenshots.sh` makes them from the app itself, with made-up headphones; run it again when
the panel or the card changes. Because the app is drawn off screen for them, switches that are
on are grey there, where on a real screen they are in the accent colour.

## App Review Information: Notes

Open Headphones Connect is a menu-bar utility for Sony wireless headphones. It has no window and no Dock icon: after it starts, look for the headphones icon in the menu bar.

It needs Sony headphones paired with the Mac over Bluetooth. Without any, the panel says "Not paired yet" and offers a button to Bluetooth settings; that is the expected behaviour. The attached screen recording shows the app with a WH-1000XM3: the card when the headphones connect, then noise control, the equalizer and the headphone settings in the panel.

The app talks to the headphones over a classic Bluetooth serial (RFCOMM) channel, using the public IOBluetooth framework. That is why it asks for Bluetooth permission and has the com.apple.security.device.bluetooth entitlement. It makes no network connections and collects no data.

It is an independent, open-source project (https://github.com/yaay/open-headphones-connect) and is not made by or affiliated with Sony, which the description says.

Attach: a screen recording of the app with real headphones. Sign-in required: no.

## Elsewhere in the submission

These are declarations, and yours to make. What the app's behaviour suggests:

- Category: Utilities, with Music as the secondary. `Info.plist` already says Utilities.
- Pricing and Availability: Free.
- Age Rating: every question "None" or "No", which gives 4+.
- App Privacy: "Data Not Collected". The app sends nothing anywhere.
- Content Rights: no third-party content. The 3D model is drawn by this project's own code.
- Export Compliance: asked nothing; `Info.plist` already says the app uses no encryption of
  its own.
