// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.os.*;
import java.io.File;
import java.nio.file.Files;
import java.util.concurrent.atomic.AtomicReference;

/** Real mailbox timing/backpressure; no emulator process or vehicle transport. */
public final class InteractiveKeyPumpInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args){super.onCreate(args);start();}
    private File root,box;private InteractiveKeyPump pump;
    private String take() throws Exception {
        long deadline=SystemClock.elapsedRealtime()+1500;
        while(!box.exists() && SystemClock.elapsedRealtime()<deadline)Thread.sleep(2);
        if(!box.exists())throw new AssertionError("Mailbox timeout");
        String text=new String(Files.readAllBytes(box.toPath()),"US-ASCII").trim();Files.delete(box.toPath());return text;
    }
    private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    public void onStart(){Bundle result=new Bundle();int status=-1;try{
        root=new File(getTargetContext().getCacheDir(),"input-test-"+SystemClock.elapsedRealtime());check(root.mkdir(),"mkdir");box=new File(root,"interactive-key.txt");
        AtomicReference<String> error=new AtomicReference<>();pump=new InteractiveKeyPump(box,error::set);
        long started=SystemClock.elapsedRealtime();pump.offer("0x09");check(take().equals("0x09"),"Arrow missing");long latency=SystemClock.elapsedRealtime()-started;
        // A busy mailbox is not overwritten. Stale arrows expire, action keys retain ordering.
        Files.write(box.toPath(),"busy\n".getBytes("US-ASCII"));pump.offer("0x0c");pump.offer("enter");pump.offer("0x01");
        Thread.sleep(650);check(new String(Files.readAllBytes(box.toPath()),"US-ASCII").equals("busy\n"),"Overwrote busy mailbox");Files.delete(box.toPath());
        check(take().equals("enter"),"Stale arrow delivered or ENTER lost");check(take().equals("0x01"),"EXIT reordered");
        Files.write(box.toPath(),"busy\n".getBytes("US-ASCII"));pump.offer("0x09");pump.close();Files.delete(box.toPath());Thread.sleep(50);
        check(!box.exists()&&!pump.offer("enter"),"Input after close");check(error.get()==null,"Writer error: "+error.get());
        result.putString("stream","PASS: immediate input, atomic mailbox, backpressure, expired arrows, action ordering and close; enqueue_to_mailbox_ms="+latency+"\n");
    }catch(Throwable e){status=0;result.putString("stream","FAIL: "+e+"\n");}finally{if(pump!=null)pump.close();if(root!=null){for(File f:root.listFiles())f.delete();root.delete();}finish(status,result);}}
}
