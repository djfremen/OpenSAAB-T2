// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.file.*;
import java.security.*;
import java.util.*;
import java.util.function.BooleanSupplier;
import java.util.zip.*;

/** Streaming archive validation. Never uses an archive entry name as an output path. */
public final class FirmwareFiles {
    public static final long CARD_BYTES=33554432L, MAX_DOWNLOAD=40L*1024*1024;
    public static void check(BooleanSupplier cancel)throws IOException{if(cancel.getAsBoolean()||Thread.currentThread().isInterrupted())throw new InterruptedIOException("Cancelled");}
    public static long copy(InputStream in,File to,long max,BooleanSupplier cancel)throws IOException{
        long count=0;byte[] buffer=new byte[65536];
        try(FileOutputStream out=new FileOutputStream(to)){
            int n;while((n=in.read(buffer))!=-1){check(cancel);count+=n;if(count>max)throw new IOException("File exceeds expected size");out.write(buffer,0,n);}out.getFD().sync();
        }
        return count;
    }
    public static String sha(File file)throws IOException{
        try{MessageDigest digest=MessageDigest.getInstance("SHA-256");try(InputStream in=new FileInputStream(file)){byte[] b=new byte[65536];int n;while((n=in.read(b))!=-1)digest.update(b,0,n);}StringBuilder out=new StringBuilder();for(byte b:digest.digest())out.append(String.format(Locale.ROOT,"%02x",b&255));return out.toString();}
        catch(NoSuchAlgorithmException e){throw new AssertionError(e);}
    }
    public static void requireSha(File file,String expected)throws IOException{if(!sha(file).equals(expected))throw new IOException("Checksum mismatch; existing firmware was not replaced");}
    public static void card(File input,File output,BooleanSupplier cancel)throws IOException{
        try(InputStream probe=new FileInputStream(input)){
            int a=probe.read(),b=probe.read();
            if(a=='P'&&b=='K'){
                try(ZipFile zip=new ZipFile(input)){
                    ZipEntry chosen=null;int count=0;long expanded=0;
                    for(Enumeration<? extends ZipEntry> entries=zip.entries();entries.hasMoreElements();){
                        ZipEntry e=entries.nextElement();if(++count>32)throw new IOException("Too many archive entries");
                        String n=e.getName().replace('\\','/');
                        if(n.startsWith("/")||n.contains(":")||Arrays.asList(n.split("/")).contains(".."))throw new IOException("Unsafe archive path");
                        if(e.getSize()<0 || e.getSize()>CARD_BYTES || (expanded+=e.getSize())>CARD_BYTES+1048576)throw new IOException("Oversized archive");
                        if(!e.isDirectory()&&n.toLowerCase(Locale.ROOT).endsWith(".bin")){
                            if(chosen!=null)throw new IOException("Choose a ZIP containing exactly one card BIN");chosen=e;
                        }
                    }
                    if(chosen==null || chosen.getSize()!=CARD_BYTES)throw new IOException("This release requires one 32 MiB Tech2Win card image");
                    CRC32 crc=new CRC32();
                    try(InputStream in=new CheckedInputStream(zip.getInputStream(chosen),crc)){copy(in,output,CARD_BYTES,cancel);}
                    if(crc.getValue()!=chosen.getCrc())throw new IOException("Archive CRC mismatch");
                }
            }else try(InputStream in=new FileInputStream(input)){copy(in,output,CARD_BYTES,cancel);}
        }
        validate("card.bin",output);
    }
    private static long u32(byte[] b,int i){return ((long)(b[i]&255)<<24)|((long)(b[i+1]&255)<<16)|((long)(b[i+2]&255)<<8)|(b[i+3]&255);}
    public static void validate(String name,File file)throws IOException{
        long expected;
        switch(name){case "card.bin":expected=CARD_BYTES;break;case "eprom.bin":expected=262144;break;case "opsys.dwn":expected=229352;break;case "candi.bin":expected=65544;break;default:throw new IOException("Select eprom.bin, opsys.dwn or candi.bin");}
        if(!file.isFile()||file.length()!=expected)throw new IOException(name+" must be "+expected+" bytes after extraction");
        try(RandomAccessFile in=new RandomAccessFile(file,"r")){
            byte[] b=new byte[128];in.readFully(b);
            if(name.equals("card.bin") && !(b[0]=='T'&&b[1]=='2'&&b[2]==' '&&b[3]==' '))throw new IOException("Unrecognized Tech2 card header");
            if(name.equals("eprom.bin")){long sp=u32(b,0),pc=u32(b,4);if((sp&1)!=0||sp==0||sp==0xffffffffL||(pc&1)!=0||pc>=262144)throw new IOException("Invalid EPROM reset vectors");}
            if(name.equals("opsys.dwn")){in.seek(0x12a4a);if(in.readUnsignedShort()!=0x42a7)throw new IOException("Unsupported opsys signature");}
            if(name.equals("candi.bin")){
                for(int i=0;i<8;i++)if(b[i]!=0)throw new IOException("Unsupported CANdi header");
                if(u32(b,12)!=0x10040 || !new String(b,0x50,17,java.nio.charset.StandardCharsets.US_ASCII).equals("CANdi Application"))throw new IOException("Unsupported CANdi layout");
            }
        }
    }
    public static void atomicCopy(File source,File dest,BooleanSupplier cancel)throws IOException{
        Files.createDirectories(dest.getParentFile().toPath());File temp=File.createTempFile("image-",".tmp",dest.getParentFile());
        try{try(InputStream in=new FileInputStream(source)){copy(in,temp,MAX_DOWNLOAD,cancel);}check(cancel);if(!sha(temp).equals(sha(source)))throw new IOException("Copy verification failed");check(cancel);Files.move(temp.toPath(),dest.toPath(),StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);}finally{temp.delete();}
    }
}
