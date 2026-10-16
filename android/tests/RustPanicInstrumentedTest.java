// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Instrumentation;
import android.content.Context;
import android.os.Build;
import android.os.Bundle;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.UUID;
import org.json.JSONObject;

/** Executes a synthetic Rust child packaged only in the test helper. No adapter or upload. */
public final class RustPanicInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args){super.onCreate(args);start();}
    void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    public void onStart(){
        Bundle result=new Bundle();int code=0;File dir=null;Process child=null;
        try{
            check(Build.PRODUCT.contains("sdk_gphone"),"Disposable emulator required");
            check(!FirmwareGate.sessionActive(),"Stop the diagnostic session first");
            Context c=getTargetContext();dir=new File(c.getFilesDir(),"native-"+UUID.randomUUID());
            check(dir.mkdir(),"Fixture directory unavailable");
            File executable=new File(getContext().getApplicationInfo().nativeLibraryDir,"libpanic_fixture.so");
            ProcessBuilder builder=new ProcessBuilder(executable.getAbsolutePath()).redirectErrorStream(true);
            builder.environment().put("RUST_BACKTRACE","0");child=builder.start();
            long observed=0,saved=0;File log=new File(dir,"console.log");
            try(BufferedReader reader=new BufferedReader(new InputStreamReader(child.getInputStream(),StandardCharsets.UTF_8));
                FileOutputStream output=new FileOutputStream(log)){
                String line;while((line=reader.readLine())!=null){
                    byte[] bytes=(line+"\n").getBytes(StandardCharsets.UTF_8);observed+=bytes.length;
                    if(SupportReports.retainProcessLogLine(saved,line)){output.write(bytes);saved+=bytes.length;}
                }
            }
            check(child.waitFor()==101,"Synthetic child did not panic");
            check(observed>2*1024*1024&&log.length()>2*1024*1024,"Panic did not exercise the log cap");
            JSONObject summary=SupportReports.logSummary(log),where=summary.getJSONObject("panic_location");
            check(where.getString("file").equals("emulator/src/panic_fixture.rs")&&where.getString("thread").equals("main"),"Real Rust panic location missing");
            check(where.getInt("line")>0&&where.getInt("column")>0,"Panic source position missing");
            JSONObject report=SupportReports.collect(c,"Synthetic preview reporting validation: controlled Rust child panic after the routine log cap; no adapter or vehicle.");
            String json=report.toString();
            check(json.contains("panic_location")&&!json.contains("PRIVATE_PANIC_CANARY")&&!json.contains("/opensaab"),"Panic message or build path exported");
            result.putString("report_json",json);
            result.putString("stream","PASS: real Rust child panic after 2 MiB; numbered-thread header collected; source position retained; message and build prefix excluded; no adapter or upload\n");code=-1;
        }catch(Throwable error){result.putString("stream","FAIL: "+error+"\n");}
        finally{
            if(child!=null)child.destroy();
            if(dir!=null){File[] files=dir.listFiles();if(files!=null)for(File f:files)f.delete();dir.delete();}
        }
        finish(code,result);
    }
}
