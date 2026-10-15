// SPDX-License-Identifier: MPL-2.0
import AppKit

/// Native layout checks with an isolated firmware cache. Never opens an adapter.
enum DesignChecks {
  static func views(_ view: NSView) -> [NSView] { [view] + view.subviews.flatMap { views($0) } }
  static func run(state: URL, output: URL, binaries: URL, catalog: URL) throws {
    guard state.path != fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("OpenSAAB").path else {
      throw ShellError.message("Design checks require an isolated state directory.")
    }
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let shell = try Shell(state: state, binaries: binaries, catalog: catalog)
    guard shell.store.selected != nil else { throw ShellError.message("Prepare an isolated firmware selection before checking layouts.") }
    try fm.createDirectory(at: output, withIntermediateDirectories: true)
    shell.applicationDidFinishLaunching(Notification(name: NSApplication.didFinishLaunchingNotification))
    defer { shell.timer?.invalidate(); shell.window.orderOut(nil); if shell.lockFD >= 0 { close(shell.lockFD) } }
    func visible(_ v: NSView, in content: NSView) -> Bool {
      !v.isHiddenOrHasHiddenAncestor && content.bounds.insetBy(dx: -1, dy: -1).contains(v.convert(v.bounds, to: content))
    }
    for (name, width, height, expanded, keypadOpen) in [
      ("ready-wide", 1060, 740, false, false),
      ("ready-keypad-wide", 1060, 740, false, true),
      ("ready-compact", 600, 800, false, false),
      ("ready-keypad-compact", 600, 800, false, true),
      ("ready-minimum", 500, 600, false, false),
      ("ready-large", 1400, 800, false, false),
      ("ready-expanded", 1060, 740, true, false),
      ("ready-keypad-expanded", 1060, 740, true, true),
      ("restored-wide", 1060, 740, false, false)
    ] {
      shell.expanded = expanded
      shell.keypadVisible = keypadOpen
      shell.window.setContentSize(NSSize(width: width, height: height))
      shell.arrange()
      guard let content = shell.window.contentView else { throw ShellError.message("Missing workspace") }
      content.layoutSubtreeIfNeeded()
      shell.window.recalculateKeyViewLoop()
      shell.window.makeFirstResponder(shell.lcd)
      shell.window.selectNextKeyView(nil)
      try ShellChecks.need((shell.window.firstResponder as? ActionButton)?.isEnabled == true,
        name + " advances keyboard focus from firmware to an enabled action")
      shell.window.makeFirstResponder(shell.lcd)
      try ShellChecks.need(abs(content.bounds.width - CGFloat(width)) < 1 && abs(content.bounds.height - CGFloat(height)) < 1,
        name + " respects requested \(width)x\(height) window size (actual \(content.bounds.width)x\(content.bounds.height))")
      let wide = width >= 720 && width > height
      try ShellChecks.need(shell.layoutMode == (expanded ? "expanded" : (wide ? "wide" : "compact")), name + " selects responsive layout")
      try ShellChecks.need(shell.lcd.frame.height >= 159 && shell.lcd.frame.width >= 250, name + " keeps usable firmware display")
      for title in ["S1", "S2", "S3", "S4", "EXIT", "Keypad", "Actions", "Donate · Support OpenSAAB"] {
        guard let button = views(content).compactMap({ $0 as? ActionButton }).first(where: { $0.title == title }) else {
          throw ShellError.message("Missing control " + title)
        }
        let minTarget: CGFloat = title.hasPrefix("Donate") ? 31 : 35
        try ShellChecks.need(visible(button, in: content) && button.bounds.height >= minTarget, name + " keeps " + title + " visible with proportional target")
      }
      try ShellChecks.need(shell.keypadPanel.isHidden == !keypadOpen, name + " keeps docked keypad visibility accurate")
      if keypadOpen {
        for keyTitle in ["HELP", "↑", "YES", "←", "ENTER", "→", "↓", "NO", "F0", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9"] {
          guard let keyBtn = views(shell.keypadPanel).compactMap({ $0 as? ActionButton }).first(where: { $0.title == keyTitle }) else {
            throw ShellError.message("Missing keypad button " + keyTitle)
          }
          try ShellChecks.need(visible(keyBtn, in: content) && keyBtn.bounds.height >= 31, name + " keeps docked keypad button " + keyTitle + " fully visible without covering LCD")
        }
        let lcdInContent = shell.lcd.convert(shell.lcd.bounds, to: content)
        let kpInContent = shell.keypadPanel.convert(shell.keypadPanel.bounds, to: content)
        try ShellChecks.need(!lcdInContent.intersects(kpInContent.insetBy(dx: 1, dy: 1)), name + " docked keypad never overlaps LCD")
      }
      try ShellChecks.need(shell.railPanel.isHidden == expanded, name + " keeps header visibility accurate")
      if wide && !expanded { try ShellChecks.need(abs(shell.railPanel.frame.width - 248) < 1, name + " keeps common 248pt rail") }
      if !expanded {
        let targets = views(shell.rail).compactMap { $0 as? ActionButton }.map { $0.title }
        try ShellChecks.need(targets.contains("Connect and start") && targets.contains("Get security access") && targets.contains("Run without an adapter"), name + " exposes all common launch actions")
        try ShellChecks.need(shell.railScroll.documentView === shell.rail && shell.railScroll.hasVerticalScroller, name + " keeps compact details scrollable independently of keys")
        guard let connect = shell.connect else { throw ShellError.message("Missing Connect action") }
        let clip = shell.railScroll.contentView
        try ShellChecks.need(clip.bounds.contains(connect.convert(connect.bounds, to: clip)),
          name + " shows the complete Connect target without scrolling")
      }
      guard let bitmap = content.bitmapImageRepForCachingDisplay(in: content.bounds) else { throw ShellError.message("Cannot render workspace") }
      content.cacheDisplay(in: content.bounds, to: bitmap)
      guard let png = bitmap.representation(using: .png, properties: [:]) else { throw ShellError.message("Cannot save workspace") }
      try png.write(to: output.appendingPathComponent(name + ".png"))
    }
    try ShellChecks.need(shell.session == nil && shell.bluetoothWorkflow == nil && shell.securityJob == nil, "design checks sent no firmware keys, vehicle commands or security requests")
    print("Native design checks passed; ready-state and REA-docked keypad layouts verified, no hardware qualification.")
  }
}
