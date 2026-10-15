// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// OS binding only: the shared Rust probe owns identity, fresh authorization,
// VIN assembly and protocol cleanup. Never accept console text as a VIN result.
struct NanoProbeResult {
  let value: [String: Any]
  let identity: String
  let vin: String?
  init(value: [String: Any], exitCode: Int32, vinRequired: Bool) throws {
    guard exitCode == 0, value["hardware"] as? String == "VCX-NANO",
      value["firmware"] as? String == "1.9.4.2",
      value["channel_cleanup"] as? Bool == true, value["usb_closed"] as? Bool == true,
      let errors = value["cleanup_errors"] as? [String], errors.isEmpty,
      value["error"] == nil || value["error"] is NSNull else {
      throw ShellError.message("Nano identity or adapter cleanup was not verified. See Console.")
    }
    identity = "VCX-NANO / 1.9.4.2"
    if vinRequired {
      guard value["status"] as? String == "hs_vin_received",
        value["startup_complete"] as? Bool == true,
        value["query_record_verified"] as? Bool == true,
        value["installed_record_verified"] as? Bool == true,
        value["fresh_dh_exchanges"] as? Int == 2,
        value["vehicle_tx"] as? Int == 2,
        value["gm_tester_present_tx"] as? Int == 0,
        value["vin_profile"] as? String == "basic-1a90",
        let vin = value["vin"] as? String, vin.utf8.count == 17,
        vin.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) }),
        !vin.contains("I"), !vin.contains("O"), !vin.contains("Q") else {
        throw ShellError.message("Nano startup or fresh VIN is incomplete. No live connection was established.")
      }
      self.vin = vin
    } else {
      guard value["status"] as? String == "identified", value["vehicle_tx"] as? Int == 0,
        value["gm_tester_present_tx"] as? Int == 0,
        value["startup_complete"] as? Bool == false,
        value["vin"] == nil || value["vin"] is NSNull else {
        throw ShellError.message("Nano identity-only check returned unexpected vehicle activity.")
      }
      vin = nil
    }
    self.value = value
  }
}
enum NanoCleanup {
  static func confirmed(_ value: [String: Any]) -> Bool {
    guard value["schema"] as? Int == 1,
      let channels = value["attempted_channels"] as? [Int], channels.sorted() == [0, 1],
      value["cleanup_request_count"] as? Int == 6,
      value["cleanup_zero_status_reply_count"] as? Int == 6,
      value["startup_complete"] as? Bool == true,
      value["query_record_verified"] as? Bool == true,
      value["installed_record_verified"] as? Bool == true,
      value["fresh_dh_exchanges"] as? Int == 2,
      value["serial_closed"] as? Bool == true,
      value["cleanup_confirmed"] as? Bool == true else { return false }
    return true
  }
  static func confirmed(run: URL) -> Bool {
    guard let bytes = try? Data(contentsOf: run.appendingPathComponent("native-nano-cleanup.json")), bytes.count <= 8192,
      let value = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { return false }
    return confirmed(value)
  }
  static func requiredForSecurity(run: URL, adapter: USBAdapterEndpoint?) throws -> Bool {
    let recorded = try RunAdapterBinding.read(run: run)
    if let adapter = adapter, let recorded = recorded, adapter != recorded {
      throw ShellError.message("Security collection adapter binding changed. Reconnect the original adapter.")
    }
    return adapter?.kind == .nano || recorded?.kind == .nano || fm.fileExists(atPath: run.appendingPathComponent("native-nano-cleanup.json").path)
  }
}

enum RunAdapterBinding {
  static func write(_ adapter: USBAdapterEndpoint?, to run: URL) throws {
    let value: [String: Any] = ["schema": 1, "kind": adapter?.kind.rawValue ?? "offline",
      "port": adapter?.port ?? "", "identity": adapter?.identity ?? ""]
    try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]).write(to: run.appendingPathComponent("adapter-binding.json"), options: .atomic)
  }
  static func read(run: URL) throws -> USBAdapterEndpoint? {
    let file = run.appendingPathComponent("adapter-binding.json")
    // Older Chipsoft-only Mac runs predate this required per-run binding.
    guard fm.fileExists(atPath: file.path) else { return nil }
    let size = try file.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max
    guard size <= 8192, let value = try JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any],
      value["schema"] as? Int == 1, let name = value["kind"] as? String else {
      throw ShellError.message("Invalid collection adapter provenance; security processing is blocked.")
    }
    if name == "offline" { return nil }
    guard let kind = USBAdapterKind(rawValue: name), let port = value["port"] as? String,
      (kind == .mdi ? MdiWorkflow.validTarget(port) : port.hasPrefix("/dev/cu.usbmodem") && URL(fileURLWithPath: port).deletingLastPathComponent().path == "/dev"),
      let identity = value["identity"] as? String else {
      throw ShellError.message("Invalid collection adapter identity; security processing is blocked.")
    }
    return USBAdapterEndpoint(kind: kind, port: port, identity: identity.isEmpty ? nil : identity)
  }
}

