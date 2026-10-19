// SPDX-License-Identifier: MPL-2.0
import AppKit
import CryptoKit
import Darwin
import Foundation

#if arch(arm64)
let shellArchitecture = "arm64"
let shellArchitectureLabel = "Apple silicon"
#elseif arch(x86_64)
let shellArchitecture = "x86_64"
let shellArchitectureLabel = "Intel"
#else
#error("Unsupported macOS architecture")
#endif

enum ShellError: LocalizedError {
  case message(String)
  var errorDescription: String? {
    if case .message(let s) = self { return s }
    return nil
  }
}
// Same evidence rule as Android TransportFailure (status 001D alone is not a diagnosis),
// now enforced once in shared Rust: emulator/src/adapter_failure.rs.
/// Presentation of the shared Rust adapter-stop classification from the run's report.json.
struct AdapterFailure {
  let kind: String, title: String, message: String
  static func read(run: URL) -> AdapterFailure? {
    let file = run.appendingPathComponent("report.json")
    guard let size = try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= 4_000_000,
      let data = try? Data(contentsOf: file),
      let report = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let failure = report["adapter_failure"] as? [String: Any],
      let kind = failure["kind"] as? String, let title = failure["title"] as? String,
      let message = failure["message"] as? String else { return nil }
    return AdapterFailure(kind: kind, title: title, message: message)
  }
  var sessionEndReason: String {
    switch kind {
    case "command_blocked": return "unsupported_command"
    case "single_wire_wake_failed": return "wake_rejected"
    default: return "adapter_stopped"
    }
  }
  var connectionReason: String {
    switch kind {
    case "adapter_disconnected": return "DISCONNECTED"
    case "can_write_timeout", "single_wire_wake_failed": return "TIMEOUT"
    default: return "PROTOCOL_OR_PROCESS_ERROR"
    }
  }
}
let fm = FileManager.default
let coreFiles = ["eprom.bin", "opsys.dwn", "candi.bin"]
enum USBAdapterKind: String, CaseIterable, Codable {
  case chipsoft = "Chipsoft Pro"
  case nano = "VCX Nano"
  case mdi = "MDI"
}
struct USBAdapterEndpoint: Equatable {
  let kind: USBAdapterKind
  let port: String
  let identity: String?
  init(kind: USBAdapterKind, port: String, identity: String? = nil) {
    self.kind = kind; self.port = port; self.identity = identity
  }
}
// Thin binding to the existing Android/Rust session policies, not a new gate.
enum ChipsoftSessionMode: String, CaseIterable {
  case fullNative = "Full native control"
  case readOnly = "Read-only diagnostics"
  case seeds = "Collect security seeds"
  var arguments: [String] { self == .seeds ? ["--candi-chipsoft-seeds"] : [] }
  func arguments(for adapter: USBAdapterKind) -> [String] {
    if adapter == .chipsoft { return arguments }
    if adapter == .mdi { return ["--candi-mdi-mode",self == .fullNative ? "full" : self == .seeds ? "seeds" : "read"] }
    switch self {
    case .fullNative: return ["--candi-nano-add-remove"]
    case .seeds: return ["--candi-nano-seeds"]
    case .readOnly: return []
    }
  }
  func status(for adapter: USBAdapterKind) -> String {
    status.replacingOccurrences(of: "Chipsoft", with: adapter.rawValue)
  }
  var status: String {
    switch self {
    case .fullNative: return "Chipsoft · Full native control · Original firmware commands"
    case .readOnly: return "Chipsoft · Read-only diagnostics"
    case .seeds: return "Chipsoft · Collect security seeds · Processing asks for your consent before any request"
    }
  }
  func environment(_ inherited: [String: String], connected: Bool) -> [String: String] {
    var env = inherited
    // A launch environment must never override the mode selected in the app.
    env["TECH2_CHIPSOFT_FULL_NATIVE"] = connected && self == .fullNative ? "1" : "0"
    env["TECH2_CHIPSOFT_KEY_STATUS"] = "0"
    env["LOAD_TEST_ACCELERATE"] = "1"
    return env
  }
}
func sha256(_ file: URL) throws -> String {
  let input = try FileHandle(forReadingFrom: file)
  defer { try? input.close() }
  var hash = SHA256()
  while let data = try input.read(upToCount: 1 << 20), !data.isEmpty { hash.update(data: data) }
  return hash.finalize().map { String(format: "%02x", $0) }.joined()
}
func regularSize(_ url: URL) throws -> Int {
  let a = try fm.attributesOfItem(atPath: url.path)
  guard a[.type] as? FileAttributeType == .typeRegular, let n = a[.size] as? NSNumber else {
    throw ShellError.message("Not a regular file: \(url.lastPathComponent)")
  }
  return n.intValue
}
struct FirmwareEntry: Codable {
  let id, label: String
  let downloadBytes: Int
  let zipSha, imageSha, language: String
  let runnable: Bool
  let url: String
}
struct FirmwareCatalog: Decodable {
  let schema: Int
  let entries: [FirmwareEntry]
}

