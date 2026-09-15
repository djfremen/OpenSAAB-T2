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
            result.putString("stream","PASS: release channel/version ordering, drafts, missing APK, invalid tag and foreign URLs; no network or USB\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finish(code,result);
    }
}
