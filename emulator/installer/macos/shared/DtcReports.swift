// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// macOS lifecycle/UI only. Parsing, list coverage, grouping and storage are shared Rust.
final class DtcObserver {
  private let queue=DispatchQueue(label:"com.opensaab.dtc-reports")
  private var working=false,run:URL?,candidate="",processed="",poll=Date.distantPast
  func tick(root:URL,session:URL,worker:URL,completed:@escaping(Result<[String:Any],Error>)->Void) {
    guard !working,Date().timeIntervalSince(poll)>0.25 else{return};poll=Date()
    if run != session {run=session;candidate="";processed=""}
    let file=session.appendingPathComponent("screen.txt")
    guard let size=try? file.resourceValues(forKeys:[.fileSizeKey]).fileSize,size<=8192,
      let text=try? String(contentsOf:file,encoding:.utf8) else{return}
    guard text==candidate else{candidate=text;return}
    guard text != processed else{return}
    processed=text;working=true
    queue.async {
      let result=Result {try SecurityWorker.call(worker,["op":"observe","root":root.path,"run":session.path])}
      DispatchQueue.main.async {self.working=false;completed(result)}
    }
  }
}
extension Shell {
  var reportsWorker:URL {binaries.appendingPathComponent("opensaab-reports")}
  func observeDtc(_ current:EmulatorSession) {
    dtcObserver.tick(root:state,session:current.run,worker:reportsWorker) { [weak self] result in
      guard let self=self,self.session === current else{return}
      switch result {
      case .success(let value):
        if value["changed"] as? Bool == true {
          self.dtcReportButton.title="DTC report · \(value["codes"] as? Int ?? 0) codes" + ((value["partial"] as? Bool == true) ? " · Partial":"") + " · Review / share"
          self.dtcReportButton.isHidden=false
        }
      case .failure:
        self.dtcReportButton.title="DTC report save failed · Open saved reports"
        self.dtcReportButton.isHidden=false
      }
    }
  }
  func savedDtcReports() {
    let worker=reportsWorker,root=state
    DispatchQueue.global().async {
      let result=Result {try SecurityWorker.call(worker,["op":"list","root":root.path])}
      DispatchQueue.main.async {
        do {
          let reports=try result.get()["reports"] as? [[String:Any]] ?? []
          guard !reports.isEmpty else {
            self.textDialog("DTC reports","Reports are saved automatically when the original firmware displays a numbered DTC Information list. Scroll through the list to capture every position. Reports cover only the displayed list, not every vehicle module.");return
          }
          let a=NSAlert();a.messageText="Saved DTC reports · latest 100"
          a.informativeText="Choose a saved report to review or export. Reports may contain your VIN."
          let picker=NSPopUpButton(frame:NSRect(x:0,y:0,width:520,height:30))
          for report in reports {picker.addItem(withTitle:report["id"] as? String ?? "Report")}
          a.accessoryView=picker;a.addButton(withTitle:"Review report");a.addButton(withTitle:"Close")
          if a.runModal() == .alertFirstButtonReturn,let id=reports[picker.indexOfSelectedItem]["id"] as? String {self.reviewDtcReport(id)}
        } catch {self.showError(error.localizedDescription)}
      }
    }
  }
  func reviewDtcReport(_ id:String) {
    let worker=reportsWorker,root=state
    DispatchQueue.global().async {
      let result=Result {try SecurityWorker.call(worker,["op":"read","root":root.path,"id":id])}
      DispatchQueue.main.async {
        do {
          let contents=try result.get()["text"] as? String ?? ""
          let a=NSAlert();a.messageText="Saved DTC report"
          let scroll=NSScrollView(frame:NSRect(x:0,y:0,width:600,height:390));scroll.hasVerticalScroller=true
          let view=NSTextView(frame:scroll.bounds);view.isEditable=false;view.isSelectable=true;view.string=contents
          view.autoresizingMask=[.width];view.textContainer?.widthTracksTextView=true;view.isVerticallyResizable=true
          scroll.documentView=view;a.accessoryView=scroll
          a.addButton(withTitle:"Export report…");a.addButton(withTitle:"Share / email…");a.addButton(withTitle:"Close")
          let choice=a.runModal()
          if choice == .alertFirstButtonReturn {
            let panel=NSSavePanel();panel.nameFieldStringValue=id+".txt";panel.message="This report may contain your VIN. Choose where to save your copy."
            if panel.runModal() == .OK,let url=panel.url {try Data(contents.utf8).write(to:url,options:.atomic)}
          } else if choice == .alertSecondButtonReturn {
            let dir=self.state.appendingPathComponent("dtc-exports")
            try fm.createDirectory(at:dir,withIntermediateDirectories:true)
            let file=dir.appendingPathComponent("OpenSAAB-DTC-"+UUID().uuidString+".txt")
            try Data(contents.utf8).write(to:file,options:.atomic)
            self.dtcSharePicker=NSSharingServicePicker(items:[file])
            if let view=self.window.contentView {self.dtcSharePicker?.show(relativeTo:NSRect(x:20,y:20,width:1,height:1),of:view,preferredEdge:.minY)}
          }
        } catch {self.showError(error.localizedDescription)}
      }
    }
  }
}
