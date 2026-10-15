// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// macOS lifecycle, UI and HTTPS only. What is collected, its privacy bounds and what counts as a
// confirmed receipt are shared Rust (support_reports.rs), ported from Android SupportReports.

/// Small private connection record, same vocabulary as Android ConnectionAttempt.
/// No port names, payloads, VIN, serial numbers or error text. Main thread only.
final class ConnectionAttempt {
  let id=UUID().uuidString.lowercased(), created=ISO8601DateFormatter().string(from:Date()), began=Date()
  private let root:URL
  private(set) var stage="", outcome="IN_PROGRESS", reason="NONE"
  private var stages:[[String:Any]]=[]
  private var adapter="UNKNOWN"
  private var boundRun:URL?
  func selectAdapter(_ name:String){adapter=["CHIPSOFT","NANO","MDI","MONGOOSE","VLINKER","OBDLINK","SELECTION"].contains(name) ? name : "UNKNOWN";save()}
  func bind(_ run:URL, mode:String){
    boundRun=run
    let label = mode.lowercased().contains("full") ? "full_native" : mode.lowercased().contains("seed") ? "security_seeds" : "read_only"
    var record=((try? Data(contentsOf:run.appendingPathComponent("connection-evidence.json"))).flatMap{try? JSONSerialization.jsonObject(with:$0) as? [String:Any]}) ?? [:]
    record["attempt_id"]=id;record["session_mode"]=label;record["last_operation"]="connection";record["primary_reason"]=reason
    try? JSONSerialization.data(withJSONObject:record).write(to:run.appendingPathComponent("connection-evidence.json"),options:.atomic)
  }
  init(state:URL){root=state.appendingPathComponent("connection-attempts");advance("DETECTION")}
  var elapsed:Int {max(0,Int(Date().timeIntervalSince(began)*1000))}
  var open:Bool {outcome=="IN_PROGRESS"}
  func advance(_ next:String){
    guard open,next != stage else{return}
    stage=next;if stages.count<24 {stages.append(["stage":next,"elapsed_ms":elapsed])};save()
  }
  func finish(_ result:String,_ why:String="NONE"){guard open else{return};outcome=result;reason=why;save()}
  private func save(){
    let record:[String:Any]=["format":1,"attempt_id":id,"created_utc":created,"adapter":adapter,"stage":stage,
      "outcome":outcome,"reason":reason,"elapsed_ms":elapsed,"stages":stages]
    if let run=boundRun {
      let file=run.appendingPathComponent("connection-evidence.json")
      var evidence=((try? Data(contentsOf:file)).flatMap{try? JSONSerialization.jsonObject(with:$0) as? [String:Any]}) ?? [:]
      evidence["primary_reason"]=reason
      try? JSONSerialization.data(withJSONObject:evidence).write(to:file,options:.atomic)
    }
    // Reporting must never break a connection.
    do {
      try fm.createDirectory(at:root,withIntermediateDirectories:true)
      try JSONSerialization.data(withJSONObject:record).write(to:root.appendingPathComponent(id+".json"),options:.atomic)
      func stamp(_ u:URL)->Date {((try? u.resourceValues(forKeys:[.contentModificationDateKey]))?.contentModificationDate) ?? .distantPast}
      let files=try fm.contentsOfDirectory(at:root,includingPropertiesForKeys:[.contentModificationDateKey])
        .filter{$0.pathExtension=="json"}.sorted{stamp($0)>stamp($1)}
      for old in files.dropFirst(20) {try? fm.removeItem(at:old)}
    } catch {}
  }
}

