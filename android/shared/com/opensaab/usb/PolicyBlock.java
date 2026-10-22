// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.File;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * A request the selected session mode refused before it reached USB.
 * Recognized from the adapter guard's existing log wording, so the native
 * components stay unchanged. Explanation only: it grants no authority; the
 * operator must explicitly restart in Full session.
 */
public final class PolicyBlock {
    private static final Pattern CHIPSOFT=Pattern.compile("command guard blocked before USB: controller=\\d+ CAN_ID=([0-9A-Fa-f]{1,8}) service=([0-9A-Fa-f]{2}|unknown) profile=([A-Za-z]+)");
    private static final String NANO_RUST="Native USB command rejected by collection profile";
    static final long TAIL_BYTES=256*1024;
    public final String adapter, service, canId;
    PolicyBlock(String adapter,String service,String canId){this.adapter=adapter;this.service=service;this.canId=canId;}

    /** One log line, or null when it is not a mode refusal. */
    public static PolicyBlock fromLine(String line){
        if(line==null)return null;
        Matcher m=CHIPSOFT.matcher(line);
        if(m.find()){
            if(!"Read".equalsIgnoreCase(m.group(3)))return null;
            String service=m.group(2).equalsIgnoreCase("unknown")?null:m.group(2).toUpperCase(Locale.ROOT);
            return new PolicyBlock("Chipsoft",service,m.group(1).toUpperCase(Locale.ROOT));
        }
        if(line.contains(NANO_RUST))return new PolicyBlock("VCX Nano",null,null);
        return null;
    }
    /** Last refusal in the bounded tail of a session log; null when none or unreadable. */
    public static PolicyBlock scan(File log){
        if(log==null||!log.isFile())return null;
        try(RandomAccessFile f=new RandomAccessFile(log,"r")){
            long start=Math.max(0,f.length()-TAIL_BYTES);byte[] b=new byte[(int)(f.length()-start)];f.seek(start);f.readFully(b);
            PolicyBlock found=null;
            for(String line:new String(b,StandardCharsets.UTF_8).split("\n")){PolicyBlock p=fromLine(line);if(p!=null)found=p;}
            return found;
        }catch(IOException|RuntimeException unreadable){return null;}
    }
    public static String serviceName(String service){
        if(service==null)return "a vehicle request";
        switch(service){
            case "27": return "security access ($27)";
            case "3B": return "a write-data request ($3B)";
            case "2E": return "a write-data request ($2E)";
            case "04": case "14": return "a clear-codes request ($"+service+")";
            case "AE": return "a device-control request ($AE)";
            case "31": return "a routine-control request ($31)";
            case "34": case "36": return "a programming request ($"+service+")";
            default: return "request $"+service;
        }
    }
    public String title(){return "This step needs a Full session";}
    public String message(){
        return "The firmware asked for "+serviceName(service)+(canId==null?"":" on "+canId)
            +". Read-only sessions block it, so it was not sent to the vehicle and the session stopped.\n\n"
            +"Restart in Full session to continue. Full session can change the vehicle: clearing, resets, adjustments and programming are allowed. Follow the original confirmations.";
    }
    public String status(){return "Read-only blocked "+serviceName(service)+" · Restart in Full session to continue";}
}
