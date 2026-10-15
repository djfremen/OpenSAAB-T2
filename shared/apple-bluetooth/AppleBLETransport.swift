// SPDX-License-Identifier: MPL-2.0
// Shared Apple OS binding. Command policy, identity and CAN decoding remain in Rust.
import Foundation
import CoreBluetooth

struct AppleBLEProfile: Equatable {
    let id: String, service: String, write: String, notify: String
    let permitsWithoutResponse: Bool
    static let mc = AppleBLEProfile(id:"vlinker-mc-ios", service:"18F0", write:"2AF1", notify:"2AF0", permitsWithoutResponse:false)
    // EVO's live M4 discovery showed this same service, .write TX and .notify RX.
    static let evo = AppleBLEProfile(id:"carista-evo", service:"18F0", write:"2AF1", notify:"2AF0", permitsWithoutResponse:false)
    static func advertised(_ name: String) -> AppleBLEProfile? {
        switch name.trimmingCharacters(in:.whitespacesAndNewlines).lowercased() {
        case "vlinker mc-ios": return .mc
        case "carista evo": return .evo
        default: return nil
        }
    }
    static func accepts(_ packet: [String:Any]) -> Bool {
        [mc,evo].contains { p in
            (packet["service"] as? String)?.uppercased() == p.service &&
            (packet["write"] as? String)?.uppercased() == p.write &&
            (packet["notify"] as? String)?.uppercased() == p.notify &&
            ((packet["write_type"] as? String == "withResponse") ||
             p.permitsWithoutResponse && packet["write_type"] as? String == "withoutResponse")
        }
    }
}

