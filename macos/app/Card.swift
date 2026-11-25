// The card that slides in when the headphones connect: their name, a 3D model that spins into
// place, then the battery level and noise mode. The Mac counterpart of src/Card.tsx and
// src/Headphones3D.tsx.

import AVFoundation
import RealityKit
import SwiftUI

/// The card's window: no frame, above other windows, in the top right corner. The menu-bar
/// item opens it when the headphones connect afresh.
struct CardScene: SwiftUI.Scene {
    static let id = "card"

    var body: some SwiftUI.Scene {
        Window("Open Headphones Connect", id: Self.id) {
            CardView(headset: headset)
        }
        .windowStyle(.plain)
        .windowLevel(.floating)
        .windowResizability(.contentSize)
        .restorationBehavior(.disabled)
        .defaultLaunchBehavior(.suppressed)
        .defaultWindowPlacement { content, _ in
            WindowPlacement(.topTrailing, size: content.sizeThatFits(.unspecified))
        }
    }
}

struct CardView: View {
    let headset: Headset
    /// False while it is drawn off screen, where a 3D view cannot be.
    var live = true
    @Environment(\.dismissWindow) private var dismissWindow
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false
    /// The battery and the status, which follow the card in once the headphones are on their way.
    @State private var revealed = false
    @State private var hovering = false
    /// The ground of this model's clip, which the card takes on so the clip has no edges on it.
    @State private var ground: Color?
    /// Whether the 3D model has loaded: the glow behind it waits for it, so that it never shows
    /// alone, like a loading spinner.
    @State private var modelReady = false

    var body: some View {
        let snapshot = headset.snapshot
        let shown = self.shown || !live
        let revealed = self.revealed || !live
        VStack(spacing: 2) {
            Text(snapshot.deviceName ?? "Sony headphones").font(.title2.weight(.semibold))
            Group {
                // A new identity for every arrival, so the model is built again and spins in
                // again, or the clip plays again; otherwise the window keeps the last one, at rest.
                if live, !reduceMotion, let clip = CardClip.file(for: snapshot.model) {
                    // Its edges fade into the card, which has its colour.
                    CardClip(url: clip)
                        .mask { Rectangle().padding(16).blur(radius: 14) }
                        .id(headset.arrivals)
                } else if live {
                    HeadphonesView(model: snapshot.model) { modelReady = true }.id(headset.arrivals)
                } else {
                    portrait(for: snapshot.model).resizable().scaledToFit().padding(30)
                }
            }
            .frame(width: 280, height: 230)
            // A faint pool of light behind the headphones, so a black pair still stands out on
            // a dark card. It swells in with them.
            .background {
                let glowing = revealed && ground == nil && (modelReady || !live)
                RadialGradient(colors: [.white.opacity(0.22), .clear], center: .center, startRadius: 0, endRadius: 120)
                    .scaleEffect(glowing ? 1 : 0.4)
                    .opacity(glowing ? 1 : 0)
                    .animation(motion(.easeOut(duration: 1.1)), value: glowing)
            }
            .accessibilityHidden(true)
            if let battery = snapshot.headlineBattery {
                // The figure rolls up from nothing as it arrives, and rolls on when it changes.
                let level = revealed ? battery.level : 0
                Label("\(level)%" + (battery.charging ? ", charging" : ""), systemImage: batterySymbol(battery))
                    .font(.title3.weight(.semibold))
                    .monospacedDigit()
                    .contentTransition(.numericText(value: Double(level)))
                    .rise(revealed, motion(.spring(duration: 0.7, bounce: 0.25).delay(0.45)))
            }
            // In full strength: on the card's translucent ground, the secondary colour is too faint.
            Label {
                Text(status(snapshot))
            } icon: {
                Image(systemName: "dot.radiowaves.left.and.right")
                    .foregroundStyle(.secondary)
                    .symbolEffect(.variableColor.iterative, options: .nonRepeating, value: revealed)
            }
            .font(.body.weight(.medium))
            .rise(revealed, motion(.spring(duration: 0.6, bounce: 0.25).delay(0.65)))
        }
        .padding(EdgeInsets(top: 20, leading: 20, bottom: 18, trailing: 20))
        .frame(width: 320)
        .modifier(CardSurface(offScreen: !live, ground: ground))
        // Room for the shadow, inside the window.
        .padding(18)
        .scaleEffect(shown ? 1 : 0.94, anchor: .topTrailing)
        .offset(x: shown ? 0 : 60)
        .opacity(shown ? 1 : 0)
        .containerBackground(.clear, for: .window)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .onTapGesture(perform: leave)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityHint("Closes this card")
        // Each fresh connection starts the stay again: seven seconds, longer while the pointer
        // rests on the card.
        .task(id: headset.arrivals) {
            // Long enough for a clip to play to its end.
            let clip = reduceMotion ? nil : CardClip.file(for: headset.snapshot.model)
            let length = await clip.asyncMap { try? await AVURLAsset(url: $0).load(.duration).seconds } ?? nil
            ground = await clip.asyncMap(CardClip.ground) ?? nil
            modelReady = false
            self.revealed = false
            withAnimation(motion(.spring(duration: 0.55, bounce: 0.3))) { self.shown = true }
            self.revealed = true
            do {
                try await Task.sleep(for: .seconds(max(7, (length ?? 0) + 1)))
                while hovering {
                    try await Task.sleep(for: .seconds(1.2))
                }
            } catch {
                return
            }
            leave()
        }
    }

