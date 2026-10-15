// SPDX-License-Identifier: MPL-2.0
import Foundation
import Darwin

// One serial JSON-lines worker owns the same Rust sessions linked into the phone apps.
// The native shell receives effects; it contains no command sequence or VIN/DTC decoder.
final class MacConnectionWorker {
    private let process = Process(), input = Pipe(), output = Pipe(), errors = Pipe()
    private var pending = Data()
    init(binary: URL, root: URL) throws {
        process.executableURL = binary; process.arguments = [root.path]
        process.standardInput = input; process.standardOutput = output; process.standardError = errors
        try process.run()
    }
    func call(_ value: [String:Any]) throws -> [String:Any] {
        guard process.isRunning else { throw ShellError.message("Shared Bluetooth worker stopped") }
        let bytes = try JSONSerialization.data(withJSONObject:value)
        guard bytes.count < 300000 else { throw ShellError.message("Worker request exceeded limit") }
        try input.fileHandleForWriting.write(contentsOf:bytes + Data([10]))
        let deadline = Date().addingTimeInterval(2)
        while Date() < deadline {
            if let end = pending.firstIndex(of:10) {
                let line = Data(pending[..<end]); pending.removeSubrange(...end)
                guard let result = try JSONSerialization.jsonObject(with:line) as? [String:Any] else { throw ShellError.message("Invalid shared Bluetooth response") }
                return result
            }
            var pollFD = pollfd(fd:output.fileHandleForReading.fileDescriptor, events:Int16(POLLIN), revents:0)
            guard poll(&pollFD,1,100) >= 0 else { throw ShellError.message("Shared Bluetooth worker read failed") }
            if pollFD.revents & Int16(POLLIN) != 0 {
                var buffer = [UInt8](repeating:0, count:4096)
                let count = Darwin.read(pollFD.fd, &buffer, buffer.count)
                guard count > 0, pending.count + count <= 512000 else { throw ShellError.message("Shared Bluetooth worker closed or exceeded limit") }
                pending.append(contentsOf:buffer.prefix(count))
            } else if pollFD.revents & Int16(POLLHUP | POLLERR) != 0 { throw ShellError.message("Shared Bluetooth worker disconnected") }
        }
        throw ShellError.message("Shared Bluetooth worker response timed out")
    }
    func close() {
        try? input.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(0.5)
        while process.isRunning && Date() < deadline { Thread.sleep(forTimeInterval:0.01) }
        if process.isRunning { process.terminate() }
        try? output.fileHandleForReading.close(); try? errors.fileHandleForReading.close()
    }
    deinit { close() }
}

final class MacBluetoothHTTPS: NSObject, URLSessionDataDelegate {
    private var session: URLSession?, task: URLSessionDataTask?, bytes = Data(), response: HTTPURLResponse?
    private var limit = 0, callback: ((Data?,Int)->Void)?
    func run(url: URL, limit: Int, timeout: Double, callback: @escaping (Data?,Int)->Void) {
        guard url.scheme == "https", limit > 0 else { callback(nil,0); return }
        self.limit = limit; self.callback = callback
        let config = URLSessionConfiguration.ephemeral
        config.httpCookieStorage = nil; config.urlCredentialStorage = nil; config.urlCache = nil
        config.timeoutIntervalForRequest = 7; config.timeoutIntervalForResource = timeout
        session = URLSession(configuration:config, delegate:self, delegateQueue:nil)
        var request = URLRequest(url:url); request.setValue("application/json",forHTTPHeaderField:"Accept")
        task = session?.dataTask(with:request); task?.resume()
    }
    func cancel() { task?.cancel() }
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?)->Void) { completionHandler(nil) }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition)->Void) {
        guard let http = response as? HTTPURLResponse, http.url?.scheme == "https", response.expectedContentLength <= Int64(limit) else { completionHandler(.cancel); return }
        self.response = http; completionHandler(.allow)
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) { if bytes.count + data.count > limit { dataTask.cancel() } else { bytes.append(data) } }
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        let completion = callback; callback = nil
        let data = error == nil ? bytes : nil, status = error == nil ? response?.statusCode ?? 0 : 0
        session.finishTasksAndInvalidate(); self.task = nil; self.session = nil
        DispatchQueue.main.async { completion?(data,status) }
    }
}

