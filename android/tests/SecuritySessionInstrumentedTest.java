// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.os.*;
import android.widget.*;
import java.io.*;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
import org.json.JSONObject;

/** Disposable data only; exercises current guest selection and the actual shared view. */
public final class SecuritySessionInstrumentedTest extends Instrumentation {
    private static final String VIN="YS3FD49Y041000000";
    public void onCreate(Bundle b){super.onCreate(b);start();}
    static void check(boolean yes,String why){if(!yes)throw new AssertionError(why);}
    interface Work {void run()throws Exception;}
    static void rejected(Work work,String why)throws Exception{try{work.run();throw new AssertionError(why);}catch(IOException|IllegalArgumentException expected){}}
    void await(SecurityAccessView view,java.util.function.BooleanSupplier condition)throws Exception{
        long deadline=SystemClock.elapsedRealtime()+8000;
        while(SystemClock.elapsedRealtime()<deadline){boolean[] done={false};runOnMainSync(()->{view.refresh();done[0]=condition.getAsBoolean();});if(done[0])return;SystemClock.sleep(50);}throw new AssertionError("Security state did not refresh");
    }
    static byte[] seeds(){byte[] b=new byte[SsaData.SIZE];Arrays.fill(b,(byte)255);b[0]=(byte)177;
        System.arraycopy(VIN.getBytes(java.nio.charset.StandardCharsets.US_ASCII),0,b,0x14,17);
        b[0x132]=1;b[0x133]=2;b[0x134]=3;b[0x135]=1;b[0x136]=0;b[0x137]=4;return b;}
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;SecurityAccessView view=null;
        try{
            check(getTargetContext().getPackageName().equals("com.opensaab.beta.diagnostics"),"Disposable package required");
            File files=getTargetContext().getFilesDir(),card=new File(files,"firmware/card.bin");card.getParentFile().mkdirs();
            byte[] pre=seeds(),post=pre.clone();Arrays.fill(post,0x26,0x2e,(byte)'1');post[0x138]=0;post[0x139]=5;
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(SsaData.OFFSET);f.write(post);}
            File parent=new File(files,"mdi-sessions/synthetic-collection"),run=new File(parent,"native");run.mkdirs();
            VehicleIdentity vehicle=new VehicleIdentity(VIN,Instant.now().toString(),"synthetic",2004,"unavailable","","","","","");
            VehicleSession.save(new File(run,VehicleSession.FILE),vehicle);
            Files.write(new File(run,"ssa-card-before.bin").toPath(),post);Files.write(new File(run,"ssa-card-after.bin").toPath(),pre);
            JSONObject evidence=new JSONObject().put("origin","original-guest-memory").put("card_offset",SsaData.OFFSET).put("bytes",SsaData.SIZE)
                .put("ssa_memory_flash_enabled",true).put("ssa_erases",1).put("ssa_programmed_bytes",714);
            File metadata=new File(run,"native-security-snapshot.json");FirmwareStore.writeJson(metadata,evidence);
            File log=new File(parent,"native-process-private.log");Files.write(log.toPath(),"LCD: Help You need Security Access from TIS2000 1. Disconnect Tech 2 from Vehicle.\n".getBytes("UTF-8"));
            Files.write(new File(run,"screen.txt").toPath(),"Initial screen".getBytes("UTF-8"));
            SecuritySessionData.recordBaseline(run,card);
            check(SecuritySessionData.load(run,card,vehicle).completed,"Legacy completed collection lost on reopen");
            check(Arrays.equals(SecuritySessionData.processingInput(run,card,vehicle,pre),pre),"Existing seeds substituted");
            byte[] changed=pre.clone();changed[0x137]=9;
            rejected(()->SecuritySessionData.processingInput(run,card,vehicle,changed),"Changed selection accepted");
            FirmwareStore.writeJson(metadata,evidence.put("ssa_programmed_bytes",100));
            rejected(()->SecuritySessionData.processingInput(run,card,vehicle,pre),"Partial collection accepted");
            FirmwareStore.writeJson(metadata,evidence.put("ssa_programmed_bytes",714));
            File baseline=new File(run,"security-card-baseline.sha256");Files.write(baseline.toPath(),SsaCardImport.hash(card).getBytes("US-ASCII"));
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(42);f.write(1);}
            rejected(()->SecuritySessionData.processingInput(run,card,vehicle,pre),"Changed firmware accepted");
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(42);f.write(0);}
            log.delete();rejected(()->SecuritySessionData.processingInput(run,card,vehicle,pre),"Missing transfer evidence accepted");
            Files.write(new File(run,"collection-ssa.bin").toPath(),pre);Files.write(new File(run,"working-ssa.bin").toPath(),pre);
            check(SecuritySessionData.load(run,card,vehicle).state==SsaState.PRE_AUTH,"Startup POST-AUTH hid working seeds");
            Files.write(new File(run,"working-ssa.bin").toPath(),new byte[]{1});
            check(SecuritySessionData.load(run,card,vehicle).state==SsaState.UNAVAILABLE,"Broken checkpoint substituted startup data");
            Files.write(new File(run,"working-ssa.bin").toPath(),pre);
            byte[] other=pre.clone();other[0x24]='2';Files.write(new File(run,"working-ssa.bin").toPath(),other);
            check(SecuritySessionData.load(run,card,vehicle).state==SsaState.UNAVAILABLE,"Another vehicle inherited seeds");
            Files.write(new File(run,"working-ssa.bin").toPath(),pre);
            Instant old=Instant.now().minusSeconds(14400);SecurityAccessStatus receipt=new SecurityAccessStatus(VIN,"old-session",old);
            receipt.processed("OpenSAAB","",old);receipt.imported(post,old);File receiptFile=new File(getTargetContext().getNoBackupFilesDir(),"security-processing-status.properties");receipt.save(receiptFile);
            check(VehicleHistoryStatus.working(vehicle,receipt,SecuritySessionData.savedBytes(files,vehicle),Instant.now()).authState.equals(SsaState.PRE_AUTH.label),"Main history still showed startup stale auth over saved seeds");
            byte[] oldReceipt=Files.readAllBytes(receiptFile.toPath());String originalCard=SsaCardImport.hash(card);
            activity=startActivitySync(new Intent(getTargetContext(),ChipsoftUsbActivity.class).putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final Activity host=activity;final boolean[] running={false};final ArrayList<Integer> keys=new ArrayList<>();final int[] starts={0};final SecurityAccessView[] ui={null};
            runOnMainSync(()->{ui[0]=new SecurityAccessView(host,false,()->run,()->running[0],()->running[0]=false,()->starts[0]++,()->starts[0]++);ui[0].setMenuKey(k->{keys.add(k);return true;});});
            view=ui[0];final SecurityAccessView v=view;
            await(v,()->"Process existing data".equals(((Button)v.findViewWithTag("security-action")).getText().toString()));
            check(v.readyToProcess(),"Old imported receipt suppressed collected seeds");
            // First refresh after manual start must not reset collection.
            runOnMainSync(()->{v.collectInCurrentSession();running[0]=true;});
            await(v,v::readyToProcess);check(keys.isEmpty()&&starts[0]==0,"Manual collection sent keys or restarted");
            // A stale POST-AUTH display must leave fresh collection reachable from Actions.
            running[0]=false;Files.write(new File(run,"working-ssa.bin").toPath(),post);
            runOnMainSync(v::returnedToFirmware);
            await(v,()->"Continue firmware".equals(((Button)v.findViewWithTag("security-action")).getText().toString()));
            check(((TextView)v.findViewWithTag("security-state")).getText().toString().contains("Stale"),"Missing advisory age");
            runOnMainSync(v::requestAccess);waitForIdleSync();
            boolean freshVisible=false;long windowDeadline=SystemClock.elapsedRealtime()+5000;
            while(SystemClock.elapsedRealtime()<windowDeadline){
                android.view.accessibility.AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();
                if(root!=null&&!root.findAccessibilityNodeInfosByText("Start collection").isEmpty()){freshVisible=true;break;}
                SystemClock.sleep(50);
            }
            check(freshVisible,"Stale receipt blocked fresh collection action");
            sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK);waitForIdleSync();
            check(starts[0]==0&&keys.isEmpty()&&!SecurityAccessView.workflowBusy(),"Dialog disclosed data or started work");
            check(originalCard.equals(SsaCardImport.hash(card))&&Arrays.equals(oldReceipt,Files.readAllBytes(receiptFile.toPath())),"Status rendering modified card or receipt age");
            // Real MDI entry with auto_start must restore saved seeds before any USB discovery.
            runOnMainSync(v::close);runOnMainSync(host::finish);
            Files.write(new File(run,"working-ssa.bin").toPath(),pre);
            MdiUsbActivity mdi=(MdiUsbActivity)startActivitySync(new Intent(getTargetContext(),MdiUsbActivity.class)
                .putExtra("auto_start",true).putExtra("mdi_security_request",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));activity=mdi;
            java.lang.reflect.Field securityField=MdiUsbActivity.class.getDeclaredField("security");securityField.setAccessible(true);
            SecurityAccessView mdiView=(SecurityAccessView)securityField.get(mdi);
            await(mdiView,mdiView::readyToProcess);
            check(!MdiUsbActivity.sessionActive()&&!FirmwareGate.sessionActive(),"Reopen connected before offering existing seeds");
            result.putString("stream","PASS: current guest PRE-AUTH over stale startup POST-AUTH; legacy completed seed reopen; exact selection, partial/missing evidence, changed firmware and vehicle rejection; corrupt checkpoint fails closed; first manual refresh preserved; actual MDI auto-start entry restores existing seeds without USB; stale Actions offers fresh collection; no API/USB/firmware keys/card or receipt writes by UI.\n");
        }catch(Throwable e){code=0;StringWriter trace=new StringWriter();e.printStackTrace(new PrintWriter(trace));result.putString("stream","FAIL: "+trace+"\n");}
        finally{if(view!=null){SecurityAccessView v=view;runOnMainSync(v::close);}if(activity!=null){Activity a=activity;runOnMainSync(a::finish);}finish(code,result);}
    }
}
