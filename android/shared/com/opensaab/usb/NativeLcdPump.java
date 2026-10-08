// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.graphics.Bitmap;
import android.os.FileObserver;
import android.os.Handler;
import android.os.Looper;
import android.widget.ImageView;
import java.io.File;
import java.nio.file.Files;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

/** Event-driven display observer; one decode worker and one pending UI frame. */
public final class NativeLcdPump implements AutoCloseable {
    private final java.util.function.Consumer<Bitmap> display;
    private final Handler ui = new Handler(Looper.getMainLooper());
    private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor(
        r -> new Thread(r, "tech2-lcd"));
    private final AtomicBoolean queued = new AtomicBoolean(), uiQueued = new AtomicBoolean();
    private final AtomicLong revision = new AtomicLong(), generation = new AtomicLong();
    private final AtomicReference<Frame> ready = new AtomicReference<>();
    private volatile boolean closed;
    private volatile File directory;
    private FileObserver observer;
    private String appliedStamp = "", lastError = ""; // worker thread only

    private static final class Frame {
        final File directory;
        final Bitmap bitmap;
        final String name;
        final long published, generation;
        Frame(File directory, Bitmap bitmap, String name, long published, long generation) {
            this.directory=directory; this.bitmap=bitmap; this.name=name; this.published=published; this.generation=generation;
        }
    }

    NativeLcdPump(ImageView view) { this(bitmap->{if(bitmap==null)view.setImageDrawable(null);else view.setImageBitmap(bitmap);}); }

    public NativeLcdPump(java.util.function.Consumer<Bitmap> display) {
        this.display = display;
        // Recovery for a missed filesystem event, not the normal refresh path.
        worker.scheduleWithFixedDelay(this::request, 0, 1, TimeUnit.SECONDS);
    }

    public synchronized void setDirectory(File directory) {
        if (closed) return;
        if (observer != null) observer.stopWatching();
        this.directory = directory;
        long epoch=generation.incrementAndGet();
        Frame pending=ready.getAndSet(null);if(pending!=null)pending.bitmap.recycle();
        Runnable clear=()->{if(!closed&&generation.get()==epoch)display.accept(null);};
        if(Looper.myLooper()==Looper.getMainLooper())clear.run();else ui.post(clear);
        revision.incrementAndGet();
        if (directory != null) {
            observer = new FileObserver(directory.getPath(), FileObserver.MOVED_TO | FileObserver.CLOSE_WRITE) {
                public void onEvent(int event, String path) {
                    if (path != null && path.endsWith(".ppm")) {
                        revision.incrementAndGet();
                        request();
                    }
                }
            };
            observer.startWatching();
        }
        request();
    }

    /** Invalidate queued decodes before clearing; disk evidence is untouched. */
    public void clear(){setDirectory(null);}
    public synchronized void clearDirectory(File ended){if(directory==ended)setDirectory(null);}

    private void request() {
        if (closed || !queued.compareAndSet(false, true)) return;
        try { worker.execute(() -> { queued.set(false); poll(); }); }
        catch (RejectedExecutionException stopped) { queued.set(false); }
    }

    private void poll() {
        long epoch=generation.get();
        File run = directory;
        if (closed || run == null) return;
        try {
            long observed = revision.get();
            File latest = new File(run, "live.ppm");
            if (!latest.isFile()) {
                File[] frames = run.listFiles((d, n) -> n.endsWith(".ppm"));
                if (frames == null || frames.length == 0) return;
                latest = frames[0];
                for (File f : frames) if (f.lastModified() > latest.lastModified()) latest = f;
            }
            long published = latest.lastModified();
            String stamp = latest.getPath() + ":" + published + ":" + latest.length() + ":" + observed;
            if (stamp.equals(appliedStamp) || latest.length() != 230415) return;
            int[] pixels = LcdFrame.decode(Files.readAllBytes(latest.toPath()));
            Bitmap bitmap = Bitmap.createBitmap(pixels, LcdFrame.WIDTH, LcdFrame.HEIGHT,
                Bitmap.Config.ARGB_8888);
            synchronized(this){
            if (closed || directory != run || generation.get()!=epoch) { bitmap.recycle(); return; }
            Frame prior = ready.getAndSet(new Frame(run, bitmap, latest.getName(), published, epoch));
            if (prior != null) prior.bitmap.recycle(); // never handed to ImageView
            appliedStamp = stamp;
            }
            present();
            lastError = "";
        } catch (Exception e) {
            String error = e.toString();
            if (!error.equals(lastError)) android.util.Log.w("OpenSaabPerf", "LCD retry: " + error);
            lastError = error;
        }
    }

    private void present() {
        if (!uiQueued.compareAndSet(false, true)) return;
        if (!ui.post(() -> {
            Frame frame = ready.getAndSet(null);
            if (frame != null) {
                if (!closed && directory == frame.directory && generation.get()==frame.generation) {
                    display.accept(frame.bitmap);
                    android.util.Log.i("OpenSaabPerf", "FRAME file=" + frame.name
                        + " published_ms=" + frame.published + " applied_ms=" + System.currentTimeMillis());
                } else frame.bitmap.recycle();
            }
            uiQueued.set(false);
            if (ready.get() != null) present();
        })) {
            uiQueued.set(false);
            Frame frame = ready.getAndSet(null);
            if (frame != null) frame.bitmap.recycle();
        }
    }

    public synchronized void close() {
        closed = true;
        directory = null;generation.incrementAndGet();
        if (observer != null) observer.stopWatching();
        worker.shutdownNow();
        Frame frame = ready.getAndSet(null);
        if (frame != null) frame.bitmap.recycle();
    }
}
