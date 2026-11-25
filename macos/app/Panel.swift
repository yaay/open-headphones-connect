// The panel that opens from the menu bar: the Mac counterpart of src/Panel.tsx.
//
// Laid out top to bottom as a picture of the headphones with their batteries, the controls, and
// a foot that stays put. Only the controls scroll, and only when the screen is too short for
// them. Everything in it is a stock SwiftUI control in its stock style.

import AppKit
import CoreBluetooth
import SwiftUI

struct Panel: View {
    let headset: Headset
    /// Whether the user has said no to Bluetooth for this app. The session cannot tell that
    /// from Bluetooth being off, or from there being nothing paired.
    var refused = Panel.bluetoothRefused
    @State private var headHeight: CGFloat = 210
    @State private var footHeight: CGFloat = 44

    static var bluetoothRefused: Bool {
        [.denied, .restricted].contains(CBCentralManager.authorization)
    }

    var body: some View {
        let snapshot = headset.snapshot
        VStack(spacing: 0) {
            Header(snapshot: snapshot, refused: refused)
                .padding(EdgeInsets(top: 18, leading: 20, bottom: 14, trailing: 20))
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { headHeight = $0 }
            FittedScroll(room: screenRoom - headHeight - footHeight) {
                Controls(headset: headset, snapshot: snapshot, refused: refused)
            }
            VStack(spacing: 0) {
                Divider()
                Footer(headset: headset, snapshot: snapshot, refused: refused)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { footHeight = $0 }
        }
        .frame(width: 340)
    }
}

/// How tall the panel may be: the screen less a little room.
private var screenRoom: CGFloat { (NSScreen.main?.visibleFrame.height ?? 900) - 16 }

/// A scroller that is exactly as tall as what is in it, or as `room` if that is less.
///
/// A menu-bar window takes the least height its content will accept, and a plain scroller
/// accepts none at all, so the panel would collapse to its head and foot. The height is kept
/// from one opening of the panel to the next, so that it opens at its size.
private struct FittedScroll<Content: View>: View {
    let room: CGFloat
    @ViewBuilder let content: Content
    @State private var height = lastHeight

    var body: some View {
        ScrollView {
            content.onGeometryChange(for: CGFloat.self) { $0.size.height } action: { measured in
                height = measured
                lastHeight = measured
            }
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(height: min(height, max(120, room)))
    }
}

private var lastHeight: CGFloat = 0

/// The app's own files for one model of headphones go by its name: for "CMF Buds Pro 2",
/// headphones-cmf-buds-pro-2.usdz, portrait-cmf-buds-pro-2.png and card-cmf-buds-pro-2.mov.
/// Models without their own use the general ones.
func resourceName(_ kind: String, for model: String?) -> String? {
    guard let model else { return nil }
    let words = model.lowercased().split { !$0.isLetter && !$0.isNumber }.map(String.init)
    return words.isEmpty ? nil : ([kind] + words).joined(separator: "-")
}

/// The still of these headphones in the app's resources, or of the general 3D model.
/// `Image("portrait")` would look for it in an asset catalog, which the app does not have.
func portrait(for model: String?) -> Image {
    let own = resourceName("portrait", for: model).flatMap { NSImage(named: $0) }
    return (own ?? NSImage(named: "portrait")).map { Image(nsImage: $0) } ?? Image(systemName: "headphones")
}

func openBluetoothSettings() {
    NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.BluetoothSettings")!)
}

// MARK: - Header

private struct Header: View {
    let snapshot: Snapshot
    let refused: Bool

    private var connected: Bool { snapshot.phase == .connected && !refused }
    private var busy: Bool {
        !refused && (snapshot.phase == .connecting || snapshot.phase == .starting || snapshot.pending != nil)
    }

