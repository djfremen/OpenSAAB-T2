// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.os.*;
import android.view.accessibility.AccessibilityNodeInfo;
import android.widget.*;
import java.io.*;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
import java.util.concurrent.atomic.*;
import org.json.JSONObject;

/** Synthetic fresh-collection regression. No USB, API, real firmware or vehicle commands. */
public final class SecurityCollectionInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    void main(Runnable r){Throwable[] error={null};runOnMainSync(()->{try{r.run();}catch(Throwable e){error[0]=e;}});if(error[0]!=null)throw new AssertionError(error[0]);}
    void await(SecurityAccessView view,java.util.function.BooleanSupplier ready){
        long until=SystemClock.elapsedRealtime()+8000;
        while(SystemClock.elapsedRealtime()<until){boolean[] done={false};main(()->{view.refresh();done[0]=ready.getAsBoolean();});if(done[0])return;SystemClock.sleep(50);}
        throw new AssertionError("Fresh collection UI did not progress");
    }
    void clickDialog(String label){
        long until=SystemClock.elapsedRealtime()+4000;
        while(SystemClock.elapsedRealtime()<until){AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();
            if(root!=null)for(AccessibilityNodeInfo node:root.findAccessibilityNodeInfosByText(label))
                if(node.getText()!=null&&label.equalsIgnoreCase(node.getText().toString())&&node.performAction(AccessibilityNodeInfo.ACTION_CLICK))return;
            SystemClock.sleep(50);
        }throw new AssertionError("Missing collection choice: "+label+" window="+windowText(getUiAutomation().getRootInActiveWindow()));
    }
    static String windowText(AccessibilityNodeInfo n){if(n==null)return "none";StringBuilder s=new StringBuilder(String.valueOf(n.getText()));for(int i=0;i<n.getChildCount();i++)s.append(" | ").append(windowText(n.getChild(i)));return s.toString();}
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;SecurityAccessView view=null;
        try{
            check("com.opensaab.beta.diagnostics".equals(getTargetContext().getPackageName()),"Disposable harness required");
            File files=getTargetContext().getFilesDir(),root=Files.createTempDirectory(files.toPath(),"fresh-test-").toFile();
            File card=new File(files,"firmware/card.bin");card.getParentFile().mkdirs();
            byte[] pre=SecuritySessionInstrumentedTest.seeds(),post=pre.clone(),empty=new byte[SsaData.SIZE];Arrays.fill(empty,(byte)255);
            Arrays.fill(post,0x26,0x2e,(byte)'1');post[0x138]=0;post[0x139]=5;
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(41);f.write(93);f.seek(SsaData.OFFSET-1);f.write(94);f.write(post);f.write(95);}
            String original=SsaCardImport.hash(card);
            FirmwareStore.writeJson(new File(card.getParentFile(),"active.json"),new JSONObject().put("id",FirmwareCatalog.ENTRIES[0].id).put("sha256_at_activation",FirmwareCatalog.ENTRIES[0].imageSha));
            File run=new File(root,"fresh");run.mkdir();SecuritySessionData.recordBaseline(run,card);
            File copy=SecurityCollectionCard.prepare(card,run);
            check(original.equals(SsaCardImport.hash(card)),"Fresh collection changed selected card");
            check(Arrays.equals(empty,SecuritySessionData.card(copy)),"Old POST-AUTH remained in guest card");
            check(Arrays.equals(post,SecurityCollectionCard.beforeImport(run)),"Original import baseline lost");
            try(RandomAccessFile f=new RandomAccessFile(copy,"r")){f.seek(41);check(f.read()==93,"Other firmware changed");f.seek(SsaData.OFFSET-1);check(f.read()==94,"Pre-SSA boundary changed");f.seek(SsaData.OFFSET+SsaData.SIZE);check(f.read()==95,"Post-SSA boundary changed");}
            SecuritySessionInstrumentedTest.rejected(()->SecurityCollectionCard.prepare(card,run),"Existing collection overwritten");
            VehicleIdentity vehicle=new VehicleIdentity("YS3FD49Y041000000",Instant.now().toString(),"synthetic",2004,"not_requested","","","","","");
            VehicleSession.save(new File(run,VehicleSession.FILE),vehicle);
            Files.write(new File(run,"ssa-card-before.bin").toPath(),empty);Files.write(new File(run,"ssa-card-after.bin").toPath(),pre);Files.write(new File(run,"collection-ssa.bin").toPath(),pre);
            JSONObject evidence=new JSONObject().put("origin","original-guest-memory").put("card_offset",SsaData.OFFSET).put("bytes",SsaData.SIZE).put("ssa_memory_flash_enabled",true).put("ssa_erases",1).put("ssa_programmed_bytes",714);
            FirmwareStore.writeJson(new File(run,"native-security-snapshot.json"),evidence);
            check(Arrays.equals(pre,SecuritySessionData.processingInput(run,card,vehicle,pre)),"Cleared guest baseline cannot import against preserved source");
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(41);f.write(0);}
            SecuritySessionInstrumentedTest.rejected(()->SecuritySessionData.processingInput(run,card,vehicle,pre),"Changed source accepted");
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(41);f.write(93);}
            // A prior matching imported receipt must not cancel a newly requested collection.
            new File(run,"ssa-card-after.bin").delete();new File(run,"collection-ssa.bin").delete();
            Files.write(new File(run,"working-ssa.bin").toPath(),post);
            SecurityAccessStatus receipt=new SecurityAccessStatus(vehicle.vin,"prior",Instant.now());receipt.processed("OpenSAAB","",Instant.now());receipt.imported(post,Instant.now());
            File receiptFile=new File(getTargetContext().getNoBackupFilesDir(),"security-processing-status.properties");receipt.save(receiptFile);byte[] saved=Files.readAllBytes(receiptFile.toPath());
            File screen=new File(run,"screen.txt");Files.write(screen.toPath(),"Main Menu\nF0: Diagnostics\nF1: Service".getBytes("UTF-8"));
            activity=startActivitySync(new Intent(getTargetContext(),ChipsoftUsbActivity.class).putExtra(SecurityCollectionMode.EXTRA,SecurityCollectionMode.GUIDED).putExtra(SecurityCollectionMode.PLATFORM_EXTRA,"9440").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final Activity host=activity;final ArrayList<Integer> keys=new ArrayList<>();final AtomicInteger starts=new AtomicInteger();final AtomicBoolean running=new AtomicBoolean(true);final SecurityAccessView[] made={null};
            main(()->{made[0]=new SecurityAccessView(host,true,()->run,running::get,()->running.set(false),starts::incrementAndGet,starts::incrementAndGet);made[0].setMenuKey(k->{keys.add(k);return true;});});view=made[0];final SecurityAccessView guided=view;
            await(guided,()->!keys.isEmpty());
            main(()->{check(keys.size()==1&&keys.get(0)==0x18,"Guided collection did not select observed Diagnostics");check(guided.collecting(),"Old receipt hijacked fresh collection");check("Waiting for collection".contentEquals(((Button)guided.findViewWithTag("security-action")).getText()),"Fresh collection showed continuation/authorization");});
            Files.write(screen.toPath(),"Turn Ignition key to LOCK position\nPress ENTER".getBytes("UTF-8"));
            await(guided,()->((TextView)guided.findViewWithTag("security-message")).getText().toString().contains("LOCK"));check(keys.size()==1,"Ignition prompt auto-acknowledged");
            main(guided::manualNavigation);Files.write(screen.toPath(),"Main Menu\nF0: Diagnostics".getBytes("UTF-8"));
            main(guided::refresh);SystemClock.sleep(300);main(guided::refresh);check(keys.size()==1,"Manual takeover sent another menu key");
            main(guided::close);host.getIntent().putExtra(SecurityCollectionMode.EXTRA,SecurityCollectionMode.MANUAL);
            main(()->{made[0]=new SecurityAccessView(host,true,()->run,running::get,()->running.set(false),starts::incrementAndGet,starts::incrementAndGet);made[0].setMenuKey(k->{keys.add(k);return true;});});view=made[0];final SecurityAccessView manual=view;
            await(manual,()->"Waiting for collection".contentEquals(((Button)manual.findViewWithTag("security-action")).getText()));SystemClock.sleep(200);main(manual::refresh);check(keys.size()==1,"Manual collection sent menu keys");
            main(manual::requestAccess);waitForIdleSync();clickDialog("Guided collection");
            long until=SystemClock.elapsedRealtime()+5000;while(starts.get()==0&&SystemClock.elapsedRealtime()<until)SystemClock.sleep(50);
            check(starts.get()==1&&!running.get(),"Fresh choice did not release and invoke collection");
            check(SecurityCollectionMode.GUIDED.equals(SecurityCollectionMode.from(host.getIntent())),"Guided choice lost across restart");
            check("9440".equals(host.getIntent().getStringExtra(SecurityCollectionMode.PLATFORM_EXTRA)),"Confirmed menu target lost");
            check(original.equals(SsaCardImport.hash(card))&&Arrays.equals(saved,Files.readAllBytes(receiptFile.toPath())),"Fresh UI changed source/receipt");
            check(!SecurityAccessView.workflowBusy(),"Fresh collection gate stayed busy");
            result.putString("stream","PASS: fresh guest card erases exactly SSA and retains original baseline; preserved-source import validation, changed source and duplicate preparation rejection; matching old POST-AUTH cannot suppress fresh collection; guided observed-menu selection; no ignition acknowledgements; manual takeover and manual collection send no keys; fresh choice releases session/calls collection and preserves mode; no USB/API/vehicle traffic.\n");
        }catch(Throwable e){code=0;StringWriter trace=new StringWriter();e.printStackTrace(new PrintWriter(trace));result.putString("stream","FAIL: "+trace+"\n");}
        finally{if(view!=null){SecurityAccessView v=view;main(v::close);}if(activity!=null){Activity a=activity;main(a::finish);}finish(code,result);}
    }
}
