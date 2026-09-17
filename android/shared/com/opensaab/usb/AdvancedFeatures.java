// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.widget.*;
import java.util.function.BooleanSupplier;

/** Secondary tools shared by the launcher and live Chipsoft session. */
public final class AdvancedFeatures {
    public static AlertDialog show(Activity a,BooleanSupplier running){
        LinearLayout panel=new LinearLayout(a);panel.setOrientation(LinearLayout.VERTICAL);
        int pad=SessionStyle.dp(a,16);panel.setPadding(pad,pad,pad,pad);panel.setBackgroundColor(0xff101922);
        Switch restricted=new Switch(a);restricted.setText("Chipsoft restricted mode");restricted.setTextColor(0xffedf4fa);
        restricted.setMinHeight(SessionStyle.dp(a,48));
        android.content.SharedPreferences prefs=a.getSharedPreferences("adapter_settings",Activity.MODE_PRIVATE);
        restricted.setChecked(prefs.getBoolean("chipsoft_restricted",false));
        restricted.setOnCheckedChangeListener((v,checked)->{
            prefs.edit().putBoolean("chipsoft_restricted",checked).apply();
            Toast.makeText(a,"Applies to the next connection",Toast.LENGTH_SHORT).show();
        });panel.addView(restricted);
        ScrollView scroll=new ScrollView(a);scroll.addView(panel);
        AlertDialog dialog=new AlertDialog.Builder(a).setTitle("Adv. Features").setView(scroll).create();
        add(panel,"Adapters",()->{if(idle(a,running)){dialog.dismiss();a.startActivity(new Intent(a,AdapterDetectorActivity.class));}});
        add(panel,"USB tests",()->{if(idle(a,running)){dialog.dismiss();a.startActivityForResult(new Intent(a,NanoProbeActivity.class),27);}});
        add(panel,"System Check",()->DeviceCompatibility.show(a));
        add(panel,"Security Access Password",()->SecurityAuthorization.show(a));
        add(panel,"Clear offset · refresh access",()->SecurityReset.show(a,running));
        add(panel,"Done",dialog::dismiss);
        SessionStyle.stack(panel);dialog.show();return dialog;
    }
    private static boolean idle(Activity a,BooleanSupplier running){
        if(!running.getAsBoolean()&&!FirmwareGate.sessionActive()&&!SecurityAccessView.workflowBusy())return true;
        Toast.makeText(a,"Stop the current session before opening adapter tools",Toast.LENGTH_LONG).show();return false;
    }
    private static void add(LinearLayout panel,String title,Runnable action){
        Button b=new Button(panel.getContext());b.setText(title);b.setOnClickListener(v->action.run());panel.addView(b);
    }
    private AdvancedFeatures(){}
}
