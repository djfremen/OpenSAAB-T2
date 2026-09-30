// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.net.Uri;
import android.widget.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import org.json.*;

/** Optional public catalog checks. No VIN, adapter data or silent installs. */
public final class AppUpdates {
    private static final String REPO="https://github.com/djfremen/OpenSAAB-T2";
    private static final java.util.concurrent.atomic.AtomicBoolean checking=new java.util.concurrent.atomic.AtomicBoolean();
    public static void show(Activity a){
        if(FirmwareGate.sessionActive() || SecurityAccessView.workflowBusy()){
            Toast.makeText(a,"Finish the vehicle session before checking for updates.",Toast.LENGTH_LONG).show();return;
        }
        CheckBox preview=new CheckBox(a);preview.setText("Check once daily when OpenSAAB is idle");
        preview.setChecked(a.getSharedPreferences("updates",0).getBoolean("automatic",true));
        preview.setOnCheckedChangeListener((button,checked)->a.getSharedPreferences("updates",0).edit().putBoolean("automatic",checked).apply());
        String installed;
        try{installed=a.getPackageManager().getPackageInfo(a.getPackageName(),0).versionName;}catch(Exception e){installed="unknown";}
        new AlertDialog.Builder(a).setTitle("OpenSAAB updates · "+installed)
            .setMessage("OpenSAAB checks its public release catalog once daily while idle. Only your installed architecture and package are matched locally; no vehicle data or device identifiers are sent. You can turn automatic checks off below. Offline use continues normally. Updates are optional; install over the existing app to keep your software and settings.")
            .setView(preview).setNegativeButton("Not now",null)
            .setPositiveButton("Check now",(d,w)->{
                check(a,true);
            }).show();
    }
    private static void check(Activity a,boolean previews){
        if(FirmwareGate.sessionActive() || SecurityAccessView.workflowBusy() || !checking.compareAndSet(false,true))return;
        a.getSharedPreferences("updates",0).edit().putLong("last_check",System.currentTimeMillis()).apply();
        Toast.makeText(a,"Checking the OpenSAAB release catalog…",Toast.LENGTH_SHORT).show();
        new Thread(()->{
            String message,releaseUrl=null;
            try{
                Selection selection=fetch(a.getPackageName(),AppBuildProfile.abi(a));
                int best=selection.code;String name=selection.tag;releaseUrl=selection.url;
                int installed=a.getPackageManager().getPackageInfo(a.getPackageName(),0).versionCode;
                if(best<0){message="No matching public APK is available yet. Your installed app is unchanged.";releaseUrl=null;}
                else if(best<=installed){message="You have the latest available version for this update channel.";releaseUrl=null;}
                else message="Available: "+name+"\n\nRead the release notes and download the signed APK from the official release page. Install it over your existing app when Android offers Update. If Android reports a signature conflict, keep your current app and follow the migration guide.";
            }catch(Exception e){message="Couldn’t check for updates. You may be offline or GitHub may be unavailable. Try again later; your installed app and files are unchanged.";releaseUrl=null;}
            finally{checking.set(false);}
            final String text=message,url=releaseUrl;
            a.runOnUiThread(()->{
                if(a.isFinishing()||a.isDestroyed()||!a.hasWindowFocus()||FirmwareGate.sessionActive()||SecurityAccessView.workflowBusy())return;
                AlertDialog.Builder dialog=new AlertDialog.Builder(a).setTitle("OpenSAAB updates").setMessage(text).setNegativeButton("Close",null);
                if(url!=null)dialog.setPositiveButton("View release",(d,w)->{
                    if(FirmwareGate.sessionActive()||SecurityAccessView.workflowBusy())return;
                    try{a.startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(url)));}
                    catch(ActivityNotFoundException e){Toast.makeText(a,"No browser installed. Visit www.opensaab.com from another device.",Toast.LENGTH_LONG).show();}
                });dialog.show();
            });
        },"opensaab-update-check").start();
    }
    static final class Selection {
        final int code;final String tag,url;final JSONObject entry;
        Selection(int code,String tag,String url){this(code,tag,url,null);}
        Selection(int code,String tag,String url,JSONObject entry){this.code=code;this.tag=tag;this.url=url;this.entry=entry;}
    }
    static Selection select(JSONArray releases,boolean previews)throws JSONException{
        int best=-1;String name="",releaseUrl=null;
                for(int i=0;i<releases.length();i++){
                    JSONObject r=releases.getJSONObject(i);if(r.optBoolean("draft")||(!previews&&r.optBoolean("prerelease")))continue;
                    String tag=r.optString("tag_name"),url=r.optString("html_url");int code=ReleaseVersion.code(tag);
                    if(code<=best || !url.equals(REPO+"/releases/tag/"+tag))continue;
                    boolean apk=false;JSONArray assets=r.optJSONArray("assets");
                    if(assets!=null)for(int j=0;j<assets.length();j++){
                        JSONObject asset=assets.getJSONObject(j);
                        if("OpenSAAB-T2-arm64-v8a.apk".equals(asset.optString("name"))
                            && asset.optString("browser_download_url").equals(REPO+"/releases/download/"+tag+"/OpenSAAB-T2-arm64-v8a.apk"))apk=true;
                    }
                    if(apk){best=code;name=tag;releaseUrl=url;}
                }
        return new Selection(best,name,releaseUrl);
    }
    static final String CATALOG="https://www.opensaab.com/static/t2/releases.json";
    static final long DAY=24*60*60*1000L;
    static boolean due(long now,long previous){return previous<=0||now<previous||now-previous>=DAY;}
    public static String installedVersion(Context c){return AppBuildProfile.installedVersion(c);}
    public static String identity(Context c){return "OpenSAAB "+installedVersion(c)+" · "+(AppBuildProfile.isHeadunit32(c)?"32-bit":"64-bit")+"\nAndroid "+android.os.Build.VERSION.RELEASE+" · "+android.os.Build.MODEL;}
    static Selection catalog(JSONObject catalog,String pkg,String abi)throws JSONException{
        if(catalog.optInt("schema")!=1)return new Selection(-1,"",null);
        boolean head="com.opensaab.tech2.headunit32".equals(pkg);
        if(!(head&&"armeabi-v7a".equals(abi))&&!("com.opensaab.tech2".equals(pkg)&&"arm64-v8a".equals(abi)))return new Selection(-1,"",null);
        JSONArray list=catalog.getJSONArray("releases");Selection best=new Selection(-1,"",null);
        for(int i=0;i<list.length();i++){
            JSONObject v=list.getJSONObject(i);if(!pkg.equals(v.optString("package"))||!abi.equals(v.optString("abi")))continue;
            String version=v.optString("version"),tag,asset;int code;
            if(head){
                if(!version.matches("[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}-headunit\\.[1-9][0-9]?"))continue;
                code=ReleaseVersion.code(version.replace("-headunit.","-preview."));tag="headunit-v"+version;asset="OpenSAAB-T2-headunit-armeabi-v7a.apk";
            }else{code=ReleaseVersion.code(version);tag="v"+version;asset="OpenSAAB-T2-arm64-v8a.apk";}
            if(code<=best.code||code!=v.optInt("version_code",-1)||v.optLong("bytes",0)<=0||v.optLong("bytes",0)>100000000
                ||!v.optString("sha256").matches("[a-f0-9]{64}")||!v.optString("url").equals(REPO+"/releases/download/"+tag+"/"+asset))continue;
            best=new Selection(code,version,REPO+"/releases/tag/"+tag,new JSONObject(v.toString()));
        }
        return best;
    }
    interface Fetch { Selection get(String pkg,String abi)throws Exception; }
    static Selection fetch(String pkg,String abi)throws Exception{
        HttpURLConnection c=(HttpURLConnection)new URL(CATALOG).openConnection();
        try{
            c.setConnectTimeout(5000);c.setReadTimeout(7000);c.setInstanceFollowRedirects(false);
            c.setRequestProperty("Accept","application/json");c.setRequestProperty("User-Agent","OpenSAAB-update-check");
            if(c.getResponseCode()!=200)throw new IOException("Catalog unavailable");
            ByteArrayOutputStream out=new ByteArrayOutputStream();long deadline=System.nanoTime()+15000000000L;
            try(InputStream in=c.getInputStream()){byte[] b=new byte[4096];int n;while((n=in.read(b))!=-1){if(out.size()+n>65536||System.nanoTime()>deadline)throw new IOException("Catalog exceeded limits");out.write(b,0,n);}}
            return catalog(new JSONObject(new String(out.toByteArray(),StandardCharsets.UTF_8)),pkg,abi);
        }finally{c.disconnect();}
    }
    /** Idle-only nonmodal reminder; never downloads, installs or stops a session. */
    public static final class Reminder {
        private final Activity activity;private final Button banner;private final java.util.function.BooleanSupplier idle;
        private final Fetch fetch;private final android.os.Handler handler=new android.os.Handler();
        private boolean resumed;private AlertDialog dialog;
        private final Runnable tick=new Runnable(){public void run(){refresh();if(resumed)handler.postDelayed(this,60000);}};
        public Reminder(Activity a,java.util.function.BooleanSupplier idle){this(a,idle,AppUpdates::fetch);}
        Reminder(Activity a,java.util.function.BooleanSupplier idle,Fetch fetch){activity=a;this.idle=idle;this.fetch=fetch;banner=new Button(a);banner.setTag("update-reminder");SessionStyle.button(banner,false);banner.setVisibility(android.view.View.GONE);banner.setTextSize(12);banner.setOnClickListener(v->offer());}
        public Button view(){return banner;}
        public void resume(){resumed=true;handler.removeCallbacks(tick);handler.post(tick);}
        public void pause(){resumed=false;handler.removeCallbacks(tick);banner.setVisibility(android.view.View.GONE);if(dialog!=null)dialog.dismiss();}
        private boolean safe(){return resumed&&!activity.isFinishing()&&!activity.isDestroyed()&&idle.getAsBoolean()&&!FirmwareGate.sessionActive()&&!SecurityAccessView.workflowBusy();}
        private int installed(){try{return activity.getPackageManager().getPackageInfo(activity.getPackageName(),0).versionCode;}catch(Exception e){return Integer.MAX_VALUE;}}
        private android.content.SharedPreferences prefs(){return activity.getSharedPreferences("updates",0);}
        private Selection saved(){try{long stamp=prefs().getLong("catalog_utc",0),now=System.currentTimeMillis();if(stamp<=0||now<stamp||now-stamp>7*DAY)return null;return catalog(new JSONObject(prefs().getString("catalog","{}")),activity.getPackageName(),AppBuildProfile.abi(activity));}catch(Exception e){return null;}}
        public void refresh(){
            long now=System.currentTimeMillis();Selection selected=saved();
            boolean available=safe()&&prefs().getBoolean("automatic",true)&&(now>=prefs().getLong("snooze_until",0)||prefs().getLong("snooze_until",0)>now+DAY)&&selected!=null&&selected.code>installed();
            banner.setVisibility(available?android.view.View.VISIBLE:android.view.View.GONE);
            if(available)banner.setText("Update available: "+selected.tag+" · View update");
            if(!safe()||!prefs().getBoolean("automatic",true)||!due(now,prefs().getLong("last_check",0)))return;
            // Isolated test packages cannot start automatic network checks.
            if(!activity.getPackageName().equals("com.opensaab.tech2")&&!activity.getPackageName().equals("com.opensaab.tech2.headunit32"))return;
            if(!checking.compareAndSet(false,true))return;
            prefs().edit().putLong("last_check",now).apply();
            new Thread(()->{try{
                Selection next=fetch.get(activity.getPackageName(),AppBuildProfile.abi(activity));
                if(next.code>0&&next.entry!=null){
                    prefs().edit().putString("catalog",new JSONObject().put("schema",1).put("releases",new JSONArray().put(next.entry)).toString()).putLong("catalog_utc",System.currentTimeMillis()).apply();
                }else prefs().edit().remove("catalog").remove("catalog_utc").apply();
            }catch(Exception offline){/* Quiet: keep offline diagnostics usable. */}finally{checking.set(false);activity.runOnUiThread(()->{if(resumed)refresh();});}},"opensaab-idle-update").start();
        }
        void offer(){Selection next=saved();if(!safe()||next==null||next.code<=installed())return;
            dialog=new AlertDialog.Builder(activity).setTitle("OpenSAAB update available").setMessage("Installed: "+installedVersion(activity)+"\nAvailable: "+next.tag+"\n\nRead the release notes, then install over your existing app to keep software and settings. No uninstall is needed. Updates are optional.")
                .setNegativeButton("Remind me tomorrow",(d,w)->{prefs().edit().putLong("snooze_until",System.currentTimeMillis()+DAY).apply();refresh();})
                .setNeutralButton("Close",null).setPositiveButton("View update",(d,w)->{if(!safe())return;try{activity.startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(next.url)));}catch(ActivityNotFoundException e){Toast.makeText(activity,"Visit www.opensaab.com from a browser to update",Toast.LENGTH_LONG).show();}}).create();dialog.show();
        }
    }
    private AppUpdates(){}
}
