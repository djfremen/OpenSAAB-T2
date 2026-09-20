// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.hardware.usb.*;
import android.os.*;
import android.widget.*;
import java.io.*;
import java.net.*;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/** Direct Chipsoft USB transport for identity, receive capture and original firmware. */
public final class ChipsoftUsbActivity extends Activity {
    volatile ConnectionAttempt connectionAttempt;
    Button reportConnection;
    TextView vinSummary,modeLabel;
    volatile VehicleIdentity vehicle;
    volatile long vehicleGeneration;
    Runnable pendingVehicleStart;
    boolean onlineDetails;
    final Object identityLock=new Object();
    AlertDialog vehicleStartPrompt;
    NativeLcdPump lcdPump;
    EmulatorHealthMonitor health;
    SecurityAccessView securityAccess;
    FirmwareMenuController menuShortcut;
    SessionWorkspace workspace;
    Tech2Controls controls;
    LinearLayout sessionDetails;
    Button launchFirmware;
    DtcReportView dtcReport;
    final java.util.concurrent.ExecutorService keyWorker=java.util.concurrent.Executors.newSingleThreadExecutor(r->new Thread(r,"chipsoft-keys"));
    final AtomicBoolean keyPending=new AtomicBoolean();
    IgnitionStatusView ignitionStatus;
    UsbManager manager; UsbDevice selected; TextView status,console;
    final AtomicBoolean running=new AtomicBoolean();
    boolean fullNative,keyStatus,audible,seeds,vinCheck,symbolOnly,nativeFirmware,receiveTest;volatile File nativeDirectory;ImageView nativeLcd;final Handler lcdHandler=new Handler(Looper.getMainLooper());volatile boolean cancelled; boolean pending,foreground;
    volatile ServerSocket server; volatile Socket client;
    final RequestGate requests=new RequestGate(); String permission;long permissionEpoch;
    final BroadcastReceiver receiver=new BroadcastReceiver(){
        public void onReceive(Context c,Intent i){
            if(permission.equals(i.getAction())){
                long epoch=i.getLongExtra("epoch",-1);if(!requests.pending(epoch))return;
                UsbDevice current=selected==null?null:manager.getDeviceList().get(selected.getDeviceName());
                if(current!=null && manager.hasPermission(current)){
                    if(foreground){pending=false;start(current,epoch);}
                    else log("USB permission granted; waiting for app to resume");
                }else {pending=false;requests.cancel();connectionFailure(ConnectionAttempt.Reason.PERMISSION_DENIED);log("USB permission denied; tap Identify to retry.");}
            }else if(UsbManager.ACTION_USB_DEVICE_DETACHED.equals(i.getAction())){
                UsbDevice d=i.getParcelableExtra(UsbManager.EXTRA_DEVICE);
                if(selected!=null && selected.equals(d)){connectionFailure(ConnectionAttempt.Reason.DISCONNECTED);log("Chipsoft disconnected");stop();}
            }
        }
    };
    public void onCreate(Bundle state){
        super.onCreate(state);fullNative=getIntent().getBooleanExtra("full_native",false);keyStatus=getIntent().getBooleanExtra("key_status",false);audible=getIntent().getBooleanExtra("audible",false);seeds=getIntent().getBooleanExtra("native_seeds",false);if((fullNative?1:0)+(keyStatus?1:0)+(seeds?1:0)+(audible?1:0)+(getIntent().getBooleanExtra("symbol_only",false)?1:0)>1){finish();return;}getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);vinCheck=getIntent().getBooleanExtra("vin_check",false);symbolOnly=getIntent().getBooleanExtra("symbol_only",false);nativeFirmware=fullNative || keyStatus || audible || seeds || symbolOnly || getIntent().getBooleanExtra("native_firmware",false);receiveTest=vinCheck || nativeFirmware || getIntent().getBooleanExtra("receive_test",false);manager=(UsbManager)getSystemService(USB_SERVICE);permission=getPackageName()+".CHIPSOFT_USB_PERMISSION";
        LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setPadding(12,12,12,12);root.setBackgroundColor(0xff101922);
        root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(SessionStyle.dp(this,12),i.getSystemWindowInsetTop()+SessionStyle.dp(this,8),SessionStyle.dp(this,12),i.getSystemWindowInsetBottom()+SessionStyle.dp(this,8));return i;});
        root.addView(new BrandHeader(this,"OpenSAAB T2 · Chipsoft"));
        if(nativeFirmware){
            modeLabel=new TextView(this);modeLabel.setTextSize(12);
            modeLabel.setText(fullNative?"Mode: Full native control":seeds?"Mode: Security-data collection":keyStatus?"Mode: Key-status test":(symbolOnly || audible)?"Mode: BCM reminder operation":"Mode: Read-only diagnostics");
            root.addView(modeLabel);
        }
        status=new TextView(this);status.setMaxLines(2);status.setEllipsize(android.text.TextUtils.TruncateAt.END);status.setText(fullNative?"Original Tech2 + CANdi · Full native control":keyStatus?"Original firmware · CIM key-status diagnostic test":vinCheck?"Read VIN / model year · startup discovery":audible?"Original firmware · BCM audible reminder":symbolOnly?"Original firmware · BCM Symbol Only operation":seeds?"Original firmware · collect security data":nativeFirmware?"Original Tech2 + CANdi · Chipsoft · read-only":receiveTest?"Raw P-bus / I-bus receive test · no diagnostic requests":"Identify adapter · no vehicle commands");root.addView(status);
        if(vinCheck || nativeFirmware){vinSummary=new TextView(this);vinSummary.setTextSize(14);vinSummary.setTextIsSelectable(true);vinSummary.setText("VIN: connect to identify vehicle");root.addView(vinSummary);}
        if(nativeFirmware){ignitionStatus=new IgnitionStatusView(this);root.addView(ignitionStatus);}
        LinearLayout actions=new LinearLayout(this);root.addView(actions,new LinearLayout.LayoutParams(-1,SessionStyle.dp(this,56)));
        Button identify=new Button(this);identify.setText(vinCheck?"Read vehicle VIN":nativeFirmware?"Connect and start":receiveTest?"Receive P-bus / I-bus · 8 seconds":"Identify connected Chipsoft");identify.setOnClickListener(v->requestVehicleStart());actions.addView(identify,new LinearLayout.LayoutParams(0,-2,1));
        Button stop=new Button(this);stop.setText("Stop USB");stop.setOnClickListener(v->stop());actions.addView(stop,new LinearLayout.LayoutParams(0,-2,1));
        Button back=new Button(this);back.setText("Back");back.setOnClickListener(v->{stop();finish();});actions.addView(back,new LinearLayout.LayoutParams(0,-2,1));
        SessionStyle.row(actions);SessionStyle.button(identify,true);
        reportConnection=new Button(this);reportConnection.setText("Report connection problem");reportConnection.setVisibility(android.view.View.GONE);
        reportConnection.setOnClickListener(v->{if(running.get()){status.setText("Waiting for USB cleanup — try again shortly");return;}startActivity(new Intent(this,SupportReportActivity.class));});root.addView(reportConnection);
        if(fullNative||seeds){
            securityAccess=new SecurityAccessView(this,seeds,()->nativeDirectory,()->running.get(),()->nativeKey("stop"),
                ()->openSecuritySession(true),this::resumeSecurityFirmware);
            securityAccess.setMenuKey(this::sendMenuKey);
            root.addView(securityAccess);

        }
        if(nativeFirmware){
            FirmwareMenuNavigator.Target target=FirmwareMenuNavigator.Target.fromShortcut(getIntent().getStringExtra("menu_shortcut"));
            menuShortcut=new FirmwareMenuController(this,target,()->nativeDirectory,()->foreground&&running.get()&&!cancelled,this::sendMenuKey);
            root.addView(menuShortcut);
            dtcReport=new DtcReportView(this,"chipsoft",()->nativeDirectory);root.addView(dtcReport);
            nativeLcd=new ImageView(this);
            health=new EmulatorHealthMonitor(this,this::stop);
            lcdPump=new NativeLcdPump(frame->{health.frame();nativeLcd.setImageBitmap(frame);});
            lcdHandler.postDelayed(new Runnable(){public void run(){if(securityAccess!=null)securityAccess.refresh();if(menuShortcut!=null)menuShortcut.refresh();if(ignitionStatus!=null)ignitionStatus.refresh(nativeDirectory,running.get() && !cancelled);updateWorkspace();if(!isFinishing())lcdHandler.postDelayed(this,(securityAccess!=null&&securityAccess.navigating())||(menuShortcut!=null&&menuShortcut.active())?100:1000);}},1000);
        }
        ScrollView scroll=new ScrollView(this);console=new TextView(this);console.setTextSize(13);console.setTypeface(android.graphics.Typeface.MONOSPACE);scroll.addView(console);
        if(nativeFirmware){
            root.removeViewAt(0);root.removeView(actions);actions.removeAllViews();
            sessionDetails=root;SessionStyle.stack(sessionDetails);sessionDetails.setPadding(0,0,0,0);sessionDetails.setOnApplyWindowInsetsListener(null);
            controls=new Tech2Controls(this,nativeLcd,scroll,code->{if(menuShortcut!=null)menuShortcut.cancel();if(securityAccess!=null)securityAccess.manualNavigation();nativeKey(String.format(java.util.Locale.ROOT,"0x%02x",code));});
            controls.setActions(this::showActions);
            workspace=new SessionWorkspace(this,"OpenSAAB T2 · Chipsoft",controls,this::showAppMenu,()->SessionSheet.show(this,"Vehicle and security details",sessionDetails));
            launchFirmware=identify;workspace.addLaunch(identify);updateWorkspace();setContentView(workspace);
        }else{root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));SessionStyle.stack(root);setContentView(root);}
        IntentFilter f=new IntentFilter(permission);f.addAction(UsbManager.ACTION_USB_DEVICE_DETACHED);
        if(Build.VERSION.SDK_INT>=33)registerReceiver(receiver,f,Context.RECEIVER_NOT_EXPORTED);else registerReceiver(receiver,f);
        if(state==null && getIntent().getBooleanExtra("auto_start",false))new Handler(Looper.getMainLooper()).post(this::requestVehicleStart);
    }
    void runShortcut(FirmwareMenuNavigator.Target target){
            if(target==FirmwareMenuNavigator.Target.SECURITY&&securityAccess!=null&&securityAccess.readyToProcess()){securityAccess.processCollected();return;}
            if(!running.get()||nativeDirectory==null||cancelled){status.setText("Start firmware before choosing a shortcut");Toast.makeText(this,"Start firmware before choosing a shortcut",Toast.LENGTH_LONG).show();return;}
            if(securityAccess!=null&&(securityAccess.busy()||securityAccess.collecting())){
                Toast.makeText(this,"Finish security collection first; the firmware controls remain available",Toast.LENGTH_LONG).show();return;
            }
            if(!fullNative&&(target==FirmwareMenuNavigator.Target.SECURITY||target==FirmwareMenuNavigator.Target.CLEAR_DTC)){
                Toast.makeText(this,"This connection is read-only. Start a full-control session for this task.",Toast.LENGTH_LONG).show();return;
            }
            if(securityAccess!=null)securityAccess.manualNavigation();
            menuShortcut.select(target);
    }
    void updateWorkspace(){
        if(workspace==null)return;
        String heading=vehicle==null?"Chipsoft · no vehicle identified":(running.get()?"Vehicle · ":"Last vehicle · ")+vehicle.modelYear+" · "+vehicle.platform;
        String security="";
        if(securityAccess!=null){TextView state=securityAccess.findViewWithTag("security-state");security=securityAccess.busy()?"Security: processing · details":state.getText().toString();}
        String progress=menuShortcut!=null&&menuShortcut.active()?menuShortcut.getText().toString():running.get()?"Session active · details":"Session stopped · details";
        workspace.summary(heading+"\n"+(security.isEmpty()?progress:security+"\n"+progress));
        launchFirmware.setVisibility(running.get()?android.view.View.GONE:android.view.View.VISIBLE);
    }
    public Dialog showActions(){
        return new SessionSheet.Menu(this)
            .add(securityAccess!=null&&securityAccess.readyToProcess()?"Process security data":"Get security access",()->runShortcut(FirmwareMenuNavigator.Target.SECURITY))
            .add("ECU information",()->runShortcut(FirmwareMenuNavigator.Target.ECU_INFO))
            .add("Original menus",()->{if(menuShortcut!=null)menuShortcut.cancel();})
            .add("Read DTC",()->runShortcut(FirmwareMenuNavigator.Target.READ_DTC))
            .add("Clear DTC",()->SessionSheet.confirmClear(this,()->runShortcut(FirmwareMenuNavigator.Target.CLEAR_DTC)))
            .add("Engine Data",()->runShortcut(FirmwareMenuNavigator.Target.ENGINE_DATA))
            .add("Saved DTC reports",()->DtcReportView.showSavedReports(this))
            .add("Vehicle and security details",()->SessionSheet.show(this,"Vehicle and security details",sessionDetails))
            .add(workspace.expanded()?"Show header":"Expand screen",workspace::toggleExpanded)
            .add("App menu",this::showAppMenu).show("Actions");
    }
    void showAppMenu(){
        SessionSheet.Menu menu=new SessionSheet.Menu(this);
        if(running.get())menu.add("Stop emulation / USB",this::stop);
        menu.add("Return to home",()->{stop();finish();})
            .add("Firmware selection",()->{if(idleTool())startActivity(new Intent(this,FirmwareActivity.class));})
            .add("Adapter & advanced tools",()->AdvancedFeatures.show(this,()->running.get()))
            .add("Report issue",()->{if(idleTool())startActivity(new Intent(this,SupportReportActivity.class));})
            .add("Console",controls::showConsole)
            .add("Check for updates",()->{if(idleTool())AppUpdates.show(this);})
            .add("About / Support",()->{if(idleTool())ProjectSupport.show(this);})
            .add("Firmware controls help",controls::showHelp).show("App menu");
    }
    boolean idleTool(){
        if(!running.get()&&!FirmwareGate.busy()&&!SecurityAccessView.workflowBusy())return true;
        Toast.makeText(this,"Stop the current session before opening this tool",Toast.LENGTH_LONG).show();return false;
    }
    boolean sendMenuKey(int code){
        File run=nativeDirectory;
        if(!foreground||!running.get()||cancelled||run==null||keyPending.get()||new File(run,"native-key.txt").exists())return false;
        nativeKey(String.format(java.util.Locale.ROOT,"0x%02x",code));return true;
    }
    void openSecuritySession(boolean collect){
        if(running.get())return;
        Intent next=new Intent(this,ChipsoftUsbActivity.class).putExtra(collect?"native_seeds":"full_native",true).putExtra("auto_start",true);
        if(selected!=null)next.putExtra("usb_device_name",selected.getDeviceName());
        startActivity(next);finish();
    }
    void resumeSecurityFirmware(){
        if(running.get()||cancelled||isFinishing()||isDestroyed())return;
        final UsbDevice device=selected;
        final VehicleIdentity identity=vehicle;
        final long generation=vehicleGeneration;
        if(device==null||identity==null){status.setText("Security access ready · reconnect your adapter to continue");return;}
        Runnable restart=()->{
            if(cancelled||generation!=vehicleGeneration||running.get()||isFinishing()||isDestroyed())return;
            if(FirmwareGate.sessionActive()||SecurityAccessView.workflowBusy()){
                status.setText("Security access ready · firmware is busy. Tap Return to firmware to retry");return;
            }
            UsbDevice attached=manager.getDeviceList().get(device.getDeviceName());
            if(attached==null||attached.getDeviceId()!=device.getDeviceId()||!manager.hasPermission(attached)){
                status.setText("Security access ready · reconnect your adapter to continue");return;
            }
            // Continue this identified connection without another VIN/start dialog.
            seeds=false;fullNative=true;
            getIntent().removeExtra("native_seeds");getIntent().putExtra("full_native",true);
            securityAccess.returnedToFirmware();
            modeLabel.setText("Mode: Full native control");
            connectionAttempt=new ConnectionAttempt(this,ConnectionAttempt.Adapter.CHIPSOFT);
            connectionAttempt.device(attached.getVendorId(),attached.getProductId());
            reportConnection.setVisibility(android.view.View.GONE);
            nativeDirectory=null;lcdPump.setDirectory(null);nativeLcd.setImageDrawable(null);
            status.setText("Security access ready · restarting firmware…");
            startIdentified(attached,identity,generation);
        };
        if(foreground)restart.run();
        else {pendingVehicleStart=restart;status.setText("Security access ready · firmware will restart when you return");}
    }
    void log(String s){android.util.Log.i("OpenSaabChipsoft",s);runOnUiThread(()->{if(console.length()>24000)console.setText("");console.append(s+"\n");if(s.startsWith("NATIVE_SESSION"))status.setText(audible?"Original firmware · BCM audible reminder":symbolOnly?"Original firmware · BCM Symbol Only operation":"Original firmware active · direct Chipsoft USB");else if(s.startsWith("USB_CLOSED"))status.setText(s.contains("cleanup_ok=true")?"Session ended · USB released":"Session ended · check cleanup log");else if(!s.startsWith("NATIVE_USB_TX"))status.setText(s);});}
    void connectionFailure(ConnectionAttempt.Reason reason){
        ConnectionAttempt attempt=connectionAttempt;
        if(attempt!=null)attempt.finish(ConnectionAttempt.Outcome.FAILED,reason);
        showConnectionReport();
    }
    void showConnectionReport(){runOnUiThread(()->{if(!isFinishing() && reportConnection!=null)reportConnection.setVisibility(android.view.View.VISIBLE);});}
    void requestVehicleStart(){
        if(running.get()||pending||(vehicleStartPrompt!=null&&vehicleStartPrompt.isShowing()))return;
        if(!vinCheck&&!nativeFirmware){discover();return;}
        LinearLayout prompt=new LinearLayout(this);prompt.setOrientation(LinearLayout.VERTICAL);int pad=SessionStyle.dp(this,20);prompt.setPadding(pad,pad,pad,0);
        TextView instructions=new TextView(this);instructions.setText("Connect the adapter and turn the ignition key to ON (dashboard lights on; engine does not need to run). ACC/accessory is not enough. A fresh VIN will be read before firmware starts.");prompt.addView(instructions);
        CheckBox details=new CheckBox(this);details.setText("Look up engine and color online (optional)");details.setChecked(getSharedPreferences("adapter_settings",MODE_PRIVATE).getBoolean("online_vehicle_details",false));prompt.addView(details);
        TextView disclosure=new TextView(this);disclosure.setText("If selected, your VIN is sent to OpenSAAB and its vehicle-data provider. Details load in the background; diagnostics do not wait for internet.");prompt.addView(disclosure);
        vehicleStartPrompt=new AlertDialog.Builder(this).setTitle("Turn the key to ON").setView(prompt)
            .setNegativeButton("Cancel",null).setPositiveButton(nativeFirmware?"Key is ON · Connect and start":"Key is ON · Read VIN",(d,w)->{
                onlineDetails=details.isChecked();getSharedPreferences("adapter_settings",MODE_PRIVATE).edit().putBoolean("online_vehicle_details",onlineDetails).apply();discover();
            }).show();
    }

    void discover(){
        if(FirmwareGate.busy()){log("Finish firmware installation first");return;}
        if(nativeFirmware){String missing=new FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty()){log("Firmware setup needed: "+missing);startActivity(new Intent(this,FirmwareActivity.class));return;}}
        if(SecurityAccessView.workflowBusy()){log("Finish security processing before starting another session");return;}
        if(!foreground || running.get() || pending)return;selected=null;vehicle=null;pendingVehicleStart=null;vehicleGeneration++;nativeDirectory=null;
        connectionAttempt=new ConnectionAttempt(this,ConnectionAttempt.Adapter.CHIPSOFT);reportConnection.setVisibility(android.view.View.GONE);
        if(lcdPump!=null){lcdPump.setDirectory(null);nativeLcd.setImageDrawable(null);}
        if(vinSummary!=null)vinSummary.setText("VIN: waiting for adapter");
        String chosenPath=getIntent().getStringExtra("usb_device_name");
        for(UsbDevice d:manager.getDeviceList().values())if((chosenPath==null || chosenPath.equals(d.getDeviceName())) && d.getVendorId()==0x0483 && d.getProductId()==0x5740){if(selected!=null){selected=null;connectionFailure(ConnectionAttempt.Reason.MULTIPLE_ADAPTERS);log("Multiple candidates; connect one Chipsoft.");return;}selected=d;}
        if(selected==null){connectionFailure(ConnectionAttempt.Reason.NO_ADAPTER);log("Chipsoft not detected; no session opened.");return;}
        connectionAttempt.device(selected.getVendorId(),selected.getProductId());connectionAttempt.stage(ConnectionAttempt.Stage.PERMISSION);
        long epoch=requests.begin();if(manager.hasPermission(selected))start(selected,epoch);
        else {pending=true;permissionEpoch=epoch;Intent i=new Intent(permission).setPackage(getPackageName()).putExtra("epoch",epoch);manager.requestPermission(selected,PendingIntent.getBroadcast(this,(int)epoch,i,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT));log("Waiting for Android USB permission");}
    }
    void start(UsbDevice d,long epoch){if(!foreground || !requests.consume(epoch) || !running.compareAndSet(false,true))return;cancelled=false;long generation=vehicleGeneration;
        if(vinSummary!=null)vinSummary.setText("Reading VIN from vehicle…");
        new Thread(()->probe(d,nativeFirmware,null,generation),"chipsoft-usb").start();}
    void probe(UsbDevice d,boolean identifyFirst,VehicleIdentity identity,long generation){
        // Finish the existing bounded VIN probe and release USB before firmware opens it.
        final boolean nativeFirmware=this.nativeFirmware && !identifyFirst;
        final boolean vinCheck=this.vinCheck || identifyFirst;
        final boolean receiveTest=this.receiveTest || identifyFirst;
        final boolean fullNative=this.fullNative && !identifyFirst;
        final boolean keyStatus=this.keyStatus && !identifyFirst;
        final boolean audible=this.audible && !identifyFirst;
        final boolean seeds=this.seeds && !identifyFirst;
        final boolean symbolOnly=this.symbolOnly && !identifyFirst;
        final ConnectionAttempt progress=connectionAttempt;
        VehicleIdentity identified=null;
        UsbDeviceConnection conn=null;UsbInterface ctl=null,data=null;UsbEndpoint rx=null,tx=null;
        boolean ctlClaim=false,dataClaim=false,quit=false,clean=true;
        UsbRequest read=null;boolean queued=false;PrintWriter out=null;java.lang.Process child=null;
        File run=new File(getFilesDir(),"chipsoft-"+java.util.UUID.randomUUID());
        FirmwareGate.Lease firmwareLease=null;
        try{
            firmwareLease=FirmwareGate.use();
            if(nativeFirmware){String missing=new FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty())throw new IOException("Firmware setup needed: "+missing);}
            if((symbolOnly || audible) && (!new File(getFilesDir(),audible?"chipsoft-audible-authority.json":"chipsoft-symbol-authority.json").isFile() || new File(getFilesDir(),"firmware/card-authorized.bin").length()!=33554432))throw new IOException("Load a validated SSA card and fresh Symbol Only authority before opening USB");
            if(!run.mkdir())throw new IOException("Cannot create session directory");
            try(BufferedWriter capture=new BufferedWriter(new FileWriter(new File(run,"usb.log")))){
                for(int i=0;i<d.getInterfaceCount();i++){
                    UsbInterface iface=d.getInterface(i);if(iface.getInterfaceClass()==2 && iface.getInterfaceSubclass()==2)ctl=iface;
                    if(iface.getInterfaceClass()==10 && iface.getAlternateSetting()==0){data=iface;for(int j=0;j<iface.getEndpointCount();j++){UsbEndpoint ep=iface.getEndpoint(j);if(ep.getType()==2){if(ep.getDirection()==128)rx=ep;else tx=ep;}}}
                }
                if(ctl==null || data==null || rx==null || tx==null || rx.getMaxPacketSize()!=64)throw new IOException("Unverified Chipsoft CDC layout");
                progress.stage(ConnectionAttempt.Stage.USB_OPEN);
                conn=manager.openDevice(d);if(conn==null)throw new IOException("USB open denied");
                progress.stage(ConnectionAttempt.Stage.INTERFACE_CLAIM);
                ctlClaim=conn.claimInterface(ctl,true);dataClaim=conn.claimInterface(data,true);if(!ctlClaim || !dataClaim)throw new IOException("USB interface claim failed");
                // STM CDC reference skips line coding and DTR/RTS to avoid resetting it.
                log("USB_OPEN "+d.getDeviceName()+" CDC; no line/control changes");
                read=new UsbRequest();if(!read.initialize(conn,rx))throw new IOException("USB read initialization failed");ByteBuffer buffer=ByteBuffer.allocateDirect(64);
                long drainEnd=SystemClock.elapsedRealtime()+1500;int discarded=0;
                while(!cancelled){
                    if(!queued){buffer.clear();if(!read.queue(buffer))throw new IOException("USB read queue failed");queued=true;}
                    try{UsbRequest done=conn.requestWait(100);if(done!=read)throw new IOException("Unexpected USB request");queued=false;discarded+=buffer.position();if(discarded>65536 || SystemClock.elapsedRealtime()>=drainEnd)throw new IOException("Adapter did not settle");}
                    catch(java.util.concurrent.TimeoutException quiet){break;}
                }
                if(cancelled)throw new IOException("Cancelled");log("STARTUP_DISCARD bytes="+discarded);
                progress.stage(ConnectionAttempt.Stage.TRANSPORT_START);
                server=new ServerSocket();server.bind(new InetSocketAddress("127.0.0.1",35674),1);server.setSoTimeout(5000);
                String token=java.util.UUID.randomUUID().toString().replace("-","");
                ProcessBuilder builder=new ProcessBuilder(getApplicationInfo().nativeLibraryDir+"/libchipsoft_probe.so",token,new File(run,"result.json").getPath());if(receiveTest)builder.command().add(vinCheck?"--vin-check":"--receive-test");
                if(nativeFirmware){
                    if(identity==null)throw new IOException("Read a fresh vehicle VIN before starting firmware");
                    synchronized(identityLock){
                        VehicleIdentity latest=vehicle;
                        VehicleSession.save(new File(run,VehicleSession.FILE),latest!=null&&latest.vin.equals(identity.vin)&&latest.observedUtc.equals(identity.observedUtc)?latest:identity);
                        nativeDirectory=run;
                    }
                    nativeDirectory=run;health.begin(run);lcdPump.setDirectory(run);File f=new File(getFilesDir(),"firmware");
                    for(String name:new String[]{"eprom.bin","opsys.dwn","card.bin","candi.bin"})if(!new File(f,name).isFile())throw new IOException("Missing original firmware: "+name);
                    builder=new ProcessBuilder(getApplicationInfo().nativeLibraryDir+"/libtech2_emu.so","--test-harness","--harness-target",symbolOnly?"dtc-link-1367":"native-manual","--candi-native-link","--candi-chipsoft-usb-token",token,"--candi-firmware",new File(f,"candi.bin").getPath(),"--boot",new File(f,"eprom.bin").getPath(),"--opsys",new File(f,"opsys.dwn").getPath(),"--max-insns","50000000000","--output-dir",run.getPath(),new File(f,(symbolOnly || audible)?"card-authorized.bin":"card.bin").getPath());if(symbolOnly)builder.command().add("--candi-chipsoft-symbol-only");if(seeds)builder.command().add("--candi-chipsoft-seeds");if(audible)builder.command().add("--candi-chipsoft-audible");
                    log("NATIVE_SESSION "+run.getName()+" requests=original-firmware command_policy="+(fullNative?"full-native":"restricted"));
                }
                if(fullNative)builder.environment().put("TECH2_CHIPSOFT_FULL_NATIVE","1");
                if(keyStatus)builder.environment().put("TECH2_CHIPSOFT_KEY_STATUS","1");
                if(nativeFirmware){progress.stage(ConnectionAttempt.Stage.FIRMWARE_START);builder.environment().put("OPENSAAB_PERFORMANCE_DIR",run.getAbsolutePath());}
                DemandStartup.configure(this,builder);
                child=builder.redirectErrorStream(true).redirectOutput(new File(run,"rust.log")).start();
                client=DemandStartup.accept(this,server,builder,child,()->cancelled);client.setSoTimeout(4000);client.setTcpNoDelay(true);InputStream in=client.getInputStream();out=new PrintWriter(new OutputStreamWriter(client.getOutputStream(),StandardCharsets.US_ASCII),true);
                if(!NanoProbeActivity.readLine(in).equals("HELLO "+token))throw new IOException("Session authentication failed");out.println(fullNative?"READY chipsoft-full-native":keyStatus?"READY chipsoft-key-status":vinCheck?"READY chipsoft-vin":audible?"READY chipsoft-audible":symbolOnly?"READY chipsoft-symbol-only":seeds?"READY chipsoft-seeds":nativeFirmware?"READY chipsoft-native":receiveTest?"READY chipsoft-receive":"READY chipsoft-identity");
                long until=fullNative?Long.MAX_VALUE:SystemClock.elapsedRealtime()+(nativeFirmware?330000:receiveTest?45000:8000);boolean sent=false;long received=0,transmits=0,diagnosticTransmits=0;
                while(!cancelled && SystemClock.elapsedRealtime()<until){
                    String cmd=NanoProbeActivity.readLine(in);if(cmd.equals("QUIT")){quit=true;break;}
                    if(cmd.startsWith("TX ")){
                        byte[] bytes=NanoProbeActivity.unhex(cmd.substring(3));
                        if(++transmits>(nativeFirmware?250000:5000) && !fullNative)throw new IOException("USB command budget exhausted (includes receive polling)");
                        if(bytes.length>1 && bytes[0]==15 && bytes[1]==0 && ++diagnosticTransmits>5000 && !fullNative)throw new IOException("Diagnostic transmission budget exhausted");
                        if(!(fullNative?ChipsoftCommandGate.fullNative(bytes):vinCheck?ChipsoftCommandGate.vinProbe(bytes):audible?ChipsoftCommandGate.audible(bytes):symbolOnly?ChipsoftCommandGate.symbolOnly(bytes):seeds?ChipsoftCommandGate.seeds(bytes):keyStatus?ChipsoftCommandGate.keyStatus(bytes):nativeFirmware?ChipsoftCommandGate.nativeRead(bytes):receiveTest?ChipsoftCommandGate.receive(bytes):cmd.equals("TX 0100000000000000")&&!sent))throw new IOException("Chipsoft command gate rejected request");
                        if(vinCheck && bytes[0]==15 && (diagnosticTransmits>2 || (bytes[32]&255)!=(diagnosticTransmits==1?2:0x30)))throw new IOException("VIN request/flow-control budget exceeded");
                        int opcode=(bytes[0]&255)|((bytes[1]&255)<<8);
                        if(opcode==1)progress.stage(ConnectionAttempt.Stage.IDENTIFICATION);
                        else if(opcode==4)progress.stage(ConnectionAttempt.Stage.CHANNEL_OPEN);
                        else if(opcode==15)progress.stage(vinCheck?ConnectionAttempt.Stage.VIN_REQUEST:ConnectionAttempt.Stage.SESSION);
                        sent=true;int n=conn.bulkTransfer(tx,bytes,bytes.length,500);if(nativeFirmware && bytes[0]==15)log(bytes.length==40 && bytes[33]==0x27 && bytes[34]==2?"NATIVE_USB_TX BCM security key (redacted)":"NATIVE_USB_TX "+cmd.substring(3));
                        capture.write(SystemClock.elapsedRealtime()+" TX "+cmd.substring(3)+" transferred="+n+"\n");capture.flush();if(!receiveTest)log("GET_INFO TX bytes="+n);if(n!=bytes.length)throw new IOException("Incomplete USB write");out.println("TXOK");
                    }else if(cmd.equals("READ") && sent){
                        if(!queued){buffer.clear();if(!read.queue(buffer))throw new IOException("USB read queue failed");queued=true;}
                        try{
                            UsbRequest done=conn.requestWait(20);if(done!=read)throw new IOException("USB request failed/detached");queued=false;
                            int n=buffer.position();byte[] bytes=new byte[n];buffer.flip();buffer.get(bytes);received+=n;if(received>(nativeFirmware?64*1024*1024:receiveTest?4*1024*1024:4096) && !fullNative)throw new IOException("USB receive byte budget exhausted");
                            String hex=NanoProbeActivity.hex(bytes,n);capture.write(SystemClock.elapsedRealtime()+" RX "+hex+"\n");capture.flush();if(!receiveTest)log("USB_RX "+hex);out.println(n==0?"EMPTY":"RX "+hex);
                        }catch(java.util.concurrent.TimeoutException e){out.println("EMPTY");}
                    }else throw new IOException("Identity-only command gate rejected request");
                    if(out.checkError())throw new IOException("Controller disconnected");
                }
                if(!quit)throw new IOException("Session cancelled or expired");
            }
        }catch(Exception e){if(cancelled)progress.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.USER_STOP);else {progress.failure(e);showConnectionReport();}if(!cancelled)SupportReports.recordError(this,e,false);log("ERROR "+e.getMessage());}
        finally{
            if(conn!=null){
                if(read!=null && queued){try{if(!read.cancel())clean=false;UsbRequest done=conn.requestWait(300);if(done!=read)clean=false;}catch(Exception e){clean=false;}}
                if(receiveTest && !quit && tx!=null){clean=false;for(String hex:new String[]{"050004000000880008800000","050004000000050005000000","2000000000000000"})try{byte[] b=NanoProbeActivity.unhex(hex);conn.bulkTransfer(tx,b,b.length,200);}catch(Exception ignored){}log("Emergency channel close sent; replies unverified");}
                if(dataClaim)clean&=conn.releaseInterface(data);if(ctlClaim)clean&=conn.releaseInterface(ctl);conn.close();if(read!=null)read.close();
            }
            if(out!=null && quit)out.println(clean?"CLOSED":"ERROR cleanup");
            if(child!=null)try{if(!child.waitFor(2,TimeUnit.SECONDS)){child.destroyForcibly();clean=false;}else {log("RUST_EXIT "+child.exitValue());if(child.exitValue()!=0)clean=false;}}catch(InterruptedException e){child.destroyForcibly();Thread.currentThread().interrupt();clean=false;}
            try{File result=new File(run,"result.json");if(quit && clean && result.length()>0){String report=new String(java.nio.file.Files.readAllBytes(result.toPath()),StandardCharsets.UTF_8);log(report);if(vinCheck){identified=VehicleSession.fromProbe(new org.json.JSONObject(report));}}}catch(Exception e){log("Result read failed");}
            if(symbolOnly || audible){new File(getFilesDir(),audible?"chipsoft-audible-authority.json":"chipsoft-symbol-authority.json").delete();new File(getFilesDir(),"firmware/card-authorized.bin").delete();}
            if(!clean && !cancelled){progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.CLEANUP_FAILED);showConnectionReport();}
            if(vinCheck && identified==null && !cancelled){progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.VIN_UNAVAILABLE);showConnectionReport();}
            if(quit && clean && !identifyFirst && (!vinCheck || identified!=null))progress.finish(ConnectionAttempt.Outcome.COMPLETED,ConnectionAttempt.Reason.NONE);
            closeSockets();if(firmwareLease!=null)firmwareLease.close();
            log("USB_CLOSED cleanup_ok="+clean+" session="+run.getName());
            if(vinCheck && identified!=null && !cancelled && generation==vehicleGeneration){
                if(!onlineDetails)identified=VehicleSession.unavailable(identified,"not_requested");
                try{synchronized(identityLock){if(!cancelled&&generation==vehicleGeneration){VehicleSession.save(new File(run,VehicleSession.FILE),identified);VehicleSession.save(new File(getFilesDir(),"last-vehicle.json"),identified);}}}
                catch(Exception e){log("Could not save vehicle identity");identified=null;}
            }
            if(nativeFirmware&&!vinCheck&&health!=null)health.ended(TransportFailure.from(run));
            running.set(false);
            final VehicleIdentity ready=identified;
            if(vinCheck)runOnUiThread(()->{
                if(cancelled || generation!=vehicleGeneration || isFinishing() || isDestroyed())return;
                if(ready==null){
                    vinSummary.setText("VIN unavailable · no vehicle session started");
                    new AlertDialog.Builder(this).setTitle("Could not identify vehicle")
                        .setMessage("Check the adapter connection and turn the ignition ON, then retry. A fresh VIN is needed to keep this session and its reports attached to the correct vehicle.")
                        .setNeutralButton("Report problem",(dialog,which)->startActivity(new Intent(this,SupportReportActivity.class)))
                        .setPositiveButton("Retry",(dialog,which)->discover()).setNegativeButton("Back",(dialog,which)->{stop();finish();}).show();return;
                }
                vehicle=ready;showVehicle(false);
                if(identifyFirst){pendingVehicleStart=()->startIdentified(d,ready,generation);if(foreground){Runnable launch=pendingVehicleStart;pendingVehicleStart=null;launch.run();}}
                if(onlineDetails)enrichVehicle(ready,run,generation);
            });
        }
    }
    void showVehicle(boolean disconnected){
        if(vinSummary==null || vehicle==null)return;
        vinSummary.setText((disconnected?"Last vehicle VIN: ":"VIN: ")+vehicle.vin+"\n"+vehicle.description()
            +("available".equals(vehicle.lookupStatus)?"":"pending".equals(vehicle.lookupStatus)?"\nOnline details loading · diagnostics can continue":"not_requested".equals(vehicle.lookupStatus)?"\nOnline details off · VIN saved":"\nOnline details unavailable · VIN saved"));
    }
    void enrichVehicle(VehicleIdentity fresh,File vinRun,long generation){
        new Thread(()->{
            VehicleIdentity enriched=VehicleSession.lookup(fresh,()->cancelled||generation!=vehicleGeneration);
            synchronized(identityLock){
                if(cancelled||generation!=vehicleGeneration)return;
                vehicle=enriched;
                try{
                    VehicleSession.save(new File(vinRun,VehicleSession.FILE),enriched);
                    VehicleSession.save(new File(getFilesDir(),"last-vehicle.json"),enriched);
                    File nativeRun=nativeDirectory;
                    if(nativeRun!=null)VehicleSession.save(new File(nativeRun,VehicleSession.FILE),enriched);
                }catch(Exception e){android.util.Log.w("OpenSaabChipsoft","Could not save optional vehicle details",e);}
            }
            runOnUiThread(()->{if(!cancelled&&generation==vehicleGeneration&&!isDestroyed()){showVehicle(false);updateWorkspace();}});
        },"vehicle-details").start();
    }
    void startIdentified(UsbDevice d,VehicleIdentity identity,long generation){
        if(cancelled || generation!=vehicleGeneration || !foreground || isFinishing())return;
        UsbDevice attached=manager.getDeviceList().get(d.getDeviceName());
        if(attached==null || attached.getDeviceId()!=d.getDeviceId() || attached.getVendorId()!=d.getVendorId() || attached.getProductId()!=d.getProductId() || !manager.hasPermission(attached)){
            connectionFailure(ConnectionAttempt.Reason.DISCONNECTED);stop();status.setText("Adapter changed · reconnect to identify vehicle");return;
        }
        if(!running.compareAndSet(false,true))return;
        new Thread(()->probe(attached,false,identity,generation),"chipsoft-firmware").start();
    }
    void nativeKey(String code){
        File run=nativeDirectory;
        if(!running.get() || cancelled || run==null)return;
        android.util.Log.i("OpenSaabPerf","KEY code="+code+" unix_ms="+System.currentTimeMillis());
        // Do not queue up delayed vehicle-menu actions behind a stalled guest.
        if(!keyPending.compareAndSet(false,true)){log("Wait for previous key");return;}
        try{keyWorker.execute(()->{
            try{
                if(!running.get() || cancelled || nativeDirectory!=run)return;
                if(health!=null)health.input();
                File dest=new File(run,"native-key.txt");
                if(dest.exists()){log("Wait for previous key");return;}
                File pending=new File(run,"native-key.tmp");
                java.nio.file.Files.write(pending.toPath(),(code+"\n").getBytes(StandardCharsets.US_ASCII));
                if(!running.get() || cancelled || nativeDirectory!=run){pending.delete();return;}
                java.nio.file.Files.move(pending.toPath(),dest.toPath(),java.nio.file.StandardCopyOption.ATOMIC_MOVE);
                android.util.Log.i("OpenSaabPerf","KEY_PUBLISHED code="+code+" unix_ms="+System.currentTimeMillis());
            }catch(Exception e){log("Key input failed: "+e.getMessage());}
            finally{keyPending.set(false);}
        });}catch(java.util.concurrent.RejectedExecutionException stopped){keyPending.set(false);}
    }
    void closeSockets(){try{if(client!=null)client.close();}catch(IOException ignored){}try{if(server!=null)server.close();}catch(IOException ignored){}}
    void stop(){if(health!=null)health.expectedStop();if(menuShortcut!=null)menuShortcut.cancel();if(vehicleStartPrompt!=null){vehicleStartPrompt.dismiss();vehicleStartPrompt=null;}if(connectionAttempt!=null)connectionAttempt.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.USER_STOP);requests.cancel();pending=false;cancelled=true;vehicleGeneration++;pendingVehicleStart=null;showVehicle(true);closeSockets();}
    protected void onResume(){super.onResume();foreground=true;
        if(pendingVehicleStart!=null){Runnable launch=pendingVehicleStart;pendingVehicleStart=null;launch.run();}
        if(pending && requests.pending(permissionEpoch) && selected!=null){
            UsbDevice current=manager.getDeviceList().get(selected.getDeviceName());
            if(current!=null && manager.hasPermission(current)){pending=false;start(current,permissionEpoch);}
        }
    }
    protected void onPause(){foreground=false;super.onPause();}
    protected void onDestroy(){if(menuShortcut!=null)menuShortcut.close();if(dtcReport!=null)dtcReport.close();if(securityAccess!=null)securityAccess.close();stop();keyWorker.shutdownNow();if(lcdPump!=null)lcdPump.close();lcdHandler.removeCallbacksAndMessages(null);unregisterReceiver(receiver);super.onDestroy();}
}
