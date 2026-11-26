// The Swift half of the bridge: gives the shared session (core/src/session.rs) this Mac's
// Bluetooth, and hands the app what the session reports. `ohc_bridge.h` is the other half.
//
// The session calls in from a thread of its own and waits for each answer, while IOBluetooth is
// only used from the main thread, which also delivers what it has to say. So every function
// here hops to the main thread, and the ones that wait for IOBluetooth to report back wait on
// the session's thread, never on the main one.

import CoreBluetooth
import Foundation
import IOBluetooth

enum HeadsetSession {
    private static var onSnapshot: ((String) -> Void)?

    /// Starts the session, once. Call on the main thread, which must be running its run loop.
    /// `onSnapshot` gets every snapshot as JSON, in the shape of src/types.ts, on the main thread.
    /// With `demo`, a made-up headset of that model stands in and Bluetooth is left alone.
    static func start(demo: String? = nil, onSnapshot: @escaping (String) -> Void) {
        self.onSnapshot = onSnapshot
        if let demo {
            return ohc_start_demo(demo, report)
        }
        Radio.shared.start()
        var bluetooth = OhcBluetooth(
            radio_is_on: {
                Radio.shared.isOn ?? onMain { IOBluetoothHostController.default()?.powerState == kBluetoothHCIPowerStateON }
            },
            find_headset: {
                onMain {
                    let paired = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice] ?? []
                    let headsets = paired.filter { ohc_is_headset_name($0.nameOrAddress ?? "") }
                    // With none connected, the one used last: that is the one Connect is meant for.
                    let lastUsed = headsets.max { ($0.recentAccessDate() ?? .distantPast) < ($1.recentAccessDate() ?? .distantPast) }
                    return (headsets.first { $0.isConnected() } ?? lastUsed).map { Unmanaged.passRetained($0).toOpaque() }
                }
            },
            headset_name: { headset in onMain { strdup(device(headset).nameOrAddress ?? "") } },
            headset_is_connected: { headset in onMain { device(headset).isConnected() } },
            headset_open: { headset, inbox, link, generation in
                switch Link.open(device(headset), inbox: inbox!) {
                case .success(let (opened, speaks)):
                    link!.pointee = Unmanaged.passRetained(opened).toOpaque()
                    generation!.pointee = speaks
                    return nil
                case .failure(let error):
                    return strdup(error.message)
                }
            },
            headset_connect_audio: { headset in
                // With a target the request returns at once; nobody waits for `connectionComplete`.
                failure("Connecting", onMain { device(headset).openConnection(Quiet.shared) })
            },
            headset_disconnect_audio: { headset in failure("Disconnecting", onMain { device(headset).closeConnection() }) },
            headset_release: { headset in onMain { Unmanaged<IOBluetoothDevice>.fromOpaque(headset!).release() } },
            link_send: { link, bytes, length in
                failure("Sending to the headset", onMain {
                    Unmanaged<Link>.fromOpaque(link!).takeUnretainedValue().channel?
                        .writeSync(UnsafeMutableRawPointer(mutating: bytes), length: UInt16(length)) ?? kIOReturnNotOpen
                })
            },
            link_close: { link in onMain { Unmanaged<Link>.fromOpaque(link!).takeRetainedValue().close() } },
            on_snapshot: report
        )
        ohc_start(&bluetooth)
    }

    /// Called on the session's thread.
    private static let report: @convention(c) (UnsafePointer<CChar>?) -> Void = { json in
        let snapshot = String(cString: json!)
        DispatchQueue.main.async { onSnapshot?(snapshot) }
    }

    /// Passes a command on to the session, as JSON: `"connect"`, `{"set_volume": 12}` and so on
    /// (`Command` in core/src/session.rs). False if it is not one.
    @discardableResult
    static func send(_ command: String) -> Bool {
        ohc_send(command)
    }
}

/// Whether the Bluetooth radio is on, as CoreBluetooth says. IOBluetooth's own answer can stick
/// at "off" in an app that started while Bluetooth was still coming on, and never change.
private final class Radio: NSObject, CBCentralManagerDelegate {
    static let shared = Radio()
    private var manager: CBCentralManager?
    private let lock = NSLock()
    private var state = CBManagerState.unknown

    func start() {
        // On a queue of its own: on the main one it can deadlock with IOBluetooth's first call.
        manager = CBCentralManager(delegate: self, queue: DispatchQueue(label: "radio"),
                                   options: [CBCentralManagerOptionShowPowerAlertKey: false])
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        lock.withLock { state = central.state }
    }

    /// `nil` while CoreBluetooth has not said, or cannot (without permission, or resetting).
    var isOn: Bool? {
        lock.withLock {
            switch state {
            case .poweredOn: true
            case .poweredOff: false
            default: nil
            }
        }
    }
}

private func onMain<T>(_ body: () -> T) -> T {
    Thread.isMainThread ? body() : DispatchQueue.main.sync(execute: body)
}

