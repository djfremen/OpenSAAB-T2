// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.*;
import android.net.Uri;
import android.os.Bundle;
import android.widget.*;
import java.io.File;
import org.json.JSONObject;

/** User reviews a frozen report before handing it to the Android share chooser. */
public final class SupportReportActivity extends Activity {
    private Button prepare;private EditText description;
    public void onCreate(Bundle b){super.onCreate(b);ScrollView scroll=new ScrollView(this);LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setPadding(24,24,24,24);scroll.addView(root);root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(24,i.getSystemWindowInsetTop()+24,24,i.getSystemWindowInsetBottom()+24);return i;});
        TextView title=new TextView(this);title.setText("Report a problem");title.setTextSize(24);root.addView(title);
        TextView info=new TextView(this);info.setText("Tell us what happened and what you expected. We’ll prepare app/device details, connection stages and failure reasons, recent adapter event counts and available error information. Review the report before sharing it by email or another app.\n\nRaw traffic, VIN, security data and screenshots are excluded. Please avoid entering private codes in your description.\n\nThis does not upload anything automatically.");root.addView(info);
        description=new EditText(this);description.setHint("What were you doing when the problem happened?");description.setMinLines(3);description.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(2000)});root.addView(description);
        prepare=new Button(this);prepare.setText("Prepare report");prepare.setOnClickListener(v->prepare());root.addView(prepare);Button back=new Button(this);back.setText("Back");back.setOnClickListener(v->finish());root.addView(back);setContentView(scroll);
    }
    private void prepare(){prepare.setEnabled(false);String text=description.getText().toString();new Thread(()->{try{JSONObject report=SupportReports.collect(this,text);File file=SupportReports.save(this,report);String formatted=report.toString(2);runOnUiThread(()->{if(isFinishing()||isDestroyed())return;prepare.setEnabled(true);TextView preview=new TextView(this);preview.setText(formatted);preview.setPadding(24,12,24,12);preview.setTextIsSelectable(true);ScrollView sc=new ScrollView(this);sc.addView(preview);new AlertDialog.Builder(this).setTitle("Review support report").setView(sc).setNegativeButton("Keep private",null).setPositiveButton("Share / email",(d,w)->share(file)).show();});}catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;prepare.setEnabled(true);new AlertDialog.Builder(this).setMessage("Could not prepare the report. Please check free space and try again.").setPositiveButton("OK",null).show();});}},"support-report").start();}
    static Intent sharingIntent(Context context,File file){Uri uri=Uri.parse("content://"+context.getPackageName()+".support-reports/"+file.getName());Intent i=new Intent(Intent.ACTION_SEND).setType("application/zip").putExtra(Intent.EXTRA_SUBJECT,"OpenSAAB problem report").putExtra(Intent.EXTRA_STREAM,uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);i.setClipData(ClipData.newRawUri("Support report",uri));return i;}
    private void share(File file){Intent i=sharingIntent(this,file);try{startActivity(Intent.createChooser(i,"Share OpenSAAB support report"));}catch(ActivityNotFoundException e){Toast.makeText(this,"No sharing app is installed. Your report is saved in the app.",Toast.LENGTH_LONG).show();}}
}
