// SPDX-License-Identifier: MPL-2.0
import AppKit
import Foundation
import IOKit
import IOKit.serial

struct USBCandidateProfile {
  let kind: USBAdapterKind
  let vendor, product: Int
  let label: String
}
struct USBAdapterCandidate {
  let port: String
  let profile: USBCandidateProfile?
  var label: String { (profile?.label ?? "Unrecognized USB device · identity unavailable") + "\nUSB " + port }
  func supports(readCodes: Bool) -> Bool { profile != nil && (!readCodes || profile?.kind == .chipsoft) }
}
struct USBDiscoverySnapshot {
  let candidates: [USBAdapterCandidate]
  let waitingForSerial: [USBCandidateProfile]
}
enum USBAdapterDiscovery {
  // Candidate IDs come from the common contract's pinned Android profiles.
  // A USB chip identity never establishes product or firmware identity.
  static func profiles(_ url: URL) throws -> [USBCandidateProfile] {
    let bytes = try Data(contentsOf:url)
    guard bytes.count <= 256 * 1024,
      let root = try JSONSerialization.jsonObject(with:bytes) as? [String:Any],
      let startup = root["adapter_startup"] as? [String:Any],
      let usb = startup["macos_usb"] as? [String:Any],
      let rows = usb["candidate_profiles"] as? [[String:Any]] else {
      throw ShellError.message("USB candidate profiles are unavailable. Reinstall this preview.")
    }
    let profiles = try rows.map { row -> USBCandidateProfile in
      guard let name = row["kind"] as? String, let kind = USBAdapterKind(rawValue:name),
        let vendor = row["vendor_id"] as? Int, let product = row["product_id"] as? Int,
        (0...65535).contains(vendor), (0...65535).contains(product),
        let label = row["label"] as? String, label == name + " candidate" else {
        throw ShellError.message("Invalid USB candidate profile.")
      }
      return USBCandidateProfile(kind:kind,vendor:vendor,product:product,label:label)
    }
    guard profiles.count == 3, Set(profiles.map{$0.kind}).count == 3,
      Set(profiles.map{"\($0.vendor):\($0.product)"}).count == 3 else {
      throw ShellError.message("Incomplete USB candidate profiles.")
    }
    return profiles
  }
  static func candidate(port:String,vendor:Int?,product:Int?,profiles:[USBCandidateProfile]) -> USBAdapterCandidate? {
    guard port.hasPrefix("/dev/cu.usbmodem"),URL(fileURLWithPath:port).deletingLastPathComponent().path == "/dev" else { return nil }
    let profile = profiles.first{$0.vendor == vendor && $0.product == product}
    return USBAdapterCandidate(port:port,profile:profile)
  }
  private static func property(_ entry:io_registry_entry_t,_ name:String) -> Any? {
    IORegistryEntryCreateCFProperty(entry,name as CFString,kCFAllocatorDefault,0)?.takeRetainedValue()
  }
  private static func ids(_ serial:io_registry_entry_t) -> (Int?,Int?) {
    var entry = serial
    IOObjectRetain(entry)
    defer { IOObjectRelease(entry) }
    for _ in 0..<16 {
      if let vendor = property(entry,"idVendor") as? NSNumber,
        let product = property(entry,"idProduct") as? NSNumber { return (vendor.intValue,product.intValue) }
      var parent:io_registry_entry_t = 0
      guard IORegistryEntryGetParentEntry(entry,kIOServicePlane,&parent) == KERN_SUCCESS else { break }
      IOObjectRelease(entry); entry = parent
    }
    return (nil,nil)
  }
  static func discover(profiles:[USBCandidateProfile]) throws -> [USBAdapterCandidate] {
    var iterator:io_iterator_t = 0
    guard let matching = IOServiceMatching(kIOSerialBSDServiceValue),
      IOServiceGetMatchingServices(kIOMainPortDefault,matching,&iterator) == KERN_SUCCESS else {
      throw ShellError.message("USB serial discovery is unavailable.")
    }
    defer { IOObjectRelease(iterator) }
    var candidates: [USBAdapterCandidate] = []
    while true {
      let entry = IOIteratorNext(iterator)
      guard entry != 0 else { break }
      defer { IOObjectRelease(entry) }
      guard let port = property(entry,kIOCalloutDeviceKey) as? String else { continue }
      let (vendor,product) = ids(entry)
      if let candidate = candidate(port:port,vendor:vendor,product:product,profiles:profiles) { candidates.append(candidate) }
    }
    return candidates.sorted{$0.port < $1.port}
  }
  static func snapshot(profiles:[USBCandidateProfile]) throws -> USBDiscoverySnapshot {
    let serial = try discover(profiles:profiles)
    var mdi:[USBAdapterCandidate]=[]
    let candidates=serial
    var iterator:io_iterator_t = 0
    guard let matching = IOServiceMatching("IOUSBHostDevice"),
      IOServiceGetMatchingServices(kIOMainPortDefault,matching,&iterator) == KERN_SUCCESS else {
      throw ShellError.message("USB device discovery is unavailable.")
    }
    defer { IOObjectRelease(iterator) }
    var waiting: [USBCandidateProfile] = []
    while true {
      let entry = IOIteratorNext(iterator)
      guard entry != 0 else { break }
      defer { IOObjectRelease(entry) }
      guard let vendor = property(entry,"idVendor") as? NSNumber,
        let product = property(entry,"idProduct") as? NSNumber else {continue}
      if vendor.intValue == 0x0525,product.intValue == 0xa4a2,
        let profile=profiles.first(where:{$0.kind == .mdi}),
        let location=property(entry,"locationID") as? NSNumber {
        var registry:UInt64=0
        guard IORegistryEntryGetRegistryEntryID(entry,&registry)==KERN_SUCCESS,registry>0 else {continue}
        mdi.append(USBAdapterCandidate(port:String(format:"mdi-usb:%08x:%llu",location.uint32Value,registry),profile:profile));continue
      }
      guard
        let profile = profiles.first(where:{$0.vendor == vendor.intValue && $0.product == product.intValue}),
        !candidates.contains(where:{$0.profile?.kind == profile.kind}),
        !waiting.contains(where:{$0.kind == profile.kind}) else { continue }
      waiting.append(profile)
    }
    return USBDiscoverySnapshot(candidates:(serial+mdi).sorted{$0.port<$1.port},waitingForSerial:waiting)
  }
}
