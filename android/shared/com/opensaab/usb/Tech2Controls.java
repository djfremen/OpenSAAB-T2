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
    private FrameLayout screen;
    private boolean guideChecked;
    android.app.Dialog keypadDialog,consoleDialog;
    private ScrollView consoleView;
    private Button actionButton;
    private Runnable actions;
    private int dp(int n) { return Math.round(n * getResources().getDisplayMetrics().density); }

    public Tech2Controls(Context context, View lcd, ScrollView console, IntConsumer send) {
        super(context);
        setOrientation(VERTICAL);
        ScrollView body = new ScrollView(context);
        body.setTag("tech2-keypad"); body.setVisibility(GONE);
        body.setContentDescription("Scroll for more firmware keys");
        LinearLayout content = new LinearLayout(context); content.setOrientation(VERTICAL);
        body.addView(content);
        TextView hint=new TextView(context);hint.setText("Swipe ↑ / ↓ on the screen to move one item. Hold for ENTER.");hint.setTextSize(14);hint.setTextColor(0xffedf4fa);content.addView(hint,new LayoutParams(-1,-2));
        display = new FirmwareGestureView(context, send);
        display.setTag("tech2-gestures");
        consoleView = console;
        if (lcd instanceof ImageView) ((ImageView) lcd).setScaleType(ImageView.ScaleType.FIT_CENTER);
        lcd.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        display.addView(lcd, new FrameLayout.LayoutParams(-1, -1));
        screen=new FrameLayout(context);screen.addView(display,new FrameLayout.LayoutParams(-1,-1));
        addView(screen, new LayoutParams(-1, 0, 1));
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
        TextView navigation=new TextView(context);navigation.setTag("tech2-navigation-hint");
        navigation.setText("Swipe ↑ / ↓ · Hold for ENTER · EXIT goes back");
        navigation.setTextSize(12);navigation.setTextColor(0xffedf4fa);navigation.setGravity(android.view.Gravity.CENTER);
        navigation.setPadding(0,dp(4),0,0);navigation.setOnClickListener(v->showHelp());
        navigation.setContentDescription("Navigation instructions. Tap for all firmware controls.");
        addView(navigation,new LayoutParams(-1,-2));
        addView(body, new LayoutParams(-1, 0));
        LinearLayout footer = new LinearLayout(context);
        Button exit = button("EXIT", () -> send.accept(0x01));
        exit.setTag("tech2-key-1"); exit.setContentDescription("EXIT — return in firmware");
        footer.addView(exit, new LayoutParams(0, -1, 1));
        Button keys = button("Keypad", () -> {});
        keys.setTag("tech2-keypad-toggle"); keys.setContentDescription("Show full keypad");
        keys.setOnClickListener(v -> {
            display.cancelGesture();
            body.removeView(content);
            android.app.Dialog dialog=SessionSheet.show((android.app.Activity)getContext(),"Firmware keypad",content);
            keypadDialog=dialog;
            dialog.setOnDismissListener(d->{if(content.getParent() instanceof android.view.ViewGroup)((android.view.ViewGroup)content.getParent()).removeView(content);body.addView(content);});
        });
        footer.addView(keys, new LayoutParams(0, -1, 1));
        actionButton=button("Console",this::showConsole);actionButton.setTag("tech2-actions");
        footer.addView(actionButton,new LayoutParams(0,-1,1));
        SessionStyle.row(footer);
        LayoutParams fp=new LayoutParams(-1,dp(48));fp.topMargin=dp(8);addView(footer,fp);
    }
    /** Called on the UI thread when firmware first produces a frame. No extra setup gate. */
    public void showFirstUseGuide(){
        if(guideChecked)return;
        guideChecked=true;
        android.content.SharedPreferences prefs=getContext().getSharedPreferences("firmware-controls",Context.MODE_PRIVATE);
        if(prefs.getBoolean("guide-v1-dismissed",false))return;
        LinearLayout guide=new LinearLayout(getContext());guide.setTag("tech2-first-use-guide");
        guide.setGravity(android.view.Gravity.CENTER_VERTICAL);guide.setPadding(dp(12),dp(8),dp(8),dp(8));guide.setBackgroundColor(0xff203447);
        // Sibling of the gesture surface: touching the guide must never send ENTER or a menu key.
        guide.setClickable(true);
        TextView text=new TextView(getContext());text.setText("Swipe ↑ / ↓ to move. Hold for ENTER. EXIT goes back. Keypad has all keys.");text.setTextSize(14);text.setTextColor(0xffedf4fa);
        guide.addView(text,new LayoutParams(0,-2,1));
        Button done=button("Got it",()->{display.cancelGesture();prefs.edit().putBoolean("guide-v1-dismissed",true).apply();screen.removeView(guide);});done.setTag("tech2-guide-dismiss");
        guide.addView(done,new LayoutParams(dp(76),dp(48)));
        FrameLayout.LayoutParams position=new FrameLayout.LayoutParams(-1,-2,android.view.Gravity.BOTTOM);
        screen.addView(guide,position);
    }
    public void setActions(Runnable open){actions=open;actionButton.setText("Actions");actionButton.setContentDescription("Open diagnostic actions");actionButton.setOnClickListener(v->{display.cancelGesture();actions.run();});}
    public void showConsole(){display.cancelGesture();consoleDialog=SessionSheet.show((android.app.Activity)getContext(),"Console",consoleView);}
    public void showHelp(){new android.app.AlertDialog.Builder(getContext()).setTitle("Firmware controls")
        .setMessage("Swipe up or down on the Tech2 screen to move one item. Hold the screen for ENTER. S1–S4 follow the firmware labels. EXIT returns within the firmware; it does not stop emulation. Open Keypad for arrows, numbers and other keys.")
        .setPositiveButton("Got it",null).show();}
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
