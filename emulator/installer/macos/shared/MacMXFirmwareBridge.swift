// SPDX-License-Identifier: MPL-2.0
import Foundation
import IOBluetooth

extension MacSPPTransport: AppleMXTransport {}
typealias MacMXFirmwareBridge = AppleMXFirmwareBridge
extension AppleMXFirmwareBridge {
  convenience init(device: IOBluetoothDevice) throws {
    let name = device.name ?? ""
    guard name == "OBDLink MX+" || name.hasPrefix("OBDLink MX+ "),
          let address = device.addressString else {
      throw AppleMXBridgeError.message("Select the OBDLink MX+ for this firmware session.")
    }
    try self.init(transport: MacSPPTransport(device:device, allowed:Self.acceptsWrite),
                  deviceName:name, deviceAddress:address, nativeAPI:"IOBluetoothRFCOMMChannel")
  }
}