final class MacBluetoothWorkflow {
    static private weak var active: MacBluetoothWorkflow?
    private let root: URL, binary: URL, mode: String
    private var worker: MacConnectionWorker?, link: MacBluetoothLink?, timer: Timer?, http: MacBluetoothHTTPS?
    private var pending: [String:Any]?, profile: [String:Any] = [:], closing = false, finished = false
    private var commandAttempts: [String:Int] = [:], commandWrites: [String:Int] = [:]
    private var trace = "", expectedCommand: String?, connectionStopping = false
    private let runID = UUID().uuidString, linkFactory: ((@escaping (String)->Bool)->MacBluetoothLink)?
    private let lookupEffect: ((URL,Int,Double,@escaping (Data?,Int)->Void)->Void)?
    var onDevices: (([[String:String]])->Void)?, onStatus: ((String)->Void)?
    var onComplete: (([String:Any])->Void)?, onVehicle: (([String:Any])->Void)?
    init(root: URL, binary: URL, mode: String = "startup", linkFactory: ((@escaping (String)->Bool)->MacBluetoothLink)? = nil, lookupEffect: ((URL,Int,Double,@escaping (Data?,Int)->Void)->Void)? = nil) {
        self.root = root; self.binary = binary; self.mode = mode; self.linkFactory = linkFactory
        self.lookupEffect = lookupEffect
    }
    func start() throws {
        guard Self.active == nil else { throw ShellError.message("Finish the active Bluetooth workflow before connecting") }
        Self.active = self
        do {
            worker = try MacConnectionWorker(binary:binary, root:root)
            record("start", ["mode":mode, "runtime":try call("manifest",[:])])
            _ = try call("vlinker", ["op":"configure", "mode":mode])
            var owner = ""
            if mode == "startup" {
                let begun = try call("connection", ["op":"begin", "firmware_ready":true])
                guard begun["state"] as? String == "selecting" else { throw ShellError.message(begun["message"] as? String ?? "Connection unavailable") }
                let selected = try call("connection", ["op":"select_adapter", "adapter":"bluetooth_mic", "transport_available":true])
                guard selected["effect"] as? String == "discover_vin" else { throw ShellError.message("Bluetooth discovery unavailable") }
                owner = selected["attempt_id"] as? String ?? ""
            }
            var claim: [String:Any] = ["op":"claim"]; if !owner.isEmpty { claim["owner"] = owner }
            let claimed = try call("vlinker",claim)
            guard claimed["state"] as? String == "idle" else { throw ShellError.message(claimed["error"] as? String ?? "Bluetooth workflow ownership unavailable") }
            let allowed: (String)->Bool = { [weak self] in self?.expectedCommand == $0 }
            let transport = linkFactory?(allowed) ?? MacAdapterBluetoothLink(allowed:allowed); link = transport
            timer = Timer.scheduledTimer(withTimeInterval:0.2, repeats:true) { [weak self] _ in self?.core(["op":"tick"]) }
            transport.open { [weak self] in self?.message($0) }
        } catch { worker?.close(); worker = nil; Self.active = nil; throw error }
    }
    func select(_ id: String) { guard !closing, !finished else { return }; link?.send(["op":"select", "id":id]) }
    func find() { guard !closing, !finished else { return }; link?.send(["op":"scan"]) }
    func stop() {
        guard !finished else {return}
        if mode == "startup" {connectionStopping = true; _ = try? call("connection",["op":"cancel"])}
        if link == nil {
            http?.cancel(); http = nil
            if mode == "startup", let value = try? call("connection",["op":"cancel"]) {onVehicle?(value)}
            complete(["status":"cancelled", "message":"Connection stopped · No vehicle connection", "bluetooth_closed":true]); return
        }
        core(["op":"cancel", "message":"Bluetooth read cancelled; no automatic replay"])
        if !closing {close()}
    }
    private func call(_ scope: String, _ fields: [String:Any]) throws -> [String:Any] {
        var request = fields; request["scope"] = scope
        guard let worker else { throw ShellError.message("Shared Bluetooth worker unavailable") }
        return try worker.call(request)
    }
    private func record(_ event: String, _ fields: [String:Any] = [:]) {
        var value = fields; value["event"] = event; value["utc"] = ISO8601DateFormatter().string(from:Date())
        if let bytes = try? JSONSerialization.data(withJSONObject:value, options:.sortedKeys), trace.utf8.count + bytes.count < 256*1024 { trace += String(decoding:bytes,as:UTF8.self)+"\n" }
    }
    private func message(_ value: [String:Any]) {
        guard !finished else { return }; record("transport",value)
        switch value["type"] as? String {
        case "candidates": if !closing { onDevices?(value["devices"] as? [[String:String]] ?? []) }
        case "discovery_status": onStatus?(value["message"] as? String ?? "")
        case "selected": onStatus?("Connecting to selected Bluetooth adapter…")
        case "ready":
            guard !closing else { return }
            let spp = value["transport"] as? String == "macos-classic-spp" && value["uuid"] as? String == MacSPPTransport.uuid && MacSPPTransport.supported(value["device_name"] as? String ?? "")
            guard AppleBLEProfile.accepts(value) || spp else { pending = ["state":"failed", "error":"Unsupported Bluetooth transport profile"]; close(); return }
            profile = value; core(["op":"start"])
        case "write_completed": if let command = value["command"] as? String { commandWrites[command,default:0] += 1; if expectedCommand == command { expectedCommand = nil } }
        case "rx": if !closing { core(["op":"rx", "bytes":value["bytes"] ?? []]) }
        case "error": if !closing { core(["op":"error", "message":value["message"] ?? "Bluetooth transport failed"]) }
        case "closed": released(value)
        default: break
        }
    }
    private func core(_ event: [String:Any]) {
        guard !closing, !finished else { return }
        do {
            let value = try call("vlinker",event)
            if let completed = value["completed"] as? [String:Any] { record("prompt_complete",completed) }
            switch value["state"] as? String {
            case "send":
                guard let command = value["command"] as? String else { throw ShellError.message("Missing shared Bluetooth command") }
                expectedCommand = command; commandAttempts[command,default:0] += 1
                record("write_attempt",["command":command]); onStatus?(command == "021A900000000000" ? "Reading a fresh ECM VIN…" : "Connecting to Bluetooth adapter…")
                link?.send(["op":"write", "command":command])
            case "complete", "failed": pending = value; close()
            default: break
            }
        } catch { pending = ["state":"failed", "error":error.localizedDescription]; close() }
    }
    private func close() {
        guard !closing else { return }; closing = true; timer?.invalidate(); timer = nil
        onStatus?("Closing Bluetooth connection…"); link?.send(["op":"close"])
    }
    private func released(_ value: [String:Any]) {
        guard !finished else { return }
        closing = true; timer?.invalidate(); timer = nil; expectedCommand = nil
        let confirmed = value["bluetooth_closed"] as? Bool == true
        let error = value["error"] as? String ?? ""
        let success = confirmed && error.isEmpty && pending?["state"] as? String == "complete"
        let result = success ? pending?["result"] as? [String:Any] ?? [:] : [:]
        var report: [String:Any] = ["schema":1, "status":success ? (result["vin"] == nil ? "identified" : "vin_received") : "failed",
            "observed_utc":ISO8601DateFormatter().string(from:Date()), "transport":profile["transport"] ?? "macos-native-bluetooth",
            "transport_profile":profile, "ble_profile":profile, "probe_mode":mode == "startup" || mode == "vin" || mode == "identity" ? "" : mode,
            "app_version":shellVersion, "app_build":shellVersion, "result":result, "bluetooth_closed":confirmed, "automatic_replay":false, "firmware_emulation":false,
            "command_attempts":commandAttempts, "command_writes":commandWrites,
            "host_vehicle_request_attempts":commandAttempts["021A900000000000",default:0], "host_vehicle_requests_written":commandWrites["021A900000000000",default:0],
            "message":success ? "Read complete · Adapter connection closed" : (!error.isEmpty ? error : pending?["error"] as? String ?? "Bluetooth read failed")]
        link?.shutdown(); link = nil
        record("finished",["success":success, "bluetooth_closed":confirmed])
        do {
            _ = try call("vlinker",["op":"configure", "mode":"release"])
            if success && ["ecm_info","ecm_dtc"].contains(mode) {
                var save = report; save["op"] = "save"
                let saved = try call("reports",save)
                if let error = saved["error"] as? String { throw ShellError.message(error) }
                report["saved_report"] = saved
            }
            let directory = root.appendingPathComponent("vlinker-tests/"+runID)
            try fm.createDirectory(at:directory,withIntermediateDirectories:true,attributes:[.posixPermissions:0o700])
            try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:directory.appendingPathComponent("result.json"),options:.atomic)
            try Data(trace.utf8).write(to:directory.appendingPathComponent("transcript.jsonl"),options:.atomic)
        } catch { report["storage_error"] = error.localizedDescription }
        if mode == "startup" {
            do {
                let event: [String:Any] = connectionStopping ? ["op":"transport_closed", "bluetooth_closed":confirmed] : ["op":"probe_result", "report":report]
                applyConnection(try call("connection",event)); return
            }
            catch { report["message"] = error.localizedDescription; report["status"] = "failed" }
        }
        complete(report)
    }
    private func applyConnection(_ value: [String:Any]) {
        if value["state"] as? String == "stopping", value["busy"] as? Bool == true {
            onStatus?(value["message"] as? String ?? "Waiting for adapter release…")
            timer?.invalidate()
            timer = Timer.scheduledTimer(withTimeInterval:0.2,repeats:true) { [weak self] _ in
                guard let self,!self.finished else {return}
                do {let result=try self.call("connection",["op":"tick"]);if result["busy"] as? Bool != true {self.applyConnection(result)}}
                catch {self.complete(["status":"failed", "message":error.localizedDescription])}
            }
            return
        }
        if value["effect"] as? String == "lookup", let address = value["url"] as? String, let url = URL(string:address),
           let limit = value["maximum_bytes"] as? Int, let timeout = value["timeout_seconds"] as? Double {
            onStatus?(value["message"] as? String ?? "Loading vehicle details…")
            let completion: (Data?,Int)->Void = { [weak self] bytes,status in
                guard let self, !self.finished else { return }; self.http = nil
                do {
                    let event: [String:Any] = bytes.map { ["op":"lookup_result", "http_status":status, "body":String(decoding:$0,as:UTF8.self)] } ?? ["op":"lookup_unavailable"]
                    self.applyConnection(try self.call("connection",event))
                } catch { self.complete(["status":"failed", "message":error.localizedDescription]) }
            }
            if let lookupEffect {lookupEffect(url,limit,timeout,completion)}
            else {let request=MacBluetoothHTTPS();http=request;request.run(url:url,limit:limit,timeout:timeout,callback:completion)}
        } else {
            onVehicle?(value)
            complete(["status":value["state"] ?? "failed", "message":value["message"] ?? "", "vehicle":value["vehicle"] ?? [:], "bluetooth_closed":true])
        }
    }
    private func complete(_ report: [String:Any]) {
        guard !finished else { return }; finished = true; timer?.invalidate(); timer = nil
        worker?.close(); worker = nil; http?.cancel(); http = nil
        Self.active = nil; onComplete?(report)
    }
    deinit { timer?.invalidate(); http?.cancel(); link?.shutdown(); worker?.close() }
}
