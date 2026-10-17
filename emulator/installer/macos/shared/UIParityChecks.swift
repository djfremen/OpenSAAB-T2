// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

enum UIParityChecks {
  static func run(contract:URL,worker:URL) throws {
    // Synthetic display fixtures only: no original firmware, transport, HTTP,
    // credentials, installed Intel app or actual security-access evidence.
    let profiles = try USBAdapterDiscovery.profiles(contract)
    let specification = try JSONSerialization.jsonObject(with:Data(contentsOf:contract)) as! [String:Any]
    let menus = specification["menus"] as! [String:Any]
    let advanced = menus["advanced"] as! [[String:Any]]
    try ShellChecks.need(!advanced.contains(where:{$0["id"] as? String == "password"}),"contract retires operator-password menu rather than treating its absence as a parity gap")
    func candidate(_ vendor:Int?,_ product:Int?) -> USBAdapterCandidate {
      USBAdapterDiscovery.candidate(port:"/dev/cu.usbmodemSynthetic",vendor:vendor,product:product,profiles:profiles)!
    }
    let nano = candidate(0x1a86,0x55d3),chipsoft = candidate(0x0483,0x5740)
    try ShellChecks.need(nano.profile?.kind == .nano && chipsoft.profile?.kind == .chipsoft,"contract USB VID/PID candidates route to exact model binding")
    try ShellChecks.need(nano.supports(readCodes:false) && !nano.supports(readCodes:true),"Nano Connect and start allowed; separate Trionic8 shortcut excluded")
    try ShellChecks.need(chipsoft.supports(readCodes:false) && chipsoft.supports(readCodes:true),"Chipsoft original menus and engine report routes preserved")
    try ShellChecks.need(candidate(nil,nil).profile == nil && !candidate(0x0483,0x55d3).supports(readCodes:false),"unknown or crossed USB IDs never guess an adapter model")
    for port in ["/dev/cu.usbserialSynthetic","/tmp/cu.usbmodemSynthetic","/dev/../tmp/cu.usbmodemSynthetic"] {
      try ShellChecks.need(USBAdapterDiscovery.candidate(port:port,vendor:0x1a86,product:0x55d3,profiles:profiles) == nil,"USB candidate rejects unrelated or noncanonical port " + port)
    }
    let root = fm.temporaryDirectory.appendingPathComponent("synthetic-ui-history-" + UUID().uuidString)
    defer {try? fm.removeItem(at:root)}
    let run = root.appendingPathComponent("runs/current"),card=root.appendingPathComponent("firmware/test.bin")
    try fm.createDirectory(at:run,withIntermediateDirectories:true)
    try fm.createDirectory(at:card.deletingLastPathComponent(),withIntermediateDirectories:true)
    let original=Data(repeating:255,count:32*1024*1024)
    try original.write(to:card);try original.write(to:run.appendingPathComponent("card.bin"))
    let vin="YS3FD49YX41000001",connection="2026-10-08T17:00:00Z",imported="2026-10-09T16:01:00Z"
    func json(_ file:URL,_ value:[String:Any]) throws {try JSONSerialization.data(withJSONObject:value).write(to:file)}
    try json(root.appendingPathComponent("vehicle-status.json"),["vin":vin,"observed_utc":connection,"adapter":"Synthetic Nano","model_year":2004])
    try json(root.appendingPathComponent("latest-session.json"),["firmware":"test","run":run.path])
    var post=Data(repeating:255,count:714)
    post.replaceSubrange(0x14..<0x25,with:Data(vin.utf8));post[0x30]=0x12;post[0x31]=0x34
    post.replaceSubrange(0x26..<0x2e,with:Data(repeating:1,count:8))
    try post.write(to:run.appendingPathComponent("working-ssa.bin"))
    let receipt="schema=1\nvehicle_hash="+SecurityStatus.hash(Data(vin.utf8))+"\nstage=imported\nstarted_utc=2026-10-09T15:00:00Z\nprocessed_utc=2026-10-09T16:00:00Z\nimported_utc="+imported+"\nssa_hash="+SecurityStatus.hash(post)+"\n"
    try Data(receipt.utf8).write(to:root.appendingPathComponent("security-processing-status.properties"))
    var request:[String:Any]=["op":"status","state":root.path,"firmware":"test","card":card.path,"now_utc":"2026-10-09T19:00:00Z"]
    func view(_ r:[String:Any]) throws -> VehicleStatusView {try VehicleStatusView(SecurityWorker.call(worker,r),firmware:"test")}
    let before=SecurityStatus.hash(post),saved=try view(request)
    try ShellChecks.need(saved.summary == "Security: [POST-AUTH] · Fresh" && saved.value["auth_status"] as? String == "auth_status: [POST-AUTH] · Fresh","real shared worker distinguishes compact Security and detailed auth_status")
    try ShellChecks.need(saved.value["status_utc"] as? String == imported && saved.value["connection_observed_utc"] as? String == connection,"binding retains original import and connection timestamps")
    try ShellChecks.need(saved.value["source_kind"] as? String == "saved-working" && saved.value["source_card_firmware_id"] as? String == "test","reopened saved working SSA retains selected firmware identity")
    try ShellChecks.need(saved.details.contains(vin) && saved.details.contains("Synthetic Nano") && saved.details.contains("Model year: 2004"),"details render only actual stored vehicle metadata")
    request["run"]=run.path
    let active=try view(request)
    try ShellChecks.need(active.value["source_kind"] as? String == "active-working" && active.value["status_utc"] as? String == imported,"active status preserves original timestamp")
    request["now_utc"]="2026-10-09T19:01:00Z"
    let older=try view(request)
    try ShellChecks.need(older.summary == "Security: [POST-AUTH] · Stale" && older.value["status_utc"] as? String == imported,"polling cannot rejuvenate Fresh at three-hour boundary")
    request["now_utc"]="2026-10-09T16:00:00Z"
    try ShellChecks.need(try view(request).value["freshness"] as? String == "Age unknown","future receipt never becomes Fresh")
    request["offline"]=true
    try ShellChecks.need(try view(request).summary == "Security: [N/A]","offline operation never borrows saved authorization")
    request["offline"]=false;request["now_utc"]="2026-10-09T19:00:00Z"
    try Data([255]).write(to:run.appendingPathComponent("working-ssa.bin"))
    try ShellChecks.need(try view(request).summary == "Security: [N/A]","malformed preferred working SSA stays unavailable rather than fallback")
    try post.write(to:run.appendingPathComponent("working-ssa.bin"))
    try ShellChecks.need(SecurityStatus.hash(try Data(contentsOf:run.appendingPathComponent("working-ssa.bin"))) == before,"display call does not change SSA bytes")
    for (key,value) in [("vehicle_access_verified",true),("history_only",false),("display_only",false),("source_card_firmware_id","other"),("state","AUTHORIZED"),("color","invalid"),("workflow_summary","ECU authorized")] as [(String,Any)] {
      var broken=saved.value;broken[key]=value;var rejected=false
      do {_ = try VehicleStatusView(broken,firmware:"test")}catch{rejected=true}
      try ShellChecks.need(rejected,"Swift rejects changed shared display invariant " + key)
    }
    let lines=VehicleStatusView.lines(status:"Ready\nSynthetic",vehicle:"VIN\nObserved",offline:false,hasVehicle:true,snapshot:saved)
    try ShellChecks.need(lines.components(separatedBy:"\n").count == 3 && lines.contains("Ready · Synthetic"),"compact summary stays three lines with identification, vehicle and security")
    try ShellChecks.need(!VehicleStatusView.lines(status:"Offline",vehicle:nil,offline:true,hasVehicle:false,snapshot:saved).contains("POST-AUTH"),"offline compact summary excludes saved security claim")
    try ShellChecks.need(VehicleStatusView.lines(status:"Ready",vehicle:"No vehicle connected",offline:false,hasVehicle:false,snapshot:nil) == "Ready\nNo vehicle connected","Android ready without history retains two lines and no invented security row")
    try ShellChecks.need(saved.details.components(separatedBy:"auth_status:").count == 2 && !saved.details.contains(imported),"details render shared auth_status once and localize its original timestamp once")
    try ShellChecks.need(VehicleStatusView.localTime(imported) != "Date unknown" && VehicleStatusView.localTime("bad") == "Date unknown","native localization formats stored dates without inventing them")
    print("UI parity binding tests passed; all inputs synthetic; no hardware, API or installed-app qualification.")
  }
}
