// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.widget.*;
import java.io.*;import java.lang.reflect.*;import org.json.*;

/** Disposable emulator only. Uses preinstalled firmware offline; never opens USB. */
public final class ReleaseParityInstrumentedTest extends Instrumentation {
    private boolean live;
    public void onCreate(Bundle b){super.onCreate(b);live="true".equals(b.getString("live_upload"));start();}
    private static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    private static String text(View view){StringBuilder s=new StringBuilder();if(view instanceof TextView)s.append(((TextView)view).getText()).append('\n');if(view instanceof ViewGroup)for(int i=0;i<((ViewGroup)view).getChildCount();i++)s.append(text(((ViewGroup)view).getChildAt(i)));return s.toString();}
    private static Object field(Activity a,String name)throws Exception{Field f=a.getClass().getDeclaredField(name);f.setAccessible(true);return f.get(a);}
    public void onStart(){Bundle result=new Bundle();Activity main=null;try{
        Context c=getTargetContext();
        check(new FirmwareStore(c.getFilesDir()).missing().isEmpty(),"Offline fixture firmware not installed");
        check(EmulatorArchitecture.label(new File(c.getApplicationInfo().nativeLibraryDir,"libtech2_emu.so")).equals("Emulator: 64-bit ARM"),"Native architecture label");
        main=startActivitySync(new Intent().setClassName(c,"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
        final Activity app=main;final String[] labels={""};runOnMainSync(()->labels[0]=text(app.getWindow().getDecorView()));
        check(labels[0].contains("System check")&&labels[0].contains("Emulator: 64-bit ARM")&&labels[0].contains("Support OpenSAAB"),"Shared main UI features");
        check(!labels[0].contains("32-bit development"),"ARM32 branding leaked into ARM64");
        runOnMainSync(()->check(app.getWindow().getDecorView().findViewWithTag("headunit-actions")==null,"Head-unit layout leaked into phone"));
        Method start=app.getClass().getDeclaredMethod("startSession");start.setAccessible(true);runOnMainSync(()->{try{start.invoke(app);}catch(Exception e){throw new RuntimeException(e);}});
        File session=null;JSONObject measurement=null;long until=SystemClock.elapsedRealtime()+60000;
        while(SystemClock.elapsedRealtime()<until){session=(File)field(app,"session");if(session!=null){measurement=PerformanceReport.session(session);JSONArray samples=measurement.optJSONArray("samples");if(samples!=null&&samples.length()>=2&&measurement.has("first_frame_observed_ms"))break;}Thread.sleep(300);}
        check(measurement!=null&&measurement.has("first_frame_observed_ms"),"No first-frame resource measurements");
        JSONArray samples=measurement.getJSONArray("samples");JSONObject last=samples.getJSONObject(samples.length()-1);
        check(last.getLong("cpu_ms")>0&&last.getLong("peak_rss_kib")>0&&last.getLong("ram_available_kib")>0,"Native CPU/RAM metrics unavailable");
        Method stop=app.getClass().getDeclaredMethod("stopSession",String.class);stop.setAccessible(true);runOnMainSync(()->{try{stop.invoke(app,"Offline release validation complete");}catch(Exception e){throw new RuntimeException(e);}});
        until=SystemClock.elapsedRealtime()+10000;while((Boolean)field(app,"running")&&SystemClock.elapsedRealtime()<until)Thread.sleep(100);
        check(!(Boolean)field(app,"running"),"Offline session did not stop");
        JSONObject report=SupportReports.collect(c,"Synthetic ARM64 release validation: offline emulator startup; no vehicle connected.");
        check(report.getString("apk_abi").equals("arm64-v8a"),"Wrong report ABI");
        check(report.getJSONObject("device_resources").getLong("ram_total_bytes")>0,"System resource context");
        check(report.toString().contains("native_performance")&&report.toString().contains("first_frame_observed_ms"),"Native metrics missing from report");
        File output=new File(c.getExternalFilesDir(null),"parity-report.json");try(FileOutputStream out=new FileOutputStream(output)){out.write(report.toString().getBytes("UTF-8"));}
        Activity welcome=startActivitySync(new Intent(c,FirmwareActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
        final String[] welcomeText={""};runOnMainSync(()->welcomeText[0]=text(welcome.getWindow().getDecorView()));
        check(welcomeText[0].contains("Emulator: 64-bit ARM"),"Firmware/welcome branded architecture header");runOnMainSync(welcome::finish);
        if(live){String number=SupportUpload.send(report.toString());result.putString("report_id",number);}
        result.putString("stream","PASS: ARM64 branding/system check; original phone layout; offline native startup/stop; real CPU/RAM/first-frame samples; sanitized report context; firmware header"+(live?"; live private upload receipt":"; no network upload"));finish(Activity.RESULT_OK,result);
    }catch(Throwable e){result.putString("stream","FAIL: "+e);finish(Activity.RESULT_CANCELED,result);}finally{if(main!=null){Activity a=main;runOnMainSync(a::finish);}}}
}
