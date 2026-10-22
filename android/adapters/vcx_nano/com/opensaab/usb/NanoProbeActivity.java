// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import static com.opensaab.usb.UsbBridgeCodec.*;

import android.app.*;
import android.content.*;
import android.hardware.usb.*;
import android.os.*;
import android.widget.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.TimeUnit;
import java.security.SecureRandom;
import android.view.WindowManager;

/** Android owns USB permission and bounded transfers; Rust owns VCX framing.
 * Identification, raw allocation, and an explicit eight-second receive test.
 * Normal CAN mode may acknowledge bus frames; it is not silent listen-only.
 * VIN mode admits one captured read-only request plus its flow-control frame.
 * No authorization replay or other diagnostic TX is admitted.
 */
public class NanoProbeActivity extends Activity {
    private volatile java.lang.Process mcpProcess;
    private final AndroidMcp.Owner mcpOwner=new AndroidMcpSession(()->this.nativeDirectory,()->this.running.get(),()->!this.nativeStop.pending()&&!this.cancelled&&this.nativeKeyPump!=null&&this.nativeKeyPump.idle()&&this.nativeDirectory!=null&&!new File(this.nativeDirectory,"native-key.txt").exists(),()->this.mcpProcess!=null&&this.mcpProcess.isAlive(),this::sendMenuKey,this::cancel,"vcx_nano");

