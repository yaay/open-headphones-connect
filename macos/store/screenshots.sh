#!/bin/sh
# Makes the four Mac App Store screenshots beside this file, from the app itself: its panel and
# card drawn off screen with made-up headphones, then laid on a canvas by compose.swift.
#
#     macos/store/screenshots.sh
#
# Run it again when the panel or the card changes. Drawn off screen, switches that are on come
# out grey rather than in the accent colour they have in a window that is in use.
set -e
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
BUILD_ONLY=1 "$here/../run.sh" app
app="$here/../build/Open Headphones Connect.app/Contents/MacOS/Open Headphones Connect"
swiftc -O "$here/compose.swift" -o "$work/compose"

# A model that has what the made-up headphones show: power off, and no wind mode.
draw() {
    picture=$1
    shift
    "$app" --demo WH-1000XM4 --render "$work/$picture.png" dark --scale 4 "$@"
}
draw card card
draw noise -eq-open NO -headphone-settings-open NO \
    --send '{"set_noise": {"mode": "ambient", "ambientLevel": 14, "focusOnVoice": true}}'
draw equalizer -eq-open YES -headphone-settings-open NO \
    --send '{"set_eq_bands": {"clearBass": 4, "bands": [3, 1, -1, 2, 5]}}'
draw settings -eq-open NO -headphone-settings-open YES

"$work/compose" "$work/card.png" "$here/screenshot-1.jpg" \
    "A card greets your headphones" "Their battery and noise mode, the moment they connect."
"$work/compose" "$work/noise.png" "$here/screenshot-2.jpg" \
    "Noise control from the menu bar" "Noise cancelling, ambient sound with its level, or off." panel
"$work/compose" "$work/equalizer.png" "$here/screenshot-3.jpg" \
    "Presets, or your own curve" "The equalizer your headphones have, band by band." panel
"$work/compose" "$work/settings.png" "$here/screenshot-4.jpg" \
    "Settings without your phone" "Speak-to-Chat, touch panel, upscaling, voice guidance and automatic power-off." panel
rm -rf "$work"
