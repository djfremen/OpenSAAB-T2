// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// OS binding only. Rust owns the T8 exchange, completeness, status decoding and text.
struct EngineStartupReport {
  let value: [String: Any]
  let vin: String
  let text: String
  init(_ result: ToolResult, vin expected: String) throws {
    let prefix = "CHIPSOFT_STARTUP_RESULT "
    let lines = result.text.components(separatedBy: "\n").filter { $0.hasPrefix(prefix) }
    guard result.code == 0, lines.count == 1,
      let data = String(lines[0].dropFirst(prefix.count)).data(using: .utf8),
      let report = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      report["schema"] as? Int == 1, report["status"] as? String == "dtc_complete",
      report["source"] as? String == "host-hscan-dtc",
      report["complete"] as? Bool == true, report["session_stopped"] as? Bool == true,
      report["usb_closed"] as? Bool == true,
      let vin = report["vin"] as? String, vin == expected, vin.count == 17,
      report["dtcs"] is [[String: Any]],
      let text = report["text"] as? String, !text.isEmpty else {
      throw ShellError.message("Engine-code report incomplete or adapter cleanup not confirmed. No confirmed code list.")
    }
    self.value = report; self.vin = vin; self.text = text
  }
  func save(root: URL) throws {
    let dir = root.appendingPathComponent("dtc-reports")
    try fm.createDirectory(at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    let id = "macos_dtc_startup_" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
    for (suffix, bytes) in [("json", try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys])), ("txt", Data(text.utf8))] {
      let file = dir.appendingPathComponent(id + "." + suffix)
      try bytes.write(to: file, options: .atomic)
      try fm.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
    }
  }
}

extension Shell {
  func engineStartupAction() {
    guard !busy, securityJob == nil else { return }
    guard session != nil else { detectAdapter(bluetoothMode:"ecm_dtc"); return }
    let a = NSAlert(); a.messageText = "Read engine codes — HS-CAN"
    a.informativeText = "Stop the current firmware session, then read a fresh VIN and Trionic 8 engine codes. Codes are not cleared."
    a.addButton(withTitle: "Stop and read engine codes"); a.addButton(withTitle: "Keep running")
    if a.runModal() == .alertFirstButtonReturn { stop { self.detectAdapter(bluetoothMode:"ecm_dtc") } }
  }
  func reviewStartup(_ report: EngineStartupReport, port: String, mode: ChipsoftSessionMode, attempt: ConnectionAttempt) {
    do { try report.save(root: state) }
    catch { attempt.finish("FAILED", "IO_ERROR"); showError("Could not save the engine report: " + error.localizedDescription); return }
    status.stringValue = "Engine-code read complete · Saved locally · Codes not cleared"
    attempt.advance("DTC_REVIEW")
    let a = NSAlert(); a.messageText = "Trionic 8 engine codes — HS-CAN"
    let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 590, height: 360)); scroll.hasVerticalScroller = true
    let view = NSTextView(frame: scroll.bounds); view.isEditable = false; view.isSelectable = true
    view.string = report.text; view.autoresizingMask = [.width]; view.textContainer?.widthTracksTextView = true
    view.isVerticallyResizable = true; scroll.documentView = view; a.accessoryView = scroll
    a.addButton(withTitle: "Continue to diagnostic menus"); a.addButton(withTitle: "Done")
    guard a.runModal() == .alertFirstButtonReturn else { attempt.finish("COMPLETED"); return }
    // Report review can remain open for a long time. Check a fresh VIN before
    // handing the released adapter to firmware; never use cached identity alone.
    busy = true; status.stringValue = "Confirming vehicle connection…"; refresh()
    DispatchQueue.global().async {
      let checked = Result { try runTool(self.binaries.appendingPathComponent("chipsoft-vin"), ["--port", port, "--timeout-ms", "8000"], timeout: 35) }
      DispatchQueue.main.async {
        self.busy = false
        do {
          let r = try checked.get(); self.lastConsole += "\n" + r.text
          guard r.code == 0, r.text.components(separatedBy: "\n").contains("VIN=" + report.vin) else {
            throw ShellError.message("Vehicle connection changed or VIN could not be confirmed. Reconnect and read again.")
          }
          self.connectionAttempt = attempt; attempt.advance("FIRMWARE_START")
          self.start(chipsoftPort: port, mode: mode)
        } catch {
          attempt.finish("FAILED", "VIN_UNAVAILABLE"); self.showError(error.localizedDescription)
        }
        self.refresh()
      }
    }
  }
}
