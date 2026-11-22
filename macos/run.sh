#!/bin/sh
# Builds the Mac app, or one of the two command-line tools, into an app bundle and runs it.
#
#     macos/run.sh app [--demo MODEL] [--render FILE [card] [dark]]   # app/: see app/App.swift
#     macos/run.sh probe                         # probe/main.swift: IOBluetooth alone
#     macos/run.sh session [seconds] [command]   # session/main.swift: the session, through the bridge
#
# The tools go in a bundle too: macOS lets a command-line tool use Bluetooth only if the program
# that started it says why it wants to, and kills the tool otherwise. In a bundle of its own the
# tool says so itself, so this works from any terminal. Their output appears when they finish.
#
#     SIGN="Apple Development" macos/run.sh app   # with your certificate, macOS asks for
#                                                 # Bluetooth once, not after every build
#     BUILD_ONLY=1 macos/run.sh app               # compile and stop
set -e
here=$(cd "$(dirname "$0")" && pwd)
tool=${1:?usage: run.sh app|probe|session [arguments]}
shift

case "$tool" in
app)
    name="Open Headphones Connect"
    id=dev.openheadphonesconnect.desktop
    why="Shows your Sony headphones' battery and changes their settings."
    app="$here/build/$name.app"
    ;;
probe | session)
    name=ohc-probe
    id=dev.openheadphonesconnect.probe
    why="Looks for a paired Sony headset and asks it read-only questions."
    app="${TMPDIR:-/tmp}/ohc-probe.app"
    ;;
*)
    echo "unknown tool: $tool" >&2
    exit 1
    ;;
esac
binary="$app/Contents/MacOS/$name"
mkdir -p "$app/Contents/MacOS"

# swiftc with the bridge compiled in: the Rust half as a library, the Swift half as source.
with_bridge() {
    cargo build --release --manifest-path "$here/bridge/Cargo.toml"
    swiftc "$@" "$here/bridge/HeadsetSession.swift" -import-objc-header "$here/bridge/ohc_bridge.h" \
        -L "$here/bridge/target/release" -lohc_bridge -o "$binary"
}
case "$tool" in
app)
    # The copy that is running would otherwise stay in the menu bar beside the new one.
    pkill -x "$name" || true
    with_bridge -parse-as-library -target "$(uname -m)-apple-macos14.0" "$here"/app/*.swift
    mkdir -p "$app/Contents/Resources"
    cp "$here"/app/models/*.usdz "$app/Contents/Resources/"
    ;;
probe) swiftc "$here/probe/main.swift" -o "$binary" ;;
session) with_bridge "$here/session/main.swift" ;;
esac
[ -z "$BUILD_ONLY" ] || exit 0

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$id</string>
<key>CFBundleExecutable</key><string>$name</string>
<key>CFBundleName</key><string>$name</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.2.1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>LSUIElement</key><true/>
<key>NSBluetoothAlwaysUsageDescription</key><string>$why</string>
</dict></plist>
PLIST
codesign --force --sign "${SIGN:--}" "$app" 2>/dev/null

case "$tool $*" in
"app "*--demo*) "$binary" "$@" ;;
# With real headphones it has to be started as an app, which looks for FILE from the root.
"app "*--render*) open -W -n "$app" --args "$@" ;;
"app "*) open -n "$app" --args "$@" ;;
*)
    out=$(mktemp)
    echo "Running. If macOS asks whether $name may use Bluetooth, allow it." >&2
    # The tool's own exit code does not come back through `open`.
    open -W -n ${OHC_TRACE:+--env OHC_TRACE="$OHC_TRACE"} --stdout "$out" --stderr "$out" "$app" --args "$@"
    cat "$out"
    rm "$out"
    ;;
esac
