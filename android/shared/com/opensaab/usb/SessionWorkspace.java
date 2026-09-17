// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.view.*;
import android.widget.*;

/** Firmware-first layout, responsive to the available window, not ABI or model. */
public final class SessionWorkspace extends LinearLayout {
    private final LinearLayout rail;
    private final Tech2Controls controls;
    private final Button summary;
    private final Activity activity;
    private boolean expanded;
    public SessionWorkspace(Activity a,String title,Tech2Controls display,Runnable menu,Runnable details){
        super(a);activity=a;controls=display;setTag("session-workspace");setBackgroundColor(0xff0d1620);
        rail=new LinearLayout(a);rail.setOrientation(VERTICAL);rail.setTag("session-header");
        LinearLayout top=new LinearLayout(a);top.setGravity(Gravity.CENTER_VERTICAL);
        top.addView(new BrandHeader(a,title,true),new LayoutParams(0,-2,1));
        Button more=new Button(a);more.setText("⋮");more.setContentDescription("App menu");more.setTag("session-menu");SessionStyle.button(more,false);more.setOnClickListener(v->menu.run());
        top.addView(more,new LayoutParams(SessionStyle.dp(a,48),SessionStyle.dp(a,48)));rail.addView(top);
        summary=new Button(a);summary.setTag("session-summary");SessionStyle.button(summary,false);summary.setTextSize(12);summary.setMaxLines(3);summary.setGravity(Gravity.START|Gravity.CENTER_VERTICAL);
        summary.setContentDescription("Vehicle and security status — tap for details");summary.setOnClickListener(v->details.run());
        LayoutParams sp=new LayoutParams(-1,SessionStyle.dp(a,64));sp.topMargin=SessionStyle.dp(a,6);rail.addView(summary,sp);
        addView(rail);addView(display);
        int pad=SessionStyle.dp(a,8);setPadding(pad,pad,pad,pad);
        setOnApplyWindowInsetsListener((v,i)->{v.setPadding(pad+i.getSystemWindowInsetLeft(),pad+i.getSystemWindowInsetTop(),pad+i.getSystemWindowInsetRight(),pad+i.getSystemWindowInsetBottom());return i;});
    }
    public void summary(String text){if(!text.contentEquals(summary.getText()))summary.setText(text);}
    public void addLaunch(View launch){LayoutParams p=new LayoutParams(-1,SessionStyle.dp(activity,48));p.topMargin=SessionStyle.dp(activity,6);rail.addView(launch,p);}
    public void toggleExpanded(){expanded=!expanded;rail.setVisibility(expanded?GONE:VISIBLE);requestLayout();}
    public boolean expanded(){return expanded;}
    @Override protected void onMeasure(int w,int h){
        int width=MeasureSpec.getSize(w),height=MeasureSpec.getSize(h);
        boolean landscape=!expanded&&width>=SessionStyle.dp(activity,720)&&width>height;
        setOrientation(landscape?HORIZONTAL:VERTICAL);
        LayoutParams rp=new LayoutParams(landscape?SessionStyle.dp(activity,248):-1,landscape?-1:-2);
        LayoutParams cp=new LayoutParams(landscape?0:-1,landscape?-1:0,1);
        if(landscape)cp.leftMargin=SessionStyle.dp(activity,8);else if(!expanded)cp.topMargin=SessionStyle.dp(activity,6);
        rail.setLayoutParams(rp);controls.setLayoutParams(cp);super.onMeasure(w,h);
    }
}
