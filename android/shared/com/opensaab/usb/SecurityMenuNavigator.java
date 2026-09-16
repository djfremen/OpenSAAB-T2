// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.Locale;
import java.util.regex.*;

/** One collection run: select known menu labels, then leave physical key prompts to the driver. */
public final class SecurityMenuNavigator {
    private enum Stage { MAIN, YEAR, PLATFORM, DIAGNOSTICS, ALL, DONE }
    private static final int[] FUNCTION_KEYS={0x18,0x04,0x13,0x17,0x03,0x12,0x16,0x02,0x11,0x15};
    private final int year;
    private final String platform;
    private Stage stage=Stage.MAIN, proposed;
    private String candidate="",sent="";
    private long stableAt,stageAt=-1;
    private int moves;
    private boolean manual;
    public SecurityMenuNavigator(VehicleIdentity vehicle){
        year=vehicle==null?0:vehicle.modelYear;
        String p=vehicle==null?"":vehicle.platform.toLowerCase(Locale.ROOT);
        platform=p.contains("9440")||p.equals("9-3 sport sedan")||p.equals("saab 9-3 sport")?"9440":"";
        if(year<2003||year>2012||platform.isEmpty())cancel();
    }
    public void cancel(){manual=true;stage=Stage.DONE;}
    public String hint(){return manual?"Choose the vehicle and Get Security Access in the firmware menus.":stage==Stage.DONE?
        "Follow the firmware's ignition-key prompts.":"Selecting "+year+" · Saab 9-3 Sport (9440) → All → Get Security Access… Touch a firmware control to choose manually.";}
    public Integer next(String screen,long now){
        if(stage==Stage.DONE)return null;
        if(stageAt<0){if(screen.trim().isEmpty())return null;stageAt=now;}
        if(now-stageAt>90000){cancel();return null;}
        String text=screen.trim();
        if(!candidate.equals(text)){candidate=text;stableAt=now;return null;}
        if(text.isEmpty()||text.equals(sent)||now-stableAt<500)return null;
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
                if(text.contains("Diagnostics")&&!text.contains("Checking")){Integer k=function(text,"All");if(k!=null){proposed=Stage.ALL;return k;}}
                break;
            case ALL:
                if(text.contains("ECU Information")){Integer k=function(text,"Get Security Access");if(k!=null){proposed=Stage.DONE;return k;}}
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
