// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation
import Security

// Thin native bindings. All SSA, provider-choice and import decisions live in Rust.
enum SecurityWorker {
  static func authorize(_ binary:URL,_ password:String, cancelled:Bool=false)throws {
    let result=try call(binary,["op":"authorize","password":password,"cancelled":cancelled])
    guard result["authorized"] as? Bool == true else {throw ShellError.message("Security processing is unavailable or cancelled.")}
  }
  static func call(_ binary: URL, _ fields: [String:Any]) throws -> [String:Any] {
    var request=fields; request["schema"]=1
    let p=Process(), input=Pipe(), output=Pipe()
    p.executableURL=binary; p.standardInput=input; p.standardOutput=output
    p.standardError=FileHandle.nullDevice
    try p.run()
    do { try input.fileHandleForWriting.write(contentsOf:JSONSerialization.data(withJSONObject:request)); try input.fileHandleForWriting.close() }
    catch { p.terminate(); throw error }
    // Worker output is one bounded JSON envelope, never a console log.
    let deadline=Date().addingTimeInterval(30)
    let lock=NSLock(); var data=Data(), tooLarge=false
    output.fileHandleForReading.readabilityHandler={ h in
      let b=h.availableData; lock.lock(); defer{lock.unlock()}
      if data.count+b.count>2*1024*1024 {tooLarge=true; if p.isRunning {p.terminate()}} else {data.append(b)}
    }
    while p.isRunning && Date()<deadline {Thread.sleep(forTimeInterval:0.01)}
    if p.isRunning {p.terminate(); Thread.sleep(forTimeInterval:0.1); if p.isRunning {kill(p.processIdentifier,SIGKILL)}}
    p.waitUntilExit(); output.fileHandleForReading.readabilityHandler=nil
    let tail=output.fileHandleForReading.readDataToEndOfFile(); lock.lock(); data.append(tail); let bytes=data; let overflow=tooLarge; lock.unlock()
    guard !overflow,bytes.count<=2*1024*1024,let envelope=try JSONSerialization.jsonObject(with:bytes) as? [String:Any] else {throw ShellError.message("Security worker returned an invalid response.")}
    guard envelope["ok"] as? Bool == true,let result=envelope["result"] as? [String:Any] else {throw ShellError.message(envelope["error"] as? String ?? "Security worker failed.")}
    return result
  }
}
enum SecurityService {
  // Public compatibility header for the primary service. It does not authenticate callers or prevent endpoint abuse; never forward it to fallback.
  static let credential = "OpenSAAB"
  static let operatorPasswordRequired = false
}
final class SecurityJob {
  private let lock=NSLock();private var stopped=false,committing=false
  private var task:URLSessionTask?,attempt:URL?
  var cancelled:Bool {lock.lock();defer{lock.unlock()};return stopped}
  func attach(_ task:URLSessionTask){lock.lock();self.task=task;let stop=stopped;lock.unlock();if stop{task.cancel()}}
  func setAttempt(_ url:URL){lock.lock();attempt=url;let stop=stopped;lock.unlock();if stop{try? Data("cancel\n".utf8).write(to:url.appendingPathComponent("cancel"),options:.atomic)}}
  func beginCommit()throws {lock.lock();defer{lock.unlock()};guard !stopped else{throw ShellError.message("Security processing cancelled; no data imported.")};committing=true}
  func cancel()->Bool {lock.lock();if committing{lock.unlock();return false};stopped=true;let t=task,a=attempt;lock.unlock();t?.cancel();if let a=a{try? Data("cancel\n".utf8).write(to:a.appendingPathComponent("cancel"),options:.atomic)};return true}
}
struct SecurityHTTPResult {let status:Int,network:String,body:Data}
final class SecurityHTTP: NSObject, URLSessionDataDelegate, URLSessionTaskDelegate {
  private var data=Data(),status=0,failure=""
  private let done=DispatchSemaphore(value:0)
  static func network(_ error:Error?)->String {
    guard let e=error as? URLError else{return error == nil ? "":"invalid"}
    switch e.code {case .timedOut:return "timeout";case .cannotConnectToHost:return "connect";case .cannotFindHost,.dnsLookupFailed:return "dns";case .notConnectedToInternet:return "route";case .cancelled:return "cancelled";default:return "tls_or_invalid"}
  }
  static func request(endpoint:String,payload:[String:Any],password:String?,accessPassword:String,worker:URL,job:SecurityJob,configuration:URLSessionConfiguration?=nil)throws -> SecurityHTTPResult {
    try SecurityWorker.authorize(worker,accessPassword,cancelled:job.cancelled)
    guard !job.cancelled,let url=URL(string:endpoint),url.scheme=="https" else {throw ShellError.message("Security request cancelled or endpoint invalid.")}
    let delegate=SecurityHTTP(),config=configuration ?? URLSessionConfiguration.ephemeral
    config.httpShouldSetCookies=false;config.httpCookieStorage=nil;config.urlCache=nil
    config.timeoutIntervalForRequest=30;config.timeoutIntervalForResource=45
    let network=URLSession(configuration:config,delegate:delegate,delegateQueue:nil)
    var r=URLRequest(url:url);r.httpMethod="POST";r.httpBody=try JSONSerialization.data(withJSONObject:payload)
    r.setValue("application/json",forHTTPHeaderField:"Content-Type");r.setValue("application/json",forHTTPHeaderField:"Accept")
    if let password=password {r.setValue("Bearer "+password,forHTTPHeaderField:"Authorization");r.setValue("security-storage-24h-v1",forHTTPHeaderField:"X-OpenSAAB-Consent")}
    let task=network.dataTask(with:r);job.attach(task);task.resume()
    if delegate.done.wait(timeout:.now()+50) == .timedOut {task.cancel();network.invalidateAndCancel();return SecurityHTTPResult(status:0,network:"timeout",body:Data())}
    network.finishTasksAndInvalidate()
    return SecurityHTTPResult(status:delegate.status,network:delegate.failure,body:delegate.data)
  }
  func urlSession(_ session:URLSession,task:URLSessionTask,willPerformHTTPRedirection response:HTTPURLResponse,newRequest request:URLRequest,completionHandler:@escaping(URLRequest?)->Void){completionHandler(nil)}
  func urlSession(_ session:URLSession,dataTask:URLSessionDataTask,didReceive response:URLResponse,completionHandler:@escaping(URLSession.ResponseDisposition)->Void){status=(response as? HTTPURLResponse)?.statusCode ?? 0;if response.expectedContentLength>1024*1024{failure="oversize";completionHandler(.cancel)}else{completionHandler(.allow)}}
  func urlSession(_ session:URLSession,dataTask:URLSessionDataTask,didReceive bytes:Data){if data.count+bytes.count>1024*1024{failure="oversize";dataTask.cancel()}else{data.append(bytes)}}
  func urlSession(_ session:URLSession,task:URLSessionTask,didCompleteWithError error:Error?){if failure.isEmpty{failure=Self.network(error)};done.signal()}
}
extension Shell {
  var securityWorker:URL {binaries.appendingPathComponent("opensaab-security")}
  func securityAction() {
    if let job=securityJob {if job.cancel(){status.stringValue="Cancelling security processing…"}else{status.stringValue="Finishing verified import…"};return}
    guard !busy else{return}
    if session?.mx != nil {
      textDialog("Security access","Security collection and processing are not available in an OBDLink MX+ read-only session. Stop the session to process data collected earlier with a USB adapter.")
      return
    }
    updateSecurity()
    guard let firmware=store.selected?.id,
      let run=session?.run ?? SecurityWorkspace.previousRun(state:state,firmware:firmware) else {
      showError("No collected security data yet. Connect an adapter to collect fresh data.");return
    }
    let current=session
    let adapter: USBAdapterEndpoint?
    do { adapter = try session?.adapter ?? securityResumeAdapter ?? RunAdapterBinding.read(run: run) }
    catch { showError(error.localizedDescription); return }
    if securityView.state=="POST_AUTH" {
      let a=NSAlert();a.messageText="Security data already processed"
      a.informativeText="Keep using the processed security data in the original firmware, or choose a new collection. Reprocessing is not needed for POST-AUTH data. Vehicle authorization is checked by the firmware."
      a.addButton(withTitle:current != nil ? "Continue firmware":"Keep existing data")
      if current?.adapter != nil {a.addButton(withTitle:"Collect fresh data")}
      a.addButton(withTitle:"Cancel")
      let choice=a.runModal()
      if choice == .alertFirstButtonReturn {window.makeFirstResponder(lcd)}
      else if choice == .alertSecondButtonReturn,let connected=current?.adapter {beginFreshSecurity(adapter:connected)}
      return
    }
    busy=true;refresh();status.stringValue="Checking existing security data…"
    DispatchQueue.global().async {
      let result=Result{try SecurityWorker.call(self.securityWorker,["op":"inspect_existing","state":self.state.path,"run":run.path,"firmware":firmware])}
      DispatchQueue.main.async {
        self.busy=false;self.refresh()
        guard self.session === current,self.store.selected?.id==firmware else{return}
        let a=NSAlert();a.messageText="Security access"
        var selectedHash:String?
        if case .success(let data)=result,let hash=data["input_hash"] as? String {
          selectedHash=hash
          self.status.stringValue="Existing security data available · Choose processing or a fresh collection."
          a.informativeText="Existing collected data matches this vehicle and diagnostic software. Process these seeds as they are, or collect fresh data. Processing existing data does not require another VIN read or a connected adapter. The service/vehicle may require fresh seeds if the vehicle session has changed."
          a.addButton(withTitle:"Process existing data")
        } else {
          let reason: String
          if case .failure(let error)=result {reason=error.localizedDescription}else{reason="No valid collected data found."}
          self.status.stringValue="Security data needs attention · See the available choices."
          a.informativeText=reason+"\n\nConnect an adapter to collect fresh data. Earlier sessions and downloaded firmware are preserved."
        }
        if current?.adapter != nil {a.addButton(withTitle:"Collect fresh data")}
        a.addButton(withTitle:"Cancel")
        let choice=a.runModal()
        if let hash=selectedHash,choice == .alertFirstButtonReturn {
          self.securityConsent(run:run,firmware:firmware,adapter:adapter,existingHash:hash)
        } else if let connected=current?.adapter,choice == (selectedHash == nil ? .alertFirstButtonReturn:.alertSecondButtonReturn) {
          self.beginFreshSecurity(adapter:connected)
        }
      }
    }
  }
  func beginFreshSecurity(adapter:USBAdapterEndpoint) {
    let a=NSAlert();a.messageText="Collect fresh security data?"
    a.informativeText="This restarts the original firmware with a new working card. Your existing data and downloaded firmware are preserved. Follow the ignition-key prompts yourself. Nothing is uploaded until you choose processing and consent."
    a.addButton(withTitle:"Collect fresh data");a.addButton(withTitle:"Cancel")
    guard a.runModal() == .alertFirstButtonReturn else{return}
    stop {
      self.start(adapter:adapter,mode:.seeds,freshSecurity:true)
      self.securityResumeAdapter=adapter;self.securityNavigation=[:]
      // The qualified Nano path collects through original menus and physical prompts.
      self.securityNavigating=adapter.kind == .chipsoft;self.securityTransferRun=nil
      self.status.stringValue="Collecting security data · Follow the original ignition-key prompts."
    }
  }