    var body: some View {
        // The name, and under it the state and the batteries, on the left; the headphones on the
        // right.
        HStack(alignment: .top, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(snapshot.deviceName ?? "Sony headphones")
                    .font(.title2.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                if !subtitle.isEmpty {
                    Text(subtitle).font(.callout).foregroundStyle(.secondary)
                }
                Spacer(minLength: 16)
                HStack(spacing: 6) {
                    if busy {
                        ProgressView().controlSize(.mini).accessibilityLabel("Working")
                    } else {
                        Circle().fill(connected ? Color.green : Color.secondary).frame(width: 7, height: 7).accessibilityHidden(true)
                    }
                    Text(status).font(.callout.weight(.medium))
                }
                if connected {
                    VStack(alignment: .leading, spacing: 4) {
                        battery(snapshot.battery.single, nil)
                        battery(snapshot.battery.left, "L")
                        battery(snapshot.battery.right, "R")
                        battery(snapshot.battery.case, "Case")
                    }
                    .padding(.top, 6)
                }
            }
            .frame(maxHeight: .infinity, alignment: .topLeading)
            Spacer(minLength: 0)
            // A still of the connect card's 3D model, made by models/portrait.swift.
            portrait(for: snapshot.model)
                .resizable()
                .interpolation(.high)
                .scaledToFit()
                .frame(width: 150, height: 130)
                .opacity(connected ? 1 : 0.45)
                .accessibilityHidden(true)
        }
        .frame(minHeight: 130)
    }

    /// The model, where the name is the owner's own, and the codec.
    private var subtitle: String {
        [snapshot.model.flatMap { $0 == snapshot.deviceName ? nil : $0 }, connected ? snapshot.codec : nil]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    private var status: String {
        if refused { return "Not allowed to use Bluetooth" }
        if snapshot.pending == "connecting_audio" { return "Connecting…" }
        if snapshot.pending == "disconnecting" { return "Disconnecting…" }
        switch snapshot.phase {
        case .starting: return "Looking…"
        case .bluetoothOff: return "Bluetooth is off"
        case .noDevice: return "Not paired yet"
        case .disconnected: return "Not connected"
        case .connecting: return "Connecting…"
        case .connected: return "Connected"
        }
    }

    @ViewBuilder
    private func battery(_ battery: Battery?, _ slot: String?) -> some View {
        if let battery {
            let low = battery.level <= 20 && !battery.charging
            Label {
                Text([slot, "\(battery.level)%"].compactMap { $0 }.joined(separator: " ")).monospacedDigit()
            } icon: {
                Image(systemName: batterySymbol(battery))
            }
            .font(.callout.weight(.medium))
            .foregroundStyle(low ? .red : .primary)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel([slot == "L" ? "Left" : slot == "R" ? "Right" : slot, "battery \(battery.level)%", battery.charging ? "charging" : nil].compactMap { $0 }.joined(separator: ", "))
        }
    }
}

func batterySymbol(_ battery: Battery) -> String {
    if battery.charging { return "battery.100percent.bolt" }
    // The nearest of the five drawings there are.
    return "battery.\(min(4, (battery.level + 12) / 25) * 25)percent"
}

// MARK: - Controls

private struct Controls: View {
    let headset: Headset
    let snapshot: Snapshot
    let refused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let notice = snapshot.notice {
                Label(notice, systemImage: "info.circle")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            switch refused ? nil : snapshot.phase {
            case nil:
                Prompt(
                    text: "Open Headphones Connect is not allowed to use Bluetooth, so it cannot see your headphones. Allow it under Privacy & Security, then quit and open it again.",
                    button: "Open Privacy Settings…",
                    action: { NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth")!) })
            case .starting, .connecting:
                EmptyView()
            case .bluetoothOff:
                Prompt(text: "Turn Bluetooth on to use your headphones with this Mac.", button: "Open Bluetooth Settings…", action: openBluetoothSettings)
            case .noDevice:
                Prompt(
                    text: "Pair your Sony headphones with this Mac first. Hold their power button for 7 seconds until the light flashes blue, then add them in Bluetooth settings.",
                    button: "Open Bluetooth Settings…", action: openBluetoothSettings)
            case .disconnected:
                let connecting = snapshot.pending == "connecting_audio"
                Prompt(text: "Switch the headphones on, then connect.", button: connecting ? "Connecting…" : "Connect", action: headset.connect)
                    .disabled(connecting)
            case .connected:
                if let noise = snapshot.noise {
                    NoiseControl(headset: headset, snapshot: snapshot, noise: noise)
                }
                if let eq = snapshot.eq {
                    Equalizer(headset: headset, snapshot: snapshot, eq: eq)
                }
                if !snapshot.devices.isEmpty {
                    Devices(headset: headset, snapshot: snapshot)
                }
                HeadphoneSettings(headset: headset, snapshot: snapshot)
            }
        }
        .padding(EdgeInsets(top: 2, leading: 20, bottom: 16, trailing: 20))
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A line of explanation and the one thing to do about it.
private struct Prompt: View {
    let text: String
    let button: String
    let action: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            Text(text).foregroundStyle(.secondary).multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
            Button(button, action: action).buttonStyle(.borderedProminent).controlSize(.large)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 6)
    }
}

/// The title of a section that opens and closes. SwiftUI's disclosure group on the Mac answers
/// only its triangle; this makes the title answer too, which is what people click.
private struct DisclosureTitle: View {
    let text: String
    @Binding var open: Bool

