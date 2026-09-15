// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.content.*;
import android.hardware.usb.*;
import android.os.Build;
import android.widget.*;
import org.json.*;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.*;

/** Descriptor inventory only: never opens a device, claims an interface, or sends commands. */
public final class AdapterDetectorActivity extends Activity {
    private UsbManager manager;
    private LinearLayout devices;
    private TextView status;
    private boolean registered;
    private final BroadcastReceiver changes=new BroadcastReceiver(){public void onReceive(Context c,Intent i){refresh();}};
    private TextView text(String s,int size){TextView v=new TextView(this);v.setText(s);v.setTextSize(size);v.setTextColor(0xffdce9f2);v.setTextIsSelectable(true);v.setPadding(0,6,0,6);return v;}
    @Override public void onCreate(android.os.Bundle state){
        super.onCreate(state);manager=(UsbManager)getSystemService(USB_SERVICE);
        LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setBackgroundColor(0xff0d1620);
        root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(24,i.getSystemWindowInsetTop()+16,24,i.getSystemWindowInsetBottom()+16);return i;});
        root.addView(text("OpenSAAB · Adapter detector",22));
        root.addView(text("USB inventory · no vehicle commands\nNano · Chipsoft Pro · Bosch/ETAS (MDI family)",14));
        status=text("Scanning attached USB devices…",15);root.addView(status);
        LinearLayout actions=new LinearLayout(this);root.addView(actions,new LinearLayout.LayoutParams(-1,-2));
        Button refresh=new Button(this);refresh.setText("Refresh");refresh.setOnClickListener(v->refresh());actions.addView(refresh,new LinearLayout.LayoutParams(0,-2,1));
        Button back=new Button(this);back.setText("Back");back.setOnClickListener(v->finish());actions.addView(back,new LinearLayout.LayoutParams(0,-2,1));
        ScrollView scroll=new ScrollView(this);devices=new LinearLayout(this);devices.setOrientation(LinearLayout.VERTICAL);scroll.addView(devices);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        setContentView(root);
        IntentFilter f=new IntentFilter(UsbManager.ACTION_USB_DEVICE_ATTACHED);f.addAction(UsbManager.ACTION_USB_DEVICE_DETACHED);
        if(Build.VERSION.SDK_INT>=33)registerReceiver(changes,f,Context.RECEIVER_NOT_EXPORTED);else registerReceiver(changes,f);registered=true;
        refresh();
    }
    private static String stringValue(String s){return s==null?"Unavailable":s;}
    private void refresh(){
        devices.removeAllViews();
        try {
            JSONObject report=new JSONObject();report.put("schema",1);report.put("observed_at_ms",System.currentTimeMillis());
            report.put("test","usb_descriptor_inventory");report.put("scope","attached_usb_only");
            report.put("device_opened",false);report.put("vehicle_commands_sent",0);report.put("active_identity_probe",false);
            List<UsbDevice> found=new ArrayList<>(manager.getDeviceList().values());Collections.sort(found,Comparator.comparingInt((UsbDevice d)->AdapterCatalog.adapterCandidate(AdapterCatalog.identify(d.getVendorId(),d.getProductId()))?0:1).thenComparing(UsbDevice::getDeviceName));
            JSONArray rows=new JSONArray();int candidates=0;
            for(UsbDevice d:found){
                AdapterCatalog.Match match=AdapterCatalog.identify(d.getVendorId(),d.getProductId());
                if(AdapterCatalog.adapterCandidate(match))candidates++;
                JSONObject row=new JSONObject();row.put("usb_path",d.getDeviceName());row.put("vendor_id",d.getVendorId());row.put("product_id",d.getProductId());
                row.put("usb_id",String.format(Locale.ROOT,"%04X:%04X",d.getVendorId(),d.getProductId()));row.put("family",match.family);row.put("label",match.label);
                row.put("transport_hint",match.transport);row.put("backend_readiness",match.readiness);row.put("evidence",match.evidence);
                row.put("identity_verified",false);row.put("firmware_version",JSONObject.NULL);row.put("vehicle_link_verified",false);
                row.put("usb_permission_granted",manager.hasPermission(d));
                String manufacturer="Unavailable",product="Unavailable";
                try{manufacturer=stringValue(d.getManufacturerName());product=stringValue(d.getProductName());}catch(SecurityException ignored){}
                row.put("manufacturer",manufacturer);row.put("product",product);row.put("usb_version",d.getVersion());
                JSONArray interfaces=new JSONArray();boolean cdc=false,bulkPair=false,rndis=false;
                StringBuilder layout=new StringBuilder();
                for(int i=0;i<d.getInterfaceCount();i++){
                    UsbInterface iface=d.getInterface(i);JSONObject item=new JSONObject();item.put("id",iface.getId());item.put("alternate",iface.getAlternateSetting());
                    item.put("class",iface.getInterfaceClass());item.put("subclass",iface.getInterfaceSubclass());item.put("protocol",iface.getInterfaceProtocol());
                    if(iface.getInterfaceClass()==2 && iface.getInterfaceSubclass()==2)cdc=true;
                    if((iface.getInterfaceClass()==0xe0 && iface.getInterfaceSubclass()==1 && iface.getInterfaceProtocol()==3)
                        || (iface.getInterfaceClass()==2 && iface.getInterfaceSubclass()==2 && iface.getInterfaceProtocol()==0xff))rndis=true;
                    JSONArray endpoints=new JSONArray();boolean in=false,out=false;
                    for(int j=0;j<iface.getEndpointCount();j++){
                        UsbEndpoint e=iface.getEndpoint(j);JSONObject ep=new JSONObject();ep.put("address",e.getAddress());ep.put("direction",e.getDirection()==UsbConstants.USB_DIR_IN?"in":"out");ep.put("type",e.getType());ep.put("max_packet_size",e.getMaxPacketSize());endpoints.put(ep);
                        if(e.getType()==UsbConstants.USB_ENDPOINT_XFER_BULK){if(e.getDirection()==UsbConstants.USB_DIR_IN)in=true;else out=true;}
                    }
                    if(in&&out)bulkPair=true;item.put("endpoints",endpoints);interfaces.put(item);
                    layout.append(String.format(Locale.ROOT,"Interface %d/%d · class %02X/%02X/%02X · %d endpoints\n",iface.getId(),iface.getAlternateSetting(),iface.getInterfaceClass(),iface.getInterfaceSubclass(),iface.getInterfaceProtocol(),iface.getEndpointCount()));
                }
                row.put("interfaces",interfaces);row.put("cdc_control_seen",cdc);row.put("bulk_in_out_pair_seen",bulkPair);row.put("rndis_descriptor_seen",rndis);
                String shape;
                if(match.family.equals("nano_candidate") || match.family.equals("chipsoft_candidate"))shape=cdc&&bulkPair?"Serial interface layout present":"Expected serial layout not confirmed";
                else if(match.transport.contains("RNDIS"))shape=rndis?"USB network interface descriptor present":"Network transport needs descriptor/OS verification";
                else shape="No adapter-specific interface expectation";
                row.put("interface_assessment",shape);rows.put(row);
                LinearLayout card=new LinearLayout(this);card.setOrientation(LinearLayout.VERTICAL);card.setPadding(18,12,18,12);card.setBackgroundColor(0xff182736);
                LinearLayout.LayoutParams cp=new LinearLayout.LayoutParams(-1,-2);cp.setMargins(0,0,0,16);devices.addView(card,cp);
                card.addView(text(match.label,18));card.addView(text(row.getString("usb_id")+" · "+product+"\n"+manufacturer+" · "+d.getDeviceName(),13));
                card.addView(text("Identity: unverified\nFirmware: not queried\n"+match.transport+"\n"+shape+"\n"+match.readiness,14));
                card.addView(text(match.evidence,12));card.addView(text(layout.toString().trim(),12));
                if(match.family.equals("chipsoft_candidate")){
                    Button identify=new Button(this);identify.setText("Identify Chipsoft · no vehicle commands");
                    identify.setOnClickListener(v->startActivity(new Intent(this,ChipsoftUsbActivity.class).putExtra("auto_start",true)));card.addView(identify);
                    Button receive=new Button(this);receive.setText("Receive P-bus / I-bus · 8 seconds");receive.setOnClickListener(v->startActivity(new Intent(this,ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("receive_test",true)));card.addView(receive);
                    Button vin=new Button(this);vin.setText("Read VIN and model year");vin.setOnClickListener(v->startActivity(new Intent(this,ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("vin_check",true)));card.addView(vin);
                    Button nativeRead=new Button(this);nativeRead.setText("Open original firmware · read diagnostics");nativeRead.setOnClickListener(v->startActivity(new Intent(this,ChipsoftUsbActivity.class).putExtra("auto_start",true).putExtra("native_firmware",true)));card.addView(nativeRead);
                }
            }
            report.put("devices",rows);report.put("adapter_candidates",candidates);report.put("automatic_selection",false);
            report.put("status",found.isEmpty()?"no_usb_devices":candidates==0?"no_known_adapter_candidate":"inventory_complete");
            File pending=new File(getFilesDir(),"adapter-detection.json.tmp"),dest=new File(getFilesDir(),"adapter-detection.json");
            Files.write(pending.toPath(),report.toString(2).getBytes(StandardCharsets.UTF_8));Files.move(pending.toPath(),dest.toPath(),StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);
            status.setText(found.isEmpty()?"No USB devices attached — connect an adapter and refresh":found.size()+" USB devices · "+candidates+" adapter "+(candidates==1?"candidate":"candidates")+" · none opened");
            if(candidates>1)devices.addView(text("Multiple candidates detected. No adapter was selected automatically.",14));
            devices.addView(text("This test checks attached USB devices. MDI discovery over Ethernet/Wi-Fi and firmware identification are separate steps. No CAN support is inferred from a USB match.",13));
            android.util.Log.i("OpenSaabAdapters","INVENTORY devices="+found.size()+" candidates="+candidates+" device_opened=false report="+dest.getName());
        }catch(Exception e){
            new File(getFilesDir(),"adapter-detection.json").delete();
            status.setText("Detection failed: "+e.getMessage());android.util.Log.e("OpenSaabAdapters","Inventory failed",e);
        }
    }
    @Override protected void onDestroy(){if(registered)unregisterReceiver(changes);super.onDestroy();}
}
