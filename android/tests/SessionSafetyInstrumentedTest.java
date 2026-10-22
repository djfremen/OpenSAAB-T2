// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.lang.reflect.*;
import java.net.*;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
import org.json.JSONObject;

/** Production Java bindings with synthetic state and loopback only; no adapter/API/native launch. */
public final class SessionSafetyInstrumentedTest extends Instrumentation {
    static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    void main(Runnable work){Throwable[] failure={null};runOnMainSync(()->{try{work.run();}catch(Throwable e){failure[0]=e;}});if(failure[0]!=null)throw new AssertionError(failure[0]);}
    static Field field(Object owner,String name)throws Exception{Field f=owner.getClass().getDeclaredField(name);f.setAccessible(true);return f;}
    static boolean text(View view,String label){if(view instanceof TextView&&label.contentEquals(((TextView)view).getText()))return true;if(view instanceof ViewGroup)for(int i=0;i<((ViewGroup)view).getChildCount();i++)if(text(((ViewGroup)view).getChildAt(i),label))return true;return false;}
    void await(SecurityAccessView view,String label)throws Exception{
        long deadline=SystemClock.elapsedRealtime()+8000;
        while(SystemClock.elapsedRealtime()<deadline){boolean[] done={false};main(()->{view.refresh();done[0]=label.contentEquals(((Button)view.findViewWithTag("security-action")).getText());});if(done[0])return;Thread.sleep(50);}
        throw new AssertionError("Missing state: "+label);
    }
    public void onCreate(Bundle b){super.onCreate(b);start();}
    public void onStart(){Bundle output=new Bundle();int code=-1;Activity active=null;SecurityAccessView view=null;Socket peer=null;File card=null,backup=null,receiptFile=null;byte[] previousReceipt=null;
        try{
            check("com.opensaab.beta.diagnostics".equals(getTargetContext().getPackageName()),"Disposable package required");
            ChipsoftUsbActivity chip=(ChipsoftUsbActivity)startActivitySync(new Intent(getTargetContext(),ChipsoftUsbActivity.class).putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));active=chip;
            File run=Files.createTempDirectory(chip.getFilesDir().toPath(),"session-safety-").toFile();
            ServerSocket server=new ServerSocket(0,1,InetAddress.getLoopbackAddress());Socket local=new Socket(InetAddress.getLoopbackAddress(),server.getLocalPort());peer=server.accept();
            main(()->{chip.nativeDirectory=run;chip.running.set(true);chip.server=server;chip.client=local;chip.stop();
                check(new File(run,"native-stop").isFile()&&chip.nativeStop.pending(),"General Stop omitted native marker");
                check(!chip.cancelled&&!local.isClosed()&&!server.isClosed(),"Stop destroyed bridge before cleanup");
                check(!chip.sendMenuKey(0x10),"Stopping accepted new firmware input");chip.stop();
                chip.nativeStop.quitReceived();chip.running.set(false);chip.sessionCleanupConfirmed=true;long generation=chip.vehicleGeneration;chip.stopForSecurityHandoff();
                check(!chip.cancelled&&chip.sessionCleanupConfirmed&&generation==chip.vehicleGeneration,"Clean idle handoff cancelled Continue");chip.closeSockets();});
            VehicleIdentity freshVin=new VehicleIdentity("YS3FD49Y041000000",Instant.now().toString(),"queued synthetic VIN",2004,"not_requested","","","","","");
            int[] launches={0};
            for(boolean foreground:new boolean[]{false,true}){
                main(()->{chip.foreground=foreground;chip.running.set(false);chip.cancelled=false;chip.nativeStop.reset();chip.vehicle=null;chip.pendingVehicleStart=null;
                    long completedGeneration=chip.vehicleGeneration;
                    Runnable callback=chip.vinResultCallback(freshVin,completedGeneration,true,run,null,()->launches[0]++);
                    new Handler(Looper.getMainLooper()).post(callback); // Real production callback queued behind this Stop.
                    chip.stop();check(chip.vehicleGeneration!=completedGeneration,"Idle Stop did not revoke queued VIN generation");
                    try{chip.checkStartupCancellation(completedGeneration);throw new AssertionError("Cancelled startup admitted USB/child effects");}catch(IOException expected){}
                });waitForIdleSync();
                main(()->check(chip.pendingVehicleStart==null&&launches[0]==0&&chip.vehicle==null&&!chip.running.get(),"Queued VIN callback created a handoff or launched firmware after Stop"));
            }
            main(()->{chip.foreground=true;chip.sessionCleanupConfirmed=true;chip.cancelled=false;long generation=chip.vehicleGeneration;chip.vehicle=freshVin;chip.stopForSecurityHandoff();check(generation==chip.vehicleGeneration&&chip.vehicle==freshVin&&!chip.cancelled,"Idle security handoff changed identified continuation");});
            FirmwareStore.writeJson(new File(run,"adapter-diagnostics.json"),new JSONObject().put("protocol_cleanup","succeeded"));
            check("succeeded".equals(NativeCleanupEvidence.protocol(run)),"Final protocol success lost");
            ConnectionAttempt attempt=new ConnectionAttempt(chip,ConnectionAttempt.Adapter.CHIPSOFT);attempt.bind(run,"full_native","original_menus");attempt.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.USER_STOP);attempt.cleanup(5,false,"succeeded","failed");
            check(attempt.failed(),"Late Stop erased real native failure");
            ConnectionAttempt.collect(chip);JSONObject persisted=FirmwareStore.json(new File(run,"connection-evidence.json"));
            check("failed".equals(persisted.getJSONObject("cleanup").getString("protocol_cleanup"))&&"succeeded".equals(persisted.getJSONObject("cleanup").getString("usb_release")),"Independent final cleanup was not retained");
            for(int scenario=0;scenario<3;scenario++){
                ConnectionAttempt terminal=new ConnectionAttempt(chip,ConnectionAttempt.Adapter.CHIPSOFT);terminal.bind(run,"full_native","original_menus");
                terminal.finish(scenario==2?ConnectionAttempt.Outcome.FAILED:ConnectionAttempt.Outcome.CANCELLED,scenario==2?ConnectionAttempt.Reason.DISCONNECTED:ConnectionAttempt.Reason.USER_STOP);
                terminal.cleanup(scenario==0?137:0,scenario==0,scenario==1?"failed":"succeeded","succeeded");ConnectionAttempt.collect(chip);
                String reason=FirmwareStore.json(new File(run,"connection-evidence.json")).getString("primary_reason");
                check(terminal.failed()&&reason.equals(scenario==0?"TIMEOUT":scenario==1?"CLEANUP_FAILED":"DISCONNECTED"),"Final cleanup/force classification or root failure lost: "+reason);
            }
            File healthRun=Files.createTempDirectory(chip.getFilesDir().toPath(),"final-failure-").toFile();
            main(()->{chip.health.begin(healthRun);chip.health.expectedStop();chip.health.endedFailure(healthRun,"Synthetic final cleanup failure; nothing was uploaded.","Session ended with a problem");});waitForIdleSync();
            org.json.JSONArray healthEvents=SessionDiagnostics.read(healthRun);
            check(healthEvents.length()==3&&"UNEXPECTED_EXIT".equals(healthEvents.getJSONObject(2).getString("event")),"Expected Stop hid final failure");
            AlertDialog healthDialog=(AlertDialog)field(chip.health,"dialog").get(chip.health);
            check(healthDialog!=null,"Final failure did not offer explicit report review");main(()->healthDialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick());
            JSONObject mdi=new JSONObject().put("process_exit",5).put("forced_kill",false).put("transport_cleanup_ok",false).put("session_end_ok",true);
            MdiSessionOutcome mdiFailed=new MdiSessionOutcome(0,mdi);
            check(mdiFailed.failed&&mdiFailed.exitCode==5,"MDI carrier success hid failed native process");
            check(new MdiSessionOutcome(0,new JSONObject().put("process_exit",0).put("forced_kill",true)).failed,"MDI forced Stop became success");
            check(!new MdiSessionOutcome(0,new JSONObject().put("process_exit",0).put("forced_kill",false)).failed,"Clean MDI Stop became process failure");

