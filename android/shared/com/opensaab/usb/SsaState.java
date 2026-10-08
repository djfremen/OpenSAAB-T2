// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;

/** Display-only NoMoreGlobal classification. Never authorizes or blocks a vehicle operation. */
public enum SsaState {
    INIT_AUTH("[INIT_AUTH]", "Security data cleared. Collect fresh data from the vehicle.", 0xff9ed8ff),
    PRE_AUTH("[PRE-AUTH]", "Vehicle data collected. Security response has not been written.", 0xffffd77c),
    POST_AUTH("[POST-AUTH]", "Processed security data is present. Continue in the firmware.", 0xff9fe0b5),
    INVALID("[INVALID]", "Security data does not match a recognized state.", 0xffffb59a),
    UNAVAILABLE("[N/A]", "No readable security data yet.", 0xffbdcbd8);
    public final String label, explanation; public final int color;
    SsaState(String label,String explanation,int color){this.label=label;this.explanation=explanation;this.color=color;}
    public static SsaState analyze(byte[] b){
        if(b==null||b.length<SsaData.SIZE)return UNAVAILABLE;
        boolean vinFF=all(b,0x14,17,255),keyFF=all(b,0x26,8,255);
        int seed=((b[0x30]&255)<<8)|(b[0x31]&255);
        if(vinFF&&keyFF&&(seed==65535||seed==0))return INIT_AUTH;
        // Match NoMoreGlobal's structure check, including ignored NUL/FF padding.
        boolean valid=true;int chars=0;
        for(int p=0x14;p<0x25;p++){
            int c=b[p]&255;if(c==0||c==255)continue;chars++;
            if(!((c>='A'&&c<='Z')||(c>='0'&&c<='9'))||c=='I'||c=='O'||c=='Q')valid=false;
        }
        if(valid&&chars>0){if(keyFF)return PRE_AUTH;if(!all(b,0x26,8,0))return POST_AUTH;}
        return INVALID;
    }
    public static SsaState read(File card){
        try(RandomAccessFile f=new RandomAccessFile(card,"r")){
            byte[] b=new byte[SsaData.SIZE];f.seek(SsaData.OFFSET);f.readFully(b);return analyze(b);
        }catch(IOException e){return UNAVAILABLE;}
    }
    private static boolean all(byte[] b,int start,int count,int value){
        for(int p=start;p<start+count;p++)if((b[p]&255)!=value)return false;return true;
    }
}