    var body: some View {
        Button {
            withAnimation { open.toggle() }
        } label: {
            SectionTitle(text: text).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityHint(open ? "Hides this section" : "Shows this section")
    }
}

private struct SectionTitle: View {
    let text: String

    var body: some View {
        Text(text).font(.subheadline.weight(.semibold)).foregroundStyle(.secondary).accessibilityAddTraits(.isHeader)
    }
}

/// A whole-number slider that shows what the headset reports, except while it is dragged:
/// then it follows the pointer, and sends each value it passes.
private struct LevelSlider: View {
    let label: String
    let value: Int
    let range: ClosedRange<Int>
    /// A tick for every value, and on macOS 26 the fill growing from 0: for a range on both sides
    /// of it. Without, there are no ticks, which on a Mac come with a step.
    var ticked = false
    let onChange: (Int) -> Void
    @State private var dragged: Double?

    var body: some View {
        let position = Binding(
            get: { dragged ?? Double(value) },
            set: { position in
                let whole = Int(position.rounded())
                if whole != Int((dragged ?? Double(value)).rounded()) {
                    onChange(whole)
                }
                dragged = position
            })
        let bounds = Double(range.lowerBound)...Double(range.upperBound)
        let editing = { (editing: Bool) in
            if !editing {
                dragged = nil
            }
        }
        Group {
            if !ticked {
                Slider(value: position, in: bounds, onEditingChanged: editing)
            } else if #available(macOS 26.0, *) {
                Slider(value: position, in: bounds, step: 1, neutralValue: 0, label: { Text(label) }, tick: { SliderTick($0) }, onEditingChanged: editing)
            } else {
                Slider(value: position, in: bounds, step: 1, onEditingChanged: editing)
            }
        }
        .labelsHidden()
        .accessibilityLabel(label)
        .accessibilityValue("\(Int((dragged ?? Double(value)).rounded()))")
    }
}

/// A label on the left, a switch on the right.
private struct SwitchRow: View {
    let label: String
    let hint: String
    let isOn: Bool
    let onChange: (Bool) -> Void

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 1) {
                Text(label)
                if !hint.isEmpty {
                    Text(hint).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer()
            Toggle(label, isOn: Binding(get: { isOn }, set: onChange))
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.small)
                .help(hint)
        }
    }
}

// MARK: Noise control

private struct NoiseControl: View {
    let headset: Headset
    let snapshot: Snapshot
    let noise: NoiseState

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle(text: "Noise Control")
            // One tile a mode, the one in effect prominent.
            HStack(spacing: 8) {
                ForEach(snapshot.noiseModes, id: \.self) { mode in
                    let selected = noise.mode == mode
                    tile(mode, selected: selected)
                        .accessibilityLabel(mode.name)
                        .accessibilityAddTraits(selected ? .isSelected : [])
                        .help(mode.name)
                }
            }

            // Where noise cancelling has strengths to choose from (Nothing's earbuds).
            if noise.mode == .noiseCancelling, let level = snapshot.ncLevel, snapshot.ncLevels.count > 1 {
                Picker("Strength", selection: Binding(get: { level }, set: headset.setNcLevel)) {
                    ForEach(snapshot.ncLevels, id: \.self) { level in
                        Text(ncLevelName(level)).tag(level)
                    }
                }
                .pickerStyle(.segmented)
            }

