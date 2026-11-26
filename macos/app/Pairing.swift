// Offers to pair headphones that are in pairing mode nearby and not yet paired with this Mac, the
// way the system does for AirPods: the card appears with a Pair button.
//
// Headphones in pairing mode say so over Bluetooth LE, for phones and PCs to offer exactly this:
// Google Fast Pair (service data FE2C holding a three-byte model id) and Microsoft Swift Pair
// (manufacturer data 0006 03 …, with the name and often the classic address). Listening for those
// is cheap and touches no audio. Pairing itself is classic Bluetooth, with a device that a short
// search found: an advertisement is anybody's word, so what it says (a name, an address) only
// ever leads to an offer. The device paired with has to answer the search itself, as an audio
// device, under a supported headphone's name.
//
// `macos/run.sh probe scan` and `probe inquire` show what this hears and finds.

import AppKit
import CoreBluetooth
import IOBluetooth
import Observation

/// Headphones nearby that could be paired.
struct PairingOffer: Equatable {
    /// What the headphones call themselves.
    var name: String
    /// Their classic Bluetooth address, where the advertisement gave it.
    var address: String?

    /// The model the app has pictures under (`resourceName`): the name, less the brand that
    /// Nothing's own products carry in theirs.
    var model: String { Pairing.key(name) }
}

@Observable
final class Pairing: NSObject {
    static let shared = Pairing()

    enum Stage: Equatable {
        case offered
        /// Finding the headphones, pairing, then waiting for them to connect; with what to say.
        case working(String)
        case failed(String)
    }

    private(set) var offer: PairingOffer?
    private(set) var stage = Stage.offered
    /// Goes up with every new offer, for the menu-bar item to open the card on.
    private(set) var offers = 0

    @ObservationIgnored private var central: CBCentralManager?
    private let queue = DispatchQueue(label: "pairing-scan")
    @ObservationIgnored private var cycle: Timer?
    /// Headphones the user said "not now" to, until the app is started again.
    @ObservationIgnored private var declined = Set<String>()
    @ObservationIgnored private var lastHeard = Date.distantPast
    /// When a search last went looking for headphones whose advertisement did not name them.
    @ObservationIgnored private var lastIdentified = Date.distantPast
    @ObservationIgnored private var inquiry: IOBluetoothDeviceInquiry?
    @ObservationIgnored private var pair: IOBluetoothDevicePair?
    @ObservationIgnored private var device: IOBluetoothDevice?
    @ObservationIgnored private var deadline: Timer?
    @ObservationIgnored private var isPreview = false
    /// Headphones that searches have found and checked, by the name they compare under, so that
    /// pairing with them need not find them again.
    @ObservationIgnored private var candidates: [String: IOBluetoothDevice] = [:]
    /// Whether listening is paused: while a search or a pairing is under way, which want the
    /// radio to themselves.
    @ObservationIgnored private var paused = false
    @ObservationIgnored private var logged = Set<String>()
    private let logLock = NSLock()

    private static let fastPair = CBUUID(string: "FE2C")

    /// Fast Pair model ids, for advertisements that do not name their sender. Nothing's and CMF's
    /// are from the Nothing X app's tables as Kuksenok-i-s/nothing_helper (MIT) lists them, not
    /// checked against real devices; the WH-1000XM3's was heard from one. Other Sony headphones
    /// name themselves in their advertisement, or are found by a search.
    private static let fastPairModels: [UInt32: String] = [
        0x0BC95C: "WH-1000XM3",
        0x31D53D: "Ear (1)", 0xDEE8C0: "Ear (2)", 0xFEB1C7: "Nothing Ear", 0xC1EBFD: "Ear (3)",
        0x1016DD: "Ear (stick)", 0x5E3FBC: "Ear (a)", 0xFC3AAF: "Ear (open)", 0x2D6FDA: "Headphone (1)",
        0xAE35FD: "CMF Neckband Pro", 0xADD2C4: "CMF Buds Pro", 0x150A27: "CMF Buds", 0xF29566: "CMF Buds Pro 2",
    ]

    /// The name headphones go by in classic Bluetooth, which is the one people know: over
    /// Bluetooth LE, Sony's call themselves "LE_WH-1000XM3".
    static func classic(_ name: String) -> String {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        return trimmed.hasPrefix("LE_") ? String(trimmed.dropFirst(3)) : trimmed
    }

