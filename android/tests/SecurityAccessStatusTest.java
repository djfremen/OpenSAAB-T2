// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.nio.file.*;
import java.time.Instant;
public final class SecurityAccessStatusTest {
    static void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    public static void main(String[] args)throws Exception{
        File dir=Files.createTempDirectory("security-status-test-").toFile(),file=new File(dir,"status"),card=new File(dir,"card");
        try{
            String vin="TESTVEH1CLE0000001";
            SecurityAccessStatus s=new SecurityAccessStatus(vin,"collection-1",Instant.parse("2026-09-16T12:00:00Z"));
            s.save(file);s=SecurityAccessStatus.read(file);
            check(s!=null&&s.matches(vin)&&!s.matches("TESTVEH1CLE0000002"),"Vehicle scope lost");
            check(!new String(Files.readAllBytes(file.toPath()),"UTF-8").contains(vin),"Receipt contains raw VIN");
            check(!s.imported()&&s.summary(true,true,false).contains("started"),"Started request implies success");
            check(s.summary(false,true,false).startsWith("Previous"),"Old processing presented as current");
            byte[] data=new byte[SsaData.SIZE];
            boolean rejected=false;try{s.imported(data,Instant.now());}catch(IllegalStateException expected){rejected=true;}check(rejected,"Import allowed without validated server reply");
            s.processed("OpenSAAB","OSSEC-00000000000000000000000000000001",Instant.parse("2026-09-16T12:00:03Z"));
            s.save(file);s=SecurityAccessStatus.read(file);
            check(!s.imported()&&s.summary(true,true,false).contains("Not yet loaded"),"API response implies vehicle grant");
            s.imported(data,Instant.parse("2026-09-16T12:00:04Z"));s.save(file);s=SecurityAccessStatus.read(file);
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET);f.write(data);}
            check(s.cardMatches(card),"Valid imported card not recognized");
            check(s.sameSession(new File(dir,"collection-1"))&&!s.sameSession(new File(dir,"collection-2")),"Session scope lost");
            check(s.summary(true,true,true).startsWith("Post-auth written"),"Import implies vehicle grant");
            check(s.summary(false,false,true).startsWith("Post-auth written"),"Stopped session hid completed import");
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET);f.write(1);}
            check(!s.cardMatches(card)&&s.summary(false,true,false).contains("Card has changed"),"Replaced card considered imported");
            check(!s.summary(true,false,true).toLowerCase().contains("unverified")&&!s.details(false,true).contains("NOT VERIFIED"),"Removed verification copy returned");
            s.failed(Instant.parse("2026-09-16T12:00:05Z"));s.save(file);s=SecurityAccessStatus.read(file);
            check(!s.imported()&&s.details(true,false).contains("Stopped:"),"Failure hidden");
            Files.write(file.toPath(),"schema=1\nstarted_utc=bad".getBytes("UTF-8"));check(SecurityAccessStatus.read(file)==null,"Malformed receipt accepted");
            System.out.println("PASS: security timestamps survive reload; vehicle/session/card scopes, failure and disconnect preserved; API/import never imply access granted; no raw VIN in receipt");
        }finally{file.delete();card.delete();dir.delete();}
    }
}
