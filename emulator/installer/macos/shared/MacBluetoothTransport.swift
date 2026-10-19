// SPDX-License-Identifier: MPL-2.0
import Foundation
import IOBluetooth

protocol MacBluetoothLink: AnyObject {
    func open(_ packet: @escaping ([String:Any])->Void)
    func send(_ request: [String:Any])
    func shutdown()
}

final class MacSPPTransport: NSObject, IOBluetoothRFCOMMChannelDelegate {
    static let uuid = "00001101-0000-1000-8000-00805f9b34fb"
    static func supported(_ name: String) -> Bool {
        let lower = name.lowercased()
        return ["carista evo", "vlinker mc", "vlinker mc-android"].contains(lower)
            || lower == "obdlink mx+" || lower.hasPrefix("obdlink mx+ ")
    }
    let device: IOBluetoothDevice
    private let allowed: (String)->Bool
    private var packet: (([String:Any])->Void)?
    private var channel: IOBluetoothRFCOMMChannel?, timer: Timer?
    private var closing = false, finished = false, writing = false, opening = false, ownsDevice = false
    private var closeError = ""
    private var deferred: [[UInt8]] = []
    init(device: IOBluetoothDevice, allowed: @escaping (String)->Bool) { self.device = device; self.allowed = allowed }
    func open(_ packet: @escaping ([String:Any])->Void) {
        self.packet = packet
        guard device.isPaired(), !device.isConnected(), Self.supported(device.name ?? ""),
              device.getLastServicesUpdate() != nil,
              let service = device.getServiceRecord(for:IOBluetoothSDPUUID(uuid16:0x1101)) else {
            fail("Pair this adapter with the Mac and confirm its Serial Port service first."); return
        }
        var id: BluetoothRFCOMMChannelID = 0
        guard service.getRFCOMMChannelID(&id) == kIOReturnSuccess else { fail("RFCOMM service channel unavailable"); return }
        timer = Timer.scheduledTimer(withTimeInterval:8, repeats:false) { [weak self] _ in self?.fail("Bluetooth Classic connection timed out; start a fresh connection.") }
        opening = true; ownsDevice = true
        let result = device.openRFCOMMChannelAsync(&channel, withChannelID:id, delegate:self)
        if result != kIOReturnSuccess { opening = false; fail("Native RFCOMM open failed: \(result)") }
    }
    func rfcommChannelOpenComplete(_ channel: IOBluetoothRFCOMMChannel!, status: IOReturn) {
        guard channel === self.channel else { return }; opening = false; timer?.invalidate(); timer = nil
        if closing { finishClose(); return }
        guard status == kIOReturnSuccess, channel.isOpen() else { fail("Native RFCOMM connection failed: \(status)"); return }
        packet?(["type":"ready", "transport":"macos-classic-spp", "native_api":"IOBluetoothRFCOMMChannel",
                 "uuid":Self.uuid, "channel_id":channel.getID(), "device_name":device.name ?? "",
                 "device_id":device.addressString ?? "", "service_discovery":"macos-sdp-cache", "connection_mode":"paired-spp"])
    }
    func send(_ request: [String:Any]) {
        if request["op"] as? String == "close" { close(); return }
        // INTERRUPT writes a bare CR (Android VlinkerSppTransport parity): it ends STN
        // monitoring; it is not a command and carries no vehicle payload.
        let interrupt = request["op"] as? String == "interrupt"
        guard !closing, !writing, interrupt || request["op"] as? String == "write",
              let command = interrupt ? "" : request["command"] as? String, interrupt || allowed(command),
              !command.contains("\r"), !command.contains("\n"), let channel, channel.isOpen() else { fail("Unsupported native Bluetooth write"); return }
        var bytes = Array((command + "\r").utf8)
        guard bytes.count <= Int(channel.getMTU()) else { fail("Native write exceeds RFCOMM MTU"); return }
        writing = true
        let result = bytes.withUnsafeMutableBytes { channel.writeSync($0.baseAddress, length:UInt16($0.count)) }
        writing = false
        guard result == kIOReturnSuccess else { fail("Native RFCOMM write failed: \(result)"); return }
        packet?(["type":"write_completed", "command":command, "interrupt":interrupt, "native_result":result])
        let received = deferred; deferred = []
        for bytes in received { guard !closing else { break }; packet?(["type":"rx", "bytes":bytes]) }
    }
    func rfcommChannelData(_ channel: IOBluetoothRFCOMMChannel!, data pointer: UnsafeMutableRawPointer!, length: Int) {
        guard channel === self.channel, !closing, let pointer, length > 0 else { return }
        guard length <= 16384 else { fail("Bluetooth response exceeded limit"); return }
        let bytes = Array(Data(bytes:pointer, count:length))
        if writing {
            guard deferred.reduce(0,{$0+$1.count}) + bytes.count <= 16384 else { fail("Bluetooth receive queue exceeded limit"); return }
            deferred.append(bytes)
        } else { packet?(["type":"rx", "bytes":bytes]) }
    }
    func rfcommChannelClosed(_ channel: IOBluetoothRFCOMMChannel!) {
        guard channel === self.channel else {return}; opening = false
        if closing {finishClose()} else {fail("Bluetooth disconnected; read again for fresh results.")}
    }
    private func fail(_ message: String) { guard !closing else { return }; packet?(["type":"error", "message":message]); close(error:message) }
    private func close(error: String = "") {
        guard !closing else { return }; closing = true; closeError = error; timer?.invalidate(); timer = nil
        if opening {
            _ = channel?.close()
            if ownsDevice && device.isConnected() {_ = device.closeConnection()}
            if opening {
                timer = Timer.scheduledTimer(withTimeInterval:5,repeats:false) { [weak self] _ in self?.packet?(["type":"discovery_status", "message":"Waiting for native Bluetooth connection cancellation confirmation…"]) }
                return
            }
        }
        finishClose()
    }
    private func finishClose() {
        guard closing, !opening, !finished else {return}; finished = true; timer?.invalidate(); timer = nil
        let channelResult = channel?.isOpen() == true ? channel!.close() : kIOReturnSuccess
        let deviceResult = ownsDevice && device.isConnected() ? device.closeConnection() : kIOReturnSuccess
        let deadline = Date().addingTimeInterval(3)
        while ownsDevice && device.isConnected() && Date() < deadline { RunLoop.current.run(until:Date().addingTimeInterval(0.05)) }
        let released = (!ownsDevice || !device.isConnected()) && channel?.isOpen() != true && channelResult == kIOReturnSuccess && deviceResult == kIOReturnSuccess
        let result: [String:Any] = ["type":"closed", "bluetooth_closed":released, "error":closeError.isEmpty && !released ? "Native Bluetooth release was not confirmed" : closeError,
                 "channel_close_result":channelResult, "device_close_result":deviceResult]
        // The MX+ needs a settle period after RFCOMM release before the same endpoint
        // can be reopened (discovery -> original-firmware handoff), as on Android.
        if (device.name ?? "").hasPrefix("OBDLink MX+") {
            DispatchQueue.main.asyncAfter(deadline:.now()+3) { [self] in packet?(result) }
        } else {packet?(result)}
    }
    func shutdown() { packet = nil; if !finished { close() }; channel?.setDelegate(nil); channel = nil }
}

