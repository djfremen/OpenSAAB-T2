// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.view.*;
import android.widget.*;

/** Bounded, scrollable overlay: secondary controls never resize the firmware. */
public final class SessionSheet {
    public static Dialog show(Activity activity,String title,View content){
        if(content.getParent() instanceof android.view.ViewGroup)((android.view.ViewGroup)content.getParent()).removeView(content);
        Dialog dialog=new Dialog(activity);dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        LinearLayout panel=new LinearLayout(activity);panel.setOrientation(LinearLayout.VERTICAL);
        int pad=SessionStyle.dp(activity,12);panel.setPadding(pad,pad,pad,pad);panel.setBackgroundColor(0xff101922);
        LinearLayout heading=new LinearLayout(activity);heading.setGravity(Gravity.CENTER_VERTICAL);
        TextView label=new TextView(activity);label.setText(title);label.setTextSize(18);label.setTextColor(0xffedf4fa);
        heading.addView(label,new LinearLayout.LayoutParams(0,-2,1));
        Button close=new Button(activity);close.setText("Done");SessionStyle.button(close,false);close.setOnClickListener(v->dialog.dismiss());
        heading.addView(close,new LinearLayout.LayoutParams(SessionStyle.dp(activity,80),SessionStyle.dp(activity,48)));
        panel.addView(heading);
        ScrollView scroll=new ScrollView(activity);scroll.setFillViewport(false);scroll.addView(content);
        LinearLayout.LayoutParams sp=new LinearLayout.LayoutParams(-1,0,1);sp.topMargin=pad;panel.addView(scroll,sp);
        dialog.setContentView(panel);dialog.show();
        Window window=dialog.getWindow();window.setBackgroundDrawableResource(android.R.color.transparent);
        android.graphics.Rect bounds=new android.graphics.Rect();activity.getWindow().getDecorView().getWindowVisibleDisplayFrame(bounds);
        int width=bounds.width()>0?bounds.width():activity.getResources().getDisplayMetrics().widthPixels;
        int height=bounds.height()>0?bounds.height():activity.getResources().getDisplayMetrics().heightPixels;
        window.setLayout(Math.min(width,SessionStyle.dp(activity,560)),height*85/100);window.setGravity(Gravity.BOTTOM);
        dialog.setOnDismissListener(d->scroll.removeView(content));return dialog;
    }
    public static final class Menu {
        final Activity activity;final LinearLayout items;Dialog dialog;
        public Menu(Activity a){activity=a;items=new LinearLayout(a);items.setOrientation(LinearLayout.VERTICAL);}
        public Menu add(String title,Runnable action){
            Button b=new Button(activity);b.setText(title);SessionStyle.button(b,false);
            LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,SessionStyle.dp(activity,48));p.topMargin=SessionStyle.dp(activity,8);items.addView(b,p);
            b.setOnClickListener(v->{if(dialog!=null)dialog.dismiss();action.run();});return this;
        }
        public Dialog show(String title){dialog=SessionSheet.show(activity,title,items);return dialog;}
    }
    public static void confirmClear(Activity a,Runnable proceed){
        new AlertDialog.Builder(a).setTitle("Clear diagnostic trouble codes?")
            .setMessage("This opens Clear DTC in the firmware. Save or read the fault codes first; clearing them can remove diagnostic information. Follow any confirmation shown by the firmware.")
            .setNegativeButton("Cancel",null).setPositiveButton("Continue",(d,w)->proceed.run()).show();
    }
    private SessionSheet(){}
}
