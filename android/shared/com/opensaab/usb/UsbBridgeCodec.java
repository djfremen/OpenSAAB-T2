// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;

/** Shared bounded socket framing only; no adapter-specific USB commands. */
public final class UsbBridgeCodec {
    public static String hex(byte[] b,int size) {
        // USB capture calls this for every packet: avoid a Formatter and its
        // temporary allocations for each individual byte.
        final String digits="0123456789ABCDEF";
        char[] text=new char[size*2];
        for(int i=0;i<size;i++){int v=b[i]&255;text[i*2]=digits.charAt(v>>>4);text[i*2+1]=digits.charAt(v&15);}
        return new String(text);
    }
    public static byte[] unhex(String s) throws IOException {
        if ((s.length()&1)!=0 || s.length()>256 || !s.matches("[0-9A-F]+")) throw new IOException("Invalid TX encoding");
        byte[] b=new byte[s.length()/2];for(int i=0;i<b.length;i++)b[i]=(byte)Integer.parseInt(s.substring(i*2,i*2+2),16);return b;
    }
    public static String readLine(InputStream in) throws IOException {
        ByteArrayOutputStream b=new ByteArrayOutputStream(); int c;
        while((c=in.read())!=-1) { if(c=='\n')return b.toString("US-ASCII"); if(c<32 || c>126 || b.size()>=260)throw new IOException("Invalid command line");b.write(c); }
        throw new EOFException("Controller disconnected");
    }
    private UsbBridgeCodec(){}
}
