// Prints what macOS knows about each paired Bluetooth device and whether the Sony control
// service can be reached, then asks the headset one question and prints the raw reply.
// Read-only: nothing here changes a setting on the headset.
//
//     macos/run.sh probe
//     macos/run.sh probe scan 20     # instead: what nearby headphones advertise over Bluetooth LE
//                                    # for 20 seconds, for the pairing card (below, `Scanner`)
//     macos/run.sh probe inquire 8   # instead: the classic Bluetooth devices that answer a search
//                                    # (those in pairing mode), which is how the card finds the
//                                    # address to pair with
//
// The Mac counterpart of `--probe` in src-tauri/src/bluetooth/probe.rs.

import CoreBluetooth
import Foundation
import IOBluetooth

let controlServices = [
    (generation: "V1", uuid: "96CC203E-5068-46AD-B32D-E316F5E069BA"),  // WH-1000XM3 generation
    (generation: "V2", uuid: "956C7B26-D49A-4BA8-B03F-B17D393CB6E2"),  // XM5 generation
]

// ponytail: the next two sections are copied from core/src/models.rs and
// core/src/protocol/framing.rs, just enough for one question. The app gets both from the Rust
// crate through the bridge; nothing else should grow on these copies.

func hasSonyName(_ name: String) -> Bool {
    let upper = name.uppercased()
    return ["WH-", "WF-", "MDR-", "LINKBUDS", "ULT WEAR", "INZONE"]
        .contains { upper.hasPrefix($0) || upper.hasPrefix("LE_" + $0) }
}

let typeAck: UInt8 = 0x01
let typeData: UInt8 = 0x0C

struct Frame {
    let kind: UInt8
    let seq: UInt8
    let payload: [UInt8]
}

/// `>` ESCAPE(type, seq, len:u32be, payload…, checksum) `<`
func pack(_ kind: UInt8, _ seq: UInt8, _ payload: [UInt8]) -> [UInt8] {
    var body = [kind, seq] + withUnsafeBytes(of: UInt32(payload.count).bigEndian) { Array($0) } + payload
    body.append(body.reduce(0, &+))
    return [0x3E] + body.flatMap { (0x3C...0x3E).contains($0) ? [0x3D, $0 - 0x10] : [$0] } + [0x3C]
}

/// Takes the complete frames off the front of `buffer`, leaving a partial one for next time.
func takeFrames(from buffer: inout [UInt8]) -> [Frame] {
    var found: [Frame] = []
    while let start = buffer.firstIndex(of: 0x3E), let end = buffer[start...].firstIndex(of: 0x3C) {
        var body: [UInt8] = []
        var escaped = false
        for byte in buffer[(start + 1)..<end] {
            if escaped {
                body.append(byte &+ 0x10)
                escaped = false
            } else if byte == 0x3D {
                escaped = true
            } else {
                body.append(byte)
            }
        }
        buffer.removeSubrange(...end)
        guard body.count >= 7,
              body.dropLast().reduce(0, &+) == body[body.count - 1],
              body[2...5].reduce(0, { $0 << 8 | Int($1) }) == body.count - 7
        else {
            print("  (dropped a malformed frame)")
            continue
        }
        found.append(Frame(kind: body[0], seq: body[1], payload: Array(body[6..<(body.count - 1)])))
    }
    return found
}

// The awkward cases from the Rust tests: marker bytes in the payload, a checksum that is a
// marker, and a frame arriving in two pieces.
do {
    let frames = pack(typeData, 1, [0x3C, 0x3D, 0x3E, 0x00, 0xFF]) + pack(typeData, 0, [0x2F])
    precondition(frames.suffix(3) == [0x3D, 0x2C, 0x3C])
    var stream = Array(frames.dropLast(4))
    precondition(takeFrames(from: &stream).map(\.payload) == [[0x3C, 0x3D, 0x3E, 0x00, 0xFF]])
    stream += frames.suffix(4)
    precondition(takeFrames(from: &stream).map(\.payload) == [[0x2F]])
}

func hex(_ bytes: [UInt8]) -> String {
    bytes.map { String(format: "%02x", $0) }.joined(separator: " ")
}

