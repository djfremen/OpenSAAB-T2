// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.graphics.Bitmap;
import android.os.SystemClock;
import java.io.File;
import java.nio.file.Files;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;
/** Measures first drawn bitmap equal to the original verified guest welcome. */
public final class StartupMeasurement {
    private final android.content.Context context;
    private final long origin;
    private File run;
    private volatile Bitmap expected;
    private volatile JSONObject nativeResult;
    private volatile long readyMs;
    private boolean recorded;
    public StartupMeasurement(android.content.Context context,long origin) {
        this.context=context.getApplicationContext();
        long now=SystemClock.elapsedRealtime();this.origin=origin>0&&origin<=now?origin:now;
    }
    public void session(File run){this.run=run;new File(context.getFilesDir(),"latest-startup.json").delete();}
    public void ready(String json) {
        readyMs=SystemClock.elapsedRealtime()-origin;
        try {
            JSONObject data=new JSONObject(json);
            if(!data.optBoolean("verified_welcome"))return;
            int[] pixels=LcdFrame.decode(Files.readAllBytes(new File(run,"welcome.ppm").toPath()));
            nativeResult=data;
            expected=Bitmap.createBitmap(pixels,320,240,Bitmap.Config.ARGB_8888);
        }catch(Exception e){android.util.Log.e("StartupMeasurement","Cannot verify welcome",e);}
    }
    public void drawn(Bitmap actual) {
        Bitmap target=expected;
        if(recorded || target==null || !actual.sameAs(target))return;
        recorded=true;
        long displayed=SystemClock.elapsedRealtime()-origin;
        new Thread(()->{try{
            JSONObject r=new JSONObject().put("verified_welcome",true).put("launch_to_native_ready_ms",readyMs)
                .put("launch_to_verified_draw_ms",displayed).put("goal_met",displayed<=10000)
                .put("candi_started",nativeResult.optBoolean("candi_started")).put("run_id",run.getName());
            byte[] bytes=r.toString(2).getBytes(StandardCharsets.UTF_8);
            Files.write(new File(run,"startup-result.json").toPath(),bytes);
            Files.write(new File(context.getFilesDir(),"latest-startup.json").toPath(),bytes);
            android.util.Log.i("StartupMeasurement",r.toString());
        }catch(Exception e){android.util.Log.e("StartupMeasurement","Cannot save timing",e);}},"startup-result").start();
    }
}