            // Some models have ambient sound without a level, or without a voice mode.
            if noise.mode == .ambient && (snapshot.maxAmbientLevel > 0 || snapshot.focusOnVoiceAvailable) {
                GroupBox {
                    VStack(spacing: 10) {
                        if snapshot.maxAmbientLevel > 0 {
                            HStack {
                                Text("Ambient level")
                                LevelSlider(label: "Ambient level", value: noise.ambientLevel, range: snapshot.minAmbientLevel...snapshot.maxAmbientLevel) { level in
                                    change { $0.ambientLevel = level }
                                }
                                Text("\(noise.ambientLevel - snapshot.minAmbientLevel + 1)").monospacedDigit().frame(width: 20, alignment: .trailing)
                            }
                        }
                        if snapshot.focusOnVoiceAvailable {
                            SwitchRow(label: "Focus on voice", hint: "Lets voices through while cutting other noise", isOn: noise.focusOnVoice) { on in
                                change { $0.focusOnVoice = on }
                            }
                        }
                    }
                    .padding(4)
                }
            }
            if let optimizer = snapshot.optimizer {
                OptimizerRow(headset: headset, optimizer: optimizer)
            }
        }
    }

    @ViewBuilder
    private func tile(_ mode: NoiseMode, selected: Bool) -> some View {
        let button = Button {
            change { $0.mode = mode }
        } label: {
            VStack(spacing: 5) {
                Image(systemName: mode.symbol).font(.title3).frame(height: 20)
                Text(mode.label).font(.caption)
            }
            .frame(maxWidth: .infinity, minHeight: 46)
        }
        if selected {
            button.buttonStyle(.borderedProminent)
        } else {
            button.buttonStyle(.bordered)
        }
    }

    private func change(_ edit: (inout NoiseState) -> Void) {
        var next = noise
        edit(&next)
        headset.setNoise(next)
    }
}

/// The NC Optimizer: what it last measured, and a run of it. A run plays tones in the wearer's
/// ears, so it takes a second press, on Start, to begin one.
private struct OptimizerRow: View {
    let headset: Headset
    let optimizer: Optimizer
    @State private var asking = false

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    if optimizer.running {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: optimizer.phase == "finished" ? "checkmark.circle.fill" : "wand.and.stars")
                            .foregroundStyle(optimizer.phase == "finished" ? AnyShapeStyle(.green) : AnyShapeStyle(.secondary))
                    }
                    VStack(alignment: .leading, spacing: 1) {
                        Text("NC Optimizer")
                        Text(status).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer()
                    if optimizer.running {
                        Button("Cancel", action: headset.cancelOptimizer)
                    } else if !asking {
                        Button(optimizer.phase == "finished" || optimizer.fitMeasured ? "Run Again…" : "Optimize…") { asking = true }
                            .disabled(!optimizer.available)
                            .help(optimizer.available ? "Tunes noise cancelling to how the headphones sit and to the air pressure" : "The headphones cannot run it right now")
                    }
                }
                if asking && !optimizer.running {
                    Text("Wear the headphones as you normally do. They play test tones\(duration).")
                        .font(.callout)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack {
                        Spacer()
                        Button("Not Now") { asking = false }
                        Button("Start") {
                            asking = false
                            headset.startOptimizer()
                        }
                        .buttonStyle(.borderedProminent)
                    }
                }
            }
            .padding(4)
        }
        .controlSize(.small)
    }

    private var duration: String {
        optimizer.seconds > 0 ? " for about \(optimizer.seconds) seconds" : ""
    }

    private var status: String {
        switch optimizer.phase {
        case "measuring_fit": return "Measuring how they sit…"
        case "measuring_pressure": return "Measuring the air pressure…"
        case "optimizing": return "Optimizing…"
        default:
            let pressure = optimizer.pressure.map { "at \((Double($0) / 10).formatted()) atm" }
            if optimizer.phase == "finished" { return ["Optimized", pressure].compactMap { $0 }.joined(separator: " ") }
            if !optimizer.available { return "Not available right now" }
            return pressure.map { "Last optimized \($0)" } ?? "Tunes noise cancelling to you"
        }
    }
}

