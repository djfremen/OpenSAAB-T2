// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.os.*;
import android.content.*;
import android.widget.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import org.json.JSONObject;

/** Transaction tests use an isolated cache directory, never the user's active card. */
public final class FirmwareInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean b,String message){if(!b)throw new AssertionError(message);}
    void main(Runnable action){Throwable[] error={null};runOnMainSync(()->{try{action.run();}catch(Throwable e){error[0]=e;}});if(error[0]!=null)throw new AssertionError(error[0]);}
    File card(File parent,String name,int value)throws Exception{
        File f=new File(parent,name);try(RandomAccessFile out=new RandomAccessFile(f,"rw")){out.setLength(FirmwareFiles.CARD_BYTES);out.write(new byte[]{'T','2',' ',' ',(byte)value});}return f;
    }
    public void onStart(){Bundle result=new Bundle();int code=-1;File dir=null;Activity activity=null;
        try{
            dir=Files.createTempDirectory(getTargetContext().getCacheDir().toPath(),"firmware-store-test-").toFile();FirmwareStore store=new FirmwareStore(dir);
            File a=card(dir,"a.bin",1),b=card(dir,"b.bin",2);String ah=FirmwareFiles.sha(a),bh=FirmwareFiles.sha(b);
            store.saveOriginal(a,"a","Program A",ah,()->false);store.saveOriginal(b,"b","Program B",bh,()->false);
            store.activate("a","Program A",ah,()->false);File active=new File(store.firmware,"card.bin");check(FirmwareFiles.sha(active).equals(ah),"First activation failed");
            try(RandomAccessFile writable=new RandomAccessFile(active,"rw")){writable.seek(0xfe0000);writable.write(42);}String workingHash=FirmwareFiles.sha(active);
            store.activate("b","Program B",bh,()->false);check(FirmwareFiles.sha(active).equals(bh),"Version switch failed");
            File[] backups=new File(store.library,"backups").listFiles();check(backups.length==1,"Working copy backup missing");
            check(FirmwareFiles.sha(new File(backups[0],"card.bin")).equals(workingHash),"SSA changes lost from backup");check(FirmwareFiles.sha(store.original("a")).equals(ah),"Downloaded original was changed");
            try{store.activate("a","Program A",ah,()->true);throw new AssertionError("Cancellation ignored");}catch(InterruptedIOException expected){}
            check(FirmwareFiles.sha(active).equals(bh),"Cancelled activation changed working card");
            JSONObject info=new JSONObject().put("id","b").put("label","Recovered B");
            FirmwareStore.writeJson(new File(store.firmware,"card-change.json"),new JSONObject().put("before_sha256",ah).put("after_sha256",bh).put("active",info));
            store.recover();check(store.active().getString("label").equals("Recovered B"),"Recovery did not finish metadata");
            FirmwareStore.writeJson(new File(store.firmware,"card-change.json"),new JSONObject().put("before_sha256",bh).put("after_sha256",ah).put("active",new JSONObject().put("id","a")));
            store.recover();check(FirmwareFiles.sha(active).equals(bh)&&store.active().getString("id").equals("b"),"Uncommitted change did not preserve old card");
            check(store.missing().contains("eprom.bin"),"Missing support files not reported");
            BundledSupport.ensure(getTargetContext().getAssets(),store,()->false);
            if(BundledSupport.available(getTargetContext().getAssets())){
            check(store.missing().isEmpty(),"Bundled support did not complete setup offline");
            String cardHash=FirmwareFiles.sha(active);File eprom=new File(store.firmware,"eprom.bin");
            try(RandomAccessFile out=new RandomAccessFile(eprom,"rw")){out.seek(1000);int original=out.read();out.seek(1000);out.write(original^1);}String custom=FirmwareFiles.sha(eprom);
            File candi=new File(store.firmware,"candi.bin");String ch=FirmwareFiles.sha(candi);check(candi.delete(),"Cannot simulate missing support file");
            BundledSupport.ensure(getTargetContext().getAssets(),store,()->false);
            check(FirmwareFiles.sha(candi).equals(ch),"Missing component not restored from APK");
            check(FirmwareFiles.sha(eprom).equals(custom),"Existing valid support profile overwritten");check(FirmwareFiles.sha(active).equals(cardHash),"Bundled support touched the working card");
            try(FileOutputStream out=new FileOutputStream(candi)){out.write(1);}
            BundledSupport.ensure(getTargetContext().getAssets(),store,()->false);check(FirmwareFiles.sha(candi).equals(ch),"Corrupt component not repaired");
            check(new File(store.library,"support-backups").listFiles().length>0,"Replaced component not backed up");
            }else{
                check(!new File(store.firmware,"eprom.bin").exists(),"Default APK silently installed EPROM");
                check(store.missingSupport().equals("eprom.bin, opsys.dwn, candi.bin"),"Missing support not explicit");
                File invalid=new File(dir,"invalid-eprom.bin");
                try(RandomAccessFile out=new RandomAccessFile(invalid,"rw")){out.setLength(262144);out.writeInt(0x20000);out.writeInt(0x100);}
                Map<String,File> inputs=new LinkedHashMap<>();inputs.put("eprom.bin",invalid);
                try{SupportFirmware.install(getTargetContext().getAssets(),store,inputs,()->false);throw new AssertionError("Unrecognized support hash accepted");}catch(IOException expected){}
                check(!new File(store.firmware,"eprom.bin").exists(),"Invalid support changed installation");
                try{SupportFirmware.install(getTargetContext().getAssets(),store,inputs,()->true);throw new AssertionError("Cancelled import accepted");}catch(InterruptedIOException expected){}
                check(FirmwareFiles.sha(active).equals(bh),"Support checks changed card");
            }
            activity=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.FirmwareActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();final Activity ui=activity;
            SystemClock.sleep(700);
            main(()->{
                android.view.ViewGroup root=(android.view.ViewGroup)ui.findViewById(android.R.id.content);Spinner spinner=findSpinner(root);
                check(spinner!=null && ((FirmwareCatalog.Entry)spinner.getAdapter().getItem(0)).id.equals(FirmwareCatalog.ENTRIES[0].id),"Default is not English Saab NAO");
                check(root.getHeight()>0,"Setup UI missing");
            });
            result.putString("stream","PASS: isolated activation, backup of modified working card, immutable originals, cancellation, interrupted activation recovery, support packaging profile checks, rejected unrecognized/cancelled imports in firmware-free profile, legacy repair checks when bundled, English NAO default; no vehicle or active image changed\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(activity!=null){Activity a=activity;main(a::finish);}if(dir!=null)try(java.util.stream.Stream<Path> paths=Files.walk(dir.toPath())){paths.sorted(Comparator.reverseOrder()).forEach(p->{try{Files.delete(p);}catch(IOException ignored){}});}catch(IOException ignored){}finish(code,result);}
    }
    Spinner findSpinner(android.view.View view){if(view instanceof Spinner && "Software version and language".contentEquals(view.getContentDescription()))return (Spinner)view;if(view instanceof android.view.ViewGroup){android.view.ViewGroup group=(android.view.ViewGroup)view;for(int i=0;i<group.getChildCount();i++){Spinner s=findSpinner(group.getChildAt(i));if(s!=null)return s;}}return null;}
}
