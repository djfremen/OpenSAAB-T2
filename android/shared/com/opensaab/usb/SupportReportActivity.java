// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.*;
import android.net.Uri;
import android.os.Bundle;
import android.widget.*;
import java.io.File;
import org.json.JSONObject;

/** User reviews a frozen report before uploading or exporting it. */
public final class SupportReportActivity extends Activity {
    private Button prepare;private EditText description;
    private static final int SAVE_REPORT = 41;
    private File pendingExport;
    private AlertDialog sending;
    public void onCreate(Bundle b){super.onCreate(b);if(b!=null){String name=b.getString("export_report");if(name!=null && name.matches("android_support_[a-f0-9-]+\\.zip"))pendingExport=new File(new File(getFilesDir(),"support-reports"),name);}ScrollView scroll=new ScrollView(this);LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setPadding(24,24,24,24);scroll.addView(root);root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(24,i.getSystemWindowInsetTop()+24,24,i.getSystemWindowInsetBottom()+24);return i;});
        TextView title=new TextView(this);title.setText("Report a problem");title.setTextSize(24);root.addView(title);
        TextView info=new TextView(this);info.setText("Tell us what happened and what you expected. We’ll prepare app/device details, connection stages and failure reasons, recent adapter event counts and available error information. Review the report, then send it privately to OpenSAAB over the internet. No account or email app is needed. Reports are stored in our Cloudflare storage and available to the project administrator, not posted to GitHub. Reports stay until an administrator deletes them. Your description is included; remove anything private you do not want to send. You can also copy or save a report without sending it.\n\nRaw traffic, VIN, security data and screenshots are excluded. Please avoid entering private codes in your description.\n\nThis does not upload anything automatically.");root.addView(info);
        try { JSONObject receipt=new JSONObject(new String(java.nio.file.Files.readAllBytes(new File(getFilesDir(),"last-support-upload.json").toPath()),java.nio.charset.StandardCharsets.UTF_8));
            String number=receipt.optString("report_id","");if(number.matches("OS-[a-f0-9]{24}")){TextView previous=new TextView(this);previous.setText("Last sent report: "+number);previous.setTextIsSelectable(true);root.addView(previous);}
        }catch(Exception ignored){}
        description=new EditText(this);description.setHint("What were you doing when the problem happened?");description.setMinLines(3);description.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(2000)});root.addView(description);
        prepare=new Button(this);prepare.setText("Prepare report");prepare.setOnClickListener(v->prepare());root.addView(prepare);Button back=new Button(this);back.setText("Back");back.setOnClickListener(v->finish());root.addView(back);setContentView(scroll);
    }
    private void prepare(){prepare.setEnabled(false);String text=description.getText().toString();new Thread(()->{try{JSONObject report=SupportReports.collect(this,text);File file=SupportReports.save(this,report);String formatted=report.toString(2);runOnUiThread(()->{if(isFinishing()||isDestroyed())return;prepare.setEnabled(true);TextView preview=new TextView(this);preview.setText(formatted);preview.setPadding(24,12,24,12);preview.setTextIsSelectable(true);ScrollView sc=new ScrollView(this);sc.addView(preview);new AlertDialog.Builder(this).setTitle("Review support report").setView(sc).setNegativeButton("Keep private",null).setNeutralButton("Other options",(d,w)->exportOptions(file,formatted)).setPositiveButton("Send to OpenSAAB",(d,w)->sendReport(file,formatted)).show();});}catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;prepare.setEnabled(true);new AlertDialog.Builder(this).setMessage("Could not prepare the report. Please check free space and try again.").setPositiveButton("OK",null).show();});}},"support-report").start();}
    private void sendReport(File file,String formatted){
        sending=new AlertDialog.Builder(this).setTitle("Sending private report")
            .setMessage("Sending to OpenSAAB… Your local copy will be kept.")
            .setCancelable(false).create();sending.show();
        new Thread(()->{
            String id=null;String error=null;
            try{
                id=SupportUpload.send(formatted);
                try{FirmwareStore.writeJson(new File(getFilesDir(),"last-support-upload.json"),new JSONObject()
                    .put("report_id",id).put("submitted_utc",java.time.Instant.now().toString()));}catch(Exception ignored){}
            }catch(java.net.UnknownHostException|java.net.SocketTimeoutException e){error="Could not reach OpenSAAB. Check your internet connection and try again. Your report is kept on this device.";}
            catch(java.io.IOException e){error=e.getMessage();}
            catch(Exception e){error="Could not send the report. Your local copy is kept; please try again.";}
            final String number=id,problem=error;
            runOnUiThread(()->{
                if(sending!=null){sending.dismiss();sending=null;}
                if(isFinishing()||isDestroyed())return;
                if(number!=null){
                    TextView text=new TextView(this);text.setText("Stored privately by OpenSAAB.\n\nReport number: "+number);text.setTextIsSelectable(true);text.setPadding(24,16,24,16);
                    new AlertDialog.Builder(this).setTitle("Report sent").setView(text).setPositiveButton("Done",null)
                        .setNeutralButton("Copy number",(d,w)->((ClipboardManager)getSystemService(CLIPBOARD_SERVICE)).setPrimaryClip(ClipData.newPlainText("OpenSAAB report number",number))).show();
                }else new AlertDialog.Builder(this).setTitle("Report not confirmed").setMessage(problem)
                    .setPositiveButton("Retry",(d,w)->sendReport(file,formatted)).setNeutralButton("Other options",(d,w)->exportOptions(file,formatted)).setNegativeButton("Keep private",null).show();
            });
        },"support-upload").start();
    }
    @Override protected void onDestroy(){if(sending!=null){sending.dismiss();sending=null;}super.onDestroy();}
    private void exportOptions(File file,String formatted){
        new AlertDialog.Builder(this).setTitle("Export support report")
            .setItems(new String[]{"Copy report text", "Save ZIP to a file", "Share / email"},(dialog,which)->{
                if(which==0){
                    ((ClipboardManager)getSystemService(CLIPBOARD_SERVICE)).setPrimaryClip(ClipData.newPlainText("OpenSAAB support report",formatted));
                    Toast.makeText(this,"Report copied. Paste it into your browser or message.",Toast.LENGTH_LONG).show();
                }else if(which==2){share(file);}else{
                    pendingExport=file;
                    try{startActivityForResult(new Intent(Intent.ACTION_CREATE_DOCUMENT)
                        .addCategory(Intent.CATEGORY_OPENABLE).setType("application/zip")
                        .putExtra(Intent.EXTRA_TITLE,file.getName()),SAVE_REPORT);}
                    catch(ActivityNotFoundException e){pendingExport=null;new AlertDialog.Builder(this).setMessage("This device has no file-saving app. Use Copy report text instead.").setPositiveButton("OK",null).show();}
                }
            }).setNegativeButton("Cancel",null).show();
    }
    @Override protected void onSaveInstanceState(Bundle state){
        if(pendingExport!=null)state.putString("export_report",pendingExport.getName());
        super.onSaveInstanceState(state);
    }
    @Override protected void onActivityResult(int request,int result,Intent data){
        super.onActivityResult(request,result,data);
        if(request!=SAVE_REPORT)return;
        File source=pendingExport;pendingExport=null;
        if(result!=RESULT_OK || data==null || data.getData()==null || source==null)return;
        Uri destination=data.getData();
        new Thread(()->{
            boolean saved=false;
            try(java.io.InputStream in=new java.io.FileInputStream(source);
                java.io.OutputStream out=getContentResolver().openOutputStream(destination,"wt")){
                if(out==null)throw new java.io.IOException("No output stream");
                byte[] buffer=new byte[8192];int count;
                while((count=in.read(buffer))!=-1)out.write(buffer,0,count);
                out.flush();saved=true;
            }catch(Exception ignored){saved=false;}
            final boolean ok=saved;
            runOnUiThread(()->{if(!isFinishing()&&!isDestroyed())Toast.makeText(this,ok?"Report saved":"Could not save the file. Your report is still kept in the app.",Toast.LENGTH_LONG).show();});
        },"support-export").start();
    }
    static Intent sharingIntent(Context context,File file){Uri uri=Uri.parse("content://"+context.getPackageName()+".support-reports/"+file.getName());Intent i=new Intent(Intent.ACTION_SEND).setType("application/zip").putExtra(Intent.EXTRA_SUBJECT,"OpenSAAB problem report").putExtra(Intent.EXTRA_STREAM,uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);i.setClipData(ClipData.newRawUri("Support report",uri));return i;}
    private void share(File file){Intent i=sharingIntent(this,file);try{startActivity(Intent.createChooser(i,"Share OpenSAAB support report"));}catch(ActivityNotFoundException e){Toast.makeText(this,"No sharing app is installed. Your report is saved in the app.",Toast.LENGTH_LONG).show();}}
}
