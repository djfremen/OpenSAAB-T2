// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.content.SharedPreferences;
import android.view.View;
import android.widget.*;

/** Primary Ready-screen choice. Selection alone never opens a transport. */
public final class SessionModeControl extends LinearLayout {
    static final String PREFS="session_mode", KEY="selected";
    private final RadioButton read, full;
    private final TextView preview;
    private final SharedPreferences prefs;
    private Runnable changed;
    /** Notified after every change, including reload(); e.g. to relabel Connect and start. */
    public void onChange(Runnable listener){changed=listener;if(listener!=null)listener.run();}
    public SessionModeControl(Context context){
        super(context);setOrientation(VERTICAL);setTag("session-mode-control");
        prefs=context.getSharedPreferences(PREFS,Context.MODE_PRIVATE);
        TextView title=new TextView(context);title.setText("Session Mode");title.setTextSize(15);title.setTextColor(0xffb6e8ff);title.setTypeface(null,android.graphics.Typeface.BOLD);addView(title);
        RadioGroup options=new RadioGroup(context);options.setOrientation(HORIZONTAL);options.setTag("session-mode-options");addView(options);
        read=option(context,SessionModes.Mode.READ.label,"session-mode-read");
        full=option(context,SessionModes.Mode.FULL.label,"session-mode-full");
        options.addView(read,new RadioGroup.LayoutParams(0,SessionStyle.dp(context,48),1));
        options.addView(full,new RadioGroup.LayoutParams(0,SessionStyle.dp(context,48),1));
        // Remember the operator's last explicit choice; first run stays Read-only.
        options.check(restore(prefs)==SessionModes.Mode.FULL?full.getId():read.getId());
        preview=new TextView(context);preview.setTag("session-mode-preview");SessionModeBadge.apply(preview,selected(),false,true);
        LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,-2);p.topMargin=SessionStyle.dp(context,4);addView(preview,p);
        options.setOnCheckedChangeListener((group,id)->{
            SessionModes.Mode mode=selected();
            try{prefs.edit().putString(KEY,mode.name()).apply();}catch(RuntimeException ignored){}
            SessionModeBadge.apply(preview,mode,false,true);
            if(changed!=null)changed.run();
        });
    }
    static SessionModes.Mode restore(SharedPreferences prefs){
        try{return SessionModes.Mode.FULL.name().equals(prefs.getString(KEY,null))?SessionModes.Mode.FULL:SessionModes.Mode.READ;}
        catch(RuntimeException unreadable){return SessionModes.Mode.READ;}
    }
    /** Make the next connection use this mode, e.g. after "Restart in Full session". */
    public static void remember(Context context,SessionModes.Mode mode){
        if(mode!=SessionModes.Mode.FULL&&mode!=SessionModes.Mode.READ)return;
        try{context.getSharedPreferences(PREFS,Context.MODE_PRIVATE).edit().putString(KEY,mode.name()).apply();}catch(RuntimeException ignored){}
    }
    private static RadioButton option(Context context,String label,String tag){
        RadioButton button=new RadioButton(context);button.setId(View.generateViewId());button.setText(label);button.setTag(tag);button.setTextSize(14);button.setTextColor(0xffedf4fa);return button;
    }
    public SessionModes.Mode selected(){return full.isChecked()?SessionModes.Mode.FULL:SessionModes.Mode.READ;}
    /** Re-read the saved choice, e.g. when returning from an adapter screen that switched mode. */
    public void reload(){
        SessionModes.Mode saved=restore(prefs);
        if(saved!=selected())(saved==SessionModes.Mode.FULL?full:read).setChecked(true);
        SessionModeBadge.apply(preview,selected(),false,true);
        if(changed!=null)changed.run();
    }
    public void available(boolean enabled){read.setEnabled(enabled);full.setEnabled(enabled);}
}
