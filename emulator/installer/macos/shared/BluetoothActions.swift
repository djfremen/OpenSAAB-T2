// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation
import IOBluetooth

extension Shell {
    func detectAdapter(bluetoothMode: String = "startup") {
        guard !busy, session == nil else { return }
        if bluetoothMode == "startup" && store.selected == nil { firmwareDialog(); return }
        let alert = NSAlert(); alert.messageText = "Select connection type"
        alert.informativeText = "Choose a connection type, then select an available adapter."
        alert.addButton(withTitle:"USB adapters")
        alert.addButton(withTitle:"Bluetooth adapters"); alert.addButton(withTitle:"Cancel")
        switch alert.runModal() {
        case .alertFirstButtonReturn: detectUSBAdapter(readCodes:bluetoothMode == "ecm_dtc")
        case .alertSecondButtonReturn: bluetoothRead(bluetoothMode)
        default: break
        }
    }
    func bluetoothRead(_ mode: String) {
        guard !busy, session == nil, securityJob == nil else { return }
        bluetoothVehicle = [:]; vehicle.stringValue = "Select a Bluetooth adapter"
        let alert = NSAlert(); alert.messageText = "Bluetooth adapters — limited engine reads"
        alert.informativeText = "OBDLink MX+ (STN2255 v5.12.4 · r3.1.3): Connect and start reads a fresh VIN, releases the adapter, then opens the original Tech2 firmware over the same MX+. Read-only: codes and key status; code clearing is unavailable. Pair the MX+ in macOS Bluetooth settings first.\n\nLegacy Carista EVO / vLinker MC+ support is limited to VIN and Trionic 8 engine reads. Native I-bus diagnostics and SW-CAN wake are unavailable.\n\nBLE connects inside OpenSAAB. Pair Classic devices in macOS Bluetooth settings first."
        alert.addButton(withTitle:"Use selected adapter").isEnabled = false
        alert.addButton(withTitle:"Cancel")
        let list = NSPopUpButton(frame:NSRect(x:0,y:0,width:520,height:30))
        let explanation = label("Finding compatible legacy BLE and paired Classic adapters…")
        let find = ActionButton("Find nearby / refresh paired adapters") { [weak self] in self?.bluetoothWorkflow?.find() }
        let content = stack([list,explanation,find],vertical:true)
        content.frame = NSRect(x:0,y:0,width:520,height:120); content.widthAnchor.constraint(equalToConstant:520).isActive = true
        alert.accessoryView = content
        var candidates: [[String:String]] = []
        let workflow = MacBluetoothWorkflow(root:state,binary:binaries.appendingPathComponent("opensaab-connection"),mode:mode)
        bluetoothWorkflow = workflow; bluetoothPicker = alert; busy = true; refresh()
        workflow.onDevices = { items in
            let previous = list.indexOfSelectedItem >= 0 && list.indexOfSelectedItem < candidates.count ? candidates[list.indexOfSelectedItem]["id"] : nil
            candidates = items; list.removeAllItems()
            list.addItems(withTitles:items.map { ($0["name"] ?? "Adapter") + " · " + ($0["transport"] == "ble" ? "BLE" : "Classic") + " · " + ($0["id"] ?? "") })
            if let previous, let index = items.firstIndex(where:{$0["id"] == previous}) { list.selectItem(at:index) }
            alert.buttons.first?.isEnabled = !items.isEmpty
            explanation.stringValue = items.isEmpty ? "No compatible legacy adapter found. Power the adapter, then refresh." : "Choose one device. Product and firmware are verified before CAN traffic."
        }
        workflow.onStatus = { [weak self] in self?.status.stringValue = $0 }
        workflow.onVehicle = { [weak self] value in
            guard let self else { return }
            self.bluetoothVehicle = value["state"] as? String == "identified" ? value["vehicle"] as? [String:Any] ?? [:] : [:]
            if let vin = self.bluetoothVehicle["vin"] as? String {
                let adapter = self.bluetoothVehicle["adapter_label"] as? String ?? "Bluetooth adapter"
                let voltage = self.bluetoothVehicle["adapter_voltage"] as? String ?? ""
                self.vehicle.stringValue = "VIN: " + vin + "\n" + adapter + (voltage.isEmpty ? "" : " · " + voltage)
            } else {self.vehicle.stringValue = "No current vehicle identity · Connect for a fresh VIN"}
        }
        workflow.onComplete = { [weak self] report in
            guard let self else { return }
            if let picker = self.bluetoothPicker, picker.window.sheetParent != nil { self.window.endSheet(picker.window,returnCode:.alertSecondButtonReturn) }
            self.bluetoothPicker = nil; self.bluetoothWorkflow = nil; self.busy = false
            self.status.stringValue = report["message"] as? String ?? "Bluetooth read finished"
            if let error = report["storage_error"] as? String { self.status.stringValue += " · Could not save report: " + error }
            self.lastConsole = String(data:(try? JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys])) ?? Data(),encoding:.utf8) ?? ""
            if ["ecm_info","ecm_dtc"].contains(mode), report["status"] as? String == "vin_received",
               let result = report["result"] as? [String:Any], let display = result["display"] as? String {
                self.textDialog(mode == "ecm_info" ? "ECM information — HS-CAN" : "Read engine codes — HS-CAN",display)
            }
            self.refresh()
            if mode == "startup", report["status"] as? String == "identified",
               let handoff = report["mx_handoff"] as? [String:String], handoff["model"] == "obdlink_mxp", let address = handoff["address"] {
                self.startMXFirmware(address:address, vehicle:report["vehicle"] as? [String:Any] ?? [:])
            }
        }
        do { try workflow.start() }
        catch { bluetoothWorkflow = nil; bluetoothPicker = nil; busy = false; showError(error.localizedDescription); refresh(); return }
        alert.beginSheetModal(for:window) { [weak self, weak workflow] choice in
            guard let self, self.bluetoothWorkflow === workflow else { return }; self.bluetoothPicker = nil
            if choice == .alertFirstButtonReturn, list.indexOfSelectedItem >= 0, list.indexOfSelectedItem < candidates.count, let id = candidates[list.indexOfSelectedItem]["id"] { workflow?.select(id) }
            else { workflow?.stop() }
        }
    }
    /// Primary MX+ route: hand the verified, released discovery endpoint to the
    /// original-firmware worker over a fresh native RFCOMM owner. A failed handoff
    /// reports unavailable; it never starts offline emulation as a substitute.
    func startMXFirmware(address: String, vehicle: [String:Any]) {
        guard !busy, session == nil else { return }
        guard fm.isExecutableFile(atPath:binaries.appendingPathComponent("tech2-mx").path) else {
            status.stringValue = "MX+ identified · Original-firmware MX+ binding is not included in this build"; return
        }
        guard let device = IOBluetoothDevice(addressString:address), (device.name ?? "").hasPrefix("OBDLink MX+") else {
            status.stringValue = "MX+ firmware session unavailable · Adapter no longer selectable · No offline fallback"; return
        }
        // The MX+ needs a short settle after the discovery RFCOMM release before the
        // same endpoint is reopened. Hold the UI busy so nothing else claims it.
        busy = true; status.stringValue = MxSessionOutcome.readSummary + " · Releasing discovery connection"; refresh()
        DispatchQueue.main.asyncAfter(deadline:.now()+3) { [weak self] in
            guard let self else { return }
            self.busy = false
            guard self.session == nil, !device.isConnected() else {
                self.status.stringValue = "MX+ firmware session unavailable · Adapter still connected elsewhere · No offline fallback"; self.refresh(); return
            }
            self.openMXFirmware(device:device, vehicle:vehicle)
        }
    }
    private func openMXFirmware(device: IOBluetoothDevice, vehicle: [String:Any]) {
        let bridge: MacMXFirmwareBridge
        do { bridge = try MacMXFirmwareBridge(device:device) }
        catch { status.stringValue = "MX+ firmware session unavailable: " + error.localizedDescription; refresh(); return }
        bridge.vehicle = vehicle
        busy = true; status.stringValue = MxSessionOutcome.readSummary + " · Opening adapter"; refresh()
        bridge.open { [weak self] result in
            guard let self else { bridge.close(); return }
            self.busy = false
            switch result {
            case .success:
                self.start(mx:bridge)
                if self.session?.mx !== bridge { bridge.close() }
            case .failure(let error):
                bridge.close()
                self.status.stringValue = "MX+ firmware session unavailable: " + error.localizedDescription + " · No offline fallback was started"
                self.refresh()
            }
        }
    }
    func bluetoothTools() {
        popup([
            ("Bluetooth adapters — limited engine reads", {self.bluetoothRead("identity")}),
            ("Read ECM VIN", {self.bluetoothRead("vin")}),
            ("Read ECM information — HS-CAN", {self.bluetoothRead("ecm_info")}),
            ("Read engine codes — HS-CAN", {self.bluetoothRead("ecm_dtc")}),
            ("USB adapters", {self.detectUSBAdapter()}),
            ("Saved DTC reports", {self.savedDtcReports()})
        ])
    }
    func bluetoothDetails() -> String {
        guard let vin = bluetoothVehicle["vin"] as? String else { return "" }
        let rows = [("VIN",vin),("Adapter",bluetoothVehicle["adapter_label"] as? String ?? ""),
            ("Voltage",bluetoothVehicle["adapter_voltage"] as? String ?? ""),
            ("Model year",String(describing:bluetoothVehicle["model_year"] ?? "")),
            ("Platform",bluetoothVehicle["platform"] as? String ?? ""),("Engine",bluetoothVehicle["engine"] as? String ?? ""),
            ("Color",bluetoothVehicle["color"] as? String ?? ""),("Gearbox",bluetoothVehicle["gearbox"] as? String ?? ""),
            ("Body",bluetoothVehicle["body"] as? String ?? "")]
        return rows.filter{!$0.1.isEmpty}.map{$0.0+": "+$0.1}.joined(separator:"\n") + "\nFresh read completed · Adapter connection closed\n\n"
    }
}
