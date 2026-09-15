// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;

public final class FirmwareFilesTest {
    interface Attempt{void run()throws Exception;}
    static void reject(Attempt action)throws Exception{try{action.run();throw new AssertionError("Invalid input accepted");}catch(IOException expected){}}
    static void zip(File path,String name,long bytes,boolean two)throws Exception{
        try(ZipOutputStream z=new ZipOutputStream(new FileOutputStream(path))){
            z.putNextEntry(new ZipEntry(name));z.write(new byte[]{'T','2',' ',' '});byte[] b=new byte[65536];for(long i=4;i<bytes;){int n=(int)Math.min(b.length,bytes-i);z.write(b,0,n);i+=n;}z.closeEntry();
            if(two){z.putNextEntry(new ZipEntry("second.bin"));z.write(1);z.closeEntry();}
        }
    }
    public static void main(String[] args)throws Exception{
        File dir=Files.createTempDirectory("firmware-files-test-").toFile();
        try{
            File zip=new File(dir,"card.zip"),image=new File(dir,"image.bin"),dest=new File(dir,"active.bin");
            zip(zip,"folder/card.bin",FirmwareFiles.CARD_BYTES,false);FirmwareFiles.card(zip,image,()->false);FirmwareFiles.validate("card.bin",image);
            String expected=FirmwareFiles.sha(image);FirmwareFiles.requireSha(image,expected);reject(()->FirmwareFiles.requireSha(image,"bad"));
            Files.write(dest.toPath(),"old card".getBytes());reject(()->FirmwareFiles.atomicCopy(image,dest,()->true));if(!new String(Files.readAllBytes(dest.toPath())).equals("old card"))throw new AssertionError("Cancelled copy replaced destination");
            zip(zip,"../escape.bin",FirmwareFiles.CARD_BYTES,false);reject(()->FirmwareFiles.card(zip,image,()->false));
            zip(zip,"/escape.bin",FirmwareFiles.CARD_BYTES,false);reject(()->FirmwareFiles.card(zip,image,()->false));
            zip(zip,"card.bin",FirmwareFiles.CARD_BYTES,true);reject(()->FirmwareFiles.card(zip,image,()->false));
            zip(zip,"card.bin",FirmwareFiles.CARD_BYTES+1,false);reject(()->FirmwareFiles.card(zip,image,()->false));
            Files.write(zip.toPath(),"not a card".getBytes());reject(()->FirmwareFiles.card(zip,image,()->false));
            File candi=new File(dir,"test-candi.bin");try(RandomAccessFile out=new RandomAccessFile(candi,"rw")){out.setLength(65544);out.seek(12);out.writeInt(0x10040);out.seek(0x50);out.write("CANdi Application".getBytes("US-ASCII"));}
            FirmwareFiles.validate("candi.bin",candi);
            try(RandomAccessFile out=new RandomAccessFile(candi,"rw")){out.seek(0x50);out.write(0);}reject(()->FirmwareFiles.validate("candi.bin",candi));
            File eprom=new File(dir,"test-eprom.bin");try(RandomAccessFile out=new RandomAccessFile(eprom,"rw")){out.setLength(262144);out.writeInt(0x103954);out.writeInt(0x500);}FirmwareFiles.validate("eprom.bin",eprom);
            try(RandomAccessFile out=new RandomAccessFile(eprom,"rw")){out.seek(4);out.writeInt(0x501);}reject(()->FirmwareFiles.validate("eprom.bin",eprom));
            File opsys=new File(dir,"test-opsys.dwn");try(RandomAccessFile out=new RandomAccessFile(opsys,"rw")){out.setLength(229352);out.seek(0x12a4a);out.writeShort(0x42a7);}FirmwareFiles.validate("opsys.dwn",opsys);
            try(FirmwareGate.Lease use=FirmwareGate.use()){reject(()->FirmwareGate.change());}
            try(FirmwareGate.Lease change=FirmwareGate.change()){reject(()->FirmwareGate.use());reject(()->FirmwareGate.change());}
            for(String input:args){File original=new File(input);if(Arrays.asList("eprom.bin","opsys.dwn","candi.bin").contains(original.getName())){FirmwareFiles.validate(original.getName(),original);System.out.println(original.getName()+" support validation PASS");}else{FirmwareFiles.card(original,image,()->false);System.out.println(original.getName()+" extracted sha256="+FirmwareFiles.sha(image));}}
            System.out.println("PASS: streaming ZIP/raw image validation, traversal/multiple images/oversize/truncation rejection, checksum mismatch, cancellation preserves destination, session/install exclusion");
        }finally{try(java.util.stream.Stream<Path> paths=Files.walk(dir.toPath())){paths.sorted(Comparator.reverseOrder()).forEach(p->{try{Files.delete(p);}catch(IOException e){throw new RuntimeException(e);}});}}
    }
}
