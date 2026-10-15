// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.app.Dialog;
import android.content.Context;
import android.content.SharedPreferences;
import android.widget.LinearLayout;
import android.widget.Switch;
import android.widget.TextView;

/** Native storage/UI for the common gesture_scrolling contract. No session commands. */
public final class FirmwareScrollPreferences {
    static final String KEY = "naturalScrolling";
    static final String LABEL = "Natural scrolling";
    static final boolean DEFAULT_NATURAL = false;

    static SharedPreferences storage(Context context) {
        return context.getSharedPreferences("firmware-controls", Context.MODE_PRIVATE);
    }
    static boolean natural(Context context) {
        return storage(context).getBoolean(KEY, DEFAULT_NATURAL);
    }
    static int swipeKey(boolean up, boolean natural) {
        return up != natural ? 0x09 : 0x0c;
    }
    static String description(boolean natural) {
        return natural
            ? "Swipe up moves the selection down; swipe down moves it up."
            : "Swipe up moves the selection up; swipe down moves it down.";
    }
    static Dialog show(Activity activity) {
        LinearLayout content = new LinearLayout(activity);
        content.setOrientation(LinearLayout.VERTICAL);
        Switch toggle = new Switch(activity);
        toggle.setTag("tech2-natural-scrolling");
        toggle.setText(LABEL);
        toggle.setTextSize(16);
        toggle.setTextColor(0xffedf4fa);
        toggle.setMinHeight(SessionStyle.dp(activity, 48));
        toggle.setChecked(natural(activity));
        content.addView(toggle, new LinearLayout.LayoutParams(-1, -2));
        TextView detail = new TextView(activity);
        detail.setTag("tech2-scroll-description");
        detail.setText(description(toggle.isChecked()));
        detail.setTextSize(14);
        detail.setTextColor(0xffedf4fa);
        content.addView(detail);
        TextView scope = new TextView(activity);
        scope.setText("Vertical swipes on the original firmware display only. Keyboard arrows, keypad buttons, hold-to-ENTER, taps and app scrolling retain their meanings.");
        scope.setTextSize(14);
        scope.setTextColor(0xffedf4fa);
        scope.setPadding(0, SessionStyle.dp(activity, 12), 0, 0);
        content.addView(scope);
        toggle.setOnCheckedChangeListener((button, checked) -> {
            storage(activity).edit().putBoolean(KEY, checked).apply();
            detail.setText(description(checked));
        });
        return SessionSheet.show(activity, "Preferences", content);
    }
    private FirmwareScrollPreferences() {}
}
