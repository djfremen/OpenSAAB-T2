// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

// Thin AppKit/IOKit binding. Discovery never opens a transport. Contract candidate
// profiles and the existing shared identity/VIN/cleanup workers remain authoritative.
final class USBAdapterChooser: NSObject {
  let alert = NSAlert()
  let list = NSPopUpButton(frame:NSRect(x:0,y:0,width:430,height:30))
  let modePicker = NSPopUpButton(frame:NSRect(x:0,y:0,width:430,height:30))
  let reason = USBAdapterChooser.label("")
  let scanStatus = USBAdapterChooser.label("")
  let checked = USBAdapterChooser.label("")
  private let scan: () throws -> USBDiscoverySnapshot
  private let readCodes: Bool
  private var candidates: [USBAdapterCandidate] = []
  private var timer: Timer?
  private(set) var scanCount = 0
  private(set) var active = false
  private(set) var accepted: USBAdapterCandidate?

  init(readCodes:Bool,mode:ChipsoftSessionMode,scan:@escaping () throws -> USBDiscoverySnapshot) {
    self.readCodes = readCodes; self.scan = scan
    super.init()
    alert.messageText = "USB adapters"
    alert.informativeText = readCodes
      ? "Choose Chipsoft Pro for the separate Trionic 8 engine-code report. VCX Nano and MDI use the diagnostic menus through Connect and start."
      : "Detected Chipsoft Pro, VCX Nano and MDI candidates appear below. Connect and start verifies the adapter identity and reads a fresh VIN before opening the diagnostic menus."
    alert.addButton(withTitle:"Connect and start")
    alert.addButton(withTitle:"Refresh")
    alert.addButton(withTitle:"Cancel")
    alert.buttons[0].target = self; alert.buttons[0].action = #selector(connect(_:))
    alert.buttons[1].target = self; alert.buttons[1].action = #selector(refreshClicked(_:))
    list.target = self; list.action = #selector(selectionChanged(_:))
    modePicker.addItems(withTitles:ChipsoftSessionMode.allCases.map{$0.rawValue})
    modePicker.selectItem(withTitle:mode.rawValue)
    let options = NSStackView(views:[Self.label("Detected USB adapter"),list,Self.label("Session mode"),modePicker,reason,scanStatus,checked,
      Self.label("The list updates while this window is open. Refresh checks again. Detecting an adapter does not connect to the car."),
      Self.label("Full native control follows the original diagnostic menus, including ignition-key tests. Read-only mode blocks control operations. Get security access collects seeds, requests your processing consent and imports a verified response.")])
    options.orientation = .vertical; options.spacing = 8; options.alignment = .leading
    options.frame = NSRect(x:0,y:0,width:460,height:330)
    options.widthAnchor.constraint(equalToConstant:460).isActive = true
    alert.accessoryView = options
  }

  private static func label(_ text:String) -> NSTextField {
    let field = NSTextField(wrappingLabelWithString:text)
    field.font = .systemFont(ofSize:13)
    field.textColor = foreground
    return field
  }

  var selected: USBAdapterCandidate? {
    candidates.indices.contains(list.indexOfSelectedItem) ? candidates[list.indexOfSelectedItem] : nil
  }
  var mode: ChipsoftSessionMode { ChipsoftSessionMode.allCases[modePicker.indexOfSelectedItem] }

  func run() -> (USBAdapterCandidate,ChipsoftSessionMode)? {
    active = true; accepted = nil
    refreshNow()
    let poll = Timer(timeInterval:0.5,repeats:true) { [weak self] _ in self?.refreshNow() }
    timer = poll
    // NSAlert runs in modalPanel mode: ordinary scheduled timers do not fire there.
    RunLoop.main.add(poll,forMode:.common); RunLoop.main.add(poll,forMode:.modalPanel)
    defer { active = false; poll.invalidate(); timer = nil }
    let result = alert.runModal()
    guard result == .alertFirstButtonReturn,let candidate = accepted else { return nil }
    return (candidate,mode)
  }

  @objc func refreshClicked(_ sender:Any?) { refreshNow() }
  @objc func selectionChanged(_ sender:Any?) { updateAcceptance() }

  func refreshNow() {
    guard active else { return }
    let previous = selected
    scanCount += 1
    checked.stringValue = "Last checked · " + DateFormatter.localizedString(from:Date(),dateStyle:.none,timeStyle:.medium)
    do {
      let snapshot = try scan()
      let new = snapshot.candidates
      // Preserve the port AND candidate identity, never an old popup index.
      let sameList = candidates.map{signature($0)} == new.map{signature($0)}
      candidates = new
      if !sameList || list.numberOfItems == 0 {
        list.removeAllItems()
        list.addItems(withTitles:new.map{$0.label.replacingOccurrences(of:"\n",with:" · ")})
        if new.isEmpty { list.addItem(withTitle:"No supported USB adapter found") }
        list.isEnabled = !new.isEmpty
        if let previous = previous,let index = new.firstIndex(where:{signature($0) == signature(previous)}) {
          list.selectItem(at:index)
        } else if let first = new.firstIndex(where:{$0.supports(readCodes:readCodes)}) { list.selectItem(at:first) }
      }
      if new.isEmpty {
        scanStatus.stringValue = snapshot.waitingForSerial.isEmpty
          ? "macOS has not detected a supported USB adapter. Waiting for it to reconnect."
          : snapshot.waitingForSerial.map{$0.kind.rawValue}.joined(separator:", ") + " USB detected. Waiting for macOS to make its serial port available."
      } else {
        scanStatus.stringValue = "USB list checked · \(new.count) candidate\(new.count == 1 ? "" : "s")"
      }
      updateAcceptance()
    } catch {
      // An unsuccessful scan cannot leave an old port selectable.
      candidates = []; list.removeAllItems(); list.addItem(withTitle:"USB discovery unavailable"); list.isEnabled = false
      alert.buttons[0].isEnabled = false
      reason.stringValue = "USB discovery failed. Choose Refresh to try again."
      scanStatus.stringValue = error.localizedDescription
    }
  }

  private func signature(_ candidate:USBAdapterCandidate) -> String {
    candidate.port + "|" + (candidate.profile.map{"\($0.vendor):\($0.product)"} ?? "unknown")
  }
  private func updateAcceptance() {
    alert.buttons[0].isEnabled = selected?.supports(readCodes:readCodes) == true
    reason.stringValue = selected?.profile == nil
      ? "No supported USB identity. Connect Chipsoft Pro, VCX Nano or MDI."
      : readCodes && selected?.profile?.kind != .chipsoft
        ? "The separate engine-code report requires Chipsoft. Use Connect and start from the main screen for this adapter."
        : "OpenSAAB verifies this candidate's product identity and firmware before vehicle traffic."
  }
  @objc func connect(_ sender:Any?) {
    guard active,let intended = selected else { return }
    // Cover unplug/replacement between the latest UI scan and the button click.
    refreshNow()
    guard let current = selected,signature(current) == signature(intended),current.supports(readCodes:readCodes) else {
      reason.stringValue = "The selected adapter changed or disconnected. Review the updated list before connecting."
      return
    }
    accepted = current
    NSApplication.shared.stopModal(withCode:.alertFirstButtonReturn)
  }
}