    /// A name for comparing, and the core's name for the model: "Nothing Ear (a)" and "Ear (a)"
    /// are the same headphones.
    static func key(_ name: String) -> String {
        let plain = classic(name)
        return plain.lowercased().hasPrefix("nothing ") ? String(plain.dropFirst(8)) : plain
    }

    private static func same(_ a: String, _ b: String) -> Bool {
        key(a).caseInsensitiveCompare(key(b)) == .orderedSame
    }

    private var wanted: Bool { UserDefaults.standard.bool(forKey: Preference.pairingOffers) }

    /// What was heard and how pairing went, on the app's standard error, each thing once.
    private func log(_ what: String) {
        logLock.withLock {
            guard logged.insert(what).inserted else { return }
            FileHandle.standardError.write(Data("Pairing: \(what)\n".utf8))
        }
    }

    /// Starts listening. Call once, on the main thread.
    func start() {
        guard central == nil else { return }
        // On a queue of its own: on the main one it can deadlock with IOBluetooth's first call.
        central = CBCentralManager(delegate: self, queue: queue, options: [CBCentralManagerOptionShowPowerAlertKey: false])
        // A device heard once is not reported again within a scan, so scans are short ones in a
        // row: headphones that enter pairing mode are heard in the next.
        cycle = Timer.scheduledTimer(withTimeInterval: 8, repeats: true) { [weak self] _ in self?.tick() }
    }

    private func tick() {
        let wanted = wanted && !paused
        queue.async { [central] in
            guard let central, central.state == .poweredOn else { return }
            central.stopScan()
            if wanted {
                central.scanForPeripherals(withServices: nil)
            }
        }
        // Headphones that left pairing mode, or were paired with something else, are no longer on offer.
        if offer != nil, stage == .offered, !isPreview, Date().timeIntervalSince(lastHeard) > 30 {
            offer = nil
        }
    }

    /// Shows an offer for made-up headphones, for trying the card without any (`--offer NAME`).
    func preview(_ name: String) {
        isPreview = true
        offer = PairingOffer(name: name)
        stage = .offered
        offers += 1
    }

    // MARK: Noticing

    /// Something in pairing mode was heard. `name` is nil where its advertisement did not say.
    private func heard(name: String?, address: String?) {
        guard wanted, !isPreview else { return }
        guard let name = name.map(Self.classic) else { return identify() }
        if let offer, Self.same(offer.name, name) {
            lastHeard = Date()
            if offer.address == nil, let address {
                self.offer?.address = address
            }
            return
        }
        guard offer == nil, !declined.contains(Self.key(name).lowercased()) else { return }
        guard ohc_is_headset_name(name) else { return log("\(name) is not headphones this app supports") }
        guard !isPaired(name: name, address: address) else { return log("\(name) is paired with this Mac already") }
        log("offering to pair \(name)\(address.map { " [\($0)]" } ?? ", address not given")")
        lastHeard = Date()
        stage = .offered
        offer = PairingOffer(name: name, address: address)
        offers += 1
    }

