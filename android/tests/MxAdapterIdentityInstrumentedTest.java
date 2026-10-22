// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Instrumentation;
import android.bluetooth.*;
import android.content.Context;
import android.os.*;
import java.io.*;
import java.security.MessageDigest;
import org.json.*;

/** Explicit owner-requested physical adapter-only check; no VIN/card/firmware operation. */
public final class MxAdapterIdentityInstrumentedTest extends Instrumentation {
    private static String exportNewTrace(File root,java.util.Set<String> before)throws Exception{
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();java.util.zip.ZipOutputStream zip=new java.util.zip.ZipOutputStream(bytes);int dirs=0,total=0;
        File[] runs=root.listFiles();if(runs!=null)for(File run:runs)if(run.isDirectory()&&!before.contains(run.getName())){dirs++;for(String name:new String[]{"result.json","transcript.jsonl","core-stderr.log"}){
            File file=new File(run,name);require(file.isFile()&&file.length()<=300000,"New identity trace missing or oversized");total+=file.length();require(total<=600000,"Identity trace exceeds export limit");
            zip.putNextEntry(new java.util.zip.ZipEntry(run.getName()+"/"+name));try(InputStream in=new FileInputStream(file)){byte[] buffer=new byte[8192];int n;while((n=in.read(buffer))!=-1)zip.write(buffer,0,n);}zip.closeEntry();
        }}require(dirs==1,"Expected exactly one new identity trace");zip.close();return android.util.Base64.encodeToString(bytes.toByteArray(),android.util.Base64.NO_WRAP);
    }
    private Bundle arguments;
    public void onCreate(Bundle args){super.onCreate(args);arguments=args;start();}
    private static String sha(File file)throws Exception{
        MessageDigest hash=MessageDigest.getInstance("SHA-256");
        try(InputStream in=new FileInputStream(file)){byte[] b=new byte[65536];int n;while((n=in.read(b))!=-1)hash.update(b,0,n);}
        StringBuilder out=new StringBuilder();for(byte b:hash.digest())out.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return out.toString();
    }
    private static void require(boolean value,String message){if(!value)throw new IllegalStateException(message);}
    public void onStart(){Bundle result=new Bundle();int code=-1;VlinkerVehicleConnection client=null;try{
        Context context=getTargetContext();String version=AppBuildProfile.installedVersion(context);
        require("0.1.1-preview.35".equals(version),"Exact preview.35 target required");
        require(!SimulatorVehicleConnection.available(context),"Physical target required; fixtures are a different check");
        require(!VlinkerVehicleConnection.active(),"Finish the existing connection first");
        String apk=sha(new File(context.getApplicationInfo().sourceDir));
        String worker=sha(new File(context.getApplicationInfo().nativeLibraryDir,"libopensaab_connection.so"));
        require(apk.equals(arguments.getString("expected_apk_sha256")),"Installed APK differs from authorized candidate");
        require(worker.equals(arguments.getString("expected_worker_sha256")),"Installed worker differs from authorized candidate");
        String label=context.getPackageManager().getApplicationLabel(context.getApplicationInfo()).toString();
        require(label.contains(version)&&label.contains("ARM64"),"Installed full label differs");
        BluetoothManager manager=(BluetoothManager)context.getSystemService(Context.BLUETOOTH_SERVICE);
        BluetoothAdapter adapter=manager==null?null:manager.getAdapter();require(adapter!=null&&adapter.isEnabled(),"Existing Android Bluetooth radio is unavailable");
        BluetoothDevice selected=null;for(BluetoothDevice d:adapter.getBondedDevices())if(d.getAddress().equals(arguments.getString("device_address")))selected=d;
        require(selected!=null&&selected.getName()!=null&&selected.getName().startsWith("OBDLink MX+"),"Requested bonded MX+ is unavailable; no pairing/discovery attempted");
        File traceRoot=new File(context.getFilesDir(),"vlinker-identity-tests");java.util.Set<String> before=new java.util.HashSet<>();String[] old=traceRoot.list();if(old!=null)java.util.Collections.addAll(before,old);
        final JSONObject[] terminal={null};
        client=VlinkerVehicleConnection.direct(context,v->{if(v.has("report"))terminal[0]=v;},selected,false);
        final VlinkerVehicleConnection running=client;runOnMainSync(()->running.startProbe("identity"));
        long until=SystemClock.elapsedRealtime()+60000;
        while((running.busy||terminal[0]==null)&&SystemClock.elapsedRealtime()<until)SystemClock.sleep(25);
        waitForIdleSync();require(!running.busy&&terminal[0]!=null,"Adapter identity check did not finish within its bound");
        JSONObject report=terminal[0].getJSONObject("report");
        require(report.getString("mode").equals("identity"),"Wrong workflow mode");
        require(report.getInt("host_vehicle_request_attempts")==0&&report.getInt("host_vehicle_requests_written")==0,"Adapter-only scope violated");
        JSONObject commands=report.getJSONObject("command_attempts");java.util.Iterator<String> keys=commands.keys();
        while(keys.hasNext())require(java.util.Arrays.asList("AT","ATI","ATD","ATE0","STDI","STI","ATRV").contains(keys.next()),"Unexpected command in adapter-only receipt");
        require(report.getBoolean("bluetooth_closed")&&!VlinkerVehicleConnection.active(),"Bluetooth release unconfirmed");
        JSONObject receipt=new JSONObject().put("helper_version","0.1.0-mx-identity-check.1").put("physical_hardware",true).put("application_version",version).put("installed_label",label).put("apk_sha256",apk).put("worker_sha256",worker).put("vehicle_requests",0).put("new_pairing",false).put("physical_button_by_agent",false).put("card_operation",false).put("report",report);
        result.putString("receipt",receipt.toString());
        result.putString("trace_zip_base64",exportNewTrace(traceRoot,before));
        result.putString("stream",report.getString("status").equals("complete")?"PASS: exact installed physical MX+ adapter-only identity/readiness and clean close; no VIN request; no vehicle qualification\n":"OBSERVED FAILURE: adapter-only attempt ended before vehicle work; retained report is not a pass\n");
    }catch(Throwable error){result.putString("stream","FAIL: "+android.util.Log.getStackTraceString(error));code=0;}
    finally{
        if(client!=null){final VlinkerVehicleConnection running=client;if(running.busy){runOnMainSync(running::stop);long until=SystemClock.elapsedRealtime()+8000;while(running.busy&&SystemClock.elapsedRealtime()<until)SystemClock.sleep(25);if(running.busy){code=0;result.putString("stream","FAIL: bounded adapter identity cancellation did not release ownership\n");}}}
        finish(code,result);
    }
    }
}
