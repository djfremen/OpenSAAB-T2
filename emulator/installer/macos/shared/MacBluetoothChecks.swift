// SPDX-License-Identifier: MPL-2.0
import Foundation

// Explicit synthetic endpoint tests; never open a radio or send a lookup request.
private final class FixtureMacBluetooth: MacBluetoothLink {
    let model: String, transport: String, allowed: (String)->Bool
    var firmware = "", badProfile = false, disconnect = false, confirmClose = true
    var sent: [String] = [], opened = false
    private var packet: (([String:Any])->Void)?
    init(model: String, transport: String, allowed: @escaping (String)->Bool) { self.model=model; self.transport=transport; self.allowed=allowed }
    func open(_ packet: @escaping ([String:Any])->Void) { opened=true; self.packet=packet; packet(["type":"candidates", "devices":[["id":"fixture", "name":model, "transport":transport]]]) }
    func send(_ request: [String:Any]) {
        if request["op"] as? String == "close" { packet?(["type":"closed", "bluetooth_closed":confirmClose, "error":""]); return }
        if request["op"] as? String == "select" {
            packet?(["type":"selected", "id":"fixture"])
            if transport == "ble" { packet?(["type":"ready", "transport":"ble", "service":badProfile ? "FFFF" : "18F0", "write":"2AF1", "notify":"2AF0", "write_type":"withResponse"])}
            else { packet?(["type":"ready", "transport":"macos-classic-spp", "uuid":MacSPPTransport.uuid, "device_name":model]) }
            return
        }
        guard let command=request["command"] as? String, allowed(command) else { packet?(["type":"error", "message":"Fixture rejected a command outside the shared effect"]); return }
        sent.append(command); packet?(["type":"write_completed", "command":command])
        if disconnect && command == "021A900000000000" { packet?(["type":"closed", "bluetooth_closed":true, "error":"Synthetic disconnect"]); return }
        let reply: String
        switch command {
        case "ATZ": reply="ELM327"
        case "VTI": reply=model == "Carista EVO" ? "Carista EVO v2.2.84" : "vLinker MC+ v2.2.92"
        case "VTVERS": reply=firmware.isEmpty ? (model == "Carista EVO" ? "MIC3413 V2.0.84" : "MIC3313 V2.0.92") : firmware
        case "ATRV": reply="12.0V"
        case "VTPRON": reply="6 [AT]"
        case "VTPBRD": reply="500000"
        case "021A900000000000": reply="7E8 10 13 5A 90 59 53 33 46\r7E8 21 44 34 39 59 58 34 31\r7E8 22 30 30 30 30 30 31 00"
        case "0210020000000000": reply="7E8 01 50 00 00 00 00 00 00"
        case "0120000000000000": reply="7E8 01 60 00 00 00 00 00 00"
        case "03A9811200000000": reply="7E8 03 7F A9 78 00 00 00 00\r5E8 81 01 07 00 6F 00 00 00\r5E8 81 00 00 00 FF 00 00 00"
        default:
            if command.hasPrefix("021A"),command.count == 16 {
                let index=command.index(command.startIndex,offsetBy:4), end=command.index(index,offsetBy:2)
                let pid=String(command[index..<end]); reply=pid == "95" ? "7E8 04 5A 95 00 0B 00 00 00" : "7E8 03 5A \(pid) 41 00 00 00 00"
            } else { reply="OK" }
        }
        let bytes=Array((reply+"\r>").utf8)
        packet?(["type":"rx", "bytes":Array(bytes.prefix(2))]); packet?(["type":"rx", "bytes":Array(bytes.dropFirst(2))])
    }
    func shutdown() { packet=nil }
}

