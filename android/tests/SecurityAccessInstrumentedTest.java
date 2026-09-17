// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.Intent;
import android.graphics.Rect;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.file.*;

/** Uses a private synthetic prompt; no API requests, USB or card import. */
public final class SecurityAccessInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    void main(Runnable r){Throwable[] failure={null};runOnMainSync(()->{try{r.run();}catch(Throwable e){failure[0]=e;}});if(failure[0]!=null)throw new AssertionError(failure[0]);}
    void awaitUi(ChipsoftUsbActivity a,java.util.function.BooleanSupplier ready){
        long deadline=SystemClock.elapsedRealtime()+5000;
        while(SystemClock.elapsedRealtime()<deadline){
            final boolean[] done={false};main(()->{a.securityAccess.refresh();done[0]=ready.getAsBoolean();});
            if(done[0])return;SystemClock.sleep(50);
        }
        throw new AssertionError("Security UI did not reach expected state");
    }
    public void onStart(){
        Bundle result=new Bundle();int code=-1;ChipsoftUsbActivity activity=null;File dir=null;File receiptFile=null;byte[] oldReceipt=null;
        try{
            activity=(ChipsoftUsbActivity)startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final ChipsoftUsbActivity a=activity;waitForIdleSync();
            receiptFile=new File(a.getNoBackupFilesDir(),"security-processing-status.properties");
            if(receiptFile.exists())oldReceipt=Files.readAllBytes(receiptFile.toPath());
            receiptFile.delete();
            dir=Files.createTempDirectory(a.getCacheDir().toPath(),"security-ui-test-").toFile();final File run=dir;
            File screen=new File(dir,"native-dtc-screen.txt");
            Files.write(screen.toPath(),"F6: Get Security Access".getBytes("UTF-8"));
            main(()->{a.nativeDirectory=run;a.securityAccess.refresh();});awaitUi(a,()->"Details".equals(((Button)a.securityAccess.findViewWithTag("security-action")).getText().toString()));
            main(()->check(((Button)a.securityAccess.findViewWithTag("security-action")).getText().toString().equals("Details"),"Menu label falsely detected as request"));
            Files.write(screen.toPath(),"Help\nYou need Security Access from TIS2000\n1. Disconnect Tech 2 from Vehicle.".getBytes("UTF-8"));
            awaitUi(a,()->"Get security access".equals(((Button)a.securityAccess.findViewWithTag("security-action")).getText().toString()));
            main(()->{
                check(a.securityAccess.getVisibility()==View.VISIBLE,"TIS prompt action missing");
                Button action=(Button)a.securityAccess.findViewWithTag("security-action");check(action.getText().toString().equals("Get security access"),"Wrong next action");
                View exit=a.getWindow().getDecorView().findViewWithTag("tech2-key-1");Rect r=new Rect();check(exit.getGlobalVisibleRect(r)&&r.height()==exit.getHeight(),"Security banner hid EXIT");
                check(!a.running.get()&&!SecurityAccessView.workflowBusy(),"Detection initiated a session or request");
                action.performClick(); // Opens an explanation only, never taps Start collection.
            });waitForIdleSync();sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);waitForIdleSync();
            main(()->check(!a.running.get()&&!SecurityAccessView.workflowBusy(),"Dismissing security dialog initiated work"));
            // Reload a local processing receipt into a later firmware session. No API or card changes.
            VehicleIdentity vehicle=new VehicleIdentity("YS3TEST1A41000001",java.time.Instant.now().toString(),"synthetic test",2004,"unavailable","","","","","");
            VehicleSession.save(new File(run,VehicleSession.FILE),vehicle);
            SecurityAccessStatus receipt=new SecurityAccessStatus(vehicle.vin,"previous-collection",java.time.Instant.now());
            receipt.processed("OpenSAAB","OSSEC-00000000000000000000000000000001",java.time.Instant.now());receipt.save(receiptFile);
            Files.write(screen.toPath(),"Diagnostics".getBytes("UTF-8"));
            awaitUi(a,()->((TextView)a.securityAccess.findViewWithTag("security-message")).getText().toString().contains("Previous security processing"));
            main(()->{
                TextView text=(TextView)a.securityAccess.findViewWithTag("security-message");
                check(a.securityAccess.getVisibility()==View.VISIBLE&&text.getText().toString().contains("Previous security processing"),"Processing history vanished on return to firmware");
                check(text.getText().toString().contains("Not yet loaded")&&!text.getText().toString().contains("unverified"),"Processing status should describe only the completed step");
                check(((Button)a.securityAccess.findViewWithTag("security-action")).getText().toString().contains("Details"),"Receipt details unavailable");
                View exit=a.getWindow().getDecorView().findViewWithTag("tech2-key-1");Rect r=new Rect();check(exit.getGlobalVisibleRect(r)&&r.height()==exit.getHeight(),"Persistent status hid EXIT");
            });
            // Enter security access naturally in a full-control run; do not start another activity/session.
            Files.write(screen.toPath(),"Checking Security Access\nReading all vehicle VINs OK\nReading all vehicle Seed Working".getBytes("UTF-8"));
            final java.util.ArrayList<Integer> sent=new java.util.ArrayList<>();
            main(()->{a.running.set(true);a.securityAccess.setMenuKey(k->{sent.add(k);return true;});});
            awaitUi(a,()->((TextView)a.securityAccess.findViewWithTag("security-state")).getText().toString().contains("Collecting pre-auth"));
            Files.write(screen.toPath(),"Help\nYou need Security Access from TIS2000\n1. Disconnect Tech 2 from Vehicle.".getBytes("UTF-8"));
            awaitUi(a,()->"Process security data".equals(((Button)a.securityAccess.findViewWithTag("security-action")).getText().toString()));
            main(()->{
                check(a.nativeDirectory==run&&a.running.get()&&!SecurityAccessView.workflowBusy(),"Manual detection restarted/stopped/submitted the session");
                check(sent.isEmpty(),"Manual detection sent firmware keys");
                View shortcuts=a.getWindow().getDecorView().findViewWithTag("tech2-actions");Rect r=new Rect();
                check(shortcuts!=null&&shortcuts.getGlobalVisibleRect(r)&&r.height()==shortcuts.getHeight(),"Session shortcuts clipped/hidden");
                a.updateWorkspace();
                TextView compact=a.getWindow().getDecorView().findViewWithTag("session-summary");check(compact.getText().toString().contains("Ready to process"),"Manual collection ready state missing from compact header");
                a.running.set(false);
            });
            new File(run,VehicleSession.FILE).delete();
            awaitUi(a,()->!((TextView)a.securityAccess.findViewWithTag("security-message")).getText().toString().contains("Previous security processing"));
            main(()->check(!((TextView)a.securityAccess.findViewWithTag("security-message")).getText().toString().contains("Previous security processing"),"History shown without matching vehicle identity"));
            result.putString("stream","PASS: manual full-control seed collection reaches Process security data in same session with no repeated navigation/API; Actions remains visible; persistent history, identity scope, prompt/menu discrimination, EXIT and dialog dismissal; no USB/API/card operations\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(activity!=null){final Activity a=activity;main(a::finish);}if(dir!=null){for(File f:dir.listFiles())f.delete();dir.delete();}
            if(receiptFile!=null){try{if(oldReceipt==null)receiptFile.delete();else Files.write(receiptFile.toPath(),oldReceipt);}catch(Exception ignored){}}
            finish(code,result);}
    }
}