/// One bounded HTTPS POST: no cookies, cache, credentials or redirects. Reply capped at 4 KiB.
final class SupportHTTP: NSObject, URLSessionDataDelegate, URLSessionTaskDelegate {
  private var data=Data(),status=0,failure=""
  private let done=DispatchSemaphore(value:0)
  static func post(endpoint:String,consent:String,body:Data)->(status:Int,network:String,body:String) {
    guard let url=URL(string:endpoint),url.scheme=="https",body.count<=65536 else {return (0,"invalid","")}
    let delegate=SupportHTTP(),config=URLSessionConfiguration.ephemeral
    config.httpShouldSetCookies=false;config.httpCookieStorage=nil;config.urlCache=nil;config.urlCredentialStorage=nil
    config.timeoutIntervalForRequest=30;config.timeoutIntervalForResource=45
    let network=URLSession(configuration:config,delegate:delegate,delegateQueue:nil)
    var r=URLRequest(url:url);r.httpMethod="POST";r.httpBody=body
    r.setValue("application/json; charset=utf-8",forHTTPHeaderField:"Content-Type")
    r.setValue(consent,forHTTPHeaderField:"X-OpenSAAB-Consent")
    let task=network.dataTask(with:r);task.resume()
    if delegate.done.wait(timeout:.now()+50) == .timedOut {task.cancel();network.invalidateAndCancel();return (0,"timeout","")}
    network.finishTasksAndInvalidate()
    return (delegate.status,delegate.failure,String(decoding:delegate.data,as:UTF8.self))
  }
  func urlSession(_ session:URLSession,task:URLSessionTask,willPerformHTTPRedirection response:HTTPURLResponse,newRequest request:URLRequest,completionHandler:@escaping(URLRequest?)->Void){completionHandler(nil)}
  func urlSession(_ session:URLSession,dataTask:URLSessionDataTask,didReceive response:URLResponse,completionHandler:@escaping(URLSession.ResponseDisposition)->Void){status=(response as? HTTPURLResponse)?.statusCode ?? 0;completionHandler(.allow)}
  func urlSession(_ session:URLSession,dataTask:URLSessionDataTask,didReceive bytes:Data){if data.count+bytes.count>4096{failure="oversize";dataTask.cancel()}else{data.append(bytes)}}
  func urlSession(_ session:URLSession,task:URLSessionTask,didCompleteWithError error:Error?){
    if failure.isEmpty,let e=error as? URLError {
      switch e.code {
      case .timedOut:failure="timeout"
      case .cannotFindHost,.dnsLookupFailed:failure="dns"
      case .notConnectedToInternet,.networkConnectionLost,.cannotConnectToHost:failure="offline"
      default:failure="invalid"
      }
    } else if failure.isEmpty,error != nil {failure="invalid"}
    done.signal()
  }
}

struct PreparedSupportReport {let text:String,file:URL,endpoint:String,consent:String}

