// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.view.View;
import android.widget.*;
import java.util.function.IntConsumer;

/** Shared original-firmware keypad. Scrolling never hides firmware EXIT. */
public final class Tech2Controls extends LinearLayout {
    // Five-bit encoder identities verified against the OEM Windows keyboard map.
    // Keep the existing guest press/release semantics; SHIFT is not yet verified.
    private static final String[][] LABELS = {
        {"S1", "S2", "S3", "S4"},
        {"HELP", "↑", "YES"}, {"←", "ENTER", "→"}, {"", "↓", "NO"},
        {"F0", "F1", "F2"}, {"F3", "F4", "F5"},
        {"F6", "F7", "F8"}, {"F9", "", ""}
    };
    private static final int[][] CODES = {
        {0x0a, 0x06, 0x07, 0x08},
        {0x19, 0x09, 0x0e}, {0x0b, 0x10, 0x0d}, {-1, 0x0c, 0x14},
        {0x18, 0x04, 0x13}, {0x17, 0x03, 0x12},
        {0x16, 0x02, 0x11}, {0x15, -1, -1}
    };
    private FirmwareGestureView display;
    private ScrollView keypad;
    private boolean keypadVisible;
    private ScrollView consoleView;
    private int dp(int n) { return Math.round(n * getResources().getDisplayMetrics().density); }

    public Tech2Controls(Context context, View lcd, ScrollView console, IntConsumer send) {
        super(context);
        setOrientation(VERTICAL);
        ScrollView body = new ScrollView(context);
        keypad = body; body.setTag("tech2-keypad"); body.setVisibility(GONE);
        body.setContentDescription("Scroll for more firmware keys");
        LinearLayout content = new LinearLayout(context); content.setOrientation(VERTICAL);
        body.addView(content);
        display = new FirmwareGestureView(context, send);
        display.setTag("tech2-gestures");
        consoleView = console;
        if (lcd instanceof ImageView) ((ImageView) lcd).setScaleType(ImageView.ScaleType.FIT_CENTER);
        lcd.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        display.addView(lcd, new FrameLayout.LayoutParams(-1, -1));
        addView(display, new LayoutParams(-1, 0, 1));
        for (int r = 0; r < LABELS.length; r++) {
            LinearLayout row = new LinearLayout(context);
            LayoutParams rowParams=new LayoutParams(-1,dp(48));rowParams.topMargin=dp(8);
            (r == 0 ? this : content).addView(row,rowParams);
            for (int c = 0; c < LABELS[r].length; c++) {
                final int code = CODES[r][c];
                if (code < 0) row.addView(new View(context), new LayoutParams(0, -1, 1));
                else {
                    Button key = button(LABELS[r][c], () -> send.accept(code));
                    key.setTag("tech2-key-" + code);
                    key.setContentDescription(description(LABELS[r][c]));
                    row.addView(key, new LayoutParams(0, -1, 1));
                }
            }
            SessionStyle.row(row);
        }
        addView(body, new LayoutParams(-1, 0));
        TextView hint = new TextView(context);
        hint.setText("Swipe ↑ / ↓ one item · Hold for ENTER");
        hint.setTextSize(12); hint.setGravity(android.view.Gravity.CENTER);
        hint.setTag("tech2-gesture-hint");
        addView(hint, new LayoutParams(-1, dp(24)));
        LinearLayout footer = new LinearLayout(context);
        Button exit = button("EXIT", () -> send.accept(0x01));
        exit.setTag("tech2-key-1"); exit.setContentDescription("EXIT — return in firmware");
        footer.addView(exit, new LayoutParams(0, -1, 1));
        Button keys = button("Keypad", () -> {});
        keys.setTag("tech2-keypad-toggle"); keys.setContentDescription("Show full keypad");
        keys.setOnClickListener(v -> {
            display.cancelGesture();
            keypadVisible = !keypadVisible;
            body.setVisibility(keypadVisible ? VISIBLE : GONE);
            keys.setText(keypadVisible ? "Hide keys" : "Keypad");
            keys.setContentDescription(keypadVisible ? "Hide full keypad" : "Show full keypad");
            requestLayout();
        });
        footer.addView(keys, new LayoutParams(0, -1, 1));
        Button toggle = button("Console", () -> {});
        toggle.setTag("tech2-console-toggle");
        toggle.setContentDescription("Show console");
        toggle.setOnClickListener(v -> {
            display.cancelGesture();
            boolean show = console.getVisibility() != VISIBLE;
            console.setVisibility(show ? VISIBLE : GONE);
            toggle.setText(show ? "Hide log" : "Console");
            toggle.setContentDescription(show ? "Hide console" : "Show console");
        });
        footer.addView(toggle, new LayoutParams(0, -1, 1));
        SessionStyle.row(footer);
        addView(footer, new LayoutParams(-1, dp(48)));
        console.setVisibility(GONE);
        // Limit expansion on short/landscape windows so navigation retains room.
        int consoleHeight = Math.min(120, getResources().getConfiguration().screenHeightDp / 5);
        addView(console, new LayoutParams(-1, dp(consoleHeight)));
    }
    @Override protected void onMeasure(int width, int height) {
        int available = View.MeasureSpec.getSize(height);
        consoleView.getLayoutParams().height = Math.min(dp(120), available / 5);
        if (consoleView.getVisibility() == VISIBLE) available -= consoleView.getLayoutParams().height;
        // The display takes all remaining room with keys hidden. Opening keys
        // gives their scrolling panel 55% of the flexible area. EXIT remains outside that scroll.
        int flexible = Math.max(0, available - dp(128)); // soft keys + hint + footer
        keypad.getLayoutParams().height = keypadVisible ? flexible * 55 / 100 : 0;
        super.onMeasure(width, height);
    }
    private Button button(String text, Runnable action) {
        Button b = new Button(getContext()); b.setText(text); b.setTextSize(14);
        b.setAllCaps(false); b.setPadding(0, 0, 0, 0); b.setMinWidth(0); b.setMinimumWidth(0);
        SessionStyle.button(b,false); b.setOnClickListener(v -> action.run());
        return b;
    }
    private static String description(String label) {
        switch (label) {
            case "↑": return "Up";
            case "↓": return "Down";
            case "←": return "Left / page up";
            case "→": return "Right / more";
            case "HELP": return "Help / question mark";
            default: return label;
        }
    }
}
