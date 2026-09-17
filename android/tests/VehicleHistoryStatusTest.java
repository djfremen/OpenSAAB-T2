package com.opensaab.usb;
import java.io.*;import java.nio.file.*;import java.time.*;import java.util.*;
public final class VehicleHistoryStatusTest {
 static void check(boolean ok,String reason){if(!ok)throw new AssertionError(reason);}
 public static void main(String[] args)throws Exception{
  File dir=Files.createTempDirectory("vehicle-history").toFile(),card=new File(dir,"card.bin");
  Instant t=Instant.parse("2026-09-16T12:00:00Z");
  String vin="YS3FD49YX41000001",other="YS3FD49YX41000002";
  VehicleIdentity car=new VehicleIdentity(vin,t.toString(),"test",2004,"available","9440","","","","");
  byte[] data=new byte[SsaData.SIZE];Arrays.fill(data,(byte)255);System.arraycopy(vin.getBytes("US-ASCII"),0,data,0x14,17);data[0x26]=1;
  SecurityAccessStatus receipt=new SecurityAccessStatus(vin,"test",t);receipt.collected(t);receipt.processed("test","",t);receipt.imported(data,t);
  try{
   try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET);f.write(data);}
   VehicleHistoryStatus h=new VehicleHistoryStatus(car,receipt,card,t.plusSeconds(60));
   check(h.auth.contains("[POST-AUTH] · Fresh"),"Fresh matching post-auth missing");check(h.timestamp.contains(VehicleHistoryStatus.time(t.toString())),"Written timestamp missing");
   h=new VehicleHistoryStatus(car,receipt,card,t.plusSeconds(10800));check(h.auth.endsWith("Stale"),"Stale boundary missing");
   check(h.connection.equals("Last connection · "+VehicleHistoryStatus.time(car.observedUtc)),"Reopening changed connection time");
   SecurityAccessStatus wrong=new SecurityAccessStatus(other,"other",t);wrong.processed("test","",t);wrong.imported(data,t);
   h=new VehicleHistoryStatus(car,wrong,card,t);check(h.timestamp.equals("Timestamp unavailable")&&!h.auth.contains("Fresh"),"Other vehicle's receipt leaked");
   data[0x26]=2;try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET);f.write(data);}
   h=new VehicleHistoryStatus(car,receipt,card,t);check(h.timestamp.equals("Timestamp unavailable"),"Changed card inherited import date");
   System.arraycopy(other.getBytes("US-ASCII"),0,data,0x14,17);try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET);f.write(data);}
   h=new VehicleHistoryStatus(car,receipt,card,t);check(h.auth.equals("auth_status: [N/A]"),"Other vehicle's card leaked");
   check(VehicleHistoryStatus.time("bad").equals("Date unknown"),"Invalid date fabricated");
   h=new VehicleHistoryStatus(null,receipt,card,t);check(h.auth.equals("auth_status: [N/A]"),"Missing identity used old receipt");
   System.out.println("Vehicle history: saved connection timestamp, freshness, changed card, other VIN and missing history PASS");
  }finally{card.delete();dir.delete();}
 }
}
