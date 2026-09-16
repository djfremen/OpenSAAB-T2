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
    private final ArrayBlockingQueue<String> keys = new ArrayBlockingQueue<>(16);
    private final AtomicBoolean stopping = new AtomicBoolean();
    private volatile java.lang.Process process;
    private volatile boolean running;
    private boolean foreground;
    private File session;
    private TextView status, console;
    private TextView vehicleSummary;
    private ScrollView consoleScroll;
    private LcdView lcd;
    private Button start, stop;
    private final StringBuilder logs = new StringBuilder();
    private static final int[] DIGITS = {0x18,0x04,0x13,0x17,0x03,0x12,0x16,0x02,0x11,0x15};
    private int dp(int n) { return Math.round(n * getResources().getDisplayMetrics().density); }
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);com.opensaab.usb.BackNavigation.install(this,()->{if(running)key(0x01);else finish();});
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        LinearLayout root = new LinearLayout(this); root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.rgb(13,22,32));
        root.setOnApplyWindowInsetsListener((v,insets) -> {
            v.setPadding(dp(12), insets.getSystemWindowInsetTop()+dp(8), dp(12), insets.getSystemWindowInsetBottom()+dp(8));
            return insets;
        });
        root.addView(new com.opensaab.usb.BrandHeader(this,"OpenSAAB T2"));
        root.addView(label("Original firmware • USB adapters • Diagnostics", 12));
        vehicleSummary=label("Connect an adapter to identify your vehicle",13);
        vehicleSummary.setTextIsSelectable(true);root.addView(vehicleSummary);
        status = label("Ready — select an adapter to start", 13); status.setSingleLine(true); status.setEllipsize(android.text.TextUtils.TruncateAt.END); root.addView(status);
        LinearLayout actions = row(root);
        start = button(actions,"Start", () -> selectAdapter("native_dtc",true));
        stop = button(actions,"Stop emulation", () -> stopSession("Stopped by operator"));
        stop.setEnabled(false);
        button(actions,"Firmware",()->{if(running){status.setText("Stop emulation before changing firmware");return;}startActivity(new android.content.Intent(this,com.opensaab.usb.FirmwareActivity.class));});
        android.content.SharedPreferences adapterSettings=getSharedPreferences("adapter_settings",MODE_PRIVATE);
        Switch restricted=new Switch(this);
        restricted.setText("Chipsoft restricted mode");restricted.setTextColor(0xffdce9f2);
        restricted.setChecked(adapterSettings.getBoolean("chipsoft_restricted",false));
        restricted.setOnCheckedChangeListener((view,checked)->
            adapterSettings.edit().putBoolean("chipsoft_restricted",checked).apply());
        root.addView(restricted);
        LinearLayout diagnostics = row(root);
        button(diagnostics,"Seed",()->selectAdapter("native_seed",false));
        button(diagnostics,"Read DTC",()->selectAdapter("native_dtc",false));
        button(diagnostics,"Clear DTC",()->selectAdapter("native_clear_dtc",false));
        button(diagnostics,"Engine Data",()->selectAdapter("native_engine_data",false));
        lcd = new LcdView();
        LinearLayout extras = row(root);
        button(extras,"Adapters",()->{
            if(running){status.setText("Stop firmware before opening adapter detection");return;}
            startActivity(new android.content.Intent(this,com.opensaab.usb.AdapterDetectorActivity.class));
        });
        button(extras,"USB tests",()->{
            if(running){status.setText("Stop offline menus before opening USB tests");return;}
            startActivityForResult(new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class),27);
        });
        button(extras,"Report issue",()->{if(running){status.setText("Stop emulation before preparing a report");return;}startActivity(new android.content.Intent(this,com.opensaab.usb.SupportReportActivity.class));});
        button(extras,"DTC reports",()->com.opensaab.usb.DtcReportView.showSavedReports(this));
        Button updates=new Button(this);updates.setText("Check for updates");updates.setAllCaps(false);
        updates.setOnClickListener(v->{if(running){status.setText("Stop firmware before checking for updates");return;}com.opensaab.usb.AppUpdates.show(this);});
        LinearLayout systemActions = row(root);
        systemActions.addView(updates,new LinearLayout.LayoutParams(0,-2,1));
        button(systemActions,"System check",()->com.opensaab.usb.DeviceCompatibility.show(this));
        LinearLayout authActions=row(root);
        button(authActions,"Security access password",()->com.opensaab.usb.SecurityAuthorization.show(this));
        button(authActions,"Clear offset · fresh access",()->com.opensaab.usb.SecurityReset.show(this,()->running));
        console=label("Console: waiting for firmware",11);
        console.setTypeface(Typeface.MONOSPACE);
        consoleScroll = new ScrollView(this); consoleScroll.addView(console);
        root.addView(new com.opensaab.usb.Tech2Controls(this,lcd,consoleScroll,code->{if(code==0x10)enqueue("enter");else key(code);}),new LinearLayout.LayoutParams(-1,0,1));
        root.addView(com.opensaab.usb.ProjectSupport.button(this,()->{
            if(running || com.opensaab.usb.FirmwareGate.busy() || com.opensaab.usb.SecurityAccessView.workflowBusy()){
                com.opensaab.usb.ProjectSupport.waitForSession(this);return;
            }
            com.opensaab.usb.ProjectSupport.show(this);
        }),new LinearLayout.LayoutParams(-1,-2));
        com.opensaab.usb.HeadunitLayout.apply(root);
        setContentView(root);
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
    private LinearLayout row(LinearLayout parent) {
        LinearLayout r=new LinearLayout(this); r.setOrientation(LinearLayout.HORIZONTAL);
        parent.addView(r,new LinearLayout.LayoutParams(-1,dp(44))); return r;
    }
    private Button button(LinearLayout row,String text,Runnable action) {
        Button b=new Button(this); b.setText(text); b.setTextSize(12); b.setAllCaps(false);
        b.setPadding(0,0,0,0); b.setMinWidth(0); b.setMinimumWidth(0);
        b.setContentDescription(text); b.setOnClickListener(v->action.run());
        row.addView(b,new LinearLayout.LayoutParams(0,-1,1)); return b;
    }
    private void key(int code) { enqueue(String.format(Locale.ROOT,"0x%02x",code)); }
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
            String label=match.family.equals("nano_candidate")?"VCX Nano":match.family.equals("chipsoft_candidate")?"Chipsoft Pro":match.label+" — not supported yet";
            labels.add(label+"\nUSB "+device.getDeviceName());
        }
        android.app.AlertDialog.Builder picker=new android.app.AlertDialog.Builder(this).setTitle("Select adapter");
        if(devices.isEmpty()){
            new com.opensaab.usb.ConnectionAttempt(this,com.opensaab.usb.ConnectionAttempt.Adapter.SELECTION)
                .finish(com.opensaab.usb.ConnectionAttempt.Outcome.FAILED,com.opensaab.usb.ConnectionAttempt.Reason.NO_ADAPTER);
            picker.setMessage("No supported adapter detected. Connect a USB adapter, then tap Refresh. If this keeps happening, use Report issue on the home screen.");
        }
        else picker.setItems(labels.toArray(new String[0]),(dialog,index)->connectAdapter(devices.get(index),mode));
        picker.setPositiveButton("Refresh",(dialog,which)->selectAdapter(mode,allowEmulation));
        picker.setNegativeButton("Cancel",null);
        if(allowEmulation)picker.setNeutralButton("Run in emulation mode",(dialog,which)->startSession());
        picker.show();
    }
    private void connectAdapter(android.hardware.usb.UsbDevice chosen,String mode){
        android.hardware.usb.UsbManager usb=(android.hardware.usb.UsbManager)getSystemService(USB_SERVICE);
        android.hardware.usb.UsbDevice current=usb.getDeviceList().get(chosen.getDeviceName());
        if(current==null || current.getDeviceId()!=chosen.getDeviceId()
            || current.getVendorId()!=chosen.getVendorId() || current.getProductId()!=chosen.getProductId()){
            status.setText("Selected adapter disconnected — select again");return;
        }
        String family=com.opensaab.usb.AdapterCatalog.identify(current.getVendorId(),current.getProductId()).family;
        android.content.Intent launch;
        if(family.equals("nano_candidate")){
            launch=new android.content.Intent(this,com.opensaab.usb.NanoProbeActivity.class).putExtra(mode,true);
        }else if(family.equals("chipsoft_candidate")){
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
        }else{status.setText("This adapter is detected, but its Android driver is not ready");return;}
        status.setText("Opening selected adapter…");
        startActivityForResult(launch.putExtra("usb_device_name",chosen.getDeviceName()).putExtra("auto_start",true),27);
    }
    private void enqueue(String command) {
        if (!running || stopping.get()) { status.setText("Tap Start to select an adapter or emulation mode"); return; }
        if(!keys.offer(command)) status.setText("Key queue full — wait for the menu");
    }
    private void append(String line) {
        // Called on the UI thread; bound both the retained and visible console.
        logs.append(line.replaceAll("\\x1b\\[[0-9;]*m", "")).append('\n');
        if(logs.length()>12000) logs.delete(0,logs.length()-8000);
        console.setText(logs.toString());
        consoleScroll.post(()->consoleScroll.fullScroll(View.FOCUS_DOWN));
        if(line.startsWith("SESSION:")) status.setText(line.substring(8).trim());
    }
    private void startSession() {
        if(running || !foreground || com.opensaab.usb.SecurityAccessView.workflowBusy()) return;
        File firmware=new File(getFilesDir(),"firmware");
        for(String name:new String[]{"eprom.bin","opsys.dwn","card.bin","candi.bin"}) {
            if(!new File(firmware,name).isFile()) { status.setText("Missing "+name+" — install your firmware files first"); return; }
        }
        if(getFilesDir().getUsableSpace()<64L*1024*1024) { status.setText("Need 64 MB free for session logs"); return; }
        running=true; stopping.set(false); keys.clear(); logs.setLength(0);
        start.setEnabled(false); stop.setEnabled(true); lcd.frame=null; lcd.invalidate();
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
            builder.environment().put("OPENSAAB_PERFORMANCE_DIR",session.getAbsolutePath());
            java.lang.Process child=builder.start();
            process=child;
            // The UI cannot be trapped by a blocked native process or lost ADB connection.
            if(stopping.get()) child.destroy();
            Thread reader=new Thread(()->readLogs(child),"tech2-console"); reader.start();
            long deadline=System.nanoTime()+TimeUnit.MINUTES.toNanos(30);
            File mailbox=new File(session,"interactive-key.txt");
            while(child.isAlive() && !stopping.get() && System.nanoTime()<deadline) {
                if(!mailbox.exists()) {
                    String command=keys.poll();
                    if(command!=null) publish(mailbox,command+"\n");
                }
                File live=new File(session,"live.ppm");
                if(live.isFile()) {
                    Bitmap frame=readFrame(live);
                    ui.post(()->{ lcd.frame=frame; lcd.invalidate(); });
                }
                Thread.sleep(120);
            }
            if(child.isAlive()) {
                publish(mailbox,"stop\n");
                if(!child.waitFor(2,TimeUnit.SECONDS)) { child.destroyForcibly(); child.waitFor(2,TimeUnit.SECONDS); }
            }
            reader.join(2000);
            File finalFrame=new File(session,"lcd.ppm");
            if(finalFrame.isFile()) { Bitmap frame=readFrame(finalFrame); ui.post(()->{lcd.frame=frame;lcd.invalidate();}); }
            end=stopping.get()?"Offline menus stopped":"Offline session ended (exit "+child.exitValue()+")";
        } catch(Exception e) {
            end="Cannot run firmware: "+e.getMessage();
        } finally {
            java.lang.Process child=process;
            if(child!=null && child.isAlive()) child.destroyForcibly();
            process=null;
            if(firmwareLease!=null)firmwareLease.close();
            final String message=end;
            ui.post(()->{running=false;stop.setEnabled(false);start.setText("Start");start.setEnabled(true);status.setText(message);append(message);});
        }
    }
    private void readLogs(java.lang.Process child) {
        try(BufferedReader in=new BufferedReader(new InputStreamReader(child.getInputStream()));
            FileOutputStream output=new FileOutputStream(new File(session,"console.log"))) {
            String line; long saved=0;
            while((line=in.readLine())!=null) {
                if(line.length()>4096) line=line.substring(0,4096);
                if(saved<2*1024*1024) { byte[] bytes=(line+"\n").getBytes(StandardCharsets.UTF_8);output.write(bytes);saved+=bytes.length; }
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
    private static Bitmap readFrame(File file) throws IOException {
        byte[] data=Files.readAllBytes(file.toPath());
        byte[] header="P6\n320 240\n255\n".getBytes(StandardCharsets.US_ASCII);
        if(data.length!=header.length+320*240*3) throw new IOException("Incomplete guest framebuffer");
        for(int i=0;i<header.length;i++) if(data[i]!=header[i])throw new IOException("Invalid guest framebuffer");
        int[] pixels=new int[320*240]; int j=header.length;
        for(int i=0;i<pixels.length;i++) { pixels[i]=0xff000000|((data[j]&255)<<16)|((data[j+1]&255)<<8)|(data[j+2]&255); j+=3; }
        return Bitmap.createBitmap(pixels,320,240,Bitmap.Config.ARGB_8888);
    }
    private static void remove(File file) { File[] children=file.listFiles(); if(children!=null)for(File child:children)remove(child);file.delete(); }
    private void stopSession(String reason) { if(running) { stopping.set(true);keys.clear();status.setText(reason+"…"); } }
    @Override protected void onStart() { super.onStart();foreground=true;
        com.opensaab.usb.VehicleIdentity last=com.opensaab.usb.VehicleSession.read(new File(getFilesDir(),"last-vehicle.json"));
        if(last!=null)vehicleSummary.setText("Last vehicle · "+last.description()+"\nVIN: "+last.vin);
        if(!running){String missing=new com.opensaab.usb.FirmwareStore(getFilesDir()).missing();if(!missing.isEmpty())status.setText("Firmware setup needed — tap Firmware");else if(status.getText().toString().startsWith("Firmware setup needed"))status.setText("Ready — select an adapter to start");}
    }
    @Override protected void onStop() { foreground=false;stopSession("Stopped in background");super.onStop(); }
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
                canvas.drawBitmap(frame,null,new RectF((getWidth()-w)/2,(getHeight()-h)/2,(getWidth()+w)/2,(getHeight()+h)/2),paint);}
        }
    }
}
