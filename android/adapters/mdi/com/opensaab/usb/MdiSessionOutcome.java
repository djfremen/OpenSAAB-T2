// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import org.json.JSONObject;

/** The JNI carrier's success cannot replace the actual firmware process result. */
public final class MdiSessionOutcome {
    public final Integer exitCode;
    public final boolean forced,failed;
    public MdiSessionOutcome(int carrierResult,JSONObject nativeResult){
        Integer exit=null;
        if(nativeResult!=null&&!nativeResult.isNull("process_exit"))try{exit=nativeResult.getInt("process_exit");}catch(Exception invalid){}
        exitCode=exit;forced=nativeResult!=null&&nativeResult.optBoolean("forced_kill");
        failed=carrierResult!=0||forced||nativeResult!=null&&(exit==null||exit!=0);
    }
}
