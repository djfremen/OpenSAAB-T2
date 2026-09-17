// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.widget.*;
import android.view.Gravity;

/** Shared in-app use of the approved launcher identity. */
public final class BrandHeader extends LinearLayout {
    public BrandHeader(Context context,String title){
        this(context,title,false);
    }
    public BrandHeader(Context context,String title,boolean compact){
        super(context);setGravity(Gravity.CENTER_VERTICAL);setBackgroundColor(0xff071525);
        if(!compact && AppBuildProfile.isHeadunit32(context)) title += " · development";
        int pad=Math.round((compact?4:8)*getResources().getDisplayMetrics().density);setPadding(pad,pad,pad,pad);
        ImageView logo=new ImageView(context);logo.setImageDrawable(context.getApplicationInfo().loadIcon(context.getPackageManager()));
        logo.setContentDescription("OpenSAAB logo");int size=Math.round((compact?32:48)*getResources().getDisplayMetrics().density);
        addView(logo,new LayoutParams(size,size));
        TextView name=new TextView(context);name.setText(title);name.setTextSize(compact?16:20);if(compact){name.setSingleLine(true);name.setEllipsize(android.text.TextUtils.TruncateAt.END);}name.setTextColor(0xffeaf4f7);name.setPadding(pad,0,0,0);
        LinearLayout labels=new LinearLayout(context);labels.setOrientation(LinearLayout.VERTICAL);
        labels.addView(name,new LayoutParams(-1,-2));
        TextView architecture=new TextView(context);architecture.setTextSize(12);architecture.setTextColor(0xffb8d9e8);architecture.setPadding(pad,0,0,0);
        architecture.setText(EmulatorArchitecture.label(new java.io.File(context.getApplicationInfo().nativeLibraryDir,"libtech2_emu.so")));
        if(!"com.opensaab.checker".equals(context.getPackageName())) labels.addView(architecture,new LayoutParams(-1,-2));
        addView(labels,new LayoutParams(0,-2,1));
    }
}
