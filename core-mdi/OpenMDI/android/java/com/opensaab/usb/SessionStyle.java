// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.content.res.ColorStateList;
import android.graphics.drawable.*;
import android.view.*;
import android.widget.*;
import java.util.ArrayList;

/** Consistent session controls, independent of device pixels and transport state. */
public final class SessionStyle {
    public static int dp(Context c,int n){return Math.round(n*c.getResources().getDisplayMetrics().density);}
    public static void button(Button b,boolean primary){
        Context c=b.getContext();b.setAllCaps(false);b.setTextSize(14);b.setGravity(Gravity.CENTER);
        b.setMinWidth(0);b.setMinimumWidth(0);b.setMinHeight(dp(c,48));b.setMinimumHeight(dp(c,48));
        b.setPadding(dp(c,8),dp(c,4),dp(c,8),dp(c,4));b.setMaxLines(2);
        b.setTextColor(new ColorStateList(new int[][]{new int[]{-android.R.attr.state_enabled},new int[]{}},new int[]{0xff7f8b99,0xffedf4fa}));
        GradientDrawable shape=new GradientDrawable();shape.setColor(primary?0xff205c83:0xff1b2b3b);
        shape.setCornerRadius(dp(c,8));shape.setStroke(dp(c,1),primary?0xff4382aa:0xff3b4c5e);
        b.setBackground(new RippleDrawable(ColorStateList.valueOf(0x557cbbe4),shape,null));
        b.setStateListAnimator(null);
    }
    public static void row(LinearLayout row){
        Context c=row.getContext();row.setBaselineAligned(false);
        for(int i=0;i<row.getChildCount();i++){
            View child=row.getChildAt(i);
            LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(0,-1,1);
            if(i>0)p.leftMargin=dp(c,8);child.setLayoutParams(p);
            if(child instanceof Button)button((Button)child,false);
        }
    }
    public static void stack(LinearLayout root){
        for(int i=0;i<root.getChildCount();i++){
            View child=root.getChildAt(i);LinearLayout.LayoutParams p=(LinearLayout.LayoutParams)child.getLayoutParams();
            if(i>0)p.topMargin=dp(root.getContext(),8);
            if(child instanceof Button){button((Button)child,false);p.width=-1;p.height=dp(root.getContext(),48);}
            child.setLayoutParams(p);
        }
    }
    /** Portrait details scroll when space/font size is limited; firmware keys stay outside. */
    public static void fitPortrait(LinearLayout root){fitPortrait(root,48);}
    public static void fitPortrait(LinearLayout root,int percent){
        if(root.getOrientation()!=LinearLayout.VERTICAL)return;
        Tech2Controls controls=null;ArrayList<View> details=new ArrayList<>();
        for(int i=0;i<root.getChildCount();i++){
            View v=root.getChildAt(i);if(v instanceof Tech2Controls)controls=(Tech2Controls)v;
            else if(!(v instanceof BrandHeader)&&!"session-shortcuts".equals(v.getTag()))details.add(v);
        }
        if(controls==null)return;
        LinearLayout content=new LinearLayout(root.getContext());content.setOrientation(LinearLayout.VERTICAL);
        for(View v:details){root.removeView(v);content.addView(v);}
        ScrollView scroll=new ScrollView(root.getContext()){
            @Override protected void onMeasure(int w,int h){
                int available=MeasureSpec.getSize(h);
                super.onMeasure(w,MeasureSpec.makeMeasureSpec(available*percent/100,MeasureSpec.AT_MOST));
            }
        };
        scroll.setTag("session-details-scroll");scroll.setFillViewport(false);scroll.addView(content);
        root.addView(scroll,1,new LinearLayout.LayoutParams(-1,-2));
    }
    private SessionStyle(){}
}