    private func leave() {
        withAnimation(motion(.easeIn(duration: 0.16))) {
            shown = false
        } completion: {
            revealed = false
            dismissWindow(id: CardScene.id)
        }
    }

    /// None at all where the user asked for less motion.
    private func motion(_ animation: Animation) -> Animation? {
        reduceMotion ? nil : animation
    }

    private func status(_ snapshot: Snapshot) -> String {
        guard let noise = snapshot.noise else { return "Connected" }
        switch noise.mode {
        case .noiseCancelling: return "Noise cancelling is on"
        case .wind: return "Wind noise reduction is on"
        // Some models have ambient sound without a level.
        case .ambient: return snapshot.maxAmbientLevel > 0 ? "Ambient sound, level \(noise.ambientLevel - snapshot.minAmbientLevel + 1)" : "Ambient sound is on"
        case .off: return "Noise control is off"
        }
    }
}

private extension View {
    /// Fades in and rises a little into place.
    func rise(_ on: Bool, _ animation: Animation?) -> some View {
        opacity(on ? 1 : 0)
            .offset(y: on ? 0 : 10)
            .animation(animation, value: on)
    }
}

/// The card's ground: Liquid Glass where the system has it, as a notification has, and the
/// frosted material with a shadow before that. Drawn off screen, where neither can be, it is the
/// window's own colour, which is what the glass shows over a plain dark desktop.
private struct CardSurface: ViewModifier {
    let offScreen: Bool
    /// A clip's own ground, which the card takes on instead, with light text on it.
    var ground: Color?

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: 22, style: .continuous)
        if let ground {
            content
                .environment(\.colorScheme, .dark)
                .background(ground, in: shape)
                .shadow(color: .black.opacity(0.25), radius: 14, y: 6)
        } else if offScreen {
            content
                .background(Color(nsColor: .windowBackgroundColor), in: shape)
                .shadow(color: .black.opacity(0.25), radius: 14, y: 6)
        } else if #available(macOS 26.0, *) {
            content.glassEffect(.regular, in: shape)
        } else {
            content
                .background(.regularMaterial, in: shape)
                .shadow(color: .black.opacity(0.25), radius: 14, y: 6)
        }
    }
}

/// The 3D headphones, arriving with a spin and then swaying a little where they stand. The
/// numbers are those of src/Headphones3D.tsx.
struct HeadphonesView: View {
    // The view the model comes to rest in: upright, turned a little to one side so the outside of
    // one ear cup and the pad of the other both show.
    private static let restYaw: Float = 0.62
    private static let restPitch: Float = 0.06
    /// It arrives from most of a turn away.
    private static let spin: Float = .pi * 1.35
    /// How long the spin-in takes before the model settles.
    private static let settle: Float = 1.7
    private static let fieldOfView: Float = 24
    /// How much of the view the resting model fills, leaving room for the entrance.
    private static let fill: Float = 0.74
    private static let aspect: Float = 280 / 230
    /// The headphones' model name: those with their own 3D file get it, the rest the general one.
    var model: String?
    /// Called once the model is in place.
    var onLoad: () -> Void = {}

    /// Models already read from disk, so that the next card has one at once.
    @MainActor private static var loaded: [URL: Entity] = [:]

    /// The longest side of the general model, in its own units. Models made elsewhere come in
    /// theirs (metres, from Blender) and are brought to this size, which the camera below and
    /// the distances the model moves are set for.
    private static let size: Float = 20

    /// What keeps the model moving, held for as long as the view is.
    private final class Motion {
        var subscription: EventSubscription?
        var elapsed: Float = 0
    }

