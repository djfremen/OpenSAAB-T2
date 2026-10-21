// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.view.View;
import android.widget.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;

/** The same pre-connection choice for USB and Bluetooth; cancellation opens no transport. */
public final class SessionModePicker {
    public static AlertDialog show(Activity activity,SessionModes.Capabilities caps,BooleanSupplier ready,Consumer<SessionModes.Mode> selected){
        LinearLayout content=new LinearLayout(activity);content.setOrientation(LinearLayout.VERTICAL);
        int pad=SessionStyle.dp(activity,16);content.setPadding(pad,pad,pad,pad);
        TextView adapter=text(activity,caps.adapter,18);content.addView(adapter);
        RadioGroup options=new RadioGroup(activity);options.setTag("session-mode-options");content.addView(options);
        RadioButton read=new RadioButton(activity);read.setId(View.generateViewId());read.setText("Read-only"+(caps.read?"":" · unavailable"));read.setTag("session-mode-read");read.setEnabled(caps.read);options.addView(read);
        RadioButton write=new RadioButton(activity);write.setId(View.generateViewId());write.setText(caps.write==null?"Full session · unavailable":caps.write.label);write.setTag("session-mode-write");write.setEnabled(caps.write!=null);options.addView(write);
        TextView description=text(activity,"",15);description.setTag("session-mode-description");content.addView(description);
        content.addView(text(activity,caps.explanation,14));
        ScrollView scroll=new ScrollView(activity);scroll.addView(content);
        AlertDialog dialog=new AlertDialog.Builder(activity).setTitle("Choose session mode").setView(scroll)
            .setNegativeButton("Cancel",null).setPositiveButton("Connect",null).create();
        AtomicBoolean consumed=new AtomicBoolean();
        Runnable refresh=()->{
            SessionModes.Mode mode=options.getCheckedRadioButtonId()==read.getId()?SessionModes.Mode.READ:options.getCheckedRadioButtonId()==write.getId()?caps.write:null;
            description.setText(mode==null?"Choose a supported mode before connecting.":mode.description);
            Button connect=dialog.getButton(AlertDialog.BUTTON_POSITIVE);if(connect!=null){connect.setEnabled(caps.allows(mode));connect.setText(mode==null?"Connect":"Connect · "+mode.label);}
        };
        options.setOnCheckedChangeListener((group,id)->refresh.run());
        dialog.setOnDismissListener(d->consumed.set(true));
        dialog.setOnShowListener(d->{
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{
                SessionModes.Mode mode=options.getCheckedRadioButtonId()==read.getId()?SessionModes.Mode.READ:options.getCheckedRadioButtonId()==write.getId()?caps.write:null;
                if(!caps.allows(mode)||!ready.getAsBoolean()||!consumed.compareAndSet(false,true))return;
                dialog.dismiss();selected.accept(mode);
            });
            if(caps.read)options.check(read.getId());refresh.run();
        });
        SessionStyle.stack(content);dialog.show();return dialog;
    }
    private static TextView text(Activity activity,String value,int size){TextView text=new TextView(activity);text.setText(value);text.setTextSize(size);text.setTextColor(0xffedf4fa);return text;}
    private SessionModePicker(){}
}
