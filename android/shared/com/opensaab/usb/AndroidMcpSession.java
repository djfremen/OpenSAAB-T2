// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.File;
import java.util.function.*;
import org.json.JSONObject;

/** Thin binding to an existing native session owner; never starts a USB probe itself. */
public final class AndroidMcpSession implements AndroidMcp.Owner {
    private final Supplier<File> directory,sessionDirectory;
    private final BooleanSupplier running,idle,alive;
    private final IntPredicate key;
    private final Runnable stop;
    private final String mode;
    private final McpMenuCache menuCache=new McpMenuCache(16383);
    private File last;private long began,stamp,observed;
    public AndroidMcpSession(Supplier<File> directory,BooleanSupplier running,BooleanSupplier idle,BooleanSupplier alive,IntPredicate key,Runnable stop,String mode){this(directory,running,idle,alive,key,stop,mode,directory);}
    public AndroidMcpSession(Supplier<File> directory,BooleanSupplier running,BooleanSupplier idle,BooleanSupplier alive,IntPredicate key,Runnable stop,String mode,Supplier<File> sessionDirectory){this.directory=directory;this.sessionDirectory=sessionDirectory;this.running=running;this.idle=idle;this.alive=alive;this.key=key;this.stop=stop;this.mode=mode;}
    public JSONObject state()throws Exception {return state(true);}
    public JSONObject inspect()throws Exception {return state(false);}
    public void close(){menuCache.close();}
    private JSONObject state(boolean fresh)throws Exception {
        File run=directory.get(),session=sessionDirectory.get();boolean active=running.getAsBoolean()&&run!=null&&session!=null;long now=android.os.SystemClock.elapsedRealtime();if(run!=last){last=run;began=now;stamp=0;}
        JSONObject j=new JSONObject().put("session",active?session.getName():"").put("session_running",active).put("emulator_alive",active&&alive.getAsBoolean()).put("busy",SecurityAccessView.workflowBusy()||FirmwareGate.busy()).put("mode",mode).put("input_idle",active&&idle.getAsBoolean()).put("session_age_ms",active?now-began:0);
        JSONObject menu=menuCache.get(active?run:null,fresh);
        if(active){File beat=new File(run,"emulator-heartbeat");if(beat.lastModified()!=stamp){stamp=beat.lastModified();observed=now;}j.put("emulator_heartbeat_age_ms",stamp==0?JSONObject.NULL:now-observed);if(menu!=null)j.put("menu",menu);}
        return j;
    }
    public boolean key(int code){return key.test(code);}
    public void startOffline(){throw new IllegalStateException("Return home to start an offline session");}
    public void stop(){stop.run();}
}
