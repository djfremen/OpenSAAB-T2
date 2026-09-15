// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;

import android.app.Activity;
import android.os.Bundle;
import android.widget.*;
import com.opensaab.usb.BrandHeader;
import com.opensaab.usb.DeviceCompatibility;

/** Java-only companion: runs on 32-bit and 64-bit Android without native code. */
public final class MainActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        ScrollView scroll = new ScrollView(this);scroll.setFillViewport(true);scroll.setBackgroundColor(0xff0d1620);
        LinearLayout root = new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);
        int pad = Math.round(16 * getResources().getDisplayMetrics().density);root.setPadding(pad,pad,pad,pad);
        root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(pad,pad+i.getSystemWindowInsetTop(),pad,pad+i.getSystemWindowInsetBottom());return i;});
        scroll.addView(root);root.addView(new BrandHeader(this,"OpenSAAB system check"));
        TextView description = new TextView(this);description.setText("Check this device before installing the emulator. No firmware or adapter is needed.");description.setTextColor(0xffdce9f2);description.setTextSize(16);root.addView(description);
        TextView report = new TextView(this);report.setTextColor(0xffdce9f2);report.setTextSize(16);report.setTextIsSelectable(true);report.setPadding(0,pad,0,pad);
        Button refresh = new Button(this);refresh.setText("Check again");refresh.setAllCaps(false);refresh.setOnClickListener(v->report.setText(DeviceCompatibility.report(this)));root.addView(refresh);
        Button copy = new Button(this);copy.setText("Copy report");copy.setAllCaps(false);copy.setOnClickListener(v->DeviceCompatibility.copy(this,report.getText().toString()));root.addView(copy);
        root.addView(report);setContentView(scroll);report.setText(DeviceCompatibility.report(this));
    }
}