// All public calls and delegate callbacks run on the main queue, on iOS and macOS.
// Discovery never connects automatically. A caller must select a discovered UUID.
final class AppleBLETransport: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    private let allowed: (String)->Bool
    private var central: CBCentralManager?
    private var devices: [String:CBPeripheral] = [:], names: [String:String] = [:]
    private var packet: (([String:Any])->Void)?
    private var device: CBPeripheral?, profile: AppleBLEProfile?
    private var tx: CBCharacteristic?, rx: CBCharacteristic?
    private var ready = false, closing = false, closed = false
    private var closeError = "", command: String?
    private var chunks: [Data] = [], deferred: [Data] = []
    private var writeType = CBCharacteristicWriteType.withResponse
    private var scanTimer: Timer?, managerTimer: Timer?
    init(allowed: @escaping (String)->Bool) { self.allowed = allowed; super.init() }
    func open(_ packet: @escaping ([String:Any])->Void) {
        precondition(Thread.isMainThread)
        guard self.packet == nil else { return }; self.packet = packet
        managerTimer = Timer.scheduledTimer(withTimeInterval:10, repeats:false) { [weak self] _ in
            guard let self, self.central?.state != .poweredOn, !self.closing else { return }
            self.fail("Bluetooth permission or radio readiness was not confirmed. Allow Bluetooth access, then start a fresh connection.")
        }
        central = CBCentralManager(delegate:self, queue:.main)
    }
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        guard packet != nil, !closed else { return }
        if closing {
            if device == nil || device?.state == .disconnected { released() }
            return
        }
        switch central.state {
        case .poweredOn: managerTimer?.invalidate(); managerTimer = nil; if device == nil { scan() }
        case .unauthorized: fail("Allow Bluetooth access in Settings to connect to the adapter.")
        case .unsupported: fail("Bluetooth LE is unavailable on this device.")
        case .poweredOff: fail("Turn on Bluetooth to connect to the adapter.")
        case .resetting: fail("Bluetooth restarted; start a fresh connection.")
        default: break
        }
    }
    private func scan() {
        guard !closing, device == nil, central?.state == .poweredOn else { return }
        scanTimer?.invalidate()
        central?.scanForPeripherals(withServices:nil, options:[CBCentralManagerScanOptionAllowDuplicatesKey:false])
        packet?(["type":"scanning", "scanning":true])
        scanTimer = Timer.scheduledTimer(withTimeInterval:15, repeats:false) { [weak self] _ in
            guard let self, !self.closing else { return }; self.central?.stopScan()
            self.packet?(["type":"scanning", "scanning":false])
        }
    }
    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral, advertisementData: [String:Any], rssi: NSNumber) {
        let name = advertisementData[CBAdvertisementDataLocalNameKey] as? String ?? peripheral.name ?? ""
        guard !closing, device == nil, AppleBLEProfile.advertised(name) != nil else { return }
        let id = peripheral.identifier.uuidString
        devices[id] = peripheral; names[id] = name
        packet?(["type":"candidates", "devices":devices.keys.sorted().map { ["id":$0, "name":names[$0] ?? "", "transport":"ble"] }])
    }
    func send(_ request: [String:Any]) {
        precondition(Thread.isMainThread)
        switch request["op"] as? String {
        case "close": close(); return
        case "scan": scan(); return
        case "select":
            guard !closing, device == nil, let id = request["id"] as? String,
                  let p = devices[id], let hint = names[id], let selected = AppleBLEProfile.advertised(hint),
                  let central, central.state == .poweredOn else { fail("Select a currently discovered Bluetooth adapter."); return }
            scanTimer?.invalidate(); central.stopScan(); device = p; profile = selected
            p.delegate = self; central.connect(p, options:nil)
            packet?(["type":"selected", "id":id, "name":hint, "transport":"ble"]); return
        case "write": break
        default: fail("Unsupported Bluetooth transport request"); return
        }
        guard !closing, ready, command == nil, let value = request["command"] as? String, allowed(value),
              let device, device.state == .connected, tx != nil else { fail("Unsupported or overlapping Bluetooth write"); return }
        let bytes = Data((value + "\r").utf8), size = device.maximumWriteValueLength(for:writeType)
        guard size > 0 else { fail("Bluetooth write size unavailable"); return }
        command = value
        chunks = stride(from:0, to:bytes.count, by:size).map { Data(bytes[$0..<min($0+size,bytes.count)]) }
        writeNext()
    }
    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        guard peripheral === device else { return }
        if closing { central.cancelPeripheralConnection(peripheral); return }
        guard let profile else { fail("Bluetooth profile unavailable"); return }
        peripheral.discoverServices([CBUUID(string:profile.service)])
    }
    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        guard peripheral === device else { return }; device = nil
        fail("Bluetooth connection failed: " + (error?.localizedDescription ?? "unknown"))
        if closing { released() }
    }
    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        guard peripheral === device else { return }
        if !closing { closeError = "Bluetooth disconnected; read again for fresh results." }
        closing = true; released()
    }
    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard peripheral === device, !closing, let profile else { return }
        guard error == nil, let service = peripheral.services?.first(where:{$0.uuid == CBUUID(string:profile.service)}) else { fail("Selected adapter BLE service unavailable"); return }
        peripheral.discoverCharacteristics([CBUUID(string:profile.write), CBUUID(string:profile.notify)], for:service)
    }
    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard peripheral === device, !closing, let profile, service.uuid == CBUUID(string:profile.service) else { return }
        guard error == nil,
              let write = service.characteristics?.first(where:{$0.uuid == CBUUID(string:profile.write)}),
              let notify = service.characteristics?.first(where:{$0.uuid == CBUUID(string:profile.notify) && $0.properties.contains(.notify)}),
              write.properties.contains(.write) || profile.permitsWithoutResponse && write.properties.contains(.writeWithoutResponse)
        else { fail("Selected adapter BLE characteristics unavailable"); return }
        writeType = write.properties.contains(.write) ? .withResponse : .withoutResponse
        tx = write; rx = notify; peripheral.setNotifyValue(true, for:notify)
    }
    func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        guard peripheral === device, characteristic === rx, !closing, !ready, let profile else { return }
        guard error == nil, characteristic.isNotifying else { fail("Bluetooth notification subscription failed"); return }
        ready = true
        packet?(["type":"ready", "transport":"ble", "native_api":"CoreBluetooth", "device_id":peripheral.identifier.uuidString,
                 "device_name":names[peripheral.identifier.uuidString] ?? peripheral.name ?? "", "profile":profile.id,
                 "service":profile.service, "write":profile.write, "notify":profile.notify,
                 "write_type":writeType == .withResponse ? "withResponse" : "withoutResponse"])
    }
    private func writeNext() {
        guard !closing, let device, let tx, command != nil else { return }
        if writeType == .withResponse {
            if !chunks.isEmpty { device.writeValue(chunks.removeFirst(), for:tx, type:.withResponse) }
        } else {
            while !chunks.isEmpty && device.canSendWriteWithoutResponse { device.writeValue(chunks.removeFirst(), for:tx, type:.withoutResponse) }
            if chunks.isEmpty { completedWrite() }
        }
    }
    func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) { if peripheral === device { writeNext() } }
    func peripheral(_ peripheral: CBPeripheral, didWriteValueFor characteristic: CBCharacteristic, error: Error?) {
        guard peripheral === device, characteristic === tx, !closing, command != nil, writeType == .withResponse else { return }
        guard error == nil else { fail("Bluetooth write failed"); return }
        if chunks.isEmpty { completedWrite() } else { writeNext() }
    }
    private func completedWrite() {
        guard let active = command else { return }; command = nil
        packet?(["type":"write_completed", "command":active, "completion_scope":writeType == .withResponse ? "gatt-response" : "corebluetooth-queued"])
        let received = deferred; deferred = []
        for bytes in received { guard !closing else { break }; packet?(["type":"rx", "bytes":Array(bytes)]) }
    }
    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard peripheral === device, characteristic === rx, !closing else { return }
        guard error == nil, let bytes = characteristic.value else { fail("Bluetooth receive failed"); return }
        guard bytes.count <= 16384 else { fail("Bluetooth response exceeded limit"); return }
        if command != nil {
            guard deferred.reduce(0,{$0+$1.count}) + bytes.count <= 16384 else { fail("Bluetooth receive queue exceeded limit"); return }
            deferred.append(bytes)
        } else { packet?(["type":"rx", "bytes":Array(bytes)]) }
    }
    private func fail(_ message: String) {
        guard !closing else { return }; closeError = message
        packet?(["type":"error", "message":message]); close()
    }
    private func close() {
        guard !closing else { return }; closing = true
        managerTimer?.invalidate(); scanTimer?.invalidate()
        if central?.state == .poweredOn { central?.stopScan() }
        if let device, device.state != .disconnected { central?.cancelPeripheralConnection(device) }
        else { released() }
    }
    private func released() {
        guard !closed else { return }; closed = true; closing = true
        managerTimer?.invalidate(); scanTimer?.invalidate()
        device?.delegate = nil; device = nil; tx = nil; rx = nil; chunks = []; deferred = []
        packet?(["type":"closed", "error":closeError, "bluetooth_closed":true])
    }
    func shutdown() {
        packet = nil; managerTimer?.invalidate(); scanTimer?.invalidate()
        if central?.state == .poweredOn { central?.stopScan() }
        if let device { device.delegate = nil; central?.cancelPeripheralConnection(device) }
        central?.delegate = nil; device = nil; central = nil
    }
}
