// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.ByteArrayOutputStream;
import java.util.Arrays;

/** Exact raw receive controls only. No queued/committed CAN writes permitted. */
public final class ChipsoftCommandGate {
    /** Original-firmware control: validate USB/CAN framing, not diagnostic services. */
    public static boolean fullNative(byte[] wire){
        if(receive(wire))return true;
        if(wire.length<32 || wire.length>40 || u16(wire,0)!=15 || u16(wire,2)!=wire.length-8 || u16(wire,4)!=0)return false;
        int sum=0;for(int i=8;i<wire.length;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        int protocol=u32(wire,12),flags=u32(wire,22),n=u16(wire,20)-4;
        if(u32(wire,8)!=50 || (protocol!=5 && protocol!=0x8008) || u32(wire,16)!=0 || u16(wire,26)!=0 || n!=wire.length-32 || wire[28]!=0 || wire[29]!=0)return false;
        int id=((wire[30]&255)<<8)|(wire[31]&255);
        if(flags==0x400)return protocol==0x8008 && id==0x100 && n==0;
        return flags==0 && id<=0x7ff;
    }
    static void word(ByteArrayOutputStream b,int v){for(int i=0;i<4;i++)b.write(v>>>(i*8));}
    static byte[] words(int... v){ByteArrayOutputStream b=new ByteArrayOutputStream();for(int x:v)word(b,x);return b.toByteArray();}
    static byte[] filter(int protocol){ByteArrayOutputStream b=new ByteArrayOutputStream();word(b,protocol);word(b,1);for(int n=0;n<3;n++){for(int i=0;i<12;i++)b.write(0);b.write(n==2?0:4);word(b,protocol);word(b,0);}return b.toByteArray();}
    static int u16(byte[] b,int at){return (b[at]&255)|((b[at+1]&255)<<8);}
    static int u32(byte[] b,int at){return u16(b,at)|(u16(b,at+2)<<16);}
    public static boolean vinProbe(byte[] wire){
        if(receive(wire))return true;
        if(!nativeRead(wire) || wire.length!=40 || u32(wire,12)!=5 || u32(wire,22)!=0 || wire[30]!=7 || (wire[31]&255)!=0xe0)return false;
        byte[] d=Arrays.copyOfRange(wire,32,40);
        return Arrays.equals(d,new byte[]{2,0x1a,(byte)0x90,0,0,0,0,0}) || Arrays.equals(d,new byte[]{0x30,0,0,0,0,0,0,0});
    }
    public static boolean audible(byte[] wire){
        if(nativeRead(wire))return true;
        if(wire.length!=40)return false;
        if(wire[33]==0x27)return symbolOnly(wire);
        if(wire[36]!=(byte)0xc0)return false;
        byte[] symbol=wire.clone();symbol[36]=0;
        int sum=0;for(int i=8;i<40;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        sum-=0xc0;symbol[6]=(byte)sum;symbol[7]=(byte)(sum>>>8);
        return symbolOnly(symbol);
    }
    public static boolean symbolOnly(byte[] wire){
        if(nativeRead(wire))return true;
        if(wire.length!=40 || u16(wire,0)!=15 || u16(wire,2)!=32 || u16(wire,4)!=0 || u32(wire,8)!=50 || u32(wire,12)!=0x8008 || u32(wire,16)!=0 || u16(wire,20)!=12 || u32(wire,22)!=0 || u16(wire,26)!=0 || wire[28]!=0 || wire[29]!=0 || wire[30]!=2 || wire[31]!=0x42)return false;
        int sum=0;for(int i=8;i<40;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        byte[] d=Arrays.copyOfRange(wire,32,40);
        if(Arrays.equals(d,new byte[]{2,0x27,1,0,0,0,0,0}) || Arrays.equals(d,new byte[]{4,0x3b,1,(byte)0x99,0,0,0,0}))return true;
        // Rust's per-session policy checks the exact key and fresh challenge.
        return d[0]==4 && d[1]==0x27 && d[2]==2 && d[5]==0 && d[6]==0 && d[7]==0;
    }
    public static boolean seeds(byte[] wire){
        if(nativeRead(wire))return true;
        if(wire.length!=40)return false;
        // Validate the entire wire envelope with the existing read gate, keeping
        // the same address/protocol/flags/padding and replacing only the service.
        byte[] read=wire.clone();read[32]=2;read[33]=0x1a;read[34]=(byte)0x90;
        int sum=0;for(int i=8;i<40;i++)sum+=read[i]&255;read[6]=(byte)sum;read[7]=(byte)(sum>>>8);
        if(!nativeRead(read))return false;
        sum=0;for(int i=8;i<40;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        for(int i=35;i<40;i++)if(wire[i]!=0)return false;
        int protocol=u32(wire,12),id=((wire[30]&255)<<8)|(wire[31]&255);
        if(wire[32]!=2)return false;
        if(wire[33]==(byte)0xae && wire[34]==0)return protocol==0x8008 && id==0x241;
        if(wire[33]!=0x27)return false;
        if(wire[34]==0x0b)return protocol==0x8008 && id==0x241;
        return wire[34]==1 && ((protocol==5 && (id==0x7e0 || id==0x7e1 || (id>=0x240 && id<=0x25f))) || (protocol==0x8008 && id>=0x240 && id<=0x25f));
    }
    /** Explicit original-firmware key-status test; no general DeviceControl. */
    public static boolean keyStatus(byte[] wire){
        if(nativeRead(wire))return true;
        if(wire.length!=40 || u32(wire,12)!=0x8008 || wire[30]!=2 || wire[31]!=0x41)return false;
        if(!Arrays.equals(Arrays.copyOfRange(wire,32,40),new byte[]{3,(byte)0xae,3,2,0,0,0,0}))return false;
        int sum=0;for(int i=8;i<40;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        // Reuse full envelope validation against an established read request.
        byte[] read=wire.clone();read[32]=2;read[33]=0x1a;read[34]=(byte)0x90;read[35]=0;
        sum=0;for(int i=8;i<40;i++)sum+=read[i]&255;read[6]=(byte)sum;read[7]=(byte)(sum>>>8);
        return nativeRead(read);
    }
    public static boolean nativeRead(byte[] wire){
        if(receive(wire))return true;
        if(wire.length<32 || wire.length>40 || u16(wire,0)!=15 || u16(wire,2)!=wire.length-8 || u16(wire,4)!=0)return false;
        int sum=0;for(int i=8;i<wire.length;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        int protocol=u32(wire,12),flags=u32(wire,22),n=u16(wire,20)-4;
        if(u32(wire,8)!=50 || (protocol!=5 && protocol!=0x8008) || u32(wire,16)!=0 || u16(wire,26)!=0 || n!=wire.length-32 || wire[28]!=0 || wire[29]!=0)return false;
        int id=((wire[30]&255)<<8)|(wire[31]&255);
        if(flags==0x400)return protocol==0x8008 && id==0x100 && n==0;
        if(flags!=0 || n==0 || !(id==0x101 || id==0x7e0 || (protocol==5 && id==0x7e1) || (id>=0x240 && id<=0x25f)))return false;
        int at=32;if((wire[at]&255)==0xfe)at++;if(at>=wire.length)return false;
        int pci=wire[at++]&255;if(pci==0x30)return wire.length-at>=2;
        if(pci<1 || pci>7 || at+pci>wire.length)return false;
        int service=wire[at]&255;
        // Captured read-only GMW3110 ReportProgrammingState broadcast.
        if(service==0xa2)return protocol==5 && id==0x101 && Arrays.equals(Arrays.copyOfRange(wire,32,wire.length),new byte[]{(byte)0xfe,1,(byte)0xa2,0,0,0,0,0});
        if(service==0x1a)return pci==2;
        if(service==0x3e || service==0x20)return pci==1;
        if(service==0xaa)return pci==3 && wire[at+1]==1;
        if(service==0xa9)return pci==3 && (wire[at+1]&255)==0x81 && (wire[at+2]==0x10 || wire[at+2]==0x12 || wire[at+2]==0x0c);
        return service==0x10 && pci==2 && (wire[at+1]==1 || wire[at+1]==2);
    }
    public static boolean receive(byte[] wire){
        if(wire.length<8 || wire.length!=8+u16(wire,2) || u16(wire,4)!=0)return false;
        int sum=0;for(int i=8;i<wire.length;i++)sum+=wire[i]&255;if((sum&65535)!=u16(wire,6))return false;
        int op=u16(wire,0);byte[] p=Arrays.copyOfRange(wire,8,wire.length);
        if((op==1 || op==8 || op==0x20) && p.length==0)return true;
        for(int protocol:new int[]{5,0x8008}){
            if(op==4 && Arrays.equals(p,words(protocol,0,protocol==5?500000:33333)))return true;
            if((op==5 || op==0x12) && Arrays.equals(p,words(protocol)))return true;
            if(op==0x10 && Arrays.equals(p,words(protocol,10)))return true;
            if(op==0x17 && Arrays.equals(p,filter(protocol)))return true;
        }
        return op==0xb && Arrays.equals(p,words(0x8008,0x8001,0x0100));
    }
}
