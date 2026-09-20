// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Defense at the USB owner: decode one VCX frame, admit captured firmware reads only. */
public final class NativeCommandGate {
    private static final class Decoded { byte[] b; int count,channel,id; Decoded(byte[] b,int count,int channel,int id){this.b=b;this.count=count;this.channel=channel;this.id=id;} }
    private static Decoded decode(byte[] wire) {
        if(wire.length<7 || (wire[0]&255)!=0xbb || (wire[wire.length-1]&255)!=0xbb)return null;
        byte[] b=new byte[64];int n=0;
        for(int i=1;i<wire.length-1;i++){
            int v=wire[i]&255;
            if(v==0xbb)return null;
            if(v==0xdd){if(++i>=wire.length-1)return null;int e=wire[i]&255;if(e==0x44)v=0xbb;else if(e==0x22)v=0xdd;else if(e==0x11)v=0xee;else return null;}
            if(n==b.length)return null;b[n++]=(byte)v;
        }
        if(n<15 || (b[0]&255)!=0x80 || b[2]!=0 || (b[3]!=0 && b[3]!=1))return null;
        int sum=0;for(int i=0;i<n-1;i++)sum+=b[i]&255;if((sum&255)!=(b[n-1]&255))return null;
        int size=((b[8]&255)<<8)|(b[9]&255);
        if(size<4 || size>12 || n!=11+size)return null;
        if(b[10]!=0 || b[11]!=0)return null;
        int id=((b[12]&255)<<8)|(b[13]&255);if(id>0x7ff)return null;
        int count=size-4, channel=b[3];
        return new Decoded(b,count,channel,id);
    }
    /** Per USB session, with a deadline and exact sequence for engine read packets. */
    public static final class Stream {
        int remaining=0,sequence=0;long deadline;boolean packetSeeds,packetClear,packetRead;
        public boolean allowed(byte[] wire,boolean seeds,boolean clearDtc,long now){
            if(seeds && clearDtc)return false;
            Decoded p=decode(wire);if(p==null)return false;
            byte[] b=p.b;int pci=b[14]&255;
            boolean engine=p.channel==0 && p.id==0x7e0 && b[4]==0 && b[5]==0 && b[6]==0 && b[7]==0;
            if(engine && p.count==8 && (pci&0xf0)==0x10){
                int length=((pci&15)<<8)|(b[15]&255);
                boolean read=(b[16]&255)==0xaa && (b[17]&255)<=4 && length>=8 && length<=32;
                for(int i=18;i<22 && read;i++){int d=b[i]&255;read=(d>=1 && d<=0x7f)||(d>=0x90 && d<=0xfe);}
                boolean definition=(b[16]&255)==0x2c && (b[17]&255)>=0xf3 && (b[17]&255)<=0xfe && length>=8 && length<=16 && length%2==0;
                if(remaining!=0 || !(read || definition))return false;
                remaining=length-6;sequence=1;deadline=now+3000;packetSeeds=seeds;packetClear=clearDtc;packetRead=read;return true;
            }
            if(p.count>0 && (pci&0xf0)==0x20){
                if(!engine || p.count!=8 || remaining==0 || now>=deadline || seeds!=packetSeeds || clearDtc!=packetClear || pci!=(0x20|sequence))return false;
                if(packetRead)for(int i=15;i<15+Math.min(7,remaining);i++){int d=b[i]&255;if(!((d>=1 && d<=0x7f)||(d>=0x90 && d<=0xfe)))return false;}
                remaining=Math.max(0,remaining-7);sequence=(sequence+1)&15;deadline=now+3000;return true;
            }
            if(engine && remaining!=0)return false;
            return NativeCommandGate.allowed(wire,seeds,clearDtc);
        }
    }

    public static boolean allowed(byte[] wire, boolean seeds) { return allowed(wire,seeds,false); }
    public static boolean allowed(byte[] wire, boolean seeds, boolean clearDtc) {
        if(seeds && clearDtc)return false;
        Decoded decoded=decode(wire);if(decoded==null)return false;
        byte[] b=decoded.b;int count=decoded.count,channel=decoded.channel,id=decoded.id;
        if(b[4]==0 && b[5]==0 && b[6]==0x10 && b[7]==0)return channel==1 && id==0x100 && count==0;
        if(b[4]!=0 || b[5]!=0 || b[6]!=0 || b[7]!=0 || count==0)return false;
        if(id!=0x101 && id!=0x7e0 && !(channel==0 && id==0x7e1) && !(id>=0x240 && id<=0x25f))return false;
        int offset=(b[14]&255)==0xfe?1:0;if(offset>=count)return false;
        int pci=b[14+offset]&255;if(pci==0x30)return count-offset>=3;
        if(pci<1 || pci>7 || offset+1+pci>count)return false;
        int p=15+offset,service=b[p]&255;
        if(service==0x04)return clearDtc && pci==1;
        if(service==0xae)return seeds && channel==1 && id==0x241 && offset==0 && pci==2 && b[p+1]==0;
        if(service==0x27)return seeds && offset==0 && pci==2 &&
            ((b[p+1]==1 && ((channel==0 && (id==0x7e0 || id==0x7e1 || (id>=0x240 && id<=0x25f))) || (channel==1 && id>=0x240 && id<=0x25f))) ||
             (b[p+1]==0x0b && channel==1 && id==0x241));
        if(service==0x1a)return pci==2;
        if(service==0x3e || service==0x20)return pci==1;
        if(service==0xaa){
            if(pci==3 && b[p+1]==1)return true;
            if(channel!=0 || id!=0x7e0 || pci<2 || b[p+1]<0 || b[p+1]>4 || (b[p+1]!=0 && pci==2))return false;
            for(int i=p+2;i<p+pci;i++){int d=b[i]&255;if(!((d>=1 && d<=0x7f)||(d>=0x90 && d<=0xfe)))return false;}
            return true;
        }
        if(service==0xa9)return pci==3 && (b[p+1]&255)==0x81 && (b[p+2]==0x10 || b[p+2]==0x12 || b[p+2]==0x0c || (clearDtc && b[p+2]==0x0a));
        if(service==0x10)return pci==2 && (b[p+1]==1 || b[p+1]==2);
        return service==0x2c && channel==0 && id==0x7e0 && pci>=4 && pci%2==0 && (b[p+1]&255)>=0xf3 && (b[p+1]&255)<=0xfe;
    }
}