extension Shell {
  func nanoProbe(port: String, vinRequired: Bool, attempt: ConnectionAttempt? = nil) throws -> (NanoProbeResult, ToolResult) {
    let directory = state.appendingPathComponent("startup-runs/nano-" + UUID().uuidString)
    try fm.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    attempt?.bind(directory,mode:"read_only")
    try? JSONSerialization.data(withJSONObject:["adapter":"VCX Nano"]).write(to:directory.appendingPathComponent("dtc-context.json"),options:.atomic)
    let receipt = directory.appendingPathComponent("result.json")
    let result = try runTool(binaries.appendingPathComponent("nano-usb-probe"),
      ["--port", port, receipt.path] + (vinRequired ? ["--hs-vin-check"] : []), timeout: vinRequired ? 45 : 8)
    // Preserve the bounded helper diagnostics even when receipt validation fails.
    let console = directory.appendingPathComponent("probe-console.log")
    try Data(result.text.utf8).write(to: console, options: .atomic)
    try fm.setAttributes([.posixPermissions: 0o600], ofItemAtPath: console.path)
    let bytes = try Data(contentsOf: receipt)
    guard bytes.count <= 65536, let value = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] else {
      throw ShellError.message("Invalid Nano result receipt.")
    }
    return (try NanoProbeResult(value: value, exitCode: result.code, vinRequired: vinRequired), result)
  }
  func verifyUSBAdapter(_ endpoint: USBAdapterEndpoint) throws -> USBAdapterEndpoint {
    switch endpoint.kind {
    case .nano:
      let (result, _) = try nanoProbe(port: endpoint.port, vinRequired: false)
      return USBAdapterEndpoint(kind: .nano, port: endpoint.port, identity: result.identity)
    case .mdi: throw ShellError.message("MDI identity is reverified through fresh Connect and start.")
    case .chipsoft:
      let result = try runTool(binaries.appendingPathComponent("chipsoft-probe"), ["--port", endpoint.port, "--timeout-ms", "2000"], timeout: 5)
      guard result.code == 0, let identity = adapterIdentity(result.text) else {
        throw ShellError.message("Chipsoft identity check failed.")
      }
      return USBAdapterEndpoint(kind: .chipsoft, port: endpoint.port, identity: identity)
    }
  }
  func connectNano(port: String, mode: ChipsoftSessionMode, attempt: ConnectionAttempt) {
    guard !busy, session == nil, store.selected != nil else { return }
    attempt.selectAdapter("NANO")
    busy = true; refresh(); attempt.advance("IDENTIFICATION")
    status.stringValue = "Verifying VCX Nano, fresh startup and VIN…"
    vehicle.stringValue = "VCX Nano · Vehicle not connected"
    DispatchQueue.global().async {
      let result = Result { try self.nanoProbe(port: port, vinRequired: true, attempt:attempt) }
      DispatchQueue.main.async {
        self.busy = false
        do {
          let (probe, tool) = try result.get()
          self.lastConsole = tool.text
          guard let vin = probe.vin else { throw ShellError.message("Fresh Nano VIN unavailable.") }
          let observation = ["vin": vin, "adapter": "VCX Nano", "firmware": "1.9.4.2", "observed_utc": ISO8601DateFormatter().string(from: Date())]
          try JSONSerialization.data(withJSONObject: observation).write(to: self.state.appendingPathComponent("vehicle-status.json"), options: .atomic)
          let endpoint = USBAdapterEndpoint(kind: .nano, port: port, identity: probe.identity)
          self.securityResumeAdapter = endpoint
          self.vehicle.stringValue = "VCX Nano · VIN " + vin
          self.status.stringValue = "VIN verified. Starting original diagnostic software…"
          attempt.advance("FIRMWARE_START"); self.connectionAttempt = attempt
          self.start(adapter: endpoint, mode: mode)
        } catch {
          attempt.finish("FAILED", "PROTOCOL_OR_PROCESS_ERROR")
          self.vehicle.stringValue = "VCX Nano · Vehicle connection not verified"
          self.status.stringValue = error.localizedDescription
          self.recordAppError(error.localizedDescription, kind: "connection_error")
          self.showError(error.localizedDescription)
        }
        self.refresh()
      }
    }
  }
}
