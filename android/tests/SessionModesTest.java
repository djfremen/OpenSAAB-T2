// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class SessionModesTest {
 private static void check(boolean ok,String reason){if(!ok)throw new AssertionError(reason);}
 public static void main(String[] args){
  check(SessionModes.forAdapter("chipsoft",false).allows(SessionModes.Mode.READ),"Chipsoft read missing");
  check(SessionModes.forAdapter("chipsoft",false).allows(SessionModes.Mode.FULL),"Chipsoft full native missing");
  check(!SessionModes.forAdapter("mdi",false).allows(SessionModes.Mode.READ),"MDI falsely claims read enforcement");
  check(!SessionModes.forAdapter("mxplus",false).allows(SessionModes.Mode.FULL),"MX+ falsely claims full write");
  check(SessionModes.forAdapter("mxplus",false).allows(SessionModes.Mode.CLEAR),"Existing MX+ clear choice missing");
  check(!SessionModes.forAdapter("nano",false).allows(SessionModes.Mode.FULL),"Release Nano exposes private authority");
  check(SessionModes.forAdapter("nano",false).allows(SessionModes.Mode.CLEAR),"Release Nano clear missing");
  check(SessionModes.forAdapter("nano",true).allows(SessionModes.Mode.FULL),"Private Nano lost existing mode");
  check(!SessionModes.forAdapter("unverified",true).allows(SessionModes.Mode.CLEAR),"Unknown adapter inherits writes");
  check(!SessionModes.forAdapter("chipsoft",true).allows(null),"Missing choice selected a mode");
  check(SessionModes.usbFlag("chipsoft",SessionModes.Mode.READ,false).equals("native_firmware"),"Read-only Chipsoft selected a write worker");
  check(SessionModes.usbFlag("chipsoft",SessionModes.Mode.FULL,false).equals("full_native"),"Chipsoft choice lost native mode");
  check(SessionModes.usbFlag("nano",SessionModes.Mode.READ,false).equals("native_dtc"),"Nano read mode changed authority");
  check(SessionModes.usbFlag("nano",SessionModes.Mode.CLEAR,false).equals("native_clear_dtc"),"Nano clear inherited full control");
  boolean rejected=false;try{SessionModes.usbFlag("nano",SessionModes.Mode.FULL,false);}catch(IllegalArgumentException expected){rejected=true;}check(rejected,"Release Nano route bypassed private-only gate");
  System.out.println("SESSION_MODES PASS: enforced read versus native authority, MX+ bounded clear, release/private Nano isolation and unknown adapter rejection");
 }
}
