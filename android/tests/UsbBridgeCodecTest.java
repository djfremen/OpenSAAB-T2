// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.nio.charset.StandardCharsets;
public final class UsbBridgeCodecTest {
    static void reject(String s) throws Exception {
        try{UsbBridgeCodec.readLine(new ByteArrayInputStream(s.getBytes(StandardCharsets.US_ASCII)));throw new AssertionError();}
        catch(IOException expected){}
    }
    public static void main(String[] args) throws Exception {
        byte[] bytes={(byte)0xbb,0,(byte)0xff};
        if(!UsbBridgeCodec.hex(bytes,3).equals("BB00FF") || !java.util.Arrays.equals(bytes,UsbBridgeCodec.unhex("BB00FF")))throw new AssertionError();
        for(String s:new String[]{"A","GG","aa",new String(new char[258]).replace('\0','A')}){
            try{UsbBridgeCodec.unhex(s);throw new AssertionError();}catch(IOException expected){}
        }
        if(!UsbBridgeCodec.readLine(new ByteArrayInputStream("HELLO test\n".getBytes(StandardCharsets.US_ASCII))).equals("HELLO test"))throw new AssertionError();
        reject("unterminated");reject("READ\u0000\n");reject(new String(new char[262]).replace('\0','X')+"\n");
        System.out.println("USB_BRIDGE_CODEC PASS; exact bytes, bounded ASCII and malformed data rejection");
    }
}
