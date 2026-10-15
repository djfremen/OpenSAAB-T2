// SPDX-License-Identifier: MPL-2.0
import Foundation

// Thin presentation binding. Shared Rust validates fresh identity, management,
// VIN and the original guest; raw records stay in the private run directory.
enum MdiWorkflow {
  static func validTarget(_ target:String)->Bool {
    let fields=target.split(separator:":",omittingEmptySubsequences:false)
    return fields.count==3 && fields[0]=="mdi-usb" && fields[1].count==8
      && UInt32(fields[1],radix:16) != nil && (UInt64(fields[2]) ?? 0)>0
  }
  static func read(_ run:URL,_ name:String,limit:Int=65536)->[String:Any]? {
    guard let data=try? Data(contentsOf:run.appendingPathComponent(name)),data.count<=limit else{return nil}
    return try? JSONSerialization.jsonObject(with:data) as? [String:Any]
  }
  static func freshVehicle(_ run:URL)->[String:String]? {
    guard let receipt=read(run,"vin-private.json",limit:262144),receipt["status"] as? String=="fresh_vin_received",
      receipt["cleanup_ok"] as? Bool==true,let context=read(run,"dtc-context.json"),
      let vin=context["vin"] as? String,receipt["fresh_vin"] as? String==vin,
      vin.utf8.count==17,vin.utf8.allSatisfy({(48...57).contains($0)||(65...90).contains($0)}),
      !vin.contains("I"),!vin.contains("O"),!vin.contains("Q"),let stamp=context["observed_utc"] as? String else{return nil}
    return ["vin":vin,"adapter":"MDI","observed_utc":stamp]
  }
}
enum MdiCleanup {
  static func confirmed(_ result:[String:Any])->Bool {
    guard result["usb_closed"] as? Bool==true,result["filter_disabled"] as? Bool==true,
      result["halt_sent"] as? Bool==true,result["USB_IN_cancel_reaped"] as? Bool==true,
      result["discovery_multicast_left"] as? Bool==true,
      let workflow=result["workflow"] as? [String:Any],let guest=workflow["native_firmware"] as? [String:Any],
      guest["forced_kill"] as? Bool==false,guest["transport_cleanup_ok"] as? Bool==true else{return false}
    return true
  }
  static func confirmed(run:URL)->Bool {
    MdiWorkflow.read(run,"mdi-result-private.json",limit:524288).map{confirmed($0)} ?? false
  }
}
extension Shell {
  func updateMDISession(_ current:EmulatorSession) {
    guard current.adapter?.kind == .mdi else{return}
    current.updateMDIIdentity()
    if let phase=MdiWorkflow.read(current.run,"session-state.json"),let state=phase["state"] as? String,state != current.mdiPhase {
      current.mdiPhase=state
      status.stringValue=state=="firmware" ? current.mode.status(for:.mdi) : phase["message"] as? String ?? "Connecting MDI…"
    }
    if let observation=MdiWorkflow.freshVehicle(current.run) {
      let existing=(try? Data(contentsOf:state.appendingPathComponent("vehicle-status.json")))
        .flatMap{try? JSONSerialization.jsonObject(with:$0) as? [String:String]}
      if existing?["observed_utc"] != observation["observed_utc"] {
        try? JSONSerialization.data(withJSONObject:observation).write(to:state.appendingPathComponent("vehicle-status.json"),options:.atomic)
        vehicle.stringValue="MDI · VIN "+observation["vin"]!
        securityResumeAdapter=current.adapter
      }
    }
  }
}
enum MdiWorkflowChecks {
  static func run() throws {
    var checks=0
    func need(_ value:Bool,_ detail:String)throws {try ShellChecks.need(value,detail);checks+=1}
    try need(MdiWorkflow.validTarget("mdi-usb:14300000:4294974292"),"MDI selected native registry target accepted")
    for target in ["/dev/cu.usbmodemA","mdi-usb:14300000:0","mdi-usb:abcd:42","mdi-usb:14300000:42:old"] {
      try need(!MdiWorkflow.validTarget(target),"MDI requires exact location and live registry identity")
    }
    try need(ChipsoftSessionMode.fullNative.arguments(for:.mdi)==["--candi-mdi-mode","full"],"MDI full control is explicit")
    try need(ChipsoftSessionMode.readOnly.arguments(for:.mdi)==["--candi-mdi-mode","read"],"MDI read mode stays constrained")
    try need(ChipsoftSessionMode.seeds.arguments(for:.mdi)==["--candi-mdi-mode","seeds"],"MDI seed mode stays separate")
    var result:[String:Any]=["usb_closed":true,"filter_disabled":true,"halt_sent":true,"USB_IN_cancel_reaped":true,"discovery_multicast_left":true,
      "workflow":["native_firmware":["forced_kill":false,"transport_cleanup_ok":true]]]
    try need(MdiCleanup.confirmed(result),"MDI confirmed guest and carrier cleanup permits continuation")
    for key in ["usb_closed","filter_disabled","halt_sent","USB_IN_cancel_reaped","discovery_multicast_left"] {
      var failed=result;failed[key]=false;try need(!MdiCleanup.confirmed(failed),"MDI unconfirmed cleanup blocks continuation")
    }
    result["workflow"]=["native_firmware":["forced_kill":true,"transport_cleanup_ok":true]]
    try need(!MdiCleanup.confirmed(result),"MDI forced guest exit cannot become cleanup success")
    print("MDI workflow checks passed: \(checks); no USB or vehicle I/O.")
  }
}
