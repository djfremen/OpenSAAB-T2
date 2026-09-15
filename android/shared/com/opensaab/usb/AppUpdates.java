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

/** User-requested public release checks. No VIN, adapter data or silent installs. */
public final class AppUpdates {
    private static final String REPO="https://github.com/djfremen/OpenSAAB-T2";
    private static final String API="https://api.github.com/repos/djfremen/OpenSAAB-T2/releases?per_page=20";
    private static final java.util.concurrent.atomic.AtomicBoolean checking=new java.util.concurrent.atomic.AtomicBoolean();
    public static void show(Activity a){
        if(AppBuildProfile.isHeadunit32(a)){
            new AlertDialog.Builder(a).setTitle("32-bit head-unit development build")
                .setMessage("This is the experimental 32-bit head-unit channel. ARM64 phone releases cannot update it. Download only separately labeled head-unit releases from the project. Automatic head-unit update discovery is not enabled yet.")
                .setPositiveButton("OK",null).show();return;
        }
        if(FirmwareGate.sessionActive() || SecurityAccessView.workflowBusy()){
            Toast.makeText(a,"Finish the vehicle session before checking for updates.",Toast.LENGTH_LONG).show();return;
        }
        CheckBox preview=new CheckBox(a);preview.setText("Include community preview releases");
        preview.setChecked(a.getSharedPreferences("updates",0).getBoolean("preview",true));
        String installed;
        try{installed=a.getPackageManager().getPackageInfo(a.getPackageName(),0).versionName;}catch(Exception e){installed="unknown";}
        new AlertDialog.Builder(a).setTitle("OpenSAAB updates · "+installed)
            .setMessage("Checking needs internet. Your installed app and diagnostic software keep working offline. Updates are optional; Android will ask before installing. No vehicle data is sent to GitHub.")
            .setView(preview).setNegativeButton("Not now",null)
            .setPositiveButton("Check now",(d,w)->{
                a.getSharedPreferences("updates",0).edit().putBoolean("preview",preview.isChecked()).apply();check(a,preview.isChecked());
            }).show();
    }
    private static void check(Activity a,boolean previews){
        if(FirmwareGate.sessionActive() || SecurityAccessView.workflowBusy() || !checking.compareAndSet(false,true))return;
        Toast.makeText(a,"Checking official GitHub releases…",Toast.LENGTH_SHORT).show();
        new Thread(()->{
            String message,releaseUrl=null;HttpURLConnection c=null;
            try{
                c=(HttpURLConnection)new URL(API).openConnection();c.setConnectTimeout(5000);c.setReadTimeout(7000);c.setInstanceFollowRedirects(false);
                c.setRequestProperty("Accept","application/vnd.github+json");c.setRequestProperty("User-Agent","OpenSAAB-T2-update-check");
                if(c.getResponseCode()!=200)throw new IOException("Release server unavailable");
                ByteArrayOutputStream out=new ByteArrayOutputStream();long deadline=System.nanoTime()+15000000000L;
                try(InputStream in=c.getInputStream()){byte[] b=new byte[4096];int n;while((n=in.read(b))!=-1){
                    if(out.size()+n>524288 || System.nanoTime()>deadline)throw new IOException("Release response exceeded limits");out.write(b,0,n);
                }}
                JSONArray releases=new JSONArray(new String(out.toByteArray(),StandardCharsets.UTF_8));
                Selection selection=select(releases,previews);
                int best=selection.code;String name=selection.tag;releaseUrl=selection.url;
                int installed=a.getPackageManager().getPackageInfo(a.getPackageName(),0).versionCode;
                if(best<0){message="No matching public APK is available yet. Your installed app is unchanged.";releaseUrl=null;}
                else if(best<=installed){message="You have the latest available version for this update channel.";releaseUrl=null;}
                else message="Available: "+name+"\n\nRead the release notes and download the signed APK from the official release page. Install it over your existing app when Android offers Update. If Android reports a signature conflict, keep your current app and follow the migration guide.";
            }catch(Exception e){message="Couldn’t check for updates. You may be offline or GitHub may be unavailable. Try again later; your installed app and files are unchanged.";releaseUrl=null;}
            finally{if(c!=null)c.disconnect();checking.set(false);}
            final String text=message,url=releaseUrl;
            a.runOnUiThread(()->{
                if(a.isFinishing()||a.isDestroyed()||FirmwareGate.sessionActive()||SecurityAccessView.workflowBusy())return;
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
        final int code;final String tag,url;
        Selection(int code,String tag,String url){this.code=code;this.tag=tag;this.url=url;}
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
    private AppUpdates(){}
}
