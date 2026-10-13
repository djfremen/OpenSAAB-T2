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
public final class SupportReportActivity extends androidx.fragment.app.FragmentActivity {
    private Button prepare;private EditText description,contact;private Spinner testContext;private CheckBox allowContact;
    private static final int SAVE_REPORT = 41;
    private File pendingExport;
    private ReportReviewModel review;
    private Button restoreReview;private TextView reportNotice;
    public void onCreate(Bundle b){super.onCreate(b);review=new androidx.lifecycle.ViewModelProvider(this).get(ReportReviewModel.class);review.initialize(b==null?null:b.getString("review_id"),b==null?null:b.getString("review_hash"),b!=null&&b.getBoolean("review_open",false));if(b!=null){String name=b.getString("export_report");if(name!=null && name.matches("android_support_[a-f0-9-]+\\.zip"))pendingExport=new File(new File(getFilesDir(),"support-reports"),name);}ScrollView scroll=new ScrollView(this);LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setPadding(24,24,24,24);scroll.addView(root);root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(24,i.getSystemWindowInsetTop()+24,24,i.getSystemWindowInsetBottom()+24);return i;});
        TextView title=new TextView(this);title.setText("Report a problem");title.setTextSize(24);root.addView(title);
        TextView info=new TextView(this);info.setText("Tell us what happened and what you expected. We’ll prepare app/device details, connection stages and failure reasons, recent adapter event counts, memory/storage information and available error information. New firmware sessions also record a small set of CPU, memory and display-file timing samples to help investigate slowness. Review the report, then send it privately to OpenSAAB over the internet. No account or email app is needed. Reports are stored in our Cloudflare storage and available to the project administrator, not posted to GitHub. Reports stay until an administrator deletes them. Your description is included; remove anything private you do not want to send. You can also copy or save a report without sending it.\n\nRaw traffic, VIN, security data and screenshots are excluded. Please avoid entering private codes in your description.\n\nThis does not upload anything automatically.");root.addView(info);
        try { JSONObject receipt=new JSONObject(new String(java.nio.file.Files.readAllBytes(new File(getFilesDir(),"last-support-upload.json").toPath()),java.nio.charset.StandardCharsets.UTF_8));
            String number=receipt.optString("report_id","");if(number.matches("OS-[a-f0-9]{24}")){TextView previous=new TextView(this);previous.setText("Last sent report: "+number);previous.setTextIsSelectable(true);root.addView(previous);}
        }catch(Exception ignored){}
        description=new EditText(this);description.setHint("What were you doing when the problem happened?");description.setMinLines(3);description.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(1700)});root.addView(description);if(b==null&&getIntent().getBooleanExtra("health_report",false))description.setText("Emulation stopped or appeared unresponsive. What I was doing: ");
        TextView contextLabel=new TextView(this);contextLabel.setText("Where were you testing?");root.addView(contextLabel);
        testContext=new Spinner(this);testContext.setAdapter(new ArrayAdapter<String>(this,android.R.layout.simple_spinner_dropdown_item,new String[]{"Not specified","In a vehicle","On a bench","Without an adapter"}));root.addView(testContext);
        contact=new EditText(this);contact.setHint("Optional email or forum handle for follow-up");contact.setSingleLine(true);contact.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(160)});root.addView(contact);
        allowContact=new CheckBox(this);allowContact.setText("Include my contact detail privately and allow OpenSAAB to contact me about this report");root.addView(allowContact);
        if(b!=null){description.setText(b.getString("report_description",""));contact.setText(b.getString("report_contact",""));allowContact.setChecked(b.getBoolean("report_allow_contact",false));testContext.setSelection(Math.max(0,Math.min(3,b.getInt("report_context",0))));}
        prepare=new Button(this);prepare.setText("Prepare report");prepare.setOnClickListener(v->prepare());root.addView(prepare);restoreReview=new Button(this);restoreReview.setText("Review saved report");restoreReview.setOnClickListener(v->review.openReview());root.addView(restoreReview);reportNotice=new TextView(this);reportNotice.setTextIsSelectable(true);root.addView(reportNotice);Button back=new Button(this);back.setText("Back");back.setOnClickListener(v->finish());root.addView(back);setContentView(scroll);review.changes.observe(this,ignored->renderReportState());
    }
    static String reportDescription(String description,int context,String contact,boolean consent){
        String[] labels={"Not specified","In a vehicle","On a bench","Without an adapter"};
        String detail=description==null?"":description;
        // Reserve room within the existing server's 2000-character description contract.
        if(detail.length()>1700)detail=detail.substring(0,1700);
        String result=detail+"\nTesting context: "+labels[Math.max(0,Math.min(3,context))];
        if(consent&&contact!=null&&!contact.trim().isEmpty()){
            String value=contact.trim();if(value.length()>160)value=value.substring(0,160);
            result+="\nPrivate follow-up contact (permission granted): "+value;
        }
        return result;
    }
    private void prepare(){
        ((android.view.inputmethod.InputMethodManager)getSystemService(INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(description.getWindowToken(),0);description.clearFocus();
        review.prepare(reportDescription(description.getText().toString(),testContext.getSelectedItemPosition(),contact.getText().toString(),allowContact.isChecked()),SupportReports.notesProvided(description.getText().toString()));
    }
    private void renderReportState(){
        if(prepare==null||isFinishing()||isDestroyed())return;
        if(review.deliveryCompleted){Toast.makeText(this,"Report sent. Receipt: "+review.receipt,Toast.LENGTH_LONG).show();finish();return;}
        prepare.setEnabled(!review.busy);restoreReview.setEnabled(review.artifact!=null&&!review.busy);reportNotice.setText(review.notice);
        if(getSupportFragmentManager().isStateSaved())return;
        androidx.fragment.app.Fragment current=getSupportFragmentManager().findFragmentByTag(ReportReviewDialog.TAG);
        if(review.reviewOpen&&review.artifact!=null&&current==null)new ReportReviewDialog().showNow(getSupportFragmentManager(),ReportReviewDialog.TAG);
        else if(!review.reviewOpen&&current instanceof ReportReviewDialog)((ReportReviewDialog)current).dismiss();
    }
    @Override protected void onPostResume(){super.onPostResume();renderReportState();}
    void exportOptions(File file,String formatted){
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
        if(review.artifact!=null){state.putString("review_id",review.artifact.id);state.putString("review_hash",review.artifact.hash);state.putBoolean("review_open",review.reviewOpen);}
        // Affirmative upload consent is ViewModel-only; never save it in this Bundle.
        state.putString("report_description",description.getText().toString());state.putString("report_contact",contact.getText().toString());state.putBoolean("report_allow_contact",allowContact.isChecked());state.putInt("report_context",testContext.getSelectedItemPosition());
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
