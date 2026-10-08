// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;

/** Read the shipped emulator ELF, independently of CPU marketing and Java process bitness. */
public final class EmulatorArchitecture {
    public static String label(File executable) {
        byte[] header=new byte[20];
        try(FileInputStream in=new FileInputStream(executable)) {
            int offset=0,n;
            while(offset<header.length && (n=in.read(header,offset,header.length-offset))>0)offset+=n;
            if(offset!=header.length)return "Emulator: architecture unavailable";
            return label(header);
        }catch(IOException e){return "Emulator: architecture unavailable";}
    }
    static String label(byte[] h) {
        if(h.length<20 || h[0]!=0x7f || h[1]!='E' || h[2]!='L' || h[3]!='F' || h[5]!=1 || h[6]!=1)
            return "Emulator: architecture unavailable";
        int machine=(h[18]&255)|((h[19]&255)<<8);
        if(h[4]==1 && machine==40)return "Emulator: 32-bit ARM";
        if(h[4]==2 && machine==183)return "Emulator: 64-bit ARM";
        return "Emulator: architecture unavailable";
    }
    private EmulatorArchitecture(){}
}
