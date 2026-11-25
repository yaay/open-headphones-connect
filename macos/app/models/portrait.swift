// Draws the still of the 3D headphones that heads the panel, portrait.png beside this file, from
// headphones.usdz: the model at rest, lit as on the connect card, on a clear ground.
//
//     swift macos/app/models/portrait.swift                  # portrait.png, from headphones.usdz
//     swift macos/app/models/portrait.swift cmf-buds-pro-2   # portrait-cmf-buds-pro-2.png, from
//                                                            # headphones-cmf-buds-pro-2.usdz
//
// The picture is committed, so the app needs neither this nor a 3D view in the panel. Run it
// again when the model changes.

import AppKit
import RealityKit

let here = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
/// A model's own files, or the general ones.
let suffix = CommandLine.arguments.dropFirst().first.map { "-" + $0 } ?? ""
let output = "portrait\(suffix).png"
let size = NSRect(x: 0, y: 0, width: 184, height: 160)
_ = NSApplication.shared
let view = ARView(frame: size)
view.environment.background = .color(.clear)

func bothSides(_ entity: Entity) {
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

let model = try Entity.load(contentsOf: here.appendingPathComponent("headphones\(suffix).usdz"))
bothSides(model)
model.position = -model.visualBounds(relativeTo: nil).center
let pivot = Entity()
pivot.addChild(model)
// The resting pose of the card (HeadphonesView in Card.swift), filling nearly all of the picture.
pivot.orientation = simd_quatf(angle: 0.06, axis: [1, 0, 0]) * simd_quatf(angle: 0.62, axis: [0, 1, 0])
let anchor = AnchorEntity(world: .zero)
anchor.addChild(pivot)
let rest = pivot.visualBounds(relativeTo: nil).extents
let reach = tan(Float(12) * .pi / 180)
let distance = max(rest.y / 0.94 / (2 * reach), rest.x / 0.94 / (2 * reach * Float(size.width / size.height))) + rest.z / 2
let camera = PerspectiveCamera()
camera.camera.fieldOfViewInDegrees = 24
camera.look(at: .zero, from: [0, distance * 0.035, distance], relativeTo: nil)
anchor.addChild(camera)
for from in [SIMD3<Float>(-10, 16, 18), [14, 8, -16]] {
    let light = DirectionalLight()
    light.light.intensity = 9000
    light.look(at: .zero, from: from, relativeTo: nil)
    anchor.addChild(light)
}
view.scene.addAnchor(anchor)

let window = NSWindow(contentRect: size, styleMask: .borderless, backing: .buffered, defer: false)
window.contentView = view
DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
    view.snapshot(saveToHDR: false) { picture in
        guard let picture, let data = picture.tiffRepresentation, let bitmap = NSBitmapImageRep(data: data),
              let png = bitmap.representation(using: .png, properties: [:])
        else { fatalError("no picture") }
        try! png.write(to: here.appendingPathComponent(output))
        print("\(output): \(bitmap.pixelsWide) × \(bitmap.pixelsHigh)")
        exit(0)
    }
}
RunLoop.main.run(until: Date().addingTimeInterval(10))
print("timed out")
exit(1)
