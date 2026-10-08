// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.Manifest;
import android.app.*;
import android.bluetooth.*;
import android.content.*;
import android.content.pm.PackageManager;
import android.os.*;
import android.provider.Settings;
import android.widget.*;
import java.util.*;

/** Foreground permissions, paired list and bounded Classic discovery; no vehicle I/O. */
public final class VlinkerDevicePicker {
    public static final int PERMISSION_REQUEST=8731;
    public interface Selection {void selected(BluetoothDevice device,boolean compatibility);}
    private final Activity activity;private final Selection selection;
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final Map<String,BluetoothDevice> nearby=new LinkedHashMap<>();
    private final List<BluetoothDevice> devices=new ArrayList<>();
    private TextView status;private Spinner picker;private Button choose,scan;private CheckBox compatibility;
    private Dialog dialog;private boolean scanning,registered,closed;private boolean scanAfterPermission;
    private final Runnable deadline=()->finishScan("Search finished. Press the adapter's button and search again if needed.");
    static boolean androidName(String name){if(name==null)return false;String n=name.trim().toLowerCase(Locale.ROOT);return n.equals("vlinker mc")||n.equals("vlinker mc-android")||n.equals("carista evo");}
    public VlinkerDevicePicker(Activity activity,Selection selection){this.activity=activity;this.selection=selection;}
    private BluetoothAdapter adapter(){BluetoothManager manager=(BluetoothManager)activity.getSystemService(Context.BLUETOOTH_SERVICE);return manager==null?null:manager.getAdapter();}
    private boolean permission(String name){return activity.checkSelfPermission(name)==PackageManager.PERMISSION_GRANTED;}
    private boolean connectPermission(){return Build.VERSION.SDK_INT<31||permission(Manifest.permission.BLUETOOTH_CONNECT);}
    private boolean scanPermission(){return permission(Build.VERSION.SDK_INT>=31?Manifest.permission.BLUETOOTH_SCAN:Manifest.permission.ACCESS_FINE_LOCATION);}
    private Button button(LinearLayout content,String text,Runnable action){Button b=new Button(activity);b.setText(text);SessionStyle.button(b,false);content.addView(b);b.setOnClickListener(v->action.run());return b;}
    public void show(){
        LinearLayout content=new LinearLayout(activity);content.setOrientation(LinearLayout.VERTICAL);
        status=new TextView(activity);status.setTextSize(14);status.setTextColor(0xffedf4fa);content.addView(status);
        TextView help=new TextView(activity);help.setText("Pair Carista EVO or vLinker MC+ in Bluetooth settings, then select it here. Close other diagnostic apps first. OpenSAAB checks the model and firmware after connecting.");help.setTextColor(0xffedf4fa);content.addView(help);
        picker=new Spinner(activity);picker.setTag("vlinker-device-list");content.addView(picker);
        button(content,"Refresh paired adapters",()->{if(!connectPermission())requestPermission(false);else refresh();});
        scan=button(content,"Find nearby adapters",this::findNearby);
        button(content,"Turn on Bluetooth",this::enableBluetooth);
        button(content,"Bluetooth settings",()->activity.startActivity(new Intent(Settings.ACTION_BLUETOOTH_SETTINGS)));
        compatibility=new CheckBox(activity);compatibility.setText("Connect without saved Bluetooth pairing");compatibility.setTextColor(0xffedf4fa);content.addView(compatibility);
        choose=button(content,"Use selected adapter",this::select);choose.setTag("vlinker-use-selected");
        dialog=SessionSheet.show(activity,"Select Bluetooth adapter",content);dialog.setOnDismissListener(d->close());refresh();
    }
    private void enableBluetooth(){
        if(!connectPermission()){requestPermission(false);return;}
        try{activity.startActivityForResult(new Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE),PERMISSION_REQUEST+1);}
        catch(SecurityException denied){status.setText("Allow Nearby devices, then tap Turn on Bluetooth again.");}
    }
    private void requestPermission(boolean search){
        scanAfterPermission=search;
        activity.requestPermissions(Build.VERSION.SDK_INT>=31?new String[]{Manifest.permission.BLUETOOTH_CONNECT,Manifest.permission.BLUETOOTH_SCAN}:new String[]{Manifest.permission.ACCESS_FINE_LOCATION},PERMISSION_REQUEST);
    }
    public void permissionsResult(int request){
        if(closed||request!=PERMISSION_REQUEST)return;
        boolean search=scanAfterPermission;scanAfterPermission=false;refresh();
        if(search&&connectPermission()&&scanPermission())findNearby();
        else if(!connectPermission()||(search&&!scanPermission()))status.setText("Bluetooth permission was not granted. Use Refresh or Find nearby after allowing it in Settings.");
    }
    public void refresh(){
        if(closed||status==null)return;devices.clear();List<String> labels=new ArrayList<>();
        if(!connectPermission())status.setText("Allow Nearby devices to list and connect to paired adapters. Tap Refresh paired adapters.");
        else try{
            BluetoothAdapter bt=adapter();
            if(bt==null)status.setText("This device has no Bluetooth adapter.");
            else if(!bt.isEnabled())status.setText("Turn on Bluetooth, then refresh the adapter list.");
            else{
                Map<String,BluetoothDevice> found=new LinkedHashMap<>(nearby);
                for(BluetoothDevice d:bt.getBondedDevices())if(androidName(d.getName()))found.put(d.getAddress(),d);
                for(BluetoothDevice d:found.values())if(androidName(d.getName()))devices.add(d);
                devices.sort(Comparator.comparing(BluetoothDevice::getAddress));
                for(BluetoothDevice d:devices)labels.add(d.getName()+" · "+d.getAddress()+(d.getBondState()==BluetoothDevice.BOND_BONDED?" · paired":" · nearby"));
                if(!scanning)status.setText(devices.isEmpty()?"No supported Bluetooth adapter found. Pair it or use Find nearby adapters.":"Select your adapter. Connecting starts only after Use selected adapter.");
            }
        }catch(SecurityException denied){status.setText("Nearby devices permission is unavailable. Refresh after granting it.");devices.clear();labels.clear();}
        if(labels.isEmpty())labels.add("No Bluetooth adapter selected");
        ArrayAdapter<String> entries=new ArrayAdapter<>(activity,android.R.layout.simple_spinner_item,labels);entries.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);picker.setAdapter(entries);choose.setEnabled(!devices.isEmpty());
    }
    private final BroadcastReceiver receiver=new BroadcastReceiver(){public void onReceive(Context context,Intent intent){
        if(closed||!scanning)return;
        if(BluetoothAdapter.ACTION_DISCOVERY_FINISHED.equals(intent.getAction())){finishScan("Search finished.");return;}
        if(!BluetoothDevice.ACTION_FOUND.equals(intent.getAction())||!connectPermission())return;
        try{BluetoothDevice d=intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE);if(d!=null&&androidName(d.getName())){nearby.put(d.getAddress(),d);refresh();}}
        catch(SecurityException denied){finishScan("Bluetooth permission was revoked.");}
    }};
    private void findNearby(){
        if(closed||scanning)return;
        if(!connectPermission()||!scanPermission()){requestPermission(true);return;}
        try{
            BluetoothAdapter bt=adapter();if(bt==null||!bt.isEnabled()){refresh();return;}
            if(Build.VERSION.SDK_INT<31){android.location.LocationManager location=(android.location.LocationManager)activity.getSystemService(Context.LOCATION_SERVICE);if(location!=null&&!location.isProviderEnabled(android.location.LocationManager.GPS_PROVIDER)&&!location.isProviderEnabled(android.location.LocationManager.NETWORK_PROVIDER)){status.setText("Turn on Location services for Bluetooth discovery on this Android version, or use an already paired adapter.");return;}}
            IntentFilter filter=new IntentFilter(BluetoothDevice.ACTION_FOUND);filter.addAction(BluetoothAdapter.ACTION_DISCOVERY_FINISHED);
            if(Build.VERSION.SDK_INT>=33)activity.registerReceiver(receiver,filter,Context.RECEIVER_EXPORTED);else activity.registerReceiver(receiver,filter);registered=true;
            bt.cancelDiscovery();scanning=true;
            if(!bt.startDiscovery()){finishScan("Bluetooth search could not start. Check permission and Bluetooth settings.");return;}
            status.setText("Searching for Carista EVO and vLinker MC+…");scan.setEnabled(false);ui.postDelayed(deadline,15000);
        }catch(SecurityException denied){finishScan("Bluetooth permission was revoked. Allow it and search again.");}
        catch(Exception error){finishScan("Bluetooth search unavailable. Check permissions and settings.");}
    }
    private void finishScan(String message){
        ui.removeCallbacks(deadline);
        if(scanning&&connectPermission()&&(Build.VERSION.SDK_INT<31||scanPermission()))try{BluetoothAdapter bt=adapter();if(bt!=null)bt.cancelDiscovery();}catch(SecurityException ignored){}
        scanning=false;if(registered){try{activity.unregisterReceiver(receiver);}catch(IllegalArgumentException ignored){}registered=false;}
        if(scan!=null)scan.setEnabled(true);if(!closed){refresh();if(!message.isEmpty())status.setText(message);}
    }
    private void select(){
        if(closed||VlinkerVehicleConnection.active())return;
        if(!connectPermission()){requestPermission(false);return;}
        int index=picker.getSelectedItemPosition();if(index<0||index>=devices.size()){refresh();return;}
        BluetoothDevice device=devices.get(index);boolean mode=compatibility.isChecked();finishScan("");close();selection.selected(device,mode);
    }
    public void close(){if(closed)return;closed=true;finishScan("");if(dialog!=null){dialog.setOnDismissListener(null);dialog.dismiss();dialog=null;}nearby.clear();devices.clear();}
}