    @State private var motion = Motion()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        RealityView { content in
            let own = resourceName("headphones", for: model).flatMap { Bundle.main.url(forResource: $0, withExtension: "usdz") }
            guard let file = own ?? Bundle.main.url(forResource: "headphones", withExtension: "usdz") else { return }
            if Self.loaded[file] == nil {
                Self.loaded[file] = try? await Entity(contentsOf: file)
            }
            guard let model = Self.loaded[file]?.clone(recursive: true) else { return }
            Self.bothSides(model)
            let longest = model.visualBounds(relativeTo: nil).extents.max()
            if longest > 0 {
                model.scale *= SIMD3(repeating: Self.size / longest)
            }

            let pivot = Entity()
            model.position = -model.visualBounds(relativeTo: nil).center
            pivot.addChild(model)

            // Far enough for the resting model to fill that share of the view, whatever its
            // proportions.
            pivot.orientation = Self.pose(yaw: Self.restYaw, pitch: Self.restPitch)
            let rest = pivot.visualBounds(relativeTo: nil).extents
            let reach = tan(Self.fieldOfView / 2 * .pi / 180)
            let distance = max(rest.y / Self.fill / (2 * reach), rest.x / Self.fill / (2 * reach * Self.aspect)) + rest.z / 2
            let camera = PerspectiveCamera()
            camera.camera.fieldOfViewInDegrees = Self.fieldOfView
            camera.camera.near = 1
            camera.camera.far = 400
            camera.look(at: .zero, from: [0, distance * 0.035, distance], relativeTo: nil)
            content.add(camera)

            // A key light from the upper front left and a rim light from behind, so dark
            // headphones keep a bright edge against a dark card.
            for (from, strength) in [(SIMD3<Float>(-10, 16, 18), Float(9000)), ([14, 8, -16], 9000)] {
                let light = DirectionalLight()
                light.light.intensity = strength
                light.look(at: .zero, from: from, relativeTo: nil)
                content.add(light)
            }
            content.add(pivot)
            onLoad()

            let still = reduceMotion
            let motion = motion
            motion.subscription = content.subscribe(to: SceneEvents.Update.self) { event in
                motion.elapsed += Float(event.deltaTime)
                let t = still ? 1 : min(1, motion.elapsed / Self.settle)
                let turn = 1 - pow(1 - t, 5)
                let idle = still ? 0 : max(0, motion.elapsed - Self.settle)
                pivot.orientation = Self.pose(
                    yaw: Self.restYaw - (1 - turn) * Self.spin + 0.17 * sin(idle * 0.75),
                    pitch: Self.restPitch + (1 - turn) * 0.35 + 0.02 * sin(idle * 0.9 + 1))
                pivot.position.y = -(1 - turn) * 3.2 + 0.1 * sin(idle * 1.1)
                // It grows as it comes, a touch past its size and back.
                let grow = still ? 1 : min(1, motion.elapsed / (Self.settle * 0.62))
                pivot.scale = SIMD3(repeating: 0.62 + 0.38 * (1 + 2.2 * pow(grow - 1, 3) + 1.2 * pow(grow - 1, 2)))
            }
        }
    }

    /// The model's surfaces are meant to be seen from both sides (see `side` in
    /// src/headphoneModel.ts). The file says so, but RealityKit does not go by that.
    static func bothSides(_ entity: Entity) {
        if var part = entity.components[ModelComponent.self] {
            part.materials = part.materials.map { material in
                guard var surface = material as? PhysicallyBasedMaterial else { return material }
                surface.faceCulling = .none
                return surface
            }
            entity.components.set(part)
        }
        entity.children.forEach(bothSides)
    }

    /// Tilt first, then turn: the turn stays about the model's own upright axis.
    static func pose(yaw: Float, pitch: Float) -> simd_quatf {
        simd_quatf(angle: pitch, axis: [1, 0, 0]) * simd_quatf(angle: yaw, axis: [0, 1, 0])
    }
}

/// A clip of the headphones, for models that have one in place of a 3D model: played once from
/// the start each time the card arrives, then held on its last frame. It has no sound, and is
/// muted besides, so it never takes the headphones' audio away from another device.
struct CardClip: NSViewRepresentable {
    let url: URL

    /// This model's clip, if it has one: card-<name>.mov or .mp4 in the app's resources.
    static func file(for model: String?) -> URL? {
        guard let name = resourceName("card", for: model) else { return nil }
        return Bundle.main.url(forResource: name, withExtension: "mov") ?? Bundle.main.url(forResource: name, withExtension: "mp4")
    }

    func makeNSView(context: Context) -> NSView {
        let player = AVPlayer(url: url)
        player.isMuted = true
        player.preventsDisplaySleepDuringVideoPlayback = false
        player.actionAtItemEnd = .pause
        let layer = AVPlayerLayer(player: player)
        // Filling the picture's place on the card.
        layer.videoGravity = .resizeAspectFill
        layer.backgroundColor = .clear
        let view = NSView()
        view.layer = layer
        view.wantsLayer = true
        player.play()
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {}

    /// The colour of the clip's ground: its first frame's top left corner.
    static func ground(of url: URL) async -> Color? {
        let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        guard let (image, _) = try? await generator.image(at: .zero),
              let corner = image.cropping(to: CGRect(x: 4, y: 4, width: 1, height: 1)),
              let colour = NSBitmapImageRep(cgImage: corner).colorAt(x: 0, y: 0)
        else { return nil }
        return Color(nsColor: colour)
    }

    static func dismantleNSView(_ view: NSView, coordinator: ()) {
        (view.layer as? AVPlayerLayer)?.player?.pause()
    }
}

private extension Optional {
    func asyncMap<T>(_ transform: (Wrapped) async -> T) async -> T? {
        guard let value = self else { return nil }
        return await transform(value)
    }
}