// MARK: Equalizer

private struct Equalizer: View {
    let headset: Headset
    let snapshot: Snapshot
    let eq: EqState
    @AppStorage("eq-open") private var open = false

    var body: some View {
        let presets = snapshot.eqPresets.isEmpty ? defaultEqPresets : snapshot.eqPresets
        // Every value runs over the range the headset gives, or Sony's usual one for the layout.
        let range = eq.range > 0 ? eq.range : eq.clearBass == nil ? 6 : 10
        let labels = bandLabels(snapshot, count: eq.bands.count)
        // The core names the presets of headphones other than Sony's.
        let named = Dictionary(zip(snapshot.eqPresets, snapshot.eqPresetNames), uniquingKeysWith: { first, _ in first })
        let name = { (preset: Int) in named[preset] ?? eqPresetLabel[preset] }

        let picker = Picker("Equalizer preset", selection: Binding(get: { eq.preset }, set: headset.setEqPreset)) {
            if !presets.contains(eq.preset) {
                Text(name(eq.preset) ?? "Custom").tag(eq.preset)
            }
            ForEach(presets, id: \.self) { preset in
                Text(name(preset) ?? "Preset \(preset)").tag(preset)
            }
        }
        .labelsHidden()
        .fixedSize()

        if eq.bands.isEmpty {
            // Presets only: nothing to open.
            HStack {
                SectionTitle(text: "Equalizer")
                Spacer()
                picker
            }
        } else {
            DisclosureGroup(isExpanded: $open) {
                GroupBox {
                    VStack(spacing: 12) {
                        bands(range: range, labels: labels)
                        if let clearBass = eq.clearBass {
                            Divider()
                            clearBassRow(clearBass, range: range)
                        }
                    }
                    .padding(6)
                }
                .padding(.top, 6)
            } label: {
                HStack {
                    DisclosureTitle(text: "Equalizer", open: $open)
                    Spacer()
                    picker
                }
            }
        }
    }

    private static let trackHeight: CGFloat = 150

    /// A slider upright for each band, as an equalizer's are, with the scale beside them. The
    /// value is in each slider's tooltip, as the guidelines suggest.
    private func bands(range: Int, labels: [(short: String, full: String)]) -> some View {
        HStack(alignment: .top, spacing: 0) {
            VStack(alignment: .trailing) {
                Text("+\(range)")
                Spacer()
                Text("0")
                Spacer()
                Text("−\(range)")
            }
            .font(.caption2)
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .frame(height: Self.trackHeight)
            .padding(.trailing, 6)
            .accessibilityHidden(true)
            ForEach(eq.bands.indices, id: \.self) { index in
                let label = index < labels.count ? labels[index] : (short: "\(index + 1)", full: "Band \(index + 1)")
                VStack(spacing: 6) {
                    BandSlider(label: label.full, value: eq.bands[index], range: range, ticks: eq.bands.count <= 6) { value in
                        var bands = eq.bands
                        bands[index] = value
                        headset.setEqBands(clearBass: eq.clearBass, bands: bands)
                    }
                    .frame(width: 28, height: Self.trackHeight)
                    Text(label.short).font(.caption2).foregroundStyle(.secondary).lineLimit(1).fixedSize()
                }
                .frame(maxWidth: .infinity)
            }
        }
    }

    /// Clear Bass, across the bottom, with its scale under it.
    private func clearBassRow(_ clearBass: Int, range: Int) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text("Clear Bass").font(.callout)
            VStack(spacing: 2) {
                LevelSlider(label: "Clear Bass", value: clearBass, range: -range...range, ticked: true) {
                    headset.setEqBands(clearBass: $0, bands: eq.bands)
                }
                HStack {
                    Text("−\(range)")
                    Spacer()
                    Text("0")
                    Spacer()
                    Text("+\(range)")
                }
                .font(.caption2)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            }
            .help("Clear Bass: \(clearBass > 0 ? "+" : "")\(clearBass)")
        }
    }
}

