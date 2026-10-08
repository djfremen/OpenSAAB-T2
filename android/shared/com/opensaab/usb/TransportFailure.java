// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.nio.charset.StandardCharsets;

/** Explain only the observed rejected single-wire wake; never infer successful CAN transmission. */
public final class TransportFailure {
    static String explain(String trace){
        int stopped=trace.lastIndexOf("Native adapter stopped: Chipsoft opcode=000F status=001D");
        if(stopped<0)return null;
        int intent=trace.lastIndexOf("CAN TX intent ",stopped);
        if(intent<0)return null;
        int end=trace.indexOf('\n',intent);if(end<0||end>stopped)return null;
        String tx=trace.substring(intent,end);
        if(!tx.contains("bus=SingleWire ")||!tx.contains("wake_control_selected=true")||!tx.contains("id=0x00000100 ")||!tx.contains("dlc=0 "))return null;
        return "Chipsoft rejected the single-wire CAN wake-up request (status 001D). The adapter connection worked, but this vehicle-network step did not complete.\n\nOn an ECM-only bench, reading the VIN over high-speed CAN can work while the original menus still require the low-speed bus. Check the bench wiring, including the single-wire CAN connection at diagnostic pin 1. No successful ECU-information or code-read result is implied by the VIN read.";
    }
    public static String from(File directory){
        if(directory==null)return null;
        try(RandomAccessFile log=new RandomAccessFile(new File(directory,"rust.log"),"r")){
            int length=(int)Math.min(65536,log.length());byte[] data=new byte[length];log.seek(log.length()-length);log.readFully(data);
            return explain(new String(data,StandardCharsets.UTF_8));
        }catch(IOException unavailable){return null;}
    }
    private TransportFailure(){}
}
