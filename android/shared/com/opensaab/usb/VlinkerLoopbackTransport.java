// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

/** Debug simulator's M4 transport; never selected automatically on a physical phone. */
final class VlinkerLoopbackTransport implements VlinkerTransport {
    private final String host;private final int port;
    private volatile Socket socket;private InputStream input;private OutputStream output;
    private final ByteArrayOutputStream line=new ByteArrayOutputStream();
    VlinkerLoopbackTransport(String host,int port){this.host=host;this.port=port;}
    public void open()throws Exception{
        socket=new Socket();socket.connect(new InetSocketAddress(host,port),2000);
        input=socket.getInputStream();output=socket.getOutputStream();send(new JSONObject().put("op","open").put("schema",1));
    }
    public void send(JSONObject value)throws Exception{output.write((value.toString()+"\n").getBytes(StandardCharsets.UTF_8));output.flush();}
    public JSONObject receive(int timeoutMs)throws Exception{
        socket.setSoTimeout(timeoutMs);
        while(true){int b=input.read();if(b<0)throw new EOFException("Mac Bluetooth helper closed before confirmation");if(b==10){JSONObject packet=new JSONObject(line.toString("UTF-8"));line.reset();return packet;}if(line.size()>=131072)throw new IOException("Transport packet too large");line.write(b);}
    }
    public void cancelPendingIO(){if(socket!=null&&!socket.isConnected())shutdown();}
    public void shutdown(){try{if(socket!=null)socket.close();}catch(IOException ignored){}socket=null;}
}
