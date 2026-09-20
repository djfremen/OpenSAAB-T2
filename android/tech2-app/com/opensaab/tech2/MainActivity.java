// SPDX-License-Identifier: MPL-2.0
package com.opensaab.tech2;

import android.app.Activity;
import android.os.Bundle;
import android.os.Handler;
import android.graphics.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Displays original guest VRAM. No synthetic menu or vehicle transport. */
public final class MainActivity extends Activity {
    private final Handler ui = new Handler();
    private volatile com.opensaab.usb.InteractiveKeyPump keyPump;
    private com.opensaab.usb.NativeLcdPump lcdPump;
    private com.opensaab.usb.EmulatorHealthMonitor health;
    private boolean consoleDirty;
    private final Runnable consoleRefresh=new Runnable(){public void run(){
        if(consoleDirty && console.isShown()){consoleDirty=false;console.setText(logs.toString());consoleScroll.post(()->consoleScroll.fullScroll(View.FOCUS_DOWN));}
        if(foreground)ui.postDelayed(this,150);
    }};
    private final AtomicBoolean stopping = new AtomicBoolean();
    private volatile java.lang.Process process;
    private volatile boolean running;
    private boolean foreground;
    private com.opensaab.usb.StartupMeasurement startup;
    private long launchOrigin;
    private boolean autoStarted;
    private File session;
    private TextView status, console;
    private TextView vehicleSummary,connectionDate,authStatus,authDate;
    private String vehicleDetails="No saved vehicle connection";
    private final ExecutorService historyWorker=Executors.newSingleThreadExecutor();
    private final AtomicBoolean historyPending=new AtomicBoolean();
    private final Runnable historyRefresh=new Runnable(){public void run(){if(foreground){refreshVehicleHistory();ui.postDelayed(this,60000);}}};
    private ScrollView consoleScroll;
    private LcdView lcd;
    private Button start, offline;
    private com.opensaab.usb.SessionWorkspace workspace;
    private com.opensaab.usb.Tech2Controls controls;
    private LinearLayout details;
    private String compactVehicle="No vehicle connected", compactSecurity="";
    private final StringBuilder logs = new StringBuilder();
    private static final int[] DIGITS = {0x18,0x04,0x13,0x17,0x03,0x12,0x16,0x02,0x11,0x15};
    private int dp(int n) { return Math.round(n * getResources().getDisplayMetrics().density); }
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);launchOrigin=getIntent().getLongExtra("launch_origin_elapsed_ms",android.os.SystemClock.elapsedRealtime());com.opensaab.usb.BackNavigation.install(this,()->{if(running)key(0x01);else finish();});
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        details=new LinearLayout(this);details.setOrientation(LinearLayout.VERTICAL);
        vehicleSummary=label("Connect an adapter to identify your vehicle",14);details.addView(vehicleSummary);
        vehicleSummary.setOnClickListener(v->new android.app.AlertDialog.Builder(this).setTitle("Last vehicle").setMessage(vehicleDetails).setPositiveButton("Done",null).show());
        connectionDate=label("Last connection · Date unknown",13);details.addView(connectionDate);
        authStatus=label("auth_status: [N/A]",14);details.addView(authStatus);
        authDate=label("Timestamp unavailable",13);details.addView(authDate);
        status=label("Ready — connect an adapter or explore offline",13);details.addView(status);
        status.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int a,int c,int f){}public void onTextChanged(CharSequence t,int a,int b,int c){updateWorkspace();}public void afterTextChanged(android.text.Editable e){}});
        lcd=new LcdView();console=label("Console: waiting for firmware",11);console.setTypeface(Typeface.MONOSPACE);
        consoleScroll=new ScrollView(this);consoleScroll.addView(console);
        controls=new com.opensaab.usb.Tech2Controls(this,lcd,consoleScroll,code->{if(code==0x10)enqueue("enter");else key(code);});
        controls.setActions(this::showActions);
        health=new com.opensaab.usb.EmulatorHealthMonitor(this,()->stopSession("Preparing report"));
        lcdPump=new com.opensaab.usb.NativeLcdPump(frame->{health.frame();lcd.frame=frame;lcd.invalidate();controls.showFirstUseGuide();});
        workspace=new com.opensaab.usb.SessionWorkspace(this,"OpenSAAB T2",controls,this::showAppMenu,()->com.opensaab.usb.SessionSheet.show(this,"Vehicle and security details",details));
        start=new Button(this);start.setText("Connect and start");com.opensaab.usb.SessionStyle.button(start,true);start.setOnClickListener(v->selectAdapter("native_dtc",true));workspace.addLaunch(start);
        offline=new Button(this);offline.setText("Run without an adapter");offline.setTag("start-offline");com.opensaab.usb.SessionStyle.button(offline,false);offline.setOnClickListener(v->{if(idleTool())startSession();});workspace.addLaunch(offline);
        com.opensaab.usb.SessionStyle.stack(details);updateWorkspace();setContentView(workspace);
        if(state==null && (getIntent().hasCategory(android.content.Intent.CATEGORY_LAUNCHER)||getIntent().getExtras()==null) && android.content.Intent.ACTION_MAIN.equals(getIntent().getAction()) && !new com.opensaab.usb.FirmwareStore(getFilesDir()).missing().isEmpty()){
            ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.FirmwareActivity.class)));return;
        }
        // Public builds use visible user actions; legacy ADB auto-start hooks are development-only.
        if((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)==0)return;
        if(getIntent().getBooleanExtra("chipsoft_vin",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("vin_check",true)));
        else if(getIntent().getBooleanExtra("chipsoft_audible",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("audible",true)));
        else if(getIntent().getBooleanExtra("chipsoft_seeds",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("native_seeds",true)));
        else if(getIntent().getBooleanExtra("chipsoft_symbol_only",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("symbol_only",true)));
        else if(getIntent().getBooleanExtra("chipsoft_key_status",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("key_status",true)));
        else if(getIntent().getBooleanExtra("chipsoft_full_native",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("full_native",true)));
        else if(getIntent().getBooleanExtra("chipsoft_native",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("native_firmware",true)));
        else if(getIntent().getBooleanExtra("chipsoft_receive",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("receive_test",true)));
        else if(getIntent().getBooleanExtra("chipsoft_identity",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("auto_start",true)));
        else if(getIntent().getBooleanExtra("adapter_detector",false))ui.post(()->startActivity(new android.content.Intent(this,com.opensaab.usb.AdapterDetectorActivity.class)));
        else if(getIntent().hasExtra("nano_probe_mode")){
            // Bounded ADB harness inside the existing app, never a second APK.
            String mode=getIntent().getStringExtra("nano_probe_mode");
            if(Arrays.asList("identity","channel","channel-sw","receive","voltage").contains(mode)){
                android.content.Intent probe=new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra("auto_start",true);
                if(mode.equals("channel") || mode.equals("channel-sw"))probe.putExtra("channel_test",true);
                if(mode.equals("channel-sw"))probe.putExtra("channel_test_sw",true);
                if(mode.equals("receive"))probe.putExtra("receive_test",true);
                if(mode.equals("voltage"))probe.putExtra("dlc_voltage",true);
                ui.post(()->startActivityForResult(probe,27));
            }else status.setText("Unknown Nano probe mode — no session started");
        }
        else if(getIntent().getBooleanExtra("native_engine_data",false))ui.post(()->startActivityForResult(new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra("native_engine_data",true).putExtra("auto_start",true),27));
        else if(getIntent().getBooleanExtra("native_clear_dtc",false))ui.post(()->startActivityForResult(new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra("native_clear_dtc",true).putExtra("auto_start",true),27));
        else if(getIntent().getBooleanExtra("native_dtc",false))ui.post(()->startActivityForResult(new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra("native_dtc",true).putExtra("auto_start",true),27));
        else if(getIntent().getBooleanExtra("native_seed",false))ui.post(()->startActivityForResult(new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra("native_seed",true).putExtra("auto_start",true),27));
    }
    @Override protected void onActivityResult(int request,int result,android.content.Intent data) {
        super.onActivityResult(request,result,data);
        if(request==27 && data!=null){String message=data.getStringExtra("summary");if(message!=null){append(message);status.setText(message);}}
    }
    private TextView label(String text, int size) {
        TextView t=new TextView(this); t.setText(text); t.setTextSize(size); t.setTextColor(0xffdce9f2); return t;
    }
    private void updateWorkspace(){
        if(workspace==null)return;
        workspace.summary(running?status.getText().toString().replace("Emulation mode • Offline • No vehicle connection","Offline emulation · no vehicle connection"):compactVehicle+"\n"+(compactSecurity.isEmpty()?status.getText():compactSecurity));
        if(start!=null)start.setVisibility(running?View.GONE:View.VISIBLE);
        if(offline!=null)offline.setVisibility(running?View.GONE:View.VISIBLE);
    }
    public android.app.Dialog showActions(){
        return new com.opensaab.usb.SessionSheet.Menu(this)
            .add("Get security access",()->selectAdapter("native_seed",false))
            .add("Read engine codes — HS-CAN",()->selectAdapter("dtc_read",false))
            .add("Read DTC",()->selectAdapter("native_dtc",false))
            .add("Clear DTC",()->com.opensaab.usb.SessionSheet.confirmClear(this,()->selectAdapter("native_clear_dtc",false)))
            .add("Engine Data",()->selectAdapter("native_engine_data",false))
            .add("Saved DTC reports",()->com.opensaab.usb.DtcReportView.showSavedReports(this))
            .add(workspace.expanded()?"Show header":"Expand screen",workspace::toggleExpanded)
            .add("App menu",this::showAppMenu).show("Actions");
    }
    private void showAppMenu(){
        com.opensaab.usb.SessionSheet.Menu menu=new com.opensaab.usb.SessionSheet.Menu(this);
        if(running)menu.add("Stop emulation",()->stopSession("Stopped by operator"));
        if(com.opensaab.usb.SetupCleanup.available(this))menu.add("Remove installer",()->{if(idleTool())com.opensaab.usb.SetupCleanup.remove(this);});
        menu.add("Run without an adapter",()->{if(idleTool())startSession();});
        menu.add("Firmware selection",()->{if(idleTool())startActivity(new android.content.Intent(this,com.opensaab.usb.FirmwareActivity.class));})
            .add("Adapter & advanced tools",()->com.opensaab.usb.AdvancedFeatures.show(this,()->running))
            .add("Report issue",()->{if(idleTool())startActivity(new android.content.Intent(this,com.opensaab.usb.SupportReportActivity.class));})
            .add("Console",controls::showConsole)
            .add("Check for updates",()->{if(idleTool())com.opensaab.usb.AppUpdates.show(this);})
            .add("About / Support",()->{if(idleTool())com.opensaab.usb.ProjectSupport.show(this);})
            .add("Firmware controls help",controls::showHelp).show("App menu");
    }
    private boolean idleTool(){
        if(!running&&!com.opensaab.usb.FirmwareGate.busy()&&!com.opensaab.usb.SecurityAccessView.workflowBusy())return true;
        Toast.makeText(this,"Stop the current session before opening this tool",Toast.LENGTH_LONG).show();return false;
    }
    private void key(int code) { enqueue(String.format(Locale.ROOT,"0x%02x",code)); }
    private void refreshAdapterLabel(){
        if(start==null||running)return;
        android.hardware.usb.UsbManager usb=(android.hardware.usb.UsbManager)getSystemService(USB_SERVICE);
        List<android.hardware.usb.UsbDevice> found=new ArrayList<>();
        for(android.hardware.usb.UsbDevice d:usb.getDeviceList().values())if(com.opensaab.usb.AdapterCatalog.adapterCandidate(com.opensaab.usb.AdapterCatalog.identify(d.getVendorId(),d.getProductId())))found.add(d);
        String label="Connect and start";
        if(found.size()==1){com.opensaab.usb.AdapterProfile profile=com.opensaab.usb.AdapterCatalog.identify(found.get(0).getVendorId(),found.get(0).getProductId()).profile;
            if(profile!=null)label=profile.candidateLabel()+" · Connect and start";}
        start.setText(label);
    }
    private void selectAdapter(String mode,boolean allowEmulation) {
        if(com.opensaab.usb.FirmwareGate.busy()){status.setText("Finish firmware installation first");return;}
        String missing=new com.opensaab.usb.FirmwareStore(getFilesDir()).missing();
        if(!missing.isEmpty()){status.setText("Firmware setup needed: "+missing);startActivity(new android.content.Intent(this,com.opensaab.usb.FirmwareActivity.class));return;}
        if(com.opensaab.usb.SecurityAccessView.workflowBusy()){status.setText("Finish security processing before starting another session");return;}
        if(running){status.setText("Stop emulation before selecting an adapter");return;}
        android.hardware.usb.UsbManager usb=(android.hardware.usb.UsbManager)getSystemService(USB_SERVICE);
        List<android.hardware.usb.UsbDevice> devices=new ArrayList<>();
        List<String> labels=new ArrayList<>();
        List<android.hardware.usb.UsbDevice> attached=new ArrayList<>(usb.getDeviceList().values());
        attached.sort(Comparator.comparing(android.hardware.usb.UsbDevice::getDeviceName));
        for(android.hardware.usb.UsbDevice device:attached){
            com.opensaab.usb.AdapterCatalog.Match match=com.opensaab.usb.AdapterCatalog.identify(device.getVendorId(),device.getProductId());
            if(!com.opensaab.usb.AdapterCatalog.adapterCandidate(match))continue;
            devices.add(device);
            String label=match.profile!=null?match.profile.candidateLabel():match.label+" — not supported yet";
            labels.add(label+"\nUSB "+device.getDeviceName());
        }
        if(devices.size()==1&&com.opensaab.usb.AdapterCatalog.supported(com.opensaab.usb.AdapterCatalog.identify(devices.get(0).getVendorId(),devices.get(0).getProductId()))){
            connectAdapter(devices.get(0),mode,!allowEmulation);return;
        }
        android.app.AlertDialog.Builder picker=new android.app.AlertDialog.Builder(this).setTitle("Select adapter");
        if(devices.isEmpty()){
            new com.opensaab.usb.ConnectionAttempt(this,com.opensaab.usb.ConnectionAttempt.Adapter.SELECTION)
                .finish(com.opensaab.usb.ConnectionAttempt.Outcome.FAILED,com.opensaab.usb.ConnectionAttempt.Reason.NO_ADAPTER);
            picker.setMessage("No supported adapter detected. Connect a USB adapter, then tap Refresh. If this keeps happening, use App menu → Report issue.");
        }
        else picker.setItems(labels.toArray(new String[0]),(dialog,index)->connectAdapter(devices.get(index),mode,!allowEmulation));
        picker.setPositiveButton("Refresh",(dialog,which)->selectAdapter(mode,allowEmulation));
        picker.setNegativeButton("Cancel",null);
        if(allowEmulation)picker.setNeutralButton("Run in emulation mode",(dialog,which)->startSession());
        picker.show();
    }
    private void connectAdapter(android.hardware.usb.UsbDevice chosen,String mode,boolean shortcut){
        android.hardware.usb.UsbManager usb=(android.hardware.usb.UsbManager)getSystemService(USB_SERVICE);
        android.hardware.usb.UsbDevice current=usb.getDeviceList().get(chosen.getDeviceName());
        if(current==null || current.getDeviceId()!=chosen.getDeviceId()
            || current.getVendorId()!=chosen.getVendorId() || current.getProductId()!=chosen.getProductId()){
            status.setText("Selected adapter disconnected — select again");return;
        }
        com.opensaab.usb.AdapterCatalog.Match match=com.opensaab.usb.AdapterCatalog.identify(current.getVendorId(),current.getProductId());
        android.content.Intent launch;
        if(mode.equals("dtc_read")){
            if(match.backend()!=com.opensaab.usb.AdapterProfile.Backend.CHIPSOFT_PRO){android.widget.Toast.makeText(this,"Direct HS-CAN engine-code reading currently requires Chipsoft",android.widget.Toast.LENGTH_LONG).show();return;}
            launch=new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class).putExtra("dtc_read",true);
        }else if(match.backend()==com.opensaab.usb.AdapterProfile.Backend.VCX_NANO){
            launch=new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra(mode,true);
        }else if(match.backend()==com.opensaab.usb.AdapterProfile.Backend.CHIPSOFT_PRO){
            boolean restricted=getSharedPreferences("adapter_settings",MODE_PRIVATE).getBoolean("chipsoft_restricted",false);
            if(restricted && (mode.equals("native_seed") || mode.equals("native_clear_dtc"))){
                status.setText("Turn off Chipsoft restricted mode to collect security data or clear DTCs");
                android.widget.Toast.makeText(this,"Restricted mode is read-only. Turn it off to collect security data or clear DTCs.",android.widget.Toast.LENGTH_LONG).show();
                return;
            }
            launch=new android.content.Intent(this,com.opensaab.usb.ChipsoftUsbActivity.class)
                // Default to full control; persist an explicit read-only choice.
                // ADB automation profiles remain independent of this UI setting.
                .putExtra(restricted?"native_firmware":mode.equals("native_seed")?"native_seeds":"full_native",true);
            if(shortcut&&!mode.equals("native_seed"))launch.putExtra("menu_shortcut",mode);
        }else{status.setText("This adapter is detected, but its Android driver is not ready");return;}
        status.setText(match.profile.openingMessage());
        android.util.Log.i("OpenSaabAdapters","BACKEND_SELECTED backend="+match.backend()+" identity_verified=false probe="+match.profile.probeExecutable);
        startActivityForResult(launch.putExtra("usb_device_name",chosen.getDeviceName()).putExtra("auto_start",true),27);
    }
    private void enqueue(String command) {
        if (!running || stopping.get()) { status.setText("Tap Start to select an adapter or emulation mode"); return; }
        health.input();
        com.opensaab.usb.InteractiveKeyPump pump=keyPump;
        if(pump==null || !pump.offer(command)) status.setText("Key queue full or firmware starting — wait for the menu");
    }
    private void append(String line) {
        // Called on the UI thread; bound both the retained and visible console.
        logs.append(line.replaceAll("\\x1b\\[[0-9;]*m", "")).append('\n');
        if(logs.length()>12000) logs.delete(0,logs.length()-8000);
        consoleDirty=true;
        if(line.startsWith("SESSION:")) status.setText(line.substring(8).trim());
    }
    private void startSession() {
        if(running || !foreground || com.opensaab.usb.SecurityAccessView.workflowBusy()) return;
        if(com.opensaab.usb.FirmwareGate.busy()){status.setText("Finish firmware installation first");return;}
        String missing=new com.opensaab.usb.FirmwareStore(getFilesDir()).missing();
        if(!missing.isEmpty()){status.setText("Firmware setup needed: "+missing);startActivity(new android.content.Intent(this,com.opensaab.usb.FirmwareActivity.class));return;}
        File firmware=new File(getFilesDir(),"firmware");
        if(getFilesDir().getUsableSpace()<64L*1024*1024) { status.setText("Need 64 MB free for session logs"); return; }
        if(com.opensaab.usb.DemandStartup.enabled(this)){startup=new com.opensaab.usb.StartupMeasurement(this,autoStarted?android.os.SystemClock.elapsedRealtime():launchOrigin);autoStarted=true;}
        running=true; stopping.set(false); logs.setLength(0);consoleDirty=true;
        start.setEnabled(false); lcd.frame=null; lcd.invalidate();
        status.setText("Emulation mode • Offline • No vehicle connection");
        new Thread(()->runSession(firmware),"tech2-session").start();
    }
    private void runSession(File firmware) {
        String end="Emulator stopped";
        com.opensaab.usb.FirmwareGate.Lease firmwareLease=null;
        try {
            firmwareLease=com.opensaab.usb.FirmwareGate.use();
            String missing=new com.opensaab.usb.FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty())throw new IOException("Firmware setup needed: "+missing);
            File runs=new File(getFilesDir(),"sessions"); runs.mkdirs();
            File[] old=runs.listFiles();
            if(old!=null) { Arrays.sort(old,Comparator.comparingLong(File::lastModified));
                for(int i=0;i<old.length-2;i++) remove(old[i]); }
            session=new File(runs,UUID.randomUUID().toString());
            if(!session.mkdir()) throw new IOException("Cannot create session directory");
            List<String> args=new ArrayList<>(Arrays.asList(
                getApplicationInfo().nativeLibraryDir+"/libtech2_emu.so",
                "--interactive-headless","--research-harness","--candi-native-link",
                "--candi-firmware",new File(firmware,"candi.bin").toString(),
                "--boot",new File(firmware,"eprom.bin").toString(),
                "--opsys",new File(firmware,"opsys.dwn").toString(),
                "--max-insns","50000000000","--output-dir",session.toString(),
                new File(firmware,"card.bin").toString()));
            ProcessBuilder builder=new ProcessBuilder(args).directory(session).redirectErrorStream(true);
            com.opensaab.usb.DemandStartup.configure(this,builder);
            if(startup!=null)startup.session(session);
            builder.environment().put("OPENSAAB_PERFORMANCE_DIR",session.getAbsolutePath());
            java.lang.Process child=builder.start();
            process=child;
            // The UI cannot be trapped by a blocked native process or lost ADB connection.
            if(stopping.get()) child.destroy();
            Thread reader=new Thread(()->readLogs(child),"tech2-console"); reader.start();
            long deadline=System.nanoTime()+TimeUnit.MINUTES.toNanos(30);
            File mailbox=new File(session,"interactive-key.txt");
            keyPump=new com.opensaab.usb.InteractiveKeyPump(mailbox,e->ui.post(()->append("Input stopped: "+e)));
            health.begin(session);
            lcdPump.setDirectory(session);
            while(child.isAlive() && !stopping.get() && System.nanoTime()<deadline) {
                Thread.sleep(50); // Lifecycle only; input and changed-frame delivery are independent.
            }
            keyPump.close();keyPump=null;
            if(child.isAlive()) {
                publish(mailbox,"stop\n");
                if(!child.waitFor(2,TimeUnit.SECONDS)) { child.destroyForcibly(); child.waitFor(2,TimeUnit.SECONDS); }
            }
            reader.join(2000);
            // NativeLcdPump observes final frame publication as well.
            end=stopping.get()?"Offline menus stopped":"Offline session ended (exit "+child.exitValue()+")";
        } catch(Exception e) {
            end="Cannot run firmware: "+e.getMessage();
        } finally {
            com.opensaab.usb.InteractiveKeyPump pump=keyPump;if(pump!=null)pump.close();keyPump=null;
            java.lang.Process child=process;
            if(child!=null && child.isAlive()) child.destroyForcibly();
            process=null;
            health.ended();
            if(firmwareLease!=null)firmwareLease.close();
            final String message=end;
            ui.post(()->{running=false;start.setText("Connect and start");start.setEnabled(true);status.setText(message);append(message);});
        }
    }
    private void readLogs(java.lang.Process child) {
        try(BufferedReader in=new BufferedReader(new InputStreamReader(child.getInputStream()));
            FileOutputStream output=new FileOutputStream(new File(session,"console.log"))) {
            String line; long saved=0;
            while((line=in.readLine())!=null) {
                if(line.length()>4096) line=line.substring(0,4096);
                if(saved<2*1024*1024) { byte[] bytes=(line+"\n").getBytes(StandardCharsets.UTF_8);output.write(bytes);saved+=bytes.length; }
                if(startup!=null && line.startsWith("STARTUP_READY:")) {
                    startup.ready(line.substring("STARTUP_READY:".length()));
                    ui.post(()->{status.setText("Ready · CANdi starts when needed · offline test");lcd.invalidate();});
                }
                if(line.startsWith("CANDI_STARTING:"))ui.post(()->status.setText("Starting CANdi…"));
                if(line.startsWith("CANDI_INITIALIZED:")) {
                    final boolean failed=line.contains("\"status\":\"failed\"");
                    ui.post(()->status.setText(failed?"CANdi initialization failed":"Preparing CANdi firmware…"));
                }
                if(line.startsWith("CANDI_GUEST_INITIALIZED:"))ui.post(()->status.setText("CANdi firmware initialized · no adapter connected"));
                final String text=line;
                // Avoid thousands of pending UI callbacks from boot diagnostics.
                if(text.startsWith("LCD:") || text.startsWith("KEYPAD:") || text.startsWith("SESSION:") || text.contains("ERROR") || text.contains("CRASH") || text.contains("| CANDI]")) {
                    ui.post(()->{append(text);if(text.startsWith("LCD:") && !stopping.get())status.setText("Emulation mode • Offline • No vehicle connection");});
                }
            }
        } catch(IOException e) { ui.post(()->append("Console closed: "+e.getMessage())); }
    }
    private static void publish(File target,String text) throws IOException {
        File tmp=new File(target.toString()+".tmp");
        Files.write(tmp.toPath(),text.getBytes(StandardCharsets.US_ASCII));
        Files.move(tmp.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
    }
    private static void remove(File file) { File[] children=file.listFiles(); if(children!=null)for(File child:children)remove(child);file.delete(); }
    private void stopSession(String reason) { if(health!=null)health.expectedStop();if(running) { stopping.set(true);com.opensaab.usb.InteractiveKeyPump pump=keyPump;if(pump!=null)pump.cancel();status.setText(reason+"…"); } }
    @Override protected void onStart() { super.onStart();foreground=true;ui.removeCallbacks(consoleRefresh);ui.post(consoleRefresh);
        ui.removeCallbacks(historyRefresh);ui.post(historyRefresh);
        refreshAdapterLabel();
        if(!running){String missing=new com.opensaab.usb.FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty())status.setText("Firmware setup needed — tap Firmware");else if(status.getText().toString().startsWith("Firmware setup needed"))status.setText("Ready — connect an adapter or explore offline");}
    }
    @Override protected void onStop() { foreground=false;ui.removeCallbacks(consoleRefresh);ui.removeCallbacks(historyRefresh);stopSession("Stopped in background");super.onStop(); }
    private void refreshVehicleHistory(){
        if(!foreground||isDestroyed()||!historyPending.compareAndSet(false,true))return;
        try{historyWorker.execute(()->{
            com.opensaab.usb.VehicleIdentity last=com.opensaab.usb.VehicleSession.read(new File(getFilesDir(),"last-vehicle.json"));
            com.opensaab.usb.SecurityAccessStatus receipt=com.opensaab.usb.SecurityAccessStatus.read(new File(getNoBackupFilesDir(),"security-processing-status.properties"));
            com.opensaab.usb.VehicleHistoryStatus history=new com.opensaab.usb.VehicleHistoryStatus(last,receipt,new File(getFilesDir(),"firmware/card.bin"),java.time.Instant.now());
            ui.post(()->{
                historyPending.set(false);if(!foreground||isDestroyed())return;
                vehicleSummary.setText(last==null?"Connect an adapter to identify your vehicle":"Last vehicle · "+last.description());
                connectionDate.setText(history.connection);authStatus.setText(history.auth);authStatus.setTextColor(history.color);authDate.setText(history.timestamp);
                compactVehicle=last==null?"No vehicle connected · tap for details":"Last vehicle · "+last.modelYear+" · "+last.platform;
                compactSecurity=last==null?"":history.auth.replace("auth_status:","Security:");updateWorkspace();
                vehicleDetails=(last==null?"No saved vehicle connection":last.description()+"\nVIN: "+last.vin)+"\n\n"+history.connection+"\n"+history.auth+"\n"+history.timestamp+
                    "\n\nSecurity status describes the saved emulator data. Fresh/Stale is an age reminder, not a vehicle-access expiry.";
            });
        });}catch(RejectedExecutionException stopped){historyPending.set(false);}
    }
    @Override public void onWindowFocusChanged(boolean focus){super.onWindowFocusChanged(focus);if(focus&&connectionDate!=null)refreshVehicleHistory();}
    @Override protected void onDestroy(){ui.removeCallbacks(consoleRefresh);if(lcdPump!=null)lcdPump.close();ui.removeCallbacks(historyRefresh);historyWorker.shutdownNow();super.onDestroy();}
    @android.annotation.SuppressLint("GestureBackNavigation") // API 33+ uses BackNavigation; this handles older Android.
    @Override public void onBackPressed() { if(running)key(0x01);else super.onBackPressed(); }
    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        int code=event.getKeyCode();
        boolean digit=code>=KeyEvent.KEYCODE_0 && code<=KeyEvent.KEYCODE_9;
        boolean function=code>=KeyEvent.KEYCODE_F1 && code<=KeyEvent.KEYCODE_F10;
        boolean mapped=digit || function || code==KeyEvent.KEYCODE_DPAD_UP
            || code==KeyEvent.KEYCODE_DPAD_DOWN || code==KeyEvent.KEYCODE_ENTER
            || code==KeyEvent.KEYCODE_NUMPAD_ENTER || code==KeyEvent.KEYCODE_DPAD_CENTER
            || code==KeyEvent.KEYCODE_ESCAPE;
        if(!mapped || !running) return super.dispatchKeyEvent(event);
        // Intercept before focused Android buttons consume Enter/arrow keys.
        // Consume the matching key-up too, avoiding a second host button click.
        if(event.getAction()!=KeyEvent.ACTION_DOWN || event.getRepeatCount()>0)return true;
        if(digit)key(DIGITS[code-KeyEvent.KEYCODE_0]);
        else if(function)key(DIGITS[code-KeyEvent.KEYCODE_F1]);
        else switch(code) {
            case KeyEvent.KEYCODE_DPAD_UP:key(0x09);break;
            case KeyEvent.KEYCODE_DPAD_DOWN:key(0x0c);break;
            case KeyEvent.KEYCODE_ESCAPE:key(0x01);break;
            default:enqueue("enter");break;
        }
        return true;
    }
    private final class LcdView extends View {
        Bitmap frame; final Paint paint=new Paint();
        LcdView() {super(MainActivity.this);setContentDescription("Original Tech2 firmware LCD");paint.setFilterBitmap(false);}
        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);canvas.drawColor(0xff04090e);
            if(frame!=null) {float scale=Math.min(getWidth()/320f,getHeight()/240f);float w=320*scale,h=240*scale;
                canvas.drawBitmap(frame,null,new RectF((getWidth()-w)/2,(getHeight()-h)/2,(getWidth()+w)/2,(getHeight()+h)/2),paint);
                if(startup!=null)startup.drawn(frame);}
        }
    }
}
