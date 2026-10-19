// SPDX-License-Identifier: MPL-2.0
import AppKit
import Darwin
import Foundation

let shellVersion = Bundle.main.object(forInfoDictionaryKey:"CFBundleShortVersionString") as? String ?? "development"
// The packaged common contract is the design source for both Mac architectures.
enum WorkspaceTheme {
  static let colors: [String: String] = {
    guard let url = Bundle.main.url(forResource: "interface-contract", withExtension: "json"),
      let data = try? Data(contentsOf: url),
      let spec = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let layout = spec["layout"] as? [String: Any] else { return [:] }
    return layout["colors"] as? [String: String] ?? [:]
  }()
  static func color(_ name: String, _ fallback: String) -> NSColor {
    let hex = colors[name] ?? fallback
    let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
    return NSColor(srgbRed: CGFloat((value >> 16) & 255) / 255,
      green: CGFloat((value >> 8) & 255) / 255, blue: CGFloat(value & 255) / 255, alpha: 1)
  }
  static let background = color("background", "#101318")
  static let foreground = color("foreground", "#f3f6fb")
  static let muted = color("muted", "#adb8c6")
  static let surface = color("surface", "#191e26")
  static let button = color("button", "#293240")
  static let primary = color("primary_button", "#256c98")
  static let border = color("border", "#3b4656")
  static let primaryBorder = color("primary_border", "#75c9f2")
  static let display = color("display", "#080c11")
}
let background = WorkspaceTheme.background
let foreground = WorkspaceTheme.foreground
final class TopAlignedClip: NSClipView {
  override var isFlipped: Bool { true }
}
final class HeaderStack: NSStackView {
  override var isFlipped: Bool { true }
}
final class ActionButton: NSButton {
  override var acceptsFirstResponder: Bool { true }
  override var canBecomeKeyView: Bool { isEnabled && !isHiddenOrHasHiddenAncestor }
  var summaryStyle = false
  var quietStyle = false
  var activeStyle = false { didSet { needsDisplay = true } }
  private var hovering = false
  private var hoverArea: NSTrackingArea?
  var textColor: NSColor = foreground { didSet { needsDisplay = true } }
  var actionBlock: () -> Void
  init(_ title: String, minHeight: CGFloat = 36, _ block: @escaping () -> Void) {
    actionBlock = block
    super.init(frame: .zero)
    self.title = title
    target = self
    action = #selector(invoke)
    isBordered = false
    setButtonType(.momentaryPushIn)
    controlSize = .regular
    font = .systemFont(ofSize: 13, weight: .medium)
    if ["S1", "S2", "S3", "S4", "EXIT", "HELP", "↑", "↓", "←", "→", "YES", "NO", "ENTER",
        "F0", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9"].contains(title) {
      font = .monospacedSystemFont(ofSize: 12.5, weight: .semibold)
    }
    focusRingType = .exterior
    contentTintColor = foreground
    setAccessibilityLabel(title)
    heightAnchor.constraint(greaterThanOrEqualToConstant: minHeight).isActive = true
  }
  required init?(coder: NSCoder) { fatalError() }
  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let area = hoverArea { removeTrackingArea(area) }
    let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self)
    addTrackingArea(area)
    hoverArea = area
  }
  override func mouseEntered(with event: NSEvent) { hovering = true; needsDisplay = true }
  override func mouseExited(with event: NSEvent) { hovering = false; needsDisplay = true }
  override var focusRingMaskBounds: NSRect { bounds }
  override func drawFocusRingMask() {
    NSBezierPath(roundedRect: bounds.insetBy(dx: 2, dy: 2), xRadius: 8, yRadius: 8).fill()
  }
  override func draw(_ dirtyRect: NSRect) {
    let primary = title == "Connect and start"
    let base = primary ? WorkspaceTheme.primary : (activeStyle ? WorkspaceTheme.button : (quietStyle ? WorkspaceTheme.surface : WorkspaceTheme.button))
    let fill = isEnabled ? base : base.blended(withFraction: 0.45, of: background) ?? base
    // Darken pressed controls to preserve text contrast, including the primary action.
    let feedback: CGFloat = isEnabled ? (isHighlighted ? 0.12 : (hovering ? 0.04 : 0)) : 0
    (fill.blended(withFraction: feedback, of: isHighlighted ? .black : .white) ?? fill).setFill()
    let shape = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 8, yRadius: 8)
    shape.fill()
    let edge = (primary || activeStyle) && isEnabled ? (activeStyle ? WorkspaceTheme.primaryBorder : WorkspaceTheme.primary) : WorkspaceTheme.border
    edge.withAlphaComponent(quietStyle && !activeStyle && !hovering && !isHighlighted ? 0.45 : 1).setStroke()
    shape.stroke()
    if summaryStyle {
      let lines = title.components(separatedBy: "\n")
      for (index, line) in lines.prefix(3).enumerated() {
        let style = NSMutableParagraphStyle(); style.lineBreakMode = .byTruncatingTail
        let attrs: [NSAttributedString.Key: Any] = [.paragraphStyle: style,
          .font: NSFont.systemFont(ofSize: index == 0 ? 13 : 11.5, weight: index == 0 ? .semibold : .regular),
          .foregroundColor: index == 0 ? foreground : textColor]
        let y = isFlipped ? 8 + CGFloat(index) * 15 : bounds.height - 21 - CGFloat(index) * 15
        (line as NSString).draw(in: NSRect(x: 10, y: y,
          width: max(1, bounds.width - 36), height: 16), withAttributes: attrs)
      }
      ("›" as NSString).draw(in: NSRect(x: bounds.maxX - 18, y: bounds.midY - 11, width: 12, height: 22),
        withAttributes: [.font: NSFont.systemFont(ofSize: 17), .foregroundColor: WorkspaceTheme.muted])
      return
    }
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = summaryStyle ? .left : .center
    paragraph.lineBreakMode = summaryStyle ? .byTruncatingTail : .byWordWrapping
    let attrs: [NSAttributedString.Key: Any] = [.paragraphStyle: paragraph, .font: font ?? NSFont.systemFont(ofSize: 13),
      .foregroundColor: isEnabled ? textColor : WorkspaceTheme.muted.withAlphaComponent(0.65)]
    let text = title as NSString
    let size = text.boundingRect(with: NSSize(width: max(1, bounds.width - 12), height: 100), options: [.usesLineFragmentOrigin], attributes: attrs).size
    text.draw(in: NSRect(x: 6, y: (bounds.height-size.height)/2, width: max(1, bounds.width-12), height: size.height), withAttributes: attrs)
  }
  @objc func invoke() { actionBlock() }
}
final class FirmwareLCD: NSView {
  var image: CGImage? { didSet { needsDisplay = true } }
  /// Shown instead of a blank display after a communication failure; cleared by a new session.
  var caption: String? { didSet { needsDisplay = true } }
  var send: (Int) -> Void = { _ in }
  private var down = NSPoint.zero, began = Date(), wheel: CGFloat = 0
  override var acceptsFirstResponder: Bool { true }
  override init(frame: NSRect) {
    super.init(frame: frame)
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel("Tech 2 firmware display")
  }
  required init?(coder: NSCoder) { fatalError() }
  override func setFrameSize(_ newSize: NSSize) {
    super.setFrameSize(newSize)
    needsDisplay = true
  }
  private var renderedImageRect: CGRect {
    let scale = min(max(1, bounds.width - 20) / 320, max(1, bounds.height - 20) / 240)
    let w = scale * 320
    let h = scale * 240
    return CGRect(x: (bounds.width - w) / 2, y: (bounds.height - h) / 2, width: w, height: h)
  }
  override func draw(_ dirtyRect: NSRect) {
    let bezel = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 12, yRadius: 12)
    WorkspaceTheme.display.setFill(); bezel.fill()
    WorkspaceTheme.border.setStroke(); bezel.stroke()
    guard let image = image, let ctx = NSGraphicsContext.current?.cgContext else {
      let failed = caption != nil
      let text = (caption ?? "Your Tech 2 screen appears here") as NSString
      let style = NSMutableParagraphStyle()
      style.alignment = .center
      let attrs: [NSAttributedString.Key: Any] = [
        .foregroundColor: failed ? NSColor.systemOrange : foreground,
        .font: NSFont.systemFont(ofSize: 15, weight: .semibold), .paragraphStyle: style,
      ]
      let box = bounds.insetBy(dx: 24, dy: 0)
      let height = text.boundingRect(with: NSSize(width: box.width, height: .greatestFiniteMagnitude),
        options: [.usesLineFragmentOrigin], attributes: attrs).height
      text.draw(with: NSRect(x: box.minX, y: bounds.midY - height / 2, width: box.width, height: height),
        options: [.usesLineFragmentOrigin], attributes: attrs)
      if !failed {
        // An original line illustration, separate from the real firmware framebuffer.
        let icon = NSBezierPath(roundedRect: NSRect(x: bounds.midX - 23, y: bounds.midY + 38, width: 46, height: 34), xRadius: 6, yRadius: 6)
        WorkspaceTheme.primaryBorder.setStroke(); icon.lineWidth = 1.5; icon.stroke()
        let keys = NSBezierPath()
        for x in [-14, -5, 5, 14] { keys.move(to: NSPoint(x: bounds.midX + CGFloat(x), y: bounds.midY + 30)); keys.line(to: NSPoint(x: bounds.midX + CGFloat(x) + 4, y: bounds.midY + 30)) }
        keys.lineWidth = 2; keys.stroke()
        let helper = "Connect and start, or explore the original menus offline." as NSString
        helper.draw(in: NSRect(x: box.minX, y: bounds.midY - height / 2 - 42, width: box.width, height: 34),
          withAttributes: [.foregroundColor: WorkspaceTheme.muted, .font: NSFont.systemFont(ofSize: 12), .paragraphStyle: style])
      }
      return
    }
    ctx.interpolationQuality = .none
    ctx.draw(image, in: renderedImageRect)
  }
  override func mouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
    down = event.locationInWindow
    began = Date()
  }
  override func mouseUp(with event: NSEvent) {
    let d = event.locationInWindow.y - down.y
    if abs(d) > 25 {
      send(d > 0 ? 9 : 12)
    } else if Date().timeIntervalSince(began) >= 0.45 {
      send(16)
    } else if image != nil {
      // REA Tech2Win parity: clicking the bottom softkey strip of the 320x240 LCD triggers S1..S4.
      let local = convert(event.locationInWindow, from: nil)
      let rect = renderedImageRect
      let softHeight = rect.height * (28.0 / 240.0)
      if rect.contains(local), local.y <= rect.minY + softHeight {
        let col = min(3, max(0, Int((local.x - rect.minX) / (rect.width / 4.0))))
        send([10, 6, 7, 8][col])
      }
    }
  }
  override func scrollWheel(with event: NSEvent) {
    wheel += event.scrollingDeltaY
    if abs(wheel) >= 3 {
      send(wheel > 0 ? 9 : 12)
      wheel = 0
    }
  }
}
final class Cancellation {
  private let lock = NSLock()
  private var value = false
  var cancelled: Bool {
    lock.lock()
    defer { lock.unlock() }
    return value
  }
  func cancel() {
    lock.lock()
    value = true
    lock.unlock()
  }
}
final class MenuAction {
  let action: () -> Void
  init(_ action: @escaping () -> Void) { self.action = action }
}
final class Shell: NSObject, NSApplicationDelegate, NSWindowDelegate {
  var window: NSWindow!, store: FirmwareStore
  var session: EmulatorSession?, timer: Timer?, eventMonitor: Any?, busy = false, quitting = false
  var privateAutomation: PrivateAutomation?
  var appMenuButton: ActionButton?, actionsButton: ActionButton?
  var vehicleSummaryButton: ActionButton?, vehicleStatusSnapshot: VehicleStatusView?
  var vehicleStatusContext = "", vehicleStatusGeneration = 0, vehicleStatusPending = false
  var vehicleStatusPoll = Date.distantPast
  var securityJob: SecurityJob?, securityActionButton: ActionButton!, securityResumeAdapter: USBAdapterEndpoint?, securityAdapterIdentity: String?
  var securityTransferRun: URL?, securityNavigation: [String:Any]=[:], securityNavigating=false, securityChecking=false
  var securityWorkflowPoll=Date.distantPast, securityNavigationGeneration=0
  var securityPoll = Date.distantPast, securityButton: ActionButton!
  var securityView = SecurityStatus.evaluate(nil)
  let dtcObserver = DtcObserver()
  var dtcReportButton: ActionButton!
  var dtcSharePicker: NSSharingServicePicker?
  var lastConsole = "", lockFD: Int32 = -1
  var connectionAttempt: ConnectionAttempt?, sessionBegan = Date()
  var bluetoothWorkflow: MacBluetoothWorkflow?, bluetoothPicker: NSAlert?
  var bluetoothVehicle: [String:Any] = [:]
  var menuNavigation: [String: Any] = [:], menuNavigating = false, menuChecking = false
  var menuPoll = Date.distantPast, menuNavigationGeneration = 0
  let state: URL, binaries: URL, catalog: URL
  let lcd = FirmwareLCD(frame: .zero),
    status = NSTextField(wrappingLabelWithString: "Ready"),
    vehicle = NSTextField(wrappingLabelWithString: "Connect an adapter to identify it"),
    selection = NSTextField(wrappingLabelWithString: "")
  let rail = HeaderStack(), body = NSStackView(), main = NSStackView()
  let emulatorColumn = NSStackView(), workspaceArea = NSStackView(), keypadPanel = NSView()
  let railScroll = NSScrollView()
  let railPanel = NSView(), workspaceHeader = NSStackView()
  var sessionSectionLabel: NSTextField?
  var expandButton: ActionButton?, keypadToggleButton: ActionButton?
  var expanded = false, keypadVisible = false, layoutMode = ""
  var layoutConstraints: [NSLayoutConstraint] = []
  var offline: ActionButton!, connect: ActionButton!, firmwareButton: ActionButton!
  var downloadTask: URLSessionDownloadTask?, downloadSession: URLSession?,
    cancellation: Cancellation?
  var firmwareWindow: NSWindow?, downloadButton: ActionButton?, choices: NSPopUpButton?,
    downloadStatus: NSTextField?
  var languageChoices: NSPopUpButton?
  var firmwareEntries: [FirmwareEntry] = []
  let languageCodes = ["en", "de", "es", "fi", "fr", "it", "nl", "ru", "se"]
  let languageNames = ["English", "Deutsch", "Español", "Suomi", "Français", "Italiano", "Nederlands", "Русский", "Svenska"]
  init(state: URL, binaries: URL, catalog: URL) throws {
    self.state = state
    self.binaries = binaries
    self.catalog = catalog
    store = try FirmwareStore(state: state, catalog: catalog)
    super.init()
  }
  func label(_ text: String, size: CGFloat = 13) -> NSTextField {
    let l = NSTextField(wrappingLabelWithString: text)
    l.font = .systemFont(ofSize: size)
    l.textColor = foreground
    return l
  }
  func sectionLabel(_ text: String) -> NSTextField {
    let l = label(text, size: 10)
    l.attributedStringValue = NSAttributedString(string: text, attributes: [
      .font: NSFont.systemFont(ofSize: 10, weight: .semibold), .foregroundColor: WorkspaceTheme.muted, .kern: 1.1])
    return l
  }
  func toggleExpanded() {
    expanded.toggle()
    arrange()
  }
  func stack(_ views: [NSView], vertical: Bool = false) -> NSStackView {
    let s = NSStackView(views: views)
    s.orientation = vertical ? .vertical : .horizontal
    s.spacing = 6
    s.alignment = vertical ? .leading : .centerY
    if !vertical { s.distribution = .fillEqually }
    return s
  }
  func pin(_ view: NSView, inside: NSView, inset: CGFloat = 12) {
    view.translatesAutoresizingMaskIntoConstraints = false
    NSLayoutConstraint.activate([
      view.leadingAnchor.constraint(equalTo: inside.leadingAnchor, constant: inset),
      view.trailingAnchor.constraint(equalTo: inside.trailingAnchor, constant: -inset),
      view.topAnchor.constraint(equalTo: inside.topAnchor, constant: inset),
      view.bottomAnchor.constraint(equalTo: inside.bottomAnchor, constant: -inset),
    ])
  }
  func applicationDidFinishLaunching(_ notification: Notification) {
    lockFD = open(state.appendingPathComponent("app.lock").path, O_CREAT | O_RDWR, 0o600)
    guard lockFD >= 0, flock(lockFD, LOCK_EX | LOCK_NB) == 0 else {
      showError("OpenSAAB is already running. Return to its existing window.")
      NSApp.terminate(nil)
      return
    }
    var setupError: String?
    do { try BundledRuntime.ensure(resources: Bundle.main.resourceURL!, store: store) }
    catch { setupError = error.localizedDescription }
    NSApp.appearance = NSAppearance(named: .darkAqua)
    let menu = NSMenu()
    let app = NSMenuItem()
    let sub = NSMenu()
    sub.addItem(withTitle: "About OpenSAAB T2", action: #selector(about), keyEquivalent: "")
      .target = self
    sub.addItem(.separator())
    sub.addItem(
      withTitle: "Quit OpenSAAB", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"
    )
    app.submenu = sub
    menu.addItem(app)
    NSApp.mainMenu = menu
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1060, height: 740),
      styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
    )
    window.title = "OpenSAAB \(shellVersion) \(shellArchitectureLabel)"
    window.minSize = NSSize(width: 500, height: 600)
    window.backgroundColor = background
    window.contentView?.wantsLayer = true
    window.contentView?.layer?.backgroundColor = background.cgColor
    window.delegate = self
    window.isReleasedWhenClosed = false
    let brand = label("OpenSAAB T2", size: 15)
    brand.font = .boldSystemFont(ofSize: 15)
    brand.maximumNumberOfLines = 1
    let identity = label(shellVersion + " · " + shellArchitectureLabel, size: 10)
    identity.textColor = WorkspaceTheme.muted
    let branding = stack([brand,identity],vertical:true); branding.spacing = 2
    let more = ActionButton("⋮", minHeight: 36) { [weak self] in self?.appMenu() }
    more.quietStyle = true
    more.setAccessibilityLabel("App menu")
    appMenuButton = more
    more.widthAnchor.constraint(equalToConstant: 36).isActive = true
    let logo = NSImageView()
    logo.image = NSImage(contentsOf: Bundle.main.resourceURL!.appendingPathComponent("opensaab.png"))
    logo.imageScaling = .scaleProportionallyUpOrDown
    logo.setAccessibilityLabel("OpenSAAB logo")
    logo.widthAnchor.constraint(equalToConstant: 32).isActive = true
    logo.heightAnchor.constraint(equalToConstant: 32).isActive = true
    branding.setContentHuggingPriority(.defaultHigh, for: .horizontal)
    let title = NSStackView(views: [logo, branding, NSView(), more])
    title.spacing = 6
    connect = ActionButton("Connect and start") { [weak self] in self?.detectAdapter() }
    dtcReportButton = ActionButton("Saved DTC reports") { [weak self] in self?.savedDtcReports() }
    dtcReportButton.isHidden = true
    offline = ActionButton("Run without an adapter") { [weak self] in self?.start() }
    offline.quietStyle = true
    firmwareButton = ActionButton("Firmware selection") { [weak self] in self?.firmwareDialog() }
    let support = ActionButton("Donate · Support OpenSAAB", minHeight: 32) { [weak self] in self?.projectSupport() }
    support.quietStyle = true
    support.font = .systemFont(ofSize: 12, weight: .medium)
    for l in [vehicle, status, selection] {
      l.textColor = foreground
      l.font = .systemFont(ofSize: 13)
    }
    status.textColor = WorkspaceTheme.primaryBorder
    rail.orientation = .vertical
    rail.alignment = .leading
    rail.spacing = 6
    securityButton = ActionButton("Security: [N/A]") { self.securityDetails() }
    let summary = ActionButton("Ready\nNo vehicle connected", minHeight: 58) { self.securityDetails() }
    summary.summaryStyle = true; summary.font = .systemFont(ofSize:12)
    summary.textColor = WorkspaceTheme.muted
    summary.heightAnchor.constraint(equalToConstant:58).isActive = true
    vehicleSummaryButton = summary
    securityActionButton = ActionButton("Get security access") { self.securityAction() }
    let sessionLabel = sectionLabel("SESSION")
    sessionSectionLabel = sessionLabel
    let softwareLabel = sectionLabel("DIAGNOSTIC SOFTWARE")
    selection.font = .systemFont(ofSize: 12, weight: .medium)
    let softwareHelp = label("Change software in App menu.", size: 12)
    softwareHelp.textColor = WorkspaceTheme.muted
    let offlineHelp = label("Offline mode does not connect to a vehicle.", size: 12)
    offlineHelp.textColor = WorkspaceTheme.muted
    let divider = NSView()
    divider.wantsLayer = true
    divider.layer?.backgroundColor = WorkspaceTheme.border.cgColor
    divider.heightAnchor.constraint(equalToConstant: 1).isActive = true
    let software = stack([divider, softwareLabel, selection, softwareHelp], vertical: true)
    software.spacing = 6
    for v in [divider, softwareLabel, selection, softwareHelp] {
      v.widthAnchor.constraint(equalTo: software.widthAnchor).isActive = true
    }
    // Android reference hierarchy; firmware selection remains in App menu.
    for v in [title, sessionLabel, summary, connect!, securityActionButton!, offline!, offlineHelp, software] {
      rail.addArrangedSubview(v)
      v.widthAnchor.constraint(equalTo: rail.widthAnchor).isActive = true
    }
    rail.setCustomSpacing(12, after: offlineHelp)
    railScroll.hasVerticalScroller = true
    railScroll.autohidesScrollers = true
    railScroll.drawsBackground = false
    railScroll.contentView = TopAlignedClip()
    railScroll.contentView.drawsBackground = false
    railScroll.documentView = rail
    rail.translatesAutoresizingMaskIntoConstraints = false
    rail.widthAnchor.constraint(equalTo: railScroll.contentView.widthAnchor).isActive = true
    railPanel.wantsLayer = true
    railPanel.layer?.backgroundColor = WorkspaceTheme.surface.cgColor
    railPanel.layer?.borderColor = WorkspaceTheme.border.cgColor
    railPanel.layer?.borderWidth = 1
    railPanel.layer?.cornerRadius = 12
    railPanel.addSubview(railScroll)
    pin(railScroll, inside: railPanel, inset: 10)
    main.orientation = .vertical
    main.alignment = .leading
    main.spacing = 6
    let workspaceTitle = stack([sectionLabel("TECH 2"), label("Diagnostic workspace", size: 16)], vertical: true)
    workspaceTitle.spacing = 2
    let expand = ActionButton("Expand screen", minHeight: 32) { [weak self] in self?.toggleExpanded() }
    expand.quietStyle = true; expand.font = .systemFont(ofSize: 12, weight: .medium)
    expand.widthAnchor.constraint(equalToConstant: 124).isActive = true
    expandButton = expand
    workspaceHeader.orientation = .horizontal
    workspaceHeader.alignment = .centerY
    workspaceHeader.addArrangedSubview(workspaceTitle)
    workspaceHeader.addArrangedSubview(NSView())
    workspaceHeader.addArrangedSubview(expand)
    let soft = stack(
      zip(["S1", "S2", "S3", "S4"], [10, 6, 7, 8]).map { name, code in
        ActionButton(name, minHeight: 36) { [weak self] in self?.key(code) }
      })
    let hint = label("Scroll / swipe ↑ ↓ · Hold for ENTER · Click LCD softkeys or S1–S4 · EXIT goes back", size: 11)
    hint.alignment = .center
    hint.textColor = WorkspaceTheme.muted
    let actionMenu = ActionButton("Actions", minHeight: 36) { [weak self] in self?.actions() }
    actionMenu.quietStyle = true
    actionsButton = actionMenu
    let keypadButton = ActionButton("Keypad", minHeight: 36) { [weak self] in self?.keypad() }
    keypadButton.quietStyle = true
    keypadToggleButton = keypadButton
    let controls = stack([
      ActionButton("EXIT", minHeight: 36) { [weak self] in self?.key(1) },
      keypadButton,
      actionMenu,
    ])
    emulatorColumn.orientation = .vertical
    emulatorColumn.alignment = .leading
    emulatorColumn.spacing = 6
    for v in [lcd, soft, hint, controls] {
      emulatorColumn.addArrangedSubview(v)
      v.widthAnchor.constraint(equalTo: emulatorColumn.widthAnchor).isActive = true
    }
    // REA Tech2Win docked 3-column hardware keypad (never covers the 320x240 LCD).
    let keypadRows: [[(String, Int)]] = [
      [("HELP", 25), ("↑", 9), ("YES", 14)],
      [("←", 11), ("ENTER", 16), ("→", 13)],
      [("", -1), ("↓", 12), ("NO", 20)],
      [("F1", 4), ("F2", 19), ("F3", 23)],
      [("F4", 3), ("F5", 18), ("F6", 22)],
      [("F7", 2), ("F8", 17), ("F9", 21)],
      [("", -1), ("F0", 24), ("", -1)],
    ]
    let kpHeader = NSStackView(views: [sectionLabel("KEYPAD"), NSView()])
    kpHeader.orientation = .horizontal
    kpHeader.alignment = .centerY
    var kpViews: [NSView] = [kpHeader]
    for (rowIndex, row) in keypadRows.enumerated() {
      if rowIndex == 3 {
        let sep = NSView()
        sep.wantsLayer = true
        sep.layer?.backgroundColor = WorkspaceTheme.border.cgColor
        sep.heightAnchor.constraint(equalToConstant: 1).isActive = true
        kpViews.append(sep)
      }
      let r = stack(row.map { name, code in
        if code < 0 { return NSView() }
        return ActionButton(name, minHeight: 32) { [weak self] in self?.key(code) }
      })
      r.spacing = 6
      kpViews.append(r)
    }
    let kpStack = HeaderStack(views: kpViews)
    kpStack.orientation = .vertical
    kpStack.alignment = .leading
    kpStack.spacing = 6
    for v in kpViews { v.widthAnchor.constraint(equalTo: kpStack.widthAnchor).isActive = true }
    keypadPanel.wantsLayer = true
    keypadPanel.layer?.backgroundColor = WorkspaceTheme.surface.cgColor
    keypadPanel.layer?.borderColor = WorkspaceTheme.border.cgColor
    keypadPanel.layer?.borderWidth = 1
    keypadPanel.layer?.cornerRadius = 12
    keypadPanel.addSubview(kpStack)
    pin(kpStack, inside: keypadPanel, inset: 10)
    keypadPanel.translatesAutoresizingMaskIntoConstraints = false
    keypadPanel.widthAnchor.constraint(equalToConstant: 192).isActive = true
    keypadPanel.isHidden = true
    workspaceArea.orientation = .horizontal
    workspaceArea.alignment = .top
    workspaceArea.distribution = .fill
    workspaceArea.spacing = 12
    workspaceArea.addArrangedSubview(emulatorColumn)
    workspaceArea.addArrangedSubview(keypadPanel)
    emulatorColumn.heightAnchor.constraint(equalTo: workspaceArea.heightAnchor).isActive = true
    for v in [workspaceHeader, workspaceArea] {
      main.addArrangedSubview(v)
      v.widthAnchor.constraint(equalTo: main.widthAnchor).isActive = true
    }
    lcd.heightAnchor.constraint(greaterThanOrEqualToConstant: 160).isActive = true
    lcd.setContentHuggingPriority(.defaultLow, for: .vertical)
    lcd.send = { [weak self] in self?.key($0) }
    body.orientation = .horizontal
    body.alignment = .top
    body.distribution = .fill
    body.spacing = 12
    body.addArrangedSubview(railPanel)
    body.addArrangedSubview(main)
    let shell = NSStackView(views: [body, support])
    shell.orientation = .vertical
    shell.alignment = .leading
    shell.spacing = 8
    body.widthAnchor.constraint(equalTo: shell.widthAnchor).isActive = true
    support.widthAnchor.constraint(equalTo: shell.widthAnchor).isActive = true
    support.heightAnchor.constraint(equalToConstant: 32).isActive = true
    window.contentView!.addSubview(shell)
    pin(shell, inside: window.contentView!, inset: 10)
    arrange()
    let t = Timer(timeInterval: 0.05, repeats: true) { [weak self] _ in self?.tick() }
    timer = t
    RunLoop.main.add(t, forMode: .common)
    RunLoop.main.add(t, forMode: .modalPanel)
    privateAutomation?.start()
    eventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      guard let self = self, self.window.isKeyWindow, !event.modifierFlags.contains(.command) else {
        return event
      }
      if event.keyCode == 48 {
        if event.modifierFlags.contains(.shift) { self.window.selectPreviousKeyView(nil) }
        else { self.window.selectNextKeyView(nil) }
        return nil
      }
      let codes: [UInt16: Int] = [
        36: 16, 76: 16, 53: 1, 126: 9, 125: 12, 123: 11, 124: 13, 122: 24, 120: 4, 99: 19, 118: 23,
        96: 3, 97: 18, 98: 22, 100: 2, 101: 17, 109: 21,
      ]
      if let code = codes[event.keyCode] {
        self.key(code)
        return nil
      }
      return event
    }
    refresh()
    window.center()
    window.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    if let error = setupError { showError(error) }
    else if store.selected == nil { firmwareDialog() }
  }
  func windowDidResize(_ notification: Notification) { arrange() }
  func arrange() {
    guard let content = window?.contentView else { return }
    let size = content.bounds.size
    let wide = size.width >= 720 && size.width > size.height
    let mode = expanded ? "expanded" : (wide ? "wide" : "compact")
    keypadPanel.isHidden = !keypadVisible
    keypadToggleButton?.activeStyle = keypadVisible
    guard mode != layoutMode else { lcd.needsDisplay = true; return }
    layoutMode = mode
    // The summary already names its state. Compact chrome saves enough space
    // for a complete Connect target even at the minimum supported window size.
    sessionSectionLabel?.isHidden = !wide && !expanded
    rail.spacing = !wide && !expanded ? 5 : 6
    NSLayoutConstraint.deactivate(layoutConstraints)
    railPanel.isHidden = expanded
    workspaceHeader.isHidden = !wide && !expanded
    expandButton?.title = expanded ? "Show header" : "Expand screen"
    expandButton?.setAccessibilityLabel(expandButton?.title ?? "Expand screen")
    body.orientation = wide || expanded ? .horizontal : .vertical
    body.alignment = wide || expanded ? .top : .leading
    if expanded {
      layoutConstraints = [main.widthAnchor.constraint(equalTo: body.widthAnchor),
                           main.heightAnchor.constraint(equalTo: body.heightAnchor)]
    } else if wide {
      layoutConstraints = [railPanel.widthAnchor.constraint(equalToConstant: 248),
        railPanel.heightAnchor.constraint(equalTo: body.heightAnchor),
        main.heightAnchor.constraint(equalTo: body.heightAnchor),
        main.trailingAnchor.constraint(equalTo: body.trailingAnchor)]
    } else {
      // A proportional, scrollable header preserves the minimum window size.
      // Fixed preferred heights also participate in AppKit's window fitting size.
      let height = railPanel.heightAnchor.constraint(equalTo: body.heightAnchor, multiplier: 0.37)
      height.priority = .defaultHigh
      layoutConstraints = [railPanel.widthAnchor.constraint(equalTo: body.widthAnchor),
        height,
        railPanel.heightAnchor.constraint(lessThanOrEqualToConstant: 202),
        main.widthAnchor.constraint(equalTo: body.widthAnchor),
        main.bottomAnchor.constraint(equalTo: body.bottomAnchor)]
    }
    NSLayoutConstraint.activate(layoutConstraints)
    lcd.needsDisplay = true
  }
  func refresh() {
    updateSecurity()
    selection.stringValue = store.selected?.label ?? "Choose a firmware language"
    connect.isEnabled = !busy && session == nil
    offline.isEnabled = !busy && session == nil
    firmwareButton.isEnabled = !busy
    securityActionButton?.isEnabled = !busy || securityJob != nil
  }
  func securityObservation() -> [String:String] {
    guard let data=try? Data(contentsOf:state.appendingPathComponent("vehicle-status.json")),data.count<=8192,
          let value=try? JSONSerialization.jsonObject(with:data) as? [String:String] else{return [:]};return value
  }
  func updateSecurity() {
    securityPoll=Date()
    var card=store.selected.map{store.card($0)}
    if let run=session?.run {card=run.appendingPathComponent("card.bin")}
    else if let firmware=store.selected?.id, let run=SecurityWorkspace.previousRun(state:state,firmware:firmware) {
      card=run.appendingPathComponent("card.bin")
    }
    let local=card?.deletingLastPathComponent().appendingPathComponent("security-processing-status.properties")
    let receipt=(local != nil && fm.fileExists(atPath:local!.path)) ? local! : state.appendingPathComponent("security-processing-status.properties")
    securityView=SecurityStatus.read(card,receipt:receipt,vehicle:securityObservation()["vin"])
    securityButton?.title=securityView.summary
    securityButton?.setAccessibilityLabel(securityView.summary)
    if let rgb=UInt32(securityView.color.dropFirst(),radix:16) {
      securityButton?.textColor=NSColor(calibratedRed:CGFloat((rgb>>16)&255)/255,green:CGFloat((rgb>>8)&255)/255,blue:CGFloat(rgb&255)/255,alpha:1)
    }
    securityButton?.toolTip="Vehicle and security status — view details"
    requestVehicleStatus()
  }
  func securityDetails() {
    updateSecurity()
    let details = vehicleStatusSnapshot?.details ?? "auth_status: [N/A]\nTimestamp unavailable\nShared vehicle history is loading or unavailable.\nSaved emulator data · Vehicle access not verified"
    textDialog("Vehicle and security details","Diagnostic software: " + (store.selected?.label ?? "None selected") + "\n\n" + bluetoothDetails()+details)
  }
  func key(_ code: Int) {
    guard !busy else { return }
    if menuNavigating { status.stringValue = "Shortcut stopped · Manual control" }
    securityNavigating=false;securityNavigationGeneration+=1
    menuNavigating=false;menuNavigationGeneration+=1
    recordOperation("manual_input")
    session?.key(code)
  }
  func tick() {
    if Date().timeIntervalSince(securityPoll) >= 1 { updateSecurity() }
    renderVehicleStatus()
    guard let current = session, !busy else { return }
    updateMDISession(current)
    if !current.running {
      lastConsole = current.consoleText
      let code = current.process.terminationStatus
      if current.mx != nil {
        // Natural MX+ exit: release Bluetooth and drain the console off the main
        // thread, then classify. Connect stays disabled until the release is known.
        session = nil; lcd.image = nil; busy = true
        status.stringValue = "MX+ session ended · Releasing Bluetooth…"; refresh()
        DispatchQueue.global().async {
          let clean = current.stop()
          DispatchQueue.main.async { self.busy = false; self.mxSessionEnded(current, clean: clean, userStop: false) }
        }
        return
      }
      // The emulator classifies adapter stops in shared Rust (adapter_failure.rs) and writes
      // the result to report.json; this shell only presents it.
      let failure = AdapterFailure.read(run: current.run)
      status.stringValue = failure.map { $0.title + " · See Console." }
        ?? "Session ended · exit \(code) · See Console"
      let endReason = failure?.sessionEndReason ?? "exited"
      recordSessionEnd(current.run, reason: endReason, exitCode: code, began: sessionBegan)
      if code == 0 && failure == nil { connectionAttempt?.finish("COMPLETED") }
      else { connectionAttempt?.finish("FAILED", failure?.connectionReason ?? "PROTOCOL_OR_PROCESS_ERROR") }
      connectionAttempt = nil
      if failure == nil && code != 0 { recordAppError(status.stringValue, kind: "session_error") }
      session = nil
      lcd.image = nil
      lcd.caption = failure.map { "Communication stopped\n" + $0.title }
      vehicle.stringValue = "Session ended · Connect again for a fresh VIN"
      refresh()
      if let failure = failure { sessionFailure(failure.message) }
      else if code != 0 { sessionFailure("The diagnostic session ended unexpectedly (exit \(code)).") }
      return
    }
    observeDtc(current)
    securityWorkflowTick(current)
    menuShortcutTick(current)
    try? current.pump()
    if let frame = try? current.frame() { lcd.image = frame }
  }
  func start(chipsoftPort: String? = nil, adapter: USBAdapterEndpoint? = nil, mode: ChipsoftSessionMode = .readOnly, freshSecurity: Bool = false, mx: MacMXFirmwareBridge? = nil) {
    guard !busy, session == nil else { return }
    if (try? store.validateCore()) == nil {
      do { try BundledRuntime.ensure(resources: Bundle.main.resourceURL!, store: store) }
      catch { showError(error.localizedDescription); return }
      if (try? store.validateCore()) == nil { importDialog(); return }
    }
    do {
      guard store.selected != nil else {
        firmwareDialog()
        return
      }
      guard chipsoftPort == nil || adapter == nil else { throw ShellError.message("Choose one adapter connection.") }
      let endpoint = adapter ?? chipsoftPort.map { USBAdapterEndpoint(kind: .chipsoft, port: $0, identity: securityAdapterIdentity) }
      session = try EmulatorSession(
        binary: binaries.appendingPathComponent("tech2-emu"), store: store, adapter: endpoint, mode: mode, freshSecurity: freshSecurity, mxBridge: mx)
      sessionBegan = Date()
      lcd.caption = nil
      menuNavigating = false; menuNavigationGeneration += 1
      if endpoint != nil { connectionAttempt?.bind(session!.run, mode: mode.rawValue); connectionAttempt?.advance("SESSION") } else { connectionAttempt = nil }
      lcd.image = nil
      dtcReportButton.isHidden = true
      if mx != nil {
        connectionAttempt = nil
        status.stringValue = MxSessionOutcome.readSummary + " · Starting original firmware"
      } else if endpoint == nil {
        bluetoothVehicle = [:]
        vehicle.stringValue = "Offline exploration · No vehicle connected"
        status.stringValue = "Emulation mode · Offline"
      } else {
        status.stringValue = mode.status(for: endpoint!.kind) + " · CANdi connects on demand."
      }
      refresh()
      window.makeFirstResponder(lcd)
    } catch {
      connectionAttempt?.finish("FAILED", "PROTOCOL_OR_PROCESS_ERROR"); connectionAttempt = nil
      showError(error.localizedDescription)
    }
  }
  /// One MX+ end-of-session presenter (Android runSession parity): an unsupported
  /// operation is reported as a capability limit even on stop; never a crash prompt.
  func mxSessionEnded(_ ended: EmulatorSession, clean: Bool, userStop: Bool) {
    lastConsole = ended.consoleText
    let code: Int32? = ended.process.isRunning ? nil : ended.process.terminationStatus
    let forced = ended.stopOutcome?.forcedProcessExit == true
    let released = ended.mx?.bluetoothClosed == true
    lcd.image = nil
    vehicle.stringValue = "Session ended · Connect again for a fresh VIN"
    if MxSessionOutcome.unsupported(mxSession: true, rejected: ended.mxPolicyRejected, forced: forced, launchFailed: false) {
      let outcome: [String:Any] = ["result":"unsupported_operation", "blocked_request_sent":false, "lifecycle":"UNSUPPORTED_OPERATION",
        "child_exit_code":code.map { Int($0) } ?? -1, "bluetooth_closed":released, "operator_stop":userStop]
      try? JSONSerialization.data(withJSONObject:outcome, options:[.sortedKeys]).write(to:ended.run.appendingPathComponent("mx-session-outcome.json"), options:.atomic)
      recordSessionEnd(ended.run, reason: "unsupported_operation", exitCode: code, began: sessionBegan)
      lcd.caption = "MX+ session ended\nOperation unavailable"
      status.stringValue = MxSessionOutcome.unavailableMessage + (released ? "" : " · Bluetooth release not confirmed")
      refresh(); return
    }
    if userStop {
      recordSessionEnd(ended.run, reason: ended.stopOutcome?.sessionEndReason ?? "unconfirmed_stop", exitCode: code, began: sessionBegan)
      status.stringValue = clean ? "MX+ session stopped · Bluetooth released" :
        forced ? "MX+ session stopped forcibly. See Console." : "MX+ session stopped · Bluetooth release not confirmed. See Console."
      lcd.caption = clean ? nil : status.stringValue
      refresh(); return
    }
    let failure = AdapterFailure.read(run: ended.run)
    status.stringValue = (failure.map { $0.title + " · See Console." } ?? "MX+ session ended · exit \(code ?? -1) · See Console")
      + (released ? "" : " · Bluetooth release not confirmed")
    recordSessionEnd(ended.run, reason: failure?.sessionEndReason ?? "exited", exitCode: code, began: sessionBegan)
    lcd.caption = failure.map { "Communication stopped\n" + $0.title }
    refresh()
    if let failure { sessionFailure(failure.message) }
    else if code != 0 { sessionFailure("The MX+ session ended unexpectedly (exit \(code ?? -1)).") }
  }
  func sessionFailure(_ text: String) {
    recordAppError(text, kind: "session_error")
    status.stringValue = text
    let a = NSAlert()
    a.messageText = "OpenSAAB T2"
    a.informativeText = text
    a.addButton(withTitle: "OK")
    a.addButton(withTitle: "Report problem…")
    if a.runModal() == .alertSecondButtonReturn {
      reportIssue(prefill: "The session stopped: \(text)\nWhat I was doing: ")
    }
  }
  func stop(_ completion: @escaping () -> Void = {}) {
    stopWithOutcome(onFailure: {}, completion: completion)
  }
  func stopWithOutcome(onFailure: @escaping () -> Void, completion: @escaping () -> Void) {
    if let workflow = bluetoothWorkflow {
      let finished = workflow.onComplete
      workflow.onComplete = { report in finished?(report); completion() }
      workflow.stop(); return
    }
    guard !busy else { return }
    guard let stopping = session else {
      completion()
      return
    }
    session = nil
    // The last screen remains in the session evidence, not in the live display.
    lcd.image = nil
    lcd.caption = nil
    busy = true
    status.stringValue = "Stopping emulation…"
    refresh()
    DispatchQueue.global().async {
      let clean = stopping.stop()
      let text = stopping.consoleText
      DispatchQueue.main.async {
        if stopping.mx != nil {
          self.busy = false; self.mxSessionEnded(stopping, clean: clean, userStop: true); completion(); return
        }
        self.recordSessionEnd(stopping.run, reason: stopping.stopOutcome?.sessionEndReason ?? "unconfirmed_stop",
          exitCode: stopping.process.isRunning ? nil : stopping.process.terminationStatus, began: self.sessionBegan)
        self.connectionAttempt?.finish(clean ? "COMPLETED" : "FAILED", clean ? "USER_STOP" : "CLEANUP_FAILED")
        self.connectionAttempt = nil
        self.lastConsole = text
        self.busy = false
        self.status.stringValue = clean ? (stopping.mx != nil ? "MX+ session stopped · Bluetooth released" : "Stopped · Ready") :
          stopping.stopOutcome?.forcedProcessExit == true ? "Session stopped forcibly. See Console." :
          "Stopped · Adapter cleanup not confirmed. See Console."
        self.lcd.caption = clean ? nil : self.status.stringValue
        self.refresh()
        if (stopping.adapter?.kind != .nano && stopping.adapter?.kind != .mdi) || clean { completion() }
        else {
          self.status.stringValue = "Stopped · Nano cleanup not confirmed. Security continuation is blocked."
          onFailure()
        }
      }
    }
  }
  func windowShouldClose(_ sender: NSWindow) -> Bool {
    if sender !== window { return true }
    NSApp.terminate(nil)
    return false
  }
  func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    if let workflow = bluetoothWorkflow {
      guard !quitting else {return .terminateLater}; quitting = true
      let finish = workflow.onComplete
      workflow.onComplete = { report in finish?(report); NSApp.reply(toApplicationShouldTerminate:true) }
      workflow.stop(); return .terminateLater
    }
    if busy {
      _ = securityJob?.cancel()
      cancellation?.cancel()
      downloadTask?.cancel()
      status.stringValue = "Finishing the current operation. Please close again shortly."
      return .terminateCancel
    }
    if session == nil { return .terminateNow }
    guard !quitting else { return .terminateLater }
    quitting = true
    stopWithOutcome(onFailure: { self.quitting = false; NSApp.reply(toApplicationShouldTerminate: false) }, completion: {
      NSApp.reply(toApplicationShouldTerminate: true)
    })
    return .terminateLater
  }
  func showError(_ text: String) {
    recordAppError(text)
    status.stringValue = text
    textDialog("OpenSAAB T2", text)
  }
  func textDialog(_ title: String, _ text: String) {
    let a = NSAlert()
    a.messageText = title
    a.informativeText = text
    a.runModal()
  }
  var supportStrings: [String: String] {
    guard let url = Bundle.main.url(forResource: "interface-contract", withExtension: "json"),
      let data = try? Data(contentsOf: url),
      let spec = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let branding = spec["branding"] as? [String: Any],
      let support = branding["support"] as? [String: String] else { return [:] }
    return support
  }
  func projectSupport() {
    let s = supportStrings
    guard !busy, session == nil else { textDialog("Support OpenSAAB", s["wait"] ?? "Finish the current session before opening project support."); return }
    let alert = NSAlert()
    alert.messageText = s["title"] ?? "Help keep OpenSAAB moving"
    alert.informativeText = (s["message"] ?? "") + "\n\n" + (s["external"] ?? "") + "\nKo-fi: " + (s["url"] ?? "") + "\n\n" + (s["community_message"] ?? "")
    alert.addButton(withTitle: s["donate"] ?? "Continue to donation page")
    alert.addButton(withTitle: s["close"] ?? "Close")
    alert.addButton(withTitle: s["community_label"] ?? "Discord community")
    let response = alert.runModal()
    let destination = response == .alertFirstButtonReturn ? s["url"] : response == .alertThirdButtonReturn ? s["community_url"] : nil
    if let destination, let url = URL(string: destination),
      (response == .alertFirstButtonReturn && destination == "https://ko-fi.com/djfremen")
        || (response == .alertThirdButtonReturn && destination == "https://discord.gg/FxnFe8vQht") {
      if !NSWorkspace.shared.open(url) { textDialog("Support OpenSAAB", s["no_browser"] ?? "No browser available.") }
    }
  }
  @objc func about() {
    textDialog(
      "OpenSAAB T2 · macOS \(shellVersion)",
      "Native \(shellArchitectureLabel) · Android-reference interface\n\nOriginal firmware menus, keypad, offline mode and diagnostic-language selection. Application controls currently remain English.\n\nUSB adapters: Chipsoft Pro and the VCX Nano preview verify identity and a fresh VIN before opening the diagnostic menus. The native MDI preview uses the Android-reference shared USB transport and original guest menus; exact-build hardware checks are required. Bluetooth adapters: OBDLink MX+ (owner unit STN2255 v5.12.4) uses Connect and start: fresh VIN discovery, adapter release, then the original firmware read-only over the same MX+ on the Android preview.18 MX+ engine; Mac hardware validation is pending. Legacy Carista EVO / vLinker MC+ support is limited to VIN and Trionic 8 engine reads; native I-bus diagnostics and SW-CAN wake are unavailable. Exact-build live workflows require separate hardware validation. Session modes follow Android: Full native control, Read-only diagnostics, and Collect security seeds. Security processing/import and passive saved DTC reports use the shared Rust workers."
    )
  }
  func popup(_ entries: [(String, () -> Void)], anchor: NSView? = nil) {
    let menu = NSMenu()
    for (name, action) in entries {
      let item = NSMenuItem(title: name, action: #selector(menuItem(_:)), keyEquivalent: "")
      item.target = self
      item.representedObject = MenuAction(action)
      menu.addItem(item)
    }
    // Keyboard/AX activation may leave the cursor anywhere. Anchor the menu to
    // its visible trigger; expanded mode falls back from hidden ⋮ to Actions.
    let trigger = [anchor,appMenuButton,actionsButton].compactMap{$0}.first {
      $0.window === window && !$0.isHiddenOrHasHiddenAncestor
    }
    guard let trigger = trigger else { return }
    let point = NSPoint(x:trigger.userInterfaceLayoutDirection == .rightToLeft ? trigger.bounds.maxX : trigger.bounds.minX,
      y:trigger.isFlipped ? trigger.bounds.maxY : trigger.bounds.minY)
    menu.popUp(positioning: nil, at: point, in: trigger)
  }
  @objc func menuItem(_ item: NSMenuItem) { (item.representedObject as? MenuAction)?.action() }
  func unavailable(_ action: String) {
    textDialog(action, "This macOS preview does not yet support this operation. No adapter command was sent. Original firmware menus remain available offline.")
  }
  func controlsHelp() {
    textDialog("Firmware controls", "Scroll or swipe ↑ / ↓ to move one item. Hold for ENTER. EXIT returns inside firmware; it does not stop emulation. Keypad contains all firmware keys.")
  }
  func appMenu() {
    var items: [(String, () -> Void)] = []
    if session != nil { items.append(("Stop emulation", { self.stop() })) }
    items += [
      ("Run without an adapter", { self.start() }),
      ("Firmware selection", { self.firmwareDialog() }),
      ("Preferences", { self.unavailable("Preferences") }),
      ("Import runtime folder", { self.importDialog() }),
      ("Adapter & advanced tools", { self.bluetoothTools() }),
      ("Report issue", { self.reportIssue() }),
      ("Console", { self.textDialog("Console", String((self.session?.consoleText ?? self.lastConsole).suffix(10000))) }),
      ("Check for updates", { self.unavailable("Check for updates") }),
      ("About / Support", { self.projectSupport() }),
      ("Firmware controls help", { self.controlsHelp() }),
    ]
    popup(items,anchor:appMenuButton)
  }
  func actions() {
    // Same order as Android ChipsoftUsbActivity.showActions().
    var items: [(String, () -> Void)] = [
      (securityJob != nil ? "Cancel security processing" : "Get security access", { self.securityAction() }),
      ("ECU information", { if self.session == nil {self.bluetoothRead("ecm_info")} else {self.menuShortcut("ecu_info", "ECU information")} }),
      ("Original menus", { self.originalMenus() }),
      ("Read engine codes — HS-CAN", { self.engineStartupAction() }),
      ("Read DTC", { self.menuShortcut("read_dtc", "Read DTC") }),
      ("Clear DTC", { self.menuShortcut("clear_dtc", "Clear DTC") }),
      ("Engine Data", { self.menuShortcut("engine_data", "Engine Data") }),
      ("Saved DTC reports", { self.savedDtcReports() }),
    ]
    items.append(("Vehicle and security details", { self.securityDetails() }))
    items.append((expanded ? "Show header" : "Expand screen", {
      self.toggleExpanded()
    }))
    items.append(("App menu", { self.appMenu() }))
    popup(items,anchor:actionsButton)
  }
  func keypad() {
    keypadVisible.toggle()
    arrange()
  }
  func importDialog() {
    guard !busy, session == nil else {
      status.stringValue = "Stop emulation before importing firmware."
      return
    }
    let p = NSOpenPanel()
    p.canChooseDirectories = true
    p.canChooseFiles = false
    p.allowsMultipleSelection = false
    p.message =
      "Select a runtime folder (eprom.bin, opsys.dwn, candi.bin) to replace the bundled support files. Files are copied; your diagnostic language images are kept."
    p.beginSheetModal(for: window) { result in
      guard result == .OK, let source = p.url else { return }
      do {
        try self.store.importRuntime(source)
        self.store = try FirmwareStore(state: self.state, catalog: self.catalog)
        self.status.stringValue = "Runtime imported. Choose a firmware language."
        self.refresh()
      } catch { self.showError(error.localizedDescription) }
    }
  }
  func firmwareDialog() {
    guard !busy else {
      status.stringValue = "Finish the current operation before changing firmware."
      return
    }
    guard firmwareWindow == nil else { return }
    if session != nil {
      let prompt = NSAlert()
      prompt.messageText = "Change diagnostic software?"
      prompt.informativeText =
        "This stops emulation and releases the adapter before opening Firmware selection. Your installed software and session logs are preserved. Choose Connect and start again after selecting your software."
      prompt.addButton(withTitle: "Stop and choose software")
      prompt.addButton(withTitle: "Keep running")
      prompt.beginSheetModal(for: window) { [weak self] response in
        guard response == .alertFirstButtonReturn, let self = self else { return }
        self.stop {
          self.lcd.image = nil
          self.vehicle.stringValue = "Session stopped · Connect again for a fresh VIN"
          self.firmwareDialog()
        }
      }
      return
    }
    let w = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 590, height: 440), styleMask: [.titled],
      backing: .buffered, defer: false)
    w.title = "Firmware selection"
    w.backgroundColor = background
    firmwareWindow = w
    let languages = NSPopUpButton()
    languages.addItems(withTitles: languageNames)
    languages.selectItem(at: languageCodes.firstIndex(of: store.selected?.language ?? "en") ?? 0)
    languages.target = self
    languages.action = #selector(languageChanged)
    languageChoices = languages
    let c = NSPopUpButton()
    choices = c
    let note = label(
      "Choose the language and version of the original Saab diagnostic software. Application controls remain English. Your installed software and logs are preserved."
    )
    let detail = label((try? store.validateCore()) != nil
      ? "App components are ready. Download the selected software to continue."
      : "This import-only build needs your communication files before emulation can start.")
    downloadStatus = detail
    languageChanged()
    let download = ActionButton("Download and continue") { [weak self] in self?.download() }
    downloadButton = download
    let done = ActionButton("Set up later / Cancel download") { [weak self] in
      guard let self = self else { return }
      if self.busy {
        self.cancellation?.cancel()
        self.downloadTask?.cancel()
        detail.stringValue = "Cancelling…"
      } else {
        self.window.endSheet(w)
        w.orderOut(nil)
        self.firmwareWindow = nil
      }
    }
    let s = stack([note, label("Diagnostic language"), languages, label("Software version"), c, detail, download, done], vertical: true)
    s.spacing = 15
    for v in s.arrangedSubviews { v.widthAnchor.constraint(equalTo: s.widthAnchor).isActive = true }
    w.contentView!.addSubview(s)
    pin(s, inside: w.contentView!, inset: 20)
    window.beginSheet(w)
  }
  @objc func languageChanged() {
    guard let languages = languageChoices, let c = choices else { return }
    firmwareEntries = store.allEntries.filter { $0.language == languageCodes[languages.indexOfSelectedItem] }
    c.removeAllItems()
    c.addItems(withTitles: firmwareEntries.map { $0.label })
    if let selected = store.selected, let i = firmwareEntries.firstIndex(where: { $0.id == selected.id }) {
      c.selectItem(at: i)
    }
  }
  func setupCompleted() {
    guard (try? store.validateCore()) != nil else {
      downloadStatus?.stringValue = "Diagnostic software selected. Import your communication files from the app menu to finish setup."
      refresh()
      return
    }
    if let w = firmwareWindow {
      window.endSheet(w)
      w.orderOut(nil)
      firmwareWindow = nil
    }
    status.stringValue = "Ready — Connect and start, or run without an adapter."
    refresh()
  }
  func download() {
    guard !busy, let c = choices, firmwareEntries.indices.contains(c.indexOfSelectedItem) else { return }
    let entry = firmwareEntries[c.indexOfSelectedItem]
    guard entry.runnable else {
      downloadStatus?.stringValue = "Download-only image. Choose Russian 148.000 to run it."
      return
    }
    if (try? store.activate(entry)) != nil {
      setupCompleted()
      return
    }
    guard let url = URL(string: entry.url), url.scheme == "https" else {
      downloadStatus?.stringValue = "Imported card is missing or changed."
      return
    }
    busy = true
    refresh()
    c.isEnabled = false
    languageChoices?.isEnabled = false
    downloadButton?.isEnabled = false
    downloadStatus?.stringValue = "Downloading and verifying firmware…"
    let cancel = Cancellation()
    cancellation = cancel
    let config = URLSessionConfiguration.ephemeral
    config.timeoutIntervalForRequest = 30
    config.timeoutIntervalForResource = 60
    let network = URLSession(configuration: config)
    downloadSession = network
    downloadTask = network.downloadTask(with: url) { temp, response, error in
      let message: String
      var completed = false
      do {
        if let error = error { throw error }
        guard let temp = temp, (response as? HTTPURLResponse)?.statusCode == 200 else {
          throw ShellError.message("Firmware download failed")
        }
        try self.store.acceptArchive(temp, entry: entry, cancelled: { cancel.cancelled })
        message = "Verified and selected. Ready to start."
        completed = true
      } catch {
        message =
          cancel.cancelled
          ? "Download cancelled; previous selection preserved." : error.localizedDescription
      }
      network.finishTasksAndInvalidate()
      DispatchQueue.main.async {
        self.busy = false
        self.downloadTask = nil
        self.downloadSession = nil
        self.cancellation = nil
        self.downloadStatus?.stringValue = message
        self.choices?.isEnabled = true
        self.languageChoices?.isEnabled = true
        self.downloadButton?.isEnabled = true
        if completed { self.setupCompleted() } else { self.refresh() }
      }
    }
    downloadTask?.resume()
  }
  func detectUSBAdapter(readCodes: Bool = false) {
    guard !busy, session == nil else { return }
    guard store.selected != nil else { firmwareDialog(); return }
    let attempt = ConnectionAttempt(state: state)
    attempt.selectAdapter("SELECTION")
    let profiles: [USBCandidateProfile]
    do {
      guard let contract = Bundle.main.url(forResource: "interface-contract", withExtension: "json") else {
        throw ShellError.message("USB candidate profiles are unavailable. Reinstall this preview.")
      }
      profiles = try USBAdapterDiscovery.profiles(contract)
    } catch { attempt.finish("FAILED", "IO_ERROR"); showError(error.localizedDescription); return }
    let savedMode = UserDefaults.standard.string(forKey:"chipsoftSessionMode") ?? ""
    let chooser = USBAdapterChooser(readCodes:readCodes,
      mode:ChipsoftSessionMode(rawValue:savedMode) ?? .fullNative,
      scan:{ try USBAdapterDiscovery.snapshot(profiles:profiles) })
    guard let (candidate,mode) = chooser.run(),let kind = candidate.profile?.kind else {
      attempt.finish("CANCELLED", "USER_STOP"); return
    }
    if mode != .seeds { UserDefaults.standard.set(mode.rawValue, forKey: "chipsoftSessionMode") }
    let port = candidate.port
    if kind == .mdi {
      attempt.selectAdapter("MDI");self.connectionAttempt=attempt
      securityResumeAdapter=USBAdapterEndpoint(kind:.mdi,port:port)
      vehicle.stringValue="MDI · Reading fresh adapter identity and VIN…"
      self.start(adapter:securityResumeAdapter,mode:mode);return
    }
    if kind == .nano {
      connectNano(port: port, mode: mode, attempt: attempt)
      return
    }
    attempt.selectAdapter("CHIPSOFT")
    busy = true
    status.stringValue = "Identifying USB adapter…"
    attempt.advance("IDENTIFICATION")
    refresh()
    DispatchQueue.global().async {
      let result = Result {
        try runTool(
          self.binaries.appendingPathComponent("chipsoft-probe"),
          ["--port", port, "--timeout-ms", "2000"], timeout: 5)
      }
      var vinResult: Result<ToolResult, Error>? = nil
      if case .success(let r) = result, r.code == 0 {
        DispatchQueue.main.async {
          attempt.advance("VIN_REQUEST")
          self.status.stringValue = readCodes ? "Reading fresh VIN, then Trionic 8 engine codes…" : "Reading fresh VIN…"
        }
        // Same shared reader as Android, on the exclusively owned CDC handle.
        vinResult = Result {
          try runTool(
            self.binaries.appendingPathComponent("chipsoft-vin"),
            ["--port", port, "--timeout-ms", "8000"] + (readCodes ? ["--dtc-read"] : []), timeout: readCodes ? 90 : 35)
        }
      }
      DispatchQueue.main.async {
        self.busy = false
        switch result {
        case .success(let r):
          self.lastConsole = r.text
          if r.code != 0 {
            self.status.stringValue = "Adapter identification failed. See Console."
            self.vehicle.stringValue = "Device identity not verified"
            attempt.finish("FAILED", "PROTOCOL_OR_PROCESS_ERROR")
            self.recordAppError(self.status.stringValue, kind: "connection_error")
            break
          }
          attempt.advance("VIN_REQUEST")
          self.securityAdapterIdentity = self.adapterIdentity(r.text)
          self.status.stringValue = "Chipsoft identified. Reading VIN over HS-CAN…"
          self.vehicle.stringValue = "Chipsoft Pro · Identity verified · Vehicle not connected"
          switch vinResult {
          case .some(.success(let v)):
            self.lastConsole += "\n" + v.text
            if readCodes {
              let dir = self.state.appendingPathComponent("startup-runs/" + UUID().uuidString)
              try? fm.createDirectory(at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
              try? Data(v.text.utf8).write(to: dir.appendingPathComponent("console.log"), options: .atomic)
            }
            let lines = v.text.split(separator: "\n").map(String.init)
            let vin = lines.first { $0.hasPrefix("VIN=") }.map { String($0.dropFirst(4)) }
            let year = lines.first { $0.hasPrefix("MODEL_YEAR=") }.map { String($0.dropFirst(11)) }
            if v.code == 0, let vin = vin, vin.count == 17 {
              let observation=["vin":vin,"observed_utc":ISO8601DateFormatter().string(from:Date())]
              try? JSONSerialization.data(withJSONObject:observation).write(to:self.state.appendingPathComponent("vehicle-status.json"),options:.atomic)
              self.vehicle.stringValue =
                "Chipsoft Pro · VIN " + vin + (year.map { $0 == "unknown" ? "" : " · MY " + $0 } ?? "")
              if readCodes {
                do {
                  let report = try EngineStartupReport(v, vin: vin)
                  self.reviewStartup(report, port: port, mode: mode, attempt: attempt)
                } catch { attempt.finish("FAILED", "PROTOCOL_OR_PROCESS_ERROR"); self.showError(error.localizedDescription) }
                self.refresh(); return
              }
              self.status.stringValue =
                "VIN verified. Starting original diagnostic software…"
              attempt.advance("FIRMWARE_START")
              attempt.selectAdapter("CHIPSOFT"); self.connectionAttempt = attempt
              self.start(chipsoftPort: port, mode: mode)
            } else {
              self.status.stringValue =
                readCodes ? "Startup VIN/code read failed or incomplete. No confirmed code list. Check ignition and see Console." : "Chipsoft identified; no VIN reply from the ECU. Check ignition and P-bus. See Console."
              attempt.finish("FAILED", "VIN_UNAVAILABLE")
              self.recordAppError(self.status.stringValue, kind: "connection_error")
            }
          case .some(.failure(let e)):
            self.lastConsole += "\nVIN_FAILED " + e.localizedDescription
            self.status.stringValue = "Chipsoft identified; VIN read failed. See Console."
            attempt.finish("FAILED", "TIMEOUT")
            self.recordAppError(self.status.stringValue, kind: "connection_error")
          case .none: break
          }
        case .failure(let e):
          self.status.stringValue = e.localizedDescription
          attempt.finish("FAILED", "IO_ERROR")
          self.recordAppError(e.localizedDescription, kind: "connection_error")
        }
        self.refresh()
      }
    }
  }
}
@main struct Launcher {
  static func main() {
    let bundle = Bundle.main
    var state = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
      .appendingPathComponent("OpenSAAB")
    let binaries = bundle.bundleURL.appendingPathComponent("Contents/MacOS")
    let catalog = bundle.bundleURL.appendingPathComponent(
      "Contents/Resources/firmware-catalog.json")
    do {
      var args = Array(CommandLine.arguments.dropFirst())
      var automation: URL?
      if args.count == 4 && args[0] == "--private-automation-dir" && args[2] == "--private-state" {
        guard args[1].hasPrefix("/"), args[3].hasPrefix("/") else { throw ShellError.message("Private paths must be absolute.") }
        automation = URL(fileURLWithPath: args[1], isDirectory: true)
        state = URL(fileURLWithPath: args[3], isDirectory: true)
        args = []
      }
      if args.count == 5 && args[0] == "--bluetooth-live-test" {
        try MacBluetoothChecks.live(binary:binaries.appendingPathComponent("opensaab-connection"),mode:args[1],endpoint:args[2],root:URL(fileURLWithPath:args[3]).appendingPathComponent(args[4]))
        return
      }
      if args == ["--mdi-workflow-test"] { try MdiWorkflowChecks.run();return }
      if args == ["--self-test"] {
        try ShellChecks.unit(catalog: catalog)
        return
      }
      if args.count == 3 && args[0] == "--design-checks" {
        try DesignChecks.run(state: URL(fileURLWithPath: args[1], isDirectory: true),
          output: URL(fileURLWithPath: args[2], isDirectory: true), binaries: binaries, catalog: catalog)
        return
      }
      if args == ["--nano-binding-test"] { try ShellChecks.nanoBinding(); return }
      if args.count == 2 && args[0] == "--usb-chooser-test" {
        try USBChooserChecks.run(contract:URL(fileURLWithPath:args[1])); return
      }
      if args.count == 3 && args[0] == "--ui-parity-test" {
        try UIParityChecks.run(contract:URL(fileURLWithPath:args[1]),worker:URL(fileURLWithPath:args[2])); return
      }
      if args.count == 2 && args[0] == "--private-automation-modal-test" {
        try ShellChecks.privateAutomationModal(catalog:URL(fileURLWithPath:args[1])); return
      }
      if args.count == 2 && args[0] == "--runtime-test" {
        try ShellChecks.runtime(
          binaries: binaries, catalog: catalog, state: URL(fileURLWithPath: args[1]))
        return
      }
      if args == ["--check-runtime"] {
        let s = try FirmwareStore(state: state, catalog: catalog)
        try s.validateCore()
        print("Runtime valid; selection: \(s.selected?.label ?? "none")")
        return
      }
      if args.count == 2 && args[0] == "--import-runtime" {
        try FirmwareStore(state: state, catalog: catalog).importRuntime(
          URL(fileURLWithPath: args[1]))
        print("Runtime imported")
        return
      }
      guard args.isEmpty else { throw ShellError.message("Unsupported launcher argument") }
      let app = NSApplication.shared
      app.setActivationPolicy(.regular)
      let shell = try Shell(state: state, binaries: binaries, catalog: catalog)
      if let directory = automation { shell.privateAutomation = try PrivateAutomation(shell: shell, directory: directory) }
      app.delegate = shell
      withExtendedLifetime(shell) { app.run() }
    } catch {
      fputs("OpenSAAB: \(error.localizedDescription)\n", stderr)
      if CommandLine.arguments.count == 1 {
        let a = NSAlert()
        a.messageText = "OpenSAAB could not start"
        a.informativeText = error.localizedDescription
        a.runModal()
      }
      exit(1)
    }
  }
}
