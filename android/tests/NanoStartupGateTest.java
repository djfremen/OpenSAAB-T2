// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.ByteArrayOutputStream;
import java.util.Arrays;
public final class NanoStartupGateTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("check "+checks);}
    static byte[] frame(int seq,int op,int channel,byte[] payload){
        ByteArrayOutputStream b=new ByteArrayOutputStream();for(int x:new int[]{0x80,seq,op,channel})b.write(x);b.write(payload,0,payload.length);
        int sum=0;for(byte x:b.toByteArray())sum+=x&255;b.write(sum&255);
        ByteArrayOutputStream wire=new ByteArrayOutputStream();wire.write(0xbb);
        for(byte x:b.toByteArray()){int v=x&255;if(v==0xbb){wire.write(0xdd);wire.write(0x44);}else if(v==0xdd){wire.write(0xdd);wire.write(0x22);}else if(v==0xee){wire.write(0xdd);wire.write(0x11);}else wire.write(v);}
        wire.write(0xbb);return wire.toByteArray();
    }
    static byte[] tx(int op,int width){byte[] b=new byte[width];Arrays.fill(b,(byte)0xbb);return frame(0,op,0,b);}
    static byte[] info(){byte[] p=new byte[65];byte[] name={'V','C','X','-','N','A','N','O',0,0};System.arraycopy(name,0,p,29,name.length);p[53]=2;p[54]=4;p[55]=9;p[56]=1;return p;}
    static void rx(NanoStartupGate g,int op,byte[] p){byte[] b=frame(0,op,0,p);g.received(b,b.length);}
    static void identify(NanoStartupGate g){check(g.allowed(tx(0x8c,0)));byte[] wire=frame(0,0x8c,0,info());for(byte b:wire)g.received(new byte[]{b},1);check(!g.failed());}
    static void phase(NanoStartupGate g,int op,int width,int reply){check(g.allowed(tx(op,width)));check(!g.allowed(tx(op,width)));rx(g,op,new byte[reply]);check(!g.failed());}
    public static void main(String[] args){
        for(NanoStartupGate.Mode mode:NanoStartupGate.Mode.values()){
            NanoStartupGate g=new NanoStartupGate(mode);
            for(int op:new int[]{0xa0,0x84,0xa1,0xa2,0x8d,0x40,0x00,0x90})check(!g.allowed(tx(op,op==0xa0?8:0)));
            check(!g.channelReady());check(!g.allowed(frame(1,0x8c,0,new byte[0])));check(!g.allowed(frame(0,0x8c,1,new byte[0])));
        }
        NanoStartupGate plain=new NanoStartupGate(NanoStartupGate.Mode.IDENTITY_ONLY);
        byte[] echo={0,'T','E','S','T'};check(plain.allowed(frame(0,0x80,0,echo)));check(!plain.allowed(tx(0x8c,0)));rx(plain,0x80,echo);identify(plain);
        check(!plain.allowed(tx(0x8c,0)));check(!plain.allowed(tx(0x80,5)));check(!plain.allowed(tx(0xa0,8)));check(!plain.allowed(tx(0x8d,0)));check(!plain.channelReady());
        NanoStartupGate init=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);identify(init);
        for(int op:new int[]{0x84,0xa1,0xa2,0x8d,0x40})check(!init.allowed(tx(op,32)));
        check(!init.allowed(tx(0xa0,7)));check(!init.allowed(tx(0xa0,9)));
        phase(init,0xa0,8,9);phase(init,0x84,32,33);phase(init,0xa0,8,9);
        check(!init.allowed(tx(0xa1,159)));check(!init.allowed(tx(0xa1,161)));phase(init,0xa1,160,1);
        check(!init.channelReady());check(init.allowed(tx(0xa2,32)));check(!init.channelReady());rx(init,0xa2,new byte[33]);check(init.handshakeComplete());check(init.channelReady());
        check(!init.allowed(tx(0xa0,8)));check(!init.allowed(tx(0xa2,32)));check(!init.allowed(tx(0x8d,0)));
        NanoStartupGate reset=new NanoStartupGate(NanoStartupGate.Mode.REBOOT);identify(reset);check(!reset.allowed(tx(0xa0,8)));check(!reset.allowed(tx(0x8d,1)));check(reset.allowed(tx(0x8d,0)));check(!reset.rebootAcknowledged());rx(reset,0x8d,new byte[]{0});check(reset.rebootAcknowledged());check(!reset.allowed(tx(0x8d,0)));check(!reset.channelReady());
        NanoStartupGate control=new NanoStartupGate(NanoStartupGate.Mode.CHANNEL_CONTROL);identify(control);check(control.channelReady());check(!control.allowed(tx(0xa0,8)));check(!control.allowed(tx(0x8d,0)));
        for(int mutation=0;mutation<3;mutation++){
            NanoStartupGate bad=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);check(bad.allowed(tx(0x8c,0)));byte[] p=info();if(mutation==0)p[0]=1;if(mutation==1)p[29]='X';if(mutation==2)p[53]=3;rx(bad,0x8c,p);check(!bad.allowed(tx(0xa0,8)));check(!bad.channelReady());
        }
        for(int stage=0;stage<5;stage++){
            NanoStartupGate bad=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);identify(bad);int[] ops={0xa0,0x84,0xa0,0xa1,0xa2},widths={8,32,8,160,32},replies={9,33,9,1,33};
            for(int i=0;i<stage;i++)phase(bad,ops[i],widths[i],replies[i]);check(bad.allowed(tx(ops[stage],widths[stage])));byte[] denied=new byte[replies[stage]];denied[0]=(byte)0xfe;rx(bad,ops[stage],denied);check(bad.failed());check(!bad.channelReady());check(!bad.allowed(tx(ops[stage],widths[stage])));
        }
        NanoStartupGate shortReply=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);identify(shortReply);check(shortReply.allowed(tx(0xa0,8)));rx(shortReply,0xa0,new byte[8]);check(shortReply.failed());
        NanoStartupGate corrupt=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);byte[] infoTx=tx(0x8c,0);infoTx[1]^=1;check(!corrupt.allowed(infoTx));check(corrupt.allowed(tx(0x8c,0)));byte[] reply=frame(0,0x8c,0,info());reply[1]^=1;corrupt.received(reply,reply.length);check(corrupt.failed());
        NanoStartupGate wrongHeader=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);check(wrongHeader.allowed(tx(0x8c,0)));byte[] reply1=frame(1,0x8c,0,info());wrongHeader.received(reply1,reply1.length);check(!wrongHeader.allowed(tx(0xa0,8)));check(!wrongHeader.channelReady());
        NanoStartupGate oversized=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);check(!oversized.allowed(tx(0xa1,4096)));oversized.received(new byte[]{(byte)0xbb},1);byte[] b=new byte[4097];oversized.received(b,b.length);check(oversized.failed());
        NanoStartupGate escaped=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);escaped.received(new byte[]{(byte)0xbb,(byte)0xdd,0x33},3);check(escaped.failed());
        System.out.println("NANO_STARTUP_GATE PASS "+checks+" checks; synthetic only, no hardware");
    }
}
