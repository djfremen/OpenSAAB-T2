// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.content.res.Configuration;
import android.view.View;
import android.widget.*;
import java.util.ArrayList;

/** Landscape shell for the separate ARMv7 head-unit build. Never changes a session. */
public final class HeadunitLayout {
    public static void apply(LinearLayout root) {
        Context context = root.getContext();
        Configuration config = context.getResources().getConfiguration();
        if (!AppBuildProfile.isHeadunit32(context) || config.screenWidthDp < 720
                || config.screenWidthDp <= config.screenHeightDp) return;
        Tech2Controls display = null;
        ArrayList<View> actions = new ArrayList<>();
        for (int i = 0; i < root.getChildCount(); i++) {
            View child = root.getChildAt(i);
            if (child instanceof Tech2Controls) display = (Tech2Controls) child;
            else actions.add(child);
        }
        if (display == null) return;
        float density = context.getResources().getDisplayMetrics().density;
        int pad = Math.round(8 * density), railWidth = Math.round(280 * density);
        root.removeAllViews();
        root.setOrientation(LinearLayout.HORIZONTAL);
        root.setPadding(pad, pad, pad, pad);
        root.setOnApplyWindowInsetsListener((v, insets) -> {
            v.setPadding(insets.getSystemWindowInsetLeft() + pad,
                insets.getSystemWindowInsetTop() + pad,
                insets.getSystemWindowInsetRight() + pad,
                insets.getSystemWindowInsetBottom() + pad);
            return insets;
        });
        root.addView(display, new LinearLayout.LayoutParams(0, -1, 1));
        ScrollView rail = new ScrollView(context);
        rail.setTag("headunit-actions");
        rail.setContentDescription("Session controls and tools; scroll for more");
        LinearLayout content = new LinearLayout(context);
        content.setOrientation(LinearLayout.VERTICAL);
        rail.addView(content);
        for (View child : actions) {
            // Only simple activity button rows are rearranged. Stateful security,
            // report and status views keep their own layout and visibility logic.
            if (child.getClass() == LinearLayout.class && isButtonRow((LinearLayout) child)) {
                LinearLayout old = (LinearLayout) child;
                ArrayList<View> buttons = new ArrayList<>();
                for (int i = 0; i < old.getChildCount(); i++) buttons.add(old.getChildAt(i));
                old.removeAllViews();
                for (View button : buttons) {
                    LinearLayout.LayoutParams bp=new LinearLayout.LayoutParams(-1,Math.round(48*density));bp.topMargin=pad;content.addView(button,bp);
                    button.setMinimumHeight(Math.round(48 * density));
                    ((Button) button).setTextSize(14);
                }
            } else {LinearLayout.LayoutParams cp=new LinearLayout.LayoutParams(-1,-2);cp.topMargin=pad;content.addView(child,cp);}
        }
        LinearLayout.LayoutParams railParams = new LinearLayout.LayoutParams(railWidth, -1);
        railParams.leftMargin = pad;
        root.addView(rail, railParams);
    }
    private static boolean isButtonRow(LinearLayout row) {
        if (row.getChildCount() == 0) return false;
        for (int i = 0; i < row.getChildCount(); i++)
            if (!(row.getChildAt(i) instanceof Button)) return false;
        return true;
    }
    private HeadunitLayout() {}
}
