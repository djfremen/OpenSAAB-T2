// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation

enum USBChooserChecks {
  static func run(contract:URL) throws {
    // Real AppKit modal/timer lifecycle, synthetic registry snapshots only.
    // No serial handle, vehicle commands, runtime state or security service.
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let profiles = try USBAdapterDiscovery.profiles(contract)
    func candidate(_ port:String,_ vendor:Int,_ product:Int) -> USBAdapterCandidate {
      USBAdapterDiscovery.candidate(port:port,vendor:vendor,product:product,profiles:profiles)!
    }
    let a = candidate("/dev/cu.usbmodemA",0x0483,0x5740)
    let b = candidate("/dev/cu.usbmodemB",0x0483,0x5740)
    let nano = candidate("/dev/cu.usbmodem0",0x1a86,0x55d3)
    var fixture = USBDiscoverySnapshot(candidates:[],waitingForSerial:[])
    var scanFailed = false
    let chooser = USBAdapterChooser(readCodes:false,mode:.readOnly) {
      if scanFailed { throw ShellError.message("Synthetic registry error") }
      return fixture
    }
    var phase = 0, failure:Error?, watchdogUsed = false
    func schedule(_ interval:TimeInterval,_ body:@escaping () -> Void) -> Timer {
      let t = Timer(timeInterval:interval,repeats:true) { _ in body() }
      RunLoop.main.add(t,forMode:.common); RunLoop.main.add(t,forMode:.modalPanel)
      return t
    }
    let observer = schedule(0.03) {
      guard failure == nil else { return }
      do {
        switch phase {
        case 0:
          guard app.modalWindow === chooser.alert.window else { return }
          try ShellChecks.need(chooser.selected == nil && !chooser.alert.buttons[0].isEnabled,"empty registry cannot connect")
          chooser.modePicker.selectItem(withTitle:ChipsoftSessionMode.seeds.rawValue)
          let count = chooser.scanCount
          chooser.alert.buttons[1].performClick(nil)
          try ShellChecks.need(chooser.scanCount == count+1 && app.modalWindow === chooser.alert.window,"Refresh scans in place without recreating or closing the modal")
          fixture = USBDiscoverySnapshot(candidates:[],waitingForSerial:[profiles.first{$0.kind == .chipsoft}!])
          phase = 1
        case 1:
          guard chooser.scanStatus.stringValue.contains("Waiting for macOS") else { return }
          try ShellChecks.need(chooser.scanStatus.stringValue.contains("Chipsoft Pro") && !chooser.alert.buttons[0].isEnabled,"USB-before-serial arrival is displayed without accepting a missing port")
          fixture = USBDiscoverySnapshot(candidates:[a],waitingForSerial:[]); phase = 2
        case 2:
          guard chooser.selected?.port == a.port else { return }
          try ShellChecks.need(chooser.alert.buttons[0].isEnabled && chooser.mode == .seeds,"late serial arrival updates an already-open modal and preserves mode")
          fixture = USBDiscoverySnapshot(candidates:[nano,a,b],waitingForSerial:[])
          chooser.alert.buttons[1].performClick(nil)
          try ShellChecks.need(chooser.selected?.port == a.port,"inserting an earlier popup row preserves selected port rather than index")
          chooser.list.selectItem(at:2); chooser.selectionChanged(nil)
          fixture = USBDiscoverySnapshot(candidates:[a,b],waitingForSerial:[])
          chooser.alert.buttons[1].performClick(nil)
          try ShellChecks.need(chooser.selected?.port == b.port && chooser.mode == .seeds,"Refresh preserves explicit adapter and mode")
          fixture = USBDiscoverySnapshot(candidates:[],waitingForSerial:[]); phase = 3
        case 3:
          guard chooser.selected == nil else { return }
          try ShellChecks.need(!chooser.alert.buttons[0].isEnabled,"unplug removes stale acceptance without clicking Refresh")
          fixture = USBDiscoverySnapshot(candidates:[a],waitingForSerial:[])
          chooser.alert.buttons[1].performClick(nil)
          fixture = USBDiscoverySnapshot(candidates:[b],waitingForSerial:[])
          chooser.alert.buttons[0].performClick(nil)
          try ShellChecks.need(chooser.accepted == nil && app.modalWindow === chooser.alert.window && chooser.selected?.port == b.port,"Connect rejects a port replacement occurring after the display scan")
          scanFailed = true; chooser.alert.buttons[1].performClick(nil)
          try ShellChecks.need(chooser.selected == nil && !chooser.alert.buttons[0].isEnabled && chooser.scanStatus.stringValue.contains("Synthetic registry error"),"registry failure clears old selection and exposes failure")
          scanFailed = false; chooser.alert.buttons[1].performClick(nil)
          try ShellChecks.need(chooser.selected?.port == b.port,"Refresh recovers after registry failure in the same modal")
          phase = 4; chooser.alert.buttons[0].performClick(nil)
        default: break
        }
      } catch {
        failure = error; app.abortModal()
      }
    }
    let deadline = schedule(8) { watchdogUsed = true; app.abortModal() }
    let result = chooser.run()
    observer.invalidate(); deadline.invalidate()
    chooser.alert.window.orderOut(nil)
    if let error = failure { throw error }
    try ShellChecks.need(phase == 4 && !watchdogUsed && result?.0.port == b.port && result?.1 == .seeds,"explicit Connect returns the current adapter and selected mode after modal hotplug")
    let count = chooser.scanCount
    chooser.refreshNow()
    try ShellChecks.need(!chooser.active && chooser.scanCount == count,"dismissed chooser rejects later refresh and stops polling")

    let restricted = USBAdapterChooser(readCodes:true,mode:.fullNative) {
      USBDiscoverySnapshot(candidates:[nano],waitingForSerial:[])
    }
    let cancel = schedule(0.03) {
      guard app.modalWindow === restricted.alert.window else { return }
      do {
        try ShellChecks.need(!restricted.alert.buttons[0].isEnabled && restricted.reason.stringValue.contains("requires Chipsoft"),"Nano does not gain the Chipsoft-only engine-code shortcut")
      } catch { failure = error }
      restricted.alert.buttons[2].performClick(nil)
    }
    let cancelDeadline = schedule(5) { watchdogUsed = true; app.abortModal() }
    let cancelled = restricted.run()
    cancel.invalidate(); cancelDeadline.invalidate(); restricted.alert.window.orderOut(nil)
    if let error = failure { throw error }
    try ShellChecks.need(cancelled == nil && !restricted.active && !watchdogUsed,"Cancel ends the chooser without accepting or connecting")
    print("USB chooser modal regressions passed; synthetic registry only, no physical reconnect qualification.")
  }
}
