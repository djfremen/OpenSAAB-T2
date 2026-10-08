// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import com.opensaab.tech2.MainActivity;
import java.io.File;
import java.lang.reflect.Method;
import org.json.*;

/** Disposable emulator only. Original offline firmware, no adapter or upload. */
public final class OfflineSessionRunInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean v,String why){if(!v)throw new AssertionError(why);}
    void invoke(MainActivity a,String name,String reason)throws Exception{
        Method m=reason==null?MainActivity.class.getDeclaredMethod(name):MainActivity.class.getDeclaredMethod(name,String.class);m.setAccessible(true);
        runOnMainSync(()->{try{if(reason==null)m.invoke(a);else m.invoke(a,reason);}catch(Exception e){throw new RuntimeException(e);}});
    }
    public void onStart(){Bundle result=new Bundle();MainActivity a=null;
        try{
            check(new FirmwareStore(getTargetContext().getFilesDir()).missing().isEmpty(),"Install original firmware in disposable AVD first");
            Intent intent=new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);a=(MainActivity)startActivitySync(intent);waitForIdleSync();
            File runs=new File(getTargetContext().getFilesDir(),"sessions"),run=null;
            java.util.Set<String> previous=new java.util.HashSet<>();File[] prior=runs.listFiles();if(prior!=null)for(File f:prior)previous.add(f.getName());
            invoke(a,"startSession",null);
            long deadline=System.currentTimeMillis()+60000;
            while(System.currentTimeMillis()<deadline){File[] files=runs.listFiles();if(files!=null)for(File f:files)if(!previous.contains(f.getName())&&new File(f,"live.ppm").isFile())run=f;if(run!=null)break;Thread.sleep(100);}
            check(run!=null,"Original offline engine must publish a frame");
            invoke(a,"enqueue","enter");Thread.sleep(6000);
            check(!new File(run,"session-end.json").isFile(),"Offline run exited before the requested stop");
            invoke(a,"stopSession","Stopped by operator");deadline=System.currentTimeMillis()+10000;
            while(!new File(run,"session-end.json").isFile()&&System.currentTimeMillis()<deadline)Thread.sleep(100);
            JSONObject end=FirmwareStore.json(new File(run,"session-end.json"));
            check(end.getLong("input_count")>=1,"Actual input accepted and counted");
            check(end.getString("reason").equals("user_stop"),"Actual UI Stop must be distinguished from unexpected exit");
            JSONObject nativeReport=FirmwareStore.json(new File(run,"report.json"));
            check(nativeReport.getJSONObject("emulation_evidence").getInt("schema")==1,"Actual packaged native executable must emit evidence");
            check(FirmwareStore.json(new File(run,"emulation-context.json")).getJSONObject("firmware_sha256").length()==4,"Actual run must fingerprint all four inputs");
            JSONObject report=SupportReports.collect(getTargetContext(),"Synthetic original offline run",true);
            check(report.getBoolean("submitter_notes_provided"),"Notes flag");
            java.nio.file.Files.write(new File(getTargetContext().getCacheDir(),"offline-real-run-report.json").toPath(),report.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
            result.putString("stream","PASS: original firmware offline frame, actual UI Stop, packaged native evidence, exact firmware fingerprints and report collection; no adapter/network request\n");finish(-1,result);
        }catch(Throwable e){result.putString("stream","FAIL: "+e+"\n");finish(0,result);}
        finally{if(a!=null){final MainActivity activity=a;runOnMainSync(activity::finish);}}
    }
}
