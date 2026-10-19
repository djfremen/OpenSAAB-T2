// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.os.FileObserver;
import java.io.File;
import java.util.*;

/** FileObserver uses one descriptor per path. Share it across LCD/menu consumers. */
public final class DirectoryWatch {
    public interface Listener {void changed(int event,String path);}
    private static final Map<String,Watch> watches=new HashMap<>();
    private static final class Watch {
        final String path;
        final Map<Registration,Listener> listeners=new LinkedHashMap<>();
        final FileObserver observer;
        Watch(String path){
            this.path=path;
            observer=new FileObserver(path,FileObserver.MOVED_TO|FileObserver.CLOSE_WRITE|
                    FileObserver.DELETE|FileObserver.DELETE_SELF|FileObserver.MOVE_SELF){
                public void onEvent(int event,String file){
                    List<Listener> copy;
                    synchronized(DirectoryWatch.class){if(watches.get(path)!=Watch.this)return;copy=new ArrayList<>(listeners.values());}
                    // Never hold the registry lock while invoking consumers.
                    for(Listener listener:copy)try{listener.changed(event,file);}catch(RuntimeException ignored){}
                }
            };
        }
    }
    public static final class Registration implements AutoCloseable {
        private Watch watch;
        private Registration(Watch watch){this.watch=watch;}
        public void close(){synchronized(DirectoryWatch.class){
            if(watch==null)return;
            watch.listeners.remove(this);
            if(watch.listeners.isEmpty()){watches.remove(watch.path);watch.observer.stopWatching();}
            watch=null;
        }}
    }
    public static synchronized Registration watch(File directory,Listener listener){
        String path=directory.getAbsolutePath();Watch watch=watches.get(path);boolean start=watch==null;
        if(start){watch=new Watch(path);watches.put(path,watch);}
        Registration registration=new Registration(watch);watch.listeners.put(registration,listener);
        if(start)watch.observer.startWatching();
        return registration;
    }
    private DirectoryWatch(){}
}
