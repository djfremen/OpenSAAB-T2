// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.view.View;
import android.widget.*;

/** Primary Ready-screen choice. Selection alone never opens a transport. */
public final class SessionModeControl extends LinearLayout {
    private final RadioButton read, full;
    public SessionModeControl(Context context){
        super(context);setOrientation(VERTICAL);setTag("session-mode-control");
        TextView title=new TextView(context);title.setText("Session Mode");title.setTextSize(15);title.setTextColor(0xffb6e8ff);title.setTypeface(null,android.graphics.Typeface.BOLD);addView(title);
        RadioGroup options=new RadioGroup(context);options.setOrientation(HORIZONTAL);options.setTag("session-mode-options");addView(options);
        read=option(context,SessionModes.Mode.READ.label,"session-mode-read");
        full=option(context,SessionModes.Mode.FULL.label,"session-mode-full");
        options.addView(read,new RadioGroup.LayoutParams(0,SessionStyle.dp(context,48),1));
        options.addView(full,new RadioGroup.LayoutParams(0,SessionStyle.dp(context,48),1));
        options.check(read.getId());
        TextView help=new TextView(context);help.setText("Full session allows supported changes. Original confirmations and security access still apply.");help.setTextSize(12);help.setTextColor(0xffa8b9c9);addView(help);
    }
    private static RadioButton option(Context context,String label,String tag){
        RadioButton button=new RadioButton(context);button.setId(View.generateViewId());button.setText(label);button.setTag(tag);button.setTextSize(14);button.setTextColor(0xffedf4fa);return button;
    }
    public SessionModes.Mode selected(){return full.isChecked()?SessionModes.Mode.FULL:SessionModes.Mode.READ;}
    public void available(boolean enabled){read.setEnabled(enabled);full.setEnabled(enabled);}
}