/// An upright slider for one band: the Mac's own, from AppKit, since SwiftUI has none upright.
/// Like `LevelSlider`, it follows the pointer while it is dragged and the headset otherwise.
private struct BandSlider: NSViewRepresentable {
    let label: String
    let value: Int
    let range: Int
    /// A tick for each value, beside the track. Ten bands leave no room for them.
    let ticks: Bool
    let onChange: (Int) -> Void

    final class Coordinator: NSObject {
        var onChange: (Int) -> Void = { _ in }
        var dragging = false
        var last = 0

        @objc func moved(_ slider: NSSlider) {
            dragging = NSApp.currentEvent?.type != .leftMouseUp
            let whole = Int(slider.doubleValue.rounded())
            if whole != last {
                last = whole
                onChange(whole)
            }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSSlider {
        let slider = NSSlider(value: Double(value), minValue: Double(-range), maxValue: Double(range), target: context.coordinator, action: #selector(Coordinator.moved))
        slider.isVertical = true
        slider.isContinuous = true
        if ticks {
            slider.numberOfTickMarks = 2 * range + 1
            slider.tickMarkPosition = .trailing
            slider.allowsTickMarkValuesOnly = true
        }
        // The fill grows from 0 both ways, as a boost or a cut.
        if #available(macOS 26.0, *) {
            slider.neutralValue = 0
        }
        slider.setAccessibilityLabel(label)
        return slider
    }

    func updateNSView(_ slider: NSSlider, context: Context) {
        context.coordinator.onChange = onChange
        if !context.coordinator.dragging {
            context.coordinator.last = value
            slider.doubleValue = Double(value)
        }
        slider.toolTip = "\(label): \(value > 0 ? "+" : "")\(value)"
    }
}

// MARK: Paired devices

/// The devices the headphones are paired with. One that is connected can be given the sound by
/// pressing its row.
private struct Devices: View {
    let headset: Headset
    let snapshot: Snapshot
    @AppStorage("devices-open") private var open = false

    var body: some View {
        DisclosureGroup(isExpanded: $open) {
            GroupBox {
                VStack(alignment: .leading, spacing: 8) {
                    // Those in use first, in the headphones' own order otherwise.
                    ForEach(snapshot.devices.filter(\.connected) + snapshot.devices.filter { !$0.connected }, id: \.address) { device in
                        row(device)
                    }
                }
                .padding(4)
            }
            .padding(.top, 6)
        } label: {
            DisclosureTitle(text: "Devices", open: $open)
        }
    }

