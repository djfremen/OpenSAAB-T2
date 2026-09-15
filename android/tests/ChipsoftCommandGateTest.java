// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public class ChipsoftCommandGateTest {
    static byte[] hex(String s){byte[] b=new byte[s.length()/2];for(int i=0;i<b.length;i++)b[i]=(byte)Integer.parseInt(s.substring(i*2,i*2+2),16);return b;}
    static void check(boolean b){if(!b)throw new AssertionError("Chipsoft receive gate");}
    static byte[] request(int id,int... data){
        byte[] b=hex("0F002000000000003200000008800000000000000C00000000000000000002420000000000000000");
        b[30]=(byte)(id>>>8);b[31]=(byte)id;for(int i=0;i<data.length;i++)b[32+i]=(byte)data[i];
        int sum=0;for(int i=8;i<b.length;i++)sum+=b[i]&255;b[6]=(byte)sum;b[7]=(byte)(sum>>>8);return b;
    }
    static byte[] hsRequest(int id,int... data){
        byte[] b=request(id,data);b[12]=5;b[13]=0;
        int sum=0;for(int i=8;i<b.length;i++)sum+=b[i]&255;b[6]=(byte)sum;b[7]=(byte)(sum>>>8);return b;
    }
    public static void main(String[] args){
        String[] controls={"0100000000000000","0800000000000000","04000c000000cd00050000000000000020a10700","120004000000050005000000","1000080000000f00050000000a000000","050004000000050005000000","2000000000000000"};
        int checks=0;
        for(String s:controls){byte[] b=hex(s);check(ChipsoftCommandGate.receive(b));checks++;for(int n=0;n<b.length;n++){check(!ChipsoftCommandGate.receive(java.util.Arrays.copyOf(b,n)));checks++;}byte[] bad=b.clone();bad[6]^=1;check(!ChipsoftCommandGate.receive(bad));checks++;}
        // Actual diagnostic queue/commit-shaped packets must never pass a receive test.
        for(int opcode:new int[]{0x22,0x0f,0x0e,0x1a}){byte[] b=hex("0100000000000000");b[0]=(byte)opcode;check(!ChipsoftCommandGate.receive(b));checks++;}
        byte[] symbol=request(0x242,4,0x3b,1,0x99,0),audible=request(0x242,4,0x3b,1,0x99,0xc0),key=request(0x242,4,0x27,2,0x12,0x34);
        check(ChipsoftCommandGate.symbolOnly(symbol));check(!ChipsoftCommandGate.nativeRead(symbol));check(!ChipsoftCommandGate.symbolOnly(audible));
        check(ChipsoftCommandGate.symbolOnly(key));check(!ChipsoftCommandGate.nativeRead(key));
        check(!ChipsoftCommandGate.symbolOnly(request(0x241,4,0x3b,1,0x99,0)));check(!ChipsoftCommandGate.symbolOnly(request(0x242,4,0x3b,1,0x98,0)));checks+=7;
        byte[] vin=request(0x7e0,2,0x1a,0x90);vin[12]=5;vin[13]=0;int sum=0;for(int i=8;i<40;i++)sum+=vin[i]&255;vin[6]=(byte)sum;vin[7]=(byte)(sum>>>8);
        check(ChipsoftCommandGate.vinProbe(vin));check(!ChipsoftCommandGate.vinProbe(symbol));check(!ChipsoftCommandGate.vinProbe(key));
        byte[] corrupt=vin.clone();corrupt[34]=(byte)0x91;check(!ChipsoftCommandGate.vinProbe(corrupt));checks+=4;
        for(int mask:new int[]{0x10,0x12,0x0c}){
            byte[] dtc=hsRequest(0x7e1,3,0xa9,0x81,mask);
            check(ChipsoftCommandGate.nativeRead(dtc));check(!ChipsoftCommandGate.vinProbe(dtc));
            check(!ChipsoftCommandGate.nativeRead(request(0x7e1,3,0xa9,0x81,mask)));checks+=3;
        }
        check(ChipsoftCommandGate.nativeRead(hsRequest(0x7e1,0x30,0,0)));
        check(!ChipsoftCommandGate.nativeRead(hsRequest(0x7e1,1,4)));
        check(!ChipsoftCommandGate.nativeRead(hsRequest(0x7e1,3,0xa9,0x81,0x0a)));
        check(!ChipsoftCommandGate.nativeRead(hsRequest(0x7e1,2,0x27,1)));
        check(!ChipsoftCommandGate.nativeRead(hsRequest(0x7e1,4,0x27,2,1,2)));
        check(!ChipsoftCommandGate.nativeRead(hsRequest(0x7e1,4,0x3b,1,0x99,0)));checks+=6;
        for(byte[] seed:new byte[][]{request(0x241,2,0x27,0x0b),request(0x242,2,0x27,1),hsRequest(0x7e0,2,0x27,1),hsRequest(0x7e1,2,0x27,1),hsRequest(0x245,2,0x27,1),hsRequest(0x251,2,0x27,1),hsRequest(0x247,2,0x27,1),request(0x241,2,0xae,0)}){
            check(ChipsoftCommandGate.seeds(seed));check(!ChipsoftCommandGate.nativeRead(seed));checks+=2;
        }
        for(byte[] denied:new byte[][]{key,symbol,audible,request(0x242,2,0x27,0x0b),hsRequest(0x241,2,0x27,0x0b),hsRequest(0x7e1,4,0x27,2,1,2),request(0x241,1,4),request(0x241,2,0xae,1),request(0x242,2,0xae,0),request(0x242,2,0x27,1,1)}){check(!ChipsoftCommandGate.seeds(denied));checks++;}
        check(ChipsoftCommandGate.audible(audible));check(!ChipsoftCommandGate.audible(symbol));
        check(ChipsoftCommandGate.audible(key));check(!ChipsoftCommandGate.seeds(audible));
        check(!ChipsoftCommandGate.audible(request(0x241,4,0x3b,1,0x99,0xc0)));
        check(!ChipsoftCommandGate.audible(request(0x242,4,0x3b,1,0x98,0xc0)));
        check(!ChipsoftCommandGate.audible(hsRequest(0x242,4,0x3b,1,0x99,0xc0)));checks+=7;
        byte[] status=request(0x241,3,0xae,3,2);
        check(ChipsoftCommandGate.keyStatus(status));check(!ChipsoftCommandGate.nativeRead(status));checks+=2;
        for(int i=0;i<8;i++){
            int[] changed={3,0xae,3,2,0,0,0,0};changed[i]^=1;
            check(!ChipsoftCommandGate.keyStatus(request(0x241,changed)));checks++;
        }
        for(byte[] denied:new byte[][]{hsRequest(0x241,3,0xae,3,2),request(0x242,3,0xae,3,2),key,symbol,audible,request(0x241,2,0x27,0x0b),request(0x241,1,4)}){
            check(!ChipsoftCommandGate.keyStatus(denied));checks++;
        }
        byte[] broken=status.clone();broken[6]^=1;check(!ChipsoftCommandGate.keyStatus(broken));checks++;
        byte[] sps=hsRequest(0x101,0xfe,1,0xa2);
        check(ChipsoftCommandGate.nativeRead(sps));checks++;
        for(byte[] denied:new byte[][]{request(0x101,0xfe,1,0xa2),hsRequest(0x242,0xfe,1,0xa2),hsRequest(0x101,1,0xa2),hsRequest(0x101,0xfe,2,0xa2,1),hsRequest(0x101,0xfe,1,0xa2,0,0,0,0,1)}){
            check(!ChipsoftCommandGate.nativeRead(denied));checks++;
        }
        for(int service:new int[]{0xa5,0x28,0x34,0x36,0x3b,0x27,4}){
            check(!ChipsoftCommandGate.nativeRead(hsRequest(0x101,0xfe,1,service)));checks++;
        }
        for(int n=0;n<sps.length;n++){check(!ChipsoftCommandGate.nativeRead(java.util.Arrays.copyOf(sps,n)));checks++;}
        byte[] badSps=sps.clone();badSps[6]^=1;check(!ChipsoftCommandGate.nativeRead(badSps));checks++;
        for(byte[] original:new byte[][]{sps,key,symbol,audible,request(0x241,2,0x10,3),request(0x241,2,0x27,0x0c),hsRequest(0x101,0xfe,1,0xa5),request(0x242,0x21,1,2,3,4,5,6,7)}){
            check(ChipsoftCommandGate.fullNative(original));checks++;
        }
        check(!ChipsoftCommandGate.fullNative(badSps));checks++;
        check(!ChipsoftCommandGate.fullNative(hsRequest(0x800,1,0xa2)));checks++;
        for(int n=0;n<sps.length;n++){check(!ChipsoftCommandGate.fullNative(java.util.Arrays.copyOf(sps,n)));checks++;}
        // Unknown USB opcodes and transport fields stay rejected even with valid checksum.
        for(int at:new int[]{0,4,8,12,16,20,22,26,28}){
            byte[] malformed=sps.clone();malformed[at]^=1;
            int wireSum=0;for(int i=8;i<malformed.length;i++)wireSum+=malformed[i]&255;malformed[6]=(byte)wireSum;malformed[7]=(byte)(wireSum>>>8);
            check(!ChipsoftCommandGate.fullNative(malformed));checks++;
        }
        System.out.println("CHIPSOFT_RECEIVE_GATE checks="+checks+" PASS; hardware_opened=false");
    }
}
