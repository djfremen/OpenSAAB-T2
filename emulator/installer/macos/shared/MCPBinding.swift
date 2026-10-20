// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation
import Darwin

// Thin native binding: Rust owns MCP, screen interpretation and path navigation.
// All app actions still use the same Shell/EmulatorSession lifecycle and encoder queue.
final class MCPBinding {
  weak var shell: Shell?
  let directory: URL
  let instance = UUID().uuidString
  private var timer: Timer?
  private var lockFD: Int32 = -1
  init(shell: Shell, directory: URL) throws {
    self.shell = shell; self.directory = directory
    try fm.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    var info = stat()
    guard lstat(directory.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR, info.st_uid == geteuid() else {
      throw ShellError.message("MCP directory must be an owned directory, without a symbolic link.")
    }
    guard chmod(directory.path, 0o700) == 0 else { throw ShellError.message("Cannot protect MCP directory.") }
    lockFD = open(directory.appendingPathComponent("app.lock").path, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard lockFD >= 0, flock(lockFD, LOCK_EX | LOCK_NB) == 0 else {
      if lockFD >= 0 { close(lockFD); lockFD = -1 }
      throw ShellError.message("Another OpenSAAB app owns this MCP endpoint.")
    }
  }
  deinit {
    timer?.invalidate()
    let disabled: [String:Any] = ["schema":1,"enabled":false,"timestamp_ms":Int(Date().timeIntervalSince1970*1000)]
    if let data = try? JSONSerialization.data(withJSONObject:disabled) { try? data.write(to:directory.appendingPathComponent("state.json"),options:.atomic) }
    if lockFD >= 0 { flock(lockFD, LOCK_UN); close(lockFD) }
  }
  func start() {
    guard timer == nil else { return }
    let t = Timer(timeInterval: 0.05, repeats: true) { [weak self] _ in self?.advance() }
    timer = t
    RunLoop.main.add(t, forMode: .common)
    RunLoop.main.add(t, forMode: .modalPanel)
    publish()
  }
  private var modal: NSWindow? {
    if let window = NSApp.modalWindow { return window }
    return shell?.window?.attachedSheet
  }
  private func views(_ view: NSView?) -> [NSView] {
    guard let view = view else { return [] }
    return [view] + view.subviews.flatMap { views($0) }
  }
  private func menu() -> [String:Any]? {
    guard let run = shell?.session?.run,
      let data = try? Data(contentsOf: run.appendingPathComponent("native-menu.json")), data.count <= 65536,
      let value = try? JSONSerialization.jsonObject(with: data) as? [String:Any], value["source"] as? String == "original_guest_lcd" else { return nil }
    return value
  }
  func snapshot() -> [String:Any] {
    guard let shell = shell else { return [:] }
    let alertViews = views(modal?.contentView)
    let labels = alertViews.compactMap { view -> String? in
      guard !(view is NSSecureTextField), let field = view as? NSTextField, !field.isHidden else { return nil }
      return field.stringValue
    }
    var monotonic = timespec()
    clock_gettime(CLOCK_MONOTONIC, &monotonic)
    return ["schema":1, "enabled":true, "instance":instance,
      "timestamp_ms":Int(Date().timeIntervalSince1970*1000),
      "heartbeat_monotonic_ms":Int(monotonic.tv_sec)*1000+Int(monotonic.tv_nsec)/1000000,
      "app_pid":Int(getpid()), "state_directory":shell.state.path,
      "emulator_pid":shell.session.map { Int($0.process.processIdentifier) } as Any? ?? NSNull(),
      "session_age_ms":shell.session == nil ? NSNull() : max(0,Int(Date().timeIntervalSince(shell.sessionBegan)*1000)) as Any,
      "app_version":shellVersion, "architecture":shellArchitecture, "program_name":shell.window?.title ?? "",
      "busy":shell.busy, "status":shell.status.stringValue,
      "session_running":shell.session?.running ?? false,
      "session":shell.session?.run.lastPathComponent ?? "",
      "connected":shell.session?.connected ?? false,
      "operator_generation":shell.mcpOperatorGeneration,
      "can_start_offline":!shell.busy && shell.session == nil && shell.store.selected != nil && shell.firmwareWindow == nil && modal == nil,
      "input_available":!shell.busy && modal == nil && shell.session?.canAcceptMenuKey == true,
      "automation_active":shell.securityNavigating || shell.menuNavigating || shell.securityJob != nil,
      "modal_open":modal != nil, "alert_labels":labels,
      "selected_firmware":shell.store.selected?.id ?? "",
      "menu":menu() as Any? ?? NSNull()]
  }
  private func publish() {
    if let data = try? JSONSerialization.data(withJSONObject:snapshot(),options:[.sortedKeys]) {
      try? data.write(to:directory.appendingPathComponent("state.json"),options:.atomic)
    }
  }
  private func reply(_ id: String, accepted: Bool, message: String) {
    let value: [String:Any] = ["id":id,"accepted":accepted,"message":message,"state":snapshot()]
    if let data = try? JSONSerialization.data(withJSONObject:value,options:[.sortedKeys]) {
      try? data.write(to:directory.appendingPathComponent("response.json"),options:.atomic)
    }
  }
  func advance() {
    guard let shell = shell, shell.window != nil else { return }
    publish()
    let file = directory.appendingPathComponent("command.json")
    guard fm.fileExists(atPath:file.path) else { return }
    var id = "invalid"
    do {
      var info = stat()
      guard lstat(file.path, &info) == 0, info.st_mode & S_IFMT == S_IFREG, info.st_uid == geteuid(), info.st_size <= 8192 else {
        throw ShellError.message("Invalid MCP command file.")
      }
      let data = try Data(contentsOf:file)
      try fm.removeItem(at:file)
      guard let command = try JSONSerialization.jsonObject(with:data) as? [String:Any],
        let requestID = command["id"] as? String, !requestID.isEmpty, requestID.count <= 64,
        let op = command["op"] as? String,
        let deadline = command["deadline_ms"] as? Int, deadline >= Int(Date().timeIntervalSince1970*1000),
        command["instance"] as? String == instance else { throw ShellError.message("Expired or invalid MCP command.") }
      id = requestID
      let common: Set<String> = ["id","op","deadline_ms","instance"]
      let allowed = op == "key" ? common.union(["encoder","session","revision","operator_generation"]) : op == "stop" ? common.union(["session"]) : common
      guard Set(command.keys).isSubset(of:allowed), !shell.busy, modal == nil else {
        throw ShellError.message("Finish the current app operation or dialog before MCP navigation.")
      }
      switch op {
      case "start_offline":
        guard snapshot()["can_start_offline"] as? Bool == true else { throw ShellError.message("Offline startup is unavailable; choose ready software first.") }
        shell.start()
      case "stop":
        guard command["session"] as? String == shell.session?.run.lastPathComponent else { throw ShellError.message("Session changed before Stop; inspect state again.") }
        shell.stop()
      case "key":
        guard let current = shell.session, current.canAcceptMenuKey,
          !shell.securityNavigating, !shell.menuNavigating, shell.securityJob == nil,
          command["session"] as? String == current.run.lastPathComponent,
          command["operator_generation"] as? Int == shell.mcpOperatorGeneration,
          let screen = menu(), screen["input_idle"] as? Bool == true,
          command["revision"] as? String == screen["revision"] as? String,
          let code = command["encoder"] as? Int,
          [1,2,3,4,6,7,8,9,10,11,12,13,14,16,17,18,19,20,21,22,23,24,25].contains(code) else {
          throw ShellError.message("Menu/session changed, operator took control, or firmware input is busy. Read state again.")
        }
        shell.key(code, fromMCP:true)
        try current.pump()
        shell.status.stringValue = "AI control (MCP) · Firmware key sent"
      default: throw ShellError.message("Unsupported MCP app operation.")
      }
      publish()
      reply(id,accepted:true,message:"Command admitted; inspect the resulting firmware state for completion.")
    } catch { reply(id,accepted:false,message:error.localizedDescription) }
  }
}

extension Shell {
  func mcpControl() {
    do {
      if mcpBinding == nil {
        mcpBinding = try MCPBinding(shell:self,directory:state.appendingPathComponent("mcp"))
        mcpBinding?.start()
      }
      guard let directory = mcpBinding?.directory else { return }
      let config: [String:Any] = ["mcpServers":["opensaab":["command":binaries.appendingPathComponent("opensaab-mcp").path,
        "args":["--bridge",directory.path]]]]
      let data = try JSONSerialization.data(withJSONObject:config,options:[.prettyPrinted,.sortedKeys,.withoutEscapingSlashes])
      let file = directory.appendingPathComponent("client-config.json")
      try data.write(to:file,options:.atomic)
      let alert = NSAlert()
      alert.messageText = "AI control (MCP)"
      alert.informativeText = "MCP is enabled for this app while it is open. AI can inspect the original menu text, select visible entries, follow menu paths and press firmware keys.\n\nClient configuration saved to:\n\(file.path)\n\n" + String(decoding:data,as:UTF8.self)
      alert.addButton(withTitle:"Close")
      alert.addButton(withTitle:"Disable MCP")
      if alert.runModal() == .alertSecondButtonReturn { mcpBinding = nil }
    } catch { showError(error.localizedDescription) }
  }
}
