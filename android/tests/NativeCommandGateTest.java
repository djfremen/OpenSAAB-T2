// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.ByteArrayOutputStream;
public class NativeCommandGateTest {
    static byte[] frame(int channel,int id,int...data){
        ByteArrayOutputStream b=new ByteArrayOutputStream();int[] header={0x80,1,0,channel,0,0,0,0,0,data.length+4,0,0,id>>8,id&255};
        for(int v:header)b.write(v);for(int v:data)b.write(v);int sum=0;for(byte v:b.toByteArray())sum+=v&255;b.write(sum&255);
        ByteArrayOutputStream wire=new ByteArrayOutputStream();wire.write(0xbb);for(byte v:b.toByteArray()){int n=v&255;if(n==0xbb){wire.write(0xdd);wire.write(0x44);}else if(n==0xdd){wire.write(0xdd);wire.write(0x22);}else if(n==0xee){wire.write(0xdd);wire.write(0x11);}else wire.write(n);}wire.write(0xbb);return wire.toByteArray();
    }
    static void check(boolean b){if(!b)throw new AssertionError();}
    public static void main(String[] args){
        byte[] seed=frame(1,0x241,2,0x27,0x0b,0,0,0,0,0);
        check(NativeCommandGate.allowed(seed,true));check(!NativeCommandGate.allowed(seed,false));
        check(NativeCommandGate.allowed(frame(1,0x241,2,0xae,0,0,0,0,0,0),true));
        check(NativeCommandGate.allowed(frame(1,0x244,2,0x27,1,0,0,0,0,0),true));
        check(!NativeCommandGate.allowed(frame(1,0x241,4,0x27,2,0xbb,0xee,0,0,0),true));
        check(!NativeCommandGate.allowed(frame(1,0x241,1,0x14,0,0,0,0,0,0),true));
        check(!NativeCommandGate.allowed(frame(1,0x242,2,0xae,0,0,0,0,0,0),true));
        check(!NativeCommandGate.allowed(frame(0,0x241,2,0x27,0x0b,0,0,0,0,0),true));
        check(NativeCommandGate.allowed(frame(1,0x241,2,0x1a,0x3f,0,0,0,0,0),false));
        for(byte[] clear:new byte[][]{frame(1,0x241,1,4,0,0,0,0,0,0),frame(0,0x7e0,1,4),frame(1,0x101,0xfe,1,4,0,0,0,0,0)}){
            check(!NativeCommandGate.allowed(clear,false));check(!NativeCommandGate.allowed(clear,true));
            check(NativeCommandGate.allowed(clear,false,true));check(!NativeCommandGate.allowed(clear,true,true));
        }
        for(int[] rejected:new int[][]{{2,4,0},{1,0x14},{2,0x27,1},{4,0x27,2,1,2},{2,0xae,0},{1,0x34},{1,0x11}})
            check(!NativeCommandGate.allowed(frame(1,0x241,rejected),false,true));
        check(!NativeCommandGate.allowed(frame(1,0x777,1,4),false,true));
        check(NativeCommandGate.allowed(frame(1,0x257,3,0xa9,0x81,0x0a,0,0,0,0),false,true));
        check(!NativeCommandGate.allowed(frame(1,0x257,3,0xa9,0x81,0x0a),false));
        byte[] clear=frame(1,0x241,1,4);clear[5]^=1;check(!NativeCommandGate.allowed(clear,false,true));
        check(NativeCommandGate.allowed(frame(0,0x7e0,4,0x2c,0xf3,0,1,0,0,0),false));
        check(NativeCommandGate.allowed(frame(0,0x7e0,4,0x2c,0xf3,0x12,0x34),false));
        // Original 2004 T8 engine-entry request captured via Nano, 2026-09-09.
        check(NativeCommandGate.allowed(frame(0,0x7e0,4,0x2c,0xfe,0x03,0x8e,0,0,0),false));
        check(!NativeCommandGate.allowed(frame(1,0x241,4,0x2c,0xfe,0x03,0x8e,0,0,0),false));
        check(!NativeCommandGate.allowed(frame(1,0x241,4,0x2c,0xf3,0,1),false));
        check(!NativeCommandGate.allowed(frame(0,0x7e0,4,0x2c,0xff,0,1),false));
        check(!NativeCommandGate.allowed(frame(0,0x7e0,5,0x2c,0xf3,0,1,2),false));
        byte[] ff=frame(0,0x7e0,0x10,0x10,0x2c,0xf3,0,0x0f,0x11,0),cf1=frame(0,0x7e0,0x21,1,2,3,4,5,6,7),cf2=frame(0,0x7e0,0x22,8,9,10,0,0,0,0);
        NativeCommandGate.Stream stream=new NativeCommandGate.Stream();
        check(!stream.allowed(cf1,false,false,0));check(stream.allowed(ff,false,false,0));
        check(!stream.allowed(ff,false,false,1));check(!stream.allowed(cf2,false,false,1));
        check(!stream.allowed(cf1,true,false,1));check(stream.allowed(cf1,false,false,1));check(stream.allowed(cf2,false,false,2));
        check(!stream.allowed(cf2,false,false,3));
        check(stream.allowed(ff,false,false,4));check(!stream.allowed(cf1,false,false,3004));
        for(int[] bad:new int[][]{{0x10,0x10,0x34,0xf3,0,0,0,0},{0x10,0x10,0x27,0xf3,0,0,0,0},{0x10,0x10,0x2c,0xff,0,0,0,0},{0x10,65,0x2c,0xf3,0,0,0,0}})
            check(!new NativeCommandGate.Stream().allowed(frame(0,0x7e0,bad),false,false,0));
        NativeCommandGate.Stream wrong=new NativeCommandGate.Stream();check(wrong.allowed(ff,false,false,0));
        check(!wrong.allowed(frame(1,0x241,0x21,0,0,0,0,0,0,0),false,false,1));
        check(!wrong.allowed(frame(0,0x7e0,0x21,0),false,false,1));
        for(int dpid=0xf3;dpid<=0xfe;dpid++){
            check(new NativeCommandGate.Stream().allowed(frame(0,0x7e0,0x10,0x0c,0x2c,dpid,0x14,0x70,0x15,0x34),false,false,0));
            check(NativeCommandGate.allowed(frame(0,0x7e0,6,0x2c,dpid,0,1,0,2),false));
        }
        for(int rate=0;rate<=4;rate++)check(NativeCommandGate.allowed(frame(0,0x7e0,4,0xaa,rate,0xf3,0xf4),false));
        check(NativeCommandGate.allowed(frame(0,0x7e0,2,0xaa,0),false));
        check(!NativeCommandGate.allowed(frame(0,0x7e0,3,0xaa,5,0xf3),false));
        NativeCommandGate.Stream periodic=new NativeCommandGate.Stream();
        check(periodic.allowed(frame(0,0x7e0,0x10,13,0xaa,4,0x12,0x13,0x15,0x16),false,false,0));
        check(!periodic.allowed(frame(0,0x7e0,0x21,0xff,1,2,3,4,5,6),false,false,1));
        check(periodic.allowed(frame(0,0x7e0,0x21,0x17,0x18,0x19,0x1a,0x1b,0x1c,0x1d),false,false,1));
        check(!periodic.allowed(frame(0,0x7e0,0x22,1,2,3,4,5,6,7),false,false,2));
        for(int[] bad:new int[][]{{0x10,13,0xaa,5,1,2,3,4},{0x10,13,0xaa,4,0xff,2,3,4},{0x10,33,0xaa,4,1,2,3,4}})
            check(!new NativeCommandGate.Stream().allowed(frame(0,0x7e0,bad),false,false,0));
        for(int mask:new int[]{0x10,0x12,0x0c}){
            check(NativeCommandGate.allowed(frame(0,0x7e1,3,0xa9,0x81,mask),false));
            check(!NativeCommandGate.allowed(frame(1,0x7e1,3,0xa9,0x81,mask),false));
        }
        check(NativeCommandGate.allowed(frame(0,0x7e1,0x30,0,0),false));
        for(int[] denied:new int[][]{{1,4},{3,0xa9,0x81,0x0a},{2,0x27,1},{4,0x27,2,1,2},{4,0x3b,1,0x99,0}})
            check(!NativeCommandGate.allowed(frame(0,0x7e1,denied),false));
        seed[5]^=1;check(!NativeCommandGate.allowed(seed,true));
        System.out.println("NATIVE_COMMAND_GATE PASS; hardware not opened");
    }
}
