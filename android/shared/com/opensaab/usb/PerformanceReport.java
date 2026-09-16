// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.ActivityManager;
import android.content.Context;
import android.os.Debug;
import android.os.StatFs;
import android.util.DisplayMetrics;
import java.io.File;
import org.json.*;

/** Numerical support context only. Missing native measurements stay unavailable. */
public final class PerformanceReport {
    public static JSONObject device(Context c) throws JSONException {
        ActivityManager.MemoryInfo memory=new ActivityManager.MemoryInfo();
        ((ActivityManager)c.getSystemService(Context.ACTIVITY_SERVICE)).getMemoryInfo(memory);
        Runtime runtime=Runtime.getRuntime();DisplayMetrics display=c.getResources().getDisplayMetrics();
        return new JSONObject().put("ram_total_bytes",memory.totalMem).put("ram_available_bytes",memory.availMem)
            .put("low_memory_threshold_bytes",memory.threshold).put("system_low_memory",memory.lowMemory)
            .put("app_heap_used_bytes",runtime.totalMemory()-runtime.freeMemory()).put("app_heap_limit_bytes",runtime.maxMemory())
            .put("app_native_heap_bytes",Debug.getNativeHeapAllocatedSize())
            .put("storage_free_bytes",new StatFs(c.getFilesDir().getAbsolutePath()).getAvailableBytes())
            .put("display_width_px",display.widthPixels).put("display_height_px",display.heightPixels)
            .put("density_dpi",display.densityDpi).put("runtime_cpu_count",runtime.availableProcessors());
    }
    private static void numbers(JSONObject source,JSONObject dest,String...keys)throws JSONException {
        for(String key:keys){Object value=source.opt(key);if((value instanceof Integer||value instanceof Long)&&((Number)value).longValue()>=0)dest.put(key,value);}
    }
    static JSONObject sanitize(JSONObject raw)throws JSONException {
        if(raw.optInt("performance_schema",0)!=1)return new JSONObject().put("unavailable",true);
        JSONObject safe=new JSONObject();numbers(raw,safe,"performance_schema","sample_interval_ms","sample_count","first_frame_observed_ms");
        if(raw.opt("complete") instanceof Boolean)safe.put("complete",raw.getBoolean("complete"));
        JSONArray source=raw.optJSONArray("samples"),samples=new JSONArray();
        if(source!=null)for(int i=Math.max(0,source.length()-12);i<source.length();i++){
            JSONObject value=source.optJSONObject(i);if(value==null)continue;JSONObject sample=new JSONObject();
            numbers(value,sample,"elapsed_ms","cpu_ms","rss_kib","peak_rss_kib","minor_faults","major_faults","ram_total_kib","ram_available_kib","swap_free_kib","frame_age_ms");
            if(sample.has("elapsed_ms"))samples.put(sample);
        }
        safe.put("samples",samples);return safe;
    }
    public static JSONObject session(File directory) {
        try {File file=new File(directory,"host-performance.json");
            if(!file.isFile()||file.length()>16384||!file.getCanonicalFile().getParentFile().equals(directory.getCanonicalFile()))return new JSONObject().put("unavailable",true);
            return sanitize(FirmwareStore.json(file));
        }catch(Exception ignored){return new JSONObject();}
    }
    private PerformanceReport(){}
}
