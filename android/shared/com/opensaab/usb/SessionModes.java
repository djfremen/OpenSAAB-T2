// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Presentation of existing adapter authorities; this class grants no transport permissions. */
public final class SessionModes {
    public enum Mode {
        READ("Read-only", "Read supported diagnostic information. Code clearing, resets and programming stay blocked."),
        CLEAR("Read + clear codes", "Read diagnostics and allow supported code clearing. Clearing erases stored diagnostic information and may reset readiness monitors. Choose Clear Codes and confirm in the original firmware; selecting this mode does not clear anything."),
        SEEDS("Security-data collection", "Collect security challenges through the original firmware. Processing and import remain separate; service writes and security-key submission stay blocked."),
        RESET("Counter reset", "Allow the original T8 misfire and knock counter reset only. Follow the original confirmation."),
        FULL("Full session", "Full native control through the original firmware. Supported clearing, adjustments and service operations can change the vehicle. Follow the original confirmations; unavailable operations remain unavailable.");
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
        if("nano".equals(family))return new Capabilities("VCX Nano USB",true,Mode.FULL,"Full native control is available through the original firmware; security collection and processing retain their own workflow.");
        if("mxplus".equals(family))return new Capabilities("OBDLink MX+ Bluetooth",true,Mode.FULL,"Full native control is available on the supported HS-CAN and SW-CAN routes. Security access requires collection, processing and valid import; follow original firmware confirmations. Other buses and unavailable operations remain unavailable.");
        return new Capabilities("Bluetooth adapter",true,null,"Write access is unavailable for this adapter. Its model and supported connection are checked after connecting.");
    }
    public static String emulationProfile(Mode mode){
        if(mode==Mode.FULL)return "mxp_full_native";if(mode==Mode.SEEDS)return "mxp_seeds";if(mode==Mode.CLEAR)return "mxp_clear_dtc";if(mode==Mode.RESET)return "mxp_reset_counters";return "mxp_native_read";
    }
    public static long nativeInstructionLimit(Mode mode,long bounded){return mode==Mode.FULL?Long.MAX_VALUE:bounded;}
    public static long nativeDeadline(Mode mode,long now,long budget){return mode==Mode.FULL?Long.MAX_VALUE:now+budget;}
    public static int stopGraceSeconds(Mode mode){return mode==Mode.FULL?15:2;}
    public static Mode bluetoothAction(String action){
        if("native_seed".equals(action))return Mode.SEEDS;
        if("native_reset_counters".equals(action))return Mode.RESET;
        if("native_clear_dtc".equals(action))return Mode.CLEAR;
        return null;
    }
    public static boolean bluetoothActionAvailable(String action,boolean normalConnect){
        return normalConnect||bluetoothAction(action)!=null||"dtc_read".equals(action)||"native_ecu_info".equals(action)||"native_dtc".equals(action)||"native_engine_data".equals(action);
    }
    public static String usbFlag(String family,Mode mode,boolean privateNano){
        if(!forAdapter(family,privateNano).allows(mode) && !("nano".equals(family)&&mode==Mode.CLEAR))throw new IllegalArgumentException("Unavailable session mode");
        if("chipsoft".equals(family))return mode==Mode.READ?"native_firmware":"full_native";
        if("nano".equals(family))return mode==Mode.READ?"native_dtc":mode==Mode.CLEAR?"native_clear_dtc":"nano_full_native";
        throw new IllegalArgumentException("Adapter uses a different native binding");
    }
    /** Headline shown on the badge; kept short enough for a phone-width line. */
    public static String headline(Mode mode){
        if(mode==null)return "OFFLINE · No vehicle connection";
        switch(mode){
            case FULL: return "FULL SESSION · Vehicle changes possible";
            case CLEAR: return "CLEAR-CODES SESSION · Code clearing allowed";
            case RESET: return "COUNTER-RESET SESSION · One reset allowed";
            case SEEDS: return "SECURITY COLLECTION · Seeds only, no writes";
            default: return "READ-ONLY SESSION · Vehicle changes blocked";
        }
    }
    /** Second line: what the operator should expect in this mode. */
    public static String detail(Mode mode){
        if(mode==null)return "Emulation only. Nothing is sent to a vehicle.";
        switch(mode){
            case FULL: return "Clearing, resets, adjustments and programming can change the car. Follow the original confirmations.";
            case CLEAR: return "Reading plus supported code clearing. Other writes stay blocked.";
            case RESET: return "Only the original T8 misfire and knock counter reset is added to reading.";
            case SEEDS: return "Collects security challenges. Service writes and key submission stay blocked.";
            default: return "Codes, data and menus only. A step that needs to write will ask to restart in Full session.";
        }
    }
    /** True for every mode that can change the vehicle. Display only. */
    public static boolean writes(Mode mode){return mode==Mode.FULL||mode==Mode.CLEAR||mode==Mode.RESET;}
    private SessionModes(){}
}
