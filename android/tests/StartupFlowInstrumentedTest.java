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
                VehicleIdentity v=new VehicleIdentity("YS3TEST1A41000001","2026-09-16T00:00:00Z","synthetic",2004,"available","9440","B207R","Silver Metallic","","");
                activity.vehicle=v;activity.showVehicle(false);
                check(activity.vinSummary.getText().toString().contains("B207R"),"Vehicle details absent");
                activity.vehicle=VehicleSession.unavailable(v,"not_requested");activity.showVehicle(false);
                check(activity.vinSummary.getText().toString().contains("Online details off"),"Offline choice mislabeled as failure");
                activity.vehicle=VehicleSession.unavailable(v,"pending");activity.showVehicle(false);
                check(activity.vinSummary.getText().toString().contains("diagnostics can continue"),"Pending lookup blocks diagnostics");
            });
            credential=new File(a.getNoBackupFilesDir(),"security-authorization.enc");if(credential.exists())backup=Files.readAllBytes(credential.toPath());
            SecurityAuthorization.save(a,"SamplePassword");
            check(SecurityAuthorization.bearer(a).equals("SamplePassword"),"Saved password changed case");
            check(!new String(Files.readAllBytes(credential.toPath()),"UTF-8").contains("SamplePassword"),"Password stored unencrypted");
            boolean rejected=false;try{SecurityAuthorization.save(a,"SamplePassword ");}catch(IllegalArgumentException expected){rejected=true;}check(rejected,"Whitespace silently accepted");
            SecurityAuthorization.forget(a);check(!SecurityAuthorization.available(a),"Rejected password cannot be cleared");
            out.putString("stream","PASS: key ON instructions precede VIN probe; optional lookup unchecked and disclosed; cancellation queues no operation; local/pending/enriched vehicle states; encrypted case-preserving password and retry cleanup; no USB/API operations\n");
        }catch(Throwable e){code=0;out.putString("stream","FAIL: "+e+"\n");}
        finally{
            if(a!=null){final Activity f=a;main(f::finish);}
            if(credential!=null)try{if(backup==null)credential.delete();else Files.write(credential.toPath(),backup);}catch(Exception ignored){}
            finish(code,out);
        }
    }
}
