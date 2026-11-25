// What the session reports, as the panel needs it, and the commands the panel sends back.
// The shapes follow src/types.ts, which describes the same JSON for the Windows interface.

import Foundation
import Observation

enum Phase: String, Decodable {
    case starting, connecting, connected, disconnected
    case bluetoothOff = "bluetooth_off"
    case noDevice = "no_device"
}

enum NoiseMode: String, Decodable {
    case off, wind, ambient
    case noiseCancelling = "noise_cancelling"

    var label: String {
        switch self {
        case .noiseCancelling: "Cancelling"
        case .wind: "Wind"
        case .ambient: "Ambient"
        case .off: "Off"
        }
    }
}

struct Battery: Decodable, Equatable {
    var level: Int
    var charging: Bool
}

struct Batteries: Decodable, Equatable {
    var single: Battery?
    var left: Battery?
    var right: Battery?
    var `case`: Battery?
}

struct NoiseState: Decodable, Equatable {
    var mode: NoiseMode
    var ambientLevel: Int
    var focusOnVoice: Bool
}

struct EqState: Decodable, Equatable {
    var preset: Int
    /// nil on headsets without a Clear Bass control.
    var clearBass: Int?
    var bands: [Int]
    /// Every value runs from minus this to plus this.
    var range: Int
}

struct SwitchState: Decodable, Equatable {
    var id: String
    var on: Bool
}

struct Snapshot: Decodable, Equatable {
    var phase = Phase.starting
    var deviceName: String?
    var battery = Batteries()
    var noise: NoiseState?
    var noiseModes: [NoiseMode] = []
    var minAmbientLevel = 0
    /// 0 where ambient sound has no level to set.
    var maxAmbientLevel = 0
    var eq: EqState?
    /// The presets this model has; empty where it does not list them.
    var eqPresets: [Int] = []
    /// The frequency of each band in hertz; empty where the model does not say.
    var eqFrequencies: [Int] = []
    var codec: String?
    var canPowerOff = false
    var focusOnVoiceAvailable = false
    var switches: [SwitchState] = []
    var autoPowerOff: Int?
    var autoPowerOffOptions: [Int] = []
    /// "connecting_audio" or "disconnecting" while the session is at it.
    var pending: String?
    var notice: String?
    /// Goes up each time the headset connects afresh.
    var connections = 0

    /// The battery to headline: the headset, or the weaker earbud.
    var headlineBattery: Battery? {
        guard phase == .connected else { return nil }
        if let single = battery.single { return single }
        if let left = battery.left, let right = battery.right { return left.level <= right.level ? left : right }
        return battery.left ?? battery.right
    }
}

@Observable
final class Headset {
    private(set) var snapshot = Snapshot()
    /// Called when the headphones have connected afresh, which is when the card appears.
    var onConnect: (() -> Void)?

    /// Starts the session, or with `demo` a made-up headset of that model.
    func start(demo: String?) {
        HeadsetSession.start(demo: demo) { [weak self] json in
            guard let self else { return }
            do {
                let before = snapshot.connections
                snapshot = try JSONDecoder().decode(Snapshot.self, from: Data(json.utf8))
                // The session counts fresh connections, so this is every time the headset is
                // switched on, and not when the link merely recovers from a hiccup.
                if snapshot.phase == .connected && snapshot.connections != before {
                    onConnect?()
                }
            } catch {
                FileHandle.standardError.write(Data("A snapshot could not be read: \(error)\n".utf8))
            }
        }
    }

    /// A command in the JSON form of `Command` in core/src/session.rs, where a test pins it.
    private func send(_ command: Any) {
        let json = try! JSONSerialization.data(withJSONObject: command, options: .fragmentsAllowed)
        if !HeadsetSession.send(String(decoding: json, as: UTF8.self)) {
            FileHandle.standardError.write(Data("The session did not take \(String(decoding: json, as: UTF8.self))\n".utf8))
        }
    }

    func connect() { send("connect") }
    func disconnect() { send("disconnect") }
    func powerOff() { send("power_off") }

    func setNoise(_ noise: NoiseState) {
        send(["set_noise": ["mode": noise.mode.rawValue, "ambientLevel": noise.ambientLevel, "focusOnVoice": noise.focusOnVoice]])
    }

    func setEqPreset(_ preset: Int) { send(["set_eq_preset": preset]) }

    func setEqBands(clearBass: Int?, bands: [Int]) {
        send(["set_eq_bands": ["clearBass": clearBass.map { $0 as Any } ?? NSNull(), "bands": bands]])
    }

    func setSwitch(_ id: String, on: Bool) { send(["set_switch": [id, on]]) }
    func setAutoPowerOff(_ choice: Int) { send(["set_auto_power_off": choice]) }
}

// The names below are Sony's, by the ids the headphones use. src/types.ts has the same tables.

/// What each on/off setting is called, and what it does.
let switchInfo: [String: (label: String, hint: String)] = [
    "upscaling": ("DSEE upscaling", "Restores detail lost in compressed music"),
    "speak_to_chat": ("Speak-to-Chat", "Pauses the music when you start talking"),
    "pause_when_removed": ("Pause when taken off", "Playback stops when you take them off"),
    "touch_panel": ("Touch panel", "Taps and swipes on the right ear cup"),
    "multipoint": ("Connect to two devices", "The headphones reconnect when this changes"),
    "sidetone": ("Hear your voice on calls", "Plays your own voice back while you talk"),
    "voice_guidance": ("Voice guidance", "Spoken prompts for power, battery and pairing"),
]

let autoPowerOffLabel: [Int: String] = [
    0x00: "After 5 minutes", 0x01: "After 30 minutes", 0x02: "After 1 hour", 0x03: "After 3 hours",
    0x04: "After 15 minutes", 0x10: "When taken off", 0x11: "Never",
]

let eqPresetLabel: [Int: String] = [
    0x00: "Off", 0x01: "Rock", 0x02: "Pop", 0x03: "Jazz", 0x04: "Dance", 0x05: "EDM",
    0x06: "R&B / Hip-hop", 0x07: "Acoustic",
    0x10: "Bright", 0x11: "Excited", 0x12: "Mellow", 0x13: "Relaxed", 0x14: "Vocal",
    0x15: "Treble boost", 0x16: "Bass boost", 0x17: "Speech",
    0x20: "Gaming", 0x21: "FPS 1", 0x22: "FPS 2", 0x23: "FPS 3",
    0x30: "Heavy", 0x31: "Clear", 0x32: "Hard", 0x33: "Soft",
    0xA0: "Manual", 0xA1: "Custom 1", 0xA2: "Custom 2", 0xA3: "Custom 3", 0xA4: "Custom 4", 0xA5: "Custom 5",
]

/// The presets offered when a model does not list its own.
let defaultEqPresets = [0x00, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0xA0, 0xA1, 0xA2]

/// What to print beside each band's slider: the model's own frequencies where it gives them,
/// else Sony's usual two layouts.
func bandLabels(_ snapshot: Snapshot, count: Int) -> [String] {
    let hertz = snapshot.eqFrequencies.count == count ? snapshot.eqFrequencies
        : count == 10 ? [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
        : [400, 1000, 2500, 6300, 16000]
    return hertz.map { $0 >= 1000 ? "\((Double($0) / 1000).formatted()) kHz" : "\($0) Hz" }
}
