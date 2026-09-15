// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;
import android.widget.Button;
import android.widget.Toast;

/** Voluntary project support. No payment SDK, tracking, or automatic navigation. */
public final class ProjectSupport {
    // Shared Java is also compiled by the SDK-only builder without a generated R class.
    @android.annotation.SuppressLint("DiscouragedApi")
    private static String text(Activity activity, String name) {
        int id=activity.getResources().getIdentifier(name,"string",activity.getPackageName());
        return id==0?"":activity.getString(id);
    }

    public static void waitForSession(Activity activity) {
        Toast.makeText(activity,text(activity,"project_support_wait"),Toast.LENGTH_SHORT).show();
    }

    public static Button button(Activity activity, Runnable open) {
        Button button=new Button(activity);
        button.setText(text(activity,"project_support_button"));
        button.setAllCaps(false);
        button.setTextSize(13);
        button.setTextColor(0xffb6e8ff);
        button.setBackgroundTintList(android.content.res.ColorStateList.valueOf(0xff173247));
        button.setMinHeight(Math.round(48*activity.getResources().getDisplayMetrics().density));
        button.setTag("project-support");
        button.setOnClickListener(v->open.run());
        return button;
    }

    public static void show(Activity activity) {
        String configured=text(activity,"project_support_url").trim();
        Uri destination=Uri.parse(configured);
        boolean ready="https".equalsIgnoreCase(destination.getScheme())
            && destination.getHost()!=null && !destination.getHost().isEmpty()
            && destination.getUserInfo()==null;
        AlertDialog.Builder dialog=new AlertDialog.Builder(activity)
            .setTitle(text(activity,"project_support_title"))
            .setMessage(text(activity,"project_support_message")+"\n\n"
                +text(activity,ready?"project_support_external":"project_support_pending"))
            .setNegativeButton(text(activity,"project_support_close"),null);
        if(ready)dialog.setPositiveButton(text(activity,"project_support_donate"),(d,w)->{
            try { activity.startActivity(new Intent(Intent.ACTION_VIEW,destination)
                .addCategory(Intent.CATEGORY_BROWSABLE)); }
            catch(ActivityNotFoundException e) {
                Toast.makeText(activity,text(activity,"project_support_no_browser"),Toast.LENGTH_LONG).show();
            }
        });
        dialog.show();
    }

    private ProjectSupport() {}
}