    private func isPaired(name: String, address: String?) -> Bool {
        if let address, let device = IOBluetoothDevice(addressString: address) {
            return device.isPaired()
        }
        // Without an address: headphones of this name are paired already, so these are those,
        // in pairing mode for something else.
        let paired = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice] ?? []
        return paired.contains { Self.same($0.name ?? "", name) }
    }

    /// A short search for what an unnamed advertisement came from. Searching can make Bluetooth
    /// audio stutter for a moment, so this is rare: once in two minutes at most.
    private func identify() {
        guard offer == nil, inquiry == nil, Date().timeIntervalSince(lastIdentified) > 120 else { return }
        lastIdentified = Date()
        search(seconds: 6)
    }

    private func search(seconds: UInt8) {
        let inquiry = IOBluetoothDeviceInquiry(delegate: self)
        inquiry?.inquiryLength = seconds
        inquiry?.updateNewDeviceNames = true
        inquiry?.setSearchCriteria(
            BluetoothServiceClassMajor(kBluetoothServiceClassMajorAny),
            majorDeviceClass: BluetoothDeviceClassMajor(kBluetoothDeviceClassMajorAudio),
            minorDeviceClass: BluetoothDeviceClassMinor(kBluetoothDeviceClassMinorAny))
        self.inquiry = inquiry
        if inquiry?.start() != kIOReturnSuccess {
            self.inquiry = nil
        }
        // A search does not always report its end.
        DispatchQueue.main.asyncAfter(deadline: .now() + TimeInterval(seconds) + 15) { [weak self] in
            if let self, self.inquiry === inquiry {
                inquiry?.stop()
                self.inquiry = nil
            }
        }
    }

    /// A classic device a search turned up.
    private func found(_ found: IOBluetoothDevice) {
        let audio = found.deviceClassMajor == BluetoothDeviceClassMajor(kBluetoothDeviceClassMajorAudio)
        log("the search found \(found.name ?? "an unnamed device") [\(found.addressString ?? "?")], audio: \(audio), paired: \(found.isPaired())")
        // Only what says it is an audio device, under a supported headphone's name, is ever
        // offered or paired with.
        guard audio, let name = found.name, ohc_is_headset_name(name), !found.isPaired() else { return }
        candidates[Self.key(name).lowercased()] = found
        if case .working = stage, let offer, device == nil {
            // Looking for the headphones on offer, to pair with them. Where their advertisement
            // gave an address, it has to be this device's.
            let address = found.addressString ?? ""
            if Self.same(offer.name, name), offer.address.map({ $0.caseInsensitiveCompare(address) == .orderedSame }) ?? true {
                inquiry?.stop()
                inquiry = nil
                begin(found)
            }
        } else if offer == nil {
            // Looking for whatever said it was in pairing mode.
            heard(name: name, address: found.addressString)
        }
    }

    // MARK: Pairing

    func pairNow() {
        guard let offer else { return }
        device = nil
        stage = .working("Looking for them…")
        if isPreview {
            return expire(after: 4, saying: nil)
        }
        // With a device a search found, even where the advertisement gave an address: see the
        // top. One found earlier will do, if the address agrees.
        if let known = candidates[Self.key(offer.name).lowercased()], !known.isPaired(),
           offer.address.map({ $0.caseInsensitiveCompare(known.addressString ?? "") == .orderedSame }) ?? true {
            return begin(known)
        }
        // The search gets the radio to itself, and goes on until it finds them or time is up:
        // one pass of ten seconds does not always catch headphones that are there.
        paused = true
        tick()
        inquiry?.stop()
        search(seconds: 10)
        if inquiry == nil {
            return fail("The search for them couldn't start.")
        }
        expire(after: 45, saying: "Couldn't find them. Check that they are still in pairing mode.")
    }

    private func begin(_ found: IOBluetoothDevice) {
        device = found
        log("pairing with \(found.name ?? "?") [\(found.addressString ?? "?")]")
        stage = .working("Pairing…")
        let pair = IOBluetoothDevicePair(device: found)
        pair?.delegate = self
        self.pair = pair
        let started = pair?.start() ?? kIOReturnError
        if started != kIOReturnSuccess {
            return fail("Pairing couldn't start (\(String(format: "0x%08x", UInt32(bitPattern: started)))).")
        }
        expire(after: 40, saying: "Pairing took too long. Put them in pairing mode again and retry.")
    }

    /// Paired: has them connect, and waits until the session has them.
    private func connect() {
        guard let device, let offer else { return }
        stage = .working("Connecting…")
        _ = device.openConnection()
        deadline?.invalidate()
        var waited = 0
        deadline = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] timer in
            guard let self else { return timer.invalidate() }
            waited += 1
            headset.rescan()
            let snapshot = headset.snapshot
            if snapshot.phase == .connected, Self.same(snapshot.deviceName ?? "", offer.name) {
                // The card goes on as the connect card.
                self.log("\(offer.name) is paired and connected")
                timer.invalidate()
                self.finish()
            } else if waited >= 25 {
                timer.invalidate()
                self.fail("Paired, but they haven't connected. Use Connect in the menu bar panel.")
            }
        }
    }

    func notNow() {
        if let offer {
            declined.insert(Self.key(offer.name).lowercased())
        }
        finish()
    }

    private func finish() {
        stop()
        offer = nil
        stage = .offered
    }

    private func fail(_ why: String) {
        log("failed: \(why)")
        stop()
        stage = .failed(why)
    }

    /// Ends whatever search or pairing is under way, and listens again.
    private func stop() {
        deadline?.invalidate()
        inquiry?.stop()
        pair?.stop()
        (inquiry, pair, device) = (nil, nil, nil)
        paused = false
        tick()
    }

    /// Gives up after a while with `why`, or (a made-up offer) just ends.
    private func expire(after seconds: TimeInterval, saying why: String?) {
        deadline?.invalidate()
        deadline = Timer.scheduledTimer(withTimeInterval: seconds, repeats: false) { [weak self] _ in
            if let why { self?.fail(why) } else { self?.finish() }
        }
    }
}

