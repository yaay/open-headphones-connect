// The card that slides in when the headphones connect: their name, a 3D model that spins into
// place, then the battery level and noise mode. The Mac counterpart of src/Card.tsx and
// src/Headphones3D.tsx.

import AppKit
import Combine
import RealityKit
import SwiftUI

/// The card's window: the app's own floating panel, which takes neither focus nor clicks away
/// from what the user is doing.
enum Card {
    private static var panel: NSPanel?
    private static var leaving: DispatchWorkItem?
    static var hovering = false

    static func show(_ headset: Headset) {
        dismiss(animated: false)
        let content = NSHostingView(rootView: CardView(headset: headset))
        let size = content.fittingSize
        let panel = NSPanel(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.contentView = content
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        self.panel = panel

        // Where notifications appear: under the menu bar at the right, sliding in from the edge.
        let screen = (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
        let rest = NSRect(x: screen.maxX - size.width - 12, y: screen.maxY - size.height - 12, width: size.width, height: size.height)
        panel.setFrame(rest.offsetBy(dx: 40, dy: 0), display: false)
        panel.alphaValue = 0
        panel.orderFrontRegardless()
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.26
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            panel.animator().setFrame(rest, display: true)
            panel.animator().alphaValue = 1
        }
        stay(7)
    }

    /// Stays for a few seconds, longer while the pointer rests on it.
    private static func stay(_ seconds: TimeInterval) {
        let work = DispatchWorkItem {
            if hovering { stay(1.2) } else { dismiss(animated: true) }
        }
        leaving = work
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }

    static func dismiss(animated: Bool) {
        leaving?.cancel()
        hovering = false
        guard let panel else { return }
        self.panel = nil
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = animated ? 0.16 : 0
            panel.animator().alphaValue = 0
        }, completionHandler: { panel.orderOut(nil) })
    }
}

struct CardView: View {
    let headset: Headset

    var body: some View {
        let snapshot = headset.snapshot
        VStack(spacing: 2) {
            Text(snapshot.deviceName ?? "Sony headphones").font(.title2.weight(.semibold))
            HeadphonesView()
                .frame(height: 230)
                // A faint pool of light behind the headphones, so a black pair still stands out
                // on a dark card.
                .background(RadialGradient(colors: [.white.opacity(0.22), .clear], center: .center, startRadius: 0, endRadius: 120))
                .accessibilityHidden(true)
            if let battery = snapshot.headlineBattery {
                Label("\(battery.level)%" + (battery.charging ? ", charging" : ""), systemImage: batterySymbol(battery))
                    .font(.title3.weight(.semibold))
                    .monospacedDigit()
            }
            Label(status(snapshot), systemImage: "dot.radiowaves.left.and.right").foregroundStyle(.secondary)
        }
        .padding(EdgeInsets(top: 20, leading: 20, bottom: 18, trailing: 20))
        .frame(width: 320)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 22))
        .contentShape(RoundedRectangle(cornerRadius: 22))
        .onHover { Card.hovering = $0 }
        .onTapGesture { Card.dismiss(animated: true) }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityHint("Closes this card")
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

/// The 3D headphones, arriving with a spin and then swaying a little where they stand. The
/// numbers are those of src/Headphones3D.tsx.
struct HeadphonesView: NSViewRepresentable {
    // The view the model comes to rest in: upright, turned a little to one side so the outside of
    // one ear cup and the pad of the other both show.
    private static let restYaw: Float = 0.62
    private static let restPitch: Float = 0.06
    /// It arrives from most of a turn away.
    private static let spin: Float = .pi * 1.35
    /// How long the spin-in takes before the model settles.
    private static let settle: Float = 1.7
    private static let fieldOfView: Float = 24

    /// Loaded once; every card gets a copy.
    private static let model: Entity? = {
        guard let file = Bundle.main.url(forResource: "headphones", withExtension: "usdz"), let model = try? Entity.load(contentsOf: file) else {
            return nil
        }
        bothSides(model)
        return model
    }()

    /// The model's surfaces are meant to be seen from both sides (see `side` in
    /// src/headphoneModel.ts). The file says so, but RealityKit does not go by that.
    private static func bothSides(_ entity: Entity) {
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

    final class Motion {
        var update: Cancellable?
        var elapsed: Float = 0
    }

    func makeCoordinator() -> Motion { Motion() }

    func makeNSView(context: Context) -> ARView {
        let view = ARView(frame: NSRect(x: 0, y: 0, width: 280, height: 230))
        // Clear, so that the card shows through around the model.
        view.environment.background = .color(.clear)
        guard let model = Self.model?.clone(recursive: true) else { return view }

        let pivot = Entity()
        let bounds = model.visualBounds(relativeTo: nil)
        model.position = -bounds.center
        pivot.addChild(model)
        let anchor = AnchorEntity(world: .zero)
        anchor.addChild(pivot)

        // Far enough for the resting model to fill a good part of the view, whatever its
        // proportions, with room to spare for the entrance.
        pivot.orientation = Self.pose(yaw: Self.restYaw, pitch: Self.restPitch)
        let rest = pivot.visualBounds(relativeTo: nil).extents
        let reach = tan(Self.fieldOfView / 2 * .pi / 180)
        let aspect = Float(view.bounds.width / view.bounds.height)
        let distance = max(rest.y / 0.74 / (2 * reach), rest.x / 0.74 / (2 * reach * aspect)) + rest.z / 2
        let camera = PerspectiveCamera()
        camera.camera.fieldOfViewInDegrees = Self.fieldOfView
        camera.camera.near = 1
        camera.camera.far = 400
        camera.look(at: .zero, from: [0, distance * 0.035, distance], relativeTo: nil)
        anchor.addChild(camera)

        // A key light from the upper front left and a rim light from behind, so dark headphones
        // keep a bright edge against a dark card.
        for (from, colour, strength) in [(SIMD3<Float>(-10, 16, 18), NSColor.white, Float(9000)), ([14, 8, -16], NSColor(red: 0.87, green: 0.91, blue: 1, alpha: 1), 9000)] {
            let light = DirectionalLight()
            light.light.color = colour
            light.light.intensity = strength
            light.look(at: .zero, from: from, relativeTo: nil)
            anchor.addChild(light)
        }
        view.scene.addAnchor(anchor)

        let still = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        let motion = context.coordinator
        motion.update = view.scene.subscribe(to: SceneEvents.Update.self) { event in
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
        return view
    }

    func updateNSView(_ view: ARView, context: Context) {}

    /// Tilt first, then turn: the turn stays about the model's own upright axis.
    private static func pose(yaw: Float, pitch: Float) -> simd_quatf {
        simd_quatf(angle: pitch, axis: [1, 0, 0]) * simd_quatf(angle: yaw, axis: [0, 1, 0])
    }
}
