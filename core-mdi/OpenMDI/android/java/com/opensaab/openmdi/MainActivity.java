// SPDX-License-Identifier: MPL-2.0
package com.opensaab.openmdi;
import android.app.*;
import android.content.*;
import android.hardware.usb.*;
import android.os.*;
import android.widget.*;
import java.io.*;
import java.util.*;

/** Private transport research harness. This does not replace the OpenSAAB product UI. */
public final class MainActivity extends Activity {
    static { System.loadLibrary("openmdi_android_probe"); }
    private static native int nativeRun(int fd, int mode, int serial, String directory, String executable);
    private UsbManager manager;
    private TextView status;
    private volatile boolean running;
    private int mode;
    private com.opensaab.usb.NativeLcdPump lcdPump;
    private com.opensaab.usb.InteractiveKeyPump keyPump;
    private volatile File activeDirectory;
    private boolean stopping;
    private LinearLayout root;
    private static final String PERMISSION="com.opensaab.openmdi.USB_PERMISSION";
    private final BroadcastReceiver receiver=new BroadcastReceiver(){
        public void onReceive(Context c, Intent i){
            if(PERMISSION.equals(i.getAction())){
                UsbDevice d=i.getParcelableExtra(UsbManager.EXTRA_DEVICE);
                if(i.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED,false)&&d!=null) execute(d);
                else show("USB permission was not granted. No adapter request sent.");
            }
        }
    };
    public void onCreate(Bundle b){
        super.onCreate(b);manager=(UsbManager)getSystemService(USB_SERVICE);
        getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        root=new LinearLayout(this);root.setOrientation(1);root.setPadding(24,24,24,24);
        status=new TextView(this);status.setTextSize(18);root.addView(status);
        String[] labels={"Check USB network and management", "Start PDU and read VIN", "Software reboot MDI", "Read live ECM data", "Read SPA preconditions", "Read SPA raw CAN frame", "Run original Tech2 with MDI"};
        for(int m=0;m<labels.length;m++){final int selected=m;Button v=new Button(this);v.setText(labels[m]);v.setOnClickListener(w->start(selected));root.addView(v);}
        setContentView(root);registerReceiver(receiver,new IntentFilter(PERMISSION),Context.RECEIVER_NOT_EXPORTED);
        show("Private OpenMDI transport research. Explicit classic-MDI fixture only.\nNo vehicle request until Start PDU and read VIN.");
        if(getIntent().hasExtra("mode"))start(getIntent().getIntExtra("mode",0));
    }
    private void show(String s){runOnUiThread(()->status.setText(s));}
    private void start(int selected){
        if(running){show("An attempt is already running.");return;}
        if(selected<0||selected>6){show("Invalid mode.");return;}
        mode=selected;
        ArrayList<UsbDevice> matches=new ArrayList<>();
        for(UsbDevice d:manager.getDeviceList().values())
            if(d.getVendorId()==0x0525 && d.getProductId()==0xa4a2)matches.add(d);
        if(matches.size()!=1){show("Expected exactly one explicitly selected 0525:a4a2 fixture; found "+matches.size());return;}
        UsbDevice d=matches.get(0);show("Opening selected classic-MDI research fixture: "+d.getDeviceName());
        if(manager.hasPermission(d))execute(d);
        else manager.requestPermission(d,PendingIntent.getBroadcast(this,0,new Intent(PERMISSION).setPackage(getPackageName()),PendingIntent.FLAG_MUTABLE|PendingIntent.FLAG_UPDATE_CURRENT));
    }
    private void execute(UsbDevice d){
        if(running)return;running=true;final int requested=mode;
        new Thread(()->{
            UsbDeviceConnection connection=null;UsbInterface control=null,data=null;boolean cc=false,dc=false;
            File directory=new File(getFilesDir(),"attempt-"+System.currentTimeMillis());directory.mkdir();
            activeDirectory=directory;stopping=false;
            if(requested==6)runOnUiThread(()->firmwareDisplay(new File(directory,"native")));
            try{
                UsbConfiguration cfg=null;
                for(int n=0;n<d.getConfigurationCount();n++)if(d.getConfiguration(n).getId()==2)cfg=d.getConfiguration(n);
                if(cfg==null)throw new IOException("Observed RNDIS configuration 2 absent");
                for(int n=0;n<cfg.getInterfaceCount();n++){
                    UsbInterface x=cfg.getInterface(n);
                    if(x.getId()==0&&x.getInterfaceClass()==2&&x.getInterfaceSubclass()==2&&x.getInterfaceProtocol()==255)control=x;
                    if(x.getId()==1&&x.getInterfaceClass()==10)data=x;
                }
                if(control==null||data==null)throw new IOException("Observed RNDIS descriptor shape absent");
                boolean in=false,out=false;
                for(int n=0;n<data.getEndpointCount();n++){UsbEndpoint ep=data.getEndpoint(n);if(ep.getType()==2){if(ep.getMaxPacketSize()!=64)throw new IOException("Observed full-speed endpoint packet size absent");in|=ep.getAddress()==0x81;out|=ep.getAddress()==0x02;}}
                if(!in||!out)throw new IOException("Observed bulk endpoints absent");
                connection=manager.openDevice(d);if(connection==null)throw new IOException("USB open failed");
                if(!connection.setConfiguration(cfg))throw new IOException("RNDIS configuration selection failed");
                cc=connection.claimInterface(control,true);if(!cc)throw new IOException("Control claim failed");
                dc=connection.claimInterface(data,true);if(!dc)throw new IOException("Data claim failed");
                show("USB claimed. Running independent RNDIS transport and shared Rust core…");
                // The serial is explicit local fixture data, provisioned separately from source.
                int serial=Integer.parseInt(new String(java.nio.file.Files.readAllBytes(new File(getFilesDir(),"serial-private.txt").toPath())).trim());
                int result=nativeRun(connection.getFileDescriptor(),requested,serial,directory.getAbsolutePath(),new File(getApplicationInfo().nativeLibraryDir,"libtech2_mdi.so").getAbsolutePath());
                show("Attempt finished: "+result+"\n"+directory.getName()+"\nEvidence retained privately. Releasing USB…");
            }catch(Throwable e){show("Attempt failed: "+e.getClass().getSimpleName()+": "+e.getMessage());
                try(FileWriter w=new FileWriter(new File(directory,"java-error.txt"))){w.write(e.toString());}catch(Exception ignored){}
            }finally{
                boolean dr=false,cr=false;
                if(connection!=null){if(dc)dr=connection.releaseInterface(data);if(cc)cr=connection.releaseInterface(control);connection.close();}
                try(FileWriter w=new FileWriter(new File(directory,"java-release.json"))){w.write("{\"data_claimed\":"+dc+",\"data_released\":"+dr+",\"control_claimed\":"+cc+",\"control_released\":"+cr+",\"connection_closed\":"+(connection!=null)+"}");}catch(Exception ignored){}
                if(keyPump!=null){keyPump.close();keyPump=null;}
                activeDirectory=null;
                runOnUiThread(()->{if(lcdPump!=null)lcdPump.clear();});
                running=false;
            }
        },"OpenMDI-attempt").start();
    }
    private void firmwareDisplay(File nativeDirectory){
        nativeDirectory.mkdirs();
        root.removeAllViews();root.addView(status);
        android.widget.ImageView lcd=new android.widget.ImageView(this);lcd.setScaleType(ImageView.ScaleType.FIT_CENTER);
        ScrollView console=new ScrollView(this);TextView note=new TextView(this);note.setText("Private MDI original-firmware test. Follow physical key prompts in the vehicle. EXIT navigates in firmware; Stop session releases MDI.");console.addView(note);
        com.opensaab.usb.Tech2Controls controls=new com.opensaab.usb.Tech2Controls(this,lcd,console,code->{
            if(!stopping && running && keyPump!=null && !keyPump.offer(String.format(java.util.Locale.ROOT,"0x%02x",code)))show("Input queue full; wait for the firmware.");
        });
        root.addView(controls,new LinearLayout.LayoutParams(-1,0,1));
        Button stop=new Button(this);stop.setText("Stop session");stop.setOnClickListener(v->stopFirmware());root.addView(stop);
        lcdPump=new com.opensaab.usb.NativeLcdPump(bitmap->{if(bitmap==null)lcd.setImageDrawable(null);else{lcd.setImageBitmap(bitmap);controls.showFirstUseGuide();}});lcdPump.setDirectory(nativeDirectory);
        keyPump=new com.opensaab.usb.InteractiveKeyPump(new File(nativeDirectory,"native-key.txt"),this::show);
    }
    private void stopFirmware(){
        if(stopping)return;stopping=true;if(keyPump!=null)keyPump.cancel();
        File run=activeDirectory;
        if(run!=null)new Thread(()->{try{java.nio.file.Files.write(new File(run,"native/native-stop").toPath(),new byte[]{1});}catch(Exception e){show("Stop failed: "+e.getMessage());}},"MDI-stop").start();
        show("Stopping original firmware and releasing MDI…");
    }
    public void onDestroy(){stopFirmware();if(lcdPump!=null)lcdPump.close();unregisterReceiver(receiver);super.onDestroy();}
}
