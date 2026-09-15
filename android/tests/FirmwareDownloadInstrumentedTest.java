// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.os.*;
import android.content.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.util.*;

/** Emulator-only: first-launch setup, offline retry, real ZIPs and honest missing-support state. */
public final class FirmwareDownloadInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean ok,String text){if(!ok)throw new AssertionError(text);}
    void main(Runnable action){Throwable[] e={null};runOnMainSync(()->{try{action.run();}catch(Throwable x){e[0]=x;}});if(e[0]!=null)throw new AssertionError(e[0]);}
    Button find(View v,String text){if(v instanceof Button&&((Button)v).getText().toString().equals(text))return (Button)v;if(v instanceof ViewGroup){ViewGroup g=(ViewGroup)v;for(int i=0;i<g.getChildCount();i++){Button b=find(g.getChildAt(i),text);if(b!=null)return b;}}return null;}
    boolean text(View v,String match){if(v instanceof TextView&&v.isShown()&&((TextView)v).getText().toString().contains(match))return true;if(v instanceof ViewGroup){ViewGroup g=(ViewGroup)v;for(int i=0;i<g.getChildCount();i++)if(text(g.getChildAt(i),match))return true;}return false;}
    Spinner spinner(View v){if(v instanceof Spinner)return (Spinner)v;if(v instanceof ViewGroup){ViewGroup g=(ViewGroup)v;for(int i=0;i<g.getChildCount();i++){Spinner s=spinner(g.getChildAt(i));if(s!=null)return s;}}return null;}
    void click(Activity a,String label){main(()->{Button b=find(a.getWindow().getDecorView(),label);check(b!=null&&b.isShown()&&b.isEnabled(),"Action unavailable: "+label);b.performClick();});waitForIdleSync();}
    void waitReady(Activity a){long end=SystemClock.elapsedRealtime()+60000;while(SystemClock.elapsedRealtime()<end){boolean[] ready={false};main(()->{Button b=find(a.getWindow().getDecorView(),"Download and set up");ready[0]=b!=null&&b.isEnabled();});if(ready[0])return;SystemClock.sleep(100);}throw new AssertionError("Setup did not finish within 60 seconds");}
    void visible(Activity a,String expected){main(()->check(text(a.getWindow().getDecorView(),expected),"Missing visible explanation: "+expected));}
    void shell(String command)throws Exception{try(ParcelFileDescriptor fd=getUiAutomation().executeShellCommand(command);InputStream in=new ParcelFileDescriptor.AutoCloseInputStream(fd)){byte[] b=new byte[4096];while(in.read(b)>0){}}}
    void shot(String name)throws Exception{SystemClock.sleep(180);android.graphics.Bitmap b=getUiAutomation().takeScreenshot();File dir=new File(getTargetContext().getCacheDir(),"onboarding-screenshots");dir.mkdirs();try(FileOutputStream out=new FileOutputStream(new File(dir,name+".png"))){b.compress(android.graphics.Bitmap.CompressFormat.PNG,100,out);}b.recycle();}
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;boolean networkOff=false;
        try{
            check(Build.FINGERPRINT.contains("generic")||Build.MODEL.contains("sdk"),"Test requires Android emulator");
            FirmwareStore store=new FirmwareStore(getTargetContext().getFilesDir());check(!new File(store.firmware,"card.bin").exists(),"Use a fresh emulator; active card already exists");
            shell("svc wifi disable");shell("svc data disable");networkOff=true;SystemClock.sleep(1000);
            ActivityMonitor monitor=addMonitor("com.opensaab.usb.FirmwareActivity",null,false);
            Activity home=startActivitySync(new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            activity=waitForMonitorWithTimeout(monitor,5000);removeMonitor(monitor);check(activity!=null,"Fresh home launch did not open setup");Activity a=activity;waitReady(a);
            visible(a,"Welcome to OpenSAAB");visible(a,"Connect to the internet");
            boolean bundled=BundledSupport.available(getTargetContext().getAssets());
            if(bundled){for(String n:BundledSupport.NAMES)FirmwareFiles.validate(n,new File(store.firmware,n));check(store.missing().equals("card.bin"),"Bundled preparation failed");}
            else {check(store.missingSupport().equals("eprom.bin, opsys.dwn, candi.bin"),"Absent support not recorded");visible(a,"not included in this app");}
            shot("01-welcome");
            click(a,"Get started");visible(a,"Download once.");
            Activity initial=a;main(()->check(((FirmwareCatalog.Entry)spinner(initial.getWindow().getDecorView()).getSelectedItem()).id.equals(FirmwareCatalog.ENTRIES[0].id),"Fresh default not English NAO"));shot("02-select-software");
            click(a,"Download and set up");waitReady(a);visible(a,"Check your internet connection");check(!new File(store.firmware,"card.bin").exists(),"Offline failure created active card");shot("03-offline-retry");
            shell("svc wifi enable");shell("svc data enable");networkOff=false;SystemClock.sleep(3000);
            for(int attempt=0;attempt<3;attempt++){
                click(a,"Try again");waitReady(a);
                if(new File(store.firmware,"card.bin").exists())break;
                visible(a,"Check your internet connection"); // Retry only network recovery, never hide install failures.
                SystemClock.sleep(3000);
            }
            FirmwareFiles.requireSha(new File(store.firmware,"card.bin"),FirmwareCatalog.ENTRIES[0].imageSha);
            if(!bundled){
                check(store.missingSupport().equals("eprom.bin, opsys.dwn, candi.bin"),"Card download secretly installed support");
                visible(a,"Import communication files");visible(a,"Still needed:");
                Activity supportNeeded=a;main(()->check(!text(supportNeeded.getWindow().getDecorView(),"You’re ready to connect"),"False readiness without support"));
                shot("04-support-needed");
                result.putString("stream","PASS: firmware-free first launch; no OEM firmware installed offline; English NAO default; offline failure and retry; actual Tech2Wiki ZIP hash verified, extracted and activated; three support files still explicitly required; no false readiness or USB access\n");
                main(home::finish);return;
            }
            check(store.missing().isEmpty(),"Download did not finish setup automatically");visible(a,"Setup complete");shot("04-ready");
            click(a,"Continue to OpenSAAB");
            activity=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.FirmwareActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));a=activity;waitReady(a);visible(a,"You’re ready to connect");
            click(a,"More options");click(a,"Choose a different version");Activity resumed=a;
            main(()->spinner(resumed.getWindow().getDecorView()).setSelection(1));click(a,"Download and set up");waitReady(a);
            FirmwareFiles.requireSha(new File(store.firmware,"card.bin"),FirmwareCatalog.ENTRIES[1].imageSha);
            File[] backups=new File(store.library,"backups").listFiles();check(backups!=null&&backups.length==1,"Version switch lost backup");FirmwareFiles.requireSha(new File(backups[0],"card.bin"),FirmwareCatalog.ENTRIES[0].imageSha);
            click(a,"More options");click(a,"Choose a different version");
            main(()->{spinner(resumed.getWindow().getDecorView()).setSelection(2);find(resumed.getWindow().getDecorView(),"Download and set up").performClick();find(resumed.getWindow().getDecorView(),"Cancel setup").performClick();});waitReady(a);visible(a,"Setup paused");
            FirmwareFiles.requireSha(new File(store.firmware,"card.bin"),FirmwareCatalog.ENTRIES[1].imageSha);
            check(store.missing().isEmpty(),"Cancellation changed a ready installation");click(a,"Continue to OpenSAAB");
            // A subsequent regular launch must stay on home, without launching an adapter.
            main(home::finish);waitForIdleSync();
            monitor=addMonitor("com.opensaab.usb.FirmwareActivity",null,false);
            Activity nextHome=startActivitySync(new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER).setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK));
            check(waitForMonitorWithTimeout(monitor,750)==null,"Completed setup repeated on launch");removeMonitor(monitor);main(nextHome::finish);main(home::finish);
            result.putString("stream","PASS: first-launch welcome, default English NAO, offline error/retry, actual HTTPS ZIP download and activation, bundled support installed offline, ready immediately after download, version switch with backup, cancellation, no repeated onboarding; no USB or vehicle\n");
        }catch(Throwable e){try{shot("failure");}catch(Exception ignored){}code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{try{if(networkOff){shell("svc wifi enable");shell("svc data enable");}}catch(Exception ignored){}if(activity!=null){Activity a=activity;main(a::finish);}finish(code,result);}
    }
}