            String vin="YS3FD49Y041000000";byte[] post=new byte[SsaData.SIZE];Arrays.fill(post,(byte)255);post[0]=(byte)177;System.arraycopy(vin.getBytes("US-ASCII"),0,post,0x14,17);Arrays.fill(post,0x26,0x2e,(byte)'1');post[0x132]=1;post[0x133]=2;post[0x134]=3;post[0x135]=1;post[0x136]=0;post[0x137]=4;post[0x138]=0;post[0x139]=5;
            card=new File(chip.getFilesDir(),"firmware/card.bin");if(card.isFile()){backup=Files.createTempFile(chip.getCacheDir().toPath(),"session-safety-card-",".bin").toFile();Files.copy(card.toPath(),backup.toPath(),StandardCopyOption.REPLACE_EXISTING);}
            receiptFile=new File(chip.getNoBackupFilesDir(),"security-processing-status.properties");if(receiptFile.isFile())previousReceipt=Files.readAllBytes(receiptFile.toPath());
            card.getParentFile().mkdirs();try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(SsaData.OFFSET);f.write(post);}
            Files.write(new File(run,"working-ssa.bin").toPath(),post);Files.write(new File(run,"screen.txt").toPath(),"Diagnostics".getBytes("UTF-8"));
            VehicleIdentity identity=new VehicleIdentity(vin,Instant.now().toString(),"synthetic",2004,"not_requested","","","","","");VehicleSession.save(new File(run,VehicleSession.FILE),identity);
            SecurityAccessStatus receipt=new SecurityAccessStatus(vin,run.getName(),Instant.now());receipt.processed("OpenSAAB","",Instant.now());receipt.imported(post,Instant.now());receipt.save(new File(chip.getNoBackupFilesDir(),"security-processing-status.properties"));
            String original=SsaCardImport.hash(card);boolean[] owned={true},firmware={true};int[] resumed={0};SecurityAccessView[] made={null};
            main(()->{made[0]=new SecurityAccessView(chip,false,()->run,()->owned[0],()->{},()->{},()->resumed[0]++);made[0].setFirmwareRunning(()->firmware[0]);});view=made[0];SecurityAccessView security=view;
            await(security,"Firmware running");main(()->{Button b=security.findViewWithTag("security-action");check(!b.isEnabled(),"Running Continue remained enabled");b.performClick();check(resumed[0]==0,"Running Continue restarted");firmware[0]=false;});
            await(security,"Waiting for cleanup");main(()->{check(!security.findViewWithTag("security-action").isEnabled(),"Unreleased owner offered Continue");owned[0]=false;});
            await(security,"Continue firmware");main(()->{((Button)security.findViewWithTag("security-action")).performClick();check(resumed[0]==1,"Stopped Continue has no callback");});
            check(original.equals(SsaCardImport.hash(card))&&!SecurityAccessView.workflowBusy(),"UI changed card or processed data");main(security::close);view=null;main(chip::finish);

            com.opensaab.tech2.MainActivity home=(com.opensaab.tech2.MainActivity)startActivitySync(new Intent(getTargetContext(),com.opensaab.tech2.MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));active=home;
            main(()->{Button next=home.getWindow().getDecorView().findViewWithTag("preview-continue");if(next!=null)next.performClick();});waitForIdleSync();
            String[] shortcuts={"Read engine codes — HS-CAN","ECU information","Read DTC","Clear DTC","Engine Data","Reset misfire and knock counters"};
            main(()->{try{field(home,"running").setBoolean(home,false);Dialog idle=home.showActions();for(String label:shortcuts)check(text(idle.getWindow().getDecorView(),label),"Idle shortcut missing: "+label);idle.dismiss();field(home,"running").setBoolean(home,true);Dialog running=home.showActions();for(String label:shortcuts)check(!text(running.getWindow().getDecorView(),label),"Active new-connection shortcut: "+label);check(text(running.getWindow().getDecorView(),"Get security access")&&text(running.getWindow().getDecorView(),"Saved DTC reports"),"Working common action hidden");running.dismiss();field(home,"running").setBoolean(home,false);}catch(Exception e){throw new RuntimeException(e);}});
            output.putString("stream","PASS: queued production VIN callback cannot create pending handoff or launch after idle Stop, foreground/background; cancelled startup gate blocks USB/child effects; idle security handoff preserves exact identity/generation; Chipsoft native Stop preserves loopback bridge and blocks keys; late-stop, forced and cleanup failures persist; final health failure overrides expected Stop; MDI carrier cannot hide native failure/force; running/closing/stopped imported UI callbacks and six active-menu guards. Synthetic only, no adapter/API/native launch or vehicle qualification.\n");
        }catch(Throwable error){code=0;StringWriter trace=new StringWriter();error.printStackTrace(new PrintWriter(trace));output.putString("stream","FAIL: "+trace+"\n");}
        finally{if(view!=null){SecurityAccessView v=view;main(v::close);}if(active!=null){Activity a=active;main(a::finish);}if(peer!=null)try{peer.close();}catch(IOException ignored){}
            try{if(card!=null){if(backup==null)card.delete();else{Files.move(backup.toPath(),card.toPath(),StandardCopyOption.REPLACE_EXISTING);}}if(receiptFile!=null){if(previousReceipt==null)receiptFile.delete();else Files.write(receiptFile.toPath(),previousReceipt);}}catch(IOException error){code=0;output.putString("stream","FAIL: synthetic fixture restoration failed: "+error);}
            finish(code,output);}
    }
}
