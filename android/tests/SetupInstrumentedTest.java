// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;
import android.app.*;
import android.content.*;
import android.os.Bundle;
import android.net.Uri;
import java.io.*;
import java.util.*;
import org.json.*;
/** Disposable-emulator test. Downloads official APKs but never installs/replaces an emulator app. */
public final class SetupInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    static void require(boolean b,String m){if(!b)throw new AssertionError(m);}
    interface Work{void run()throws Exception;}
    static void rejects(Work w)throws Exception{boolean failed=false;try{w.run();}catch(IOException e){failed=true;}require(failed,"Expected rejection");}
    public void onStart(){Bundle out=new Bundle();try{
        require("armeabi-v7a".equals(SetupRelease.preferred(27,new String[]{"armeabi-v7a"},false,false)),"ARM32");
        require("arm64-v8a".equals(SetupRelease.preferred(29,new String[]{"arm64-v8a","armeabi-v7a"},false,false)),"ARM64");
        require("armeabi-v7a".equals(SetupRelease.preferred(29,new String[]{"arm64-v8a","armeabi-v7a"},true,false)),"Preserve installed ARM32");
        require("".equals(SetupRelease.preferred(25,new String[]{"arm64-v8a"},false,false)),"Old Android");
        require("".equals(SetupRelease.preferred(29,new String[]{"x86_64"},false,false)),"Unsupported ABI");
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();try(InputStream in=getContext().getAssets().open("releases.json")){byte[] buf=new byte[4096];int n;while((n=in.read(buf))!=-1)bytes.write(buf,0,n);}
        String catalog=bytes.toString("UTF-8");List<SetupRelease> releases=SetupRelease.parse(catalog);require(releases.size()==2,"Both independent channels");
        JSONObject changed=new JSONObject(catalog);changed.getJSONArray("releases").getJSONObject(0).put("url","http://example.com/malware.apk");final String wrongUrl=changed.toString();rejects(()->SetupRelease.parse(wrongUrl));
        changed=new JSONObject(catalog);changed.getJSONArray("releases").getJSONObject(0).put("package","com.opensaab.tech2.headunit32");final String wrongPackage=changed.toString();rejects(()->SetupRelease.parse(wrongPackage));
        JSONObject duplicate=new JSONObject(catalog);duplicate.getJSONArray("releases").put(1,duplicate.getJSONArray("releases").getJSONObject(0));rejects(()->SetupRelease.parse(duplicate.toString()));
        for(SetupRelease r:releases){
            try{File apk=r.download(getTargetContext(),p->{});r.verify(getTargetContext(),apk);
                Uri u=Uri.parse("content://com.opensaab.checker.installers/"+apk.getName());
                try(InputStream in=getTargetContext().getContentResolver().openInputStream(u)){require(in.read()=='P',"Provider reads APK");}
                rejects(()->getTargetContext().getContentResolver().openOutputStream(u));
                try(RandomAccessFile f=new RandomAccessFile(apk,"rw")){f.seek(100);f.write(0);}
                rejects(()->r.verify(getTargetContext(),apk));apk.delete();
            }catch(IOException e){
                // Existing debug-signed ARM64 emulator must be preserved, not uninstalled for this test.
                require(r.abi.equals("arm64-v8a")&&e.getMessage().startsWith("Installed app uses a different signing key"),"Unexpected download failure: "+e.getMessage());
            }
        }
        rejects(()->getTargetContext().getContentResolver().openInputStream(Uri.parse("content://com.opensaab.checker.installers/..%2Fprivate.apk")));
        out.putString("stream","PASS: ABI/API routing, installed-channel preservation, malformed catalog rejection, live signed downloads, checksum/signature/package/ABI verification, debug-key conflict preserves installed app, provider read-only and traversal denial; no APK installation");finish(Activity.RESULT_OK,out);
    }catch(Throwable e){out.putString("stream","FAIL: "+e);finish(Activity.RESULT_CANCELED,out);}}
}
