// Open Headphones Connect for the Mac: an item in the menu bar, the panel it opens, and the card
// that appears when the headphones connect.
//
//     macos/run.sh app                              # with your headphones
//     macos/run.sh app --demo WH-1000XM5            # with made-up ones; Bluetooth is left alone.
//                                                   # Disconnect and Connect replay the card.
//     macos/run.sh app --demo WF-1000XM5 --render panel.png [card] [dark] [refused] [--scale 4]
//                                                   # a picture of the panel or the card, then
//                                                   # quit; `refused` as if Bluetooth were not
//                                                   # allowed, `--scale` for more pixels than the
//                                                   # screen's 2
//     … --render panel.png --send '{"set_eq_preset": 16}'
//                                                   # after that command, or several, has been sent

import AppKit
import SwiftUI

let headset = Headset()

@main
enum Main {
    static func main() {
        let arguments = CommandLine.arguments
        func value(after flag: String) -> String? {
            arguments.firstIndex(of: flag).flatMap { $0 + 1 < arguments.count ? arguments[$0 + 1] : nil }
        }
        let demo = value(after: "--demo")
        UserDefaults.standard.register(defaults: [Preference.connectCard: true])
        if let picture = value(after: "--render") {
            headset.start(demo: demo)
            // Long enough for a real headset to have said what it has; a made-up one is quick.
            // Counted in turns of the run loop, so that time spent stuck behind the Bluetooth
            // permission question does not count.
            for _ in 0..<(demo == nil ? 60 : 15) {
                RunLoop.main.run(until: Date().addingTimeInterval(0.1))
            }
            for (at, argument) in arguments.enumerated() where argument == "--send" && at + 1 < arguments.count {
                HeadsetSession.send(arguments[at + 1])
                RunLoop.main.run(until: Date().addingTimeInterval(0.5))
            }
            let card = arguments.contains("card")
            let refused = Panel.bluetoothRefused || arguments.contains("refused")
            let panel = Panel(headset: headset, refused: refused).background(Color(nsColor: .windowBackgroundColor))
            render(card ? AnyView(CardView(headset: headset, live: false)) : AnyView(panel),
                   to: picture, dark: arguments.contains("dark"), scale: value(after: "--scale").flatMap(Int.init) ?? 2)
        } else {
            Shortcut.shared.action = { headset.toggleNoise() }
            Shortcut.shared.enabled = UserDefaults.standard.bool(forKey: Preference.shortcut)
            headset.start(demo: demo)
            OpenHeadphonesConnect.main()
        }
    }

    /// Draws a view, with everything in it opened up, into a PNG file: a look at it without a
    /// screen, and without a headset when it is a made-up one. A 3D view cannot be drawn this
    /// way, so the card shows its still picture instead.
    private static func render(_ content: AnyView, to path: String, dark: Bool, scale: Int) {
        UserDefaults.standard.register(defaults: ["eq-open": true, "headphone-settings-open": true])
        let controller = NSHostingController(rootView: content)
        controller.safeAreaRegions = []
        let view = controller.view
        view.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        // In a window, so that the controls draw as they do on screen.
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: view.fittingSize), styleMask: .borderless, backing: .buffered, defer: false)
        window.contentView = view
        view.layoutSubtreeIfNeeded()
        // The panel measures itself and then takes its size, which needs a turn of the run loop.
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        // The least size the view accepts, which is the size a menu-bar window gives it. A view
        // that can shrink shows here as it would there: shrunk.
        window.setContentSize(controller.sizeThatFits(in: .zero))
        view.layoutSubtreeIfNeeded()
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int((view.bounds.width * CGFloat(scale)).rounded()), pixelsHigh: Int((view.bounds.height * CGFloat(scale)).rounded()),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        bitmap.size = view.bounds.size
        view.cacheDisplay(in: view.bounds, to: bitmap)
        try! bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
    }
}

struct OpenHeadphonesConnect: App {
    var body: some SwiftUI.Scene {
        MenuBarExtra {
            Panel(headset: headset)
        } label: {
            MenuBarLabel()
        }
        .menuBarExtraStyle(.window)
        CardScene()
    }
}

/// The item in the menu bar: the headphones, with the battery beside them as in the Windows
/// tray. It is always there, so it is also what opens the card when they connect afresh.
private struct MenuBarLabel: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "headphones")
            if let battery = headset.snapshot.headlineBattery {
                Text("\(battery.level)%")
            }
        }
        .onChange(of: headset.arrivals) {
            if UserDefaults.standard.bool(forKey: Preference.connectCard) {
                openWindow(id: CardScene.id)
            }
        }
    }
}
