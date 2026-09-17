// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Instrumentation;
import android.os.Bundle;
import android.os.SystemClock;
import android.graphics.drawable.BitmapDrawable;
import android.widget.ImageView;
import java.io.File;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;

/** Runs against the real Android FileObserver/Looper/Bitmap APIs; never opens USB. */
public final class NativeLcdPumpInstrumentedTest extends Instrumentation {
    private ImageView view;
    private NativeLcdPump pump;
    private File root;
    public void onCreate(Bundle args) { super.onCreate(args); start(); }
    private void publish(File dir, int color) throws Exception {
        byte[] header="P6\n320 240\n255\n".getBytes("US-ASCII");
        byte[] bytes=new byte[230415];System.arraycopy(header,0,bytes,0,header.length);
        for(int i=header.length;i<bytes.length;i+=3){bytes[i]=(byte)(color>>16);bytes[i+1]=(byte)(color>>8);bytes[i+2]=(byte)color;}
        Files.write(new File(dir,"live.ppm.tmp").toPath(),bytes);
        Files.move(new File(dir,"live.ppm.tmp").toPath(),new File(dir,"live.ppm").toPath(),StandardCopyOption.ATOMIC_MOVE);
    }
    private int color() {
        int[] result={0};
        runOnMainSync(()->{if(view.getDrawable() instanceof BitmapDrawable)result[0]=((BitmapDrawable)view.getDrawable()).getBitmap().getPixel(0,0);});
        return result[0];
    }
    private long awaitColor(int wanted) throws Exception {
        long start=SystemClock.elapsedRealtime();
        while(SystemClock.elapsedRealtime()-start<3000){if(color()==wanted)return SystemClock.elapsedRealtime()-start;Thread.sleep(10);}
        throw new AssertionError("Frame not displayed: "+Integer.toHexString(wanted)+" actual="+Integer.toHexString(color()));
    }
    public void onStart() {
        Bundle result=new Bundle();int status=-1;
        try {
            root=new File(getTargetContext().getCacheDir(),"lcd-test-"+SystemClock.elapsedRealtime());
            File a=new File(root,"a"),b=new File(root,"b");if(!a.mkdirs() || !b.mkdir())throw new Exception("Test directories");
            runOnMainSync(()->{view=new ImageView(getTargetContext());pump=new NativeLcdPump(view);pump.setDirectory(a);});
            ArrayList<Long> latency=new ArrayList<>();
            for(int i=1;i<=8;i++){long began=SystemClock.elapsedRealtime();publish(a,0xff000000|i);awaitColor(0xff000000|i);latency.add(SystemClock.elapsedRealtime()-began);}
            // The one-second recovery poll must not decode/allocate unchanged frames.
            android.graphics.Bitmap[] same={null};runOnMainSync(()->same[0]=((BitmapDrawable)view.getDrawable()).getBitmap());
            Thread.sleep(1200);runOnMainSync(()->{if(same[0]!=((BitmapDrawable)view.getDrawable()).getBitmap())throw new AssertionError("Unchanged frame decoded again");});
            // A fast burst coalesces to the newest complete frame.
            for(int i=20;i<=60;i++)publish(a,0xff000000|i);
            awaitColor(0xff00003c);
            // Malformed frames must not replace the last verified display.
            Files.write(new File(a,"live.ppm").toPath(),new byte[]{'P','6'});Thread.sleep(150);
            if(color()!=0xff00003c)throw new AssertionError("Partial frame applied");
            publish(a,0xff112233);awaitColor(0xff112233);
            runOnMainSync(()->pump.setDirectory(b));
            publish(b,0xff445566);awaitColor(0xff445566);
            publish(a,0xffabcdef);Thread.sleep(150);
            if(color()!=0xff445566)throw new AssertionError("Old session frame applied");
            runOnMainSync(()->pump.close());publish(b,0xff123456);Thread.sleep(150);
            if(color()!=0xff445566)throw new AssertionError("Closed observer updated view");
            result.putString("stream","PASS: atomic publication, burst coalescing, malformed-frame retry, session switch, close. publish_to_view_ms="+latency+"; USB never opened\n");
        } catch(Throwable e) {status=0;result.putString("stream","FAIL: "+e+"\n");}
        finally {if(pump!=null)runOnMainSync(()->pump.close());if(root!=null)remove(root);finish(status,result);}
    }
    private void remove(File file){File[] children=file.listFiles();if(children!=null)for(File child:children)remove(child);file.delete();}
}
