// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.os.Bundle;
import org.json.*;

/** No network or adapter access; validate untrusted release metadata. */
public final class AppUpdatesInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    JSONObject release(String tag,boolean preview)throws Exception{
        String base="https://github.com/djfremen/OpenSAAB-T2/releases/";
        return new JSONObject().put("tag_name",tag).put("prerelease",preview)
            .put("html_url",base+"tag/"+tag).put("assets",new JSONArray().put(new JSONObject()
                .put("name","OpenSAAB-T2-arm64-v8a.apk")
                .put("browser_download_url",base+"download/"+tag+"/OpenSAAB-T2-arm64-v8a.apk")));
    }
    JSONObject entry(boolean head,String version)throws Exception{
        String tag=(head?"headunit-v":"v")+version;
        return new JSONObject().put("package",head?"com.opensaab.tech2.headunit32":"com.opensaab.tech2").put("abi",head?"armeabi-v7a":"arm64-v8a")
            .put("version",version).put("version_code",ReleaseVersion.code(version.replace("-headunit.","-preview."))).put("bytes",100)
            .put("sha256",new String(new char[64]).replace('\0','a')).put("url","https://github.com/djfremen/OpenSAAB-T2/releases/download/"+tag+"/"+(head?"OpenSAAB-T2-headunit-armeabi-v7a.apk":"OpenSAAB-T2-arm64-v8a.apk"));
    }
    JSONObject catalog(JSONObject... entries)throws Exception{JSONArray list=new JSONArray();for(JSONObject e:entries)list.put(e);return new JSONObject().put("schema",1).put("releases",list);}
    void ui(Runnable r){runOnMainSync(r);}
    void reminderTests()throws Exception{
        android.content.SharedPreferences p=getTargetContext().getSharedPreferences("updates",0);p.edit().clear().putBoolean("automatic",false).commit();
        android.app.Activity a=startActivitySync(new android.content.Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK));
        final AppUpdates.Reminder[] reminder={null};final boolean[] idle={true};final java.util.concurrent.atomic.AtomicInteger requests=new java.util.concurrent.atomic.AtomicInteger();
        AppUpdates.Selection next=AppUpdates.catalog(catalog(entry(false,"0.2.0-preview.1")),"com.opensaab.tech2","arm64-v8a");
        try{
            p.edit().putBoolean("automatic",true).commit();
            ui(()->{reminder[0]=new AppUpdates.Reminder(a,()->idle[0],(pkg,abi)->{requests.incrementAndGet();return next;});reminder[0].resume();});
            long end=android.os.SystemClock.elapsedRealtime()+10000;
            while(!p.contains("catalog")&&android.os.SystemClock.elapsedRealtime()<end)android.os.SystemClock.sleep(25);
            final boolean[] shown={false};ui(()->{reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});
            check(shown[0]&&requests.get()==1,"daily fetch/new release banner");
            ui(()->{idle[0]=false;reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});check(!shown[0],"banner during session");
            ui(()->{idle[0]=true;reminder[0].refresh();reminder[0].offer();});waitForIdleSync();
            java.lang.reflect.Field f=AppUpdates.Reminder.class.getDeclaredField("dialog");f.setAccessible(true);android.app.AlertDialog dialog=(android.app.AlertDialog)f.get(reminder[0]);
            ui(()->dialog.getButton(android.content.DialogInterface.BUTTON_NEGATIVE).performClick());waitForIdleSync();
            check(p.getLong("snooze_until",0)>System.currentTimeMillis(),"snooze not persisted");
            ui(()->{reminder[0].pause();reminder[0].resume();reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});check(!shown[0]&&requests.get()==1,"snooze/resume recheck");
            p.edit().putLong("snooze_until",0).putBoolean("automatic",false).commit();ui(()->{reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});check(!shown[0],"disabled reminder shown");
            p.edit().putBoolean("automatic",true).putString("catalog",catalog(entry(false,AppUpdates.installedVersion(a))).toString()).putLong("catalog_utc",System.currentTimeMillis()).commit();
            ui(()->{reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});check(!shown[0],"already-current offer");
            p.edit().putString("catalog",catalog(next.entry).toString()).putLong("catalog_utc",System.currentTimeMillis()-8*AppUpdates.DAY).commit();
            ui(()->{reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});check(!shown[0],"expired cached offer");
            ui(()->reminder[0].pause());p.edit().clear().putBoolean("automatic",true).commit();requests.set(0);
            FirmwareGate.Lease lease=FirmwareGate.use();
            ui(()->{reminder[0]=new AppUpdates.Reminder(a,()->true,(pkg,abi)->{requests.incrementAndGet();throw new java.io.IOException("synthetic offline");});reminder[0].resume();reminder[0].refresh();});waitForIdleSync();check(requests.get()==0,"fetch during firmware lease");lease.close();
            ui(()->reminder[0].refresh());end=android.os.SystemClock.elapsedRealtime()+5000;while(requests.get()==0&&android.os.SystemClock.elapsedRealtime()<end)android.os.SystemClock.sleep(25);
            waitForIdleSync();ui(()->{reminder[0].refresh();shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE;});check(requests.get()==1&&!shown[0],"offline response retries/prompts");
            // An in-flight result must not present a reminder after the app goes into the background.
            ui(()->reminder[0].pause());p.edit().clear().putBoolean("automatic",true).commit();java.util.concurrent.CountDownLatch entered=new java.util.concurrent.CountDownLatch(1),release=new java.util.concurrent.CountDownLatch(1);
            ui(()->{reminder[0]=new AppUpdates.Reminder(a,()->true,(pkg,abi)->{entered.countDown();release.await(5,java.util.concurrent.TimeUnit.SECONDS);return next;});reminder[0].resume();});
            check(entered.await(5,java.util.concurrent.TimeUnit.SECONDS),"fetch not started");ui(()->reminder[0].pause());release.countDown();android.os.SystemClock.sleep(100);ui(()->shown[0]=reminder[0].view().getVisibility()==android.view.View.VISIBLE);check(!shown[0],"background response surfaced UI");
        }finally{ui(()->{if(reminder[0]!=null)reminder[0].pause();a.finish();});p.edit().clear().putBoolean("automatic",false).commit();}
    }
    public void onStart(){Bundle result=new Bundle();int code=-1;
        try{
            check(AppUpdates.select(new JSONArray(),true).code==-1,"empty release list");
            JSONObject stable=release("v0.1.0",false),preview=release("v0.2.0-preview.1",true);
            JSONArray list=new JSONArray().put(preview).put(stable);
            check(AppUpdates.select(list,false).tag.equals("v0.1.0"),"stable filter");
            check(AppUpdates.select(list,true).tag.equals("v0.2.0-preview.1"),"preview selection");
            check(AppUpdates.select(new JSONArray().put(release("v2.0.0",false).put("draft",true)).put(stable),true).tag.equals("v0.1.0"),"draft ignored");
            JSONObject wrongHost=release("v2.0.0",false).put("html_url","https://example.com/download");
            check(AppUpdates.select(new JSONArray().put(wrongHost),true).code==-1,"release URL rejected");
            JSONObject wrongAsset=release("v2.0.0",false);
            wrongAsset.getJSONArray("assets").getJSONObject(0).put("browser_download_url","https://example.com/evil.apk");
            check(AppUpdates.select(new JSONArray().put(wrongAsset),true).code==-1,"asset URL rejected");
            check(AppUpdates.select(new JSONArray().put(release("v3.0.0",false).put("assets",new JSONArray())),true).code==-1,"no APK means no update");
            check(AppUpdates.select(new JSONArray().put(release("v1.2.3/evil",false)),true).code==-1,"invalid tag rejected");
            check(AppUpdates.select(new JSONArray().put(release("v0.1.0-preview.99",true)).put(stable),true).tag.equals("v0.1.0"),"stable supersedes same-version previews");
            JSONObject both=catalog(entry(false,"0.1.0-preview.28"),entry(true,"0.1.0-headunit.17"));
            check(AppUpdates.catalog(both,"com.opensaab.tech2","arm64-v8a").code==100028,"ARM64 catalog");
            check(AppUpdates.catalog(both,"com.opensaab.tech2.headunit32","armeabi-v7a").code==100017,"ARM32 catalog");
            check(AppUpdates.catalog(both,"com.opensaab.tech2","armeabi-v7a").code==-1,"cross architecture");
            check(AppUpdates.catalog(catalog(entry(false,"0.1.0-preview.28").put("url","https://example.com/app.apk")),"com.opensaab.tech2","arm64-v8a").code==-1,"foreign catalog URL");
            check(AppUpdates.catalog(catalog(entry(false,"0.1.0-preview.28").put("version_code",100030)),"com.opensaab.tech2","arm64-v8a").code==-1,"version mismatch");
            check(AppUpdates.due(100,0)&&!AppUpdates.due(1000,100)&&AppUpdates.due(AppUpdates.DAY+100,100)&&AppUpdates.due(50,100),"daily schedule/clock rollback");
            reminderTests();
            result.putString("stream","PASS: channel/package/ABI isolation, strict catalog URLs/version codes, daily rate limit, snooze, opt-out, foreground/session gating and in-flight background result; fake catalog only, no network or USB\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finish(code,result);
    }
}
