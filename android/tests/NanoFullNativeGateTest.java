// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/** Actual VCX/ASCII USB-owner gate; synthetic frames only, no Android/device I/O. */
public final class NanoFullNativeGateTest {
    static int checks;
    static void check(boolean ok){checks++;if(!ok)throw new AssertionError("check "+checks);}
    static byte[] body(int channel,int id,int flags,int... data){
        byte[] b=NanoKeyStatusGateTest.body(channel,id,data);
        b[4]=(byte)(flags>>>24);b[5]=(byte)(flags>>>16);b[6]=(byte)(flags>>>8);b[7]=(byte)flags;
        return b;
    }
    static byte[] wire(int channel,int id,int flags,int... data){
        return NanoKeyStatusGateTest.wire(body(channel,id,flags,data));
    }
    static void rejectedBodyMutation(byte[] source,int index,int value){
        byte[] b=source.clone();b[index]=(byte)value;
        check(!NativeCommandGate.fullNative(NanoKeyStatusGateTest.wire(b)));
    }
    public static void main(String[] args)throws Exception{
        check(NativeCommandGate.fullNativeModeAllowed(true,false,false));
        check(!NativeCommandGate.fullNativeModeAllowed(false,false,false));
        check(!NativeCommandGate.fullNativeModeAllowed(true,true,false));
        check(!NativeCommandGate.fullNativeModeAllowed(true,false,true));
        check(!NativeCommandGate.fullNativeModeAllowed(true,true,true));
        // The explicit mode admits original security/Add frames; default Read and Seeds do not.
        int[][] commands={{4,0x27,2,0x12,0x34,0,0,0},{4,0x3b,1,0x99,0,0,0,0},
            {3,0xae,3,2,0,0,0,0},{1,4,0,0,0,0,0,0},
            {7,0x3b,0x10,0x20,0x30,0xbb,0xdd,0xee}};
        for(int[] d:commands){
            byte[] w=wire(1,0x242,0,d);
            check(NativeCommandGate.fullNative(w));
            check(!new NativeCommandGate.Stream().allowed(w,false,false,1));
            check(!new NativeCommandGate.Stream().allowed(w,true,false,1));
            check(!new NativeCommandGate.Stream().allowed(w,false,false,true,1));
        }
        // Standard CAN boundaries/DLC and byte preservation, not a diagnostic whitelist.
        for(int ch:new int[]{0,1})for(int id:new int[]{0,0x100,0x241,0x242,0x7e0,0x7ff}){
            for(int length:new int[]{0,1,7,8}){
                int[] d=new int[length];for(int i=0;i<length;i++)d[i]=new int[]{0xbb,0xdd,0xee,0,0xff,0x27,2,0x3b}[i];
                byte[] w=wire(ch,id,0,d);byte[] original=w.clone();
                check(NativeCommandGate.fullNative(w));check(Arrays.equals(w,original));
            }
        }
        // Exact SW wake and exclusions: unrelated flags, extended IDs, other routes/DLC.
        check(NativeCommandGate.fullNative(wire(1,0x100,0x1000)));
        check(!NativeCommandGate.fullNative(wire(0,0x100,0x1000)));
        check(!NativeCommandGate.fullNative(wire(1,0x101,0x1000)));
        check(!NativeCommandGate.fullNative(wire(1,0x100,0x1000,0)));
        for(int flag:new int[]{1,2,0x400,0x800,0x1001,0x2000,0x80000000,-1}){
            check(!NativeCommandGate.fullNative(wire(0,0x242,flag,4,0x27,2,1,2,0,0,0)));
            check(!NativeCommandGate.fullNative(wire(1,0x100,flag)));
        }
        for(int ch:new int[]{2,3,255})check(!NativeCommandGate.fullNative(wire(ch,0x242,0,1,0x20)));
        for(int id:new int[]{0x800,0x1000})check(!NativeCommandGate.fullNative(wire(1,id,0,1,0x20)));
        for(int n:new int[]{9,16,50})check(!NativeCommandGate.fullNative(wire(1,0x242,0,new int[n])));
        byte[] b=body(1,0x242,0,4,0x27,2,0x12,0x34,0,0,0);
        for(int i:new int[]{0,2,3,4,5,6,7,8,9,10,11,12}){
            int value=i==0?0:i==2?0x40:i==3?2:i==8?1:i==9?11:i==12?8:1;
            rejectedBodyMutation(b,i,value);
        }
        // Malformed framing, CRC, escaped delimiters and concatenation must not become CAN.
        byte[] valid=wire(1,0x242,0,4,0x27,2,0xbb,0xdd,0xee,0,0);
        byte[] corrupt=valid.clone();corrupt[corrupt.length-2]^=1;check(!NativeCommandGate.fullNative(corrupt));
        corrupt=valid.clone();corrupt[0]=0;check(!NativeCommandGate.fullNative(corrupt));
        corrupt=valid.clone();corrupt[corrupt.length-1]=0;check(!NativeCommandGate.fullNative(corrupt));
        for(byte[] w:new byte[][]{new byte[0],new byte[]{(byte)0xbb,(byte)0xdd,0x33,(byte)0xbb},
            new byte[]{(byte)0xbb,(byte)0xdd,(byte)0xbb},new byte[]{(byte)0xbb,(byte)0xbb}})
            check(!NativeCommandGate.fullNative(w));
        byte[] two=new byte[valid.length*2];System.arraycopy(valid,0,two,0,valid.length);System.arraycopy(valid,0,two,valid.length,valid.length);
        check(!NativeCommandGate.fullNative(two));check(!NativeCommandGate.fullNative(Arrays.copyOf(valid,valid.length-1)));
        // Production SocketUsb ASCII form through actual shared codec and explicit gate.
        StringBuilder hex=new StringBuilder();for(byte v:valid)hex.append(String.format(java.util.Locale.ROOT,"%02X",v&255));
        String line=UsbBridgeCodec.readLine(new ByteArrayInputStream(("TX "+hex+"\n").getBytes(StandardCharsets.US_ASCII)),NanoStartupGate.MAX_COMMAND_CHARS);
        byte[] decoded=UsbBridgeCodec.unhex(line.substring(3),NanoStartupGate.MAX_TX_HEX_CHARS);
        check(Arrays.equals(decoded,valid));check(NativeCommandGate.fullNative(decoded));
        // Full CAN policy cannot admit adapter startup/reset/channel commands itself.
        for(int op:new int[]{0x8c,0xa0,0x84,0xa1,0xa2,0x8d,0x40,0x41,0x48}){
            byte[] control=b.clone();control[2]=(byte)op;check(!NativeCommandGate.fullNative(NanoKeyStatusGateTest.wire(control)));
        }
        for(int dp:new int[]{1,6,8,11})check(new NativeCommandGate.Stream().allowed(wire(1,0x241,0,3,0xaa,1,dp,0,0,0,0),false,false,1));
        System.out.println("NANO_FULL_NATIVE_GATE PASS checks="+checks+"; hardware not opened");
    }
}
