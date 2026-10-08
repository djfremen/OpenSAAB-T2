// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Passive observer of the existing GET_INFO exchange; never sends USB commands. */
public final class ChipsoftIdentity {
    private final byte[] frame=new byte[136];
    private int size;
    private boolean waiting;
    public void request(byte[] command){
        waiting=command.length==8 && command[0]==1;
        for(int i=1;i<command.length && waiting;i++)waiting=command[i]==0;
        size=0;
    }
    private int word(int offset){return (frame[offset]&255)|((frame[offset+1]&255)<<8);}
    public String receive(byte[] bytes){
        if(!waiting)return null;
        for(byte b:bytes){
            frame[size++]=b;
            if(size==8 && (word(0)!=1 || word(4)!=0 || word(2)>128)){waiting=false;return null;}
            if(size>=8 && size==8+word(2)){
                waiting=false;
                int sum=0;for(int i=8;i<size;i++)sum=(sum+(frame[i]&255))&65535;
                if(sum!=word(6))return null;
                String identity=new String(frame,8,size-8,java.nio.charset.StandardCharsets.US_ASCII);
                String prefix="CHIPSOFT J2534 Pro v. ";
                if(!identity.startsWith(prefix))return null;
                String version=identity.substring(prefix.length());
                return version.matches("[0-9]{1,3}(?:\\.[0-9]{1,3}){1,3}(?:[-+][A-Za-z0-9._-]{1,20})?")?version:null;
            }
        }
        return null;
    }
}