// A BLE UUID and a Classic address identify different transport endpoints, not models.
// Only the explicitly selected endpoint is opened. No automatic transport fallback.
final class MacAdapterBluetoothLink: MacBluetoothLink {
    private let allowed: (String)->Bool
    private var ble: AppleBLETransport?, spp: MacSPPTransport?
    private var paired: [String:IOBluetoothDevice] = [:], bleDevices: [[String:String]] = []
    private var packet: (([String:Any])->Void)?
    private var selected = "", closing = false, authorized = false, pairedPending = false
    init(allowed: @escaping (String)->Bool) { self.allowed = allowed }
    func open(_ packet: @escaping ([String:Any])->Void) {
        self.packet = packet
        let transport = AppleBLETransport(allowed:allowed); ble = transport
        transport.open { [weak self] value in
            guard let self else { return }
            if value["type"] as? String == "scanning", value["scanning"] as? Bool == true {
                self.authorized = true; self.refreshPaired()
            } else if value["type"] as? String == "candidates" {
                self.bleDevices = (value["devices"] as? [[String:String]] ?? []).map { var item=$0; item["id"]="ble:"+(item["id"] ?? ""); return item }; self.candidates()
            } else if self.selected.hasPrefix("ble:") { self.packet?(value) }
            else if self.selected.isEmpty && !self.closing && value["type"] as? String == "error" { self.packet?(["type":"discovery_status", "message":value["message"] ?? "BLE discovery unavailable; paired Classic devices can still be selected."]) }
        }
    }
    private func refreshPaired() {
        // macOS 27's coordinator can wait for permission inside pairedDevices().
        // Request authorization through CoreBluetooth first; never block AppKit.
        guard authorized, !pairedPending, !closing else { return }; pairedPending = true
        DispatchQueue.global().async { [weak self] in
            let devices = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice] ?? []
            DispatchQueue.main.async {
                guard let self else {return}; self.pairedPending = false
                guard !self.closing, self.selected.isEmpty else {return}
                self.paired = [:]
                for device in devices {
                    if MacSPPTransport.supported(device.name ?? ""), let address = device.addressString { self.paired["classic:"+address] = device }
                }
                self.candidates()
            }
        }
    }
    private func candidates() {
        let classic = paired.keys.sorted().map { ["id":$0, "name":paired[$0]?.name ?? "", "transport":"macos-classic-spp"] }
        packet?(["type":"candidates", "devices":classic + bleDevices])
    }
    func send(_ request: [String:Any]) {
        if request["op"] as? String == "scan" { refreshPaired(); ble?.send(request); return }
        if request["op"] as? String == "select" {
            guard selected.isEmpty, !closing, let id = request["id"] as? String else { return }
            if let device = paired[id] {
                selected = id; ble?.shutdown(); ble = nil
                packet?(["type":"selected", "id":id, "name":device.name ?? "", "transport":"macos-classic-spp"])
                let transport = MacSPPTransport(device:device, allowed:allowed); spp = transport
                transport.open { [weak self] in self?.packet?($0) }
            } else if bleDevices.contains(where:{$0["id"] == id}), id.hasPrefix("ble:") {
                selected = id; var value = request; value["id"] = String(id.dropFirst(4)); ble?.send(value)
            }
            return
        }
        if request["op"] as? String == "close" {
            closing = true
            if selected.isEmpty { ble?.shutdown(); ble = nil; packet?(["type":"closed", "bluetooth_closed":true, "error":""]) }
            else if selected.hasPrefix("classic:") { spp?.send(request) } else { ble?.send(request) }
            return
        }
        if selected.hasPrefix("classic:") { spp?.send(request) } else if selected.hasPrefix("ble:") { ble?.send(request) }
    }
    func shutdown() { packet = nil; ble?.shutdown(); spp?.shutdown(); ble = nil; spp = nil }
}
