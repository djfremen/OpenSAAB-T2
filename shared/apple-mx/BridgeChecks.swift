// SPDX-License-Identifier: MPL-2.0
import Darwin
import Foundation

private final class Fixture: AppleMXTransport {
    var callback: (([String:Any]) -> Void)?
    func open(_ packet: @escaping ([String:Any]) -> Void) { callback=packet; packet(["type":"ready"]) }
    func send(_ request:[String:Any]) {
        if request["op"] as? String == "close" {
            callback?(["type":"channel_closed","ok":true]);callback?(["type":"closed","bluetooth_closed":true]);return
        }
        DispatchQueue.main.asyncAfter(deadline:.now()+0.1) { [self] in
            callback?(["type":"write_completed"])
            callback?(["type":"rx","bytes":Array("OK\r>".utf8)])
        }
    }
    func shutdown() { callback=nil }
}
private final class ResultBox: @unchecked Sendable {
    let lock=NSLock(); var done=false; var error:String?
    func finish(_ failure:String?) {lock.lock();error=failure;done=true;lock.unlock()}
    var finished:Bool { lock.lock();defer{lock.unlock()};return done }
}
@main struct BridgeChecks {
    static func main() throws {
        for command in ["", "ATZ\r0100", "ATZ\n", String(repeating:"A",count:129)] {
            precondition(!AppleMXFirmwareBridge.acceptsWrite(command))
        }
        let fixture=Fixture(), result=ResultBox()
        let bridge=try AppleMXFirmwareBridge(transport:fixture,deviceName:"fixture",deviceAddress:"fixture",nativeAPI:"fixture")
        bridge.open { outcome in if case .failure(let e)=outcome {result.finish(e.localizedDescription)} }
        DispatchQueue.global().async {
            let fd=socket(AF_INET,SOCK_STREAM,0);defer{Darwin.close(fd)}
            var address=sockaddr_in();address.sin_len=UInt8(MemoryLayout<sockaddr_in>.size);address.sin_family=sa_family_t(AF_INET)
            address.sin_port=AppleMXFirmwareBridge.port.bigEndian;address.sin_addr=in_addr(s_addr:inet_addr("127.0.0.1"))
            let connected=withUnsafePointer(to:&address) {$0.withMemoryRebound(to:sockaddr.self,capacity:1) {connect(fd,$0,socklen_t(MemoryLayout<sockaddr_in>.size))}}
            guard connected==0 else {result.finish("connect failed");return}
            func exchange(_ request:String)->String {
                let data=Array((request+"\n").utf8);_ = data.withUnsafeBytes {write(fd,$0.baseAddress,$0.count)}
                var bytes=[UInt8](),byte:UInt8=0
                while read(fd,&byte,1)==1 {if byte==10 {break};bytes.append(byte)}
                return String(decoding:bytes,as:UTF8.self)
            }
            guard exchange("HELLO "+bridge.token)==AppleMXFirmwareBridge.readyRead else {result.finish("authentication failed");return}
            let started=Date()
            guard exchange("WRITE ATZ")=="WRITTEN",Date().timeIntervalSince(started)>=0.08 else {result.finish("write acknowledged before native completion");return}
            let received=exchange("READ")
            guard let data=received.data(using:.utf8),let packet=try? JSONSerialization.jsonObject(with:data) as? [String:Any],
                  packet["bytes"] as? [Int] == Array("OK\r>".utf8).map(Int.init) else {result.finish("byte delivery changed");return}
            guard exchange("CLOSE")=="CLOSED",bridge.waitForRelease(seconds:1),bridge.bluetoothClosed else {result.finish("release unconfirmed");return}
            result.finish(nil)
        }
        let deadline=Date().addingTimeInterval(10)
        while !result.finished && Date()<deadline {RunLoop.main.run(until:Date().addingTimeInterval(0.01))}
        precondition(result.finished,"native bridge check timed out")
        if let error=result.error {fatalError(error)}
        print("PASS bounded writes, authenticated byte delivery, delayed native completion, native release")
    }
}
