// The panel that opens from the menu bar: the Mac counterpart of src/Panel.tsx.

import AppKit
import SwiftUI

struct Panel: View {
    let headset: Headset

    var body: some View {
        let snapshot = headset.snapshot
        VStack(alignment: .leading, spacing: 14) {
            Header(snapshot: snapshot)
            if let notice = snapshot.notice {
                Label(notice, systemImage: "info.circle")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            switch snapshot.phase {
            case .starting, .connecting:
                EmptyView()
            case .bluetoothOff:
                Prompt(text: "Turn Bluetooth on to use your headphones with this Mac.", button: "Open Bluetooth settings", action: openBluetoothSettings)
            case .noDevice:
                Prompt(
                    text: "Pair your Sony headphones with this Mac first. Hold their power button for 7 seconds until the light flashes blue, then add them in Bluetooth settings.",
                    button: "Open Bluetooth settings", action: openBluetoothSettings)
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
                HeadphoneSettings(headset: headset, snapshot: snapshot)
            }
            Divider()
            HStack {
                if snapshot.phase == .connected {
                    Button("Disconnect", action: headset.disconnect).disabled(snapshot.pending != nil)
                    if snapshot.canPowerOff {
                        Button("Power off", systemImage: "power", action: headset.powerOff)
                    }
                }
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
            }
        }
        .padding(16)
        .frame(width: 320)
    }

    private func openBluetoothSettings() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.BluetoothSettings")!)
    }
}

private struct Header: View {
    let snapshot: Snapshot

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "headphones")
                .font(.system(size: 30))
                .foregroundStyle(snapshot.phase == .connected ? .primary : .tertiary)
                .frame(width: 40, height: 40)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(snapshot.deviceName ?? "Sony headphones").font(.headline)
                Text(status).foregroundStyle(.secondary)
                if snapshot.phase == .connected {
                    HStack(spacing: 8) {
                        battery(snapshot.battery.single, nil)
                        battery(snapshot.battery.left, "L")
                        battery(snapshot.battery.right, "R")
                        battery(snapshot.battery.case, "Case")
                    }
                    // Three of them only fit beside the picture in small print.
                    .font(snapshot.battery.single == nil ? .caption : .callout)
                    .padding(.top, 2)
                }
            }
            Spacer()
            if snapshot.phase == .connecting || snapshot.phase == .starting || snapshot.pending != nil {
                ProgressView().controlSize(.small).accessibilityLabel("Working")
            }
        }
    }

    private var status: String {
        if snapshot.pending == "connecting_audio" { return "Connecting…" }
        if snapshot.pending == "disconnecting" { return "Disconnecting…" }
        switch snapshot.phase {
        case .starting: return "Looking for your headphones…"
        case .bluetoothOff: return "Bluetooth is off"
        case .noDevice: return "Not paired yet"
        case .disconnected: return "Not connected"
        case .connecting: return "Connecting…"
        case .connected: return snapshot.codec.map { "Connected, \($0)" } ?? "Connected"
        }
    }

    @ViewBuilder
    private func battery(_ battery: Battery?, _ slot: String?) -> some View {
        if let battery {
            let text = [slot, "\(battery.level)%"].compactMap { $0 }.joined(separator: " ")
            let spoken = [["L": "Left", "R": "Right"][slot ?? ""] ?? slot, "\(battery.level)%"].compactMap { $0 }.joined(separator: " ")
            Label(text, systemImage: batterySymbol(battery))
                .labelStyle(.titleAndIcon)
                .fixedSize()
                .foregroundStyle(battery.level <= 20 && !battery.charging ? .red : .primary)
                .accessibilityLabel(spoken + (battery.charging ? ", charging" : ""))
        }
    }
}

func batterySymbol(_ battery: Battery) -> String {
    if battery.charging { return "battery.100percent.bolt" }
    // The nearest of the five drawings there are.
    return "battery.\(min(4, (battery.level + 12) / 25) * 25)percent"
}

