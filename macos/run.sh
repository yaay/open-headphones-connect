#!/bin/sh
# Builds the Mac app, or one of the two command-line tools, into an app bundle and runs it.
#
#     macos/run.sh app [--demo MODEL] [--render FILE [card | settings]]   # app/: see app/App.swift
#     macos/run.sh probe                         # probe/main.swift: IOBluetooth alone
#     macos/run.sh session [seconds] [command]   # session/main.swift: the session, through the bridge
#     macos/run.sh package                       # the app for other people's Macs: optimised, for
#                                                # Apple silicon and Intel, in a disk image
#     macos/run.sh store                         # the same app sandboxed, in an installer package
#                                                # for the Mac App Store: see macos/README.md
#
# The tools go in a bundle too: macOS lets a command-line tool use Bluetooth only if the program
# that started it says why it wants to, and kills the tool otherwise. In a bundle of its own the
# tool says so itself, so this works from any terminal. Their output appears when they finish.
#
#     SIGN="Apple Development" macos/run.sh app   # with your certificate, macOS asks for
#                                                 # Bluetooth once, not after every build
#     BUILD_ONLY=1 macos/run.sh app               # compile and stop
#     SIGN="Developer ID Application" NOTARY=profile macos/run.sh package
#                                                 # signed and notarised, so that it opens on
#                                                 # other Macs unasked: see macos/README.md
#     SIGN="Apple Distribution" INSTALLER="3rd Party Mac Developer Installer" \
#         PROFILE=file.provisionprofile BUILD=2 macos/run.sh store
#                                                 # as the store wants it signed; BUILD is the
#                                                 # build number, higher with every upload
set -e
here=$(cd "$(dirname "$0")" && pwd)
tool=${1:?usage: run.sh app|probe|session|package|store [arguments]}
shift

name="Open Headphones Connect"
id=dev.openheadphonesconnect.desktop
why="Shows your Sony headphones' battery and changes their settings."
case "$tool" in
app) app="$here/build/$name.app" ;;
# Apart from the copy `app` builds, which may be the one that is running.
package | store) app="$here/build/$tool/$name.app" ;;
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
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

# compile ARCH OUTPUT ARGUMENTS…: swiftc for one kind of Mac, with the bridge compiled in, the
# Rust half as a library and the Swift half as source.
compile() {
    arch=$1
    output=$2
    shift 2
    case "$arch" in
    arm64) rust=aarch64-apple-darwin ;;
    *) rust=x86_64-apple-darwin ;;
    esac
    cargo build --release --manifest-path "$here/bridge/Cargo.toml" --target "$rust"
    swiftc "$@" -target "$arch-apple-macos15.0" "$here/bridge/HeadsetSession.swift" \
        -import-objc-header "$here/bridge/ohc_bridge.h" -L "$here/bridge/target/$rust/release" -lohc_bridge -o "$output"
}
case "$tool" in
app)
    # The copy that is running would otherwise stay in the menu bar beside the new one.
    pkill -x "$name" || true
    compile "$(uname -m)" "$binary" -parse-as-library "$here"/app/*.swift
    ;;
package | store)
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    for arch in arm64 x86_64; do
        compile "$arch" "$binary.$arch" -O -parse-as-library "$here"/app/*.swift
    done
    lipo -create "$binary.arm64" "$binary.x86_64" -output "$binary"
    rm "$binary.arm64" "$binary.x86_64"
    ;;
probe) swiftc "$here/probe/main.swift" -o "$binary" ;;
session) compile "$(uname -m)" "$binary" "$here/session/main.swift" ;;
esac
[ -z "$BUILD_ONLY" ] || exit 0

# The version is the one the Windows app has.
version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$here/../package.json")
case "$tool" in
app | package | store)
    cp "$here"/app/models/*.usdz "$here"/app/models/portrait.png "$here/../src-tauri/icons/icon.icns" "$here/../LICENSE" "$app/Contents/Resources/"
    ;;
esac
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$id</string>
<key>CFBundleExecutable</key><string>$name</string>
<key>CFBundleName</key><string>$name</string>
<key>CFBundleIconFile</key><string>icon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>${BUILD:-$version}</string>
<key>ITSAppUsesNonExemptEncryption</key><false/>
<key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
<key>LSMinimumSystemVersion</key><string>15.0</string>
<key>LSUIElement</key><true/>
<key>NSBluetoothAlwaysUsageDescription</key><string>$why</string>
</dict></plist>
PLIST
# The hardened runtime is what Apple asks of an app before it will notarise it, and so is a
# timestamp of Apple's on a signature that is not ad hoc. codesign has something to say every
# time; only what it says when it fails is worth showing.
sign() {
    if ! said=$(codesign --force ${SIGN:+--timestamp} --sign "${SIGN:--}" "$@" 2>&1); then
        echo "$said" >&2
        exit 1
    fi
}
rights=
case "$tool" in
store)
    # The store takes sandboxed apps only, and Bluetooth is something a sandboxed app has to
    # ask for. A provisioning profile, which TestFlight insists on, brings two more lines: who
    # the app belongs to, in the profile's own words.
    rights="$here/build/store/entitlements.plist"
    whose=
    if [ -n "$PROFILE" ]; then
        cp "$PROFILE" "$app/Contents/embedded.provisionprofile"
        security cms -D -i "$PROFILE" > "$rights.profile"
        read_profile() { /usr/libexec/PlistBuddy -c "Print :Entitlements:$1" "$rights.profile"; }
        whose="<key>com.apple.application-identifier</key><string>$(read_profile com.apple.application-identifier)</string>
<key>com.apple.developer.team-identifier</key><string>$(read_profile com.apple.developer.team-identifier)</string>"
        rm "$rights.profile"
    fi
    cat > "$rights" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.app-sandbox</key><true/>
<key>com.apple.security.device.bluetooth</key><true/>
$whose
</dict></plist>
PLIST
    ;;
esac
# A file downloaded with a browser, as a provisioning profile is, carries a mark saying so, and
# a copy of it carries the mark on. The store turns a package down for one such file, so the
# marks come off everything in the app once it is all there.
xattr -cr "$app"
sign --options runtime ${rights:+--entitlements "$rights"} "$app"

case "$tool $*" in
"package "*)
    image="$here/build/Open-Headphones-Connect-$version.dmg"
    stage=$(mktemp -d)
    cp -R "$app" "$stage/"
    ln -s /Applications "$stage/Applications"
    hdiutil create -quiet -ov -volname "$name" -srcfolder "$stage" -format UDZO "$image"
    rm -rf "$stage"
    if [ -n "$NOTARY" ]; then
        : "${SIGN:?NOTARY needs SIGN, the name of a Developer ID Application certificate}"
        sign "$image"
        xcrun notarytool submit "$image" --keychain-profile "$NOTARY" --wait
        # Fails if Apple turned the app down, with the reason in what notarytool printed.
        xcrun stapler staple "$image"
    fi
    echo "$image"
    ;;
"store "*)
    installer="$here/build/Open-Headphones-Connect-$version.pkg"
    productbuild --component "$app" /Applications ${INSTALLER:+--sign "$INSTALLER"} "$installer" > /dev/null
    echo "$installer"
    ;;
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
