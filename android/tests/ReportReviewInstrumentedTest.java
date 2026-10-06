// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.content.pm.ActivityInfo;
import android.os.*;
import android.widget.*;
import androidx.lifecycle.ViewModelProvider;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicReference;
import java.util.zip.*;
import org.json.JSONObject;

/** Real dialog/ViewModel rotation; synthetic receipts only. Never sends a network request. */
public final class ReportReviewInstrumentedTest extends Instrumentation {
    static void check(boolean yes,String why){if(!yes)throw new AssertionError(why);}
    private final AtomicReference<SupportReportActivity> current=new AtomicReference<>();
    private Bundle saved;
    private Application app;
    private java.io.File root;
    private Application.ActivityLifecycleCallbacks callbacks;
    public void onCreate(Bundle b){super.onCreate(b);start();}
    interface Condition {boolean test()throws Exception;}
    private void await(Condition condition,String why)throws Exception {
        long until=SystemClock.elapsedRealtime()+8000;
        while(SystemClock.elapsedRealtime()<until){waitForIdleSync();if(condition.test())return;SystemClock.sleep(50);}throw new AssertionError(why);
    }
    private ReportReviewModel model(){if(Looper.myLooper()==Looper.getMainLooper())return new ViewModelProvider(current.get()).get(ReportReviewModel.class);final ReportReviewModel[] m={null};runOnMainSync(()->m[0]=new ViewModelProvider(current.get()).get(ReportReviewModel.class));return m[0];}
    private ReportReviewDialog dialog(){if(Looper.myLooper()==Looper.getMainLooper())return (ReportReviewDialog)current.get().getSupportFragmentManager().findFragmentByTag(ReportReviewDialog.TAG);final ReportReviewDialog[] d={null};runOnMainSync(()->d[0]=(ReportReviewDialog)current.get().getSupportFragmentManager().findFragmentByTag(ReportReviewDialog.TAG));return d[0];}
    private static <T> T field(Object object,String name,Class<T> type)throws Exception {java.lang.reflect.Field f=object.getClass().getDeclaredField(name);f.setAccessible(true);return type.cast(f.get(object));}
    private void checkVisibleFooter(){runOnMainSync(()->{try{
        android.graphics.Rect window=new android.graphics.Rect();dialog().requireDialog().getWindow().getDecorView().getWindowVisibleDisplayFrame(window);
        for(String name:new String[]{"consent","send","options","keep"}){
            android.view.View view=field(dialog(),name,android.view.View.class);android.graphics.Rect visible=new android.graphics.Rect();
            check(view.getGlobalVisibleRect(visible)&&visible.height()>=view.getHeight()-2&&visible.width()>=view.getWidth()-2,"Clipped review control: "+name);
            int[] location=new int[2];view.getLocationOnScreen(location);
            android.graphics.Rect screen=new android.graphics.Rect(location[0],location[1],location[0]+view.getWidth(),location[1]+view.getHeight());
            check(window.contains(screen),"Review control outside window: "+name+" "+screen+" vs "+window);
        }
    }catch(Exception error){throw new AssertionError(error);}});}
    private void replace(File file,String body)throws Exception {
        File tmp=new File(file.getParentFile(),"test-repack.tmp");
        try(ZipOutputStream zip=new ZipOutputStream(new FileOutputStream(tmp))){zip.putNextEntry(new ZipEntry("diagnostics.json"));zip.write(body.getBytes(StandardCharsets.UTF_8));zip.closeEntry();}
        Files.move(tmp.toPath(),file.toPath(),StandardCopyOption.REPLACE_EXISTING);
    }
    public void onStart(){Bundle result=new Bundle();int code=-1;
        try{
            app=(Application)getTargetContext().getApplicationContext();root=new File(app.getFilesDir(),"support-reports");
            // Isolated harness package. Never run this destructive fixture setup against a public install.
            check(app.getPackageName().equals("com.opensaab.beta.diagnostics"),"Disposable harness required");
            remove(root);root.mkdirs();
            String exact="{\n  \"format\": 1, \"description\": \"Synthetic reviewed é report\"\n}\n";
            File file=SupportReports.saveText(app,exact);ReportArtifacts.Artifact a=ReportArtifacts.read(app,file.getName(),null);
            check(Arrays.equals(a.bytes(),exact.getBytes(StandardCharsets.UTF_8)),"JSON bytes reformatted");
            check(a.hash.equals(ReportArtifacts.digest(exact.getBytes(StandardCharsets.UTF_8))),"Wrong body hash");
            String fake="OS-000000000000000000000001";ReportArtifacts.recordReceipt(app,a,fake);
            replace(file,exact);check(ReportArtifacts.receipt(app,ReportArtifacts.read(app,a.id,a.hash)).equals(fake),"ZIP repacking broke JSON receipt");
            replace(file,exact+" ");
            try{ReportArtifacts.recordReceipt(app,a,fake);throw new AssertionError("Changed file recorded");}catch(IOException expected){}
            check(ReportArtifacts.receipt(app,ReportArtifacts.read(app,a.id,null)).isEmpty(),"Changed JSON inherits receipt");
            replace(file,exact);ReportArtifacts.pruneReceipt(app,a.id);ReportArtifacts.remember(app,a,false);
            callbacks=new Application.ActivityLifecycleCallbacks(){
                public void onActivityResumed(Activity activity){if(activity instanceof SupportReportActivity)current.set((SupportReportActivity)activity);}
                public void onActivitySaveInstanceState(Activity activity,Bundle state){if(activity instanceof SupportReportActivity)saved=new Bundle(state);}
                public void onActivityCreated(Activity activity,Bundle b){}public void onActivityStarted(Activity activity){}public void onActivityPaused(Activity activity){}public void onActivityStopped(Activity activity){}public void onActivityDestroyed(Activity activity){}
            };app.registerActivityLifecycleCallbacks(callbacks);
            startActivitySync(new Intent(app,SupportReportActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
            ReportReviewModel first=model();runOnMainSync(first::openReview);await(()->dialog()!=null&&dialog().getDialog()!=null,"Review dialog absent");
            CheckBox consent=field(dialog(),"consent",CheckBox.class);Button send=field(dialog(),"send",Button.class);
            check(!consent.isChecked()&&!send.isEnabled(),"Send enabled without consent");
            runOnMainSync(first::send);check(!first.busy,"Unconsented send started");
            runOnMainSync(consent::performClick);check(first.consent&&first.canSend(),"Checkbox does not gate Send");
            for(int orientation:new int[]{ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE,ActivityInfo.SCREEN_ORIENTATION_PORTRAIT}){
                SupportReportActivity before=current.get();runOnMainSync(()->before.setRequestedOrientation(orientation));
                await(()->current.get()!=before&&!current.get().isDestroyed()&&dialog()!=null&&dialog().getDialog()!=null,"Rotation did not restore review");
                check(model()==first,"ViewModel not retained through rotation");check(first.consent&&first.artifact.hash.equals(a.hash)&&first.artifact.text.equals(exact),"Rotation lost exact review/consent");
                checkVisibleFooter();
                check(field(dialog(),"consent",CheckBox.class).isChecked()&&field(dialog(),"send",Button.class).isEnabled(),"Rotated consent UI wrong");
            }
            check(saved!=null&&saved.getString("review_id").equals(a.id)&&saved.getString("review_hash").equals(a.hash),"Reviewed ID/hash not saved");
            check(!saved.containsKey("consent")&&!saved.containsKey("upload_consent"),"Consent saved in Bundle");
            ReportReviewModel[] cold={null};runOnMainSync(()->{cold[0]=new ReportReviewModel(app);cold[0].initialize(saved.getString("review_id"),saved.getString("review_hash"),saved.getBoolean("review_open"));});
            check(!cold[0].consent&&cold[0].reviewOpen&&cold[0].artifact.hash.equals(a.hash),"New model inherited consent or lost exact bytes");
            runOnMainSync(()->dialog().requireDialog().cancel());await(()->!first.reviewOpen&&dialog()==null,"Dismissal did not close review");check(!first.consent,"Dismissal retained consent");
            runOnMainSync(first::openReview);await(()->dialog()!=null,"Reopen review absent");check(!first.consent,"Reopen inherited consent");
            ReportArtifacts.recordReceipt(app,a,fake);runOnMainSync(()->current.get().finish());waitForIdleSync();
            SupportReportActivity previous=current.get();startActivitySync(new Intent(app,SupportReportActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            await(()->current.get()!=previous&&dialog()!=null&&dialog().getDialog()!=null,"Cold launch did not restore sent review");
            ReportReviewModel restored=model();check(restored.receipt.equals(fake)&&!restored.canSend()&&!restored.consent,"Sent report offered again");
            check(field(dialog(),"send",Button.class).getVisibility()==android.view.View.GONE&&field(dialog(),"consent",CheckBox.class).getVisibility()==android.view.View.GONE,"Sent controls visible");
            runOnMainSync(restored::send);check(!restored.busy,"Sent report retransmission started");
            runOnMainSync(()->{restored.closeReview();restored.prepare("Synthetic new incident");});await(()->!restored.busy&&restored.reviewOpen,"New report not prepared");
            check(!restored.artifact.id.equals(a.id)&&restored.receipt.isEmpty()&&!restored.consent,"New incident inherited delivery/consent");
            // Exercise the actual Send button and Activity completion with a local fake uploader.
            java.lang.reflect.Field uploadField=ReportReviewModel.class.getDeclaredField("uploader");uploadField.setAccessible(true);
            final int[] buttonUploads={0};
            uploadField.set(restored,(ReportReviewModel.Uploader)body->{buttonUploads[0]++;throw new IOException("Synthetic upload failure");});
            runOnMainSync(()->{restored.confirm(true);try{field(dialog(),"send",Button.class).performClick();}catch(Exception e){throw new AssertionError(e);}});
            await(()->!restored.busy,"Failed button upload stalled");
            check(!current.get().isFinishing()&&dialog()!=null&&restored.reviewOpen&&restored.receipt.isEmpty(),"Failed upload dismissed the report");
            uploadField.set(restored,(ReportReviewModel.Uploader)body->{buttonUploads[0]++;return fake;});
            SupportReportActivity sentHost=current.get();
            runOnMainSync(()->{try{field(dialog(),"send",Button.class).performClick();}catch(Exception e){throw new AssertionError(e);}});
            await(()->sentHost.isFinishing(),"Successful Send did not dismiss report screen");
            check(buttonUploads[0]==2&&ReportArtifacts.receipt(app,restored.artifact).equals(fake),"Unexpected retry or lost delivery receipt");
            startActivitySync(new Intent(app,SupportReportActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            await(()->current.get()!=sentHost,"Report reopen failed");
            check(!model().reviewOpen&&!model().canSend()&&model().receipt.equals(fake),"Reopen offered duplicate send");
            runOnMainSync(()->model().prepare("Synthetic subsequent incident"));
            await(()->!model().busy&&model().reviewOpen,"Subsequent report missing");
            final ReportReviewModel later=model();
            // A lost/failed reply must remain unconfirmed, never "Already sent" with an empty receipt.
            final byte[][] captured={null};final int[] attempts={0};ReportReviewModel[] failed={null},success={null};
            runOnMainSync(()->{failed[0]=new ReportReviewModel(app,body->{captured[0]=body;attempts[0]++;throw new IOException("Synthetic lost response");});
                failed[0].initialize(later.artifact.id,later.artifact.hash,true);failed[0].confirm(true);failed[0].send();});
            await(()->!failed[0].busy,"Failed reply test stalled");
            check(failed[0].receipt.isEmpty()&&!failed[0].notice.contains("Already sent")&&failed[0].reviewOpen&&failed[0].canSend(),"Failed reply fabricated delivery or lost explicit retry");
            check(attempts[0]==1&&Arrays.equals(captured[0],later.artifact.bytes()),"Body changed or automatic retry");
            runOnMainSync(()->{success[0]=new ReportReviewModel(app,body->{check(Arrays.equals(body,later.artifact.bytes()),"Success body changed");return fake;});
                success[0].initialize(later.artifact.id,later.artifact.hash,true);success[0].confirm(true);success[0].send();});
            await(()->!success[0].busy,"Confirmed reply test stalled");
            check(!success[0].reviewOpen&&success[0].deliveryCompleted,"Confirmed send did not close review");
            check(success[0].receipt.equals(fake)&&!success[0].consent&&!success[0].canSend()&&ReportArtifacts.receipt(app,later.artifact).equals(fake),"Successful model delivery not bound to exact body");
            ReportArtifacts.pruneReceipt(app,later.artifact.id);
            // Changed local bytes invalidate consent before any network call can begin.
            ReportArtifacts.Artifact changed=later.artifact;replace(changed.file,changed.text+" ");
            runOnMainSync(()->{later.confirm(true);later.send();});await(()->!later.busy,"Changed file check stalled");
            check(!later.consent&&later.artifact==null&&!later.reviewOpen,"Changed file retained consent/review");
            // Bounded retention removes the matching receipt, not just the ZIP.
            file.setLastModified(1);ReportArtifacts.recordReceipt(app,a,fake);
            for(int n=0;n<9;n++)SupportReports.saveText(app,"{\"description\":\"Synthetic pruning "+n+"\"}");
            check(!file.exists()&&!new File(root,"uploaded/"+a.id+".json").exists(),"Pruned receipt survived");
            for(String id:new String[]{"../card.bin","android_support_bad.zip"})try{ReportArtifacts.read(app,id,null);throw new AssertionError("Unsafe identity accepted");}catch(IOException expected){}
            result.putString("stream","PASS: actual Send success dismisses Activity and review with durable receipt; failure stays open and retry remains explicit; cold reopen avoids duplicate send; exact UTF-8 JSON, repacked ZIP receipt, altered-file rejection, real portrait/landscape DialogFragment restoration with same ViewModel consent, cold-model consent reset, dismissal/reopen reset, sent cold restore/hidden controls, new incident reset, retention marker pruning, path validation. Synthetic receipts only; no network or adapter.\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(app!=null&&callbacks!=null)app.unregisterActivityLifecycleCallbacks(callbacks);SupportReportActivity a=current.get();if(a!=null)runOnMainSync(a::finish);finish(code,result);}
    }
    static void remove(File f){File[] kids=f.listFiles();if(kids!=null)for(File k:kids)remove(k);f.delete();}
}