/// A line of explanation and the one thing to do about it.
private struct Prompt: View {
    let text: String
    let button: String
    let action: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(text).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            Button(button, action: action).buttonStyle(.borderedProminent)
        }
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
    let onChange: (Int) -> Void
    @State private var dragged: Double?

    var body: some View {
        // No `step`: on a Mac that draws a tick for every value.
        Slider(
            value: Binding(
                get: { dragged ?? Double(value) },
                set: { position in
                    let whole = Int(position.rounded())
                    if whole != Int((dragged ?? Double(value)).rounded()) {
                        onChange(whole)
                    }
                    dragged = position
                }),
            in: Double(range.lowerBound)...Double(range.upperBound),
            onEditingChanged: { editing in
                if !editing {
                    dragged = nil
                }
            }
        )
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
                Text(hint).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
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

private struct NoiseControl: View {
    let headset: Headset
    let snapshot: Snapshot
    let noise: NoiseState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionTitle(text: "Noise control")
            Picker("Noise control", selection: Binding(get: { noise.mode }, set: { mode in change { $0.mode = mode } })) {
                ForEach(snapshot.noiseModes, id: \.self) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            // Some models have ambient sound without a level, or without a voice mode.
            if noise.mode == .ambient {
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
        }
    }

    private func change(_ edit: (inout NoiseState) -> Void) {
        var next = noise
        edit(&next)
        headset.setNoise(next)
    }
}

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

        VStack(alignment: .leading, spacing: 8) {
            HStack {
                SectionTitle(text: "Equalizer")
                Spacer()
                Picker("Equalizer preset", selection: Binding(get: { eq.preset }, set: headset.setEqPreset)) {
                    if !presets.contains(eq.preset) {
                        Text(eqPresetLabel[eq.preset] ?? "Custom").tag(eq.preset)
                    }
                    ForEach(presets, id: \.self) { preset in
                        Text(eqPresetLabel[preset] ?? "Preset \(preset)").tag(preset)
                    }
                }
                .labelsHidden()
                .fixedSize()
                if !eq.bands.isEmpty {
                    Button(open ? "Hide sliders" : "Adjust sliders", systemImage: open ? "chevron.up" : "chevron.down") { open.toggle() }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                }
            }
            if open && !eq.bands.isEmpty {
                Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 4) {
                    if let clearBass = eq.clearBass {
                        band("Clear Bass", clearBass, range) { headset.setEqBands(clearBass: $0, bands: eq.bands) }
                    }
                    ForEach(eq.bands.indices, id: \.self) { index in
                        band(index < labels.count ? labels[index] : "Band \(index + 1)", eq.bands[index], range) { value in
                            var bands = eq.bands
                            bands[index] = value
                            headset.setEqBands(clearBass: eq.clearBass, bands: bands)
                        }
                    }
                }
            }
        }
    }

    private func band(_ label: String, _ value: Int, _ range: Int, onChange: @escaping (Int) -> Void) -> some View {
        GridRow {
            Text(label).font(.callout)
            LevelSlider(label: label, value: value, range: -range...range, onChange: onChange).controlSize(.small)
            Text(value > 0 ? "+\(value)" : "\(value)").font(.callout).monospacedDigit().gridColumnAlignment(.trailing)
        }
    }
}

/// Settings stored in the headphones themselves. Closed until asked for.
private struct HeadphoneSettings: View {
    let headset: Headset
    let snapshot: Snapshot
    @AppStorage("headphone-settings-open") private var open = false

    var body: some View {
        let powerOff = snapshot.autoPowerOff.flatMap { snapshot.autoPowerOffOptions.isEmpty ? nil : $0 }
        if !snapshot.switches.isEmpty || powerOff != nil {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    SectionTitle(text: "Headphone settings")
                    Spacer()
                    Button(open ? "Hide headphone settings" : "Show headphone settings", systemImage: open ? "chevron.up" : "chevron.down") { open.toggle() }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                }
                if open {
                    ForEach(snapshot.switches, id: \.id) { state in
                        let info = switchInfo[state.id] ?? (label: state.id, hint: "")
                        SwitchRow(label: info.label, hint: info.hint, isOn: state.on) { headset.setSwitch(state.id, on: $0) }
                    }
                    if let powerOff {
                        VStack(alignment: .leading, spacing: 1) {
                            HStack {
                                Text("Automatic power-off")
                                Spacer()
                                Picker("Automatic power-off", selection: Binding(get: { powerOff }, set: headset.setAutoPowerOff)) {
                                    ForEach(snapshot.autoPowerOffOptions, id: \.self) { choice in
                                        Text(autoPowerOffLabel[choice] ?? "Option \(choice)").tag(choice)
                                    }
                                }
                                .labelsHidden()
                                .fixedSize()
                            }
                            Text("Switches off by itself to save the battery").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }
}
