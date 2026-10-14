// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.hardware.usb.*;
import android.os.*;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.net.*;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.zip.*;

/** Android USB effects for the capture-derived Rust Mongoose beta. */
public final class MongooseUsbActivity extends Activity {
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final RequestGate gate=new RequestGate();
    private final AtomicBoolean busy=new AtomicBoolean(),cancel=new AtomicBoolean();
    private UsbManager manager;private TextView status,details;private RadioGroup power;
    private Button startup,channel,vin,swread,swwake,stop,share;private RadioGroup ignition;private EditText waitMinutes,notes;private boolean continued;private String runIgnition="unknown",runWait="",runNotes="";private boolean foreground,registered;
    private volatile ServerSocket server;private volatile Socket peer;private volatile java.lang.Process child;
    private String pendingMode,pendingPower;private File lastBundle;private String lastSummary;
    private UsbDevice selected;
    private final BroadcastReceiver receiver=new BroadcastReceiver(){public void onReceive(Context c,Intent i){
        if(UsbManager.ACTION_USB_DEVICE_DETACHED.equals(i.getAction())){
            UsbDevice d=i.getParcelableExtra(UsbManager.EXTRA_DEVICE);if(d!=null&&selected!=null&&d.getDeviceId()==selected.getDeviceId())cancel.set(true);return;
        }
        if(!permissionAction().equals(i.getAction()))return;
        long id=i.getLongExtra("request_id",Long.MIN_VALUE);
        UsbDevice d=i.getParcelableExtra(UsbManager.EXTRA_DEVICE);
        if(!gate.consume(id))return;
        if(d==null||selected==null||!same(d,selected)||!foreground){status.setText("Return to this screen and choose a test");return;}
        if(!i.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED,false)){saveFailure("usb_permission","USB permission denied",pendingMode,pendingPower);return;}
        begin(pendingMode,pendingPower);
    }};
    private String permissionAction(){return getPackageName()+".MONGOOSE_PERMISSION";}
    private static boolean same(UsbDevice a,UsbDevice b){return a.getDeviceId()==b.getDeviceId()&&a.getDeviceName().equals(b.getDeviceName())&&a.getVendorId()==b.getVendorId()&&a.getProductId()==b.getProductId();}
    private TextView text(String value,int size){TextView v=new TextView(this);v.setText(value);v.setTextSize(size);v.setTextColor(0xffedf4fa);v.setTextIsSelectable(true);return v;}
    private Button button(LinearLayout root,String title,Runnable click){Button b=new Button(this);b.setText(title);MongooseChrome.button(b,false);b.setOnClickListener(v->click.run());root.addView(b,new LinearLayout.LayoutParams(-1,MongooseChrome.dp(this,56)));return b;}
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);manager=(UsbManager)getSystemService(USB_SERVICE);setTitle(MongooseChrome.identity(this));
        if(saved!=null){continued=saved.getBoolean("continued",false);String name=saved.getString("bundle");if(name!=null&&name.matches("android_support_[a-f0-9-]+\\.zip")){lastBundle=new File(new File(getFilesDir(),"support-reports"),name);if(!lastBundle.isFile())lastBundle=null;}lastSummary=saved.getString("summary");}
        if(continued)showTests(saved);else showWelcome();
        IntentFilter f=new IntentFilter(permissionAction());f.addAction(UsbManager.ACTION_USB_DEVICE_DETACHED);
        if(Build.VERSION.SDK_INT>=33)registerReceiver(receiver,f,Context.RECEIVER_NOT_EXPORTED);else registerReceiver(receiver,f);registered=true;
        BackNavigation.install(this,()->{cancel.set(true);finish();});
    }
    private LinearLayout content(String title){
        ScrollView scroll=new ScrollView(this);LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setBackgroundColor(0xff101318);int pad=MongooseChrome.dp(this,20);root.setPadding(pad,pad,pad,pad);scroll.addView(root);root.addView(MongooseChrome.header(this,title));setContentView(scroll);return root;
    }
    private void showWelcome(){
        LinearLayout root=content("Developer Preview");
        root.addView(text("Version date: "+MongooseChrome.releaseDate(this),14));
        root.addView(text("Development is ongoing. If something goes wrong, use Report issue to tell us what happened and what you expected. Your feedback helps us improve OpenSAAB.",16));
        root.addView(text("You can review your report before you send it.",14));
        root.addView(text("Personal MongoosePro GM II USB test: startup, HS-CAN VIN and I-Bus wake/CIM reads. No diagnostic-card download is needed.",14));
        Button proceed=button(root,"Continue",()->{continued=true;showTests(null);});MongooseChrome.button(proceed,true);
        button(root,"Report issue",this::savedReports);MongooseChrome.footer(this,()->busy.get());
    }
    private void showTests(Bundle saved){
        LinearLayout root=content("Mongoose connection test");
        root.addView(text("Choose the power history for this run:",15));power=new RadioGroup(this);
        String[] choices={"After Tech2Win · USB hot swap","Fresh power · USB and OBD disconnected","Power history unknown"};
        for(int n=0;n<choices.length;n++){RadioButton r=new RadioButton(this);r.setId(1001+n);r.setText(choices[n]);r.setTextColor(0xfff3f6fb);power.addView(r);}power.check(saved==null?1003:saved.getInt("power",1003));root.addView(power);
        root.addView(text("Ignition and sleep context (your report, not automatically verified):",14));ignition=new RadioGroup(this);
        String[] states={"Ignition ON · engine off","Ignition OFF · waited for sleep","Ignition state unknown"};
        for(int n=0;n<states.length;n++){RadioButton r=new RadioButton(this);r.setId(1101+n);r.setText(states[n]);r.setTextColor(0xfff3f6fb);ignition.addView(r);}ignition.check(saved==null?1103:saved.getInt("ignition",1103));root.addView(ignition);
        waitMinutes=new EditText(this);waitMinutes.setHint("Minutes waited for sleep (optional)");waitMinutes.setTextColor(0xfff3f6fb);waitMinutes.setHintTextColor(0xffadb8c6);waitMinutes.setInputType(android.text.InputType.TYPE_CLASS_NUMBER);waitMinutes.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(4)});root.addView(waitMinutes);
        notes=new EditText(this);notes.setHint("Car model/year/engine and test notes (optional)");notes.setTextColor(0xfff3f6fb);notes.setHintTextColor(0xffadb8c6);notes.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(400)});root.addView(notes);if(saved!=null){waitMinutes.setText(saved.getString("wait",""));notes.setText(saved.getString("notes",""));}
        swread=button(root,"Read I-Bus CIM without HV wake",()->choose("swcan"));
        swwake=button(root,"Wake I-Bus and read CIM",()->choose("swcan_wake"));MongooseChrome.button(swwake,true);
        root.addView(text("I-Bus test reads fresh CIM ignition data and VIN. A reply can pass with the key OFF or state unknown. Engine running and prior bus sleep remain unverified. The VIN is included in the reviewed report.",14));
        vin=button(root,"Read vehicle VIN",()->choose("vin"));root.addView(text("This retained HS-CAN VIN test needs ignition ON, engine OFF.",14));
        startup=button(root,"Test Mongoose startup",()->choose("startup"));channel=button(root,"Test HS-CAN channel",()->choose("hs_channel"));
        stop=button(root,"Stop test",()->{gate.cancel();cancel.set(true);status.setText("Stopping · waiting for cleanup…");});share=button(root,"Share test report",this::reviewReport);
        status=text("Ready · connect Mongoose through USB OTG",16);root.addView(status);details=text("Every test uses a new connection and new ECU data. Reports can include VIN; review before sharing.",13);root.addView(details);
        button(root,"Saved test reports",this::savedReports);button(root,"Back",()->{cancel.set(true);finish();});MongooseChrome.footer(this,()->busy.get());buttons();
    }
    private void savedReports(){
        File folder=new File(getFilesDir(),"support-reports");File[] files=folder.listFiles((d,n)->n.matches("android_support_[a-f0-9-]+\\.zip"));
        if(files==null||files.length==0){new AlertDialog.Builder(this).setTitle("Report issue").setMessage("No saved Mongoose test reports. Continue and run a test, then review its report before sharing.").setPositiveButton("Close",null).show();return;}
        Arrays.sort(files,(a,b)->Long.compare(b.lastModified(),a.lastModified()));String[] labels=new String[files.length];for(int n=0;n<labels.length;n++)labels[n]=files[n].getName();
        new AlertDialog.Builder(this).setTitle("Saved test reports").setItems(labels,(d,n)->{try(ZipInputStream z=new ZipInputStream(new FileInputStream(files[n]))){ZipEntry entry=z.getNextEntry();if(entry==null||!"mongoose-test.json".equals(entry.getName()))throw new IOException("Unexpected report");ByteArrayOutputStream bytes=new ByteArrayOutputStream();byte[] buffer=new byte[4096];int count;while((count=z.read(buffer))!=-1){if(bytes.size()+count>100000)throw new IOException("Report size limit");bytes.write(buffer,0,count);}lastBundle=files[n];lastSummary=new String(bytes.toByteArray(),StandardCharsets.UTF_8);reviewReport();}catch(Exception e){Toast.makeText(this,"Saved report unavailable",Toast.LENGTH_LONG).show();}}).setNegativeButton("Close",null).show();
    }
    @Override protected void onResume(){super.onResume();foreground=true;}
    @Override protected void onStop(){foreground=false;gate.cancel();cancel.set(true);super.onStop();}
    @Override protected void onDestroy(){cancel.set(true);if(registered)unregisterReceiver(receiver);super.onDestroy();}
    @Override protected void onSaveInstanceState(Bundle b){b.putBoolean("continued",continued);if(power!=null)b.putInt("power",power.getCheckedRadioButtonId());if(ignition!=null)b.putInt("ignition",ignition.getCheckedRadioButtonId());if(waitMinutes!=null)b.putString("wait",waitMinutes.getText().toString());if(notes!=null)b.putString("notes",notes.getText().toString());if(lastBundle!=null)b.putString("bundle",lastBundle.getName());b.putString("summary",lastSummary);super.onSaveInstanceState(b);}
    private void buttons(){startup.setEnabled(!busy.get());channel.setEnabled(!busy.get());vin.setEnabled(!busy.get());swread.setEnabled(!busy.get());swwake.setEnabled(!busy.get());ignition.setEnabled(!busy.get());waitMinutes.setEnabled(!busy.get());notes.setEnabled(!busy.get());stop.setEnabled(busy.get());share.setEnabled(!busy.get()&&lastBundle!=null);for(int n=0;n<power.getChildCount();n++)power.getChildAt(n).setEnabled(!busy.get());for(int n=0;n<ignition.getChildCount();n++)ignition.getChildAt(n).setEnabled(!busy.get());}
    private String history(){return power.getCheckedRadioButtonId()==1001?"hot_swap_after_tech2win":power.getCheckedRadioButtonId()==1002?"fresh_usb_and_obd_power":"unknown";}
    private void choose(String mode){
        if(busy.get())return;
        runIgnition=ignition.getCheckedRadioButtonId()==1101?"on_engine_off":ignition.getCheckedRadioButtonId()==1102?"off_waited_for_sleep":"unknown";runWait=waitMinutes.getText().toString().trim();runNotes=notes.getText().toString().trim();

        ArrayList<UsbDevice> found=new ArrayList<>();for(UsbDevice d:manager.getDeviceList().values())if(d.getVendorId()==0x18e1&&d.getProductId()==0x0107)found.add(d);
        if(found.size()!=1){saveFailure("usb_selection",found.isEmpty()?"No Mongoose GM II detected":"Multiple Mongoose adapters; connect one",mode,history());return;}
        selected=found.get(0);pendingMode=mode;pendingPower=history();
        if(manager.hasPermission(selected)){begin(mode,pendingPower);return;}
        long id=gate.begin();Intent intent=new Intent(permissionAction()).setPackage(getPackageName()).putExtra("request_id",id);
        int flags=PendingIntent.FLAG_UPDATE_CURRENT|(Build.VERSION.SDK_INT>=31?PendingIntent.FLAG_MUTABLE:0);
        manager.requestPermission(selected,PendingIntent.getBroadcast(this,(int)id,intent,flags));status.setText("Allow Android USB access, then the test will start");
    }
    private void begin(String mode,String history){
        if(!busy.compareAndSet(false,true))return;
        cancel.set(false);lastBundle=null;lastSummary=null;runIgnition=ignition.getCheckedRadioButtonId()==1101?"on_engine_off":ignition.getCheckedRadioButtonId()==1102?"off_waited_for_sleep":"unknown";runWait=waitMinutes.getText().toString().trim();runNotes=notes.getText().toString().trim();buttons();status.setText("Opening Mongoose · test deadline 45 seconds");details.setText("Running "+mode+" · "+history);
        Thread worker=new Thread(()->execute(mode,history),"mongoose-usb-test");worker.start();
        ui.postDelayed(()->{if(busy.get()&&worker.isAlive()){cancel.set(true);try{if(peer!=null)peer.close();}catch(IOException ignored){}try{if(server!=null)server.close();}catch(IOException ignored){}if(child!=null)child.destroy();}},45000);
    }
    private JSONObject metadata(String mode,String history)throws Exception{
        JSONObject d=new JSONObject();d.put("schema",2);d.put("test",mode);d.put("program_name",MongooseChrome.identity(this));d.put("release_date",MongooseChrome.metadata(this,"release_date"));d.put("architecture",MongooseChrome.metadata(this,"architecture"));d.put("source_revision",MongooseChrome.metadata(this,"source_revision"));d.put("ignition_context",runIgnition);d.put("sleep_wait_minutes_reported",runWait);d.put("sleep_verified",false);d.put("test_notes",runNotes);d.put("power_history",history);d.put("power_history_verified",false);
        d.put("phone_manufacturer",Build.MANUFACTURER);d.put("phone_model",Build.MODEL);d.put("android_api",Build.VERSION.SDK_INT);d.put("android_version",Build.VERSION.RELEASE);d.put("supported_abis",new JSONArray(Arrays.asList(Build.SUPPORTED_ABIS)));
        d.put("package",getPackageName());d.put("version",getPackageManager().getPackageInfo(getPackageName(),0).versionName);d.put("observed_at_ms",System.currentTimeMillis());
        java.security.MessageDigest digest=java.security.MessageDigest.getInstance("SHA-256");try(InputStream in=new FileInputStream(getApplicationInfo().sourceDir)){byte[] block=new byte[65536];int n;while((n=in.read(block))!=-1)digest.update(block,0,n);}d.put("apk_sha256",hex(digest.digest()));
        d.put("application_vehicle_transmits",0);d.put("raw_payloads_included",false);d.put("vehicle_link_verified",false);d.put("vin_value_included",false);return d;
    }
    private void saveFailure(String stage,String error,String mode,String history){try{
        JSONObject d=metadata(mode,history);d.put("status","failed");d.put("failure_stage",stage);d.put("error",error);d.put("usb_closed",true);d.put("device_opened",false);freeze(d);
        status.setText(error);details.setText(lastSummary);buttons();
    }catch(Exception e){status.setText("Could not save test report");}}
    private void execute(String mode,String history){
        UsbDeviceConnection connection=null;UsbInterface iface=null;UsbRequest request=null;boolean queued=false,usbClosed=false,claimed=false,quit=false,cancelDelivered=false;java.lang.Process process=null;
        File run=new File(getFilesDir(),"mongoose-runs/"+UUID.randomUUID());String stage="usb_selection";JSONObject report=null;
        try{
            if(!run.mkdirs())throw new IOException("Cannot create private run directory");
            UsbDevice current=manager.getDeviceList().get(selected.getDeviceName());if(current==null||!same(current,selected))throw new IOException("Mongoose disconnected before open");
            UsbEndpoint rx=null,tx=null;
            for(int n=0;n<current.getInterfaceCount();n++){UsbInterface candidate=current.getInterface(n);UsbEndpoint in=null,out=null;
                for(int j=0;j<candidate.getEndpointCount();j++){UsbEndpoint e=candidate.getEndpoint(j);if(e.getType()==UsbConstants.USB_ENDPOINT_XFER_BULK&&e.getAddress()==0x82)in=e;if(e.getType()==UsbConstants.USB_ENDPOINT_XFER_BULK&&e.getAddress()==1)out=e;}
                if(in!=null&&out!=null){if(iface!=null)throw new IOException("Ambiguous Mongoose bulk interface");iface=candidate;rx=in;tx=out;}}
            if(iface==null||iface.getAlternateSetting()!=0)throw new IOException("Captured Mongoose endpoint layout absent");
            stage="usb_open";connection=manager.openDevice(current);if(connection==null)throw new IOException("Android USB open failed");report=metadata(mode,history);report.put("device_opened",true);
            stage="usb_claim";claimed=connection.claimInterface(iface,true);if(!claimed)throw new IOException("Android USB claim failed");
            report.put("usb_vendor",current.getVendorId());report.put("usb_product",current.getProductId());report.put("usb_interface",iface.getId());report.put("bulk_in",0x82);report.put("bulk_out",1);
            ByteBuffer buffer=ByteBuffer.allocateDirect(rx.getMaxPacketSize());request=new UsbRequest();if(!request.initialize(connection,rx))throw new IOException("USB receive initialization failed");
            server=new ServerSocket(0,1,InetAddress.getByName("127.0.0.1"));server.setSoTimeout(3000);
            byte[] random=new byte[16];new java.security.SecureRandom().nextBytes(random);String token=hex(random);
            File result=new File(run,"result.json");stage="native_worker_start";
            process=new ProcessBuilder(getApplicationInfo().nativeLibraryDir+"/libmongoose_probe.so",Integer.toString(server.getLocalPort()),token,result.getAbsolutePath(),mode).redirectErrorStream(true).redirectOutput(new File(run,"worker.log")).start();child=process;
            peer=server.accept();peer.setSoTimeout(6000);BufferedReader input=new BufferedReader(new InputStreamReader(peer.getInputStream(),StandardCharsets.US_ASCII));BufferedWriter output=new BufferedWriter(new OutputStreamWriter(peer.getOutputStream(),StandardCharsets.US_ASCII));
            if(!("HELLO "+token).equals(line(input)))throw new IOException("Wrong native session token");respond(output,"READY mongoose");
            long deadline=SystemClock.elapsedRealtime()+40000;MongooseUsbGate.Run budget=new MongooseUsbGate.Run(mode);JSONArray controls=new JSONArray();report.put("usb_controls",controls);stage="native_test";
            while(!quit&&SystemClock.elapsedRealtime()<deadline){String command=line(input);String answer;
                if(command.equals("CTRL 0")||command.equals("CTRL 1")){
                    int value=command.endsWith("1")?1:0;if(cancel.get()&&value!=0){respond(output,"CANCEL");continue;}
                    long at=SystemClock.elapsedRealtime();int count=connection.controlTransfer(0x40,0xdb,value,0,null,0,500);
                    controls.put(new JSONObject().put("value",value).put("result",count).put("elapsed_ms",SystemClock.elapsedRealtime()-at));answer=count==0?"CTRLOK":"CONTROL_FAILED";
                }else if(command.startsWith("TX ")){
                    byte[] wire=unhex(command.substring(3));if(!budget.admit(wire,cancel.get()))throw new IOException("Mongoose beta command gate rejected request");
                    if((wire[8]&255)==8){report.put("application_vehicle_transmits",budget.transmitAttempts());}
                    if(!queued){buffer.clear();if(!request.queue(buffer))throw new IOException("USB receive queue failed");queued=true;}
                    int count=connection.bulkTransfer(tx,wire,wire.length,500);answer=count==wire.length?"TXOK":"TX_FAILED";
                }else if(command.equals("READ")){
                    if(cancel.get()&&!cancelDelivered){cancelDelivered=true;respond(output,"CANCEL");continue;}
                    if(!queued){buffer.clear();if(!request.queue(buffer))throw new IOException("USB receive queue failed");queued=true;}
                    try{UsbRequest done=connection.requestWait(100);if(done!=request)throw new IOException("Unexpected USB completion");queued=false;buffer.flip();byte[] bytes=new byte[buffer.remaining()];buffer.get(bytes);answer=bytes.length==0?"EMPTY":"RX "+hex(bytes);}
                    catch(TimeoutException e){answer="EMPTY";}
                }else if(command.equals("QUIT")){
                    quit=true;boolean clean=true;
                    if(queued){try{clean=request.cancel()&&connection.requestWait(300)==request;}catch(Exception e){clean=false;}queued=false;}
                    request.close();request=null;clean=connection.releaseInterface(iface)&&clean;claimed=false;connection.close();connection=null;usbClosed=true;
                    answer=clean?"USB_CLOSED":"USB_CLEANUP_UNCONFIRMED";
                }else throw new IOException("Unknown native USB command");
                respond(output,answer);
            }
            if(!quit)throw new IOException("Mongoose whole-test deadline expired");
            if(!process.waitFor(3,TimeUnit.SECONDS))throw new IOException("Native worker did not exit");
            if(!result.isFile()||result.length()<2||result.length()>65536)throw new IOException("Native result unavailable or oversized");
            JSONObject nativeResult=new JSONObject(new String(Files.readAllBytes(result.toPath()),StandardCharsets.UTF_8));report.put("native",nativeResult);report.put("status",nativeResult.optString("status","failed"));report.put("worker_exit",process.exitValue());
            report.put("vehicle_link_verified",nativeResult.optBoolean("vehicle_link_verified",false));report.put("vin_value_included",nativeResult.optBoolean("vin_value_included",false));
            if(process.exitValue()!=0)report.put("status","failed");
        }catch(Exception e){try{if(report==null)report=metadata(mode,history);report.put("status",cancel.get()?"cancelled":"failed");report.put("failure_stage",stage);report.put("error",e.getClass().getSimpleName()+": "+e.getMessage());report.put("native_cleanup_confirmed",false);}catch(Exception ignored){}
        }finally{
            if(request!=null){if(queued)try{request.cancel();connection.requestWait(300);}catch(Exception ignored){}request.close();}
            if(connection!=null){if(claimed)try{connection.releaseInterface(iface);}catch(Exception ignored){}try{connection.close();usbClosed=true;}catch(Exception ignored){}}
            try{if(peer!=null)peer.close();}catch(IOException ignored){}try{if(server!=null)server.close();}catch(IOException ignored){}
            if(process!=null&&process.isAlive()){process.destroy();try{if(!process.waitFor(500,TimeUnit.MILLISECONDS))process.destroyForcibly();}catch(InterruptedException e){process.destroyForcibly();}}
            try{if(report==null)report=metadata(mode,history);report.put("usb_closed",usbClosed);report.put("cancel_requested",cancel.get());if(cancel.get())report.put("status","cancelled");freeze(report);}catch(Exception ignored){}
            child=null;peer=null;server=null;busy.set(false);ui.post(()->{if(isDestroyed())return;status.setText(lastSummary==null?"Test stopped · report unavailable":"Test "+(reportStatus(lastSummary)));details.setText(resultSummary(lastSummary));buttons();});
        }
    }
    private static String reportStatus(String s){try{return new JSONObject(s).optString("status","failed");}catch(Exception e){return "failed";}}
    private static String line(Reader reader)throws IOException{StringBuilder b=new StringBuilder();for(int n=0;n<32775;n++){int c=reader.read();if(c<0)throw new EOFException("Native USB peer closed");if(c=='\n')return b.toString();if(c<32||c>126)throw new IOException("Invalid native command text");b.append((char)c);}throw new IOException("Native command length limit");}
    private static void respond(Writer out,String value)throws IOException{out.write(value+"\n");out.flush();}
    private static String hex(byte[] b){StringBuilder s=new StringBuilder();for(byte v:b)s.append(String.format(Locale.ROOT,"%02x",v&255));return s.toString();}
    private static byte[] unhex(String s)throws IOException{if(s.length()>70||s.length()%2!=0||!s.matches("[a-fA-F0-9]+"))throw new IOException("Invalid Mongoose request hex");byte[] b=new byte[s.length()/2];for(int n=0;n<b.length;n++)b[n]=(byte)Integer.parseInt(s.substring(n*2,n*2+2),16);return b;}
    private void freeze(JSONObject report)throws Exception{
        String text=report.toString(2);if(text.length()>100000)throw new IOException("Report size limit");
        File directory=new File(getFilesDir(),"support-reports");directory.mkdirs();File bundle=new File(directory,"android_support_"+UUID.randomUUID()+".zip");
        try(ZipOutputStream z=new ZipOutputStream(new FileOutputStream(bundle))){z.putNextEntry(new ZipEntry("mongoose-test.json"));z.write(text.getBytes(StandardCharsets.UTF_8));z.closeEntry();}
        lastSummary=text;lastBundle=bundle;
    }
    public static Intent sharingIntent(Context context,File bundle){android.net.Uri uri=new android.net.Uri.Builder().scheme("content").authority(context.getPackageName()+".support-reports").appendPath(bundle.getName()).build();Intent intent=new Intent(Intent.ACTION_SEND).setType("application/zip").putExtra(Intent.EXTRA_STREAM,uri).putExtra(Intent.EXTRA_SUBJECT,bundle.getName()).putExtra(Intent.EXTRA_TITLE,bundle.getName()).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);intent.setClipData(ClipData.newUri(context.getContentResolver(),bundle.getName(),uri));return intent;}
    private static String resultSummary(String text){try{JSONObject report=new JSONObject(text);JSONObject nativeResult=report.optJSONObject("native");if(nativeResult==null)return report.optString("error","See reviewed report for details");String value="USB cleanup: "+nativeResult.optBoolean("cleanup_ok",false);JSONObject sw=nativeResult.optJSONObject("single_wire");if(sw!=null)value+="\nCIM reply: "+sw.optBoolean("cim_reply_verified",false)+" · ignition: "+sw.optString("cim_ignition_state","unknown")+" · tuple: "+sw.optJSONArray("cim_state_tuple");if(nativeResult.optBoolean("vin_value_included",false))value+="\nVIN: "+nativeResult.optString("vin");if(!"passed".equals(nativeResult.optString("status")))value+="\nStage: "+nativeResult.optString("failure_stage")+"\n"+nativeResult.optString("error");return value;}catch(Exception e){return "No confirmed result";}}
    private void reviewReport(){if(lastBundle==null||busy.get())return;new AlertDialog.Builder(this).setTitle("Review Mongoose test report").setMessage(lastSummary==null?"Saved test report":lastSummary).setNegativeButton("Cancel",null).setPositiveButton("Share report",(d,w)->startActivity(Intent.createChooser(sharingIntent(this,lastBundle),"Share Mongoose test report"))).show();}
}
