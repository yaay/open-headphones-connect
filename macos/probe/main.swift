// Prints what macOS knows about each paired Bluetooth device and whether the Sony control
// service can be reached, then asks the headset one question and prints the raw reply.
// Read-only: nothing here changes a setting on the headset.
//
//     macos/run.sh probe
//
// The Mac counterpart of `--probe` in src-tauri/src/bluetooth/probe.rs.

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