extension Shell {
  var supportIntro:String {
    "Tell us what happened and what you expected. We’ll prepare app/computer details, connection stages and failure reasons, recent adapter event counts, emulator outcome and timing samples, bounded execution addresses, evidence availability, and separate process and USB cleanup results. Review the report, then send it privately to OpenSAAB over the internet. No account or email app is needed. Reports are stored in private OpenSAAB Cloudflare storage for the project administrator, not posted to GitHub, and stay until an administrator deletes them. You can also copy or save a report without sending it.\n\nRaw traffic, VIN, security data and screenshots are excluded. Please avoid entering private codes in your description. Nothing uploads automatically."
  }
  func recordOperation(_ operation:String){
    guard let run=session?.run else{return}
    let file=run.appendingPathComponent("connection-evidence.json")
    var value=((try? Data(contentsOf:file)).flatMap{try? JSONSerialization.jsonObject(with:$0) as? [String:Any]}) ?? [:]
    value["last_operation"]=operation
    try? JSONSerialization.data(withJSONObject:value).write(to:file,options:.atomic)
  }
  func hostFacts()->[String:Any] {
    let info=Bundle.main.infoDictionary ?? [:]
    var facts:[String:Any]=["platform":"macos","arch":shellArchitecture,
      "version":info["CFBundleShortVersionString"] as? String ?? "development",
      "build":info["OpenSAABBuild"] as? String ?? "",
      "os_version":ProcessInfo.processInfo.operatingSystemVersionString,
      "ram_total_bytes":ProcessInfo.processInfo.physicalMemory,
      "runtime_cpu_count":ProcessInfo.processInfo.activeProcessorCount]
    if let free=try? state.resourceValues(forKeys:[.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage {facts["storage_free_bytes"]=max(0,free)}
    if let screen=window?.screen ?? NSScreen.main {
      facts["display_width_px"]=Int(screen.frame.width*screen.backingScaleFactor)
      facts["display_height_px"]=Int(screen.frame.height*screen.backingScaleFactor)
    }
    return facts
  }
  /// Handled-error breadcrumb for the next report. Rust bounds it and removes VIN-like text.
  func recordAppError(_ message:String,kind:String="handled_error") {
    var record:[String:Any]=["utc":ISO8601DateFormatter().string(from:Date()),"kind":kind,"message":String(message.prefix(600))]
    if let run=session?.run {record["run_name"]=run.lastPathComponent}
    try? JSONSerialization.data(withJSONObject:record).write(to:state.appendingPathComponent("last-app-error.json"),options:.atomic)
  }
  func recordSessionEnd(_ run:URL,reason:String,exitCode:Int32?,began:Date) {
    var record:[String:Any]=["reason":reason,"elapsed_ms":max(0,Int(Date().timeIntervalSince(began)*1000))]
    if let code=exitCode {record["exit_code"]=max(0,Int(code))}
    try? JSONSerialization.data(withJSONObject:record).write(to:run.appendingPathComponent("session-end.json"),options:.atomic)
  }
  func reportIssue(prefill:String="") {
    let a=NSAlert();a.messageText="Report a problem"
    var intro=supportIntro
    if let last=try? SecurityWorker.call(reportsWorker,["op":"support_last","root":state.path])["report_id"] as? String {intro+="\n\nLast sent report: "+last}
    a.informativeText=intro
    let scroll=NSScrollView(frame:NSRect(x:0,y:0,width:460,height:110));scroll.hasVerticalScroller=true;scroll.borderType = .bezelBorder
    let field=NSTextView(frame:scroll.bounds);field.isRichText=false;field.autoresizingMask=[.width];field.string=prefill
    field.isAutomaticQuoteSubstitutionEnabled=false;field.font = .systemFont(ofSize:13)
    scroll.documentView=field;a.accessoryView=scroll
    a.addButton(withTitle:"Prepare report");a.addButton(withTitle:"Back")
    a.window.initialFirstResponder=field
    guard a.runModal() == .alertFirstButtonReturn else{return}
    prepareSupportReport(String(field.string.prefix(2000)))
  }
  func prepareSupportReport(_ description:String) {
    let worker=reportsWorker,root=state,host=hostFacts()
    status.stringValue="Preparing support report…"
    DispatchQueue.global().async {
      let result=Result {try SecurityWorker.call(worker,["op":"support_collect","root":root.path,"description":description,"host":host])}
      DispatchQueue.main.async {
        switch result {
        case .success(let r):
          guard let text=r["text"] as? String,let path=r["path"] as? String,let endpoint=r["endpoint"] as? String,let consent=r["consent"] as? String else {
            self.textDialog("Report a problem","Could not prepare the report. Please check free space and try again.");return
          }
          self.status.stringValue="Support report prepared · Review before sending"
          self.reviewSupportReport(PreparedSupportReport(text:text,file:URL(fileURLWithPath:path),endpoint:endpoint,consent:consent))
        case .failure:
          self.status.stringValue="Support report not prepared"
          self.textDialog("Report a problem","Could not prepare the report. Please check free space and try again.")
        }
      }
    }
  }
  func reviewSupportReport(_ report:PreparedSupportReport) {
    let a=NSAlert();a.messageText="Review support report"
    a.informativeText="This is exactly what will be sent. Your description is included; remove anything private first by choosing Keep private and preparing again."
    let scroll=NSScrollView(frame:NSRect(x:0,y:0,width:620,height:400));scroll.hasVerticalScroller=true
    let view=NSTextView(frame:scroll.bounds);view.isEditable=false;view.isSelectable=true;view.string=report.text
    view.font = .monospacedSystemFont(ofSize:11,weight:.regular)
    view.autoresizingMask=[.width];view.textContainer?.widthTracksTextView=true;view.isVerticallyResizable=true
    scroll.documentView=view;a.accessoryView=scroll
    a.addButton(withTitle:"Send to OpenSAAB");a.addButton(withTitle:"Other options…");a.addButton(withTitle:"Keep private")
    switch a.runModal() {
    case .alertFirstButtonReturn: sendSupportReport(report)
    case .alertSecondButtonReturn: supportExportOptions(report)
    default: status.stringValue="Support report kept private on this Mac"
    }
  }
  func sendSupportReport(_ report:PreparedSupportReport) {
    status.stringValue="Sending private report to OpenSAAB… Your local copy is kept."
    let worker=reportsWorker,root=state
    DispatchQueue.global().async {
      let reply=SupportHTTP.post(endpoint:report.endpoint,consent:report.consent,body:Data(report.text.utf8))
      let decision=Result {try SecurityWorker.call(worker,["op":"support_receipt","root":root.path,"status":reply.status,"body":reply.body,"network":reply.network])}
      DispatchQueue.main.async {
        let r:[String:Any]=(try? decision.get()) ?? ["stored":false,"retry":true,"message":"Could not send the report. Your local copy is kept; please try again."]
        if r["stored"] as? Bool == true,let number=r["report_id"] as? String {
          self.status.stringValue="Report sent · "+number
          let a=NSAlert();a.messageText="Report sent";a.informativeText="Stored privately by OpenSAAB.\n\nReport number: "+number
          a.addButton(withTitle:"Done");a.addButton(withTitle:"Copy number")
          if a.runModal() == .alertSecondButtonReturn {NSPasteboard.general.clearContents();NSPasteboard.general.setString(number,forType:.string)}
        } else {
          self.status.stringValue="Report not confirmed · Local copy kept"
          let a=NSAlert();a.messageText="Report not confirmed";a.informativeText=r["message"] as? String ?? "Storage was not confirmed."
          if r["retry"] as? Bool != false {a.addButton(withTitle:"Retry")}
          a.addButton(withTitle:"Other options…");a.addButton(withTitle:"Keep private")
          let retry=r["retry"] as? Bool != false
          let choice=a.runModal()
          if retry && choice == .alertFirstButtonReturn {self.sendSupportReport(report)}
          else if choice == (retry ? NSApplication.ModalResponse.alertSecondButtonReturn : NSApplication.ModalResponse.alertFirstButtonReturn) {self.supportExportOptions(report)}
        }
      }
    }
  }
  func supportExportOptions(_ report:PreparedSupportReport) {
    let a=NSAlert();a.messageText="Export support report"
    a.informativeText="Copy, save or share this reviewed report without uploading it."
    a.addButton(withTitle:"Copy report text");a.addButton(withTitle:"Save to a file…");a.addButton(withTitle:"Share / email…");a.addButton(withTitle:"Cancel")
    switch a.runModal() {
    case .alertFirstButtonReturn:
      NSPasteboard.general.clearContents();NSPasteboard.general.setString(report.text,forType:.string)
      status.stringValue="Report copied. Paste it into your browser or message."
    case .alertSecondButtonReturn:
      let panel=NSSavePanel();panel.nameFieldStringValue=report.file.lastPathComponent
      if panel.runModal() == .OK,let url=panel.url {
        do {try Data(report.text.utf8).write(to:url,options:.atomic);status.stringValue="Report saved"}
        catch {textDialog("Export support report","Could not save the file. Your report is still kept in the app.")}
      }
    case .alertThirdButtonReturn:
      dtcSharePicker=NSSharingServicePicker(items:[report.file])
      if let view=window.contentView {dtcSharePicker?.show(relativeTo:NSRect(x:20,y:20,width:1,height:1),of:view,preferredEdge:.minY)}
    default: break
    }
  }
}
