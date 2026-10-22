// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.widget.TextView;
import java.util.Map;
import java.util.WeakHashMap;

/**
 * One visual language for the active session mode on every screen.
 * Full sessions are always shown in a warning color with an explicit
 * "vehicle changes possible" line; Read-only is shown as blocked.
 * Display only: this class grants or removes no transport authority.
 */
public final class SessionModeBadge {
    public static final String TAG="session-mode-badge";
    // Polling screens re-apply every tick; skip identical styling so the layout is not invalidated.
    private static final Map<TextView,String> APPLIED=new WeakHashMap<>();
    private static boolean unchanged(TextView badge,String key){
        if(key.equals(APPLIED.get(badge))&&badge.getVisibility()==android.view.View.VISIBLE)return true;
        APPLIED.put(badge,key);return false;
    }
    static final int FULL_BG=0xff6b2410, FULL_FG=0xffffe2d3, FULL_EDGE=0xffff8a5c;
    static final int WRITE_BG=0xff5a4410, WRITE_FG=0xffffefc2, WRITE_EDGE=0xffffc94d;
    static final int READ_BG=0xff123047, READ_FG=0xffcdeeff, READ_EDGE=0xff5bb7e8;
    static final int OFFLINE_BG=0xff1f2a33, OFFLINE_FG=0xffd5dee6, OFFLINE_EDGE=0xff6c7d8c;

    public static String headline(SessionModes.Mode mode){return SessionModes.headline(mode);}
    public static String detail(SessionModes.Mode mode){return SessionModes.detail(mode);}
    public static boolean writes(SessionModes.Mode mode){return SessionModes.writes(mode);}

    public static TextView create(Context context,SessionModes.Mode mode,boolean offline){
        TextView badge=new TextView(context);badge.setTag(TAG);apply(badge,mode,offline);return badge;
    }
    public static void apply(TextView badge,SessionModes.Mode mode,boolean offline){apply(badge,mode,offline,false);}
    /** Compact = headline only, for the Ready-screen chooser where height is limited. */
    public static void apply(TextView badge,SessionModes.Mode mode,boolean offline,boolean compact){
        SessionModes.Mode shown=offline?null:mode;
        if(unchanged(badge,"mode:"+shown+":"+compact))return;
        int bg,fg,edge;
        if(shown==null){bg=OFFLINE_BG;fg=OFFLINE_FG;edge=OFFLINE_EDGE;}
        else if(shown==SessionModes.Mode.FULL){bg=FULL_BG;fg=FULL_FG;edge=FULL_EDGE;}
        else if(writes(shown)){bg=WRITE_BG;fg=WRITE_FG;edge=WRITE_EDGE;}
        else {bg=READ_BG;fg=READ_FG;edge=READ_EDGE;}
        String text=compact?headline(shown):headline(shown)+"\n"+detail(shown);
        if(!text.contentEquals(badge.getText()))badge.setText(text);
        badge.setTextColor(fg);badge.setTextSize(compact?12:13);badge.setTypeface(null,Typeface.BOLD);
        int pad=SessionStyle.dp(badge.getContext(),compact?6:8);badge.setPadding(pad,pad/3,pad,pad/3);
        GradientDrawable panel=new GradientDrawable();panel.setColor(bg);panel.setCornerRadius(SessionStyle.dp(badge.getContext(),6));
        panel.setStroke(SessionStyle.dp(badge.getContext(),shown==SessionModes.Mode.FULL?2:1),edge);badge.setBackground(panel);
        badge.setContentDescription(headline(shown)+". "+detail(shown));
        badge.setVisibility(android.view.View.VISIBLE);
    }
    /** Special single-purpose sessions (key-status test, BCM reminder) keep their own wording. */
    public static void applyCustom(TextView badge,String headline,boolean writes){
        if(unchanged(badge,"custom:"+writes+":"+headline))return;
        int bg=writes?WRITE_BG:READ_BG,fg=writes?WRITE_FG:READ_FG,edge=writes?WRITE_EDGE:READ_EDGE;
        if(!headline.contentEquals(badge.getText()))badge.setText(headline);
        badge.setTextColor(fg);badge.setTextSize(12);badge.setTypeface(null,Typeface.BOLD);
        int pad=SessionStyle.dp(badge.getContext(),6);badge.setPadding(pad,pad/3,pad,pad/3);
        GradientDrawable panel=new GradientDrawable();panel.setColor(bg);panel.setCornerRadius(SessionStyle.dp(badge.getContext(),6));
        panel.setStroke(SessionStyle.dp(badge.getContext(),1),edge);badge.setBackground(panel);
        badge.setContentDescription(headline);badge.setVisibility(android.view.View.VISIBLE);
    }
    private SessionModeBadge(){}
}