func code(_ status: IOReturn) -> String {
    status == kIOReturnSuccess ? "ok" : String(format: "error 0x%08x", UInt32(bitPattern: status))
}

func sdpUUID(_ string: String) -> IOBluetoothSDPUUID {
    var bytes = [UInt8](repeating: 0, count: 16)
    NSUUID(uuidString: string)!.getBytes(&bytes)
    return IOBluetoothSDPUUID(bytes: bytes, length: 16)
}

/// IOBluetooth answers on the main run loop, so waiting for it means running that loop.
@discardableResult
func wait(upTo seconds: TimeInterval, until done: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(seconds)
    while !done() && Date() < deadline {
        let tick = Date().addingTimeInterval(0.02)
        if !RunLoop.current.run(mode: .default, before: tick) {
            Thread.sleep(until: tick)
        }
    }
    return done()
}

final class Listener: NSObject, IOBluetoothRFCOMMChannelDelegate {
    var sdpStatus: IOReturn?
    var openStatus: IOReturn?
    var closed = false
    var received: [UInt8] = []

    @objc func sdpQueryComplete(_ device: IOBluetoothDevice!, status: IOReturn) {
        sdpStatus = status
    }

    func rfcommChannelOpenComplete(_ rfcommChannel: IOBluetoothRFCOMMChannel!, status error: IOReturn) {
        openStatus = error
    }

    func rfcommChannelData(_ rfcommChannel: IOBluetoothRFCOMMChannel!, data dataPointer: UnsafeMutableRawPointer!, length dataLength: Int) {
        received.append(contentsOf: UnsafeRawBufferPointer(start: dataPointer, count: dataLength))
    }

    func rfcommChannelClosed(_ rfcommChannel: IOBluetoothRFCOMMChannel!) {
        closed = true
    }
}

let listener = Listener()
// So that what was found so far survives a crash or a hang when the output goes to a file.
setvbuf(stdout, nil, _IOLBF, 0)

/// The serial channel a service record names, or nil when it names none.
func channel(of record: IOBluetoothSDPServiceRecord) -> BluetoothRFCOMMChannelID? {
    var id: BluetoothRFCOMMChannelID = 0
    return record.getRFCOMMChannelID(&id) == kIOReturnSuccess ? id : nil
}

var target: (device: IOBluetoothDevice, generation: String, channel: BluetoothRFCOMMChannelID)?

/// Prints whether macOS has a record of each control service, and remembers the first one found.
func lookUp(_ device: IOBluetoothDevice, _ when: String) {
    for service in controlServices {
        let found = device.getServiceRecord(for: sdpUUID(service.uuid)).flatMap(channel(of:))
        print("    \(service.generation) control service, \(when): \(found.map { "channel \($0)" } ?? "not found")")
        if let found, target == nil, device.isConnected() {
            target = (device, service.generation, found)
        }
    }
}

/// Asks the headset itself for its services, replacing what macOS remembers.
func askForServices(_ device: IOBluetoothDevice) -> String {
    listener.sdpStatus = nil
    let started = device.performSDPQuery(listener)
    guard started == kIOReturnSuccess else { return "could not start, \(code(started))" }
    guard wait(upTo: 15, until: { listener.sdpStatus != nil }) else { return "no answer in 15 s" }
    return code(listener.sdpStatus!)
}

