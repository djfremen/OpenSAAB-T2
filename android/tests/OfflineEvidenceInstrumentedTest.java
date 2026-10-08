// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.content.Context;
import android.os.Bundle;
import java.io.File;
import java.nio.file.Files;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import org.json.*;

/** Synthetic offline fixtures. No adapter, vehicle, network request or firmware launch. */
public final class OfflineEvidenceInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean v,String why){if(!v)throw new AssertionError(why);}
    void write(File f,String text)throws Exception{Files.write(f.toPath(),text.getBytes(StandardCharsets.UTF_8));}
    void remove(File f){File[] children=f.listFiles();if(children!=null)for(File c:children)remove(c);f.delete();}
    public void onStart(){Bundle result=new Bundle();File run=null,fixture=null;
        try{
            Context c=getTargetContext();run=new File(c.getFilesDir(),"sessions/"+UUID.randomUUID());check(run.mkdirs(),"run directory");
            fixture=new File(c.getCacheDir(),"evidence-inputs-"+UUID.randomUUID());check(fixture.mkdir(),"input directory");
            for(String name:new String[]{"eprom.bin","opsys.dwn","candi.bin","card.bin","engine"})write(new File(fixture,name),"synthetic immutable input");
            OfflineSessionEvidence recorder=new OfflineSessionEvidence();recorder.begin(run,new File(fixture,"engine"),fixture);
            recorder.input();recorder.frame();recorder.requestStop(false);recorder.requestStop(true);recorder.ended(3,false,true,false);
            JSONObject end=FirmwareStore.json(new File(run,"session-end.json"));
            check(end.getString("reason").equals("user_stop")&&end.getInt("exit_code")==3&&end.getInt("input_count")==1&&end.getInt("frame_count")==1,"stop intent must outrank ambiguous incomplete exit");
            String id=OfflineSessionEvidence.id(run);check(id.startsWith("offline-"),"offline session correlation");
            JSONObject context=OfflineSessionEvidence.context(FirmwareStore.json(new File(run,"emulation-context.json")));
            check(context.getString("session_id").equals(id)&&context.getString("engine_sha256").equals(OfflineSessionEvidence.digest(new File(fixture,"engine"))),"exact input fingerprint");
            recorder.begin(run,new File(fixture,"engine"),fixture);recorder.ended(3,false,false,false);
            check(FirmwareStore.json(new File(run,"session-end.json")).getString("reason").equals("process_exit"),"unrequested exit differs from Stop");
            recorder.begin(run,new File(fixture,"engine"),fixture);recorder.ended(null,true,true,false);
            check(FirmwareStore.json(new File(run,"session-end.json")).getString("reason").equals("host_deadline"),"host deadline classification");
            String secret="PRIVATE-PAYLOAD-SHOULD-NOT-ESCAPE";
            JSONObject nativeEvidence=new JSONObject().put("schema",1).put("reason","candi_stopped").put("stage","vehicle_link_wait")
                .put("screen",secret).put("candi",new JSONObject().put("reason","unsupported_access").put("pc",20).put("completed",50).put("payload",secret));
            write(new File(run,"report.json"),new JSONObject().put("status","incomplete").put("exit_code",3).put("reason",secret).put("emulation_evidence",nativeEvidence).toString());
            JSONObject report=SupportReports.collect(c,"Emulation stopped or appeared unresponsive. What I was doing: \nTesting context: Without an adapter",false);
            JSONObject found=null;JSONArray sessions=report.getJSONArray("recent_sessions");
            for(int i=0;i<sessions.length();i++)if(id.equals(sessions.getJSONObject(i).optString("session_id")))found=sessions.getJSONObject(i);
            check(found!=null,"report must include exact offline session");
            check(found.getJSONObject("emulation_evidence").getJSONObject("candi").getString("reason").equals("unsupported_access"),"native failure stage retained");
            check(found.getJSONObject("session_end").getString("reason").equals("host_deadline"),"host evidence included");
            check(!report.toString().contains(secret)&&!report.getBoolean("submitter_notes_provided"),"privacy and explicit no-notes flag");
            check(!SupportReports.notesProvided("Emulation stopped or appeared unresponsive. What I was doing: ")&&SupportReports.notesProvided("Tapped diagnostics"),"generated prompt is not submitter notes");
            JSONObject poison=OfflineSessionEvidence.nativeEvidence(new JSONObject().put("schema",1).put("reason",secret).put("stage",secret));
            check(!poison.toString().contains(secret)&&poison.getString("reason").equals("unknown"),"unknown reason must remain fixed vocabulary");
            JSONArray samples=new JSONArray();for(int i=0;i<42;i++)samples.put(new JSONObject().put("elapsed_ms",i).put("raw",secret));
            JSONObject perf=PerformanceReport.sanitize(new JSONObject().put("performance_schema",1).put("sample_count",42).put("samples",samples).put("samples_truncated",true).put("max_frame_age_ms",9000));
            check(perf.getJSONArray("samples").length()==12&&perf.getJSONArray("samples").getJSONObject(0).getInt("elapsed_ms")==0&&perf.getJSONArray("samples").getJSONObject(2).getInt("elapsed_ms")==32&&!perf.toString().contains(secret),"first/last bounded evidence");
            new File(run,"session-end.json").delete();new File(run,"emulation-context.json").delete();
            JSONObject legacy=new JSONObject();OfflineSessionEvidence.collect(run,legacy);
            check(legacy.getJSONObject("session_end").getBoolean("unavailable"),"missing host record is unknown, not proof of crash");
            write(new File(c.getCacheDir(),"offline-evidence-fixture.json"),report.toString());
            result.putString("stream","PASS: offline stop intent, fingerprints, correlation, privacy, notes, bounded performance and unavailable legacy evidence\n");finish(-1,result);
        }catch(Throwable error){result.putString("stream","FAIL: "+error+"\n");finish(0,result);}
        finally{if(run!=null)remove(run);if(fixture!=null)remove(fixture);}
    }
}
