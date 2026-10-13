// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.hardware.usb.*;
import android.widget.Toast;
import java.util.*;

/** Nano-only selection and Android launch binding. No reboot occurs on attachment/selection. */
public final class NanoRebootAction {
    static List<UsbDevice> candidates(Activity activity){
        UsbManager manager=(UsbManager)activity.getSystemService(Activity.USB_SERVICE);List<UsbDevice> devices=new ArrayList<>();
        for(UsbDevice device:manager.getDeviceList().values())if(NanoProfile.PROFILE.matches(device.getVendorId(),device.getProductId()))devices.add(device);
        devices.sort(Comparator.comparing(UsbDevice::getDeviceName));return devices;
    }
    public static boolean available(Activity activity){return !candidates(activity).isEmpty();}
    public static boolean idle(){return !NanoProbeActivity.sessionActive()&&!MdiUsbActivity.sessionActive()&&!VlinkerVehicleConnection.active()&&!FirmwareGate.busy()&&!SecurityAccessView.workflowBusy();}
    public static Intent intent(Activity activity,UsbDevice selected){
        if(selected==null||!NanoProfile.PROFILE.matches(selected.getVendorId(),selected.getProductId()))throw new IllegalArgumentException("Select Nano before rebooting");
        return new Intent(activity,NanoProbeActivity.class).putExtra("reboot_nano",true).putExtra("auto_start",true)
            .putExtra("usb_device_name",selected.getDeviceName()).putExtra("usb_device_id",selected.getDeviceId());
    }
    public static void choose(Activity activity){
        if(!idle()){Toast.makeText(activity,"Stop the current session before rebooting Nano",Toast.LENGTH_LONG).show();return;}
        List<UsbDevice> devices=candidates(activity);
        if(devices.isEmpty()){Toast.makeText(activity,"Connect Nano before choosing Reboot Nano",Toast.LENGTH_LONG).show();return;}
        if(devices.size()==1){activity.startActivityForResult(intent(activity,devices.get(0)),27);return;}
        String[] labels=new String[devices.size()];for(int i=0;i<labels.length;i++)labels[i]="VCX Nano · USB "+devices.get(i).getDeviceName();
        new AlertDialog.Builder(activity).setTitle("Select Nano to reboot").setItems(labels,(d,i)->{
            if(idle())activity.startActivityForResult(intent(activity,devices.get(i)),27);
        }).setNegativeButton("Cancel",null).show();
    }
    private NanoRebootAction(){}
}