print("Bluetooth radio on: \(IOBluetoothHostController.default()?.powerState == kBluetoothHCIPowerStateON)")
/// Prints the advertisements that say "pair me", and those of anything named like headphones:
/// Google Fast Pair (service data FE2C: three bytes are a model id, sent only in pairing mode;
/// anything longer is an account filter, sent otherwise) and Microsoft Swift Pair (manufacturer
/// data 0006 03 …, with the classic address when the scenario byte is 01).
final class Scanner: NSObject, CBCentralManagerDelegate {
    var manager: CBCentralManager?
    private var printed: [UUID: String] = [:]
    private let start = Date()
    /// Everything heard, to tell "nothing says pair me" from "nothing was heard at all".
    var heard = Set<UUID>()

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        print("Bluetooth LE state: \(central.state.rawValue) (5 is on, 3 not allowed)")
        if central.state == .poweredOn {
            central.scanForPeripherals(withServices: nil, options: [CBCentralManagerScanOptionAllowDuplicatesKey: true])
        }
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber) {
        heard.insert(peripheral.identifier)
        let name = advertisementData[CBAdvertisementDataLocalNameKey] as? String ?? peripheral.name
        let serviceData = advertisementData[CBAdvertisementDataServiceDataKey] as? [CBUUID: Data] ?? [:]
        let services = advertisementData[CBAdvertisementDataServiceUUIDsKey] as? [CBUUID] ?? []
        let maker = (advertisementData[CBAdvertisementDataManufacturerDataKey] as? Data).map { [UInt8]($0) }
        let fastPair = serviceData[CBUUID(string: "FE2C")].map { [UInt8]($0) }
        let swiftPair = maker.flatMap { $0.count >= 4 && $0[0] == 0x06 && $0[1] == 0x00 && $0[2] == 0x03 ? $0 : nil }
        let upper = (name ?? "").uppercased()
        let named = hasSonyName(name ?? "") || ["CMF", "NOTHING", "EAR (", "HEADPHONE", "BUDS", "SOUNDCORE"].contains { upper.contains($0) }
        guard fastPair != nil || swiftPair != nil || named else { return }
        var parts = ["name=\(name ?? "-")"]
        if let fastPair {
            parts.append("fast-pair=[\(hex(fastPair))]" + (fastPair.count == 3 ? " (model id: pairing mode)" : " (account filter: not pairing)"))
        }
        if let swiftPair {
            var said = "swift-pair=[\(hex(swiftPair))] scenario \(swiftPair[3])"
            if swiftPair[3] == 0x01, swiftPair.count >= 14 {
                said += " address " + swiftPair[5..<11].reversed().map { String(format: "%02X", $0) }.joined(separator: ":")
                said += " name \"" + (String(bytes: swiftPair[14...], encoding: .utf8) ?? "?") + "\""
            }
            parts.append(said)
        } else if let maker {
            parts.append("maker=[\(hex(Array(maker.prefix(12))))]")
        }
        if !services.isEmpty { parts.append("services=\(services.map(\.uuidString).joined(separator: ","))") }
        let others = serviceData.keys.filter { $0 != CBUUID(string: "FE2C") }.map(\.uuidString)
        if !others.isEmpty { parts.append("service-data=\(others.joined(separator: ","))") }
        let line = parts.joined(separator: "  ")
        // Again only when what it says changes: entering pairing mode changes it.
        if printed[peripheral.identifier] != line {
            printed[peripheral.identifier] = line
            print(String(format: "%5.1f s  rssi %4d  ", Date().timeIntervalSince(start), rssi.intValue) + line)
        }
    }
}

/// Prints every classic device that answers an inquiry, as it is found and again once named.
final class Inquirer: NSObject, IOBluetoothDeviceInquiryDelegate {
    var done = false

    func say(_ what: String, _ device: IOBluetoothDevice) {
        print("\(what): \(device.nameOrAddress ?? "?")  [\(device.addressString ?? "?")]  class \(String(format: "%06x", device.classOfDevice))  major \(device.deviceClassMajor)  paired=\(device.isPaired())")
    }

    func deviceInquiryStarted(_ sender: IOBluetoothDeviceInquiry!) { print("Inquiry started") }
    func deviceInquiryDeviceFound(_ sender: IOBluetoothDeviceInquiry!, device: IOBluetoothDevice!) { say("found", device) }
    func deviceInquiryDeviceNameUpdated(_ sender: IOBluetoothDeviceInquiry!, device: IOBluetoothDevice!, devicesRemaining: UInt32) { say("named", device) }
    func deviceInquiryComplete(_ sender: IOBluetoothDeviceInquiry!, error: IOReturn, aborted: Bool) {
        print("Inquiry complete: \(code(error)), aborted \(aborted)")
        done = true
    }
}

