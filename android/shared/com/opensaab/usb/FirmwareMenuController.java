// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.os.SystemClock;
import android.widget.TextView;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.*;

/** Reads captured firmware menus off the UI thread; never sends keys after stop/manual takeover. */
public final class FirmwareMenuController extends TextView {
    private final Activity activity;
    private final FirmwareMenuNavigator.Target target;
    private final Supplier<File> session;
    private final BooleanSupplier running;
    private final IntPredicate send;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final AtomicBoolean polling=new AtomicBoolean();
    private FirmwareMenuNavigator navigator;
    private File observed;
    private boolean cancelled,closed;
    public FirmwareMenuController(Activity a,FirmwareMenuNavigator.Target target,Supplier<File> session,BooleanSupplier running,IntPredicate send){
        super(a);activity=a;this.target=target;this.session=session;this.running=running;this.send=send;
        setTextSize(13);setTextColor(0xffb8d8f0);setText("Shortcut: "+target.label+" · waiting for firmware");
    }
    public boolean active(){return !closed&&!cancelled&&(navigator==null||navigator.active());}
    public void cancel(){cancelled=true;if(navigator!=null)navigator.cancel();setText("Automatic navigation stopped · use the firmware controls");}
    public void refresh(){
        File run=session.get();
        if(!active()||!running.getAsBoolean()||run==null||!polling.compareAndSet(false,true))return;
        try{worker.execute(()->{
            String screen="";VehicleIdentity vehicle=VehicleSession.read(new File(run,VehicleSession.FILE));
            try{File f=new File(run,"native-dtc-screen.txt");if(f.isFile()&&f.length()<8192)screen=new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8);}catch(IOException ignored){}
            final String text=screen;
            activity.runOnUiThread(()->{
                polling.set(false);if(!active()||!running.getAsBoolean()||session.get()!=run)return;
                if(observed!=run){observed=run;navigator=new FirmwareMenuNavigator(vehicle,target);}
                long now=SystemClock.elapsedRealtime();Integer key=navigator.next(text,now);
                if(key!=null&&send.test(key))navigator.sent(now);
                setText(navigator.hint());
            });
        });}catch(RejectedExecutionException e){polling.set(false);}
    }
    public void close(){closed=true;worker.shutdownNow();}
}
