// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// Thin presentation of the shared display-only status operation. This value
// never grants access or participates in collection, processing or import gates.
struct VehicleStatusView {
  let value: [String:Any]
  init(_ value:[String:Any],firmware:String) throws {
    guard value["schema"] as? Int == 1,value["display_only"] as? Bool == true,value["history_only"] as? Bool == true,
      value["vehicle_access_verified"] as? Bool == false,
      value["source_card_firmware_id"] as? String == firmware,
      let state = value["state"] as? String,["INIT_AUTH","PRE_AUTH","POST_AUTH","INVALID","UNAVAILABLE"].contains(state),
      ["summary","auth_status","timestamp","details","color","freshness"].allSatisfy({value[$0] is String}),
      let color = value["color"] as? String,color.range(of:"^#[a-fA-F0-9]{6}$",options:.regularExpression) != nil else {
      throw ShellError.message("Shared vehicle status is unavailable or changed.")
    }
    if let workflow = value["workflow_summary"],!(workflow is NSNull) {
      guard let text = workflow as? String,["Security status · Collecting pre-auth","Security status · Ready to process"].contains(text) else {
        throw ShellError.message("Shared collection display is unavailable or changed.")
      }
    }
    self.value = value
  }
  var summary: String { value["summary"] as! String }
  var color: NSColor {
    let rgb = UInt32((value["color"] as! String).dropFirst(),radix:16)!
    return NSColor(calibratedRed:CGFloat((rgb>>16)&255)/255,green:CGFloat((rgb>>8)&255)/255,blue:CGFloat(rgb&255)/255,alpha:1)
  }
  static func localTime(_ utc:String?) -> String {
    guard let utc = utc,let time = SecurityStatus.time(utc) else { return "Date unknown" }
    let format = DateFormatter(); format.dateFormat = "MMM d, yyyy · HH:mm:ss z"
    return format.string(from:time)
  }
  var details: String {
    let connection = "Last connection · " + Self.localTime(value["connection_observed_utc"] as? String)
    var sharedDetails = value["details"] as! String
    if let utc = value["status_utc"] as? String {
      sharedDetails = sharedDetails.replacingOccurrences(of:utc,with:Self.localTime(utc))
    }
    var vehicle = value["vehicle_summary"] as? String ?? "No saved vehicle connection"
    if let observed = value["observed_vehicle"] as? [String:Any] {
      for (key,label) in [("vin","VIN"),("adapter","Adapter"),("firmware","Adapter firmware"),
        ("model_year","Model year"),("platform","Platform"),("engine","Engine"),
        ("gearbox","Gearbox"),("body","Body"),("color","Color")] {
        if let text = observed[key] as? String,!text.isEmpty { vehicle += "\n" + label + ": " + text }
        else if key == "model_year",let year = observed[key] as? Int { vehicle += "\n" + label + ": " + String(year) }
      }
    }
    return [vehicle,connection,value["workflow_summary"] as? String,sharedDetails].compactMap{$0}.joined(separator:"\n\n")
  }
  static func lines(status:String,vehicle:String?,offline:Bool,hasVehicle:Bool,snapshot:VehicleStatusView?) -> String {
    var lines = [status.replacingOccurrences(of:"\n",with:" · ")]
    if let vehicle = vehicle { lines.append(vehicle.replacingOccurrences(of:"\n",with:" · ")) }
    if !offline && hasVehicle { lines.append(snapshot?.value["workflow_summary"] as? String ?? snapshot?.summary ?? "Security: [N/A]") }
    return lines.prefix(3).joined(separator:"\n")
  }
}
extension Shell {
  private var statusContext: String {
    let observation = (try? Data(contentsOf:state.appendingPathComponent("vehicle-status.json")))
      .flatMap{$0.count <= 8192 ? SecurityStatus.hash($0) : nil} ?? ""
    return [store.selected?.id ?? "",session?.run.path ?? "",String(session?.connected ?? false),
      observation].joined(separator:"|")
  }
  func requestVehicleStatus() {
    let context = statusContext
    if context != vehicleStatusContext {
      vehicleStatusContext = context; vehicleStatusGeneration += 1; vehicleStatusSnapshot = nil
      renderVehicleStatus()
    }
    guard !vehicleStatusPending,Date().timeIntervalSince(vehicleStatusPoll) >= 1 else { return }
    vehicleStatusPending = true; vehicleStatusPoll = Date()
    let generation = vehicleStatusGeneration, firmware = store.selected?.id ?? "", worker = securityWorker
    var request:[String:Any] = ["op":"status","state":state.path,"firmware":firmware,
      "offline":session != nil && session?.connected == false,"now_utc":ISO8601DateFormatter().string(from:Date())]
    if let selected = store.selected { request["card"] = store.card(selected).path }
    if let current = session { request["run"] = current.run.path }
    DispatchQueue.global().async {
      let result = Result { try VehicleStatusView(SecurityWorker.call(worker,request),firmware:firmware) }
      DispatchQueue.main.async {
        self.vehicleStatusPending = false
        guard generation == self.vehicleStatusGeneration,context == self.statusContext else { return }
        if case .success(let status) = result { self.vehicleStatusSnapshot = status }
        else { self.vehicleStatusSnapshot = nil }
        self.renderVehicleStatus()
      }
    }
  }
  func renderVehicleStatus() {
    var currentVehicle:String?
    if session?.connected == true || !bluetoothVehicle.isEmpty {
      currentVehicle = vehicle.stringValue
    } else if session == nil {
      currentVehicle = vehicleStatusSnapshot?.value["observed_vehicle"] as? [String:Any] != nil
        ? vehicleStatusSnapshot?.value["compact_vehicle"] as? String : "No vehicle connected"
    }
    vehicleSummaryButton?.title = VehicleStatusView.lines(status:status.stringValue,vehicle:currentVehicle,
      offline:session?.connected == false,hasVehicle:session?.connected == true || !bluetoothVehicle.isEmpty || vehicleStatusSnapshot?.value["observed_vehicle"] as? [String:Any] != nil,snapshot:vehicleStatusSnapshot)
    vehicleSummaryButton?.textColor = vehicleStatusSnapshot?.color ?? foreground
    vehicleSummaryButton?.setAccessibilityLabel("Vehicle and security status — tap for details")
  }
}