  func securityWorkflowTick(_ current:EmulatorSession) {
    guard !securityChecking,Date().timeIntervalSince(securityWorkflowPoll)>=0.6,current.adapter != nil else{return}
    securityWorkflowPoll=Date();securityChecking=true
    let run=current.run,worker=securityWorker,nav=securityNavigation,vin=securityObservation()["vin"] ?? "",navigate=securityNavigating && current.canAcceptMenuKey,generation=securityNavigationGeneration
    DispatchQueue.global().async {
      let result=Result {()->([String:Any],[String:Any]?) in
        let bytes=try Data(contentsOf:run.appendingPathComponent("screen.txt"));guard bytes.count<=8192 else{throw ShellError.message("Invalid firmware screen.")}
        let screen=String(decoding:bytes,as:UTF8.self)
        let observed=try SecurityWorker.call(worker,["op":"observe","screen":screen])
        let next=navigate ? try SecurityWorker.call(worker,["op":"navigate","screen":screen,"vin":vin,"navigation":nav,"now_ms":Int(Date().timeIntervalSince1970*1000)]):nil
        return(observed,next)
      }
      DispatchQueue.main.async {
        self.securityChecking=false
        guard self.session === current,!self.busy else{return}
        if case .success(let (observed,next))=result {
          if observed["transfer"] as? Bool == true {
            self.securityTransferRun=run;self.securityNavigating=false
            self.securityActionButton.title="Process security data"
            self.status.stringValue="Collection reached TIS transfer · Choose Process security data."
          } else {self.securityTransferRun=nil;self.securityActionButton.title="Get security access"}
          if let next=next,self.securityNavigating,generation==self.securityNavigationGeneration,current.canAcceptMenuKey {
            self.securityNavigation=next["navigation"] as? [String:Any] ?? [:]
            if let key=next["key"] as? Int {current.key(key)}
            self.securityNavigating=next["active"] as? Bool ?? false
          }
        }
      }
    }
  }
  func securityConsent(run:URL,firmware:String,adapter:USBAdapterEndpoint?,existingHash:String) {
    let a=NSAlert();a.messageText="Process security access"
    a.informativeText="Send the collected VIN and 714-byte security data to OpenSAAB? The service retains security-request data for up to 24 hours and operational metrics for up to 7 days.\n\nOptionally allow one fallback to Bojer (sas.mysaab.info) if OpenSAAB is unreachable or returns 500/502/503/504. Bojer receives the same vehicle data, never the OpenSAAB primary-service header. No fallback for authentication, TLS or invalid responses.\n\nThe emulator will stop and release USB before processing. A verified reply updates only the security region of a new working card."
    a.addButton(withTitle:"OpenSAAB only");a.addButton(withTitle:"Allow fallback");a.addButton(withTitle:"Cancel")
    let consent=a.runModal();guard consent == .alertFirstButtonReturn || consent == .alertSecondButtonReturn else{return}
    let secret=SecurityService.credential,fallback=consent == .alertSecondButtonReturn
    securityNavigating=false;securityNavigationGeneration+=1;securityResumeAdapter=adapter
    stop {self.processSecurity(run:run,firmware:firmware,adapter:adapter,password:secret,fallback:fallback,existingHash:existingHash)}
  }
  func processSecurity(run:URL,firmware:String,adapter:USBAdapterEndpoint?,password:String,fallback:Bool,existingHash:String) {
    guard !busy else{return}
    do {try SecurityWorker.authorize(securityWorker,password)}
    catch {showError(error.localizedDescription);return}
    let job=SecurityJob(),worker=securityWorker,state=self.state
    securityJob=job;busy=true;securityActionButton.title="Cancel security processing"
    status.stringValue="Verifying collection and contacting OpenSAAB…";refresh()
    DispatchQueue.global().async {
      let result=Result {()->[String:Any] in
        if adapter?.kind == .mdi && !MdiCleanup.confirmed(run:run) {throw ShellError.message("MDI cleanup was not confirmed; collection remains preserved.")}
        if try NanoCleanup.requiredForSecurity(run: run, adapter: adapter), !NanoCleanup.confirmed(run: run) {
          throw ShellError.message("Nano channel shutdown and serial release were not confirmed. Security processing is blocked; collected data is preserved.")
        }
        let prepared=try SecurityWorker.call(worker,["op":"prepare_existing","state":state.path,"run":run.path,"firmware":firmware,"allow_fallback":fallback,"input_hash":existingHash,"password":password,"cancelled":job.cancelled])
        guard let attempt=prepared["attempt"] as? String,let payload=prepared["payload"] as? [String:Any],var endpoint=prepared["endpoint"] as? String else{throw ShellError.message("Invalid prepared request.")}
        job.setAttempt(URL(fileURLWithPath:attempt))
        for index in 0..<2 {
          let reply=try SecurityHTTP.request(endpoint:endpoint,payload:payload,password:index==0 ? password:nil,accessPassword:password,worker:worker,job:job)
          let decision=try SecurityWorker.call(worker,["op":"http_result","status":reply.status,"network":reply.network,"primary":index==0,"allow_fallback":fallback,"cancelled":job.cancelled,"password":password])
          switch decision["action"] as? String {
          case "fallback":guard let next=decision["endpoint"] as? String else{throw ShellError.message("Missing fallback endpoint.")};endpoint=next
          case "unauthorized":throw ShellError.message("The security service rejected this request. Update OpenSAAB or report the problem.")
          case "validate":
            guard let response=try JSONSerialization.jsonObject(with:reply.body) as? [String:Any] else{throw ShellError.message("Security service returned invalid JSON. No data imported.")}
            try job.beginCommit()
            return try SecurityWorker.call(worker,["op":"import","attempt":attempt,"provider":endpoint,"response":response,"password":password,"cancelled":job.cancelled])
          default:throw ShellError.message(decision["message"] as? String ?? "Security request failed. No data imported.")
          }
        }
        throw ShellError.message("Security processing did not produce a valid reply.")
      }
      DispatchQueue.main.async {
        self.securityJob=nil;self.busy=false;self.securityActionButton.title="Get security access";self.securityTransferRun=nil;self.refresh()
        switch result {
        case .failure(let error):self.status.stringValue="Security processing stopped · Collection preserved for retry.";self.showError(error.localizedDescription)
        case .success:
          self.status.stringValue="Processed security data imported · Continue in the original firmware."
          guard let adapter=adapter else {
            self.textDialog("Security data ready","The verified response is saved. Connect and start when the adapter and vehicle are ready. No new vehicle connection is needed to preserve this processed data.")
            return
          }
          let a=NSAlert();a.messageText="Security data ready";a.informativeText="The verified response is saved in a new working card. POST-AUTH describes this card, not confirmed vehicle authorization. Keep the same adapter and vehicle connected. Resume starts the original firmware without requiring another VIN read while the ignition may be off. Follow its prompts."
          a.addButton(withTitle:"Resume diagnostics");a.addButton(withTitle:"Keep stopped")
          if a.runModal() == .alertFirstButtonReturn {self.resumeSecurity(adapter:adapter)}
        }
      }
    }
  }
  func resumeSecurity(adapter:USBAdapterEndpoint) {
    guard !busy,session==nil else{return}
    if adapter.kind == .mdi { start(adapter:adapter,mode:.fullNative);return }
    busy=true;refresh()
    status.stringValue="Checking the connected \(adapter.kind.rawValue) adapter…"
    DispatchQueue.global().async {
      let result=Result { try self.verifyUSBAdapter(adapter) }
      DispatchQueue.main.async {
        self.busy=false
        if case .success(let checked)=result,let expected=adapter.identity,checked.identity==expected {
          self.start(adapter:checked,mode:.fullNative)
        } else {
          self.status.stringValue="Security data saved · Adapter identity could not be reverified."
          self.showError("Keep the original adapter connected, then Connect and start when the ignition is on. The imported security card is preserved.")
        }
        self.refresh()
      }
    }
  }
  func adapterIdentity(_ text:String)->String? {
    // Stable GET_INFO model/firmware reply; paired with the original CDC port.
    // This does not claim a unique hardware serial or re-read the vehicle VIN.
    let lines=text.split(separator:"\n").map(String.init).filter{$0.hasPrefix("Identified: CHIPSOFT J2534 Pro v. ")}
    return lines.isEmpty ? nil:lines.joined(separator:"\n")
  }
}

