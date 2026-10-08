// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.File;
public final class EmulatorArchitectureTest {
    static byte[] elf(int bits,int machine){byte[] h=new byte[20];h[0]=0x7f;h[1]='E';h[2]='L';h[3]='F';h[4]=(byte)bits;h[5]=1;h[6]=1;h[18]=(byte)machine;h[19]=(byte)(machine>>8);return h;}
    static void check(String expected,String got){if(!expected.equals(got))throw new AssertionError(got);}
    public static void main(String[] args){
        check("Emulator: 32-bit ARM",EmulatorArchitecture.label(elf(1,40)));
        check("Emulator: 64-bit ARM",EmulatorArchitecture.label(elf(2,183)));
        String unknown="Emulator: architecture unavailable";
        check(unknown,EmulatorArchitecture.label(elf(2,40)));
        check(unknown,EmulatorArchitecture.label(elf(1,183)));
        check(unknown,EmulatorArchitecture.label(elf(2,62)));
        check(unknown,EmulatorArchitecture.label(new byte[4]));
        check(unknown,EmulatorArchitecture.label(new File("/does-not-exist/opensaab-emulator")));
        for(String path:args)System.out.println(path+": "+EmulatorArchitecture.label(new File(path)));
        System.out.println("PASS: executable architecture, mismatched headers, missing files; independent of host CPU");
    }
}
