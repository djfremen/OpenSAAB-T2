// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

/** Tests the actual USB envelope and per-session gate; no Android/device effects. */
public final class NanoKeyStatusGateTest {
    static int checks;
    static void check(boolean ok){checks++;if(!ok)throw new AssertionError("check "+checks);}
    static byte[] body(int channel,int id,int... data){
        ByteArrayOutputStream b=new ByteArrayOutputStream();
        for(int v:new int[]{0x80,7,0,channel,0,0,0,0,0,data.length+4,0,0,id>>8,id&255})b.write(v);
        for(int v:data)b.write(v);return b.toByteArray();
    }
    static byte[] wire(byte[] body){
        ByteArrayOutputStream out=new ByteArrayOutputStream();out.write(0xbb);int sum=0;
        for(byte v:body)sum+=v&255;
        ByteArrayOutputStream b=new ByteArrayOutputStream();b.write(body,0,body.length);b.write(sum&255);
        for(byte v:b.toByteArray()){int n=v&255;if(n==0xbb){out.write(0xdd);out.write(0x44);}else if(n==0xdd){out.write(0xdd);out.write(0x22);}else if(n==0xee){out.write(0xdd);out.write(0x11);}else out.write(n);}
        out.write(0xbb);return out.toByteArray();
    }
    static byte[] exact(){return wire(body(1,0x241,3,0xae,3,2,0,0,0,0));}
    static boolean enabled(NativeCommandGate.Stream s,byte[] w){return s.allowed(w,false,false,true,1);}
    public static void main(String[] args)throws Exception{
        byte[] request=exact();
        check(!NativeCommandGate.allowed(request,false));
        check(!new NativeCommandGate.Stream().allowed(request,false,false,0));
        check(!new NativeCommandGate.Stream().allowed(request,false,false,false,0));
        check(!new NativeCommandGate.Stream().allowed(request,true,false,true,0));
        check(!new NativeCommandGate.Stream().allowed(request,false,true,true,0));
        check(!new NativeCommandGate.Stream().allowed(wire(body(1,0x241,2,0x1a,0x90)),true,false,true,0));
        NativeCommandGate.Stream session=new NativeCommandGate.Stream();
        for(int i=0;i<3;i++){
            check(enabled(session,request));
            for(int dpid:new int[]{1,0x0b,8,6})check(enabled(session,wire(body(1,0x241,3,0xaa,1,dpid,0,0,0,0))));
        }
        check(!enabled(session,request));check(enabled(new NativeCommandGate.Stream(),request));
        // Rebuild checksums on corrupt semantic fields: checksum-only testing would miss these.
        for(int channel:new int[]{0,2,255})check(!enabled(new NativeCommandGate.Stream(),wire(body(channel,0x241,3,0xae,3,2,0,0,0,0))));
        for(int id:new int[]{0x101,0x240,0x242,0x7e0,0x800})check(!enabled(new NativeCommandGate.Stream(),wire(body(1,id,3,0xae,3,2,0,0,0,0))));
        for(int index:new int[]{0,2,4,5,6,7,8,9,10,11,14,15,16,17,18,19,20,21}){
            byte[] b=body(1,0x241,3,0xae,3,2,0,0,0,0);b[index]^=1;
            check(!enabled(new NativeCommandGate.Stream(),wire(b)));
        }
        for(int length:new int[]{0,3,7,9}){
            int[] data=new int[length];for(int i=0;i<Math.min(length,4);i++)data[i]=new int[]{3,0xae,3,2}[i];
            check(!enabled(new NativeCommandGate.Stream(),wire(body(1,0x241,data))));
        }
        for(int[] data:new int[][]{{3,0xae,1,1,0,0,0,0},{2,0xae,0,0,0,0,0,0},{1,4,0,0,0,0,0,0},{2,0x27,1,0,0,0,0,0},{4,0x27,2,1,2,0,0,0},{4,0x3b,1,0x99,0,0,0,0},{1,0x34,0,0,0,0,0,0}})
            check(!enabled(new NativeCommandGate.Stream(),wire(body(1,0x241,data))));
        byte[] bad=request.clone();bad[bad.length-2]^=1;check(!enabled(new NativeCommandGate.Stream(),bad));
        bad=request.clone();bad[0]=0;check(!enabled(new NativeCommandGate.Stream(),bad));
        bad=request.clone();bad[bad.length-1]=0;check(!enabled(new NativeCommandGate.Stream(),bad));
        check(!enabled(new NativeCommandGate.Stream(),new byte[]{(byte)0xbb,(byte)0xdd,0x33,(byte)0xbb}));
        // Invalid input never consumes the three valid requests.
        NativeCommandGate.Stream afterBad=new NativeCommandGate.Stream();check(!enabled(afterBad,bad));
        for(int i=0;i<3;i++)check(enabled(afterBad,request));check(!enabled(afterBad,request));
        StringBuilder hex=new StringBuilder();for(byte b:request)hex.append(String.format(java.util.Locale.ROOT,"%02X",b&255));
        String line=UsbBridgeCodec.readLine(new ByteArrayInputStream(("TX "+hex+"\n").getBytes(StandardCharsets.US_ASCII)),NanoStartupGate.MAX_COMMAND_CHARS);
        check(line.startsWith("TX "));check(enabled(new NativeCommandGate.Stream(),UsbBridgeCodec.unhex(line.substring(3),NanoStartupGate.MAX_TX_HEX_CHARS)));
        // Ordinary Read remains unchanged for every captured key-status context poll.
        for(int dpid:new int[]{1,0x0b,8,6})check(new NativeCommandGate.Stream().allowed(wire(body(1,0x241,3,0xaa,1,dpid,0,0,0,0)),false,false,0));
        System.out.println("NANO_KEY_STATUS_GATE PASS checks="+checks+"; hardware not opened");
    }
}