    private func row(_ device: PairedDevice) -> some View {
        let symbol = ["computer": "laptopcomputer", "phone": "smartphone"][device.kind] ?? "dot.radiowaves.left.and.right"
        let name = device.name.isEmpty ? device.address : device.name
        let selectable = device.connected && !device.source && snapshot.canSelectSource
        return HStack(spacing: 8) {
            Button {
                headset.selectSource(device.address)
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: symbol)
                        .font(.system(size: 13))
                        .frame(width: 26, height: 26)
                        .background(.quaternary, in: Circle())
                    VStack(alignment: .leading, spacing: 1) {
                        Text(name).lineLimit(1).truncationMode(.middle)
                        if device.source {
                            Text("Playing from this device").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .foregroundStyle(device.connected ? .primary : .secondary)
                    Spacer(minLength: 4)
                    if device.source {
                        Image(systemName: "speaker.wave.2.fill").foregroundStyle(.tint).accessibilityHidden(true)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            // Not `disabled`, which would grey out the one that is playing.
            .allowsHitTesting(selectable)
            .help(selectable ? "Play sound from \(name)" : "")
            .accessibilityLabel([name, device.connected ? "connected" : "not connected", device.source ? "playing" : nil].compactMap { $0 }.joined(separator: ", "))

            if device.connected {
                Button("Disconnect") { headset.disconnectDevice(device.address) }
            } else {
                Button("Connect") { headset.connectDevice(device.address) }
            }
        }
        .controlSize(.small)
    }
}

// MARK: Settings kept in the headphones

/// Settings stored in the headphones themselves. Closed until asked for.
private struct HeadphoneSettings: View {
    let headset: Headset
    let snapshot: Snapshot
    @AppStorage("headphone-settings-open") private var open = false

    var body: some View {
        let powerOff = snapshot.autoPowerOff.flatMap { snapshot.autoPowerOffOptions.isEmpty ? nil : $0 }
        if !snapshot.switches.isEmpty || powerOff != nil || !snapshot.assignments.isEmpty {
            DisclosureGroup(isExpanded: $open) {
                GroupBox {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(snapshot.assignments, id: \.key) { assignment in
                            HStack {
                                Text(keyLabel(assignment))
                                Spacer()
                                Picker(keyLabel(assignment), selection: Binding(get: { assignment.function }, set: { headset.setAssignment(key: assignment.key, function: $0) })) {
                                    // A job the headphones have it on that they did not list.
                                    if !assignment.options.contains(assignment.function) {
                                        Text(jobLabel(assignment.function)).tag(assignment.function)
                                    }
                                    ForEach(assignment.options, id: \.self) { job in
                                        Text(jobLabel(job)).tag(job)
                                    }
                                }
                                .labelsHidden()
                                .fixedSize()
                            }
                        }
                        ForEach(snapshot.switches, id: \.id) { state in
                            let info = switchInfo[state.id] ?? (label: state.id, hint: "")
                            SwitchRow(label: info.label, hint: info.hint, isOn: state.on) { headset.setSwitch(state.id, on: $0) }
                        }
                        if let powerOff {
                            HStack {
                                VStack(alignment: .leading, spacing: 1) {
                                    Text("Automatic power-off")
                                    Text("Saves the battery").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Picker("Automatic power-off", selection: Binding(get: { powerOff }, set: headset.setAutoPowerOff)) {
                                    ForEach(snapshot.autoPowerOffOptions, id: \.self) { choice in
                                        Text(autoPowerOffLabel[choice] ?? "Option \(choice)").tag(choice)
                                    }
                                }
                                .labelsHidden()
                                .fixedSize()
                            }
                        }
                    }
                    .padding(4)
                }
                .padding(.top, 6)
            } label: {
                DisclosureTitle(text: "Headphone Settings", open: $open)
            }
        }
    }
}

// MARK: - Foot

/// What stays at the bottom: the one thing done often, and a menu for the rest, the app's own
/// settings among it.
private struct Footer: View {
    let headset: Headset
    let snapshot: Snapshot
    let refused: Bool
    @AppStorage(Preference.connectCard) private var showCard = true
    @AppStorage(Preference.shortcut) private var shortcut = false
    @State private var confirmingPowerOff = false

    var body: some View {
        let connected = snapshot.phase == .connected && !refused
        HStack {
            if connected {
                Button("Disconnect", action: headset.disconnect)
                    .disabled(snapshot.pending != nil)
                    .help("Disconnects the headphones from this Mac")
            }
            Spacer()
            Menu {
                if connected && snapshot.canPowerOff {
                    Button("Power Off Headphones…", systemImage: "power") { confirmingPowerOff = true }
                    Divider()
                }
                Toggle("Open at Login", isOn: Binding(get: { LoginItem.isOn }, set: { LoginItem.isOn = $0 }))
                Toggle("Show Card on Connect", isOn: $showCard)
                Toggle("Switch Noise Mode with ⌥⌘A", isOn: Binding(get: { shortcut }, set: { on in
                    shortcut = on
                    Shortcut.shared.enabled = on
                }))
                Divider()
                Button("Copy Diagnostics Report", systemImage: "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(report(snapshot), forType: .string)
                }
                Button("Open Bluetooth Settings…", systemImage: "gearshape", action: openBluetoothSettings)
                Divider()
                Button("Quit Open Headphones Connect") { NSApp.terminate(nil) }.keyboardShortcut("q")
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("More")
            .accessibilityLabel("More")
        }
        .controlSize(.small)
        // Turning the headphones off cannot be undone from here: they have to be switched on by
        // hand. So it is asked first, as the Mac asks before shutting down.
        .confirmationDialog("Power off the headphones?", isPresented: $confirmingPowerOff) {
            Button("Power Off", role: .destructive, action: headset.powerOff)
        } message: {
            Text("Turn them on again with their power button.")
        }
    }
}
