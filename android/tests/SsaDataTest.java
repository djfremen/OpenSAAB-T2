// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.util.*;
import java.io.*;
import java.nio.file.*;

public final class SsaDataTest {
    static void reject(Runnable action){try{action.run();throw new AssertionError("Accepted invalid data");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args)throws Exception{
        byte[] input=new byte[714];Arrays.fill(input,(byte)255);input[0]=(byte)0xb1;
        System.arraycopy("YS3FF49Y541000000".getBytes("US-ASCII"),0,input,0x14,17);
        input[0x132]=0;input[0x133]=1;input[0x134]=3;input[0x135]=0x61;input[0x136]=0x12;input[0x137]=0x34;
        if(SsaData.validateInput(input)!=1)throw new AssertionError();
        byte[] cleared=new byte[714];Arrays.fill(cleared,(byte)255);
        reject(()->SsaData.validateInput(cleared));
        byte[] noSeeds=input.clone();Arrays.fill(noSeeds,0x132,714,(byte)255);
        reject(()->SsaData.validateInput(noSeeds));
        byte[] reply=input.clone();reply[1]=0;Arrays.fill(reply,0x26,0x2e,(byte)'A');reply[0x138]=0x56;reply[0x139]=0x78;
        SsaData.validateReply(input,reply);
        reject(()->SsaData.validateInput(Arrays.copyOf(input,713)));
        byte[] stale=input.clone();stale[0x26]='A';reject(()->SsaData.validateInput(stale));
        for(int offset:new int[]{0x14,0x40,0x132,0x134,0x136,0x140}){
            byte[] bad=reply.clone();bad[offset]^=1;reject(()->SsaData.validateReply(input,bad));
        }
        byte[] partial=reply.clone();partial[0x138]=(byte)255;partial[0x139]=(byte)255;reject(()->SsaData.validateReply(input,partial));
        if(!SsaData.needsAccess("Help\nYou need Security Access from TIS2000\n1. Disconnect Tech 2 from Vehicle."))throw new AssertionError("Prompt missed");
        if(SsaData.needsAccess("F6: Get Security Access")||SsaData.needsAccess("No security access required"))throw new AssertionError("False prompt");
        Path dir=Files.createTempDirectory("ssa-import-test");
        try{
            File card=dir.resolve("card.bin").toFile();
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(SsaData.OFFSET);f.write(input);}
            String before=SsaCardImport.hash(card);File evidence=Files.createDirectory(dir.resolve("evidence")).toFile();
            try{SsaCardImport.apply(card,input,reply,before,evidence,()->false);throw new AssertionError("Cancelled import applied");}catch(IOException expected){}
            if(!SsaCardImport.hash(card).equals(before))throw new AssertionError("Cancelled import changed card");
            String after=SsaCardImport.apply(card,input,reply,before,evidence,()->true);
            if(!SsaCardImport.hash(new File(evidence,"card-before-security.bin")).equals(before)||!SsaCardImport.hash(card).equals(after))throw new AssertionError("Backup/import mismatch");
            try(RandomAccessFile f=new RandomAccessFile(card,"r")){byte[] b=new byte[714];f.seek(SsaData.OFFSET);f.readFully(b);if(!Arrays.equals(reply,b))throw new AssertionError("SSA import mismatch");}
            try{SsaCardImport.verifyBaseline(card,input);throw new AssertionError("Stale baseline accepted");}catch(IOException expected){}
            File reset=Files.createDirectory(dir.resolve("reset")).toFile();
            try{SsaCardReset.clear(card,reset,()->false);throw new AssertionError("Active/cancelled reset applied");}catch(IOException expected){}
            if(!SsaCardImport.hash(card).equals(after))throw new AssertionError("Cancelled reset modified card");
            SsaCardReset.clear(card,reset,()->true);
            try(InputStream current=new BufferedInputStream(new FileInputStream(card));InputStream original=new BufferedInputStream(new FileInputStream(new File(reset,"card-before-security.bin")))){
                for(int i=0;i<33554432;i++){int value=current.read(),old=original.read();if(value!=(i>=SsaData.OFFSET&&i<SsaData.OFFSET+SsaData.SIZE?255:old))throw new AssertionError("Clear changed wrong byte: "+i);}
            }
            if(!SsaCardImport.hash(new File(reset,"card-before-security.bin")).equals(after))throw new AssertionError("Reset backup mismatch");
        }finally{try(java.util.stream.Stream<Path> paths=Files.walk(dir)){paths.sorted(Comparator.reverseOrder()).forEach(p->{try{Files.delete(p);}catch(IOException e){throw new RuntimeException(e);}});}}
        System.out.println("PASS: SSA contract, changed-VIN/seed/metadata/partial-key rejection, prompt detection, cancelled import, full-card backup and SSA-only transaction; NoMoreGlobal reset clears exactly 714 bytes with unchanged surrounding bytes and backup; no network or vehicle");
    }
}
