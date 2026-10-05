// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Session-local envelope gate. Rust owns DH, record validation and reply deadlines.
 * No key, entitlement metadata or challenge is generated or interpreted here. */
public final class NanoStartupGate {
    public static final int MAX_WIRE_BYTES=332;
    public static final int MAX_TX_HEX_CHARS=MAX_WIRE_BYTES*2;
    public static final int MAX_COMMAND_CHARS=3+MAX_TX_HEX_CHARS;
    public enum Mode { IDENTITY_ONLY, INITIALIZE, REBOOT, CHANNEL_CONTROL }
    private static final int[] OPS={0xa0,0x84,0xa0,0xa1,0xa2};
    private static final int[] WIDTHS={8,32,8,160,32};
    private static final int[] REPLY_WIDTHS={9,33,9,1,33};
    private final Mode mode;
    private int stage=0,pending=-1;
    private boolean echoSent,identitySent,identified,supported,failed,rebootAcknowledged;
    private final byte[] rxBody=new byte[4096];
    private int rxCount;
    private boolean rxStarted,rxEscaped;

    public NanoStartupGate(Mode mode){if(mode==null)throw new IllegalArgumentException("mode");this.mode=mode;}
    private static byte[] decode(byte[] wire){
        if(wire==null || wire.length<7 || wire.length>MAX_WIRE_BYTES || (wire[0]&255)!=0xbb || (wire[wire.length-1]&255)!=0xbb)return null;
        byte[] body=new byte[165];int n=0;
        for(int i=1;i<wire.length-1;i++){
            int v=wire[i]&255;if(v==0xbb)return null;
            if(v==0xdd){if(++i>=wire.length-1)return null;int e=wire[i]&255;v=e==0x44?0xbb:e==0x22?0xdd:e==0x11?0xee:-1;if(v<0)return null;}
            if(n==body.length)return null;body[n++]=(byte)v;
        }
        if(n<5 || (body[0]&255)!=0x80 || body[1]!=0 || body[3]!=0)return null;
        int sum=0;for(int i=0;i<n-1;i++)sum+=body[i]&255;if((sum&255)!=(body[n-1]&255))return null;
        return java.util.Arrays.copyOf(body,n-1);
    }
    /** Called before one whole USB write. A rejected write never advances the sequence. */
    public boolean allowed(byte[] wire){
        byte[] b=decode(wire);if(b==null || failed || pending!=-1)return false;
        int op=b[2]&255,width=b.length-4;
        if(op==0x80){
            if(echoSent || identitySent || width!=5 || b[4]!=0 || b[5]!='T' || b[6]!='E' || b[7]!='S' || b[8]!='T')return false;
            echoSent=true;pending=op;return true;
        }
        if(op==0x8c){if(identitySent || width!=0)return false;identitySent=true;pending=op;return true;}
        if(!identified || !supported)return false;
        if(mode==Mode.REBOOT){
            if(op!=0x8d || width!=0 || rebootAcknowledged)return false;pending=op;return true;
        }
        if(mode!=Mode.INITIALIZE || stage==OPS.length || op!=OPS[stage] || width!=WIDTHS[stage])return false;
        pending=op;return true;
    }
    /** Only structural/status checks are duplicated at the OS owner, never decryption.
     * Split/coalesced replies are preserved. A failed reply reaches Rust unchanged. */
    public void received(byte[] bytes,int length){
        if(bytes==null || length<0 || length>bytes.length)throw new IllegalArgumentException("RX bounds");
        if(failed || handshakeComplete() || rebootAcknowledged || (mode==Mode.IDENTITY_ONLY && identified))return;
        for(int i=0;i<length;i++){
            int v=bytes[i]&255;
            if(v==0xbb){
                if(rxEscaped){failed=true;return;}
                rxStarted=true;
                if(rxCount>0){observe();rxCount=0;if(failed)return;}
                continue;
            }
            if(!rxStarted){failed=true;return;}
            if(rxEscaped){rxEscaped=false;v=v==0x44?0xbb:v==0x22?0xdd:v==0x11?0xee:-1;if(v<0){failed=true;return;}}
            else if(v==0xdd){rxEscaped=true;continue;}
            if(rxCount==rxBody.length){failed=true;return;}rxBody[rxCount++]=(byte)v;
        }
    }
    private void observe(){
        if(rxCount<5){failed=true;return;}
        int sum=0;for(int i=0;i<rxCount-1;i++)sum+=rxBody[i]&255;
        if((sum&255)!=(rxBody[rxCount-1]&255)){failed=true;return;}
        if(pending<0 || (rxBody[0]&255)!=0x80 || rxBody[1]!=0 || (rxBody[2]&255)!=pending || rxBody[3]!=0)return;
        int width=rxCount-5;
        if(pending==0x80){
            if(width!=5 || rxBody[4]!=0 || rxBody[5]!='T' || rxBody[6]!='E' || rxBody[7]!='S' || rxBody[8]!='T'){failed=true;return;}
        }else if(pending==0x8c){
            byte[] name={'V','C','X','-','N','A','N','O',0,0};
            if(width!=65 || rxBody[4]!=0){failed=true;return;}
            for(int j=0;j<name.length;j++)if(rxBody[4+29+j]!=name[j]){failed=true;return;}
            identified=true;supported=rxBody[4+53]==2 && rxBody[4+54]==4 && rxBody[4+55]==9 && rxBody[4+56]==1;
        }else if(pending==0x8d){
            if(width!=1 || rxBody[4]!=0){failed=true;return;}rebootAcknowledged=true;
        }else{
            if(stage>=OPS.length || width!=REPLY_WIDTHS[stage] || rxBody[4]!=0){failed=true;return;}stage++;
        }
        pending=-1;
    }
    public boolean handshakeComplete(){return mode==Mode.INITIALIZE && stage==OPS.length && pending==-1 && !failed;}
    public boolean channelReady(){return handshakeComplete() || (mode==Mode.CHANNEL_CONTROL && identified && supported && pending==-1 && !failed);}
    public boolean rebootAcknowledged(){return mode==Mode.REBOOT && rebootAcknowledged && !failed;}
    public boolean failed(){return failed;}
}