enum MacBluetoothChecks {
    static func live(binary: URL, mode: String, endpoint: String, root: URL) throws {
        guard ["identity","vin","ecm_info","ecm_dtc"].contains(mode),
              root.path.contains("bluetooth-live-test"), !root.path.contains("Application Support/OpenSAAB") else {
            throw ShellError.message("Live Bluetooth checks require an explicit supported read and an isolated bluetooth-live-test directory")
        }
        func normalized(_ value: String) -> String {value.lowercased().replacingOccurrences(of:"-",with:":")}
        var result: [String:Any]?, selected = false
        let job=MacBluetoothWorkflow(root:root,binary:binary,mode:mode)
        job.onDevices={ [weak job] items in
            guard !selected, let id=items.compactMap({$0["id"]}).first(where:{normalized($0)==normalized(endpoint)}) else {return}
            selected=true;job?.select(id)
        }
        job.onComplete={result=$0};try job.start()
        let deadline=Date().addingTimeInterval(45)
        while result == nil && Date()<deadline {RunLoop.current.run(until:Date().addingTimeInterval(0.05))}
        if result == nil {job.stop()}
        let releaseDeadline=Date().addingTimeInterval(8)
        while result == nil && Date()<releaseDeadline {RunLoop.current.run(until:Date().addingTimeInterval(0.05))}
        guard let report=result else {throw ShellError.message("Live Bluetooth release remains unconfirmed")}
        try fm.createDirectory(at:root,withIntermediateDirectories:true,attributes:[.posixPermissions:0o700])
        try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:root.appendingPathComponent("live-result.json"),options:.atomic)
        guard ["vin_received","identified"].contains(report["status"] as? String ?? ""),report["bluetooth_closed"] as? Bool == true,report["storage_error"] == nil else {
            throw ShellError.message(report["message"] as? String ?? "Live Bluetooth read failed")
        }
        print("PASS native \(mode) · explicitly selected \(endpoint.hasPrefix("ble:") ? "BLE" : "Classic SPP") · confirmed release · private receipt saved")
    }
    static func run(binary: URL, reports: URL, root: URL) throws {
        try ShellChecks.need(AppleBLEProfile.advertised("Carista EVO") == .evo && AppleBLEProfile.advertised("vLinker MC-IOS") == .mc && AppleBLEProfile.advertised("vLinker FD") == nil,"BLE names select known transport hints; unrelated models rejected")
        for model in ["Carista EVO","vLinker MC"] {
            for transport in ["ble","classic"] {
                for mode in ["vin","ecm_info","ecm_dtc"] {
                    let directory=root.appendingPathComponent(UUID().uuidString)
                    var link: FixtureMacBluetooth!, result: [String:Any]?
                    let job=MacBluetoothWorkflow(root:directory,binary:binary,mode:mode,linkFactory:{ allowed in link=FixtureMacBluetooth(model:model,transport:transport,allowed:allowed);return link })
                    job.onComplete={result=$0}; try job.start()
                    try ShellChecks.need(link.opened && link.sent.isEmpty,"\(model) \(transport) \(mode): discovery sends no command before selection")
                    job.select("fixture")
                    guard let report=result else {throw ShellError.message("Synthetic Bluetooth workflow did not finish")}
                    let observed=report["result"] as? [String:Any] ?? [:]
                    try ShellChecks.need(report["status"] as? String == "vin_received" && report["bluetooth_closed"] as? Bool == true && report["storage_error"] == nil,
                        "\(model) \(transport) \(mode): fresh VIN and confirmed release")
                    try ShellChecks.need(observed["adapter_model"] as? String == (model == "Carista EVO" ? "carista_evo" : "vlinker_mc_plus") && link.sent.filter{$0=="021A900000000000"}.count==1 && link.sent.last=="VTPC","\(model) \(transport) \(mode): shared model identification and one VIN request")
                    if mode != "vin" {
                        let listed=try SecurityWorker.call(reports,["op":"list","root":directory.path])["reports"] as? [[String:Any]] ?? []
                        guard let id=listed.first?["id"] as? String else {throw ShellError.message("Synthetic HS-CAN report missing")}
                        let saved=try SecurityWorker.call(reports,["op":"read","root":directory.path,"id":id])["text"] as? String ?? ""
                        try ShellChecks.need(saved.contains(mode == "ecm_dtc" ? "P0107 · failure 00 · status 6F" : "Seven ECM identifiers"),"\(model) \(transport) \(mode): saved report reopens through packaged shared worker")
                    }
                }
            }
        }
        for failure in ["cancel","overlap","profile","firmware","disconnect","release"] {
            var link: FixtureMacBluetooth!, result: [String:Any]?
            let directory=root.appendingPathComponent(failure)
            let job=MacBluetoothWorkflow(root:directory,binary:binary,mode:"ecm_dtc",linkFactory:{allowed in link=FixtureMacBluetooth(model:"Carista EVO",transport:"ble",allowed:allowed);return link})
            job.onComplete={result=$0};try job.start()
            switch failure {
            case "cancel":job.stop()
            case "overlap":
                var rejected=false
                do {try MacBluetoothWorkflow(root:directory,binary:binary).start()} catch {rejected=true}
                try ShellChecks.need(rejected && link.sent.isEmpty,"Native Bluetooth owner rejects overlapping startup/probe before I/O");job.stop()
            case "profile":link.badProfile=true;job.select("fixture")
            case "firmware":link.firmware="MIC3413 V2.1.0";job.select("fixture")
            case "disconnect":link.disconnect=true;job.select("fixture")
            default:link.confirmClose=false;job.select("fixture")
            }
            try ShellChecks.need(result?["status"] as? String == "failed" && (result?["result"] as? [String:Any])?.isEmpty == true && !fm.fileExists(atPath:directory.appendingPathComponent("dtc-reports").path),"\(failure): no fresh vehicle or completed DTC report from a failed native read")
            if ["cancel","overlap","profile","firmware"].contains(failure) {try ShellChecks.need(!link.sent.contains("021A900000000000"),"\(failure): no vehicle request")}
        }
        for cancel in [false,true] {
            let directory=root.appendingPathComponent(cancel ? "startup-cancel" : "startup")
            var link: FixtureMacBluetooth!, result: [String:Any]?, vehicle: [String:Any]?, lookups=0
            let job=MacBluetoothWorkflow(root:directory,binary:binary,mode:"startup",linkFactory:{allowed in link=FixtureMacBluetooth(model:"Carista EVO",transport:"ble",allowed:allowed);return link},lookupEffect:{_,limit,timeout,completion in
                lookups += 1
                let payload: [String:Any] = ["vin":"YS3FD49YX41000001","success":true,"vehicle":["model_year":2004,"platform":"Saab 9-3 Sport Sedan","engine":"B207R","color":["english":"Silver Metallic"],"gearbox":"FA57","body_display":"Sedan"]]
                completion(try! JSONSerialization.data(withJSONObject:payload),200)
            })
            job.onComplete={result=$0};job.onVehicle={vehicle=$0};try job.start()
            if cancel {job.stop()} else {job.select("fixture")}
            if cancel {
                try ShellChecks.need(result?["status"] as? String == "cancelled" && lookups == 0 && link.sent.isEmpty,"Startup picker cancellation releases native and shared ownership without a vehicle read")
            } else {
                try ShellChecks.need(result?["status"] as? String == "identified" && lookups == 1 && vehicle?["vehicle_connected"] as? Bool == false,"Native startup publishes fresh identity after release and shared lookup; firmware transport remains unavailable")
                let identity=vehicle?["vehicle"] as? [String:Any] ?? [:]
                try ShellChecks.need(identity["adapter_label"] as? String == "Carista EVO" && identity["adapter_voltage"] as? String == "12.0V" && identity["gearbox"] as? String == "FA57","Native startup retains shared adapter, voltage and descriptive vehicle fields")
            }
            let paths=(try? fm.contentsOfDirectory(at:directory.appendingPathComponent("connection-attempts"),includingPropertiesForKeys:nil)) ?? []
            guard let path=paths.first else {throw ShellError.message("Connection attempt receipt missing")}
            let receipt=try JSONSerialization.jsonObject(with:Data(contentsOf:path)) as? [String:Any] ?? [:]
            try ShellChecks.need(receipt["outcome"] as? String != "IN_PROGRESS","Startup records a terminal shared connection outcome")
        }
    }
}
