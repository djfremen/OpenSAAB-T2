// SPDX-License-Identifier: MPL-2.0
package com.opensaab.mongoosepersonaltests;
import android.app.*;
import android.content.*;
import android.content.pm.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import com.opensaab.usb.MongooseUsbGate;
import com.opensaab.usb.MongooseUsbActivity;
import org.json.*;
import java.io.*;
import java.net.*;
import java.nio.file.Files;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
/** Installed production worker/gate/UI. All USB/ECU responses are simulated. */
public final class MongoosePersonalInstrumentation extends Instrumentation {
 private int passed,fixtures;
 public void onCreate(Bundle b){super.onCreate(b);start();}
 private void check(boolean v,String m){if(!v)throw new AssertionError(m);passed++;}
 private static int word(byte[] b,int i){return(b[i]&255)|((b[i+1]&255)<<8);}
 private static void put(byte[] b,int i,int n){b[i]=(byte)n;b[i+1]=(byte)(n>>8);}
 private static byte[] bytes(String s){byte[] b=new byte[s.length()/2];for(int i=0;i<b.length;i++)b[i]=(byte)Integer.parseInt(s.substring(i*2,i*2+2),16);return b;}
 private static String hex(byte[] b){StringBuilder s=new StringBuilder();for(byte v:b)s.append(String.format(Locale.ROOT,"%02x",v&255));return s.toString();}
 private static byte[] record(int src,int op,int seq,byte[] body){byte[] r=new byte[16+body.length];put(r,0,r.length-4);put(r,2,(r.length-4)^0x51e6);put(r,6,src);put(r,8,op);put(r,10,seq);if(op==8)r[12]=1;System.arraycopy(body,0,r,16,body.length);return r;}
 private static byte[] received(int src,int address,byte[] frame){byte[] b=new byte[24];put(b,8,12);put(b,10,12);b[14]=(byte)(address>>8);b[15]=(byte)address;System.arraycopy(frame,0,b,16,8);return record(src,9,0,b);}
 private JSONObject fixture(String mode,String fault)throws Exception{
  boolean sw=mode.startsWith("swcan");ArrayList<Integer> ops=new ArrayList<>();ArrayList<String> bodies=new ArrayList<>();ArrayList<Throwable> errors=new ArrayList<>();ArrayDeque<byte[]> rx=new ArrayDeque<>();ArrayList<Integer> controls=new ArrayList<>();
  ServerSocket server=new ServerSocket(0,1,InetAddress.getByName("127.0.0.1"));server.setSoTimeout(5000);String token="0123456789abcdef0123456789abcdef";
  Thread controller=new Thread(()->{boolean cancelled=false;try(Socket socket=server.accept()){
   socket.setSoTimeout(6000);BufferedReader in=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.US_ASCII));PrintWriter out=new PrintWriter(socket.getOutputStream(),true);MongooseUsbGate.Run budget=new MongooseUsbGate.Run(mode);
   if(!("HELLO "+token).equals(in.readLine()))throw new IOException("Wrong token");out.println("READY mongoose");
   for(int n=0;n<12000;n++){
    String c=in.readLine();if(c==null)throw new EOFException();
    if(c.startsWith("CTRL ")){int value=Integer.parseInt(c.substring(5));controls.add(value);out.println(fault.equals("control_close")&&value==0?"CONTROL_FAILED":"CTRLOK");continue;}
    if(c.equals("QUIT")){out.println(fault.equals("usb_close")?"USB_CLOSE_FAILED":"USB_CLOSED");break;}
    if(c.equals("READ")){
     if(fault.equals("cancel_status")&&bodies.contains("40000000000000000700000000000241aa0101")&&!cancelled){cancelled=true;out.println("CANCEL");continue;}
     if(fault.equals("cancel_open")&&ops.contains(3)&&!cancelled){cancelled=true;out.println("CANCEL");continue;}
     Thread.sleep(2);out.println(rx.isEmpty()?"EMPTY":"RX "+hex(rx.remove()));continue;
    }
    if(!c.startsWith("TX "))throw new IOException("Unexpected command");byte[] r=bytes(c.substring(3));if(!budget.admit(r,cancelled))throw new IOException("Production Run gate rejected "+mode+"/"+fault+" op "+word(r,8));
    check(word(r,12)==(word(r,8)==8?1:0)&&word(r,14)==0,"Exact captured request header");
    int op=word(r,8),route=word(r,4);String body=hex(Arrays.copyOfRange(r,16,r.length));ops.add(op);bodies.add(body);out.println("TXOK");if(op==3&&fault.equals("drop_open"))continue;
    int size=op==0x100?0:op==0xc?16:op==0xd||op==8?12:4;byte[] reply=record(route,op|0x8000,word(r,10),new byte[size]);
    if(size>0)reply[16]=(byte)(op==0x103?7:op==5&&fault.equals("cleanup")?1:0);
    if(op==0xc){int sel=word(r,16);if(sel==0x2b){reply[29]=(byte)(fault.equals("version")?9:8);reply[30]=2;reply[31]=1;}if(sel==0x2a){reply[29]=8;reply[30]=1;reply[31]=1;}}
    if(op==0x109&&fault.equals("wrong_route"))put(reply,6,0x601);
    if(op==0xd&&(fault.equals("filter")||fault.equals("raw_filter")&&route==0x1701))reply[16]=1;
    ArrayList<byte[]> incoming=new ArrayList<>();
    if(op==8){
     reply[17]=1;if(fault.equals("acceptance"))reply[24]=(byte)255;
     boolean wake=body.equals("00040000000000000400000000000100"),status=body.endsWith("0241aa0101"),vin=body.endsWith("1a90"),stop=body.endsWith("024120");
     if(!fault.equals("no_completion")&&!(fault.equals("wake_completion")&&wake)&&!(fault.equals("stop_completion")&&stop)){
      byte[] done=record(route,10,word(r,10),bytes("060100007b000000"));if(fault.equals("completion_route")&&(wake||!sw))put(done,6,sw?0x1801:0x1801);incoming.add(done);
     }
     if(status){
      incoming.add(received(0x1801,0x641,bytes("037faa7800000000")));
      if(!fault.equals("no_status")){
       byte[] f=bytes(fault.equals("on")?"0100060202000000":"0100000000000000");byte[] statusFrame=received(0x1701,0x541,f);
       if(fault.equals("bad_status"))put(statusFrame,26,11); // extra_index must remain12
       if(fault.equals("status_route"))put(statusFrame,6,0x1801);
       incoming.add(statusFrame);
      }
     }
     if(vin&&!fault.equals("no_ecu")){
      int src=sw?0x1801:0x601,address=sw?0x641:0x7e8;
      if(sw)incoming.add(received(src,address,bytes("037faa7800000000")));
      byte[] payload=bytes("5a905953334644343959363731303030303031");byte[][] frames={new byte[8],new byte[8],new byte[8]};frames[0][0]=0x10;frames[0][1]=19;System.arraycopy(payload,0,frames[0],2,6);frames[1][0]=0x21;System.arraycopy(payload,6,frames[1],1,7);frames[2][0]=0x22;System.arraycopy(payload,13,frames[2],1,6);
      if(fault.equals("sequence"))frames[1][0]=0x22;if(fault.equals("negative"))frames[0]=bytes("037f1a3100000000");for(byte[] f:frames)incoming.add(received(src,address,f));
     }
    }
    if(fault.equals("early_ecu")&&op==8){rx.addAll(incoming);rx.add(reply);}else{rx.add(reply);rx.addAll(incoming);}
   }
  }catch(Throwable t){errors.add(t);}finally{try{server.close();}catch(Exception ignored){}}});controller.start();
  File result=new File(getTargetContext().getCacheDir(),"fixture-"+UUID.randomUUID()+".json");java.lang.Process p=new ProcessBuilder(getTargetContext().getApplicationInfo().nativeLibraryDir+"/libmongoose_probe.so",Integer.toString(server.getLocalPort()),token,result.getAbsolutePath(),mode).redirectErrorStream(true).start();
  if(!p.waitFor(20,TimeUnit.SECONDS)){p.destroyForcibly();throw new IOException("Fixture timeout "+mode+"/"+fault);}controller.join(3000);if(!errors.isEmpty())throw new IOException("Controller "+mode+"/"+fault,errors.get(0));
  JSONObject report=new JSONObject(new String(Files.readAllBytes(result.toPath()),StandardCharsets.UTF_8));result.delete();fixtures++;
  boolean positive=fault.equals("none")||fault.equals("on")||fault.equals("early_ecu");check(report.getString("status").equals(positive?"passed":"failed"),mode+"/"+fault+" status: "+report);check((p.exitValue()==0)==positive,"Exit matches result");check(report.getBoolean("usb_closed")==!fault.equals("usb_close"),"USB close confirmation");check(controls.equals(Arrays.asList(1,0)),"Enable/disable controls");check(report.getInt("application_vehicle_transmits")<= (sw?mode.equals("swcan_wake")?5:4:1),"Bounded vehicle attempts");check(Collections.frequency(bodies,"00040000000000000400000000000100")<=1,"No repeated wake");check(ops.get(ops.size()-1)==5,"Device close attempted");
  if(positive&&(sw||mode.equals("vin"))){check(report.getString("vin").equals("YS3FD49Y671000001"),"Synthetic VIN exact");check(report.getBoolean("vin_transmit_completion_observed"),"VIN completion");check(report.getInt("vin_response_frames")==3,"Ordered frame count");}
  if(positive&&sw){JSONObject s=report.getJSONObject("single_wire");check(s.getBoolean("cim_reply_verified"),"CIM reply verified");check(s.getString("cim_ignition_state").equals(fault.equals("on")?"on":"unknown"),"Zero tuple stays unknown");check(s.getBoolean("wake_requested")==mode.equals("swcan_wake"),"Wake control distinct");check(!s.getBoolean("electrical_wake_verified")&&!s.getBoolean("prior_bus_sleep_verified"),"No electrical/sleep claim");check(report.getInt("application_vehicle_transmits")== (mode.equals("swcan_wake")?5:4),"Exact4/5 vehicle writes");check(report.getBoolean("cim_stop_confirmed"),"Stop confirmed");}
  if(fault.equals("no_status")||fault.equals("status_route")){check(report.getBoolean("vin_value_included"),"Partial VIN retained");check(!report.getJSONObject("single_wire").getBoolean("cim_reply_verified"),"Missing status is not a pass");}
  if(fault.equals("cancel_status")){check(!bodies.contains("400000000000000006000000000002411a90"),"No VIN after cancellation");check(report.getBoolean("cim_stop_confirmed"),"Cancelled status stop cleanup");}
  if(fault.equals("version")||fault.equals("raw_filter")||fault.equals("filter")){check(!ops.contains(8),"No vehicle TX after setup mismatch");}
  System.out.println("FIXTURE PASS "+mode+"/"+fault+" attempts="+report.getInt("application_vehicle_transmits"));return report;
 }
 private static View find(View v,String s){if(v instanceof TextView&&((TextView)v).getText().toString().equals(s))return v;if(v instanceof ViewGroup){ViewGroup g=(ViewGroup)v;for(int n=0;n<g.getChildCount();n++){View f=find(g.getChildAt(n),s);if(f!=null)return f;}}return null;}
 private static String sha(File f)throws Exception{java.security.MessageDigest d=java.security.MessageDigest.getInstance("SHA-256");try(InputStream in=new FileInputStream(f)){byte[] b=new byte[65536];int n;while((n=in.read(b))!=-1)d.update(b,0,n);}return hex(d.digest());}
 private void physicalGateCases(){
  byte[] wake=record(0,8,1,bytes("00040000000000000400000000000100"));put(wake,4,0x1701);
  check(MongooseUsbGate.allowed(wake,false,"swcan_wake"),"Captured HV wake admitted");check(!MongooseUsbGate.allowed(wake,false,"swcan"),"Control cannot send HV wake");check(!MongooseUsbGate.allowed(wake,true,"swcan_wake"),"Cancelled wake blocked");
  byte[] legacy=wake.clone();legacy[12]=0;check(!MongooseUsbGate.allowed(legacy,false,"swcan_wake"),"Legacy zero transmit header rejected");
  for(int i=12;i<16;i++){byte[] bad=wake.clone();bad[i]=2;check(!MongooseUsbGate.allowed(bad,false,"swcan_wake"),"Unknown transmit header byte rejected");}
  MongooseUsbGate.Run budget=new MongooseUsbGate.Run("swcan_wake");check(budget.admit(wake,false),"First wake admitted");put(wake,10,2);check(!budget.admit(wake,false),"Second wake blocked despite fresh sequence");check(budget.transmitAttempts()==1,"Rejected wake did not spend or repeat TX");
  byte[] query=record(0,8,1,bytes("40000000000000000700000000000241aa0101"));put(query,4,0x1801);check(MongooseUsbGate.allowed(query,false,"swcan"),"Captured status admitted");query[query.length-1]=2;check(!MongooseUsbGate.allowed(query,false,"swcan"),"Uncaptured status selector blocked");
  byte[] stop=record(0,8,1,bytes("4000000000000000050000000000024120"));put(stop,4,0x1801);check(!new MongooseUsbGate.Run("swcan").admit(stop,true),"Stop forbidden without status attempt");
  byte[] close=record(0,5,2,new byte[0]);put(close,4,1);check(!new MongooseUsbGate.Run("swcan").admit(close,false),"Sequence gap blocked");put(close,10,1);close[12]=1;check(!MongooseUsbGate.allowed(close,false,"swcan"),"Unknown auxiliary field blocked");
 }
 private void uiAndSecurity()throws Exception{
  Context c=getTargetContext();PackageInfo pi=c.getPackageManager().getPackageInfo(c.getPackageName(),PackageManager.GET_ACTIVITIES|PackageManager.GET_PROVIDERS|PackageManager.GET_SERVICES|PackageManager.GET_RECEIVERS);
  check(pi.versionName.equals("0.1.2-preview.1")&&pi.getLongVersionCode()==100004,"Packaged version/code");check(c.getApplicationInfo().loadLabel(c.getPackageManager()).toString().equals("OpenSAAB 0.1.2-preview.1 ARM64"),"Installed full program name");check((c.getApplicationInfo().flags&ApplicationInfo.FLAG_DEBUGGABLE)==0,"Nondebuggable");check(pi.activities.length==1&&pi.activities[0].name.equals("com.opensaab.usb.MongooseUsbActivity"),"Only Mongoose activity");check(pi.services==null&&pi.receivers==null&&pi.providers.length==1,"No security/automation services");
  Activity a=startActivitySync(new Intent().setClassName(c,"com.opensaab.usb.MongooseUsbActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
  runOnMainSync(()->{View root=a.findViewById(android.R.id.content);check(find(root,"Developer Preview")!=null,"Fresh launch welcome");check(find(root,"Report issue")!=null,"Report issue welcome");check(a.getTitle().toString().equals("OpenSAAB 0.1.2-preview.1 ARM64"),"Main window title from package");check(find(root,"Wake I-Bus and read CIM")==null,"No test before Continue");find(root,"Continue").performClick();});waitForIdleSync();
  for(String mode:new String[]{"Read I-Bus CIM without HV wake","Wake I-Bus and read CIM","Read vehicle VIN"}){runOnMainSync(()->{View root=a.findViewById(android.R.id.content);check(find(root,mode)!=null,"Scoped test button");find(root,mode).performClick();check(find(root,"No Mongoose GM II detected")!=null,"No-adapter failure");check(find(root,"Share test report").isEnabled(),"Failure share available");});waitForIdleSync();}
  runOnMainSync(()->{View root=a.findViewById(android.R.id.content);check(find(root,"Security access")==null&&find(root,"Import processed response")==null,"No security/import UI");a.finish();});waitForIdleSync();
  File folder=new File(c.getFilesDir(),"support-reports");File[] reports=folder.listFiles((d,n)->n.matches("android_support_[a-f0-9-]+\\.zip"));check(reports!=null&&reports.length>=3,"Saved unique report ZIPs");File report=null;for(File f:reports){try(java.util.zip.ZipInputStream z=new java.util.zip.ZipInputStream(new FileInputStream(f))){if(z.getNextEntry()==null)continue;ByteArrayOutputStream bytes=new ByteArrayOutputStream();byte[] b=new byte[1024];int n;while((n=z.read(b))>0)bytes.write(b,0,n);JSONObject j=new JSONObject(bytes.toString("UTF-8"));if(j.optString("version").equals(pi.versionName)){report=f;check(j.getString("apk_sha256").equals(sha(new File(c.getApplicationInfo().sourceDir))),"Report exact installed APK hash");check(j.getString("source_revision").matches("[0-9a-f]{40}"),"Report full source revision");check(!j.getBoolean("sleep_verified")&&j.getInt("application_vehicle_transmits")==0,"No-adapter report semantics");}}}
  check(report!=null,"New installed report found");Intent share=MongooseUsbActivity.sharingIntent(c,report);check(share.getStringExtra(Intent.EXTRA_SUBJECT).equals(report.getName())&&share.getStringExtra(Intent.EXTRA_TITLE).equals(report.getName()),"Unique .zip subject/title");android.net.Uri uri=share.getParcelableExtra(Intent.EXTRA_STREAM);check(share.getClipData()!=null,"Share clip grants");try(android.database.Cursor cursor=c.getContentResolver().query(uri,null,null,null,null)){check(cursor!=null&&cursor.moveToFirst()&&cursor.getString(cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)).equals(report.getName()),"Provider ZIP name");}try(java.util.zip.ZipInputStream z=new java.util.zip.ZipInputStream(c.getContentResolver().openInputStream(uri))){check(z.getNextEntry().getName().equals("mongoose-test.json"),"Readable frozen ZIP");}
  boolean blocked=false;try{c.getContentResolver().openFileDescriptor(uri,"w").close();}catch(Exception e){blocked=true;}check(blocked,"Provider write/import unavailable");blocked=false;try{c.getContentResolver().insert(uri,new ContentValues());}catch(Exception e){blocked=true;}check(blocked,"Provider processed-response insert unavailable");
  for(String mode:new String[]{"security","seed","key","import","fallback","processed_response"}){File result=new File(c.getCacheDir(),"forbidden-"+mode+".json");java.lang.Process p=new ProcessBuilder(c.getApplicationInfo().nativeLibraryDir+"/libmongoose_probe.so","1","token",result.getAbsolutePath(),mode).redirectErrorStream(true).start();check(p.waitFor(3,TimeUnit.SECONDS)&&p.exitValue()!=0&&!result.exists(),"Native forbidden mode rejected: "+mode);}
  for(String name:new String[]{"SecurityAccessActivity","SecurityImportActivity","SupportReportActivity","Tech2Activity"}){try{Class.forName("com.opensaab.usb."+name);throw new AssertionError("Forbidden class present");}catch(ClassNotFoundException expected){passed++;}}
  Activity fresh=startActivitySync(new Intent().setClassName(c,"com.opensaab.usb.MongooseUsbActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();runOnMainSync(()->{check(find(fresh.findViewById(android.R.id.content),"Developer Preview")!=null,"Welcome on next fresh launch");fresh.finish();});
  System.out.println("INSTALLED_UI_SECURITY PASS; security processing/import/automation/fallback unavailable");
 }
 public void onStart(){Bundle result=new Bundle();try{
  fixture("startup","none");fixture("hs_channel","none");fixture("vin","none");fixture("vin","early_ecu");fixture("vin","no_completion");fixture("vin","acceptance");fixture("startup","cancel_open");fixture("startup","cleanup");fixture("hs_channel","version");
  fixture("swcan","none");fixture("swcan_wake","none");fixture("swcan_wake","on");fixture("swcan_wake","early_ecu");
  for(String fault:new String[]{"version","raw_filter","acceptance","no_completion","completion_route","wake_completion","no_status","status_route","bad_status","no_ecu","sequence","negative","cancel_status","cleanup","control_close","usb_close","stop_completion"})fixture("swcan_wake",fault);
  physicalGateCases();uiAndSecurity();result.putString("stream","MONGOOSE_PERSONAL PASS: "+passed+" assertions; "+fixtures+" installed native fixtures; simulated USB only, physical Mongoose pending");finish(0,result);
 }catch(Throwable t){t.printStackTrace();result.putString("stream","MONGOOSE_PERSONAL FAIL: "+t);finish(1,result);}}
}
