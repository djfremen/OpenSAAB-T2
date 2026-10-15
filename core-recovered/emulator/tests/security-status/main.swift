import Foundation
let vectors=try JSONSerialization.jsonObject(with:Data(contentsOf:URL(fileURLWithPath:CommandLine.arguments[1]))) as! [[String:Any]]
for c in vectors {
 var bytes:Data?
 if let hex=c["hex"] as? String {let chars=Array(hex);bytes=Data(stride(from:0,to:chars.count,by:2).map{UInt8(String(chars[$0...$0+1]),radix:16)!})}
 let value=SecurityStatus.evaluate(bytes,receipt:c["receipt"] as! [String:String],now:SecurityStatus.time(c["now"] as! String)!,vehicle:c["vehicle"] as? String)
 guard value.state==c["state"] as! String,value.age==c["age"] as! String else{fatalError("Failed \(c["name"]!): \(value.summary)")}
}
print("PASS Swift: \(vectors.count) shared security-status vectors")
