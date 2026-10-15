// SPDX-License-Identifier: MPL-2.0
import Foundation
import CryptoKit
struct SecurityStatus {
 static let size=714, offset: UInt64=0xfe0000
 static let explanation="Fresh means written less than 3 hours ago; Stale means 3 hours or older. This is an age reminder, not a confirmed vehicle-access expiry. Follow the firmware if it requests security access again."
 let state, age, timestamp, summary, color, details: String
 static func hash(_ b: Data)->String { SHA256.hash(data:b).map{String(format:"%02x",$0)}.joined() }
 static func analyze(_ data: Data?)->String {
  guard let data=data, data.count>=size else{return "UNAVAILABLE"};let b=[UInt8](data)
  func all(_ p:Int,_ n:Int,_ v:UInt8)->Bool {b[p..<p+n].allSatisfy{$0==v}}
  let seed=Int(b[0x30])*256+Int(b[0x31])
  if all(0x14,17,255)&&all(0x26,8,255)&&(seed==0||seed==65535){return "INIT_AUTH"}
  let chars=b[0x14..<0x25].filter{$0 != 0 && $0 != 255}
  if !chars.isEmpty && chars.allSatisfy({Array("ABCDEFGHJKLMNPRSTUVWXYZ0123456789".utf8).contains($0)}) {
   if all(0x26,8,255){return "PRE_AUTH"};if !all(0x26,8,0){return "POST_AUTH"}
  };return "INVALID"
 }
 static func time(_ s:String)->Date? {
  let f=ISO8601DateFormatter();f.formatOptions=[.withInternetDateTime,.withFractionalSeconds]
  return f.date(from:s) ?? ISO8601DateFormatter().date(from:s)
 }
 static func evaluate(_ data:Data?, receipt r:[String:String]=[:], now:Date=Date(), vehicle:String?=nil)->SecurityStatus {
  var state=analyze(data),age="Age unknown",timestamp="Timestamp unavailable"
  let vin=data != nil && data!.count>=size ? Data(data![0x14..<0x25]):Data()
  let v=String(decoding:vin,as:UTF8.self),valid=v.range(of:"^[A-HJ-NPR-Z0-9]{17}$",options:.regularExpression) != nil
  let mismatch=vehicle != nil && !vehicle!.isEmpty && state != "INIT_AUTH" && (!valid || vehicle! != v)
  if mismatch{state="UNAVAILABLE"}
  var label="[N/A]",message="No readable security data yet.",color="#bdcbd8"
  switch state {
   case "INIT_AUTH":label="[INIT_AUTH]";message="Security data cleared. Collect fresh data from the vehicle.";color="#9ed8ff"
   case "PRE_AUTH":label="[PRE-AUTH]";message="Vehicle data collected. Security response has not been written.";color="#ffd77c"
   case "POST_AUTH":label="[POST-AUTH]";message="Processed security data is present. Continue in the firmware.";color="#9fe0b5"
   case "INVALID":label="[INVALID]";message="Security data does not match a recognized state.";color="#ffb59a"
   default:break
  }
  if mismatch{message="Security data belongs to another or unverified vehicle."}
  let timesValid=time(r["started_utc"] ?? "") != nil && ["pre_auth_utc","processed_utc","imported_utc","failed_utc"].allSatisfy { r[$0] == nil || time(r[$0]!) != nil }
  let matches=r["schema"]=="1" && valid && r["vehicle_hash"]==hash(vin) && timesValid && ["processing","processed","imported","failed"].contains(r["stage"] ?? "")
  if matches,state=="POST_AUTH",r["stage"]=="imported",let data=data,r["ssa_hash"]==hash(data),time(r["started_utc"] ?? "") != nil,time(r["processed_utc"] ?? "") != nil,let imported=time(r["imported_utc"] ?? "") {
   let seconds=now.timeIntervalSince(imported)
   if seconds>=0 {age=seconds<10800 ? "Fresh":"Stale";timestamp="Post-auth written · "+r["imported_utc"]!;if age=="Stale"{color="#ffd77c"}}
  }else if matches,state=="PRE_AUTH",["processing","processed","failed"].contains(r["stage"] ?? ""),let t=r["pre_auth_utc"],time(t) != nil {timestamp="Pre-auth collected · "+t}
  let summary="Security: "+label+(state=="INIT_AUTH" ? " · cleared":state=="POST_AUTH" ? " · "+age:"")
  return SecurityStatus(state:state,age:age,timestamp:timestamp,summary:summary,color:color,details:summary+"\n"+message+"\n"+timestamp+"\n\n"+explanation+"\nThis is card data, not confirmation of current vehicle authorization.")
 }
 static func read(_ card:URL?,receipt:URL,vehicle:String?=nil)->SecurityStatus {
  var data:Data?,r:[String:String]=[:]
  if let card=card { data=SecurityWorkspace.read(card) }
  if let bytes=try? Data(contentsOf:receipt),bytes.count<=8192,let text=String(data:bytes,encoding:.utf8) {
   for line in text.components(separatedBy:.newlines) where !line.hasPrefix("#") && !line.hasPrefix("!") {
    if let p=line.firstIndex(of:"="){r[String(line[..<p])]=String(line[line.index(after:p)...]).replacingOccurrences(of:"\\:",with:":")}
   }
  };return evaluate(data,receipt:r,vehicle:vehicle)
 }
}

