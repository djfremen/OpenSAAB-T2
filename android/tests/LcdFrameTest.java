// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.nio.charset.StandardCharsets;
import java.util.Arrays;

public final class LcdFrameTest {
    static void rejected(byte[] bytes) throws Exception {
        try { LcdFrame.decode(bytes); throw new AssertionError("Malformed frame accepted"); }
        catch (java.io.IOException expected) { }
    }
    public static void main(String[] args) throws Exception {
        byte[] header="P6\n320 240\n255\n".getBytes(StandardCharsets.US_ASCII);
        byte[] bytes=new byte[header.length+320*240*3];
        System.arraycopy(header,0,bytes,0,header.length);
        bytes[header.length]=10; // whitespace-valued first pixel must not be skipped
        bytes[header.length+1]=32;
        bytes[header.length+2]=(byte)255;
        bytes[bytes.length-1]=(byte)128;
        int[] pixels=LcdFrame.decode(bytes);
        if(pixels.length!=76800 || pixels[0]!=0xff0a20ff || pixels[76799]!=0xff000080)
            throw new AssertionError("Pixel colors or dimensions changed");
        rejected(Arrays.copyOf(bytes,bytes.length-1));
        rejected(Arrays.copyOf(bytes,bytes.length+1));
        rejected(new byte[0]);
        bytes[0]='X';rejected(bytes);
        System.out.println("LcdFrame tests passed: exact pixels, truncation, extra data, malformed header");
    }
}
