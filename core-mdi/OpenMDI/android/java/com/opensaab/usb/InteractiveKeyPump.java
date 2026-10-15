// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.os.SystemClock;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

/** Offline input writer: wakes on input, never blocks the UI or framebuffer worker. */
public final class InteractiveKeyPump implements AutoCloseable {
    private static final class Key {
        final String command;
        final long queued=SystemClock.elapsedRealtime();
        Key(String command){this.command=command;}
        boolean expired(){return (command.equals("0x09")||command.equals("0x0c")) && SystemClock.elapsedRealtime()-queued>500;}
    }
    private final ArrayBlockingQueue<Key> queue=new ArrayBlockingQueue<>(16);
    private final File mailbox;
    private final Thread worker;
    private final Consumer<String> error;
    private volatile boolean closed;
    public InteractiveKeyPump(File mailbox,Consumer<String> error){
        this.mailbox=mailbox;this.error=error;
        worker=new Thread(this::run,"tech2-input");worker.start();
    }
    public boolean offer(String command){
        if(closed)return false;
        if(!command.equals("enter")&&!command.matches("0x[01][0-9a-f]"))throw new IllegalArgumentException("Invalid firmware key");
        Key key=new Key(command);
        boolean accepted=queue.offer(key);
        if(closed){queue.remove(key);return false;}
        if(accepted)android.util.Log.i("OpenSaabPerf","INPUT queued command="+command+" wall_ms="+System.currentTimeMillis());
        return accepted;
    }
    private void run(){
        try {
            while(!closed){
                Key key=queue.poll(250,TimeUnit.MILLISECONDS);
                if(key==null)continue;
                // One atomic mailbox, one producer. Never overwrite an unconsumed key.
                while(!closed && mailbox.exists() && !key.expired())Thread.sleep(5);
                synchronized(this){
                    if(closed)break;
                    if(key.expired()){android.util.Log.i("OpenSaabPerf","INPUT expired arrow age_ms="+(SystemClock.elapsedRealtime()-key.queued));continue;}
                    File pending=new File(mailbox+".tmp");
                    Files.write(pending.toPath(),(key.command+"\n").getBytes(StandardCharsets.US_ASCII));
                    if(closed){pending.delete();break;}
                    Files.move(pending.toPath(),mailbox.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
                    android.util.Log.i("OpenSaabPerf","INPUT published command="+key.command+" queue_ms="+(SystemClock.elapsedRealtime()-key.queued)+" wall_ms="+System.currentTimeMillis());
                }
            }
        }catch(InterruptedException stopped){Thread.currentThread().interrupt();}
        catch(Exception e){if(!closed)error.accept(e.toString());}
        finally{closed=true;queue.clear();}
    }
    /** UI-safe cancellation; the owning session joins before publishing stop. */
    public void cancel(){closed=true;queue.clear();worker.interrupt();}
    public void close(){cancel();if(Thread.currentThread()!=worker)try{worker.join(1000);}catch(InterruptedException e){Thread.currentThread().interrupt();}}
}
