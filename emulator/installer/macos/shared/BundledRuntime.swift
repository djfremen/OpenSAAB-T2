// SPDX-License-Identifier: MPL-2.0
import Foundation

struct SupportFile: Decodable {
  let bytes: Int
  let sha256: String
}

// Mirrors Android's approved support-file profile. The diagnostic card remains
// a separate, user-selected download. Valid existing components are never replaced.
enum BundledRuntime {
  static let sizes = ["eprom.bin": 262144, "opsys.dwn": 229352, "candi.bin": 65544]
  static func validate(_ name: String, _ file: URL) throws {
    guard let size = sizes[name], (try? regularSize(file)) == size else {
      throw ShellError.message("Missing or invalid app component: \(name).")
    }
    let data = try Data(contentsOf: file)
    func word(_ offset: Int) -> UInt32 {
      data[offset..<offset+4].reduce(UInt32(0)) { ($0 << 8) | UInt32($1) }
    }
    let valid: Bool
    switch name {
    case "eprom.bin":
      let sp = word(0), pc = word(4)
      valid = sp != 0 && sp != UInt32.max && sp & 1 == 0 && pc & 1 == 0 && pc < 262144
    case "opsys.dwn": valid = data[0x12a4a] == 0x42 && data[0x12a4b] == 0xa7
    case "candi.bin":
      valid = data.prefix(8).allSatisfy { $0 == 0 } && word(12) == 0x10040
        && String(data: data[0x50..<0x61], encoding: .ascii) == "CANdi Application"
    default: valid = false
    }
    guard valid else { throw ShellError.message("Unsupported app component: \(name).") }
  }

  @discardableResult
  static func ensure(resources: URL, store: FirmwareStore) throws -> Int {
    let system = resources.appendingPathComponent("system")
    let bundled = coreFiles.filter { fm.fileExists(atPath: system.appendingPathComponent($0).path) }
    if bundled.isEmpty { return 0 } // Explicit import-only package.
    guard bundled.count == coreFiles.count else {
      throw ShellError.message("The app package is missing components. Reinstall OpenSAAB.")
    }
    let needed = coreFiles.filter { (try? validate($0, store.runtime.appendingPathComponent($0))) == nil }
    if needed.isEmpty { return 0 }
    let manifest = try JSONDecoder().decode([String: SupportFile].self,
      from: Data(contentsOf: system.appendingPathComponent("manifest.json")))
    guard Set(manifest.keys) == Set(coreFiles) else {
      throw ShellError.message("Invalid app component manifest.")
    }
    var staged: [String: Data] = [:]
    // Check every replacement before writing any. Interrupted installation leaves
    // individually valid components; the next launch completes missing/invalid ones.
    for name in needed {
      let file = system.appendingPathComponent(name)
      let spec = manifest[name]!
      try validate(name, file)
      guard spec.bytes == sizes[name], try sha256(file) == spec.sha256 else {
        throw ShellError.message("App component checksum failed: \(name). Reinstall OpenSAAB.")
      }
      staged[name] = try Data(contentsOf: file)
    }
    try fm.createDirectory(at: store.runtime, withIntermediateDirectories: true)
    for name in needed {
      try staged[name]!.write(to: store.runtime.appendingPathComponent(name), options: .atomic)
    }
    try store.validateCore()
    return needed.count
  }
}