final class FirmwareStore {
  let state, runtime, directory: URL
  let entries: [FirmwareEntry]
  private var active: URL { directory.appendingPathComponent("active.txt") }
  init(state: URL, catalog: URL) throws {
    self.state = state
    runtime = state.appendingPathComponent("runtime")
    directory = state.appendingPathComponent("firmware")
    let data = try JSONDecoder().decode(FirmwareCatalog.self, from: Data(contentsOf: catalog))
    guard data.schema == 1, !data.entries.isEmpty,
      Set(data.entries.map { $0.id }).count == data.entries.count,
      data.entries.allSatisfy({
        $0.id.range(of: "^[a-zA-Z0-9_.-]+$", options: .regularExpression) != nil
          && $0.imageSha.count == 64
      })
    else { throw ShellError.message("Invalid firmware catalog") }
    entries = data.entries
    try fm.createDirectory(at: directory, withIntermediateDirectories: true)
    // Preserve working installations. A local image gets a recorded hash,
    // not a fabricated catalog identity, and is never modified in place.
    let previous = runtime.appendingPathComponent("Saab NAO.bin")
    if !fm.fileExists(atPath: active.path), (try? regularSize(previous)) == 32 * 1024 * 1024 {
      let hash = try sha256(previous)
      let entry =
        entries.first { $0.runnable && $0.imageSha == hash }
        ?? FirmwareEntry(
          id: "imported-local", label: "Imported local firmware", downloadBytes: 0, zipSha: "",
          imageSha: hash, language: "local", runnable: true, url: "")
      if !fm.fileExists(atPath: card(entry).path) { try fm.copyItem(at: previous, to: card(entry)) }
      if entry.id == "imported-local" {
        try JSONEncoder().encode(entry).write(
          to: directory.appendingPathComponent("imported-local.json"), options: .atomic)
      }
      try activate(entry)
    }
  }
  var allEntries: [FirmwareEntry] {
    if let data = try? Data(contentsOf: directory.appendingPathComponent("imported-local.json")),
      let local = try? JSONDecoder().decode(FirmwareEntry.self, from: data),
      local.id == "imported-local"
    {
      return entries + [local]
    }
    return entries
  }
  var selected: FirmwareEntry? {
    guard
      let id = try? String(contentsOf: active, encoding: .utf8).trimmingCharacters(
        in: .whitespacesAndNewlines)
    else { return nil }
    return allEntries.first { $0.id == id && $0.runnable }
  }
  func card(_ entry: FirmwareEntry) -> URL { directory.appendingPathComponent(entry.id + ".bin") }
  func validateCore() throws {
    for name in coreFiles { try BundledRuntime.validate(name, runtime.appendingPathComponent(name)) }
  }
  func importRuntime(_ source: URL) throws {
    for name in coreFiles { try BundledRuntime.validate(name, source.appendingPathComponent(name)) }
    guard !fm.fileExists(atPath: runtime.path) else {
      throw ShellError.message(
        "Runtime already exists and was preserved. Firmware languages can be changed separately.")
    }
    let stage = state.appendingPathComponent("import-" + UUID().uuidString)
    try fm.createDirectory(at: stage, withIntermediateDirectories: true)
    defer { try? fm.removeItem(at: stage) }
    for name in coreFiles {
      try fm.copyItem(
        at: source.appendingPathComponent(name), to: stage.appendingPathComponent(name))
    }
    let legacy = source.appendingPathComponent("Saab NAO.bin")
    if (try? regularSize(legacy)) == 32 * 1024 * 1024 {
      try fm.copyItem(at: legacy, to: stage.appendingPathComponent("Saab NAO.bin"))
    }
    try fm.moveItem(at: stage, to: runtime)
  }
  func activate(_ entry: FirmwareEntry) throws {
    guard entry.runnable else {
      throw ShellError.message("This image is download-only. Choose Russian 148.000 to run it.")
    }
    guard try regularSize(card(entry)) == 32 * 1024 * 1024,
      try sha256(card(entry)) == entry.imageSha
    else { throw ShellError.message("Firmware checksum failed; previous selection preserved.") }
    try Data(entry.id.utf8).write(to: active, options: .atomic)
  }
  // Extract only the one verified image. Never unpack paths from the archive.
  func acceptArchive(_ archive: URL, entry: FirmwareEntry, cancelled: () -> Bool = { false }) throws
  {
    guard !cancelled() else {
      throw ShellError.message("Download cancelled; previous selection preserved.")
    }
    guard entry.runnable, try regularSize(archive) == entry.downloadBytes,
      try sha256(archive) == entry.zipSha
    else {
      throw ShellError.message("Download checksum or size failed; previous selection preserved.")
    }
    let listing = try runTool(
      URL(fileURLWithPath: "/usr/bin/unzip"), ["-Z1", archive.path], timeout: 10)
    let files = listing.text.split(separator: "\n").map(String.init).filter { !$0.hasSuffix("/") }
    guard listing.code == 0, files.count == 1, files[0].lowercased().hasSuffix(".bin"),
      !files[0].hasPrefix("-"), !files[0].contains(".."), !files[0].contains("*")
    else { throw ShellError.message("Unexpected firmware archive contents") }
    let temp = directory.appendingPathComponent(UUID().uuidString + ".bin")
    defer { try? fm.removeItem(at: temp) }
    fm.createFile(atPath: temp.path, contents: nil)
    let out = try FileHandle(forWritingTo: temp)
    defer { try? out.close() }
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
    p.arguments = ["-p", archive.path, files[0]]
    p.standardOutput = out
    p.standardError = FileHandle.nullDevice
    try p.run()
    p.waitUntilExit()
    guard p.terminationStatus == 0, try regularSize(temp) == 32 * 1024 * 1024,
      try sha256(temp) == entry.imageSha
    else { throw ShellError.message("Extracted firmware checksum failed") }
    guard !cancelled() else {
      throw ShellError.message("Download cancelled; previous selection preserved.")
    }
    // rename is atomic even when repairing a corrupt cache entry.
    guard rename(temp.path, card(entry).path) == 0 else {
      throw ShellError.message("Could not store verified firmware")
    }
    try activate(entry)
  }
}

