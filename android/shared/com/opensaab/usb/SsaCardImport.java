// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.file.*;
import java.security.*;
import java.util.*;
import java.util.function.BooleanSupplier;

/** Bounded-memory working-card transaction. Never modifies source instruction bytes. */
public final class SsaCardImport {
    public static String hash(File file)throws Exception{
        MessageDigest h=MessageDigest.getInstance("SHA-256");byte[] b=new byte[16384];
        try(InputStream in=new FileInputStream(file)){int n;while((n=in.read(b))!=-1)h.update(b,0,n);}
        StringBuilder s=new StringBuilder();for(byte x:h.digest())s.append(String.format(Locale.ROOT,"%02x",x&255));return s.toString();
    }
    public static void verifyBaseline(File card,byte[] before)throws Exception{
        if(card.length()!=33554432 || before.length!=SsaData.SIZE)throw new IOException("Invalid working card/SSA size");
        byte[] region=new byte[SsaData.SIZE];try(RandomAccessFile f=new RandomAccessFile(card,"r")){f.seek(SsaData.OFFSET);f.readFully(region);}
        if(!Arrays.equals(region,before))throw new IOException("Working card changed; collect again.");
    }
    public static String apply(File card,byte[] before,byte[] after,String expectedHash,File evidence,BooleanSupplier allowed)throws Exception{
        if(after.length!=SsaData.SIZE)throw new IOException("Invalid processed SSA size");
        verifyBaseline(card,before);
        if(!allowed.getAsBoolean() || !hash(card).equals(expectedHash))throw new IOException("Session or working card changed; import cancelled.");
        File backup=new File(evidence,"card-before-security.bin");Files.copy(card.toPath(),backup.toPath());
        if(!hash(backup).equals(expectedHash))throw new IOException("Card backup verification failed");
        File temp=new File(card.getParentFile(),"card-security-"+UUID.randomUUID()+".tmp");
        try{
            Files.copy(backup.toPath(),temp.toPath());
            try(RandomAccessFile f=new RandomAccessFile(temp,"rw")){f.seek(SsaData.OFFSET);f.write(after);f.getFD().sync();}
            // Verify every byte, including all instruction bytes outside the SSA region.
            try(InputStream a=new BufferedInputStream(new FileInputStream(backup));InputStream b=new BufferedInputStream(new FileInputStream(temp))){
                for(int i=0;i<33554432;i++){
                    int old=a.read(),value=b.read();int expected=i>=SsaData.OFFSET&&i<SsaData.OFFSET+SsaData.SIZE?after[i-SsaData.OFFSET]&255:old;
                    if(old<0||value!=expected)throw new IOException("Patched card verification failed");
                }
                if(a.read()!=-1||b.read()!=-1)throw new IOException("Patched card size changed");
            }
            String result=hash(temp);
            if(!allowed.getAsBoolean()||!hash(card).equals(expectedHash))throw new IOException("Session or working card changed before import.");
            Files.move(temp.toPath(),card.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
            return result;
        }finally{temp.delete();}
    }
}
