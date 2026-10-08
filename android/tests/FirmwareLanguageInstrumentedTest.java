// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.Intent;
import android.os.Bundle;
import android.view.*;
import android.widget.*;

/** Exercises real spinner callbacks; no download, vehicle request or active card mutation. */
public final class FirmwareLanguageInstrumentedTest extends Instrumentation {
    void check(boolean v,String why){if(!v)throw new AssertionError(why);}
    void main(Runnable r){Throwable[] err={null};runOnMainSync(()->{try{r.run();}catch(Throwable t){err[0]=t;}});if(err[0]!=null)throw new AssertionError(err[0]);}
    Spinner spinner(View v,String label){if(v instanceof Spinner&&label.contentEquals(v.getContentDescription()))return (Spinner)v;if(v instanceof ViewGroup)for(int i=0;i<((ViewGroup)v).getChildCount();i++){Spinner s=spinner(((ViewGroup)v).getChildAt(i),label);if(s!=null)return s;}return null;}
    public void onCreate(Bundle b){super.onCreate(b);start();}
    public void onStart(){Bundle out=new Bundle();int code=-1;Activity activity=null;
        try{
            activity=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.FirmwareActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final Activity a=activity;waitForIdleSync();android.os.SystemClock.sleep(800);
            final Spinner[] language={null},versions={null};
            main(()->{language[0]=spinner(a.getWindow().getDecorView(),"Diagnostic language");versions[0]=spinner(a.getWindow().getDecorView(),"Software version and language");check(language[0]!=null&&versions[0]!=null,"Missing selectors");});
            for(int i=0;i<FirmwareCatalog.LANGUAGE_CODES.length;i++){
                final int index=i;main(()->language[0].setSelection(index));waitForIdleSync();android.os.SystemClock.sleep(100);
                main(()->{
                    FirmwareCatalog.Entry e=(FirmwareCatalog.Entry)versions[0].getSelectedItem();
                    check(e.language.equals(FirmwareCatalog.LANGUAGE_CODES[index]),"Language selected wrong firmware: "+e.id);
                    check(a.getSharedPreferences("firmware_library",0).getString("selected_id","").equals(e.id),"Callback persisted wrong catalog index");
                    check(FirmwareCatalog.runnable(e),"Default language choice cannot boot");
                    for(int j=0;j<versions[0].getCount();j++)check(((FirmwareCatalog.Entry)versions[0].getItemAtPosition(j)).language.equals(e.language),"Mixed languages");
                });
            }
            main(a::finish);activity=null;
            activity=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("dtc_read",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final ChipsoftUsbActivity chip=(ChipsoftUsbActivity)activity;waitForIdleSync();
            main(()->{
                check(!chip.running.get()&&!chip.continueDiagnostics.isEnabled(),"Startup exposed continuation before verified report");
                chip.requestVehicleStart();
                check(chip.vehicleStartPrompt.isShowing(),"Power confirmation missing");
                chip.vehicleStartPrompt.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
                check(!chip.running.get()&&chip.vehicle==null&&!chip.continueDiagnostics.isEnabled(),"Cancel left stale identity or queued a read");
            });
            out.putString("stream","PASS: all nine language selectors map to their own firmware and persisted IDs; Chipsoft VIN/DTC startup confirmation cancels without USB or stale results\n");
        }catch(Throwable e){code=0;out.putString("stream","FAIL: "+e+"\n");}
        finally{if(activity!=null){final Activity a=activity;main(a::finish);}finish(code,out);}
    }
}