extension Pairing: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        if central.state == .poweredOn, UserDefaults.standard.bool(forKey: Preference.pairingOffers) {
            central.scanForPeripherals(withServices: nil)
        }
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber) {
        let serviceData = advertisementData[CBAdvertisementDataServiceDataKey] as? [CBUUID: Data]
        let maker = (advertisementData[CBAdvertisementDataManufacturerDataKey] as? Data).map { [UInt8]($0) } ?? []
        var name = advertisementData[CBAdvertisementDataLocalNameKey] as? String ?? peripheral.name
        var address: String?
        var pairable = false
        // Fast Pair: three bytes are a model id, sent only in pairing mode.
        if let id = serviceData?[Self.fastPair].map({ [UInt8]($0) }), id.count == 3 {
            pairable = true
            name = name ?? Self.fastPairModels[UInt32(id[0]) << 16 | UInt32(id[1]) << 8 | UInt32(id[2])]
        }
        // Swift Pair: 0006 03, a scenario, a reserved byte, then for scenario 1 the classic
        // address and class of device, for 2 the class alone, and the name to show.
        if maker.count >= 5, maker[0] == 0x06, maker[1] == 0x00, maker[2] == 0x03 {
            pairable = true
            let start = maker[3] == 0x01 ? 14 : maker[3] == 0x02 ? 8 : 5
            if maker[3] == 0x01, maker.count >= 11 {
                address = maker[5..<11].reversed().map { String(format: "%02x", $0) }.joined(separator: "-")
            }
            if maker.count > start, let said = String(bytes: maker[start...], encoding: .utf8), !said.isEmpty {
                name = said
            }
        }
        guard pairable else { return }
        let fastPair = serviceData?[Self.fastPair].map { $0.map { String(format: "%02x", $0) }.joined() }
        log("heard \(name ?? "something unnamed") in pairing mode"
            + (fastPair.map { ", Fast Pair \($0)" } ?? "")
            + (maker.count >= 5 && maker[2] == 0x03 && maker[0] == 0x06 ? ", Swift Pair \(maker.map { String(format: "%02x", $0) }.joined())" : ""))
        DispatchQueue.main.async { [name, address] in
            self.heard(name: name, address: address)
        }
    }
}

extension Pairing: IOBluetoothDeviceInquiryDelegate {
    func deviceInquiryDeviceFound(_ sender: IOBluetoothDeviceInquiry!, device: IOBluetoothDevice!) {
        found(device)
    }

    func deviceInquiryDeviceNameUpdated(_ sender: IOBluetoothDeviceInquiry!, device: IOBluetoothDevice!, devicesRemaining: UInt32) {
        found(device)
    }

    func deviceInquiryComplete(_ sender: IOBluetoothDeviceInquiry!, error: IOReturn, aborted: Bool) {
        guard sender === inquiry else { return }
        inquiry = nil
        // Not found yet: look again, until the time `pairNow` allowed is up.
        if case .working = stage, device == nil, !aborted {
            search(seconds: 10)
        }
    }
}

extension Pairing: IOBluetoothDevicePairDelegate {
    /// Headphones have no screen to show a number on, and macOS does not ask this for them (a
    /// WH-1000XM3 pairs without). What does ask is not headphones, whatever it calls itself, and
    /// is refused: nothing is ever confirmed on the user's behalf.
    func devicePairingUserConfirmationRequest(_ sender: Any!, numericValue: BluetoothNumericValue) {
        log("asked to confirm a number while pairing: refused")
        (sender as? IOBluetoothDevicePair)?.replyUserConfirmation(false)
        if (sender as? IOBluetoothDevicePair) === pair {
            fail("These asked to compare a code, which headphones don't. Pair them in Bluetooth settings instead.")
        }
    }

    /// The same for a device that wants a code typed in.
    func devicePairingPINCodeRequest(_ sender: Any!) {
        log("asked for a code while pairing: refused")
        if (sender as? IOBluetoothDevicePair) === pair {
            fail("These asked for a code. Pair them in Bluetooth settings instead.")
        }
    }

    func devicePairingFinished(_ sender: Any!, error: IOReturn) {
        guard (sender as? IOBluetoothDevicePair) === pair else { return }
        log("pairing finished: \(String(format: "0x%08x", UInt32(bitPattern: error)))")
        pair = nil
        if error == kIOReturnSuccess {
            connect()
        } else {
            fail("Pairing didn't work (\(String(format: "0x%08x", UInt32(bitPattern: error)))). Put them in pairing mode again and retry.")
        }
    }
}
