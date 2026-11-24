// The card that slides in when the headphones connect: their name, a 3D model that spins into
// place, then the battery level and noise mode. The Mac counterpart of src/Card.tsx and
// src/Headphones3D.tsx.

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
    @State private var hovering = false

    var body: some View {
        let snapshot = headset.snapshot
        VStack(spacing: 2) {
            Text(snapshot.deviceName ?? "Sony headphones").font(.title2.weight(.semibold))
            Group {
                if live {
                    HeadphonesView()
                } else {
                    portrait.resizable().scaledToFit().padding(30)
                }
            }
            .frame(width: 280, height: 230)
            // A faint pool of light behind the headphones, so a black pair still stands out on
            // a dark card.
            .background(RadialGradient(colors: [.white.opacity(0.22), .clear], center: .center, startRadius: 0, endRadius: 120))
            .accessibilityHidden(true)
            if let battery = snapshot.headlineBattery {
                Label("\(battery.level)%" + (battery.charging ? ", charging" : ""), systemImage: batterySymbol(battery))
                    .font(.title3.weight(.semibold))
                    .monospacedDigit()
            }
            // In full strength: on the card's translucent ground, the secondary colour is too faint.
            Label {
                Text(status(snapshot))
            } icon: {
                Image(systemName: "dot.radiowaves.left.and.right").foregroundStyle(.secondary)
            }
            .font(.body.weight(.medium))
        }
        .padding(EdgeInsets(top: 20, leading: 20, bottom: 18, trailing: 20))
        .frame(width: 320)
        .modifier(CardSurface())
        // Room for the shadow, inside the window.
        .padding(18)
        .offset(x: shown || !live ? 0 : 40)
        .opacity(shown || !live ? 1 : 0)
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
            withAnimation(reduceMotion ? nil : .easeOut(duration: 0.26)) { shown = true }
            do {
                try await Task.sleep(for: .seconds(7))
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
        withAnimation(reduceMotion ? nil : .easeIn(duration: 0.16)) {
            shown = false
        } completion: {
            dismissWindow(id: CardScene.id)
        }
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

/// The card's ground: Liquid Glass where the system has it, as a notification has, and the
/// frosted material with a shadow before that.
private struct CardSurface: ViewModifier {
    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: 22, style: .continuous)
        if #available(macOS 26.0, *) {
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

    /// What keeps the model moving, held for as long as the view is.
    private final class Motion {
        var subscription: EventSubscription?
        var elapsed: Float = 0
    }

    @State private var motion = Motion()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        RealityView { content in
            guard let file = Bundle.main.url(forResource: "headphones", withExtension: "usdz"),
                  let model = try? await Entity(contentsOf: file)
            else { return }
            Self.bothSides(model)

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
