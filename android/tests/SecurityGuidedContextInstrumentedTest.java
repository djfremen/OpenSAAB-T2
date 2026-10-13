// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.os.*;
import android.view.View;
import android.widget.*;
import java.io.*;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
import org.json.JSONObject;

/** Reproduces publication of a session directory before its verified VIN file exists. */
public final class SecurityGuidedContextInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    void main(Runnable r){Throwable[] error={null};runOnMainSync(()->{try{r.run();}catch(Throwable e){error[0]=e;}});if(error[0]!=null)throw new AssertionError(error[0]);}
    void await(SecurityAccessView view,java.util.function.BooleanSupplier condition){long until=SystemClock.elapsedRealtime()+6000;
        while(SystemClock.elapsedRealtime()<until){boolean[] done={false};main(()->{view.refresh();done[0]=condition.getAsBoolean();});if(done[0])return;SystemClock.sleep(50);}throw new AssertionError("Guided collection did not recover when verified context arrived");}
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;SecurityAccessView view=null;
        try{
            check("com.opensaab.beta.diagnostics".equals(getTargetContext().getPackageName()),"Disposable harness required");
            File files=getTargetContext().getFilesDir(),card=new File(files,"firmware/card.bin");card.getParentFile().mkdirs();
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(SsaData.OFFSET);byte[] empty=new byte[SsaData.SIZE];Arrays.fill(empty,(byte)255);f.write(empty);}
            FirmwareStore.writeJson(new File(card.getParentFile(),"active.json"),new JSONObject().put("id",FirmwareCatalog.ENTRIES[0].id).put("sha256_at_activation",FirmwareCatalog.ENTRIES[0].imageSha));
            File run=Files.createTempDirectory(files.toPath(),"late-context-").toFile();Files.write(new File(run,"screen.txt").toPath(),"Main Menu\nF0: Diagnostics\nF1: Service".getBytes("UTF-8"));
            VehicleIdentity car=new VehicleIdentity("YS3FD49Y041000000",Instant.now().toString(),"synthetic fresh VIN",2004,"not_requested","","","","","");
            activity=startActivitySync(new Intent(getTargetContext(),ChipsoftUsbActivity.class).putExtra(SecurityCollectionMode.EXTRA,SecurityCollectionMode.GUIDED).putExtra(SecurityCollectionMode.PLATFORM_EXTRA,"9440").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final Activity host=activity;final ArrayList<Integer> keys=new ArrayList<>();final SecurityAccessView[] made={null};
            main(()->{made[0]=new SecurityAccessView(host,true,()->run,()->true,()->{},()->{},()->{});made[0].setMenuKey(k->{keys.add(k);return true;});});view=made[0];final SecurityAccessView delayed=view;
            await(delayed,()->delayed.getVisibility()==View.VISIBLE);main(()->check(keys.isEmpty(),"Missing identity sent menu input"));
            VehicleSession.save(new File(run,VehicleSession.FILE),car);
            await(delayed,()->!keys.isEmpty());
            main(()->{check(keys.size()==1&&keys.get(0)==0x18,"Late context did not select observed Diagnostics");check(delayed.navigating(),"Guidance remained cancelled after same-run context arrival");});
            Files.write(new File(run,"screen.txt").toPath(),"Turn Ignition key to LOCK position\nPress ENTER".getBytes("UTF-8"));
            await(delayed,()->((TextView)delayed.findViewWithTag("security-message")).getText().toString().contains("LOCK"));main(()->check(keys.size()==1,"Physical prompt auto-acknowledged"));
            main(delayed::close);
            File manual=Files.createTempDirectory(files.toPath(),"manual-late-context-").toFile();Files.write(new File(manual,"screen.txt").toPath(),"Main Menu\nF0: Diagnostics".getBytes("UTF-8"));
            main(()->{made[0]=new SecurityAccessView(host,true,()->manual,()->true,()->{},()->{},()->{});made[0].setMenuKey(k->{keys.add(k);return true;});});view=made[0];final SecurityAccessView taken=view;
            await(taken,()->taken.getVisibility()==View.VISIBLE);main(taken::manualNavigation);
            VehicleSession.save(new File(manual,VehicleSession.FILE),car);main(taken::refresh);SystemClock.sleep(200);main(taken::refresh);SystemClock.sleep(200);
            main(()->check(keys.size()==1&&!taken.navigating(),"Late identity undid manual takeover"));
            result.putString("stream","PASS: missing same-session VIN defers guidance; late verified context starts observed original-menu selection; physical key prompts remain manual; manual takeover before context arrival remains respected. No USB/API/real firmware/vehicle commands.\n");
        }catch(Throwable e){code=0;StringWriter trace=new StringWriter();e.printStackTrace(new PrintWriter(trace));result.putString("stream","FAIL: "+trace+"\n");}
        finally{if(view!=null){SecurityAccessView v=view;main(v::close);}if(activity!=null){Activity a=activity;main(a::finish);}finish(code,result);}
    }
}