// Native transport contract checks use URLProtocol; no sockets, secrets or vehicle.
private final class SecurityFixtureProtocol: URLProtocol {
  override class func canInit(with request:URLRequest)->Bool {true}
  override class func canonicalRequest(for request:URLRequest)->URLRequest {request}
  override func startLoading(){
    let path=request.url!.path
    let status=path=="/denied" ? 401:path=="/outage" ? 503:200
    if path=="/tls" {client?.urlProtocol(self,didFailWithError:URLError(.serverCertificateUntrusted));return}
    let headers=request.allHTTPHeaderFields ?? [:]
    let body=path=="/oversize" ? Data(repeating:0,count:1024*1024+1):(try! JSONSerialization.data(withJSONObject:headers))
    client?.urlProtocol(self,didReceive:HTTPURLResponse(url:request.url!,statusCode:status,httpVersion:"HTTP/1.1",headerFields:[:])!,cacheStoragePolicy:.notAllowed)
    client?.urlProtocol(self,didLoad:body);client?.urlProtocolDidFinishLoading(self)
  }
  override func stopLoading(){}
}
enum SecurityWorkflowChecks {
  static func run() throws {
    let worker=Bundle.main.executableURL!.deletingLastPathComponent().appendingPathComponent("opensaab-security")
    let access=try SecurityWorker.call(worker,["op":"authorize"])
    try ShellChecks.need(access["authorized"] as? Bool == true && access["mode"] as? String == "no_operator_password","packaged worker permits processing without operator credentials")
    try ShellChecks.need(!SecurityService.operatorPasswordRequired,"operator password is absent in every channel")
    try ShellChecks.need(Bundle.main.object(forInfoDictionaryKey:"OpenSAABSecurityAccessMode") as? String == "no_operator_password","packaged application declares common open-access policy")
    func request(_ path:String,_ password:String?=nil,_ accessPassword:String="OpenSAAB")throws->SecurityHTTPResult {
      let config=URLSessionConfiguration.ephemeral;config.protocolClasses=[SecurityFixtureProtocol.self]
      return try SecurityHTTP.request(endpoint:"https://security-fixture.invalid/"+path,payload:["REQUEST_VERSION":1,"SSA_DATA":"synthetic"],password:password,accessPassword:accessPassword,worker:worker,job:SecurityJob(),configuration:config)
    }
    let stopped=SecurityJob();_ = stopped.cancel();var stoppedRejected=false
    do {_ = try SecurityHTTP.request(endpoint:"https://security-fixture.invalid/primary",payload:[:],password:SecurityService.credential,accessPassword:"",worker:worker,job:stopped,configuration:URLSessionConfiguration.ephemeral)} catch {stoppedRejected=true}
    try ShellChecks.need(stoppedRejected,"cancelled processing stops before primary service disclosure")
    let primary=try request("primary","synthetic-not-a-secret")
    let rawHeaders=try JSONSerialization.jsonObject(with:primary.body) as! [String:String]
    let headers=Dictionary(uniqueKeysWithValues:rawHeaders.map{($0.key.lowercased(),$0.value)})
    try ShellChecks.need(primary.status==200 && headers["authorization"]=="Bearer synthetic-not-a-secret" && headers["x-opensaab-consent"]=="security-storage-24h-v1","native primary sends the compatibility header and explicit retention consent")
    let fallback=try request("fallback"),f=try JSONSerialization.jsonObject(with:fallback.body) as! [String:String]
    try ShellChecks.need(!f.keys.contains(where:{$0.lowercased()=="authorization" || $0.lowercased()=="x-opensaab-consent"}),"native fallback never receives the primary compatibility header")
    try ShellChecks.need(try request("denied").status==401 && request("outage").status==503,"native HTTP status preserved for shared provider decision")
    try ShellChecks.need(try request("oversize").network=="oversize","native response body limited to one MiB")
    try ShellChecks.need(try request("tls").network=="tls_or_invalid","native TLS failure cannot become outage fallback")
    let delegate=SecurityHTTP();var redirected=true
    delegate.urlSession(URLSession.shared,task:URLSession.shared.dataTask(with:URL(string:"https://security-fixture.invalid")!),willPerformHTTPRedirection:HTTPURLResponse(url:URL(string:"https://security-fixture.invalid")!,statusCode:302,httpVersion:nil,headerFields:[:])!,newRequest:URLRequest(url:URL(string:"https://other.invalid")!)){redirected=$0 != nil}
    try ShellChecks.need(!redirected,"native security redirects disabled")
    let job=SecurityJob();_ = job.cancel();var rejected=false
    do{try job.beginCommit()}catch{rejected=true}
    try ShellChecks.need(rejected,"cancelled native job cannot begin card import")
    let commit=SecurityJob();try commit.beginCommit()
    try ShellChecks.need(!commit.cancel(),"native commit cannot be relabelled as cancelled after activation starts")
  }
}
