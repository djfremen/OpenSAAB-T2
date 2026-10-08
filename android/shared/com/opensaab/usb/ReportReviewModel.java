// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Application;
import android.os.Handler;
import android.os.Looper;
import androidx.lifecycle.AndroidViewModel;
import androidx.lifecycle.MutableLiveData;
import java.util.concurrent.*;
import org.json.JSONObject;

/** Host-scoped rotation state. Affirmative consent is NEVER written to saved state. */
public final class ReportReviewModel extends AndroidViewModel {
    public ReportArtifacts.Artifact artifact;
    public boolean consent,reviewOpen,busy;
    public String receipt="",notice="";
    public final MutableLiveData<Integer> changes=new MutableLiveData<>(0);
    private boolean initialized,cleared;
    private int revision;
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    interface Uploader {String send(byte[] body)throws Exception;}
    private final Uploader uploader;
    public ReportReviewModel(Application app){this(app,SupportUpload::send);}
    ReportReviewModel(Application app,Uploader uploader){super(app);this.uploader=uploader;}
    public void initialize(String id,String hash,boolean open){
        if(initialized)return;initialized=true;consent=false;
        if(id==null&&!new java.io.File(getApplication().getFilesDir(),"support-reports/review-state.json").isFile())return;
        try{
            if(id==null){JSONObject saved=ReportArtifacts.remembered(getApplication());id=saved.getString("saved_id");hash=saved.getString("report_sha256");open=saved.optBoolean("review_open",false);}
            if(hash==null||!hash.matches("[a-f0-9]{64}"))throw new java.io.IOException("Missing report hash");
            artifact=ReportArtifacts.read(getApplication(),id,hash);reviewOpen=open;
            receipt=ReportArtifacts.receipt(getApplication(),artifact);
            notice=receipt.isEmpty()?"Saved report restored locally. Review and confirm before uploading.":"Already sent. Receipt: "+receipt;
        }catch(java.io.FileNotFoundException absent){}
        catch(Exception invalid){notice="The saved review is unavailable or changed. Prepare a new report; nothing was regenerated or uploaded.";}
    }
    public void changed(){changes.setValue(++revision);}
    public boolean canSend(){return artifact!=null&&reviewOpen&&consent&&!busy&&receipt.isEmpty();}
    public void confirm(boolean value){consent=value&&artifact!=null&&reviewOpen&&receipt.isEmpty()&&!busy;changed();}
    public void openReview(){if(artifact==null||busy)return;consent=false;reviewOpen=true;remember();changed();}
    public void closeReview(){consent=false;reviewOpen=false;remember();changed();}
    private void remember(){if(artifact!=null)try{ReportArtifacts.remember(getApplication(),artifact,reviewOpen);}catch(Exception failed){notice="Could not save review restoration state. Your report file is still kept locally.";}}
    public void prepare(String description){prepare(description,SupportReports.notesProvided(description));}
    public void prepare(String description,boolean notes){
        if(busy)return;final ReportArtifacts.Artifact prior=artifact;final String priorReceipt=receipt;busy=true;artifact=null;consent=false;receipt="";reviewOpen=false;notice="Preparing report…";changed();
        worker.execute(()->{
            ReportArtifacts.Artifact next=null;String error=null;
            try{JSONObject report=SupportReports.collect(getApplication(),description,notes);String json=report.toString(2);
                java.io.File file=SupportReports.saveText(getApplication(),json);
                next=ReportArtifacts.read(getApplication(),file.getName(),null);
                try{ReportArtifacts.remember(getApplication(),next,true);}catch(Exception stateFailure){error="Report saved, but review restoration state could not be saved. Keep a local copy before closing.";}
            }catch(Exception failed){error="Could not prepare the report. Check free space and try again.";}
            final ReportArtifacts.Artifact frozen=next;final String problem=error;
            ui.post(()->{if(cleared)return;busy=false;if(frozen!=null){artifact=frozen;reviewOpen=true;notice=problem==null?"Review the saved report before sending.":problem;}else {artifact=prior;receipt=priorReceipt;notice=problem;}changed();});
        });
    }
    public void send(){
        if(!canSend())return;
        final ReportArtifacts.Artifact frozen=artifact;busy=true;notice="Sending to OpenSAAB… Your local copy will be kept.";changed();
        worker.execute(()->{
            String number=null,problem=null;boolean recorded=false,validated=false;
            try{
                synchronized(ReportArtifacts.class){
                    ReportArtifacts.Artifact current=ReportArtifacts.read(getApplication(),frozen.id,frozen.hash);validated=true;
                    String existing=ReportArtifacts.receipt(getApplication(),current);
                    number=existing.isEmpty()?uploader.send(frozen.bytes()):existing;
                    try{ReportArtifacts.recordReceipt(getApplication(),frozen,number);recorded=true;}catch(Exception failed){}
                }
            }catch(java.net.UnknownHostException|java.net.SocketTimeoutException e){problem="Could not confirm upload. Check your connection and retry explicitly. The same report is kept locally.";}
            catch(java.io.IOException e){problem=e.getMessage();}
            catch(Exception e){problem="Could not confirm upload. Your local report is kept; retry explicitly.";}
            final String received=number,error=problem;final boolean persisted=recorded,invalidArtifact=!validated;
            ui.post(()->{if(cleared)return;busy=false;
                if(received!=null){receipt=received;consent=false;notice="Already sent. Receipt: "+receipt+(persisted?"":"\nCould not save the local delivery record. Keep this receipt; reopening may offer the report again.");}
                else {notice=error;if(invalidArtifact){artifact=null;reviewOpen=false;consent=false;}}changed();});
        });
    }
    @Override protected void onCleared(){cleared=true;worker.shutdown();}
}
