// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class SessionModesTest {
 private static void check(boolean ok,String reason){if(!ok)throw new AssertionError(reason);}
 public static void main(String[] args){
  check(SessionModes.forAdapter("chipsoft",false).allows(SessionModes.Mode.READ),"Chipsoft read missing");
  check(SessionModes.forAdapter("chipsoft",false).allows(SessionModes.Mode.FULL),"Chipsoft full native missing");
  check(!SessionModes.forAdapter("mdi",false).allows(SessionModes.Mode.READ),"MDI falsely claims read enforcement");
  check(SessionModes.forAdapter("mxplus",false).allows(SessionModes.Mode.FULL),"MX+ full native missing");
  check(!SessionModes.forAdapter("mxplus",false).allows(SessionModes.Mode.SEEDS),"Collection must remain a separate entry");
  check(SessionModes.forAdapter("nano",false).allows(SessionModes.Mode.FULL),"Release Nano full route hidden");
  check(!SessionModes.forAdapter("nano",false).allows(SessionModes.Mode.SEEDS),"Separate collector is not a normal mode");
  check(SessionModes.forAdapter("nano",true).allows(SessionModes.Mode.FULL),"Private Nano lost existing mode");
  check(!SessionModes.forAdapter("unverified",true).allows(SessionModes.Mode.CLEAR),"Unknown adapter inherits writes");
  check(!SessionModes.forAdapter("chipsoft",true).allows(null),"Missing choice selected a mode");
  check(SessionModes.usbFlag("chipsoft",SessionModes.Mode.READ,false).equals("native_firmware"),"Read-only Chipsoft selected a write worker");
  check(SessionModes.usbFlag("chipsoft",SessionModes.Mode.FULL,false).equals("full_native"),"Chipsoft choice lost native mode");
  check(SessionModes.usbFlag("nano",SessionModes.Mode.READ,false).equals("native_dtc"),"Nano read mode changed authority");
  check(SessionModes.usbFlag("nano",SessionModes.Mode.CLEAR,false).equals("native_clear_dtc"),"Nano clear inherited full control");
  check(SessionModes.usbFlag("nano",SessionModes.Mode.FULL,false).equals("nano_full_native"),"Full choice did not reach existing Nano native authority");
  check(SessionModes.bluetoothActionAvailable("native_seed",false),"Bluetooth security entry hidden");
  check(SessionModes.bluetoothAction("native_seed")==SessionModes.Mode.SEEDS,"Security entry inherited full authority");
  check(SessionModes.bluetoothAction("native_reset_counters")==SessionModes.Mode.RESET,"Maintenance entry lost explicit profile");
  check(SessionModes.bluetoothAction("native_clear_dtc")==SessionModes.Mode.CLEAR,"Bounded clearing entry lost isolation");
  check(!SessionModes.bluetoothActionAvailable("unknown",false),"Unknown action exposed Bluetooth");
  System.out.println("SESSION_MODES PASS: enforced read versus native authority, explicit MX+/Nano full native, separate collection and unknown adapter rejection");
 }
}
