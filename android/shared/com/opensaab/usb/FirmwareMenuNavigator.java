// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.Locale;
import java.util.regex.*;

/** One explicit shortcut: select recognized menus, then leave task prompts to the driver. */
public final class FirmwareMenuNavigator {
    public enum Target { SECURITY("Get Security Access"), READ_DTC("Read DTC"), CLEAR_DTC("Clear DTC"), ENGINE_DATA("Engine Data");
        final String label;Target(String label){this.label=label;}
        public static Target fromShortcut(String value){
            if("native_dtc".equals(value))return READ_DTC;
            if("native_clear_dtc".equals(value))return CLEAR_DTC;
            if("native_engine_data".equals(value))return ENGINE_DATA;return null;
        }
    }
    private final Target target;
    private boolean joinCurrentMenu;
    private enum Stage { MAIN, YEAR, PLATFORM, DIAGNOSTICS, ALL, DTC, ENGINE, ENGINE_MENU, DONE }
    private static final int[] FUNCTION_KEYS={0x18,0x04,0x13,0x17,0x03,0x12,0x16,0x02,0x11,0x15};
    private final int year;
    private final String platform;
    private Stage stage=Stage.MAIN, proposed;
    private String candidate="",sent="";
    private long stableAt,stageAt=-1;
    private int moves;
    private boolean manual;
    public FirmwareMenuNavigator(VehicleIdentity vehicle,Target target){
        this(vehicle,target,false);
    }
    public FirmwareMenuNavigator(VehicleIdentity vehicle,Target target,boolean joinCurrentMenu){
        this.target=target;this.joinCurrentMenu=joinCurrentMenu;
        year=vehicle==null?0:vehicle.modelYear;
        String p=vehicle==null?"":vehicle.platform.toLowerCase(Locale.ROOT);
        platform=p.contains("9440")||p.equals("9-3 sport sedan")||p.equals("saab 9-3 sport")?"9440":"";
        if(target==null||year<2003||year>2012||platform.isEmpty())cancel();
    }
    public boolean active(){return stage!=Stage.DONE;}
    public void cancel(){manual=true;stage=Stage.DONE;}
    public String hint(){String label=target==null?"your task":target.label;
        return manual?"Return to a firmware menu and tap "+label+" again, or navigate manually.":stage==Stage.DONE?
            "Opened "+label+" · follow the firmware prompts.":"Selecting "+year+" · Saab 9-3 Sport (9440) → "+label+"… Touch a firmware control to choose manually.";}
    public Integer next(String screen,long now){
        if(stage==Stage.DONE)return null;
        if(stageAt<0){if(screen.trim().isEmpty())return null;stageAt=now;}
        if(now-stageAt>90000){cancel();return null;}
        String text=screen.trim();
        if(!candidate.equals(text)){candidate=text;stableAt=now;return null;}
        if(text.isEmpty()||text.equals(sent)||now-stableAt<50)return null;
        // The operator can return to the logo after a task or an automatic post-auth restart.
        // Advance only the explicit ENTER splash prompt, then continue through the normal menus.
        if(stage==Stage.MAIN&&text.contains("Software Version")
                &&(text.contains("Press [ENTER]")||text.contains("Press ENTER"))){
            joinCurrentMenu=false;proposed=Stage.MAIN;return 0x10;
        }
        if(joinCurrentMenu){
            joinCurrentMenu=false;
            if(text.contains("Main Menu")&&function(text,"Diagnostics")!=null)stage=Stage.MAIN;
            else if(text.contains("Model Year"))stage=Stage.YEAR;
            else if(text.contains("Vehicle Type"))stage=Stage.PLATFORM;
            else if(text.contains("Diagnostics")&&!text.contains("Checking")&&function(text,"All")!=null)stage=Stage.DIAGNOSTICS;
            else if(target!=Target.ENGINE_DATA&&text.contains("ECU Information")&&function(text,"Get Security Access")!=null)stage=Stage.ALL;
            else if((target==Target.READ_DTC||target==Target.CLEAR_DTC)&&function(text,"Read DTC")!=null&&function(text,"Clear DTC")!=null)stage=Stage.DTC;
            else if(target==Target.ENGINE_DATA&&text.contains("Customer Functions")&&last(text).equals("Engine Control"))stage=Stage.ENGINE;
            else {cancel();return null;}
        }
        proposed=stage;
        switch(stage){
            case MAIN:
                if(text.contains("Main Menu")){Integer k=function(text,"Diagnostics");if(k!=null){proposed=Stage.YEAR;return k;}}
                break;
            case YEAR:
                if(text.contains("Model Year")){
                    Matcher selected=Pattern.compile("(?:\\([A-Z0-9]\\)\\s*)?((?:19|20)\\d{2})").matcher(last(text));
                    if(selected.matches()){
                        int shown=Integer.parseInt(selected.group(1));
                        if(shown==year){proposed=Stage.PLATFORM;return 0x10;}
                        if(moves>=30){cancel();return null;}
                        return shown>year?0x0c:0x09;
                    }
                }
                break;
            case PLATFORM:
                if(text.contains("Vehicle Type")){
                    String selected=last(text).toLowerCase(Locale.ROOT);
                    if(selected.equals("saab 9-3 sport (9440)")){proposed=Stage.DIAGNOSTICS;return 0x10;}
                    if(text.contains("Saab 9-3 Sport (9440)")&&selected.equals("saab 9-5")&&moves<30)return 0x0c;
                }
                break;
            case DIAGNOSTICS:
                if(text.contains("Diagnostics")&&!text.contains("Checking")){Integer k=function(text,target==Target.ENGINE_DATA?"Engine":"All");if(k!=null){proposed=target==Target.ENGINE_DATA?Stage.ENGINE:Stage.ALL;return k;}}
                break;
            case ALL:
                if(text.contains("ECU Information")){Integer k=function(text,target==Target.SECURITY?"Get Security Access":"Diagnostic Trouble Codes (DTC)");if(k!=null){proposed=target==Target.SECURITY?Stage.DONE:Stage.DTC;return k;}}
                break;
            case DTC:
                if(text.contains("Diagnostic Trouble Codes")){Integer k=function(text,target.label);if(k!=null){proposed=Stage.DONE;return k;}}
                break;
            case ENGINE:
                if(text.contains("Customer Functions")&&last(text).equals("Engine Control")){proposed=Stage.ENGINE_MENU;return 0x10;}
                break;
            case ENGINE_MENU:
                if(text.contains("Engine Data Display")){stage=Stage.DONE;return null;}
                if(text.contains("Engine")){Integer k=function(text,"Data Display");if(k==null)k=function(text,"Engine Data");
                    if(k!=null){proposed=Stage.DONE;return k;}}
                break;
            default: break;
        }
        return null;
    }
    /** Called only after the host accepted a key for this session. Never repeat on a stale screen. */
    public void sent(long now){sent=candidate;moves++;if(stage!=proposed){stage=proposed;stageAt=now;moves=0;}}
    private static String last(String text){String[] lines=text.split("\\R");return lines[lines.length-1].trim();}
    private static Integer function(String text,String label){
        Matcher m=Pattern.compile("(?m)^\\s*F([0-9]):\\s*"+Pattern.quote(label)+"\\s*$").matcher(text);
        return m.find()?FUNCTION_KEYS[Integer.parseInt(m.group(1))]:null;
    }
}
