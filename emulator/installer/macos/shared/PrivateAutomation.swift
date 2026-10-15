// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation
import Darwin

// Private, explicitly enabled local operator binding. Calls the real Shell
// actions and NSAlert buttons; invocation acknowledgments never claim success.
final class PrivateAutomation {
  weak var shell: Shell?
  let directory: URL
  private var secretFD: Int32 = -1
  private var secretBytes = Data()
  private var lastState = Date.distantPast
  private var controlTimer: Timer?
  private var actionPending = false
  init(shell: Shell, directory: URL) throws {
    self.shell = shell; self.directory = directory
    try fm.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    try fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
    let fifo = directory.appendingPathComponent("credential.fifo")
    if mkfifo(fifo.path, 0o600) != 0 && errno != EEXIST { throw ShellError.message("Cannot create private credential FIFO.") }
    var info = stat()
    guard lstat(fifo.path, &info) == 0, info.st_mode & S_IFMT == S_IFIFO, info.st_uid == geteuid() else {
      throw ShellError.message("Private credential path is not an owned FIFO.")
    }
    guard chmod(fifo.path, 0o600) == 0 else { throw ShellError.message("Cannot protect private credential FIFO.") }
    secretFD = open(fifo.path, O_RDONLY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW)
    guard secretFD >= 0 else { throw ShellError.message("Cannot open private credential FIFO.") }
  }
  deinit { controlTimer?.invalidate(); if secretFD >= 0 { close(secretFD) }; secretBytes.removeAll() }
  func start() {
    guard controlTimer == nil else { return }
    // A session tick can enter NSAlert.runModal. Its own Timer cannot re-enter
    // that callback, so the explicitly enabled operator driver has a separate
    // timer registered in the actual modal run-loop mode.
    let timer = Timer(timeInterval: 0.05, repeats: true) { [weak self] _ in self?.advance() }
    controlTimer = timer
    RunLoop.main.add(timer, forMode: .common)
    RunLoop.main.add(timer, forMode: .modalPanel)
  }
  static func command(_ bytes: Data) throws -> [String: Any] {
    guard bytes.count <= 8192, let value = try JSONSerialization.jsonObject(with: bytes) as? [String: Any],
      let id = value["id"] as? String, UUID(uuidString: id) != nil,
      let op = value["op"] as? String else { throw ShellError.message("Invalid private command.") }
    let keys: [String: Set<String>] = [
      "status": [], "snapshot": [], "setup": [], "download": [], "security": [], "stop": [], "original_menus": [],
      "cancel_setup": [], "select_software": ["firmware"], "key": ["code"], "alert": ["button"], "connect": ["port", "mode"]]
    guard let allowed = keys[op], Set(value.keys).isSubset(of: allowed.union(["id", "op"])) else {
      throw ShellError.message("Unsupported private command fields; credentials belong only in the FIFO.")
    }
    return value
  }
  private func views(_ root: NSView?) -> [NSView] {
    guard let root = root else { return [] }
    return [root] + root.subviews.flatMap { views($0) }
  }
  private var alertWindow: NSWindow? {
    if let modal = NSApp.modalWindow { return modal }
    if let sheet = shell?.window.attachedSheet, sheet !== shell?.firmwareWindow { return sheet }
    return nil
  }
  func advance() {
    guard let shell = shell, shell.window != nil else { return }
    receiveCredential()
    if Date().timeIntervalSince(lastState) >= 0.2 { writeState(); lastState = Date() }
    let input = directory.appendingPathComponent("command.json")
    guard fm.fileExists(atPath: input.path) else { return }
    var id = "invalid", op = "invalid"
    do {
      let attributes = try fm.attributesOfItem(atPath: input.path)
      guard attributes[.type] as? FileAttributeType == .typeRegular,
        let size = attributes[.size] as? NSNumber, size.intValue <= 8192 else {
        throw ShellError.message("Private command must be a bounded regular file.")
      }
      let bytes = try Data(contentsOf: input)
      try fm.removeItem(at: input)
      let command = try Self.command(bytes)
      id = command["id"] as! String; op = command["op"] as! String
      let controls = ["status", "snapshot", "alert"]
      if actionPending && !controls.contains(op) {
        throw ShellError.message("An operator action is still active; inspect state or respond to its alert first.")
      }
      if alertWindow != nil && !["status", "snapshot", "alert"].contains(op) {
        throw ShellError.message("Respond to the current alert first.")
      }
      // Publish admission before invoking an action that may run a nested modal loop.
      try response(id: id, op: op, status: "invoked", message: "Operator action invoked; inspect state and original screen for its outcome.")
      if controls.contains(op) { invoke(command) }
      else {
        actionPending = true
        // Leave the control Timer's callback before a Shell action can open a
        // modal alert. The timer remains able to admit only status/snapshot or
        // an explicitly labelled alert button while that real action is active.
        DispatchQueue.main.async { [weak self] in
          guard let self = self else { return }
          defer { self.actionPending = false; self.writeState() }
          self.invoke(command)
        }
      }
    } catch {
      try? response(id: id, op: op, status: "rejected", message: error.localizedDescription)
    }
  }
  private func invoke(_ command: [String:Any]) {
    guard let shell = shell else { return }
    let id = command["id"] as! String, op = command["op"] as! String
    do {
      if alertWindow != nil && !["status", "snapshot", "alert"].contains(op) {
        throw ShellError.message("Respond to the current alert first.")
      }
      switch op {
      case "status": break
      case "snapshot": try capture()
      case "setup": shell.firmwareDialog()
      case "select_software":
        guard !shell.busy, let firmware = command["firmware"] as? String,
          let entry = shell.store.allEntries.first(where: { $0.id == firmware }),
          let language = shell.languageCodes.firstIndex(of: entry.language), shell.firmwareWindow != nil else {
          throw ShellError.message("Open Firmware selection and choose a catalog entry.")
        }
        shell.languageChoices?.selectItem(at: language); shell.languageChanged()
        guard let index = shell.firmwareEntries.firstIndex(where: { $0.id == firmware }) else { throw ShellError.message("Software entry unavailable.") }
        shell.choices?.selectItem(at: index)
      case "cancel_setup":
        guard !shell.busy, let content = shell.firmwareWindow?.contentView else {
          throw ShellError.message("No idle firmware selection to cancel.")
        }
        let buttons = views(content).compactMap { $0 as? NSButton }.filter {
          $0.title == "Set up later / Cancel download" && $0.isEnabled && !$0.isHidden
        }
        guard buttons.count == 1 else { throw ShellError.message("Firmware cancel action unavailable.") }
        buttons[0].performClick(nil)
      case "download":
        guard !shell.busy, shell.firmwareWindow != nil, let button = shell.downloadButton else { throw ShellError.message("No software download action available.") }
        button.performClick(nil)
      case "connect":
        guard !shell.busy, shell.session == nil, shell.store.selected != nil, shell.firmwareWindow == nil,
          let port = command["port"] as? String, port.hasPrefix("/dev/cu.usbmodem"),
          URL(fileURLWithPath: port).deletingLastPathComponent().path == "/dev",
          let text = command["mode"] as? String, let mode = ChipsoftSessionMode(rawValue: text) else {
          throw ShellError.message("Choose ready software, an explicit Nano CDC port and a session mode.")
        }
        shell.connectNano(port: port, mode: mode, attempt: ConnectionAttempt(state: shell.state))
      case "key":
        guard let code = command["code"] as? Int, (0...31).contains(code), shell.session?.canAcceptMenuKey == true else {
          throw ShellError.message("Firmware key queue is unavailable or key is invalid.")
        }
        shell.key(code)
      case "security": shell.securityAction()
      case "original_menus": shell.originalMenus()
      case "stop": shell.stop()
      case "alert":
        guard let label = command["button"] as? String, let modal = alertWindow else { throw ShellError.message("No current alert.") }
        let candidates: [NSButton] = views(modal.contentView).compactMap { $0 as? NSButton }
        // NSAlert's rounded action buttons only; never toggle a consent checkbox.
        let buttons = candidates.filter { button in
          button.title == label && button.isEnabled && !button.isHidden && button.bezelStyle == .rounded
        }
        guard buttons.count == 1 else { throw ShellError.message("Requested alert button is absent or ambiguous.") }
        buttons[0].performClick(nil)
      default: throw ShellError.message("Unsupported action.")
      }
      writeState()
    } catch {
      try? response(id: id, op: op, status: "rejected", message: error.localizedDescription)
    }
  }
  private func response(id: String, op: String, status: String, message: String) throws {
    // Only labels and outcome metadata; never echo arguments or credential bytes.
    let value = ["id": id, "op": op, "status": status, "message": message]
    try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]).write(to: directory.appendingPathComponent("response.json"), options: .atomic)
  }
  private func receiveCredential() {
    var bytes = [UInt8](repeating: 0, count: 4096)
    let count = read(secretFD, &bytes, bytes.count)
    guard count > 0 else { return }
    secretBytes.append(contentsOf: bytes.prefix(count))
    guard secretBytes.count <= 4096 else { secretBytes.removeAll(); return }
    guard let newline = secretBytes.firstIndex(of: 10) else { return }
    let line = secretBytes[..<newline]
    defer { secretBytes.removeAll(keepingCapacity: false) }
    guard newline == secretBytes.count - 1, let modal = alertWindow,
      let text = String(data: line, encoding: .utf8), !text.isEmpty else { return }
    let fields = views(modal.contentView).compactMap { $0 as? NSSecureTextField }.filter { !$0.isHidden && $0.isEnabled }
    guard fields.count == 1 else { return }
    fields[0].stringValue = text
    // The operator must explicitly invoke the real Process button afterward.
  }
  private func writeState() {
    guard let shell = shell else { return }
    let modal = alertWindow
    let all = views(modal?.contentView)
    let labels = all.compactMap { view -> String? in
      guard !(view is NSSecureTextField), let field = view as? NSTextField, !field.isHidden else { return nil }
      return field.stringValue
    }
    let buttons = all.compactMap { $0 as? NSButton }.filter { !$0.isHidden }.map { $0.title }
    let value: [String: Any] = ["schema": 1, "private_operator_binding": true,
      "utc": ISO8601DateFormatter().string(from: Date()), "busy": shell.busy,
      "operator_action_pending": actionPending,
      "window_title": shell.window.title,
      "program_name": Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? "",
      "pending_firmware": shell.choices.flatMap { choices in
        shell.firmwareEntries.indices.contains(choices.indexOfSelectedItem) ? shell.firmwareEntries[choices.indexOfSelectedItem].id : nil
      } ?? "",
      "status": shell.status.stringValue, "vehicle": shell.vehicle.stringValue,
      "selected_firmware": shell.store.selected?.id ?? "", "session_running": shell.session?.running ?? false,
      "run": shell.session?.run.path ?? "", "key_queue_available": shell.session?.canAcceptMenuKey ?? false,
      "alert_labels": labels, "alert_buttons": buttons,
      "secure_prompt": all.contains { $0 is NSSecureTextField }, "firmware_selection_open": shell.firmwareWindow != nil,
      "credential_fifo": directory.appendingPathComponent("credential.fifo").path]
    if let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
      try? data.write(to: directory.appendingPathComponent("state.json"), options: .atomic)
    }
  }
  private func capture() throws {
    guard let shell = shell, let content = shell.window.contentView else { throw ShellError.message("App window unavailable.") }
    try capture(content, to: "window.png")
    if let content = alertWindow?.contentView { try capture(content, to: "alert.png") }
    else { try? fm.removeItem(at: directory.appendingPathComponent("alert.png")) }
    if let content = shell.firmwareWindow?.contentView { try capture(content, to: "setup.png") }
    else { try? fm.removeItem(at: directory.appendingPathComponent("setup.png")) }
  }
  private func capture(_ content: NSView, to name: String) throws {
    let fields = views(content).compactMap { $0 as? NSSecureTextField }
    let values = fields.map { $0.stringValue }
    for field in fields { field.stringValue = "" }
    defer { for (field, value) in zip(fields, values) { field.stringValue = value } }
    guard let image = content.bitmapImageRepForCachingDisplay(in: content.bounds) else { throw ShellError.message("Cannot capture app view.") }
    content.cacheDisplay(in: content.bounds, to: image)
    guard let data = image.representation(using: .png, properties: [:]) else { throw ShellError.message("Cannot encode app view.") }
    try data.write(to: directory.appendingPathComponent(name), options: .atomic)
  }
}