    volatile ConnectionAttempt connectionAttempt;
    Button reportConnection;
    String PERMISSION;
    static final String TAG="OpenSaabNano";
    // Returning Back must not let a new activity reopen USB during old cleanup.
    static final AtomicBoolean usbSessionOwned=new AtomicBoolean(false);
    public static boolean sessionActive(){return usbSessionOwned.get();}
    final NanoRebootHandoff rebootHandoff=new NanoRebootHandoff();
    volatile boolean sessionCleanupConfirmed;
    TextView modeBadge;
    SessionModes.Mode activeMode(){return nativeResetCounters?SessionModes.Mode.RESET:nativeFullNative?SessionModes.Mode.FULL:nativeClearDtc?SessionModes.Mode.CLEAR:(nativeSeeds||nativeSecurityCollect)?SessionModes.Mode.SEEDS:SessionModes.Mode.READ;}
    void refreshModeBadge(){if(modeBadge!=null)SessionModeBadge.apply(modeBadge,activeMode(),false);}
    boolean rebootRequestRejected;
    UsbManager manager;
    TextView text, summary;
    Button retry;
    ScrollView scroll;
    String token;
    final AtomicBoolean running=new AtomicBoolean(false);
    volatile boolean cancelled=false;
    final NanoNativeStopGate nativeStop=new NanoNativeStopGate();
    volatile boolean nativeProtocolCleanupConfirmed=false;
    volatile ServerSocket server;
    volatile Socket client;
    UsbDevice selected;
    boolean foreground=false;
    boolean permissionPending=false;
    final RequestGate requestGate=new RequestGate();
    long activeSessionId;
    boolean channelTest=false;
    boolean channelTestSw=false;
    boolean receiveTest=false;
    boolean hsVin=false;
    boolean dlcVoltage=false;
    boolean rebootNano=false;
    boolean initHandshake=false;
    boolean skipInit=false;
    boolean nativeSeeds=false;
    boolean nativeDtc=false;
    boolean nativeEngineData=false;
    boolean nativeClearDtc=false;
    boolean nativeResetCounters=false;
    boolean nativeResetRejected=false;
    boolean nativeKeyStatus=false;
    boolean nativeFullNative=false;
    boolean nativeFullNativeRejected=false;
    boolean nativeSecurityCollect=false;
    volatile boolean securityVinProbe=false;
    volatile VehicleIdentity securityVehicle;
    volatile UsbDevice securityDevice;
    SecurityAccessView securityAccess;
    Runnable pendingSecurityStart;
    boolean nativeFirmware(){return !securityVinProbe && (nativeSeeds || nativeDtc || nativeKeyStatus || nativeFullNative);}
    boolean fullNativeActive(){return nativeFullNative && !securityVinProbe;}
    IgnitionStatusView ignitionStatus;
    volatile File nativeDirectory;
    volatile String nativeFailure;
    volatile boolean readModeRejected;
    ImageView nativeLcd;
    private Tech2Controls controls;
    EmulatorHealthMonitor health;
    final Handler lcdHandler=new Handler(Looper.getMainLooper());
    NativeLcdPump nativeLcdPump;
    volatile InteractiveKeyPump nativeKeyPump;
    TextView title,purpose;
    int logLines=0;
    final BroadcastReceiver receiver=new BroadcastReceiver() {
        public void onReceive(Context c, Intent i) {
            UsbDevice d=i.getParcelableExtra(UsbManager.EXTRA_DEVICE);
            if (PERMISSION.equals(i.getAction())) {
                long requestId=i.getLongExtra("request_epoch",-1);
                if(!requestGate.pending(requestId)){log("Ignored stale or cancelled USB permission callback.");return;}
                permissionPending=false;
                if (selected==null || (d!=null && !selected.equals(d))) return;
                UsbDevice current=manager.getDeviceList().get(selected.getDeviceName());
                // Immutable callbacks may omit extras; query the actual OS grant.
                if (current!=null && manager.hasPermission(current)) {
                    if(foreground)startProbe(current,requestId);else {requestGate.cancel();log("Permission granted. Return to this app and choose a test.");}
                }
                else {requestGate.cancel();connectionFailure(ConnectionAttempt.Reason.PERMISSION_DENIED);summary.setText("USB permission denied — no test started");log("USB permission denied; no connection opened.");}
            } else if (UsbManager.ACTION_USB_DEVICE_DETACHED.equals(i.getAction()) && selected!=null && selected.equals(d)) {
                permissionPending=false;rebootHandoff.cancel();connectionFailure(ConnectionAttempt.Reason.DISCONNECTED);log("Adapter disconnected."); securityVehicle=null;securityDevice=null;pendingSecurityStart=null;cancel();
            } else if (UsbManager.ACTION_USB_DEVICE_ATTACHED.equals(i.getAction()) && foreground && !running.get()) {
                if(d!=null && NanoProfile.PROFILE.matches(d.getVendorId(),d.getProductId()))log("Nano connected. Choose a test to start; attachment never restarts a previous test.");
            }
        }
    };
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);com.opensaab.usb.BackNavigation.install(this,this::leaveScreen);
        rebootNano=getIntent().getBooleanExtra("reboot_nano",false);
        nativeFullNative=getIntent().getBooleanExtra("nano_full_native",false);
        nativeSecurityCollect=getIntent().getBooleanExtra("nano_security_collect",false);
        final boolean fullNativeRequested=nativeFullNative || nativeSecurityCollect;
        nativeKeyStatus=getIntent().getBooleanExtra("native_key_status",false);
        nativeSeeds=getIntent().getBooleanExtra("native_seed",false) || nativeSecurityCollect;
        nativeEngineData=getIntent().getBooleanExtra("native_engine_data",false);
        nativeClearDtc=getIntent().getBooleanExtra("native_clear_dtc",false);
        nativeResetCounters=getIntent().getBooleanExtra("native_reset_counters",false);
        nativeDtc=getIntent().getBooleanExtra("native_dtc",false) || nativeClearDtc || nativeEngineData || nativeResetCounters;
        boolean conflictingModes=(nativeSeeds && nativeDtc) || (nativeEngineData && nativeClearDtc) || (nativeKeyStatus && (nativeSeeds || nativeDtc)) || (nativeFullNative && (nativeSeeds || nativeDtc || nativeKeyStatus)) || (nativeSecurityCollect && getIntent().getBooleanExtra("native_seed",false));
        conflictingModes |= nativeResetCounters && (getIntent().getBooleanExtra("native_dtc",false) || nativeEngineData || nativeClearDtc || nativeSeeds || nativeKeyStatus || nativeFullNative);
        if(conflictingModes){nativeSeeds=false;nativeDtc=false;nativeClearDtc=false;nativeResetCounters=false;nativeEngineData=false;nativeKeyStatus=false;nativeFullNative=false;nativeSecurityCollect=false;}
        PERMISSION=getPackageName()+".USB_PERMISSION";
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        manager=(UsbManager)getSystemService(USB_SERVICE);
        LinearLayout root=new LinearLayout(this); root.setOrientation(LinearLayout.VERTICAL); root.setPadding(24,48,24,24);
        root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(24,i.getSystemWindowInsetTop()+24,24,i.getSystemWindowInsetBottom()+24);return i;});
        title=new TextView(this); setTitle(AppBuildProfile.programName(this));title.setText(AppBuildProfile.programName(this)); title.setTextSize(23); root.addView(title);
        TextView architecture=new TextView(this);architecture.setTextSize(12);architecture.setText(EmulatorArchitecture.label(new File(getApplicationInfo().nativeLibraryDir,"libtech2_emu.so")));root.addView(architecture);
        purpose=new TextView(this); purpose.setText(nativeFullNative?"Original Tech2 + CANdi firmware · direct USB\nFull native control · original Add / Remove menus":nativeKeyStatus?"Original Tech2 + CANdi firmware · direct USB\nSelect Diagnostics / All / Service / Check Ignition Key Status":nativeResetCounters?"Original Tech2 + CANdi firmware · direct USB\nReset T8 misfire and knock counters · follow original menus":nativeEngineData?"Original Tech2 + CANdi firmware · direct USB\nSelect vehicle year and model, then Engine / Engine Control":nativeClearDtc?"Original Tech2 + CANdi firmware · direct USB\nClear mode enabled · follow original prompts":nativeDtc?"Original Tech2 + CANdi firmware · direct USB\nRead codes · follow original ignition prompts":nativeSeeds?"Original Tech2 + CANdi firmware · direct USB\nSeed collection only · follow original ignition prompts":"Direct Android USB · bounded channel tests\nOptional read-only VIN check · no security access"); root.addView(purpose);
        if(nativeFirmware() || nativeSecurityCollect){modeBadge=SessionModeBadge.create(this,activeMode(),false);root.addView(modeBadge);}
        summary=new TextView(this);summary.setMaxLines(2);summary.setEllipsize(android.text.TextUtils.TruncateAt.END);summary.setText("Ready · no active USB session");summary.setTextSize(15);root.addView(summary);
        // App navigation stays above the LCD and console, including on errors.
        // Back leaves this screen; the firmware EXIT key only navigates guest menus.
        LinearLayout recovery=new LinearLayout(this);root.addView(recovery);
        Button back=new Button(this);back.setText("Back");back.setOnClickListener(v->leaveScreen());
        recovery.addView(back,new LinearLayout.LayoutParams(0,-2,1));
        Button stop=new Button(this);stop.setText("Stop");stop.setOnClickListener(v->cancel());
        recovery.addView(stop,new LinearLayout.LayoutParams(0,-2,1));
        if(nativeFirmware()){
            retry=new Button(this);retry.setText("Retry");retry.setOnClickListener(v->discover());
            recovery.addView(retry,new LinearLayout.LayoutParams(0,-2,1));
            Button menu=new Button(this);menu.setText("App menu");
            menu.setOnClickListener(v->new SessionSheet.Menu(this)
                .add("Preferences",controls::showPreferences)
                .add("AI control (MCP)",()->AndroidMcp.show(this))
                .add("Console",controls::showConsole)
                .add("Firmware controls help",controls::showHelp).show("App menu"));
            recovery.addView(menu,new LinearLayout.LayoutParams(0,-2,1));
        }
        reportConnection=new Button(this);reportConnection.setText("Report connection problem");reportConnection.setVisibility(android.view.View.GONE);
        reportConnection.setOnClickListener(v->{if(running.get()){summary.setText("Waiting for USB cleanup — try again shortly");return;}startActivity(new Intent(this,SupportReportActivity.class));});root.addView(reportConnection);
        if(nativeFirmware()){ignitionStatus=new IgnitionStatusView(this);root.addView(ignitionStatus);}
        if(nativeFullNative || nativeSecurityCollect){
            securityAccess=new SecurityAccessView(this,nativeSecurityCollect,()->nativeDirectory,()->running.get(),this::stopForSecurityHandoff,
                ()->openSecuritySession(true),this::resumeSecurityFirmware);
            // Guided collection sends recognized menu keys only; physical prompts stay manual.
            securityAccess.setMenuKey(this::sendSecurityMenuKey);
            root.addView(securityAccess);
        }
        Button start=new Button(this); start.setText("Identify connected Nano"); start.setOnClickListener(v->selectMode(false)); root.addView(start);
        Button channel=new Button(this); channel.setText("Test channel open / close"); channel.setOnClickListener(v->selectMode(true)); root.addView(channel);
        Button receive=new Button(this); receive.setText("Receive P-bus / I-bus · 8 seconds"); receive.setOnClickListener(v->{if(!running.get() && !permissionPending){receiveTest=true;channelTest=false;channelTestSw=false;dlcVoltage=false;hsVin=false;rebootNano=false;initHandshake=false;skipInit=false;discover();}}); root.addView(receive);
        Button vin=new Button(this);vin.setText("P-bus VIN transport check");vin.setOnClickListener(v->{if(!running.get() && !permissionPending){receiveTest=true;channelTest=false;channelTestSw=false;dlcVoltage=false;hsVin=true;rebootNano=false;initHandshake=false;skipInit=false;discover();}});root.addView(vin);
        // Explicit adapter controls; USB attachment never invokes a reboot or initialization.
        if(!nativeFirmware()){
            if(!rebootNano&&(getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)!=0){Button init=new Button(this);init.setText("Initialize Nano");init.setOnClickListener(v->selectStartup(false));root.addView(init);}
            Button reboot=new Button(this);reboot.setText("Reboot Nano");reboot.setTag("nano-reboot");reboot.setOnClickListener(v->selectStartup(true));root.addView(reboot);
        }
        if(rebootNano){
            purpose.setText("Reboot Nano · adapter software restart\nThe app closes USB afterward. Connect and start to reconnect.");
            start.setVisibility(android.view.View.GONE);channel.setVisibility(android.view.View.GONE);receive.setVisibility(android.view.View.GONE);vin.setVisibility(android.view.View.GONE);
        }
        if(nativeFirmware()) {
            start.setVisibility(android.view.View.GONE);channel.setVisibility(android.view.View.GONE);receive.setVisibility(android.view.View.GONE);vin.setVisibility(android.view.View.GONE);
            dtcReport=new DtcReportView(this,"nano",()->nativeDirectory);root.addView(dtcReport);
            nativeLcd=new ImageView(this);health=new EmulatorHealthMonitor(this,this::cancel);
            nativeLcdPump=new NativeLcdPump(frame->{
                if(frame==null){nativeLcd.setImageDrawable(null);return;}
                if(!running.get() || cancelled || nativeStop.pending())return;
                if(health!=null)health.frame();nativeLcd.setImageBitmap(frame);
                if(controls!=null)controls.showFirstUseGuide();
            });
            lcdHandler.postDelayed(new Runnable(){public void run(){updateNativeLcd();if(!isFinishing())lcdHandler.postDelayed(this,1000);}},1000);
        }
        scroll=new ScrollView(this); text=new TextView(this); text.setTextSize(13);text.setTypeface(android.graphics.Typeface.MONOSPACE); scroll.addView(text); if(nativeFirmware()){controls=new Tech2Controls(this,nativeLcd,scroll,code->{AndroidMcp.manual();if(securityAccess!=null)securityAccess.manualNavigation();nativeKey(String.format(java.util.Locale.ROOT,"0x%02x",code));});controls.setActions(this::showActions);root.addView(controls,new LinearLayout.LayoutParams(-1,0,1));}else root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        HeadunitLayout.apply(root);
        setContentView(root);
        IntentFilter f=new IntentFilter(PERMISSION); f.addAction(UsbManager.ACTION_USB_DEVICE_DETACHED); f.addAction(UsbManager.ACTION_USB_DEVICE_ATTACHED);
        if (Build.VERSION.SDK_INT>=33) registerReceiver(receiver,f,Context.RECEIVER_NOT_EXPORTED); else registerReceiver(receiver,f);
        log(nativeFirmware()?"Ready for original firmware with direct Nano USB.":"Ready. Connect Nano, then tap Identify. The test runs entirely on this phone.");
        channelTest=getIntent().getBooleanExtra("channel_test",false);
        channelTestSw=getIntent().getBooleanExtra("channel_test_sw",false);
        receiveTest=getIntent().getBooleanExtra("receive_test",false);
        hsVin=getIntent().getBooleanExtra("hs_vin_check",false);if(hsVin || nativeFirmware())receiveTest=true;
        dlcVoltage=getIntent().getBooleanExtra("dlc_voltage",false);
        rebootNano=getIntent().getBooleanExtra("reboot_nano",false);
        initHandshake=getIntent().getBooleanExtra("init_handshake",false);
        skipInit=getIntent().getBooleanExtra("skip_init",false);
        boolean privateBuild=(getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)!=0;
        conflictingModes |= (nativeKeyStatus && (channelTest || channelTestSw || getIntent().hasExtra("nano_probe_mode") || getIntent().getBooleanExtra("receive_test",false) || hsVin || dlcVoltage || rebootNano || initHandshake || skipInit)) || (rebootNano && initHandshake) || ((rebootNano || initHandshake) && (nativeFirmware() || channelTest || receiveTest || hsVin || dlcVoltage)) || ((initHandshake || skipInit || nativeKeyStatus) && !privateBuild) || (skipInit && (!channelTest || nativeFirmware() || receiveTest || hsVin || dlcVoltage || rebootNano || initHandshake));
        boolean otherNativeProfile=getIntent().getBooleanExtra("native_seed",false) || getIntent().getBooleanExtra("native_dtc",false) || getIntent().getBooleanExtra("native_clear_dtc",false) || getIntent().getBooleanExtra("native_engine_data",false) || getIntent().getBooleanExtra("native_key_status",false) || getIntent().getBooleanExtra("native_reset_counters",false);
        boolean adapterTest=channelTest || channelTestSw || getIntent().hasExtra("nano_probe_mode") || getIntent().getBooleanExtra("receive_test",false) || hsVin || dlcVoltage || rebootNano || initHandshake || skipInit;
        if(nativeResetCounters && adapterTest)conflictingModes=true;
        nativeResetRejected=getIntent().getBooleanExtra("native_reset_counters",false) && conflictingModes;
        if(nativeFullNative && !NativeCommandGate.fullNativeModeAllowed(otherNativeProfile,adapterTest))conflictingModes=true;
        if(nativeSecurityCollect && !NativeCommandGate.securityCollectionModeAllowed(otherNativeProfile,adapterTest))conflictingModes=true;
        rebootRequestRejected=rebootNano && conflictingModes;
        nativeFullNativeRejected=fullNativeRequested && conflictingModes;
        if(conflictingModes)log("ERROR Conflicting native modes; no session started.");
        if(!conflictingModes && getIntent().getBooleanExtra("auto_start",false))new Handler(Looper.getMainLooper()).post(this::discover);
    }
    void log(String s) {
        android.util.Log.i(TAG,s);
        runOnUiThread(()->{ if (++logLines>150) { text.setText("");logLines=0; } text.append(s+"\n");scroll.post(()->scroll.fullScroll(android.view.View.FOCUS_DOWN)); if(s.startsWith("ERROR") || s.contains("RAW_CHANNEL_STARTED") || s.contains("CAN_RX_SUMMARY") || s.startsWith("USB_CLOSED"))summary.setText(s); });
    }
    void selectMode(boolean channel) {
        if(running.get() || permissionPending) { log("Finish or stop the current request first.");return; }
        channelTest=channel;channelTestSw=false;receiveTest=false;hsVin=false;dlcVoltage=false;rebootNano=false;initHandshake=false;skipInit=false;discover();
    }
    void selectStartup(boolean reboot){
        if(running.get() || permissionPending){log("Finish or stop the current request first.");return;}
        channelTest=false;channelTestSw=false;receiveTest=false;hsVin=false;dlcVoltage=false;skipInit=false;rebootNano=reboot;initHandshake=!reboot;discover();
    }
    void connectionFailure(ConnectionAttempt.Reason reason){
        if(connectionAttempt!=null)connectionAttempt.finish(ConnectionAttempt.Outcome.FAILED,reason);
        showConnectionReport();
    }
    void showConnectionReport(){runOnUiThread(()->{if(!isFinishing() && reportConnection!=null)reportConnection.setVisibility(android.view.View.VISIBLE);});}
    void discover() {
        if(nativeResetRejected){log("ERROR Counter reset requires an exclusive maintenance session.");return;}
        if(rebootNano&&!NanoRebootAction.idle()){log("Finish the current session before rebooting Nano");return;}
        if(rebootRequestRejected){log("Reboot Nano requires a separate adapter-only session");return;}
        if(nativeFullNativeRejected){log("ERROR Full native control requires an exclusive Nano session.");return;}
        if(FirmwareGate.busy()){log("Finish firmware installation first");return;}
        if(nativeFirmware()){String missing=new FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty()){log("Firmware setup needed: "+missing);startActivity(new Intent(this,FirmwareActivity.class));return;}}
        if(SecurityAccessView.workflowBusy()){log("Finish security processing before starting another session");return;}
        if(!foreground){log("Return to this app before starting a USB test.");return;}
        if (running.get()) { log("A bounded probe is already active."); return; }
        if (permissionPending) { log("Waiting for the Android USB permission dialog."); return; }
        connectionAttempt=new ConnectionAttempt(this,ConnectionAttempt.Adapter.NANO);reportConnection.setVisibility(android.view.View.GONE);
        long requestId=requestGate.begin();
        setResult(RESULT_CANCELED,new Intent().putExtra("summary","USB test has no completed result"));
        selected=null;
        String chosenPath=getIntent().getStringExtra("usb_device_name");
        for (UsbDevice d:manager.getDeviceList().values()) {
            if(chosenPath!=null && !chosenPath.equals(d.getDeviceName()))continue;
            if (NanoProfile.PROFILE.matches(d.getVendorId(),d.getProductId())) {
                if (selected!=null) { selected=null; requestGate.cancel();connectionFailure(ConnectionAttempt.Reason.MULTIPLE_ADAPTERS);log("Multiple Nano candidates; select one physically."); return; }
                selected=d;
            }
        }
        if (selected==null) { requestGate.cancel();connectionFailure(ConnectionAttempt.Reason.NO_ADAPTER);summary.setText("Nano not found — no test started");log("No matching USB adapter. Connect Nano and retry."); return; }
        if(getIntent().hasExtra("usb_device_id")&&selected.getDeviceId()!=getIntent().getIntExtra("usb_device_id",-1)){
            selected=null;requestGate.cancel();connectionFailure(ConnectionAttempt.Reason.DISCONNECTED);log("Selected Nano changed · choose it again before rebooting");return;
        }
        connectionAttempt.device(selected.getVendorId(),selected.getProductId());connectionAttempt.stage(ConnectionAttempt.Stage.PERMISSION);
        log("Candidate 1A86:55D3 · "+selected.getDeviceName());
        if (manager.hasPermission(selected)) startProbe(selected,requestId);
        else {
            permissionPending=true;
            Intent intent=new Intent(PERMISSION).setPackage(getPackageName()).putExtra("request_epoch",requestId);
            manager.requestPermission(selected,PendingIntent.getBroadcast(this,(int)requestId,intent,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT));
            log("Waiting for Android USB permission.");
        }
    }
    void startProbe(UsbDevice device,long requestId) {
        if(rebootNano&&(!NanoRebootAction.idle()||(getIntent().hasExtra("usb_device_id")&&device.getDeviceId()!=getIntent().getIntExtra("usb_device_id",-1)))){
            requestGate.cancel();permissionPending=false;summary.setText("Reboot not started · selected Nano changed or another session is active");return;
        }
        if(!foreground || !requestGate.consume(requestId) || !running.compareAndSet(false,true))return;
        if(!usbSessionOwned.compareAndSet(false,true)){
            running.set(false);requestGate.cancel();
            summary.setText("Previous USB session is closing — retry in a moment");
            return;
        }
        activeSessionId=requestId;
        if((nativeFullNative || nativeSecurityCollect) && securityVehicle!=null && (securityDevice==null || !securityDevice.getDeviceName().equals(device.getDeviceName()) || securityDevice.getDeviceId()!=device.getDeviceId())){
            securityVehicle=null;securityDevice=null;
        }
        securityVinProbe=(nativeFullNative || nativeSecurityCollect) && securityVehicle==null;
        if(retry!=null)retry.setEnabled(false);
        nativeDirectory=null;
        nativeFailure=null;readModeRejected=false;sessionCleanupConfirmed=false;
        if(nativeLcd!=null)nativeLcd.setImageDrawable(null);
        summary.setText("Starting USB test… Stop remains available");
        cancelled=false;nativeStop.reset();nativeProtocolCleanupConfirmed=false;
        byte[] random=new byte[16];new SecureRandom().nextBytes(random);token=hex(random,random.length);
        new Thread(()->probe(device,requestId),"nano-usb-probe").start();
    }
    void control(UsbDeviceConnection c,int request,int value,int index) throws IOException {
        int rc=c.controlTransfer(0x40,request,value,index,null,0,500);
        log(String.format("USB_CONTROL request=%02X value=%04X index=%04X rc=%d",request,value,index,rc));
        if(rc!=0)throw new IOException("USB control request failed");
    }
    /** A timeout waits again on the same request; it never cancels consumed bytes. */
    static final class QueuedReader {
        final UsbDeviceConnection connection; final UsbRequest request=new UsbRequest();
        final java.nio.ByteBuffer buffer; boolean queued;
        QueuedReader(UsbDeviceConnection connection,UsbEndpoint endpoint) throws IOException {
            this.connection=connection;buffer=java.nio.ByteBuffer.allocateDirect(endpoint.getMaxPacketSize());
            if(!request.initialize(connection,endpoint)){request.close();throw new IOException("Cannot initialize queued USB read");}
        }
        int read(byte[] destination,int timeout) throws IOException {
            if(!queued){buffer.clear();if(!request.queue(buffer))throw new IOException("Cannot queue USB read");queued=true;}
            final UsbRequest done;
            try {done=connection.requestWait(timeout);}catch(java.util.concurrent.TimeoutException e){return 0;}
            if(done!=request)throw new IOException("USB read completion missing or unexpected");
            queued=false;int n=buffer.position();
            if(n<0 || n>destination.length)throw new IOException("USB read length invalid");
            buffer.flip();buffer.get(destination,0,n);return n;
        }
        void stop() throws IOException {
            if(!queued)return;
            request.cancel();
            try {if(connection.requestWait(200)!=request)throw new IOException("USB cancel completion missing");queued=false;}
            catch(java.util.concurrent.TimeoutException e){throw new IOException("USB cancel completion timed out",e);}
        }
        void release(){request.close();}
    }
    void probe(UsbDevice device,long sessionId) {
        UsbDeviceConnection connection=null; UsbInterface data=null,control=null;
        UsbEndpoint rx=null,tx=null; QueuedReader usbReader=null; ReceivePump receivePump=null;
        BufferedWriter wireLog=null;
        boolean dataClaimed=false,controlClaimed=false,configured=false;
        int diagnosticWrites=0;boolean vinSent=false,flowSent=false;
        final NanoStartupGate startup=new NanoStartupGate(rebootNano?NanoStartupGate.Mode.REBOOT:skipInit?NanoStartupGate.Mode.CHANNEL_CONTROL:(initHandshake || nativeFirmware() || receiveTest || channelTest)?NanoStartupGate.Mode.INITIALIZE:NanoStartupGate.Mode.IDENTITY_ONLY);
        final ConnectionAttempt progress=connectionAttempt;
        PrintWriter out=null; boolean requestedQuit=false; boolean cleaned=true; boolean forcedTermination=false;
        boolean resultPublished=false;boolean resultSucceeded=false;
        String failureReason=null;
        java.lang.Process child=null;
        Thread outputReader=null;
        FirmwareGate.Lease firmwareLease=null;
        File resultFile=new File(getFilesDir(),"nano-"+java.util.UUID.randomUUID().toString()+".json");
        try {
            firmwareLease=FirmwareGate.use();
            if(nativeFirmware()){String missing=new FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty())throw new IOException("Firmware setup needed: "+missing);}
            wireLog=new BufferedWriter(new FileWriter(new File(resultFile.getPath()+".usb.log")));
            for(int i=0;i<device.getInterfaceCount();i++) {
                UsbInterface iface=device.getInterface(i);
                if(iface.getInterfaceClass()==2 && iface.getInterfaceSubclass()==2) control=iface;
                if(iface.getInterfaceClass()==10 && iface.getAlternateSetting()==0) {
                    data=iface;
                    for(int j=0;j<iface.getEndpointCount();j++) {
                        UsbEndpoint e=iface.getEndpoint(j);
                        if(e.getType()==UsbConstants.USB_ENDPOINT_XFER_BULK) { if(e.getDirection()==UsbConstants.USB_DIR_IN)rx=e;else tx=e; }
                    }
                }
            }
            if(data==null || control==null || rx==null || tx==null)throw new IOException("Expected CDC ACM interfaces unavailable");
            progress.stage(ConnectionAttempt.Stage.USB_OPEN);
            connection=manager.openDevice(device); if(connection==null)throw new IOException("USB open denied");
            progress.stage(ConnectionAttempt.Stage.INTERFACE_CLAIM);
            controlClaimed=connection.claimInterface(control,true);
            dataClaimed=connection.claimInterface(data,true);
            if(!controlClaimed || !dataClaimed)throw new IOException("USB interface claim failed");
            int receivePacketSize=rx.getMaxPacketSize();
            if(receivePacketSize<1 || receivePacketSize>1024)throw new IOException("Invalid bulk IN packet size");
            log(String.format("USB_OPEN data=%d OUT=%02X IN=%02X rx_packet=%d",data.getId(),tx.getAddress(),rx.getAddress(),receivePacketSize));
            // Exact 921600-baud open sequence from the matching Windows USBPcap.
            // Do not apply this CH343 profile to Chipsoft or another USB device.
            configured=true;
            control(connection,0xa1,0xc39c,0xf307);
            control(connection,0xa4,0xff,0);
            control(connection,0xa4,0xdf,0);
            control(connection,0xa4,0x9f,0);
            // A cancelled serial session may leave a partial frame and its stop
            // replies buffered. Drain ONLY before this session's first command;
            // the Rust decoder remains strict once fresh traffic starts.
            usbReader=new QueuedReader(connection,rx);
            byte[] stale=new byte[receivePacketSize];int discarded=0;boolean quiet=false;
            long drainUntil=SystemClock.elapsedRealtime()+1500;
            while(!cancelled && SystemClock.elapsedRealtime()<drainUntil) {
                int n=usbReader.read(stale,200);
                if(n<=0){quiet=true;break;}
                discarded+=n;
                wireLog.write(SystemClock.elapsedRealtime()+" STARTUP_DISCARD "+hex(stale,n)+"\n");
                if(discarded>65536)throw new IOException("Previous adapter stream exceeded startup drain limit");
            }
            log("USB_STARTUP_DRAIN discarded="+discarded+" quiet="+quiet);
            if(!quiet)throw new IOException("Adapter stream did not settle before new session");
            if(cancelled)throw new IOException("Cancelled");
            final QueuedReader queuedReader=usbReader;
            receivePump=new ReceivePump(new ReceivePump.Source(){
                public int read(byte[] buffer) throws IOException{return queuedReader.read(buffer,100);}
                public void stop() throws IOException{queuedReader.stop();}
            },receivePacketSize,4096);
            progress.stage(ConnectionAttempt.Stage.TRANSPORT_START);
            server=new ServerSocket();server.bind(new InetSocketAddress(InetAddress.getByName("127.0.0.1"),35673),1);server.setSoTimeout(15000);
            log("USB ready; starting Rust "+(nativeFirmware()?"original firmware":receiveTest?"channel receive test": "adapter test")+" on this phone.");
            String executable=getApplicationInfo().nativeLibraryDir+"/"+NanoProfile.PROFILE.probeExecutable;
            ProcessBuilder builder=(hsVin || securityVinProbe)?new ProcessBuilder(executable,token,resultFile.getAbsolutePath(),"--hs-vin-check"):receiveTest?new ProcessBuilder(executable,token,resultFile.getAbsolutePath(),"--receive-test"):channelTest?new ProcessBuilder(executable,token,resultFile.getAbsolutePath(),channelTestSw?"--channel-test-sw":"--channel-test"):new ProcessBuilder(executable,token,resultFile.getAbsolutePath());
            if(dlcVoltage && !nativeFirmware())builder=new ProcessBuilder(executable,token,resultFile.getAbsolutePath(),"--voltage-check");
            if(rebootNano)builder=new ProcessBuilder(executable,token,resultFile.getAbsolutePath(),"--reboot");
            else if(initHandshake)builder=new ProcessBuilder(executable,token,resultFile.getAbsolutePath(),"--init-handshake");
            if(skipInit)builder.command().add("--skip-init");
            if(nativeFirmware()){
                nativeDirectory=new File(getFilesDir(),"native-"+java.util.UUID.randomUUID().toString());
                if(!nativeDirectory.mkdir())throw new IOException("Cannot create native session directory");
                progress.bind(nativeDirectory,nativeFullNative?"full_native":nativeSeeds?"security_seeds":nativeResetCounters?"reset_engine_counters":"read_only","original_menus");health.begin(nativeDirectory);
                nativeKeyPump=new InteractiveKeyPump(new File(nativeDirectory,"native-key.txt"),e->log("Key input stopped: "+e));
                nativeLcdPump.setDirectory(nativeDirectory);
                File firmware=new File(getFilesDir(),"firmware");
                SecuritySessionData.recordBaseline(nativeDirectory,new File(firmware,"card.bin"));
                File sessionCard=nativeSecurityCollect?SecurityCollectionCard.prepare(new File(firmware,"card.bin"),nativeDirectory):new File(firmware,"card.bin");
                if(nativeSecurityCollect)SecurityCollectionMode.record(this,nativeDirectory);
                for(String name:new String[]{"eprom.bin","opsys.dwn","card.bin","candi.bin"})if(!new File(firmware,name).isFile())throw new IOException("Missing original firmware: "+name);
                builder=new ProcessBuilder(getApplicationInfo().nativeLibraryDir+"/libtech2_emu.so",
                    "--test-harness","--harness-target",(nativeFullNative || nativeSecurityCollect || nativeKeyStatus || nativeDtc&&!nativeClearDtc)?"native-manual":nativeDtc?"dtc-link-1367":"security-link-1367","--candi-native-link",
                    "--candi-nano-usb-token",token,"--candi-firmware",new File(firmware,"candi.bin").getPath(),
                    "--boot",new File(firmware,"eprom.bin").getPath(),"--opsys",new File(firmware,"opsys.dwn").getPath(),
                    "--max-insns","50000000000","--output-dir",nativeDirectory.getPath(),sessionCard.getPath());
                if(nativeClearDtc)builder.command().add("--candi-nano-clear-dtc");
                if(nativeResetCounters)builder.command().add("--candi-nano-reset-engine-counters");
                if(nativeKeyStatus)builder.command().add("--candi-nano-key-status");
                if(nativeFullNative)builder.command().add("--candi-nano-full-native");
                if(nativeSecurityCollect)builder.command().add("--candi-nano-seeds");
                if(nativeFullNative || nativeSecurityCollect){
                    if(securityVehicle==null || securityDevice==null || securityDevice.getDeviceId()!=device.getDeviceId() || !securityDevice.getDeviceName().equals(device.getDeviceName()))throw new IOException("Read a fresh vehicle VIN before security firmware");
                    VehicleSession.save(new File(nativeDirectory,VehicleSession.FILE),securityVehicle);
                }
                log("NATIVE_SESSION "+nativeDirectory.getName()+" commands=original-firmware");
            }
            if(nativeFirmware())progress.stage(ConnectionAttempt.Stage.FIRMWARE_START);
            if(nativeFirmware())builder.environment().put("OPENSAAB_PERFORMANCE_DIR",nativeDirectory.getAbsolutePath());
            DemandStartup.configure(this,builder);
            child=builder.redirectErrorStream(true).start();mcpProcess=child;
            final java.lang.Process probeChild=child;
            final File processLog=new File(nativeFirmware()?nativeDirectory:getFilesDir(),nativeFirmware()?"native-process.log":resultFile.getName()+".process.log");
            outputReader=new Thread(()->{
                try(BufferedReader reader=new BufferedReader(new InputStreamReader(probeChild.getInputStream(),StandardCharsets.UTF_8));
                    BufferedWriter savedOutput=new BufferedWriter(new FileWriter(processLog))) {
                    String line;int count=0;long savedBytes=0;while((line=reader.readLine())!=null){
                        if(SupportReports.retainProcessLogLine(savedBytes,line)){savedOutput.write(line);savedOutput.newLine();savedOutput.flush();savedBytes+=line.length()+1;}
                        if(line.startsWith("ERROR:"))nativeFailure=line.substring(6).trim();
                        String rejected=NativeCommandGate.collectionRejectedExplanation(line,nativeDtc && !nativeClearDtc && !nativeResetCounters && !nativeSeeds && !nativeKeyStatus && !nativeFullNative);
                        if(rejected!=null){nativeFailure=rejected;readModeRejected=true;}
                        if(line.startsWith("NATIVE_USB_CLOSED cleanup_ok=true "))nativeProtocolCleanupConfirmed=true;
                        if(nativeFirmware()){
                        if(line.contains("NATIVE_USB") || line.startsWith("LCD:") || line.contains("HARNESS") || line.contains("ERROR") || line.startsWith("OUTCOME"))log("FIRMWARE "+line);
                    }else if(count++<256)log("PROBE "+line);}
                }catch(IOException e){log("Probe output closed: "+e.getMessage());}
            },"nano-probe-log");outputReader.setDaemon(true);outputReader.start();
            client=DemandStartup.accept(this,server,builder,child,()->cancelled);client.setSoTimeout(4000);client.setTcpNoDelay(true);
            InputStream in=client.getInputStream();out=new PrintWriter(new OutputStreamWriter(client.getOutputStream(),StandardCharsets.US_ASCII),true);
            if(!readLine(in,NanoStartupGate.MAX_COMMAND_CHARS).equals("HELLO "+token))throw new IOException("Session token mismatch");
            out.println(fullNativeActive()?"READY nano-full-native":nativeSecurityCollect && !securityVinProbe?"READY nano-seeds":nativeKeyStatus?"READY native-key-status":nativeResetCounters?"READY native-reset-engine-counters":nativeFirmware()?"READY native-firmware":"READY adapter-only");
            runOnUiThread(()->{if(activeSessionId==sessionId)summary.setText(nativeFirmware()?"Original firmware active · direct Nano USB":"USB test active");});
            long until=fullNativeActive()?Long.MAX_VALUE:SystemClock.elapsedRealtime()+(nativeFirmware()?330000:(initHandshake || receiveTest || channelTest)?120000:20000);long txCount=0;long rxBytes=0;
            // Keep a queued one-packet receive alive across requestWait timeouts.
            // No short polling wait may discard partially received stream data.
            byte[] buffer=new byte[receivePacketSize];
            NativeCommandGate.Stream nativeGate=new NativeCommandGate.Stream();
            while(!cancelled && (SystemClock.elapsedRealtime()<until || nativeStop.awaitingQuit() && !nativeStop.expired(SystemClock.elapsedRealtime()))) {
                String cmd=readLine(in,NanoStartupGate.MAX_COMMAND_CHARS);
                if(cmd.equals("QUIT")) { requestedQuit=true;nativeStop.quitReceived();break; }
                receivePump.checkHealthy();
                if(cmd.startsWith("TX ")) {
                    String wire=cmd.substring(3);
                    byte[] bytes=unhex(wire,NanoStartupGate.MAX_TX_HEX_CHARS);
                    boolean stopping=nativeStop.pending();
                    boolean stopCleanup=stopping && nativeStop.cleanupAllowed(wire,SystemClock.elapsedRealtime());
                    if(stopping && !stopCleanup){
                        // Consume the queued line without a USB write or fake TXOK.
                        // SocketUsb returns an error, then its worker can still issue cleanup.
                        if(!nativeStop.rejectQueued(SystemClock.elapsedRealtime()))throw new IOException("Native Stop rejection/deadline budget exceeded");
                        log("NATIVE_STOP_REJECTED queued_request=true usb_write=false");
                        out.println("ERROR stopped");if(out.checkError())throw new IOException("Stop controller output closed");continue;
                    }
                    boolean vinRequest=(hsVin || securityVinProbe) && startup.channelReady() && wire.equals("BB8001000000000000000C000007E0021A90000000000020BB");
                    boolean vinFlow=(hsVin || securityVinProbe) && startup.channelReady() && wire.equals("BB8002000000000000000C000007E03000000000000000A5BB");
                    if((vinRequest && vinSent) || (vinFlow && (!vinSent || flowSent)))throw new IOException("VIN command order/count rejected");
                    boolean nativeTx=!stopping && nativeFirmware() && startup.channelReady() && (nativeFullNative?NativeCommandGate.fullNative(bytes):nativeGate.allowed(bytes,nativeSeeds,nativeClearDtc,nativeKeyStatus,nativeResetCounters,SystemClock.elapsedRealtime()));
                    boolean permitted=nativeTx || vinRequest || vinFlow || startup.allowed(bytes) || (startup.channelReady() && channelTest && (channelTestSw ? (wire.equals("BB800040010000810143BB") || wire.equals("BB80004101C2BB")) : (wire.equals("BB800040000000810142BB") || wire.equals("BB80004100C1BB")))) || (startup.channelReady() && receiveTest && receiveControlAllowed(wire));
                    if(stopping)permitted=stopCleanup;
                    permitted |= !stopping && dlcVoltage && !nativeFirmware() && wire.equals("BB800086001016BB");
                    if((++txCount>(nativeFirmware()?5000:receiveTest?45:channelTest?9:initHandshake?7:rebootNano?3:dlcVoltage?3:2) && !fullNativeActive()) || !permitted)throw new IOException("Diagnostic-TX exclusion gate rejected command");
                    if(wire.equals("BB80008C000CBB"))progress.stage(ConnectionAttempt.Stage.IDENTIFICATION);
                    else if(wire.startsWith("BB800040"))progress.stage(ConnectionAttempt.Stage.CHANNEL_OPEN);
                    else if(vinRequest)progress.stage(ConnectionAttempt.Stage.VIN_REQUEST);
                    int n=connection.bulkTransfer(tx,bytes,bytes.length,500);
                    log("USB_TX "+wire+" transferred="+n);
                    wireLog.write(SystemClock.elapsedRealtime()+" TX "+wire+" transferred="+n+"\n");
                    if(n!=bytes.length)throw new IOException("Incomplete USB write");
                    if(nativeTx)diagnosticWrites++;if(vinRequest){vinSent=true;diagnosticWrites++;}if(vinFlow){flowSent=true;diagnosticWrites++;}
                    out.println("TXOK");
                } else if(cmd.equals("READ")) {
                    int n=receivePump.read(buffer,nativeFirmware()?5:200);
                    if(n>0) { rxBytes+=n;if(rxBytes>(nativeFirmware()?64*1024*1024:receiveTest?4*1024*1024:16384) && !fullNativeActive())throw new IOException("USB receive limit exceeded");String wire=hex(buffer,n);wireLog.write(SystemClock.elapsedRealtime()+" RX "+wire+"\n");startup.received(buffer,n);if(!receiveTest)log("USB_RX "+wire);out.println("RX "+wire); }
                    else {wireLog.write(SystemClock.elapsedRealtime()+" RX_WAIT_EMPTY result="+n+"\n");out.println("EMPTY");} // Absolute Rust deadlines still bound silence/errors.
                } else throw new IOException("Unsupported command");
                if(out.checkError())throw new IOException("Controller output closed");
            }
            if(!requestedQuit)throw new IOException("Probe cancelled or expired");
        } catch(Exception e) { if(cancelled || nativeStop.pending())progress.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.USER_STOP);else {progress.failure(e);showConnectionReport();}SupportReports.recordError(this,e,false,nativeDirectory);failureReason=e.getMessage();log("ERROR "+failureReason);if(out!=null)out.println("ERROR transport"); }
        finally {
            final boolean stoppedByUser=cancelled || nativeStop.pending();
            if(connection!=null) {
                if(receivePump!=null){
                    try {receivePump.stop();}catch(Exception e){cleaned=false;log("CLEAN receive worker: "+e.getMessage());}
                    log("USB_RECEIVE_PUMP "+receivePump.stats());
                } else if(usbReader!=null)try {usbReader.stop();}catch(Exception e){cleaned=false;log("CLEAN queued read: "+e.getMessage());}
                if(receiveTest && startup.channelReady() && !requestedQuit && tx!=null) {
                    log("Abnormal end: attempting channel stop/close; replies unverified.");
                    for(String wire:EMERGENCY_CLOSE)try {byte[] b=unhex(wire);connection.bulkTransfer(tx,b,b.length,200);}catch(Exception ignored){}
                }
                if(configured)try { control(connection,0xa4,0xff,0); } catch(Exception e) { cleaned=false;log("CLEAN control failed: "+e.getMessage()); }
                if(dataClaimed)cleaned &= connection.releaseInterface(data);
                if(controlClaimed)cleaned &= connection.releaseInterface(control);
                connection.close();
                if(usbReader!=null)usbReader.release();
            }
            if(wireLog!=null)try {wireLog.close();}catch(IOException e){cleaned=false;log("Capture flush failed");}
            log("USB_CLOSED cleanup_ok="+cleaned+" diagnostic_usb_writes="+diagnosticWrites);
            if(requestedQuit && out!=null)out.println(cleaned?"CLOSED":"ERROR cleanup");
            if(child!=null) {
                try {
                    NativeProcessExit.Result exited=NativeProcessExit.await(child,2000);
                    if(exited.forced) { forcedTermination=true;if(!stoppedByUser)progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.TIMEOUT);log("PROBE_EXIT timeout"); }
                    else { if(outputReader!=null)outputReader.join(1000);log("PROBE_EXIT "+child.exitValue()); if(!nativeFirmware() && resultFile.isFile())log("RESULT_FILE "+resultFile.getName());if(child.exitValue()!=0 && !stoppedByUser){progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.PROTOCOL_OR_PROCESS_ERROR);showConnectionReport();} if(nativeFirmware()){
                        final String message=NativeSessionEnd.message("Nano",child.exitValue(),false,stoppedByUser,cleaned?"succeeded":"failed",NativeCleanupEvidence.protocol(nativeDirectory));
                        resultPublished=true;resultSucceeded=!stoppedByUser && child.exitValue()==0;
                        runOnUiThread(()->{if(activeSessionId==sessionId){summary.setText(message);setResult(RESULT_CANCELED,new Intent().putExtra("summary",message));}});
                    } else if(child.exitValue()==0 && cleaned && requestedQuit){
                        try {
                            org.json.JSONObject result=new org.json.JSONObject(new String(java.nio.file.Files.readAllBytes(resultFile.toPath()),StandardCharsets.UTF_8));
                            if(!result.has("error") || !result.isNull("error") || !result.getBoolean("channel_cleanup") || !result.getBoolean("usb_closed") || result.getJSONArray("cleanup_errors").length()!=0)throw new IOException("Probe result did not confirm cleanup/success");
                            org.json.JSONArray counts=result.getJSONArray("channel_rx");
                            String state=result.getString("status");
                            String expected=rebootNano?"adapter_software_reboot_acknowledged":initHandshake?"init_handshake_completed":(hsVin || securityVinProbe)?"hs_vin_received":receiveTest?"channels_receive_test_completed":channelTest?"channel_open_close_verified":dlcVoltage?"dlc_voltage_read":"identified";
                            if(!state.equals(expected) && !((hsVin || securityVinProbe) && state.equals("hs_vin_no_reply")))throw new IOException("Unexpected probe result for selected mode");
                            if(rebootNano && !NanoRebootHandoff.verified(state,result.getString("origin"),result.getBoolean("reboot_acknowledged"),startup.rebootAcknowledged(),result.getLong("vehicle_tx"),result.getBoolean("usb_closed"),result.getBoolean("channel_cleanup"),result.getJSONArray("cleanup_errors").length(),diagnosticWrites))throw new IOException("Reboot acknowledgement not observed");
                            if(initHandshake && (!startup.handshakeComplete() || result.getLong("vehicle_tx")!=0))throw new IOException("Initialization acknowledgement not observed");
                            if((hsVin || securityVinProbe) && state.equals("hs_vin_received") && (result.getLong("vehicle_tx")!=2 || !result.getString("vin").matches("[A-HJ-NPR-Z0-9]{17}")))throw new IOException("Invalid VIN result");
                            if(securityVinProbe && state.equals("hs_vin_received")){
                                if(!NativeCommandGate.securityVinReady(state,result.getString("origin"),result.getString("request_origin"),result.getBoolean("startup_complete"),result.getBoolean("query_record_verified"),result.getBoolean("installed_record_verified"),result.getLong("fresh_dh_exchanges"),result.getLong("vehicle_tx"),result.getBoolean("usb_closed"),result.getBoolean("channel_cleanup"),result.getJSONArray("cleanup_errors").length()==0))throw new IOException("Unverified Nano security VIN discovery");
                                String vin=result.getString("vin");
                                securityVehicle=new VehicleIdentity(vin,java.time.Instant.now().toString(),"Nano P-bus ECU VIN reply",VehicleIdentity.year(vin),"not_requested","","","","","");
                                securityDevice=device;
                                log("NANO_SECURITY_VIN verified=true origin=bounded-host-VIN-probe USB_closed=true");
                            }
                            String outcome;
                            switch(state){
                                case "hs_vin_no_reply":progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.VIN_UNAVAILABLE);showConnectionReport();outcome="No ECU VIN reply";break;
                                case "hs_vin_received":outcome="VIN received";break;
                                case "init_handshake_completed":outcome="Nano initialized";break;
                                case "adapter_software_reboot_acknowledged":outcome="Nano software reboot acknowledged";break;
                                case "identified":outcome="Nano identified";break;
                                case "dlc_voltage_read":outcome="OBD pin 16: "+result.getLong("dlc_voltage_mv")+" mV";break;
                                case "channel_open_close_verified":outcome="Channel open/close verified";break;
                                case "channels_receive_test_completed":outcome="Receive test completed";break;
                                default:throw new IOException("Unknown probe result status");
                            }
                            String message=rebootNano?"Nano software reboot acknowledged · USB closed. Back returns to Ready; Connect and start to reconnect.":outcome+" · P-bus "+counts.getLong(0)+" / I-bus "+counts.getLong(1)+" frames · USB closed";
                            final int code=state.equals("hs_vin_no_reply")?RESULT_CANCELED:RESULT_OK;
                            resultSucceeded=code==RESULT_OK;
                            resultPublished=true;
                            runOnUiThread(()->{if(activeSessionId==sessionId){summary.setText(message);setResult(code,new Intent().putExtra("summary",message));}});
                        }catch(Exception e){failureReason=e.getMessage();log("Summary unavailable: "+e.getMessage());}
                    } }
                }catch(InterruptedException e){NativeProcessExit.Result exited=NativeProcessExit.await(child,0);forcedTermination|=exited.forced;Thread.currentThread().interrupt();}
            }
            if(!resultPublished) {
                final String message=stoppedByUser?(rebootNano?"Reboot cancelled · Back returns to Ready":"Stopped — choose a test or Back"):(rebootNano?"Reboot not confirmed: ":"Unable to start: ")+(failureReason==null?"see console":failureReason)+" — retry or Back";
                runOnUiThread(()->{if(activeSessionId==sessionId){summary.setText(message);setResult(RESULT_CANCELED,new Intent().putExtra("summary",message));}});
            }
            Integer childCode=null;try{if(child!=null)childCode=child.exitValue();}catch(IllegalThreadStateException stillRunning){}
            String finalProtocol=nativeFirmware()?NativeCleanupEvidence.protocol(nativeDirectory):"unknown";
            if(!"unknown".equals(finalProtocol))nativeProtocolCleanupConfirmed="succeeded".equals(finalProtocol);
            progress.cleanup(childCode,forcedTermination,cleaned?"succeeded":"failed",nativeProtocolCleanupConfirmed?"succeeded":finalProtocol);
            if(!cleaned && !stoppedByUser){progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.CLEANUP_FAILED);showConnectionReport();}
            else if(requestedQuit && cleaned && resultPublished && resultSucceeded)progress.finish(ConnectionAttempt.Outcome.COMPLETED,ConnectionAttempt.Reason.NONE);
            else if(!stoppedByUser && !resultPublished){progress.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.PROTOCOL_OR_PROCESS_ERROR);showConnectionReport();}
            if(progress.failed())log(ConnectionAttempt.failureSummary(this,nativeDirectory==null?resultFile.getParentFile():nativeDirectory));
            if(nativeFirmware() && stoppedByUser)log("NATIVE_STOP_RESULT usb_closed="+cleaned+" protocol_cleanup_confirmed="+nativeProtocolCleanupConfirmed+" quit_received="+requestedQuit);
            final boolean startSecurityFirmware=securityVinProbe && requestedQuit && cleaned && resultSucceeded && !stoppedByUser && securityVehicle!=null;
            InteractiveKeyPump endedInput=nativeKeyPump;if(endedInput!=null)endedInput.close();nativeKeyPump=null;
            sessionCleanupConfirmed=cleaned&&!forcedTermination&&(!nativeFirmware()||nativeProtocolCleanupConfirmed);
            closeSockets();if(firmwareLease!=null)firmwareLease.close();usbSessionOwned.set(false);if(health!=null){if(forcedTermination||childCode!=null&&childCode!=0||!sessionCleanupConfirmed)health.endedFailure(nativeDirectory,"The native process or adapter cleanup did not finish successfully. Changes not verified; check saved session.","Session ended with a problem");else if(stoppedByUser||requestedQuit&&cleaned)health.expectedStop();else health.ended();}running.set(false);
            if(securityVinProbe){securityVinProbe=false;if(!startSecurityFirmware){securityVehicle=null;securityDevice=null;}}
            if(readModeRejected && !stoppedByUser){
                final PolicyBlock blocked=PolicyBlock.fromLine("Native USB command rejected by collection profile");
                log("POLICY_BLOCKED adapter=nano profile=Read vehicle_tx=false");
                runOnUiThread(()->{if(activeSessionId==sessionId && !isFinishing() && !isDestroyed()){summary.setText(blocked.status());FullSessionPrompt.offer(this,blocked,()->openSecuritySession(false));}});
            }
            if(startSecurityFirmware)runOnUiThread(()->{if(activeSessionId==sessionId && foreground && !isFinishing() && !isDestroyed())startSameSecurityAdapter();});
            runOnUiThread(()->{if(activeSessionId==sessionId)clearNativePresentation();});
            runOnUiThread(()->{if(retry!=null && !isFinishing())retry.setEnabled(true);});
        }
    }
    android.app.Dialog showActions(){
        SessionSheet.Menu menu=new SessionSheet.Menu(this);
        menu.add("AI control (MCP)",()->AndroidMcp.show(this));
        if(securityAccess!=null)menu.add(securityAccess.readyToProcess()?"Process existing data":"Get security access",securityAccess::requestAccess);
        return menu.add("Reboot Nano",this::requestReboot).add("Console",controls::showConsole).show("Actions");
    }
    void requestReboot(){
        if(SecurityAccessView.workflowBusy()){summary.setText("Finish security processing before rebooting Nano");return;}
        if(rebootHandoff.state()==NanoRebootHandoff.State.WAITING||rebootHandoff.state()==NanoRebootHandoff.State.READY)return;
        UsbDevice target=selected;
        if(target==null){java.util.List<UsbDevice> devices=NanoRebootAction.candidates(this);if(devices.size()==1)target=devices.get(0);}
        if(target==null){summary.setText("Connect and select Nano before rebooting");return;}
        final UsbDevice chosen=target;
        if(running.get()||usbSessionOwned.get()||permissionPending)new AlertDialog.Builder(this).setTitle("Reboot Nano")
            .setMessage("This ends the current diagnostic session, then reboots Nano. Reconnect with Connect and start afterward.")
            .setNegativeButton("Keep session",null).setPositiveButton("Stop and reboot",(d,w)->beginReboot(chosen)).show();
        else beginReboot(chosen);
    }
    void beginReboot(UsbDevice chosen){
        if(SecurityAccessView.workflowBusy()||!foreground)return;
        boolean needsCleanup=running.get()||usbSessionOwned.get();
        if(!rebootHandoff.begin(SystemClock.elapsedRealtime(),needsCleanup))return;
        pendingSecurityStart=null;cancel();summary.setText("Reboot Nano · waiting for session cleanup");
        log("NANO_REBOOT_REQUEST waiting_for_cleanup="+needsCleanup);
        final Runnable advance=new Runnable(){public void run(){
            NanoRebootHandoff.State state=rebootHandoff.poll(SystemClock.elapsedRealtime(),running.get()||usbSessionOwned.get()||permissionPending,sessionCleanupConfirmed,foreground&&!isFinishing()&&!isDestroyed());
            if(state==NanoRebootHandoff.State.WAITING){lcdHandler.postDelayed(this,100);return;}
            if(state==NanoRebootHandoff.State.FAILED){summary.setText("Reboot not started · session cleanup was not confirmed. Reconnect Nano, then retry.");log("NANO_REBOOT_BLOCKED cleanup_unconfirmed=true");return;}
            if(state!=NanoRebootHandoff.State.READY)return;
            UsbDevice current=manager.getDeviceList().get(chosen.getDeviceName());
            if(current==null||current.getDeviceId()!=chosen.getDeviceId()||!NanoRebootAction.idle()){
                rebootHandoff.cancel();summary.setText("Reboot not started · selected Nano changed or another session is active");return;
            }
            if(rebootHandoff.consume()){
                startActivity(NanoRebootAction.intent(NanoProbeActivity.this,current).addFlags(Intent.FLAG_ACTIVITY_FORWARD_RESULT));finish();
            }
        }};lcdHandler.post(advance);
    }
    boolean sendSecurityMenuKey(int code){
        if(!foreground || !running.get() || cancelled || nativeStop.pending() || nativeDirectory==null || new File(nativeDirectory,"native-key.txt").exists())return false;
        nativeKey(String.format(java.util.Locale.ROOT,"0x%02x",code));return true;
    }
    void stopForSecurityHandoff(){
        // Processing saved seeds also enters here after a clean operator Stop.
        // An idle handoff must not turn that completed Stop into cancellation.
        if(!running.get()&&!usbSessionOwned.get()&&!permissionPending)return;
        if(health!=null)health.expectedStop();cancel();
    }
    void openSecuritySession(boolean collect){
        if(running.get() || isFinishing() || isDestroyed())return;
        Intent next=new Intent(this,NanoProbeActivity.class).putExtra(collect?"nano_security_collect":"nano_full_native",true).putExtra("auto_start",true).putExtra(SecurityCollectionMode.EXTRA,SecurityCollectionMode.from(getIntent())).putExtra(SecurityCollectionMode.PLATFORM_EXTRA,getIntent().getStringExtra(SecurityCollectionMode.PLATFORM_EXTRA));
        if(selected!=null)next.putExtra("usb_device_name",selected.getDeviceName());
        startActivity(next);finish();
    }
    void startSameSecurityAdapter(){
        if(running.get() || usbSessionOwned.get() || !foreground || cancelled || !sessionCleanupConfirmed || isFinishing() || isDestroyed() || securityVehicle==null || securityDevice==null)return;
        UsbDevice attached=manager.getDeviceList().get(securityDevice.getDeviceName());
        if(attached==null || attached.getDeviceId()!=securityDevice.getDeviceId() || !manager.hasPermission(attached)){
            securityVehicle=null;securityDevice=null;summary.setText("Security access ready · reconnect the adapter to continue");return;
        }
        if(FirmwareGate.sessionActive() || SecurityAccessView.workflowBusy()){summary.setText("Security access ready · firmware busy; retry Return to firmware");return;}
        selected=attached;discover();
    }
    void resumeSecurityFirmware(){
        if(running.get() || cancelled || isFinishing() || isDestroyed())return;
        if(usbSessionOwned.get() || permissionPending){
            summary.setText("Security data imported · Nano USB is still closing. Tap Continue firmware again in a moment.");return;
        }
        if(!sessionCleanupConfirmed){
            // Not a dead end, but never automatic: this screen keeps its mode until the operator chooses a fresh connection.
            summary.setText("Security data imported · Nano cleanup not confirmed in this screen");
            if(foreground&&!isFinishing()&&!isDestroyed())new AlertDialog.Builder(this).setTitle("Connect again in Full session?")
                .setMessage("This screen has no confirmed Nano session to continue. Connect again to return to the original firmware in a FULL SESSION, where vehicle changes are possible. The VIN is identified again.\n\nIf the Nano stays busy, unplug it and plug it back in first.")
                .setNegativeButton("Not now",null).setPositiveButton("Connect in Full session",(d,w)->{SessionModeControl.remember(this,SessionModes.Mode.FULL);openSecuritySession(false);}).show();
            return;
        }
        nativeSecurityCollect=false;nativeSeeds=false;nativeFullNative=true;
        title.setText(AppBuildProfile.programName(this));
        purpose.setText("Original Tech2 + CANdi firmware · direct USB\nFull native control · original Add / Remove menus");
        if(modeBadge==null){modeBadge=SessionModeBadge.create(this,activeMode(),false);((android.view.ViewGroup)purpose.getParent()).addView(modeBadge,((android.view.ViewGroup)purpose.getParent()).indexOfChild(purpose)+1);}else refreshModeBadge();
        getIntent().removeExtra("nano_security_collect");getIntent().removeExtra("native_seed");getIntent().putExtra("nano_full_native",true);
        if(securityAccess!=null)securityAccess.returnedToFirmware();
        Runnable restart=this::startSameSecurityAdapter;
        if(foreground)restart.run();else pendingSecurityStart=restart;
    }
    boolean sendMenuKey(int code){if(!foreground||!running.get()||cancelled||nativeStop.pending()||nativeDirectory==null||nativeKeyPump==null)return false;if(health!=null)health.input();return nativeKeyPump.offer(String.format(java.util.Locale.ROOT,"0x%02x",code));}
    void nativeKey(String code){
        if(!nativeFirmware() || !running.get() || cancelled || nativeStop.pending() || nativeDirectory==null){log("No active native diagnostic session.");return;}
        if(health!=null)health.input();
        InteractiveKeyPump pump=nativeKeyPump;
        if(pump==null || !pump.offer(code)){log("Key input queue is full or closing.");return;}
        log("NATIVE_KEY "+code+" queued via original keypad");
    }
    DtcReportView dtcReport;
    void clearNativePresentation(){if(nativeLcdPump!=null)nativeLcdPump.clear();else if(nativeLcd!=null)nativeLcd.setImageDrawable(null);}
    void updateNativeLcd() {
        if(securityAccess!=null)securityAccess.refresh();
        if(nativeStop.pending()){if(ignitionStatus!=null)ignitionStatus.setText("Ignition: Unknown · stopping");return;}
        if(ignitionStatus!=null)ignitionStatus.refresh(nativeDirectory,running.get() && !cancelled);
        // The shared event-driven pump reads and decodes actual VRAM off the UI thread.
    }
    // Captured normal-mode controls only; opcode 00 CAN transmission is excluded.
    static boolean receiveControlAllowed(String wire) {
        switch(wire) {
            case "BB800040000000810142BB":
            case "BB800040010000810143BB":
            case "BB80004100C1BB":
            case "BB80004101C2BB":
            case "BB8000420000C2BB":
            case "BB8000420100C3BB":
            case "BB80004300C3BB":
            case "BB80004301C4BB":
            case "BB8000450000030007A12000020000000001000000000093BB":
            case "BB8000450000030007A12090BB":
            case "BB8000450100030000823500020100000001000000000286BB":
            case "BB8000450100030000823580BB":
            case "BB8000470001C8BB":
            case "BB8000470101C9BB":
            case "BB800048000100010100CBBB":
            case "BB8000480001000101040000000000000000CFBB":
            case "BB800048010100010100CCBB":
            case "BB8000480101000101040000000000000000D0BB":
                return true;
            default:return false;
        }
    }
    static final String[] EMERGENCY_CLOSE={"BB80004301C4BB","BB80004101C2BB","BB80004300C3BB","BB80004100C1BB"};
    void closeSockets() { try { if(client!=null)client.close(); }catch(IOException ignored){} try { if(server!=null)server.close(); }catch(IOException ignored){} }
    void cancel() {
        if(health!=null)health.expectedStop();
        InteractiveKeyPump input=nativeKeyPump;if(input!=null)input.cancel();
        if(connectionAttempt!=null)connectionAttempt.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.USER_STOP);
        requestGate.cancel();permissionPending=false;
        if(nativeFirmware() && running.get() && nativeDirectory!=null){
            if(nativeStop.pending())return;
            try{
                // Existing shared engine Stop command; original menus and adapter checks unchanged.
                File pending=new File(nativeDirectory,"native-stop.tmp"),dest=new File(nativeDirectory,"native-stop");
                java.nio.file.Files.write(pending.toPath(),new byte[0]);
                java.nio.file.Files.move(pending.toPath(),dest.toPath(),java.nio.file.StandardCopyOption.ATOMIC_MOVE);
                nativeStop.begin(SystemClock.elapsedRealtime());
                final long stopSession=activeSessionId;
                summary.setText("Stopping original firmware · waiting for Nano cleanup");clearNativePresentation();
                log("NATIVE_STOP_REQUEST grace_ms="+NanoNativeStopGate.GRACE_MS+" bridge_kept_open=true");
                Thread guard=new Thread(()->{
                    try{Thread.sleep(NanoNativeStopGate.GRACE_MS);}catch(InterruptedException e){Thread.currentThread().interrupt();}
                    if(activeSessionId==stopSession && running.get() && nativeStop.awaitingQuit() && nativeStop.expired(SystemClock.elapsedRealtime())){
                        log("NATIVE_STOP_TIMEOUT protocol_cleanup_unconfirmed=true");cancelled=true;closeSockets();
                    }
                },"nano-native-stop-deadline");guard.setDaemon(true);guard.start();return;
            }catch(IOException e){log("Native Stop command failed: "+e.getMessage());}
        }
        cancelled=true;closeSockets();log("Stop requested.");
    }
    void leaveScreen() { cancel();finish(); }
    @android.annotation.SuppressLint("GestureBackNavigation") // API 33+ uses BackNavigation; this handles older Android.
    @Override public void onBackPressed() { leaveScreen(); }
    @Override protected void onStart() {AndroidMcp.bind(this,mcpOwner); super.onStart();foreground=true;if(pendingSecurityStart!=null){Runnable next=pendingSecurityStart;pendingSecurityStart=null;next.run();} }
    @Override protected void onStop() {AndroidMcp.unbind(this); foreground=false;rebootHandoff.cancel();super.onStop();if(running.get() || permissionPending)cancel();else requestGate.cancel(); }
    @Override protected void onDestroy() { if(dtcReport!=null)dtcReport.close();if(securityAccess!=null)securityAccess.close();pendingSecurityStart=null;cancel();if(nativeLcdPump!=null)nativeLcdPump.close();lcdHandler.removeCallbacksAndMessages(null);unregisterReceiver(receiver);super.onDestroy(); }
}
