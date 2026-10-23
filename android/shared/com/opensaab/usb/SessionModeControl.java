// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.widget.*;

/** Normal connections use Full session; this is an informational status, not a chooser. */
public final class SessionModeControl extends LinearLayout {
    static final String PREFS="session_mode", KEY="selected";
    private final TextView preview;
    private Runnable changed;
    /** Notified after every change, including reload(); e.g. to relabel Connect and start. */
    public void onChange(Runnable listener){changed=listener;if(listener!=null)listener.run();}
    public SessionModeControl(Context context){
        super(context);setOrientation(VERTICAL);setTag("session-mode-control");
        preview=new TextView(context);preview.setTag("session-mode-preview");SessionModeBadge.apply(preview,selected(),false,true);
        LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,-2);p.topMargin=SessionStyle.dp(context,4);addView(preview,p);
    }
    /** Compatibility with explicit Full-session continuation callers. */
    public static void remember(Context context,SessionModes.Mode mode){
        if(mode!=SessionModes.Mode.FULL)return;
        try{context.getSharedPreferences(PREFS,Context.MODE_PRIVATE).edit().putString(KEY,mode.name()).apply();}catch(RuntimeException ignored){}
    }
    public SessionModes.Mode selected(){return SessionModes.Mode.FULL;}
    public void reload(){
        SessionModeBadge.apply(preview,selected(),false,true);
        if(changed!=null)changed.run();
    }
    public void available(boolean enabled){setEnabled(enabled);}
}
