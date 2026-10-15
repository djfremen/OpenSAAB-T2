// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// Android Actions shortcuts (ECU information, Read DTC, Clear DTC, Engine Data, Original menus).
// macOS lifecycle only: recognition, key choice, timing and bounds are shared Rust (menu_navigation.rs).
extension Shell {
  func menuShortcut(_ target: String, _ title: String) {
    guard let current = session, current.running else { textDialog(title, "Start firmware before choosing a shortcut."); return }
    guard current.connected else {
      textDialog(title, "Shortcuts need a connected, identified vehicle. Offline, use the original firmware menus directly: Diagnostics → model year → vehicle type → \(title).")
      return
    }
    if securityJob != nil || securityNavigating {
      textDialog(title, "Finish security collection first; the firmware controls remain available."); return
    }
    if target == "clear_dtc" {
      let a = NSAlert()
      a.messageText = "Clear diagnostic trouble codes?"
      a.informativeText = "OpenSAAB will open Clear DTC in the original firmware. Nothing is erased until you confirm the firmware's own prompt. Review or save the current codes first."
      a.addButton(withTitle: "Open Clear DTC")
      a.addButton(withTitle: "Cancel")
      guard a.runModal() == .alertFirstButtonReturn else { return }
    }
    recordOperation(["read_dtc":"dtc_read","clear_dtc":"dtc_clear","ecu_info":"ecu_info","engine_data":"engine_data"][target] ?? "unknown")
    menuNavigationGeneration += 1
    let generation = menuNavigationGeneration, worker = securityWorker
    let request: [String: Any] = ["op": "menu_navigate", "target": target, "vin": securityObservation()["vin"] ?? "",
                                  "full_control": current.mode == .fullNative]
    DispatchQueue.global().async {
      let result = Result { try SecurityWorker.call(worker, request) }
      DispatchQueue.main.async {
        guard self.session === current, generation == self.menuNavigationGeneration else { return }
        switch result {
        case .success(let r):
          let hint = r["hint"] as? String ?? ""
          if r["rejected"] as? Bool == true || r["active"] as? Bool != true {
            self.menuNavigating = false
            self.textDialog(title, hint)
            return
          }
          self.menuNavigation = r["navigation"] as? [String: Any] ?? [:]
          self.menuNavigating = true
          self.status.stringValue = hint
          self.window.makeFirstResponder(self.lcd)
        case .failure(let e): self.showError(e.localizedDescription)
        }
      }
    }
  }
  func originalMenus() {
    recordOperation("original_menus")
    let wasAutomating = menuNavigating || securityNavigating
    menuNavigating = false; securityNavigating = false
    menuNavigationGeneration += 1; securityNavigationGeneration += 1
    status.stringValue = wasAutomating ? "Shortcut stopped · The original firmware menus are yours." : "Original firmware menus · Use the keypad, arrows and function keys."
    window.makeFirstResponder(lcd)
  }
  func menuShortcutTick(_ current: EmulatorSession) {
    guard menuNavigating, !menuChecking, current.canAcceptMenuKey, Date().timeIntervalSince(menuPoll) >= 0.05 else { return }
    menuPoll = Date(); menuChecking = true
    let run = current.run, worker = securityWorker, nav = menuNavigation, generation = menuNavigationGeneration
    DispatchQueue.global().async {
      let result = Result { () -> [String: Any] in
        let bytes = try Data(contentsOf: run.appendingPathComponent("screen.txt"))
        guard bytes.count <= 8192 else { throw ShellError.message("Invalid firmware screen.") }
        return try SecurityWorker.call(worker, ["op": "menu_navigate", "navigation": nav, "screen": String(decoding: bytes, as: UTF8.self),
                                                "now_ms": Int(Date().timeIntervalSince1970 * 1000)])
      }
      DispatchQueue.main.async {
        self.menuChecking = false
        // A manual key, Original menus or a new session invalidates this answer: never send a stale key.
        guard self.session === current, !self.busy, self.menuNavigating, generation == self.menuNavigationGeneration,
              current.canAcceptMenuKey, case .success(let r) = result else { return }
        self.menuNavigation = r["navigation"] as? [String: Any] ?? [:]
        if let key = r["key"] as? Int { current.key(key) }
        let active = r["active"] as? Bool ?? false
        if !active || r["key"] != nil { self.status.stringValue = r["hint"] as? String ?? "" }
        self.menuNavigating = active
      }
    }
  }
}
