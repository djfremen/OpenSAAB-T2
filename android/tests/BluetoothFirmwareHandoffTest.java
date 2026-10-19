// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class BluetoothFirmwareHandoffTest {
    private static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    public static void main(String[] args){
        BluetoothFirmwareHandoff gate=new BluetoothFirmwareHandoff();long id=gate.begin();
        check(gate.finish(id,true,true,"identified","obdlink_mxp",true),"Verified MX+ did not hand off");
        check(!gate.finish(id,true,true,"identified","obdlink_mxp",true),"Duplicate terminal callback reopened adapter");
        id=gate.begin();gate.cancel();check(!gate.finish(id,true,true,"identified","obdlink_mxp",true),"Cancelled/background attempt started firmware");
        id=gate.begin();long newer=gate.begin();check(!gate.finish(id,true,true,"identified","obdlink_mxp",true),"Old selection started firmware");
        check(gate.finish(newer,true,true,"identified","obdlink_mxp",true),"Old callback consumed current selection");
        for(String model:new String[]{"vlinker_mc_plus","carista_evo","", "OBDLink MX+"}){
            id=gate.begin();check(!gate.finish(id,true,true,"identified",model,true),"Unverified model/name was qualified");
        }
        id=gate.begin();check(!gate.finish(id,true,false,"identified","obdlink_mxp",true),"Native owner still held");
        id=gate.begin();check(!gate.finish(id,false,true,"identified","obdlink_mxp",true),"Unavailable/background binding started");
        id=gate.begin();check(!gate.finish(id,true,true,"identified","obdlink_mxp",false),"Unconfirmed discovery/release started");
        for(String state:new String[]{"lookup","reading","failed","cancelled"}){
            id=gate.begin();check(!gate.finish(id,true,true,state,"obdlink_mxp",true),"Nonterminal identification started firmware");
        }
        System.out.println("BLUETOOTH_FIRMWARE_HANDOFF PASS: verified MX+, once-only, cancel/background, stale selection, unsupported models, unreleased owner, failed discovery");
    }
}
