// SPDX-License-Identifier: MPL-2.0
import com.opensaab.usb.*;
import java.io.*;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
public class AndroidReference {
 public static void main(String[] args)throws Exception {
  File[] dirs=new File(args[0]).listFiles(File::isDirectory);int count=0;
  for(File dir:dirs){
   Properties p=new Properties();try(InputStream in=new FileInputStream(new File(dir,"case.properties"))){p.load(in);}
   File raw=new File(dir,"ssa.bin"),card=new File(dir,"card.bin");byte[] bytes=raw.exists()?Files.readAllBytes(raw.toPath()):null;
   SsaState state=SsaState.analyze(bytes);String age="Age unknown",vin=null,vehicle=p.getProperty("vehicle");
   try{vin=SsaData.vin(bytes);}catch(Exception invalid){}
   if(vehicle!=null&&state!=SsaState.INIT_AUTH&&!vehicle.equals(vin))state=SsaState.UNAVAILABLE;
   SecurityAccessStatus receipt=SecurityAccessStatus.read(new File(dir,"receipt.properties"));
   if(state==SsaState.POST_AUTH&&receipt!=null&&vin!=null&&receipt.matches(vin))age=receipt.freshness(receipt.cardMatches(card),Instant.parse(p.getProperty("now")));
   if(!state.name().equals(p.getProperty("state"))||!age.equals(p.getProperty("age")))throw new AssertionError(p.getProperty("name")+": "+state+" "+age);
   count++;
  }
  System.out.println("PASS Android reference: "+count+" shared security-status vectors");
 }
}
