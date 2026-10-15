// SPDX-License-Identifier: MPL-2.0
import Foundation
import AppKit

enum ShellChecks {
  static func privateAutomationModal(catalog: URL) throws {
    // Actual AppKit nested modal loops and the production private driver, with
    // synthetic prompts only. No runtime, adapter, download, API or credential.
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let root = fm.temporaryDirectory.appendingPathComponent("private-modal-" + UUID().uuidString)
    defer { try? fm.removeItem(at: root) }
    let shell = try Shell(state: root.appendingPathComponent("state"), binaries: root, catalog: catalog)
    shell.window = NSWindow(contentRect: NSRect(x:0,y:0,width:480,height:160),
      styleMask:[.titled],backing:.buffered,defer:false)
    shell.window.isReleasedWhenClosed = false
    shell.status.frame = NSRect(x:12,y:12,width:450,height:120)
    shell.window.contentView!.addSubview(shell.status)
    let directory = root.appendingPathComponent("operator")
    let driver = try PrivateAutomation(shell:shell,directory:directory)
    shell.privateAutomation = driver
    driver.start(); driver.start() // Starting twice must not create duplicate timers.
    let failureText = "Synthetic adapter failure: no live session or ignition action."
    var phase = 0, requestID = "", done = false, fatalEntered = false, fatalReturned = false
    var failure: Error?, watchdogUsed = false
    func object(_ name:String) -> [String:Any] {
      guard let bytes = try? Data(contentsOf:directory.appendingPathComponent(name)),
        let value = try? JSONSerialization.jsonObject(with:bytes) as? [String:Any] else { return [:] }
      return value
    }
    func send(_ op:String,_ fields:[String:Any] = [:]) throws {
      requestID = UUID().uuidString
      var value = fields; value["id"] = requestID; value["op"] = op
      try JSONSerialization.data(withJSONObject:value).write(to:directory.appendingPathComponent("command.json"),options:.atomic)
    }
    func response(_ status:String) -> Bool {
      let value = object("response.json")
      return value["id"] as? String == requestID && value["status"] as? String == status
    }
    func schedule(_ interval:TimeInterval,_ action:@escaping () -> Void) -> Timer {
      let timer = Timer(timeInterval:interval,repeats:true) { _ in action() }
      RunLoop.main.add(timer,forMode:.common); RunLoop.main.add(timer,forMode:.modalPanel)
      return timer
    }
    let sessionTimer = schedule(0.03) {
      guard !fatalEntered else { return }
      fatalEntered = true
      shell.sessionFailure(failureText)
      fatalReturned = true
    }
    let observer = schedule(0.02) {
      guard failure == nil && !done else { return }
      do {
        let state = object("state.json")
        switch phase {
        case 0:
          guard app.modalWindow != nil,state["status"] as? String == failureText else { return }
          try need(state["session_running"] as? Bool == false && state["key_queue_available"] as? Bool == false,
            "fatal modal publishes fresh ended-session state independently of blocked session timer")
          try send("status"); phase = 1
        case 1:
          guard response("invoked") else { return }
          try need(!fatalReturned,"status command admitted while actual fatal NSAlert blocks session timer")
          try send("key",["code":16]); phase = 2
        case 2:
          guard response("rejected") else { return }
          try need(app.modalWindow != nil,"fatal alert rejects firmware keys and remains open")
          try send("alert",["button":"Continue"]); phase = 3
        case 3:
          guard response("rejected") else { return }
          try need(app.modalWindow != nil,"unrequested or absent alert label cannot dismiss fatal alert")
          try send("snapshot"); phase = 4
        case 4:
          guard response("invoked"),fm.fileExists(atPath:directory.appendingPathComponent("alert.png").path) else { return }
          try need((try? regularSize(directory.appendingPathComponent("window.png"))) ?? 0 > 0,
            "actual own-view snapshot remains available inside fatal modal")
          try send("alert",["button":"OK"]); phase = 5
        case 5:
          guard response("invoked"),fatalReturned else { return }
          try need(app.modalWindow == nil,"explicit matching OK action closes actual fatal alert without watchdog")
          try send("security"); phase = 6
        case 6:
          guard response("invoked"),app.modalWindow != nil,
            state["operator_action_pending"] as? Bool == true else { return }
          try need((state["alert_labels"] as? [String] ?? []).contains(where:{$0.contains("No collected security data")}),
            "deferred real security action preserves missing-data gate and opens its actual alert")
          try send("stop"); phase = 7
        case 7:
          guard response("rejected") else { return }
          try need(app.modalWindow != nil,"second mutating operator action rejected while first action waits in modal")
          try send("status"); phase = 8
        case 8:
          guard response("invoked") else { return }
          try need(state["operator_action_pending"] as? Bool == true,"status command admitted inside command-originated modal")
          try send("alert",["button":"OK"]); phase = 9
        case 9:
          guard response("invoked"),app.modalWindow == nil,
            state["operator_action_pending"] as? Bool == false else { return }
          try need(shell.session == nil && shell.securityJob == nil,"explicit alert response completes action without starting firmware or security API")
          try send("key",["code":16]); phase = 10
        case 10:
          guard response("rejected") else { return }
          try need(shell.session == nil,"ended session cannot receive firmware keys after modal dismissal")
          done = true
        default: break
        }
      } catch { failure = error; app.abortModal() }
    }
    let watchdog = schedule(6) {
      guard !done else { return }
      watchdogUsed = true
      failure = ShellError.message("Private modal fixture deadline at phase \(phase)")
      app.abortModal(); app.modalWindow?.orderOut(nil)
    }
    defer { sessionTimer.invalidate(); observer.invalidate(); watchdog.invalidate(); shell.window.close() }
    let deadline = Date().addingTimeInterval(8)
    while !done && failure == nil && Date()<deadline { _ = RunLoop.main.run(mode:.default,before:deadline) }
    if let failure = failure { throw failure }
    try need(done && !watchdogUsed,"production private driver passes both actual modal origins within bounded fixture")
    print("Private automation modal tests passed; no hardware or Intel installed-app qualification.")
  }
  static func nanoBinding() throws {
    let vin = "YS3FD49Y941234567"
    var good: [String:Any] = ["status":"hs_vin_received", "hardware":"VCX-NANO", "firmware":"1.9.4.2",
      "channel_cleanup":true, "usb_closed":true, "cleanup_errors":[String](), "error":NSNull(),
      "startup_complete":true, "query_record_verified":true, "installed_record_verified":true,
      "fresh_dh_exchanges":2, "vehicle_tx":3, "gm_tester_present_tx":1,
      "vin_profile":"saab9440-hs-tester-present-1", "vin":vin]
    try need(try NanoProbeResult(value:good,exitCode:0,vinRequired:true).vin == vin, "fresh Nano startup/VIN boundary")
    for field in ["channel_cleanup","usb_closed","startup_complete","query_record_verified","installed_record_verified"] {
      var bad = good; bad[field] = false
      var rejected = false
      do { _ = try NanoProbeResult(value:bad,exitCode:0,vinRequired:true) } catch { rejected = true }
      try need(rejected,"Nano rejects unconfirmed " + field)
    }
    for (field,value) in [("hardware","CH343"),("firmware","1.9.4.3"),("vin","YS3FD49Y94I234567"),("vin_profile","basic-1a90")] {
      var bad = good; bad[field] = value
      var rejected = false
      do { _ = try NanoProbeResult(value:bad,exitCode:0,vinRequired:true) } catch { rejected = true }
      try need(rejected,"Nano rejects unrelated " + field)
    }
    for (field,value) in [("fresh_dh_exchanges",1),("vehicle_tx",2),("vehicle_tx",4),("gm_tester_present_tx",0),("gm_tester_present_tx",2)] {
      var bad = good; bad[field] = value
      var rejected = false
      do { _ = try NanoProbeResult(value:bad,exitCode:0,vinRequired:true) } catch { rejected = true }
      try need(rejected,"Nano rejects incomplete/extra " + field)
    }
    var rejected = false
    do { _ = try NanoProbeResult(value:good,exitCode:1,vinRequired:true) } catch { rejected = true }
    try need(rejected,"failed Nano helper cannot establish VIN")
    good["status"] = "identified"; good["vehicle_tx"] = 0; good["gm_tester_present_tx"] = 0; good["startup_complete"] = false; good["vin"] = NSNull()
    try need(try NanoProbeResult(value:good,exitCode:0,vinRequired:false).vin == nil,"post-import identity check sends no vehicle request")
    good["gm_tester_present_tx"] = 1; rejected = false
    do { _ = try NanoProbeResult(value:good,exitCode:0,vinRequired:false) } catch { rejected = true }
    try need(rejected,"identity-only check rejects tester-present activity")
    good["gm_tester_present_tx"] = 0
    good["vehicle_tx"] = 1; rejected = false
    do { _ = try NanoProbeResult(value:good,exitCode:0,vinRequired:false) } catch { rejected = true }
    try need(rejected,"identity-only check rejects vehicle activity")
    let cleanup: [String:Any] = ["schema":1,"attempted_channels":[0,1],"cleanup_request_count":6,
      "cleanup_zero_status_reply_count":6,"startup_complete":true,"serial_closed":true,"cleanup_confirmed":true,
      "query_record_verified":true,"installed_record_verified":true,"fresh_dh_exchanges":2]
    try need(NanoCleanup.confirmed(cleanup),"Nano six actual shutdown replies plus CDC close")
    for (field,value) in [("cleanup_zero_status_reply_count",5),("cleanup_request_count",0)] {
      var bad = cleanup; bad[field] = value
      try need(!NanoCleanup.confirmed(bad),"Nano blocks continuation after incomplete " + field)
    }
    for field in ["cleanup_confirmed","serial_closed","startup_complete","query_record_verified","installed_record_verified"] {
      var bad = cleanup; bad[field] = false
      try need(!NanoCleanup.confirmed(bad),"Nano blocks continuation without " + field)
    }
    try need(!NanoCleanup.confirmed([:]),"missing Nano teardown receipt blocks processing")
    let run = fm.temporaryDirectory.appendingPathComponent("nano-binding-" + UUID().uuidString)
    try fm.createDirectory(at:run,withIntermediateDirectories:true)
    defer {try? fm.removeItem(at:run)}
    let endpoint = USBAdapterEndpoint(kind:.nano,port:"/dev/cu.usbmodem-fixture",identity:"VCX-NANO / 1.9.4.2")
    try RunAdapterBinding.write(endpoint,to:run)
    try need(try RunAdapterBinding.read(run:run) == endpoint,"per-run endpoint survives app relaunch")
    try need(try NanoCleanup.requiredForSecurity(run:run,adapter:nil),"reopened Nano collection requires actual cleanup")
    try need(!NanoCleanup.confirmed(run:run),"reopened Nano collection cannot bypass missing cleanup")
    rejected=false
    do {_ = try NanoCleanup.requiredForSecurity(run:run,adapter:USBAdapterEndpoint(kind:.nano,port:"/dev/cu.usbmodem-other",identity:endpoint.identity))}catch{rejected=true}
    try need(rejected,"changed endpoint cannot process collection")
    try Data("invalid".utf8).write(to:run.appendingPathComponent("adapter-binding.json"))
    rejected=false
    do {_ = try NanoCleanup.requiredForSecurity(run:run,adapter:nil)}catch{rejected=true}
    try need(rejected,"malformed per-run provenance blocks security processing")
    try need(EmulatorSession.gracefulStopBudget(for:.nano)==45 && EmulatorSession.gracefulStopBudget(for:.chipsoft)==12,"Nano bounded startup plus shutdown stop budget; Chipsoft unchanged")
    let graceful = Process(); graceful.executableURL=URL(fileURLWithPath:"/usr/bin/true")
    try graceful.run(); graceful.waitUntilExit()
    let incomplete = SessionStopOutcome(forcedProcessExit:false,protocolCleanupConfirmed:false)
    try need(graceful.terminationStatus==0 && incomplete.sessionEndReason=="protocol_cleanup_failed" && !incomplete.continuationSafe,
      "normal process exit with failed protocol cleanup is reported without claiming forced exit or continuation")
    let forced = Process(); forced.executableURL=URL(fileURLWithPath:"/bin/sleep"); forced.arguments=["20"]
    try forced.run(); let forceRequired=forced.isRunning; forced.terminate()
    let forceDeadline=Date().addingTimeInterval(2)
    while forced.isRunning && Date()<forceDeadline {Thread.sleep(forTimeInterval:0.01)}
    if forced.isRunning {kill(forced.processIdentifier,SIGKILL)}
    forced.waitUntilExit()
    let forcedOutcome = SessionStopOutcome(forcedProcessExit:forceRequired,protocolCleanupConfirmed:true)
    try need(forceRequired && forced.terminationReason == .uncaughtSignal && forcedOutcome.sessionEndReason=="forced_stop" && !forcedOutcome.continuationSafe,
      "actual terminated process stays forced-stop even when protocol cleanup is confirmed")
    let complete = SessionStopOutcome(forcedProcessExit:false,protocolCleanupConfirmed:true)
    try need(complete.sessionEndReason=="user_stop" && complete.continuationSafe,"graceful process plus confirmed protocol cleanup retains normal continuation")
    for mode in ChipsoftSessionMode.allCases {
      let flags = mode.arguments(for:.nano)
      try need(flags == (mode == .seeds ? ["--candi-nano-seeds"] : mode == .fullNative ? ["--candi-nano-add-remove"] : []),"Nano " + mode.rawValue + " explicit permissions")
      try need(mode.arguments(for:.chipsoft) == mode.arguments,"Chipsoft " + mode.rawValue + " flags preserved")
    }
    let command: [String:Any] = ["id":UUID().uuidString,"op":"key","code":16]
    try need(try PrivateAutomation.command(JSONSerialization.data(withJSONObject:command))["op"] as? String == "key","operator key command admitted")
    for key in ["password","credential","secret"] {
      var bad = command; bad[key] = "synthetic"
      rejected = false
      do { _ = try PrivateAutomation.command(JSONSerialization.data(withJSONObject:bad)) } catch { rejected = true }
      try need(rejected,"operator command refuses persisted " + key)
    }
    print("Nano binding tests passed; no hardware or installed workflow qualification.")
  }
  static func need(_ condition: Bool, _ message: String) throws {
    guard condition else { throw ShellError.message("FAIL " + message) }
    print("PASS " + message)
  }
  static func unit(catalog: URL) throws {
    try nanoBinding()
    let temp = fm.temporaryDirectory.appendingPathComponent("opensaab-check-" + UUID().uuidString)
    defer { try? fm.removeItem(at: temp) }
    let store = try FirmwareStore(state: temp, catalog: catalog)
    try need(
      store.entries.count == 11 && store.entries.filter { $0.runnable }.count == 10,
      "shared catalog")
    try need(store.selected == nil, "empty setup has no invented selection")
    var rejected = false
    do { try store.validateCore() } catch { rejected = true }
    try need(rejected, "missing runtime rejected")
    rejected = false
    do { try store.activate(store.entries.first { !$0.runnable }!) } catch { rejected = true }
    try need(rejected && store.selected == nil, "unsupported image preserves selection")
    try Data("corrupt".utf8).write(to: store.card(store.entries[0]))
    rejected = false
    do { try store.activate(store.entries[0]) } catch { rejected = true }
    try need(rejected && store.selected == nil, "corrupt image rejected")
    rejected = false
    do { _ = try decodeFrame(Data("P6\n".utf8)) } catch { rejected = true }
    try need(rejected, "torn framebuffer rejected")
    var rgb = Data("P6\n320 240\n255\n".utf8)
    rgb.append(Data(repeating: 0, count: 320 * 240 * 3))
    let frame = try decodeFrame(rgb)
    try need(frame.width == 320 && frame.height == 240, "320x240 framebuffer")
    let result = try runTool(URL(fileURLWithPath: "/usr/bin/printf"), ["probe-output"], timeout: 2)
    try need(result.code == 0 && result.text == "probe-output", "bounded helper output captured")
    // Adapter-stop classification is shared Rust (adapter_failure.rs, with its own tests);
    // the shell only presents report.json. Check that binding.
    let failureRun = temp.appendingPathComponent("failure-run")
    try fm.createDirectory(at: failureRun, withIntermediateDirectories: true)
    try need(AdapterFailure.read(run: failureRun) == nil, "missing report has no adapter failure")
    try Data("{\"status\":\"incomplete\",\"adapter_failure\":null}".utf8).write(to: failureRun.appendingPathComponent("report.json"))
    try need(AdapterFailure.read(run: failureRun) == nil, "report without adapter failure is a plain session end")
    let wakeReport = "{\"status\":\"incomplete\",\"adapter_failure\":{\"schema\":1,\"kind\":\"single_wire_wake_failed\",\"title\":\"Single-wire CAN wake-up failed · Check pin 1\",\"message\":\"Check the single-wire CAN connection at diagnostic pin 1.\",\"ecu_rejected\":false}}"
    try Data(wakeReport.utf8).write(to: failureRun.appendingPathComponent("report.json"))
    let wake = AdapterFailure.read(run: failureRun)
    try need(wake?.kind == "single_wire_wake_failed" && wake?.message.contains("pin 1") == true
      && wake?.sessionEndReason == "wake_rejected" && wake?.connectionReason == "TIMEOUT",
      "shared single-wire wake classification presented from report.json")
    try Data("{\"adapter_failure\":{\"kind\":\"command_blocked\",\"title\":\"t\",\"message\":\"m\"}}".utf8).write(to: failureRun.appendingPathComponent("report.json"))
    try need(AdapterFailure.read(run: failureRun)?.sessionEndReason == "unsupported_command", "command guard stop maps to unsupported command")
    try Data("not json".utf8).write(to: failureRun.appendingPathComponent("report.json"))
    try need(AdapterFailure.read(run: failureRun) == nil, "malformed report is not misreported as an adapter failure")
    for mode in ChipsoftSessionMode.allCases {
      let inherited = ["TECH2_CHIPSOFT_FULL_NATIVE":"1", "TECH2_CHIPSOFT_KEY_STATUS":"1"]
      let env = mode.environment(inherited, connected: true)
      try need(env["TECH2_CHIPSOFT_FULL_NATIVE"] == (mode == .fullNative ? "1" : "0") && env["TECH2_CHIPSOFT_KEY_STATUS"] == "0",
        "selected \(mode.rawValue) overrides inherited policy")
      try need(mode.arguments == (mode == .seeds ? ["--candi-chipsoft-seeds"] : []) && mode.environment(inherited, connected: false)["TECH2_CHIPSOFT_FULL_NATIVE"] == "0",
        "\(mode.rawValue) keeps seeds separate and offline disconnected")
    }
    try MacBluetoothChecks.run(binary:Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/opensaab-connection"),reports:Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/opensaab-reports"),root:temp.appendingPathComponent("bluetooth-checks"))
    try SecurityWorkflowChecks.run()
    try dtcObserver(temp:temp.appendingPathComponent("dtc-check"))
    try startupReport(temp:temp.appendingPathComponent("startup-check"))
    try supportReport(temp:temp.appendingPathComponent("support-check"))
    try menuShortcuts()
    try securityWorkspace(temp: temp.appendingPathComponent("security-check"))
    try bundled(catalog: catalog, temp: temp.appendingPathComponent("bundled-check"))
  }
  static func startupReport(temp:URL) throws {
    let vin = "YS3FD49YX41000001"
    let value:[String:Any] = ["schema":1,"status":"dtc_complete","source":"host-hscan-dtc",
      "vin":vin,"complete":true,"session_stopped":true,"usb_closed":true,"dtcs":[[String:Any]](),
      "text":"Synthetic engine report — no current/history codes reported."]
    func result(_ v:[String:Any], code:Int32 = 0) throws -> ToolResult {
      ToolResult(code:code,text:"CHIPSOFT_STARTUP_RESULT " + String(data:try JSONSerialization.data(withJSONObject:v),encoding:.utf8)! + "\n")
    }
    let report = try EngineStartupReport(result(value),vin:vin)
    try report.save(root:temp)
    let worker=Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/opensaab-reports")
    let listing=try SecurityWorker.call(worker,["op":"list","root":temp.path])
    let reports=listing["reports"] as? [[String:Any]] ?? []
    guard let row=reports.first(where:{($0["id"] as? String)?.hasPrefix("macos_dtc_startup_") == true}) else {
      throw ShellError.message("Saved startup report was not listed")
    }
    let read=try SecurityWorker.call(worker,["op":"read","root":temp.path,"id":row["id"]!])
    try need(read["text"] as? String == report.text,"startup report persists and reopens with passive reports")
    for key in ["complete","session_stopped","usb_closed"] {
      var broken=value;broken[key]=false;var rejected=false
      do {_ = try EngineStartupReport(result(broken),vin:vin)} catch {rejected=true}
      try need(rejected,"startup report rejects missing \(key)")
    }
    var rejected=false
    do {_ = try EngineStartupReport(result(value,code:1),vin:vin)} catch {rejected=true}
    try need(rejected,"startup result from failed process cannot continue")
    rejected=false
    do {_ = try EngineStartupReport(result(value),vin:"YS3FD49YX41000002")} catch {rejected=true}
    try need(rejected,"startup report cannot use an unrelated VIN")
  }
  static func dtcObserver(temp:URL) throws {
    let run=temp.appendingPathComponent("runs/synthetic")
    try fm.createDirectory(at:run,withIntermediateDirectories:true)
    try Data("DTC Information\nACC B0260 06 Synthetic test\n1 / 2\n".utf8).write(to:run.appendingPathComponent("screen.txt"))
    let observer=DtcObserver(),worker=Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/opensaab-reports")
    var outcome:Result<[String:Any],Error>?
    let deadline=Date().addingTimeInterval(10)
    while outcome == nil && Date()<deadline {
      observer.tick(root:temp,session:run,worker:worker) {outcome=$0}
      RunLoop.current.run(until:Date().addingTimeInterval(0.05))
    }
    guard let result=outcome else {throw ShellError.message("DTC observer timed out")}
    let observed=try result.get()
    try need(observed["changed"] as? Bool == true && observed["partial"] as? Bool == true,"passive stable-screen observer saves partial DTC report")
    let listed=try SecurityWorker.call(worker,["op":"list","root":temp.path])
    let reports=listed["reports"] as? [[String:Any]] ?? []
    try need(reports.count==1,"saved DTC report reopens through packaged worker")
    let read=try SecurityWorker.call(worker,["op":"read","root":temp.path,"id":reports[0]["id"]!])
    try need((read["text"] as? String)?.contains("VIN: not captured") == true,"passive report never inherits another session VIN")
  }
  static func menuShortcuts() throws {
    let worker=Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/opensaab-security"),vin="YS3FD49YX41000001"
    let ro=try SecurityWorker.call(worker,["op":"menu_navigate","target":"clear_dtc","vin":vin,"full_control":false])
    try need(ro["rejected"] as? Bool == true,"read-only session rejects Clear DTC shortcut")
    var r=try SecurityWorker.call(worker,["op":"menu_navigate","target":"read_dtc","vin":vin,"full_control":false])
    var keys:[Int]=[],now=1000
    for screen in ["Main Menu\nF2: Diagnostics","Model Year\n2004","Vehicle Type\nSaab 9-3 Sport (9440)","Diagnostics\nF0: Engine\nF4: All",
                   "All\nF0: Diagnostic Trouble Codes (DTC)\nF1: ECU Information","Diagnostic Trouble Codes\nF2: Read DTC\nF5: Clear DTC"] {
      for _ in 0..<2 {
        now+=100
        r=try SecurityWorker.call(worker,["op":"menu_navigate","navigation":r["navigation"]!,"screen":screen,"now_ms":now])
        if let k=r["key"] as? Int {keys.append(k)}
      }
    }
    try need(keys==[0x13,0x10,0x10,0x03,0x18,0x13] && r["active"] as? Bool == false,"Read DTC shortcut follows Android menu keys and ends at the firmware task")
    let offline=try SecurityWorker.call(worker,["op":"menu_navigate","target":"engine_data","vin":""])
    try need(offline["active"] as? Bool == false,"unidentified vehicle leaves menus to the operator")
  }
  static func supportReport(temp:URL) throws {
    let run=temp.appendingPathComponent("runs/synthetic"),vin="YS3FD49YX41000001"
    try fm.createDirectory(at:run,withIntermediateDirectories:true)
    try Data("VIN=\(vin)\nUSB_TX 01\nTIMEOUT\n".utf8).write(to:run.appendingPathComponent("console.log"))
    try Data("{\"adapter\":\"Chipsoft Pro\",\"vin\":\"\(vin)\"}".utf8).write(to:run.appendingPathComponent("dtc-context.json"))
    let attempt=ConnectionAttempt(state:temp);attempt.advance("VIN_REQUEST");attempt.finish("FAILED","VIN_UNAVAILABLE")
    let worker=Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/opensaab-reports")
    let host:[String:Any]=["platform":"macos","arch":shellArchitecture,"version":shellVersion,"os_version":ProcessInfo.processInfo.operatingSystemVersionString]
    let r=try SecurityWorker.call(worker,["op":"support_collect","root":temp.path,"description":"Synthetic \(vin)","host":host])
    let text=r["text"] as? String ?? ""
    try need(text.contains("com.opensaab.desktop") && text.contains("VIN_UNAVAILABLE") && text.contains("\"TIMEOUT\": 1"),"support report collects desktop session and connection summary")
    try need(!text.contains(vin) && !text.contains("usbmodem"),"support report excludes VIN and port names")
    try need(r["endpoint"] as? String == "https://www.opensaab.com/api/support/reports","support report uses the Android upload endpoint")
    let bad=try SecurityWorker.call(worker,["op":"support_receipt","root":temp.path,"status":200,"body":"{\"stored\":true,\"report_id\":\"OS-0123456789abcdef01234567\"}","network":""])
    try need(bad["stored"] as? Bool == false,"non-201 upload is never reported as sent")
  }
  static func securityWorkspace(temp:URL) throws {
    let run=temp.appendingPathComponent("runs/previous"),next=temp.appendingPathComponent("runs/next")
    try fm.createDirectory(at:run,withIntermediateDirectories:true)
    try fm.createDirectory(at:next,withIntermediateDirectories:true)
    let card=run.appendingPathComponent("card.bin"),newCard=next.appendingPathComponent("card.bin")
    var blank=Data(repeating:0xff,count:Int(SecurityStatus.offset)+SecurityStatus.size+8)
    blank[0]=0x42
    try blank.write(to:card);try blank.write(to:newCard)
    let vin="YS3FD49YX41000001"
    var pre=Data(repeating:0xff,count:SecurityStatus.size);pre.replaceSubrange(20..<37,with:vin.utf8)
    let meta:[String:Any]=["origin":"original-guest-memory","bytes":714,"card_offset":Int(SecurityStatus.offset),"ssa_memory_flash_enabled":true]
    try JSONSerialization.data(withJSONObject:meta).write(to:run.appendingPathComponent("native-security-snapshot.json"))
    try pre.write(to:run.appendingPathComponent("ssa-card-after.bin"))
    let receipt=run.appendingPathComponent("security-processing-status.properties")
    try Data("schema=1\nimported_utc=2026-09-23T00:00:00Z\n".utf8).write(to:receipt)
    try need(SecurityStatus.read(card,receipt:receipt,vehicle:vin).state=="PRE_AUTH", "legacy collected snapshot overrides cleared startup card")
    try need(SecurityStatus.read(card,receipt:receipt,vehicle:"YS3FD49YX41000002").state=="UNAVAILABLE", "another vehicle cannot inherit collected status")
    try JSONSerialization.data(withJSONObject:["firmware":"test","run":run.path]).write(to:temp.appendingPathComponent("latest-session.json"))
    try JSONSerialization.data(withJSONObject:["vin":vin]).write(to:temp.appendingPathComponent("vehicle-status.json"))
    try need(try SecurityWorkspace.restore(state:temp,firmware:"test",to:newCard), "matching vehicle restores collected SSA for next session")
    var expected=blank;expected.replaceSubrange(Int(SecurityStatus.offset)..<Int(SecurityStatus.offset)+714,with:pre)
    let origin=try JSONSerialization.jsonObject(with:Data(contentsOf:next.appendingPathComponent("security-collection-origin.json"))) as! [String:String]
    try need(origin["source_session"]==run.path && origin["firmware"]=="test", "reopen preserves security collection provenance")
    try need(try Data(contentsOf:newCard)==expected && Data(contentsOf:card)==blank, "restore changes only new card SSA and preserves old evidence")
    try need(try Data(contentsOf:next.appendingPathComponent(receipt.lastPathComponent))==Data(contentsOf:receipt), "reopen preserves receipt timestamp bytes")
    try need(try !SecurityWorkspace.restore(state:temp,firmware:"other",to:newCard), "different firmware cannot inherit SSA")
    try JSONSerialization.data(withJSONObject:["vin":"YS3FD49YX41000002"]).write(to:temp.appendingPathComponent("vehicle-status.json"))
    try need(try !SecurityWorkspace.restore(state:temp,firmware:"test",to:newCard), "different vehicle cannot restore SSA")
    try Data(repeating:0xff,count:714).write(to:run.appendingPathComponent("working-ssa.bin"))
    try need(SecurityStatus.read(card,receipt:receipt,vehicle:vin).state=="INIT_AUTH", "live reset overrides old collected snapshot")
    try Data([0xff]).write(to:run.appendingPathComponent("working-ssa.bin"))
    try need(SecurityStatus.read(card,receipt:receipt,vehicle:vin).state=="UNAVAILABLE", "damaged live checkpoint never falls back to stale success")
  }
  static func bundled(catalog: URL, temp: URL) throws {
    let resources = catalog.deletingLastPathComponent()
    if !fm.fileExists(atPath: resources.appendingPathComponent("system/eprom.bin").path) { return }
    let store = try FirmwareStore(state: temp, catalog: catalog)
    try need(try BundledRuntime.ensure(resources: resources, store: store) == 3,
      "fresh setup installs all three verified Android support files")
    try store.validateCore()
    var custom = try Data(contentsOf: store.runtime.appendingPathComponent("eprom.bin"))
    custom[custom.count-1] ^= 1
    try custom.write(to: store.runtime.appendingPathComponent("eprom.bin"), options: .atomic)
    try need(try BundledRuntime.ensure(resources: resources, store: store) == 0,
      "valid custom support profile preserved on upgrade")
    try need(try Data(contentsOf: store.runtime.appendingPathComponent("eprom.bin")) == custom,
      "existing component content unchanged")
    try fm.removeItem(at: store.runtime.appendingPathComponent("candi.bin"))
    try Data("interrupted".utf8).write(to: store.runtime.appendingPathComponent("opsys.dwn"))
    try need(try BundledRuntime.ensure(resources: resources, store: store) == 2,
      "missing and interrupted support installation repaired")
    let corruptResources = temp.appendingPathComponent("bad-package")
    try fm.createDirectory(at: corruptResources, withIntermediateDirectories: true)
    try fm.copyItem(at: resources.appendingPathComponent("system"),
                   to: corruptResources.appendingPathComponent("system"))
    let opsys = corruptResources.appendingPathComponent("system/opsys.dwn")
    var data = try Data(contentsOf: opsys)
    data[data.count-1] ^= 1
    try data.write(to: opsys, options: .atomic)
    let empty = try FirmwareStore(state: temp.appendingPathComponent("empty"), catalog: catalog)
    var rejected = false
    do { try BundledRuntime.ensure(resources: corruptResources, store: empty) } catch { rejected = true }
    try need(rejected && !fm.fileExists(atPath: empty.runtime.path),
      "corrupt bundled file rejected before any component installation")
    data[0x12a4a] = 0
    try data.write(to: opsys, options: .atomic)
    rejected = false
    do { try BundledRuntime.validate("opsys.dwn", opsys) } catch { rejected = true }
    try need(rejected, "same-size invalid opsys signature rejected")
  }
  static func wait(_ name: String, _ check: () throws -> Bool) throws {
    let start = Date()
    while Date().timeIntervalSince(start) < 15 {
      if try check() {
        print("PASS \(name) \(Int(Date().timeIntervalSince(start)*1000))ms")
        return
      }
      Thread.sleep(forTimeInterval: 0.02)
    }
    throw ShellError.message("Timeout: " + name)
  }
  // Only accepts a prepared isolated directory; never touches the user's active selection.
  static func runtime(binaries: URL, catalog: URL, state: URL) throws {
    guard fm.fileExists(atPath: state.appendingPathComponent("test-archives").path),
      !state.path.hasSuffix("Application Support/OpenSAAB")
    else { throw ShellError.message("Runtime tests require an isolated prepared directory") }
    let store = try FirmwareStore(state: state, catalog: catalog)
    for language in ["en", "de"] {
      let entry = store.entries.first { $0.runnable && $0.language == language }!
      let archive = state.appendingPathComponent("test-archives/" + entry.id + ".zip")
      try store.acceptArchive(archive, entry: entry)
      let hash = try sha256(store.card(entry))
      let session = try EmulatorSession(
        binary: binaries.appendingPathComponent("tech2-emu"), store: store)
      defer { session.stop() }
      try wait(language + " framebuffer") {
        guard session.running else { throw ShellError.message(session.consoleText) }
        return (try? session.frame()) != nil
      }
      func screen(_ text: String) -> Bool {
        ((try? String(contentsOf: session.run.appendingPathComponent("screen.txt"), encoding: .utf8))
          ?? "").contains(text)
      }
      session.key(16)
      try wait(language + " main menu") {
        try session.pump()
        return screen(language == "en" ? "Main Menu" : "Hauptmen")
      }
      if language == "en" {
        session.key(16)
        try wait("model years") {
          try session.pump()
          return screen("Model Year")
        }
        session.key(12)
        try wait("Down encoder") {
          try session.pump()
          return session.consoleText.contains("encoder=0x0c")
        }
        session.key(1)
        try wait("EXIT returns to menu") {
          try session.pump()
          return screen("Main Menu")
        }
        for code in [10, 6, 7, 8] {
          session.key(code)
          try wait("soft-key \(code) delivered") {
            try session.pump()
            return session.consoleText.contains(String(format: "encoder=0x%02x", code))
          }
        }
      }
      try need(session.stop(), language + " graceful stop")
      try need(try sha256(store.card(entry)) == hash, "original firmware unchanged")
      let before = store.selected?.id
      var cancelled = false
      do { try store.acceptArchive(archive, entry: entry, cancelled: { true }) } catch {
        cancelled = true
      }
      try need(cancelled && store.selected?.id == before, "cancel preserves selection")
    }
    // Start again after both languages stopped, without creating a second owner.
    let reopened = try EmulatorSession(
      binary: binaries.appendingPathComponent("tech2-emu"), store: store)
    try wait("reopen framebuffer") { (try? reopened.frame()) != nil }
    try need(reopened.stop(), "reopened session stops")
  }
}
