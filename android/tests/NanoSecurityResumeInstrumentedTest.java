// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.Intent;
import android.os.*;
import android.widget.Button;
import java.io.*;
import java.nio.file.Files;
import java.time.Instant;
import java.util.Arrays;

/** Saved import and clean idle handoff in an isolated AVD; no adapter or API access. */
public final class NanoSecurityResumeInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    static void check(boolean yes,String why){if(!yes)throw new AssertionError(why);}
    public void onStart(){Bundle result=new Bundle();int code=-1;NanoProbeActivity a=null;SecurityAccessView view=null;
        try{
            check("com.opensaab.beta.diagnostics".equals(getTargetContext().getPackageName()),"Disposable harness required");
            a=(NanoProbeActivity)startActivitySync(new Intent(getTargetContext(),NanoProbeActivity.class).putExtra("nano_security_collect",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final NanoProbeActivity host=a;waitForIdleSync();
            runOnMainSync(()->{
                host.cancelled=false;host.sessionCleanupConfirmed=true;
                check(!host.running.get()&&!NanoProbeActivity.sessionActive(),"Unexpected transport ownership");
                host.stopForSecurityHandoff();check(!host.cancelled,"Clean idle handoff cancelled resume");
                host.sessionCleanupConfirmed=false;host.resumeSecurityFirmware();
                check(host.nativeSecurityCollect&&!host.nativeFullNative,"Unconfirmed cleanup resumed firmware");
                host.sessionCleanupConfirmed=true;host.foreground=false;host.resumeSecurityFirmware();
                check(host.nativeFullNative&&!host.nativeSecurityCollect&&!host.running.get(),"Clean import did not switch mode or started in background");
                check(host.pendingSecurityStart!=null,"Background continuation lost");
                host.pendingSecurityStart=null;host.cancelled=true;host.nativeFullNative=false;host.nativeSecurityCollect=true;
                host.resumeSecurityFirmware();check(!host.nativeFullNative,"Explicit cancellation resumed firmware");
                host.cancelled=false;
            });
            File card=new File(getTargetContext().getFilesDir(),"firmware/card.bin"),run=Files.createTempDirectory(getTargetContext().getFilesDir().toPath(),"import-resume-").toFile();card.getParentFile().mkdirs();
            byte[] pre=SecuritySessionInstrumentedTest.seeds(),post=pre.clone();Arrays.fill(post,0x26,0x2e,(byte)'1');post[0x138]=0;post[0x139]=5;
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(SsaData.OFFSET);f.write(post);}
            Files.write(new File(run,"working-ssa.bin").toPath(),pre);
            VehicleIdentity vehicle=new VehicleIdentity(SsaData.vin(pre),Instant.now().toString(),"synthetic",2004,"not_requested","","","","","");VehicleSession.save(new File(run,VehicleSession.FILE),vehicle);
            SecurityAccessStatus receipt=new SecurityAccessStatus(vehicle.vin,run.getName(),Instant.now());receipt.processed("OpenSAAB","",Instant.now());receipt.imported(post,Instant.now());
            File receiptFile=new File(getTargetContext().getNoBackupFilesDir(),"security-processing-status.properties");receipt.save(receiptFile);byte[] originalReceipt=Files.readAllBytes(receiptFile.toPath());String originalCard=SsaCardImport.hash(card);
            check(SecuritySessionData.load(run,card,vehicle).state==SsaState.PRE_AUTH,"Fixture needs historical PRE");
            check(SecuritySessionData.imported(run,card,vehicle,receipt).state==SsaState.POST_AUTH,"Matching imported result hidden by old PRE");
            check(SecuritySessionData.imported(new File(run,"other"),card,vehicle,receipt)==null,"Other session inherited import");
            File mdiOne=new File(run,"mdi-one/native"),mdiTwo=new File(run,"mdi-two/native");
            SecurityAccessStatus mdiReceipt=new SecurityAccessStatus(vehicle.vin,SecurityAccessStatus.sessionId(mdiOne),Instant.now());
            check(mdiReceipt.sameSession(mdiOne)&&!mdiReceipt.sameSession(mdiTwo),"MDI native basename aliases distinct collections");
            VehicleIdentity different=new VehicleIdentity("YS3FD49Y051000000",Instant.now().toString(),"synthetic",2005,"not_requested","","","","","");
            check(SecuritySessionData.imported(run,card,different,receipt)==null,"Other full VIN inherited import");
            final SecurityAccessView[] made={null};runOnMainSync(()->made[0]=new SecurityAccessView(host,false,()->run,()->false,()->{},()->{},()->{}));view=made[0];final SecurityAccessView saved=view;
            long until=SystemClock.elapsedRealtime()+8000;boolean ready=false;
            while(SystemClock.elapsedRealtime()<until){boolean[] ok={false};runOnMainSync(()->{saved.refresh();ok[0]="Continue firmware".contentEquals(((Button)saved.findViewWithTag("security-action")).getText());});if(ok[0]){ready=true;break;}SystemClock.sleep(50);}
            check(ready&&!saved.readyToProcess(),"Imported result still offers duplicate processing");
            check(originalCard.equals(SsaCardImport.hash(card))&&Arrays.equals(originalReceipt,Files.readAllBytes(receiptFile.toPath())),"Recovery mutated data or import time");
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET+0x26);f.write('2');}
            check(SecuritySessionData.imported(run,card,vehicle,receipt)==null,"Changed SSA inherited import");
            result.putString("stream","PASS: idle handoff preserves resume; unconfirmed cleanup and cancellation block it; background defers it; same-run validated import supersedes historical PRE; different VIN/session/SSA rejected; Continue firmware avoids duplicate processing and retains card/receipt/time. Synthetic AVD, no USB/API/vehicle commands.\n");
        }catch(Throwable e){code=0;StringWriter trace=new StringWriter();e.printStackTrace(new PrintWriter(trace));result.putString("stream","FAIL: "+trace+"\n");}
        finally{if(view!=null){SecurityAccessView v=view;runOnMainSync(v::close);}if(a!=null){Activity host=a;runOnMainSync(host::finish);}finish(code,result);}
    }
}
