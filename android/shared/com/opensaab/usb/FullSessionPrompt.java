// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.app.AlertDialog;

/** Explicit "Restart in Full session" choice after a Read-only refusal. Never restarts by itself. */
public final class FullSessionPrompt {
    public static void offer(Activity activity,PolicyBlock block,Runnable restartFull){
        if(activity.isFinishing()||activity.isDestroyed())return;
        new AlertDialog.Builder(activity).setTitle(block.title()).setMessage(block.message())
            .setNegativeButton("Stay read-only",null)
            .setPositiveButton("Restart in Full session",(d,w)->{SessionModeControl.remember(activity,SessionModes.Mode.FULL);restartFull.run();})
            .show();
    }
    private FullSessionPrompt(){}
}
