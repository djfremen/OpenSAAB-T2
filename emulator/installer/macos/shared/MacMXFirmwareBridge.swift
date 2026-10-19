// SPDX-License-Identifier: MPL-2.0
import Darwin
import Foundation
import IOBluetooth
import Security

/// App-owned OBDLink MX+ Bluetooth Classic owner plus the authenticated loopback
/// byte bridge used by the original-firmware worker (`tech2-mx`).
///
/// This is the macOS binding of Android preview.18 `MxNativeBridge`: one native
/// owner, one 32-hex token, and the same line protocol on 127.0.0.1:35676
/// (HELLO token / READ / WRITE command / INTERRUPT / CLOSE). The Rust worker owns
/// every STN command, raw CAN frame, flow-control frame and request policy; this
/// class only moves bytes and reports transport state. No replay, no fallback.
final class MacMXFirmwareBridge {
  static let port: UInt16 = 35676
  static let readyRead = "READY obdlink-native-read"
  let token: String
  let deviceName: String
  let deviceAddress: String
  /// Fresh VIN/identity from the verified discovery that preceded this session.
  var vehicle: [String: Any] = [:]
  /// Called on the main thread once the adapter's native close is known.
  var onReleased: ((Bool) -> Void)?
  /// Run directory for the bridge receipt (intel.10 parity: mx-bridge-result-private.json).
  var evidenceDirectory: URL?

  private let transport: MacSPPTransport
  private let cond = NSCondition()
  private var queue: [[String: Any]] = []
  private var transportError: String?
  private var closedPacket: [String: Any]?
  private var listener: Int32 = -1
  private var client: Int32 = -1
  private var wake: [Int32] = [-1, -1]
  private var ready = false, closing = false, closeRequested = false, serving = false
  private var channelClosed: Bool?, workerRequestedClose = false
  private var bytesWritten = 0, bytesReceived = 0, writes = 0
  private let opened = Date()
  private var readyCallback: ((Result<Void, Error>) -> Void)?

  var bluetoothClosed: Bool {
    cond.lock(); defer { cond.unlock() }
    return closedPacket?["bluetooth_closed"] as? Bool == true
  }
  var released: Bool {
    cond.lock(); defer { cond.unlock() }
    return closedPacket != nil
  }

  /// Printable ASCII only, at most 128 characters: the Android transport bound.
  static func acceptsWrite(_ command: String) -> Bool {
    !command.isEmpty && command.utf8.count <= 128 && command.utf8.allSatisfy { $0 >= 0x20 && $0 <= 0x7e }
  }

  /// Must be created on the main thread (IOBluetooth delegates use the main run loop).
  init(device: IOBluetoothDevice) throws {
    let name = device.name ?? ""
    guard name.hasPrefix("OBDLink MX+") else { throw ShellError.message("Select the OBDLink MX+ for this firmware session.") }
    guard let address = device.addressString else { throw ShellError.message("OBDLink MX+ address unavailable.") }
    var random = [UInt8](repeating: 0, count: 16)
    guard SecRandomCopyBytes(kSecRandomDefault, random.count, &random) == errSecSuccess else {
      throw ShellError.message("Secure random token unavailable.")
    }
    token = random.map { String(format: "%02x", $0) }.joined()
    deviceName = name; deviceAddress = address
    transport = MacSPPTransport(device: device, allowed: MacMXFirmwareBridge.acceptsWrite)
    listener = try Self.bindLoopback()
    guard pipe(&wake) == 0 else { Darwin.close(listener); listener = -1; throw ShellError.message("MX+ bridge wake pipe unavailable.") }
  }