struct ToolResult {
  let code: Int32
  let text: String
}
func runTool(_ tool: URL, _ arguments: [String], timeout: TimeInterval) throws -> ToolResult {
  let p = Process()
  let pipe = Pipe()
  let lock = NSLock()
  var data = Data()
  pipe.fileHandleForReading.readabilityHandler = { handle in
    let chunk = handle.availableData
    lock.lock()
    defer { lock.unlock() }
    if data.count < 65536 { data.append(chunk.prefix(65536 - data.count)) }
  }
  defer { pipe.fileHandleForReading.readabilityHandler = nil }
  p.executableURL = tool
  p.arguments = arguments
  p.standardOutput = pipe
  p.standardError = pipe
  try p.run()
  let deadline = Date().addingTimeInterval(timeout)
  while p.isRunning && Date() < deadline { Thread.sleep(forTimeInterval: 0.01) }
  if p.isRunning {
    p.interrupt()
    Thread.sleep(forTimeInterval: 0.1)
    if p.isRunning { kill(p.processIdentifier, SIGKILL) }
    p.waitUntilExit()
    throw ShellError.message("Operation timed out; retry after checking the device.")
  }
  p.waitUntilExit()
  pipe.fileHandleForReading.readabilityHandler = nil
  let remaining = pipe.fileHandleForReading.readDataToEndOfFile()
  lock.lock()
  data.append(remaining.prefix(max(0, 65536 - data.count)))
  let text = String(decoding: data, as: UTF8.self)
  lock.unlock()
  return ToolResult(code: p.terminationStatus, text: text)
}

