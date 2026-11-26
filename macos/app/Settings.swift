// The app's own settings, which live in the panel's ⋯ menu: opening at login, the connect card,
// a global shortcut, and a report of what the app knows, for problem reports.

import AppKit
import Carbon.HIToolbox
import ServiceManagement

enum Preference {
    static let connectCard = "show-connect-card"
    static let shortcut = "global-shortcut"
    /// The model of the headphones last connected, whose 3D file is read as the app starts.
    static let lastModel = "last-model"
}

/// Opening at login, through the system's Login Items.
enum LoginItem {
    static var isOn: Bool {
        get { [.enabled, .requiresApproval].contains(SMAppService.mainApp.status) }
        set {
            try? newValue ? SMAppService.mainApp.register() : SMAppService.mainApp.unregister()
            // Where the system wants the user's own say, or would not do it, the user can do it
            // there by hand.
            if newValue && SMAppService.mainApp.status != .enabled {
                SMAppService.openSystemSettingsLoginItems()
            }
        }
    }
}

/// ⌥⌘A from anywhere. A hot key registered this way needs no permission from the user, as a
/// watch on every key press would.
final class Shortcut {
    static let shared = Shortcut()
    var action: () -> Void = {}
    private var hotKey: EventHotKeyRef?
    private var handler: EventHandlerRef?

    var enabled: Bool {
        get { hotKey != nil }
        set {
            if newValue, hotKey == nil {
                if handler == nil {
                    var pressed = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
                    InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in
                        DispatchQueue.main.async { Shortcut.shared.action() }
                        return noErr
                    }, 1, &pressed, nil, &handler)
                }
                // "OHCA": this app's one hot key.
                let id = EventHotKeyID(signature: OSType(0x4F48_4341), id: 1)
                RegisterEventHotKey(UInt32(kVK_ANSI_A), UInt32(cmdKey | optionKey), id, GetApplicationEventTarget(), 0, &hotKey)
            } else if !newValue, let hotKey {
                UnregisterEventHotKey(hotKey)
                self.hotKey = nil
            }
        }
    }
}

/// What the app knows about the headphones, for a problem report. It leaves out anything
/// personal: the name the owner gave the headphones, and paired devices' names and addresses.
func report(_ snapshot: Snapshot) -> String {
    let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    let system = ProcessInfo.processInfo.operatingSystemVersion
    let batteries = [("", snapshot.battery.single), ("left ", snapshot.battery.left), ("right ", snapshot.battery.right), ("case ", snapshot.battery.case)]
        .compactMap { slot, battery in battery.map { "\(slot)\($0.level)%\($0.charging ? " charging" : "")" } }
    var lines = [
        "Open Headphones Connect \(version), macOS \(system.majorVersion).\(system.minorVersion).\(system.patchVersion)",
        "Model: \(snapshot.model ?? "unknown"), firmware \(snapshot.firmware ?? "unknown")",
        "State: \(snapshot.phase.rawValue)\(snapshot.codec.map { ", codec \($0)" } ?? "")",
        "Battery: \(batteries.isEmpty ? "unknown" : batteries.joined(separator: ", "))",
        "Noise control: \(snapshot.noise.map { "\($0.mode.rawValue), level \($0.ambientLevel), voice \($0.focusOnVoice)" } ?? "none"); modes \(snapshot.noiseModes.map(\.rawValue).joined(separator: ", "))",
        "Equalizer: \(snapshot.eq.map { "preset \($0.preset), \($0.bands.count) bands\($0.clearBass == nil ? "" : " and Clear Bass")" } ?? "none")",
        "Switches: \(snapshot.switches.map { "\($0.id) \($0.on ? "on" : "off")" }.joined(separator: ", "))",
        "Buttons: \(snapshot.assignments.map { "\($0.key) → \($0.function)" }.joined(separator: ", "))",
        "Optimizer: \(snapshot.optimizer.map { "\($0.phase), \($0.available ? "available" : "unavailable")" } ?? "none")",
        "Paired devices: \(snapshot.devices.count), \(snapshot.devices.filter(\.connected).count) connected",
    ]
    if let notice = snapshot.notice { lines.append("Last issue: \(notice)") }
    return lines.joined(separator: "\n")
}