  private static func bindLoopback() throws -> Int32 {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    guard fd >= 0 else { throw ShellError.message("MX+ bridge socket unavailable.") }
    var one: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, socklen_t(MemoryLayout<Int32>.size))
    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_port = port.bigEndian
    address.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))
    let bound = withUnsafePointer(to: &address) {
      $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) }
    }
    guard bound == 0, listen(fd, 1) == 0 else {
      Darwin.close(fd)
      throw ShellError.message("MX+ bridge port 127.0.0.1:\(port) is busy. Stop the other MX+ session first.")
    }
    return fd
  }

  /// Opens the native RFCOMM channel. `completion` runs on the main thread once the
  /// adapter is ready (then the loopback server accepts exactly one worker) or failed.
  func open(_ completion: @escaping (Result<Void, Error>) -> Void) {
    readyCallback = completion
    transport.open { [weak self] packet in self?.handle(packet) }
  }

  private func handle(_ packet: [String: Any]) {
    let type = packet["type"] as? String ?? ""
    cond.lock()
    switch type {
    case "ready":
      if !ready && !closing { ready = true }
    case "channel_closed":
      if channelClosed == nil { channelClosed = packet["ok"] as? Bool == true }
    case "closed":
      closedPacket = packet
      if channelClosed == nil { channelClosed = packet["channel_close_result"] as? Int32 == kIOReturnSuccess }
    case "error":
      if transportError == nil { transportError = packet["message"] as? String ?? "Bluetooth transport failed" }
      queue.append(packet)
    case "rx", "write_completed":
      if queue.count < 4096 { queue.append(packet) } else if transportError == nil { transportError = "MX+ receive queue exceeded limit" }
    default: break
    }
    let becameReady = type == "ready" && ready && readyCallback != nil
    let failedEarly = !ready && (type == "error" || type == "closed")
    cond.broadcast()
    cond.unlock()
    if becameReady {
      let callback = readyCallback; readyCallback = nil
      cond.lock(); serving = true; cond.unlock()
      Thread.detachNewThread { [self] in serve() }
      callback?(.success(()))
    } else if failedEarly, let callback = readyCallback {
      readyCallback = nil
      callback(.failure(ShellError.message(packet["message"] as? String ?? (packet["error"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "OBDLink MX+ connection failed.")))
    }
    if type == "closed" {
      transport.shutdown()
      writeReceipt(packet)
      onReleased?(packet["bluetooth_closed"] as? Bool == true)
    }
  }

  // MARK: Loopback server (one worker, one session)

  private func serve() {
    defer { finishServing() }
    var pollers = [pollfd(fd: listener, events: Int16(POLLIN), revents: 0), pollfd(fd: wake[0], events: Int16(POLLIN), revents: 0)]
    guard poll(&pollers, 2, 12_000) > 0, pollers[1].revents == 0, pollers[0].revents & Int16(POLLIN) != 0 else { return }
    cond.lock()
    if closing { cond.unlock(); return }
    let accepted = accept(listener, nil, nil)
    if accepted >= 0 { client = accepted }
    Darwin.close(listener); listener = -1
    cond.unlock()
    guard accepted >= 0 else { return }
    var timeout = timeval(tv_sec: 5, tv_usec: 0)
    setsockopt(accepted, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
    var nosig: Int32 = 1
    setsockopt(accepted, SOL_SOCKET, SO_NOSIGPIPE, &nosig, socklen_t(MemoryLayout<Int32>.size))
    do {
      guard try line(accepted) == "HELLO " + token else { throw ShellError.message("MX+ bridge authentication rejected") }
      try reply(accepted, Self.readyRead)
      while true {
        let request = try line(accepted)
        if request == "CLOSE" {
          cond.lock(); workerRequestedClose = true; cond.unlock()
          requestNativeClose()
          // The worker reads this reply with a 3 s timeout (SocketUsb). Like Android's
          // socket close, confirm the RFCOMM channel close here; the baseband release
          // is confirmed separately by the app before a stop counts as clean.
          guard waitForChannelClose(seconds: 2.5) else { throw ShellError.message("RFCOMM channel close unconfirmed") }
          try reply(accepted, "CLOSED")
          return
        } else if request == "READ" {
          try reply(accepted, try nextReceive())
        } else if request == "INTERRUPT" {
          try nativeWrite(["op": "interrupt"])
          try reply(accepted, "WRITTEN")
        } else if request.hasPrefix("WRITE ") {
          let command = String(request.dropFirst(6))
          guard Self.acceptsWrite(command) else { throw ShellError.message("Invalid native transport write") }
          try nativeWrite(["op": "write", "command": command])
          try reply(accepted, "WRITTEN")
        } else {
          throw ShellError.message("Unknown MX byte effect")
        }
      }
    } catch {
      NSLog("OpenSAAB MX+ native byte bridge ended: %@", String(describing: error))
    }
  }

  /// READ: skip write acknowledgements; return one rx packet or EMPTY after 10 ms.
  private func nextReceive() throws -> String {
    cond.lock(); defer { cond.unlock() }
    while true {
      if let error = transportError, queue.first?["type"] as? String != "rx" { throw ShellError.message(error) }
      if let packet = queue.first {
        queue.removeFirst()
        let type = packet["type"] as? String
        if type == "write_completed" { continue }
        guard type == "rx", var bytes = packet["bytes"] as? [UInt8] else { throw ShellError.message("Unexpected Bluetooth event") }
        if bytes.count > 1024 {
          queue.insert(["type": "rx", "bytes": Array(bytes[1024...])], at: 0)
          bytes = Array(bytes[..<1024])
        }
        bytesReceived += bytes.count
        let json = try JSONSerialization.data(withJSONObject: ["type": "rx", "bytes": bytes.map { Int($0) }])
        return String(decoding: json, as: UTF8.self)
      }
      if closedPacket != nil { throw ShellError.message("Bluetooth disconnected") }
      if !cond.wait(until: Date().addingTimeInterval(0.010)) && queue.isEmpty { return "EMPTY" }
    }
  }

  private func nativeWrite(_ request: [String: Any]) throws {
    cond.lock(); let failed = transportError ?? (closedPacket != nil ? "Bluetooth disconnected" : nil); cond.unlock()
    if let failed { throw ShellError.message(failed) }
    let done = DispatchSemaphore(value: 0)
    DispatchQueue.main.async { [self] in transport.send(request); done.signal() }
    if done.wait(timeout: .now() + 2) == .timedOut {
      cond.lock(); if transportError == nil { transportError = "Bluetooth write timed out" }; cond.unlock()
      requestNativeClose()
      throw ShellError.message("Bluetooth write timed out")
    }
    cond.lock(); let error = transportError
    if error == nil { writes += 1; bytesWritten += ((request["command"] as? String)?.utf8.count ?? 0) + 1 }
    cond.unlock()
    if let error { throw ShellError.message(error) }
  }

  private func requestNativeClose() {
    cond.lock()
    let first = !closeRequested; closeRequested = true; closing = true
    cond.unlock()
    if first { DispatchQueue.main.async { [self] in transport.send(["op": "close"]) } }
  }

  private func finishServing() {
    cond.lock()
    if client >= 0 { Darwin.close(client); client = -1 }
    if listener >= 0 { Darwin.close(listener); listener = -1 }
    serving = false
    cond.unlock()
    requestNativeClose()
  }

  /// Waits for the adapter's close confirmation. Never call on the main thread:
  /// the confirmation is delivered there.
  @discardableResult func waitForRelease(seconds: TimeInterval) -> Bool {
    precondition(!Thread.isMainThread, "MX+ release wait would block its own delivery")
    let deadline = Date().addingTimeInterval(seconds)
    cond.lock(); defer { cond.unlock() }
    while closedPacket == nil && cond.wait(until: deadline) {}
    return closedPacket != nil
  }

  /// True once the RFCOMM channel close succeeded (or the full release confirmed it).
  private func waitForChannelClose(seconds: TimeInterval) -> Bool {
    let deadline = Date().addingTimeInterval(seconds)
    cond.lock(); defer { cond.unlock() }
    while channelClosed == nil && cond.wait(until: deadline) {}
    return channelClosed == true
  }

  private func writeReceipt(_ packet: [String: Any]) {
    guard let directory = evidenceDirectory else { return }
    cond.lock()
    let receipt: [String: Any] = ["schema": 1, "adapter": "OBDLink MX+", "native_api": "IOBluetoothRFCOMMChannel",
      "status": packet["bluetooth_closed"] as? Bool == true && (packet["error"] as? String ?? "").isEmpty ? "closed" : "failed",
      "bluetooth_closed": packet["bluetooth_closed"] as? Bool ?? false, "error": packet["error"] as? String ?? "",
      "channel_close_result": packet["channel_close_result"] as? Int32 ?? -1, "device_close_result": packet["device_close_result"] as? Int32 ?? -1,
      "channel_closed": channelClosed ?? false, "worker_requested_close": workerRequestedClose,
      "transport_error": transportError ?? "", "writes": writes, "bytes_written": bytesWritten, "bytes_received": bytesReceived,
      "owned_seconds": Int(Date().timeIntervalSince(opened)), "CAN_acknowledgement_inferred": false, "automatic_replay": false]
    cond.unlock()
    let file = directory.appendingPathComponent("mx-bridge-result-private.json")
    if let data = try? JSONSerialization.data(withJSONObject: receipt, options: [.prettyPrinted, .sortedKeys]),
       fm.createFile(atPath: file.path, contents: data, attributes: [.posixPermissions: 0o600]) == false {
      NSLog("OpenSAAB MX+ bridge receipt could not be written")
    }
  }

  /// Ends ownership from the app side (stop, launch failure, process exit). Safe to repeat.
  func close() {
    cond.lock()
    closing = true
    if serving {
      // Wake the server thread (accept wait or blocked read); it closes its own descriptors.
      if client >= 0 { shutdown(client, SHUT_RDWR) }
      if wake[1] >= 0 { var byte: UInt8 = 1; _ = write(wake[1], &byte, 1) }
    } else if listener >= 0 { Darwin.close(listener); listener = -1 }
    cond.unlock()
    requestNativeClose()
  }

  private func line(_ fd: Int32) throws -> String {
    var bytes: [UInt8] = []
    var byte: UInt8 = 0
    while true {
      let count = read(fd, &byte, 1)
      if count == 0 { throw ShellError.message("MX bridge peer closed") }
      if count < 0 { throw ShellError.message(errno == EAGAIN ? "MX bridge request timeout" : "MX bridge read failed") }
      if byte == 10 { break }
      guard bytes.count < 256 else { throw ShellError.message("MX bridge request too large") }
      bytes.append(byte)
    }
    guard let text = String(bytes: bytes, encoding: .ascii) else { throw ShellError.message("MX bridge request not ASCII") }
    return text
  }

  private func reply(_ fd: Int32, _ value: String) throws {
    let data = Array((value + "\n").utf8)
    var sent = 0
    while sent < data.count {
      let count = data[sent...].withUnsafeBytes { write(fd, $0.baseAddress, $0.count) }
      guard count > 0 else { throw ShellError.message("MX bridge reply failed") }
      sent += count
    }
  }

  deinit {
    if client >= 0 { Darwin.close(client) }
    if listener >= 0 { Darwin.close(listener) }
    for fd in wake where fd >= 0 { Darwin.close(fd) }
  }
}

/// Classifies the existing Rust MX+ request gate outcome (Android preview.18
/// `MxSessionOutcome`). It never grants or transmits a request.
enum MxSessionOutcome {
  static let rejectedMarker = "Native adapter stopped: STN original-firmware session policy rejected request"
  static func rejected(_ console: String) -> Bool { console.contains(rejectedMarker) }
  static func unsupported(mxSession: Bool, rejected: Bool, forced: Bool, launchFailed: Bool) -> Bool {
    mxSession && rejected && !forced && !launchFailed
  }
  static let readSummary = "MX+ read-only session · Read codes and key status · Code clearing unavailable"
  static let unavailableMessage = "Operation unavailable in this MX+ session. The blocked request was not sent to the vehicle. Code clearing is not supported in this build."
}