// Session lifecycle owns restoration; status reading never modifies files or transport.
enum SecurityWorkspace {
 static func read(_ card:URL)->Data? {
  let run=card.deletingLastPathComponent()
  let working=run.appendingPathComponent("working-ssa.bin")
  if FileManager.default.fileExists(atPath:working.path) {
   guard let data=try? Data(contentsOf:working),data.count==SecurityStatus.size else{return nil}
   return data
  }
  // Recover evidence produced by 0.2.11/12, before live checkpoints existed.
  if let metaData=try? Data(contentsOf:run.appendingPathComponent("native-security-snapshot.json")),
     metaData.count<8192,let meta=try? JSONSerialization.jsonObject(with:metaData) as? [String:Any],
     meta["origin"] as? String == "original-guest-memory",meta["bytes"] as? Int == SecurityStatus.size,
     meta["card_offset"] as? Int == Int(SecurityStatus.offset),meta["ssa_memory_flash_enabled"] as? Bool == true,
     let data=try? Data(contentsOf:run.appendingPathComponent("ssa-card-after.bin")),data.count==SecurityStatus.size {
   return data
  }
  guard let file=try? FileHandle(forReadingFrom:card) else{return nil}
  defer{try? file.close()}
  do {try file.seek(toOffset:SecurityStatus.offset);return try file.read(upToCount:SecurityStatus.size)} catch{return nil}
 }
 static func previousRun(state:URL,firmware:String)->URL? {
  guard let data=try? Data(contentsOf:state.appendingPathComponent("latest-session.json")),data.count<8192,
   let meta=try? JSONSerialization.jsonObject(with:data) as? [String:Any],meta["firmware"] as? String == firmware,
   let path=meta["run"] as? String else{return nil}
  let run=URL(fileURLWithPath:path).standardizedFileURL.resolvingSymlinksInPath()
  let parent=state.appendingPathComponent("runs").standardizedFileURL.resolvingSymlinksInPath()
  return run.deletingLastPathComponent()==parent ? run:nil
 }
 @discardableResult static func restore(state:URL,firmware:String,to card:URL) throws -> Bool {
  guard let previous=previousRun(state:state,firmware:firmware),
    let data=read(previous.appendingPathComponent("card.bin")),data.count==SecurityStatus.size,
    let observation=try? Data(contentsOf:state.appendingPathComponent("vehicle-status.json")),observation.count<8192,
    let vehicle=try? JSONSerialization.jsonObject(with:observation) as? [String:String],let vin=vehicle["vin"],
    vin.range(of:"^[A-HJ-NPR-Z0-9]{17}$",options:.regularExpression) != nil,
    String(decoding:data[0x14..<0x25],as:UTF8.self)==vin,
    ["PRE_AUTH","POST_AUTH"].contains(SecurityStatus.analyze(data)) else{return false}
  // Only the brand-new disposable session card; never modify the download or old evidence.
  let handle=try FileHandle(forUpdating:card);defer{try? handle.close()}
  try handle.seek(toOffset:SecurityStatus.offset);try handle.write(contentsOf:data);try handle.synchronize()
  let origin=previous.appendingPathComponent("security-collection-origin.json")
  let destination=card.deletingLastPathComponent().appendingPathComponent("security-collection-origin.json")
  if FileManager.default.fileExists(atPath:origin.path) {
   try FileManager.default.copyItem(at:origin,to:destination)
  } else {
   try JSONSerialization.data(withJSONObject:["source_session":previous.path,"firmware":firmware])
     .write(to:destination,options:.atomic)
  }
  let receipt=previous.appendingPathComponent("security-processing-status.properties")
  if FileManager.default.fileExists(atPath:receipt.path) {
   try FileManager.default.copyItem(at:receipt,to:card.deletingLastPathComponent().appendingPathComponent(receipt.lastPathComponent))
  }
  return true
 }
}
