// Open Headphones Connect for the Mac: an item in the menu bar, the panel it opens, and the card
// that appears when the headphones connect.
//
//     macos/run.sh app                              # with your headphones
//     macos/run.sh app --demo WH-1000XM5            # with made-up ones; Bluetooth is left alone.
//                                                   # Disconnect and Connect replay the card.
//     macos/run.sh app --demo WF-1000XM5 --render panel.png [card] [dark] [refused] [--scale 4]
//                                                   # a picture of the panel or the card, then quit;
//                                                   # `refused` as if Bluetooth were not allowed,
//                                                   # `--scale` for more pixels than the screen's 2
//     … --render panel.png --send '{"set_eq_preset": 16}'
//                                                   # after that command, or several, has been sent

import AppKit
import RealityKit
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
            render(card ? AnyView(CardView(headset: headset)) : AnyView(Panel(headset: headset, refused: refused).background(Color(nsColor: .windowBackgroundColor))),
                   to: picture, dark: arguments.contains("dark"), scale: value(after: "--scale").flatMap(Int.init) ?? 2)
        } else {
            headset.onConnect = { Card.show(headset) }
            headset.start(demo: demo)
            OpenHeadphonesConnect.main()
        }
    }

    /// Draws a view, with everything in it opened up, into a PNG file: a look at it without a
    /// screen, and without a headset when it is a made-up one.
    private static func render(_ content: AnyView, to path: String, dark: Bool, scale: Int) {
        UserDefaults.standard.register(defaults: ["eq-open": true, "headphone-settings-open": true])
        let view = NSHostingView(rootView: content)
        view.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        view.safeAreaRegions = []
        // In a window, so that the controls draw as they do on screen.
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: view.fittingSize), styleMask: .borderless, backing: .buffered, defer: false)
        window.contentView = view
        view.layoutSubtreeIfNeeded()
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int((view.bounds.width * CGFloat(scale)).rounded()), pixelsHigh: Int((view.bounds.height * CGFloat(scale)).rounded()),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        bitmap.size = view.bounds.size
        view.cacheDisplay(in: view.bounds, to: bitmap)

        // The 3D model draws itself apart from the rest, so its picture is asked for and laid in.
        func models(in view: NSView) -> [ARView] {
            view.subviews.flatMap { ($0 as? ARView).map { [$0] } ?? models(in: $0) }
        }
        for model in models(in: view) {
            // Once it has spun into place.
            RunLoop.main.run(until: Date().addingTimeInterval(2.5))
            var picture: NSImage?
            var answered = false
            model.snapshot(saveToHDR: false) { picture = $0; answered = true }
            while !answered {
                RunLoop.main.run(until: Date().addingTimeInterval(0.05))
            }
            var place = model.convert(model.bounds, to: view)
            place.origin.y = view.bounds.height - place.maxY
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
            picture?.draw(in: place)
            NSGraphicsContext.restoreGraphicsState()
        }
        try! bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
    }
}

struct OpenHeadphonesConnect: App {
    var body: some SwiftUI.Scene {
        MenuBarExtra {
            Panel(headset: headset)
        } label: {
            // The battery beside the icon, as in the Windows tray.
            Image(systemName: "headphones")
            if let battery = headset.snapshot.headlineBattery {
                Text("\(battery.level)%")
            }
        }
        .menuBarExtraStyle(.window)
    }
}