if let at = CommandLine.arguments.firstIndex(of: "inquire") {
    let seconds = CommandLine.arguments.dropFirst(at + 1).first.flatMap(UInt8.init) ?? 8
    let inquirer = Inquirer()
    let inquiry = IOBluetoothDeviceInquiry(delegate: inquirer)!
    inquiry.inquiryLength = seconds
    inquiry.updateNewDeviceNames = true
    print("Starting: \(code(inquiry.start()))")
    _ = wait(upTo: TimeInterval(seconds) + 20) { inquirer.done }
    exit(0)
}

if let at = CommandLine.arguments.firstIndex(of: "scan") {
    let seconds = CommandLine.arguments.dropFirst(at + 1).first.flatMap(Double.init) ?? 15
    let scanner = Scanner()
    scanner.manager = CBCentralManager(delegate: scanner, queue: nil)
    RunLoop.main.run(until: Date().addingTimeInterval(seconds))
    print("Scanned for \(Int(seconds)) seconds: \(scanner.heard.count) devices advertising, the ones above of interest.")
    exit(0)
}

let paired = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice] ?? []
print("Paired devices: \(paired.count)")
for device in paired {
    let name = device.nameOrAddress ?? "?"
    let sony = hasSonyName(name)
    print("- \(name)  [\(device.addressString ?? "?")]  connected=\(device.isConnected())  sony-name=\(sony)")
    guard sony else { continue }
    lookUp(device, "as macOS remembers it")
    guard device.isConnected() else {
        print("    not connected; not asking the headset")
        continue
    }
    print("    asking the headset for its services: \(askForServices(device))")
    lookUp(device, "as the headset lists it")
    for case let record as IOBluetoothSDPServiceRecord in device.services ?? [] {
        // The service's ids. A list by the specification; for the WH-1000XM3 macOS hands back
        // a lone id in every record.
        let classes = record.getAttributeDataElement(0x0001)
        let ids = (classes?.getArrayValue() as? [IOBluetoothSDPDataElement] ?? [classes].compactMap { $0 })
            .compactMap { $0.getUUIDValue()?.getWithLength(16) }
            .map { NSUUID(uuidBytes: [UInt8]($0 as Data)).uuidString }
        let serial = channel(of: record).map { "channel \($0)" } ?? "no serial channel"
        print("      \(ids.joined(separator: ", "))  \(serial)  \(record.getServiceName() ?? "")")
    }
}

print()
guard let target else {
    print("No connected Sony headset with a control service macOS can find.")
    exit(1)
}

var opened: IOBluetoothRFCOMMChannel?
let opening = target.device.openRFCOMMChannelAsync(&opened, withChannelID: target.channel, delegate: listener)
guard opening == kIOReturnSuccess, let link = opened,
      wait(upTo: 15, until: { listener.openStatus != nil }), listener.openStatus == kIOReturnSuccess
else {
    print("Control link: could not open channel \(target.channel), \(code(listener.openStatus ?? opening))")
    exit(1)
}
print("Control link to \(target.device.nameOrAddress ?? "?") open on channel \(target.channel), speaking \(target.generation)")

func send(_ bytes: [UInt8]) -> IOReturn {
    var bytes = bytes
    return link.writeSync(&bytes, length: UInt16(bytes.count))
}

// The same first question in both generations.
let question: [UInt8] = [0x00, 0x00]
print("> protocol info: \(hex(question))")
let sent = send(pack(typeData, 0, question))
guard sent == kIOReturnSuccess else {
    print("  sending failed, \(code(sent))")
    exit(1)
}

var acked = false
var answered = false
func readReplies() {
    for frame in takeFrames(from: &listener.received) {
        if frame.kind == typeAck {
            acked = true
        } else {
            answered = true
            print("  < type 0x\(hex([frame.kind]))  \(hex(frame.payload))")
            _ = send(pack(typeAck, 1 &- frame.seq, []))
        }
    }
}
wait(upTo: 1.5) { readReplies(); return answered || listener.closed }
// A short window for anything else the headset adds.
wait(upTo: 0.25) { readReplies(); return listener.closed }

if listener.closed {
    print("  (closed by the headset)")
} else if !acked {
    print("  (not acknowledged)")
} else if !answered {
    print("  (acknowledged, no reply)")
}
link.close()
exit(answered ? 0 : 1)
