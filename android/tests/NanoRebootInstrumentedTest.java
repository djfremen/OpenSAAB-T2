// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.os.*;
import android.widget.*;
import java.io.*;

/** Public-mode UI in disposable AVD; no Nano attached, firmware or transport is opened. */
public final class NanoRebootInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    public void onStart(){Bundle result=new Bundle();int code=-1;NanoProbeActivity activity=null;
        try{
            check("com.opensaab.beta.diagnostics".equals(getTargetContext().getPackageName()),"Disposable harness required");
            check((getTargetContext().getApplicationInfo().flags&ApplicationInfo.FLAG_DEBUGGABLE)==0,"Exercise actual public-mode permissions");
            activity=(NanoProbeActivity)startActivitySync(new Intent(getTargetContext(),NanoProbeActivity.class).putExtra("reboot_nano",true).putExtra("auto_start",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final NanoProbeActivity a=activity;waitForIdleSync();
            runOnMainSync(()->{
                check(a.rebootNano&&!a.rebootRequestRejected&&!a.nativeFirmware(),"Public standalone reboot was rejected or used firmware mode");
                Button reboot=a.getWindow().getDecorView().findViewWithTag("nano-reboot");check(reboot!=null&&"Reboot Nano".contentEquals(reboot.getText()),"Public Reboot Nano action missing");
                check(a.securityAccess==null&&!a.running.get()&&!NanoProbeActivity.sessionActive(),"Reboot selected firmware/security or claimed USB without Nano");
                check(a.summary.getText().toString().contains("Nano not found"),"No-adapter reboot claimed success or requested diagnostic firmware");
                check(!NanoRebootAction.available(a),"No Nano should expose main-menu action");
                reboot.performClick();
                check(!a.running.get()&&!NanoProbeActivity.sessionActive(),"No-adapter retry claimed ownership");a.finish();
            });
            activity=(NanoProbeActivity)startActivitySync(new Intent(getTargetContext(),NanoProbeActivity.class).putExtra("reboot_nano",true).putExtra("native_dtc",true).putExtra("auto_start",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final NanoProbeActivity mixed=activity;waitForIdleSync();
            runOnMainSync(()->{check(mixed.rebootRequestRejected&&!mixed.running.get()&&!NanoProbeActivity.sessionActive(),"Mixed diagnostic/reboot request was allowed");});
            result.putString("stream","PASS: nondebug/public-mode Reboot Nano visible; dedicated adapter-only entry does not require firmware; no Nano fails without ownership or false success; retry sends nothing without adapter; mixed reboot/diagnostics rejected. No hardware/API/security/firmware commands.\n");
        }catch(Throwable error){code=0;StringWriter trace=new StringWriter();error.printStackTrace(new PrintWriter(trace));result.putString("stream","FAIL: "+trace+"\n");}
        finally{if(activity!=null){Activity a=activity;runOnMainSync(a::finish);}finish(code,result);}
    }
}
