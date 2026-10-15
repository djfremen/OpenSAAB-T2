// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.Intent;
import android.os.Bundle;
import android.widget.TextView;
import android.widget.CheckBox;
import android.view.*;
import java.io.*;
import java.nio.file.*;

/** Synthetic UI/credential tests only; never starts USB, firmware or a network request. */
public final class StartupFlowInstrumentedTest extends Instrumentation {
    void check(boolean v,String why){if(!v)throw new AssertionError(why);}
    void main(Runnable r){Throwable[] e={null};runOnMainSync(()->{try{r.run();}catch(Throwable t){e[0]=t;}});if(e[0]!=null)throw new AssertionError(e[0]);}
    boolean findText(View v,String text){if(v instanceof TextView&&((TextView)v).getText().toString().contains(text))return true;if(v instanceof ViewGroup)for(int i=0;i<((ViewGroup)v).getChildCount();i++)if(findText(((ViewGroup)v).getChildAt(i),text))return true;return false;}
    CheckBox findCheck(View v){if(v instanceof CheckBox)return (CheckBox)v;if(v instanceof ViewGroup)for(int i=0;i<((ViewGroup)v).getChildCount();i++){CheckBox c=findCheck(((ViewGroup)v).getChildAt(i));if(c!=null)return c;}return null;}
    public void onCreate(Bundle b){super.onCreate(b);start();}
    public void onStart(){
        Bundle out=new Bundle();int code=-1;ChipsoftUsbActivity a=null;File credential=null;byte[] backup=null;
        try{
            getTargetContext().getSharedPreferences("adapter_settings",0).edit().remove("online_vehicle_details").commit();
            a=(ChipsoftUsbActivity)startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final ChipsoftUsbActivity activity=a;waitForIdleSync();
            main(()->{
                activity.requestVehicleStart();
                android.view.View prompt=activity.vehicleStartPrompt.getWindow().getDecorView();
                check(findText(prompt,"ACC/accessory is not enough"),"Ignition instructions missing");
                check(findText(prompt,"VIN is sent to OpenSAAB"),"Lookup disclosure missing");
                CheckBox choice=findCheck(prompt);check(choice!=null&&!choice.isChecked(),"Online lookup must be opt-in");
                check(!activity.running.get()&&activity.nativeDirectory==null,"Prompt started a vehicle operation");
                activity.vehicleStartPrompt.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
                check(!activity.running.get()&&activity.pendingVehicleStart==null,"Cancel must not queue firmware");
                VehicleIdentity v=new VehicleIdentity("YS3FD49Y041000000","2026-09-16T00:00:00Z","synthetic",2004,"available","9440","B207R","Silver Metallic","","");
                activity.vehicle=v;activity.showVehicle(false);
                check(activity.vinSummary.getText().toString().contains("B207R"),"Vehicle details absent");
                activity.vehicle=VehicleSession.unavailable(v,"not_requested");activity.showVehicle(false);
                check(activity.vinSummary.getText().toString().contains("Online details off"),"Offline choice mislabeled as failure");
                activity.vehicle=VehicleSession.unavailable(v,"pending");activity.showVehicle(false);
                check(activity.vinSummary.getText().toString().contains("diagnostics can continue"),"Pending lookup blocks diagnostics");
            });
            credential=new File(a.getNoBackupFilesDir(),"security-authorization.enc");if(credential.exists())backup=Files.readAllBytes(credential.toPath());
            SecurityAuthorization.save(a,"OpenSAAB");
            check(SecurityAuthorization.bearer(a).equals("OpenSAAB"),"Saved password changed case");
            check(!new String(Files.readAllBytes(credential.toPath()),"UTF-8").contains("OpenSAAB"),"Password stored unencrypted");
            for(String wrong:new String[]{"","opensaab","OPENSAAB","SamplePassword","OpenSAAB ",null}){
                boolean rejected=false;try{SecurityAuthorization.save(a,wrong);}catch(IllegalArgumentException expected){rejected=true;}check(rejected,"Non-matching password accepted: "+wrong);
            }
            SecurityAuthorization.forget(a);check(!SecurityAuthorization.available(a),"Rejected password cannot be cleared");
            main(()->SecurityAuthorization.show(activity));waitForIdleSync();
            sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);waitForIdleSync();
            check(!SecurityAuthorization.available(a),"Cancelled password prompt authorized processing");
            // Retained credentials from an older/invalid installation must be
            // decrypted and revalidated, even with a valid AndroidKeyStore key.
            java.lang.reflect.Method key=SecurityAuthorization.class.getDeclaredMethod("key");key.setAccessible(true);
            javax.crypto.Cipher cipher=javax.crypto.Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(javax.crypto.Cipher.ENCRYPT_MODE,(javax.crypto.SecretKey)key.invoke(null));
            byte[] encrypted=cipher.doFinal("opensaab".getBytes("US-ASCII"));
            byte[] remembered=new byte[12+encrypted.length];System.arraycopy(cipher.getIV(),0,remembered,0,12);System.arraycopy(encrypted,0,remembered,12,encrypted.length);
            Files.write(credential.toPath(),remembered);
            check(!SecurityAuthorization.available(a)&&!credential.exists(),"Invalid remembered password bypassed validation");
            java.lang.reflect.Method post=SecurityAccessView.class.getDeclaredMethod("postSecurityData",String.class,byte[].class);post.setAccessible(true);
            for(String endpoint:new String[]{"http://127.0.0.1:1/primary",SecurityApiClient.FALLBACK}){
                boolean blocked=false;try{post.invoke(a.securityAccess,endpoint,new byte[0]);}
                catch(java.lang.reflect.InvocationTargetException failure){blocked=failure.getCause() instanceof IOException&&failure.getCause().getMessage().contains("password OpenSAAB");}
                check(blocked,"Unauthenticated primary/fallback processing was not blocked before HTTP");
            }
            java.lang.reflect.Method process=SecurityAccessView.class.getDeclaredMethod("processAttempt",File.class,boolean.class,VehicleIdentity.class,SecurityAccessStatus.class,byte[].class);process.setAccessible(true);
            boolean importBlocked=false;try{process.invoke(a.securityAccess,null,true,null,null,new byte[0]);}
            catch(java.lang.reflect.InvocationTargetException failure){importBlocked=failure.getCause() instanceof IOException&&failure.getCause().getMessage().contains("password OpenSAAB");}
            check(importBlocked,"Unauthenticated process/import route was not blocked before file access");
            out.putString("stream","PASS: ignition and lookup startup contract; exact OpenSAAB accepted/encrypted; blank, wrong, case-changed, cancelled and invalid remembered credentials rejected; unauthenticated primary, fallback and process/import entry blocked before HTTP/file access; no USB/API operations\n");
        }catch(Throwable e){code=0;out.putString("stream","FAIL: "+e+"\n");}
        finally{
            if(a!=null){final Activity f=a;main(f::finish);}
            if(credential!=null)try{if(backup==null)credential.delete();else Files.write(credential.toPath(),backup);}catch(Exception ignored){}
            finish(code,out);
        }
    }
}
