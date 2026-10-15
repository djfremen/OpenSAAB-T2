// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.content.Intent;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.time.LocalDate;
import java.time.format.*;
import java.util.Locale;
import org.json.JSONObject;

/** Launch information from packaged metadata, before setup or diagnostics. */
public final class PreviewWelcome {
    public static String dateLine(Activity activity) {
        try(InputStream in=activity.getAssets().open("build.json")) {
            ByteArrayOutputStream out=new ByteArrayOutputStream();byte[] b=new byte[1024];int n;
            while((n=in.read(b))!=-1){if(out.size()+n>4096)throw new IOException("Build metadata limit");out.write(b,0,n);}
            JSONObject metadata=new JSONObject(new String(out.toByteArray(),StandardCharsets.UTF_8));
            if(!AppBuildProfile.installedVersion(activity).equals(metadata.getString("version_name")))throw new IOException("Version mismatch");
            String date=LocalDate.parse(metadata.getString("release_date")).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG).withLocale(Locale.getDefault()));
            return "Version date: "+date;
        }catch(Exception unavailable){return "Release date unavailable";}
    }
    private static TextView text(Activity a,String value,int size,String tag) {
        TextView t=new TextView(a);t.setText(value);t.setTextSize(size);t.setTextColor(0xffedf4fa);t.setTag(tag);
        t.setLayoutParams(new LinearLayout.LayoutParams(-1,-2));return t;
    }
    public static void show(Activity activity,Runnable continueIntoApp) {
        activity.setTitle(AppBuildProfile.programName(activity));
        ScrollView scroll=new ScrollView(activity);scroll.setFillViewport(true);scroll.setBackgroundColor(0xff0d1620);scroll.setTag("developer-preview-welcome");
        LinearLayout content=new LinearLayout(activity);content.setOrientation(LinearLayout.VERTICAL);
        int pad=SessionStyle.dp(activity,24);content.setPadding(pad,pad,pad,pad);content.setGravity(Gravity.CENTER_VERTICAL);
        content.addView(new BrandHeader(activity,AppBuildProfile.programName(activity),true));
        content.addView(text(activity,"Developer Preview",28,"preview-title"));
        content.addView(text(activity,AppBuildProfile.programName(activity),18,"preview-version"));
        content.addView(text(activity,dateLine(activity),16,"preview-release-date"));
        content.addView(text(activity,"Help improve OpenSAAB",20,"preview-help-title"));
        content.addView(text(activity,"Development is ongoing. If something goes wrong, use Report issue to tell us what happened and what you expected. Your feedback helps us improve OpenSAAB.",16,"preview-message"));
        content.addView(text(activity,"You can review your report before you send it.",14,"preview-review-note"));
        Button proceed=new Button(activity);proceed.setText("Continue");proceed.setTag("preview-continue");proceed.setOnClickListener(v->{proceed.setEnabled(false);continueIntoApp.run();});content.addView(proceed);
        Button report=new Button(activity);report.setText("Report issue");report.setTag("preview-report-issue");report.setOnClickListener(v->activity.startActivity(new Intent(activity,SupportReportActivity.class)));content.addView(report);
        SessionStyle.stack(content);SessionStyle.button(proceed,true);scroll.addView(content);
        activity.setContentView(scroll);
    }
    private PreviewWelcome(){}
}