func decodeFrame(_ data: Data) throws -> CGImage {
  let header = Data("P6\n320 240\n255\n".utf8)
  guard data.count == header.count + 320 * 240 * 3, data.starts(with: header),
    let provider = CGDataProvider(data: data.dropFirst(header.count) as CFData),
    let image = CGImage(
      width: 320, height: 240, bitsPerComponent: 8, bitsPerPixel: 24, bytesPerRow: 960,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue), provider: provider,
      decode: nil, shouldInterpolate: false, intent: .defaultIntent)
  else { throw ShellError.message("Incomplete LCD frame") }
  return image
}
struct SessionStopOutcome {
  // Forced means this binding had to terminate/kill after its graceful deadline.
  // A normal process exit alone does not establish adapter protocol cleanup.
  let forcedProcessExit: Bool
  let protocolCleanupConfirmed: Bool
  var continuationSafe: Bool { !forcedProcessExit && protocolCleanupConfirmed }
  var sessionEndReason: String {
    if forcedProcessExit { return "forced_stop" }
    return protocolCleanupConfirmed ? "user_stop" : "protocol_cleanup_failed"
  }
}
final class EmulatorSession {
  let process = Process(), run: URL
  private(set) var adapter: USBAdapterEndpoint?
  var chipsoftPort: String? { adapter?.kind == .chipsoft ? adapter?.port : nil }
  var connected: Bool { adapter != nil }
  let mode: ChipsoftSessionMode
  private(set) var stopOutcome: SessionStopOutcome?
  /// Owned OBDLink MX+ native bridge; nil for offline and USB sessions.
  private(set) var mx: MacMXFirmwareBridge?
  /// The MX+ worker (Android libtech2_mx parity) reads encoder bytes from native-key.txt.
  var mailbox: URL { run.appendingPathComponent(mx == nil ? "interactive-key.txt" : "native-key.txt") }
  var canAcceptMenuKey: Bool {running && queue.isEmpty && !fm.fileExists(atPath:mailbox.path)}
  private let pipe = Pipe(), log: FileHandle, lock = NSLock()
  private var bytes = 0, console = Data(), queue: [String] = [], frameDate: Date?
  // MX+ outcome evidence latched per line as output streams (Android readLogs parity),
  // so console truncation or late delivery cannot hide the read-gate rejection.
  private var partialLine = Data(), rejectedLatched = false, outputEnded = false
  var mxPolicyRejected: Bool { lock.lock(); defer { lock.unlock() }; return rejectedLatched }
  var running: Bool { process.isRunning }
  var mdiPhase = ""
  func updateMDIIdentity() {
    if adapter?.kind == .mdi,let updated=try? RunAdapterBinding.read(run:run),updated.port==adapter?.port {adapter=updated}
  }
  static func gracefulStopBudget(for kind: USBAdapterKind?) -> TimeInterval { kind == .mdi ? 60 : kind == .nano ? 45 : 12 }
  static let mxInstructionLimit = "50000000000"
  var consoleText: String {
    lock.lock()
    defer { lock.unlock() }
    return String(decoding: console, as: UTF8.self)
  }
  init(binary: URL, store: FirmwareStore, chipsoftPort: String? = nil, adapter: USBAdapterEndpoint? = nil, mode: ChipsoftSessionMode = .readOnly, freshSecurity: Bool = false, mxBridge: MacMXFirmwareBridge? = nil) throws {
    guard chipsoftPort == nil || adapter == nil else { throw ShellError.message("Choose one adapter connection.") }
    guard mxBridge == nil || (chipsoftPort == nil && adapter == nil && mode == .readOnly && !freshSecurity) else {
      throw ShellError.message("MX+ sessions are exclusive, read-only original-firmware sessions.")
    }
    self.mx = mxBridge
    self.adapter = adapter ?? chipsoftPort.map { USBAdapterEndpoint(kind: .chipsoft, port: $0) }
    self.mode=mode
    try store.validateCore()
    guard let selected = store.selected else {
      throw ShellError.message("Choose and download a firmware language first.")
    }
    try store.activate(selected)
    run = store.state.appendingPathComponent(
      "runs/" + ISO8601DateFormatter().string(from: Date()).replacingOccurrences(of: ":", with: "-")
        + "-" + UUID().uuidString.prefix(8))
    try fm.createDirectory(at: run, withIntermediateDirectories: true)
    let card = run.appendingPathComponent("card.bin")
    try fm.copyItem(at: store.card(selected), to: card)
    if freshSecurity {
      let handle=try FileHandle(forUpdating:card)
      defer {try? handle.close()}
      try handle.seek(toOffset:SecurityStatus.offset)
      try handle.write(contentsOf:Data(repeating:255,count:SecurityStatus.size));try handle.synchronize()
    } else {try SecurityWorkspace.restore(state: store.state, firmware: selected.id, to: card)}
    // Bind DTC identity to this connection only; offline never inherits a saved VIN.
    var dtcIdentity: [String:Any] = ["adapter":self.adapter?.kind.rawValue ?? "offline (no vehicle connected)"]
    if self.adapter != nil,self.adapter?.kind != .mdi,let data=try? Data(contentsOf:store.state.appendingPathComponent("vehicle-status.json")),
       let value=try? JSONSerialization.jsonObject(with:data) as? [String:String] {
      dtcIdentity["vin"]=value["vin"];dtcIdentity["observed_utc"]=value["observed_utc"]
      dtcIdentity["vehicle_description"]=value["description"]
    }
    if let mx = mxBridge {
      // Bind only this connection's fresh discovery; never a saved USB-era VIN.
      dtcIdentity = ["adapter":"OBDLink MX+"]
      if let vin = mx.vehicle["vin"] as? String { dtcIdentity["vin"] = vin }
    }
    try JSONSerialization.data(withJSONObject:dtcIdentity).write(to:run.appendingPathComponent("dtc-context.json"),options:.atomic)
    // Required, per-run provenance survives app relaunch and binds security cleanup.
    try RunAdapterBinding.write(self.adapter, to: run)
    let info=Bundle.main.infoDictionary ?? [:]
    var diagnosticBinding:[String:Any] = ["app_version":info["CFBundleShortVersionString"] as? String ?? "development",
      "architecture":shellArchitecture,"build":info["OpenSAABBuild"] as? String ?? "unavailable",
      "family":self.adapter?.kind == .chipsoft ? "chipsoft" : self.adapter?.kind == .nano ? "nano" : self.adapter?.kind == .mdi ? "mdi" : "offline",
      "transport":self.adapter == nil ? "offline" : self.adapter?.kind == .mdi ? "usb_rndis" : "usb_cdc",
      "session_mode":self.adapter == nil ? "offline" : mode == .fullNative ? "full_native" : mode == .seeds ? "security_seeds" : "read_only",
      "last_operation":self.adapter == nil ? "original_menus" : "connection"]
    if mxBridge != nil {
      diagnosticBinding["family"] = "obdlink_mxp"; diagnosticBinding["transport"] = "bluetooth_rfcomm"
      diagnosticBinding["native_api"] = "IOBluetoothRFCOMMChannel"; diagnosticBinding["session_mode"] = "mxp_native_read"
      diagnosticBinding["engine"] = "tech2-mx"
    }
    try JSONSerialization.data(withJSONObject:diagnosticBinding).write(to:run.appendingPathComponent("connection-evidence.json"),options:.atomic)

    let logURL = run.appendingPathComponent("console.log")
    fm.createFile(atPath: logURL.path, contents: nil)
    log = try FileHandle(forWritingTo: logURL)
    process.executableURL = binary
    process.arguments = [
      "--interactive-headless", "--research-harness", "--candi-native-link", "--candi-on-demand",
      "--max-insns", "50000000000", "--output-dir", run.path, "--boot",
      store.runtime.appendingPathComponent("eprom.bin").path, "--opsys",
      store.runtime.appendingPathComponent("opsys.dwn").path, "--candi-firmware",
      store.runtime.appendingPathComponent("candi.bin").path, card.path,
    ]
    if let endpoint = self.adapter,endpoint.kind != .mdi {
      process.arguments! += [endpoint.kind == .chipsoft ? "--candi-chipsoft-port" : "--candi-nano-port", endpoint.port, "--harness-target", "native-manual"]
      process.arguments! += mode.arguments(for: endpoint.kind)
    }
    if let endpoint=self.adapter,endpoint.kind == .mdi {
      process.executableURL=binary.deletingLastPathComponent().appendingPathComponent("opensaab-mdi")
      process.arguments=["--target",endpoint.port,"--directory",run.path,"--runtime",store.runtime.path,"--card",card.path,"--mode",mode == .fullNative ? "full" : mode == .seeds ? "seeds" : "read"]
    }
    if let mx = mxBridge {
      // Android preview.18 MX+ launch (MainActivity.runSession) on the same engine
      // source: eager CANdi, native manual input, no automatic replay.
      process.executableURL = binary.deletingLastPathComponent().appendingPathComponent("tech2-mx")
      process.arguments = [
        "--test-harness", "--boot", store.runtime.appendingPathComponent("eprom.bin").path,
        "--max-insns", Self.mxInstructionLimit, "--output-dir", run.path,
        "--candi-native-link", "--candi-firmware", store.runtime.appendingPathComponent("candi.bin").path,
        "--opsys", store.runtime.appendingPathComponent("opsys.dwn").path,
        "--candi-obdlink-token", mx.token, "--harness-target", "native-manual", card.path,
      ]
    }
    process.currentDirectoryURL = run
    process.environment = mode.environment(ProcessInfo.processInfo.environment, connected: self.adapter?.kind == .chipsoft)
    // Nano permissions come only from the explicit selected mode/CLI, never inherited environment.
    process.environment!["TECH2_NANO_FULL_NATIVE"] = "0"
    process.environment!["TECH2_NANO_KEY_STATUS"] = "0"
    // Passive, bounded CPU/memory samples for support reports (same schema as Android sessions).
    process.environment!["OPENSAAB_PERFORMANCE_DIR"] = run.path
    process.standardOutput = pipe
    process.standardError = pipe
    pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
      let data = handle.availableData
      guard let self = self else { return }
      self.lock.lock()
      defer { self.lock.unlock() }
      if self.bytes < 16 * 1024 * 1024 {
        let chunk = data.prefix(16 * 1024 * 1024 - self.bytes)
        try? self.log.write(contentsOf: chunk)
        self.bytes += chunk.count
      }
      self.console.append(data)
      if self.console.count > 24000 { self.console = self.console.suffix(20000) }
      if data.isEmpty { self.outputEnded = true; handle.readabilityHandler = nil }
      if self.mx != nil && !self.rejectedLatched {
        self.partialLine.append(data)
        while let newline = self.partialLine.firstIndex(of: 10) {
          let line = String(decoding: self.partialLine[self.partialLine.startIndex..<newline], as: UTF8.self)
          self.partialLine.removeSubrange(self.partialLine.startIndex...newline)
          if MxSessionOutcome.rejected(line) { self.rejectedLatched = true; break }
        }
        if self.outputEnded, MxSessionOutcome.rejected(String(decoding: self.partialLine, as: UTF8.self)) { self.rejectedLatched = true }
        if self.partialLine.count > 4096 { self.partialLine = Data(self.partialLine.suffix(512)) }
      }
    }
    do { try process.run() } catch {
      pipe.fileHandleForReading.readabilityHandler = nil
      throw error
    }
    let metadata: [String: Any] = [
      "pid": process.processIdentifier, "mode": self.adapter == nil ? "offline" : mode.rawValue, "architecture": shellArchitecture, "run": run.path,
      "adapter": mxBridge != nil ? "obdlink_mxp" : self.adapter?.kind.rawValue ?? "offline",
      "firmware": selected.id,
      "seed_collection": self.adapter != nil && mode == .seeds,
    ]
    // Failure to write convenience metadata must not orphan the child.
    try? JSONSerialization.data(withJSONObject: metadata, options: [.prettyPrinted, .sortedKeys])
      .write(to: store.state.appendingPathComponent("latest-session.json"), options: .atomic)
  }
  func key(_ code: Int) {
    if running && queue.count < 16 {
      // MX+ worker accepts encoder bytes only: Enter is 0x10 (Android preview.17 fix).
      queue.append(code == 16 && mx == nil ? "enter" : String(format: "0x%02x", code))
    }
  }
  func pump() throws {
    let mailbox = self.mailbox
    if running && !queue.isEmpty && !fm.fileExists(atPath: mailbox.path) {
      try Data((queue.removeFirst() + "\n").utf8).write(to: mailbox, options: .atomic)
    }
  }
  func frame() throws -> CGImage? {
    let file = run.appendingPathComponent("live.ppm")
    let stamp = try file.resourceValues(forKeys: [.contentModificationDateKey])
      .contentModificationDate
    if stamp == frameDate { return nil }
    let image = try decodeFrame(Data(contentsOf: file))
    frameDate = stamp
    return image
  }
  @discardableResult func stop() -> Bool {
    queue.removeAll()
    if let outcome = stopOutcome { return outcome.continuationSafe }
    if !running {
      if mx != nil { releaseMX() }
      return finishStop(forced: false)
    }
    if adapter?.kind == .mdi { try? Data("stop\n".utf8).write(to:run.appendingPathComponent("mdi-stop"),options:.atomic) }
    try? Data("stop\n".utf8).write(to: mailbox, options: .atomic)
    // Nano can finish its bounded startup and six 3s shutdown exchanges on stop.
    let deadline = Date().addingTimeInterval(Self.gracefulStopBudget(for: adapter?.kind))
    while running && Date() < deadline { Thread.sleep(forTimeInterval: 0.01) }
    let clean = !running
    if running {
      process.terminate()
      let end = Date().addingTimeInterval(1)
      while running && Date() < end { Thread.sleep(forTimeInterval: 0.01) }
      if running { kill(process.processIdentifier, SIGKILL) }
    }
    process.waitUntilExit()
    if mx != nil { releaseMX() }
    return finishStop(forced: !clean)
  }
  /// MX+ end of session: release the native owner, then let the console reach EOF so
  /// outcome classification sees the final line. Call off the main thread.
  private func releaseMX() {
    guard let mx else { return }
    mx.close()
    guard !Thread.isMainThread else { return }
    mx.waitForRelease(seconds: 8)
    let deadline = Date().addingTimeInterval(2)
    while Date() < deadline {
      lock.lock(); let ended = outputEnded; lock.unlock()
      if ended { break }
      Thread.sleep(forTimeInterval: 0.02)
    }
  }
  private func finishStop(forced: Bool) -> Bool {
    let outcome = SessionStopOutcome(forcedProcessExit: forced,
      protocolCleanupConfirmed: mx != nil ? mx!.bluetoothClosed : adapter?.kind == .mdi ? MdiCleanup.confirmed(run:run) : adapter?.kind != .nano || NanoCleanup.confirmed(run: run))
    let detail:[String:Any] = ["child_exit_code":Int(process.terminationStatus),"forced_termination":forced,
      "usb_release":adapter?.kind == .mdi ? (MdiCleanup.confirmed(run:run) ? "succeeded":"failed") : "unknown","protocol_cleanup":adapter?.kind == .mdi ? (MdiCleanup.confirmed(run:run) ? "succeeded":"failed") : adapter?.kind == .nano ? (NanoCleanup.confirmed(run:run) ? "succeeded":"failed") : "unknown"]
    var evidence = detail
    if let mx { evidence["bluetooth_release"] = mx.released ? (mx.bluetoothClosed ? "succeeded" : "failed") : "unconfirmed"; evidence["usb_release"] = "not_applicable" }
    try? JSONSerialization.data(withJSONObject:evidence).write(to:run.appendingPathComponent("cleanup-evidence.json"),options:.atomic)
    stopOutcome = outcome
    return outcome.continuationSafe
  }
  deinit {
    if running { kill(process.processIdentifier, SIGKILL) }
    mx?.close()
    pipe.fileHandleForReading.readabilityHandler = nil
    try? log.close()
  }
}
