// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Presentation of existing adapter authorities; this class grants no transport permissions. */
public final class SessionModes {
    public enum Mode {
        READ("Read-only", "Read supported diagnostic information. Code clearing, resets and programming stay blocked."),
        CLEAR("Read + clear codes", "Read diagnostics and allow supported code clearing. Clearing erases stored diagnostic information and may reset readiness monitors. Choose Clear Codes and confirm in the original firmware; selecting this mode does not clear anything."),
        FULL("Read + write", "Full native control through the original firmware. Supported clearing, adjustments and service operations can change the vehicle. Follow the original confirmations; unavailable operations remain unavailable.");
        public final String label, description;
        Mode(String label,String description){this.label=label;this.description=description;}
        public String badge(){return "Session mode: "+label;}
    }
    public static final class Capabilities {
        public final String adapter, explanation;
        public final boolean read;
        public final Mode write;
        Capabilities(String adapter,boolean read,Mode write,String explanation){this.adapter=adapter;this.read=read;this.write=write;this.explanation=explanation;}
        public boolean allows(Mode mode){return mode!=null&&(mode==Mode.READ?read:mode==write);}
    }
    public static Capabilities forAdapter(String family,boolean privateNano){
        if("chipsoft".equals(family))return new Capabilities("Chipsoft USB",true,Mode.FULL,"Full native control is available. The original firmware and adapter enforce their supported operations.");
        if("mdi".equals(family))return new Capabilities("MDI USB",false,Mode.FULL,"Read-only enforcement is unavailable for MDI in this build. Connect only if you want full native control.");
        if("nano".equals(family))return new Capabilities("VCX Nano USB",true,privateNano?Mode.FULL:Mode.CLEAR,privateNano?"Full native control is available in this private build.":"Write access is limited to supported code clearing in this build. Full native control is unavailable.");
        if("mxplus".equals(family))return new Capabilities("OBDLink MX+ Bluetooth",true,Mode.CLEAR,"Write access is limited to supported code clearing. Full native control, security access and programming are unavailable. Counter reset is a separate maintenance operation.");
        return new Capabilities("Bluetooth adapter",true,null,"Write access is unavailable for this adapter. Its model and supported connection are checked after connecting.");
    }
    public static String usbFlag(String family,Mode mode,boolean privateNano){
        if(!forAdapter(family,privateNano).allows(mode))throw new IllegalArgumentException("Unavailable session mode");
        if("chipsoft".equals(family))return mode==Mode.READ?"native_firmware":"full_native";
        if("nano".equals(family))return mode==Mode.READ?"native_dtc":mode==Mode.CLEAR?"native_clear_dtc":"nano_full_native";
        throw new IllegalArgumentException("Adapter uses a different native binding");
    }
    private SessionModes(){}
}
