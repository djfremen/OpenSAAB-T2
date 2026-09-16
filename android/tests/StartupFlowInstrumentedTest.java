// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.Intent;
import android.os.Bundle;
import android.widget.TextView;
import java.io.*;
import java.nio.file.*;

/** Synthetic UI/credential tests only; never starts USB, firmware or a network request. */
public final class StartupFlowInstrumentedTest extends Instrumentation {
    void check(boolean v,String why){if(!v)throw new AssertionError(why);}
    void main(Runnable r){Throwable[] e={null};runOnMainSync(()->{try{r.run();}catch(Throwable t){e[0]=t;}});if(e[0]!=null)throw new AssertionError(e[0]);}
    public void onCreate(Bundle b){super.onCreate(b);start();}
    public void onStart(){
        Bundle out=new Bundle();int code=-1;ChipsoftUsbActivity a=null;File credential=null;byte[] backup=null;
        try{
            a=(ChipsoftUsbActivity)startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final ChipsoftUsbActivity activity=a;waitForIdleSync();
            main(()->{
                activity.requestVehicleStart();
                String text=((TextView)activity.vehicleStartPrompt.findViewById(android.R.id.message)).getText().toString();
                check(text.contains("ON")&&text.contains("ACC/accessory is not enough")&&text.contains("engine and color"),"Ignition/lookup instructions missing");
                check(!activity.running.get()&&activity.nativeDirectory==null,"Prompt started a vehicle operation");
                activity.vehicleStartPrompt.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
                VehicleIdentity v=new VehicleIdentity("YS3TEST1A41000001","2026-09-16T00:00:00Z","synthetic",2004,"available","9440","B207R","Silver Metallic","","");
                activity.confirmIdentified(null,v,activity.vehicleGeneration);
                text=((TextView)activity.vehicleStartPrompt.findViewById(android.R.id.message)).getText().toString();
                check(text.contains(v.vin)&&text.contains("Engine: B207R")&&text.contains("Color: Silver Metallic"),"Vehicle details not presented before firmware");
                check(!activity.running.get(),"Firmware started without confirmation");
                activity.vehicleStartPrompt.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
                activity.confirmIdentified(null,VehicleSession.unavailable(v,"offline"),activity.vehicleGeneration);
                text=((TextView)activity.vehicleStartPrompt.findViewById(android.R.id.message)).getText().toString();
                check(text.contains("VIN only")&&text.contains("Engine: Unavailable"),"Offline lookup failure hidden");
                activity.vehicleStartPrompt.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            });
            credential=new File(a.getNoBackupFilesDir(),"security-authorization.enc");if(credential.exists())backup=Files.readAllBytes(credential.toPath());
            SecurityAuthorization.save(a,"SamplePassword");
            check(SecurityAuthorization.bearer(a).equals("SamplePassword"),"Saved password changed case");
            check(!new String(Files.readAllBytes(credential.toPath()),"UTF-8").contains("SamplePassword"),"Password stored unencrypted");
            boolean rejected=false;try{SecurityAuthorization.save(a,"SamplePassword ");}catch(IllegalArgumentException expected){rejected=true;}check(rejected,"Whitespace silently accepted");
            SecurityAuthorization.forget(a);check(!SecurityAuthorization.available(a),"Rejected password cannot be cleared");
            out.putString("stream","PASS: key ON instructions precede VIN probe; engine/color shown before explicit firmware start; offline details disclosed; encrypted case-preserving password and retry cleanup; no USB/API operations\n");
        }catch(Throwable e){code=0;out.putString("stream","FAIL: "+e+"\n");}
        finally{
            if(a!=null){final Activity f=a;main(f::finish);}
            if(credential!=null)try{if(backup==null)credential.delete();else Files.write(credential.toPath(),backup);}catch(Exception ignored){}
            finish(code,out);
        }
    }
}
