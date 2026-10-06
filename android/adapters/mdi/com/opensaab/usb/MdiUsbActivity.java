// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.hardware.usb.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import org.json.JSONObject;

/** Thin Android binding around the MDI module and the common OpenSAAB workspace. */
public final class MdiUsbActivity extends Activity {
    private static final AtomicBoolean OWNED=new AtomicBoolean();
    public static boolean sessionActive(){return OWNED.get();}
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final RequestGate requests=new RequestGate();
    private final AtomicBoolean running=new AtomicBoolean();
    private final ExecutorService observer=Executors.newSingleThreadExecutor();
    private final AtomicBoolean observing=new AtomicBoolean();
    private UsbManager manager;
    private UsbDevice selected;
    private volatile File directory,nativeDirectory;
    private volatile VehicleIdentity identity;
    private volatile boolean stopping;
    private boolean foreground,permissionPending,frameSeen,closed;
    private String permission,message="Ready · MDI USB adapter selected",authority;
    private SessionWorkspace workspace;
    private Tech2Controls controls;
    private NativeLcdPump lcdPump;
    private volatile InteractiveKeyPump keyPump;
    private EmulatorHealthMonitor health;
    private SecurityAccessView security;
    private DtcReportView reports;
    private IgnitionStatusView ignition;
    private LinearLayout details;
    private TextView console;
    private Button connect,cancel;
    private ConnectionAttempt attempt;
    private String observedPhase="";
    private Runnable pendingTool;
    private final Runnable poll=new Runnable(){public void run(){observe();if(!closed)ui.postDelayed(this,500);}};
    private final BroadcastReceiver receiver=new BroadcastReceiver(){public void onReceive(Context c,Intent i){
        UsbDevice device=i.getParcelableExtra(UsbManager.EXTRA_DEVICE);
        if(permission.equals(i.getAction())){
            long epoch=i.getLongExtra("request_epoch",-1);
            if(!requests.pending(epoch))return;
            permissionPending=false;
            UsbDevice current=attached();
            if(foreground&&current!=null&&manager.hasPermission(current))begin(current,epoch);
            else {requests.cancel();setMessage("USB permission unavailable · Connect and start to retry");if(attempt!=null)attempt.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.PERMISSION_DENIED);}
        }else if(UsbManager.ACTION_USB_DEVICE_DETACHED.equals(i.getAction())&&selected!=null&&selected.equals(device)){
            stop("MDI disconnected · releasing the session");
        }
    }};

    @Override public void onCreate(Bundle state){
        super.onCreate(state);BackNavigation.install(this,()->{if(running.get())key(1);else finish();});
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        manager=(UsbManager)getSystemService(USB_SERVICE);permission=getPackageName()+".MDI_USB_PERMISSION";
        authority=getIntent().getBooleanExtra("mdi_security_collect",false)?"seeds":"full";
        details=new LinearLayout(this);details.setOrientation(LinearLayout.VERTICAL);
        TextView description=new TextView(this);description.setText("Classic MDI · USB\nOriginal firmware owns diagnostic commands. Follow its physical ignition-key prompts.");details.addView(description);
        ignition=new IgnitionStatusView(this);details.addView(ignition);
        security=new SecurityAccessView(this,authority.equals("seeds"),()->nativeDirectory,()->running.get(),
            ()->stop("Closing firmware for security processing"),()->restart(true),()->restart(false));
        security.manualNavigation();security.setMenuKey(this::key);details.addView(security);
        reports=new DtcReportView(this,"mdi",()->nativeDirectory);details.addView(reports);
        ImageView lcd=new ImageView(this);lcd.setScaleType(ImageView.ScaleType.FIT_CENTER);
        console=new TextView(this);console.setTypeface(android.graphics.Typeface.MONOSPACE);console.setTextSize(11);
        console.setText("MDI session log stays on this device.");ScrollView scroll=new ScrollView(this);scroll.addView(console);
        controls=new Tech2Controls(this,lcd,scroll,this::key);controls.setActions(this::actions);
        health=new EmulatorHealthMonitor(this,()->stop("Preparing report"));
        lcdPump=new NativeLcdPump(bitmap->{
            if(bitmap==null){lcd.setImageDrawable(null);return;}
            if(!running.get()||stopping)return;
            lcd.setImageBitmap(bitmap);health.frame();controls.showFirstUseGuide();
            if(!frameSeen){frameSeen=true;setMessage("Original firmware active · MDI USB");if(attempt!=null)attempt.stage(ConnectionAttempt.Stage.SESSION);}
        });
        workspace=new SessionWorkspace(this,"OpenSAAB T2",controls,this::appMenu,
            ()->{security.refresh();SessionSheet.show(this,"Vehicle and security details",details);});
        connect=new Button(this);connect.setText("Connect and start");connect.setTag("mdi-connect-start");SessionStyle.button(connect,true);connect.setOnClickListener(v->discover());workspace.addLaunch(connect);
        cancel=new Button(this);cancel.setText("Cancel");cancel.setTag("mdi-cancel");SessionStyle.button(cancel,false);cancel.setOnClickListener(v->stop("Stopped by operator"));workspace.addLaunch(cancel);
        SessionStyle.stack(details);update();setContentView(workspace);
        IntentFilter filter=new IntentFilter(permission);filter.addAction(UsbManager.ACTION_USB_DEVICE_DETACHED);
        if(Build.VERSION.SDK_INT>=33)registerReceiver(receiver,filter,Context.RECEIVER_NOT_EXPORTED);else registerReceiver(receiver,filter);
        ui.post(poll);
        boolean auto=state==null&&getIntent().getBooleanExtra("auto_start",false);
        if(authority.equals("seeds")){if(auto)ui.post(this::discover);}else restoreCollectedSession(auto);
    }
    private void restoreCollectedSession(boolean auto){
        observer.execute(()->{
            File restoredRoot=null,restoredOutput=null;VehicleIdentity restoredVehicle=null;
            try{
                File[] runs=new File(getFilesDir(),"mdi-sessions").listFiles(File::isDirectory);
                if(runs!=null&&runs.length>0){
                    Arrays.sort(runs,Comparator.comparingLong(File::lastModified).reversed());
                    File card=new File(getFilesDir(),"firmware/card.bin"),root=runs[0],output=new File(root,"native");
                    VehicleIdentity vehicle=VehicleSession.read(new File(output,VehicleSession.FILE));
                    SecuritySessionData data=SecuritySessionData.load(output,card,vehicle);
                    // Only the most recent session; never substitute earlier history.
                    if(data.completed&&data.state==SsaState.PRE_AUTH){
                        SecuritySessionData.processingInput(output,card,vehicle,data.bytes);
                        restoredRoot=root;restoredOutput=output;restoredVehicle=vehicle;
                    }
                }
            }catch(Exception changed){}
            final File root=restoredRoot,output=restoredOutput;final VehicleIdentity vehicle=restoredVehicle;
            ui.post(()->{if(closed||running.get()||directory!=null)return;
                if(root!=null){directory=root;nativeDirectory=output;identity=vehicle;
                    setMessage("Collected security data restored · Process existing data");security.refresh();update();
                    if(getIntent().getBooleanExtra("mdi_security_request",false))SessionSheet.show(this,"Vehicle and security details",details);
                }else if(getIntent().getBooleanExtra("mdi_security_request",false)){SessionSheet.show(this,"Vehicle and security details",details);security.requestAccess();}
                else if(auto)discover();
            });
        });
    }
    private UsbDevice attached(){
        if(selected==null)return null;
        UsbDevice d=manager.getDeviceList().get(selected.getDeviceName());
        return d!=null&&d.getDeviceId()==selected.getDeviceId()&&d.getVendorId()==selected.getVendorId()&&d.getProductId()==selected.getProductId()?d:null;
    }
    private void update(){
        if(workspace==null)return;
        TextView auth=security==null?null:security.findViewWithTag("security-state");
        workspace.summary(message+(auth==null?"":"\n"+auth.getText())+(identity==null?"\nVehicle access not verified":(running.get()&&!stopping?"\nVIN: ":"\nLast session VIN: ")+identity.vin+"\nVehicle access not verified"));
        connect.setVisibility(running.get()||permissionPending?View.GONE:View.VISIBLE);
        cancel.setVisibility(running.get()||permissionPending?View.VISIBLE:View.GONE);
    }
    private void setMessage(String text){message=text;update();}
    private void discover(){
        if(!foreground||closed||running.get()||permissionPending)return;
        if(SecurityAccessView.workflowBusy()||FirmwareGate.sessionActive()||VlinkerVehicleConnection.active()||sessionActive()){
            setMessage("Finish the current session before connecting MDI");return;
        }
        if(!MdiProfile.packaged(this)){setMessage("MDI support is unavailable for this app architecture");return;}
        try{MdiProfile.read(this);}catch(IOException missing){setMessage(missing.getMessage());return;}
        String missing=new FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty()){setMessage("Firmware setup needed");startActivity(new Intent(this,FirmwareActivity.class));return;}
        String path=getIntent().getStringExtra("usb_device_name");selected=null;
        for(UsbDevice d:manager.getDeviceList().values()){
            if(path!=null&&!path.equals(d.getDeviceName()))continue;
            if(!MdiProfile.candidate(AdapterCatalog.identify(d.getVendorId(),d.getProductId())))continue;
            if(selected!=null){setMessage("Multiple MDI candidates · select one adapter");selected=null;return;}selected=d;
        }
        if(selected==null){setMessage("Selected adapter disconnected · return and Refresh");return;}
        identity=null;frameSeen=false;stopping=false;observedPhase="";
        attempt=new ConnectionAttempt(this,ConnectionAttempt.Adapter.MDI);attempt.device(selected.getVendorId(),selected.getProductId());
        long epoch=requests.begin();
        if(manager.hasPermission(selected)){begin(selected,epoch);return;}
        permissionPending=true;attempt.stage(ConnectionAttempt.Stage.PERMISSION);setMessage("Allow OpenSAAB to access the MDI USB adapter");
        Intent grant=new Intent(permission).setPackage(getPackageName()).putExtra("request_epoch",epoch);
        PendingIntent pending=PendingIntent.getBroadcast(this,(int)epoch,grant,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        manager.requestPermission(selected,pending);
    }
    private void begin(UsbDevice device,long epoch){
        if(!foreground||closed||!requests.consume(epoch)||!OWNED.compareAndSet(false,true))return;
        running.set(true);stopping=false;setMessage("Opening selected MDI USB adapter");
        new Thread(()->execute(device),"opensaab-mdi-session").start();
    }
    private void execute(UsbDevice device){
        UsbDeviceConnection connection=null;UsbInterface control=null,data=null;boolean cc=false,dc=false;
        FirmwareGate.Lease lease=null;boolean transportClean=false,usbClean=true,workflowSucceeded=false;String end="MDI connection failed";MdiProfile profile=null;
        try{
            lease=FirmwareGate.use();profile=MdiProfile.read(this);
            directory=new File(getFilesDir(),"mdi-sessions/"+UUID.randomUUID());Files.createDirectories(directory.toPath());
            nativeDirectory=new File(directory,"native");Files.createDirectories(nativeDirectory.toPath());
            SecuritySessionData.recordBaseline(nativeDirectory,new File(getFilesDir(),"firmware/card.bin"));
            if(stopping)Files.write(new File(directory,"session-stop").toPath(),new byte[]{1});
            keyPump=new InteractiveKeyPump(new File(nativeDirectory,"native-key.txt"),e->ui.post(()->setMessage("Firmware input unavailable")));
            if(stopping)keyPump.cancel();
            File output=nativeDirectory;ui.post(()->{if(!closed){health.begin(output);lcdPump.setDirectory(output);}});
            UsbConfiguration cfg=null;
            for(int n=0;n<device.getConfigurationCount();n++)if(device.getConfiguration(n).getId()==2)cfg=device.getConfiguration(n);
            if(cfg==null)throw new IOException("Unsupported MDI USB configuration");
            for(int n=0;n<cfg.getInterfaceCount();n++){
                UsbInterface x=cfg.getInterface(n);
                if(x.getId()==0&&x.getInterfaceClass()==2&&x.getInterfaceSubclass()==2&&x.getInterfaceProtocol()==255)control=x;
                if(x.getId()==1&&x.getInterfaceClass()==10)data=x;
            }
            if(control==null||data==null)throw new IOException("Unsupported MDI USB interface profile");
            boolean in=false,out=false;
            for(int n=0;n<data.getEndpointCount();n++){UsbEndpoint ep=data.getEndpoint(n);if(ep.getType()==UsbConstants.USB_ENDPOINT_XFER_BULK){
                if(ep.getMaxPacketSize()!=64)throw new IOException("MDI USB profile is not qualified");in|=ep.getAddress()==0x81;out|=ep.getAddress()==0x02;}}
            if(!in||!out)throw new IOException("Unsupported MDI USB endpoints");
            if(stopping)throw new IOException("Stopped before USB open");
            attempt.stage(ConnectionAttempt.Stage.USB_OPEN);connection=manager.openDevice(device);
            if(connection==null)throw new IOException("MDI USB open failed");
            if(!connection.setConfiguration(cfg))throw new IOException("MDI configuration selection failed");
            attempt.stage(ConnectionAttempt.Stage.INTERFACE_CLAIM);
            cc=connection.claimInterface(control,true);if(!cc)throw new IOException("MDI control interface unavailable");
            dc=connection.claimInterface(data,true);if(!dc)throw new IOException("MDI data interface unavailable");
            attempt.stage(ConnectionAttempt.Stage.TRANSPORT_START);
            boolean capture=(getApplicationInfo().flags&android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)!=0&&getIntent().getBooleanExtra("mdi_capture",false);
            int result=MdiNative.nativeRun(connection.getFileDescriptor(),profile.serial,profile.key,directory.getAbsolutePath(),
                new File(getApplicationInfo().nativeLibraryDir,"libtech2_mdi.so").getAbsolutePath(),new File(getFilesDir(),"firmware").getAbsolutePath(),authority,capture);
            workflowSucceeded=result==0;
            JSONObject report=json(new File(directory,"result-private.json"),65536);
            transportClean=report.optBoolean("filter_disabled")&&report.optBoolean("halt_sent")&&report.optBoolean("USB_IN_cancel_reaped");
            JSONObject workflow=report.optJSONObject("workflow");JSONObject nativeResult=workflow==null?null:workflow.optJSONObject("native_firmware");
            if(nativeResult!=null)transportClean&=nativeResult.optBoolean("transport_cleanup_ok");
            end=stopping?"Stopped by operator":result==0?"MDI session ended":"MDI connection or session failed";
            if(!transportClean)end+=" · transport cleanup unconfirmed";
        }catch(Throwable error){end=stopping?"Stopped by operator":error instanceof IOException?error.getMessage():"MDI connection failed";
            if(attempt!=null&&!stopping)attempt.failure(error);
        }finally{
            if(attempt!=null)attempt.stage(ConnectionAttempt.Stage.CLEANUP);
            if(profile!=null)Arrays.fill(profile.key,(byte)0);
            if(connection!=null){if(dc)usbClean&=connection.releaseInterface(data);if(cc)usbClean&=connection.releaseInterface(control);connection.close();}
            try{if(directory!=null)FirmwareStore.writeJson(new File(directory,"java-release.json"),new JSONObject().put("data_claimed",dc).put("control_claimed",cc).put("USB_release_ok",usbClean).put("connection_closed",connection!=null));}catch(Exception ignored){}
            InteractiveKeyPump input=keyPump;if(input!=null)input.close();keyPump=null;
            if(lease!=null)lease.close();OWNED.set(false);running.set(false);
            final String result=end+(usbClean?" · USB closed":" · USB cleanup incomplete");
            if(attempt!=null)attempt.finish(stopping?ConnectionAttempt.Outcome.CANCELLED:workflowSucceeded&&transportClean&&usbClean?ConnectionAttempt.Outcome.COMPLETED:ConnectionAttempt.Outcome.FAILED,
                stopping?ConnectionAttempt.Reason.USER_STOP:workflowSucceeded&&transportClean&&usbClean?ConnectionAttempt.Reason.NONE:ConnectionAttempt.Reason.CLEANUP_FAILED);
            ui.post(()->{if(closed)return;health.expectedStop();lcdPump.clear();setMessage(result);setResult(RESULT_CANCELED,new Intent().putExtra("summary",result));
                if(pendingTool!=null){Runnable next=pendingTool;pendingTool=null;next.run();}});
        }
    }
    private static JSONObject json(File f,int maximum)throws Exception{
        if(!f.isFile()||f.length()>maximum)throw new IOException("MDI result unavailable");
        return new JSONObject(new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8));
    }
    private void observe(){
        security.refresh();update();
        if(closed||!running.get()||directory==null||!observing.compareAndSet(false,true))return;
        final File run=directory,output=nativeDirectory;
        observer.execute(()->{try{
            JSONObject phase=json(new File(run,"session-state.json"),4096);
            VehicleIdentity fresh=identity;
            if(fresh==null){try{
                JSONObject vin=json(new File(run,"vin-private.json"),256*1024);
                if(vin.optBoolean("cleanup_ok")&&"fresh_vin_received".equals(vin.optString("status"))){
                    String value=vin.getString("fresh_vin");
                    fresh=new VehicleIdentity(value,java.time.Instant.now().toString(),"MDI fresh HS-CAN ECU VIN",VehicleIdentity.year(value),"not_requested","","","","","");
                    VehicleSession.save(new File(output,VehicleSession.FILE),fresh);VehicleSession.save(new File(getFilesDir(),"last-vehicle.json"),fresh);
                }
            }catch(Exception notReady){}}
            final VehicleIdentity vehicle=fresh;
            ui.post(()->{if(closed||directory!=run||!running.get()||stopping)return;
                identity=vehicle;String state=phase.optString("state");
                if(!state.equals(observedPhase)){observedPhase=state;if(!frameSeen)setMessage(phase.optString("message"));}
                security.refresh();ignition.refresh(output,true);update();});
        }catch(Exception pending){}finally{observing.set(false);}});
    }
    private boolean key(int code){
        if(!foreground||!running.get()||stopping||code<0||code>31)return false;
        InteractiveKeyPump pump=keyPump;if(pump==null)return false;health.input();return pump.offer(String.format(Locale.ROOT,"0x%02x",code));
    }
    private void stop(String reason){
        requests.cancel();permissionPending=false;stopping=true;health.expectedStop();
        InteractiveKeyPump pump=keyPump;if(pump!=null)pump.cancel();
        File run=directory;
        if(run!=null&&running.get())new Thread(()->{try{Files.write(new File(run,"session-stop").toPath(),new byte[]{1});}catch(IOException ignored){}} ,"mdi-stop").start();
        lcdPump.clear();setMessage(running.get()?reason+" · waiting for MDI cleanup":reason);
    }
    private void restart(boolean collect){
        if(running.get()||closed)return;
        UsbDevice current=attached();if(current==null){setMessage("Reconnect MDI before returning to firmware");return;}
        if(collect){startActivity(new Intent(this,MdiUsbActivity.class).putExtra("mdi_security_collect",true)
            .putExtra("usb_device_name",current.getDeviceName()).putExtra("auto_start",true));finish();return;}
        authority="full";stopping=false;security.returnedToFirmware();discover();
    }
    private android.app.Dialog actions(){
        return new SessionSheet.Menu(this)
            .add(security.readyToProcess()?"Process existing data":"Get security access",()->{SessionSheet.show(this,"Vehicle and security details",details);security.requestAccess();})
            .add("ECU information",()->originalMenus("ECU information"))
            .add("Read DTC",()->originalMenus("Read DTC"))
            .add("Clear DTC",()->originalMenus("Clear DTC"))
            .add("Engine Data",()->originalMenus("Engine Data"))
            .add("Saved DTC reports",()->DtcReportView.showSavedReports(this))
            .add(workspace.expanded()?"Show header":"Expand screen",workspace::toggleExpanded)
            .add("App menu",this::appMenu).show("Actions");
    }
    private void originalMenus(String name){new AlertDialog.Builder(this).setTitle(name).setMessage("Use the original firmware menus for this MDI operation. Automatic menu shortcuts are not available yet.").setPositiveButton("Continue firmware",null).show();}
    private void appMenu(){
        SessionSheet.Menu menu=new SessionSheet.Menu(this);
        if(running.get()||permissionPending)menu.add("Stop emulation",()->stop("Stopped by operator"));
        else menu.add("Connect and start",this::discover);
        menu.add("Back to adapter selection",()->{stop("Returning to adapter selection");finish();});
        menu.add("Firmware selection",()->{
            Runnable open=()->startActivity(new Intent(this,FirmwareActivity.class));
            if(running.get())new AlertDialog.Builder(this).setTitle("Firmware selection").setMessage("Stop and release MDI before selecting diagnostic software?")
                .setNegativeButton("Keep running",null).setPositiveButton("Stop and select",(d,w)->{pendingTool=open;stop("Stopping before firmware selection");}).show();else open.run();})
            .add("MDI connection profile",()->{if(running.get()||permissionPending){setMessage("Stop MDI before changing its connection profile");return;}startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("application/json").addCategory(Intent.CATEGORY_OPENABLE),91);})
            .add("Preferences",controls::showPreferences)
            .add("Report issue",()->{if(!running.get())startActivity(new Intent(this,SupportReportActivity.class));})
            .add("Console",controls::showConsole)
            .add("About / Support",()->ProjectSupport.show(this))
            .add("Firmware controls help",controls::showHelp).show("App menu");
    }
    @Override protected void onActivityResult(int request,int result,Intent data){super.onActivityResult(request,result,data);
        if(request==91&&result==RESULT_OK&&data!=null&&data.getData()!=null){
            try(InputStream input=getContentResolver().openInputStream(data.getData())){MdiProfile.importProfile(this,input);setMessage("MDI connection profile saved · Connect and start");}
            catch(Exception invalid){setMessage("MDI connection profile could not be imported");}
        }
    }
    @Override protected void onStart(){super.onStart();foreground=true;}
    @Override protected void onStop(){foreground=false;if(running.get()||permissionPending)stop("App left foreground");super.onStop();}
    @Override protected void onDestroy(){closed=true;stop("Session closed");ui.removeCallbacksAndMessages(null);lcdPump.close();security.close();reports.close();observer.shutdown();unregisterReceiver(receiver);super.onDestroy();}
}