private func device(_ headset: UnsafeMutableRawPointer?) -> IOBluetoothDevice {
    Unmanaged<IOBluetoothDevice>.fromOpaque(headset!).takeUnretainedValue()
}

private func describe(_ action: String, _ status: IOReturn) -> String {
    "\(action) failed (\(String(format: "0x%08x", UInt32(bitPattern: status))))"
}

/// An error message for the session, which frees it, or nil when `status` is success.
private func failure(_ action: String, _ status: IOReturn) -> UnsafeMutablePointer<CChar>? {
    status == kIOReturnSuccess ? nil : strdup(describe(action, status))
}

private struct Failure: Error {
    let message: String
}

/// A target for requests whose outcome nobody waits for.
private final class Quiet: NSObject {
    static let shared = Quiet()
    @objc func connectionComplete(_ device: IOBluetoothDevice!, status: IOReturn) {}
}

/// Waits on the session's thread for something IOBluetooth reports on the main one.
private final class Report: NSObject {
    private let arrived = DispatchSemaphore(value: 0)
    private var status: IOReturn = kIOReturnTimeout

    func arrive(_ status: IOReturn) {
        self.status = status
        arrived.signal()
    }

    func wait(seconds: Int) -> IOReturn {
        _ = arrived.wait(timeout: .now() + .seconds(seconds))
        return status
    }

    @objc func sdpQueryComplete(_ device: IOBluetoothDevice!, status: IOReturn) {
        arrive(status)
    }
}

/// An open control channel. Everything but `open` runs on the main thread.
private final class Link: NSObject, IOBluetoothRFCOMMChannelDelegate {
    private static let services: [(generation: UInt8, uuid: String)] = [
        (1, "96CC203E-5068-46AD-B32D-E316F5E069BA"),  // WH-1000XM3 generation
        (2, "956C7B26-D49A-4BA8-B03F-B17D393CB6E2"),  // XM5 generation
        (3, "AEAC4A03-DFF5-498F-843A-34487CF133EB"),  // Nothing and CMF
    ]

    private let inbox: UnsafeRawPointer
    private let opened = Report()
    private var closed = false
    fileprivate var channel: IOBluetoothRFCOMMChannel?

    private init(inbox: UnsafeRawPointer) {
        self.inbox = inbox
    }

    /// Which control service macOS has a record of for this headset, and its channel.
    private static func service(of device: IOBluetoothDevice) -> (generation: UInt8, channel: BluetoothRFCOMMChannelID)? {
        for service in services {
            var bytes = [UInt8](repeating: 0, count: 16)
            NSUUID(uuidString: service.uuid)!.getBytes(&bytes)
            var channel: BluetoothRFCOMMChannelID = 0
            if let record = device.getServiceRecord(for: IOBluetoothSDPUUID(bytes: bytes, length: 16)),
               record.getRFCOMMChannelID(&channel) == kIOReturnSuccess {
                return (service.generation, channel)
            }
        }
        return nil
    }

    /// Called on the session's thread.
    static func open(_ device: IOBluetoothDevice, inbox: UnsafeRawPointer) -> Result<(Link, UInt8), Failure> {
        var found = onMain { service(of: device) }
        if found == nil {
            // macOS remembers a headset's services from earlier; this one it has to ask.
            let asked = Report()
            if onMain({ device.performSDPQuery(asked) }) == kIOReturnSuccess, asked.wait(seconds: 10) == kIOReturnSuccess {
                found = onMain { service(of: device) }
            }
        }
        guard let found else {
            return .failure(Failure(message: "The headphones do not offer a control service this app knows"))
        }

        let link = Link(inbox: inbox)
        let started = onMain {
            device.openRFCOMMChannelAsync(&link.channel, withChannelID: found.channel, delegate: link)
        }
        let status = started == kIOReturnSuccess ? link.opened.wait(seconds: 15) : started
        guard status == kIOReturnSuccess else {
            onMain { link.close() }
            return .failure(Failure(message: describe("Opening the control channel", status)))
        }
        return .success((link, found.generation))
    }

    /// After this nothing more goes to the inbox, which the session is about to free.
    func close() {
        closed = true
        channel?.setDelegate(nil)
        channel?.close()
        channel = nil
    }

    func rfcommChannelOpenComplete(_ rfcommChannel: IOBluetoothRFCOMMChannel!, status error: IOReturn) {
        opened.arrive(error)
    }

    func rfcommChannelData(_ rfcommChannel: IOBluetoothRFCOMMChannel!, data dataPointer: UnsafeMutableRawPointer!, length dataLength: Int) {
        if !closed {
            ohc_link_received(inbox, dataPointer, dataLength)
        }
    }

    func rfcommChannelClosed(_ rfcommChannel: IOBluetoothRFCOMMChannel!) {
        if !closed {
            closed = true
            ohc_link_closed(inbox, nil)
        }
    }
}
