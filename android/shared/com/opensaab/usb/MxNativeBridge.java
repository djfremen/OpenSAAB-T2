// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.bluetooth.BluetoothDevice;
import android.content.Context;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import org.json.JSONObject;

/** App-owned, authenticated loopback byte effects for the shared STN/CANdi worker. */
public final class MxNativeBridge implements AutoCloseable {
    private final VlinkerSppTransport transport;
    private final ServerSocket server;
    private final String token;
    private final boolean resetCounters;
    private Thread worker;
    private volatile Socket client;
    private volatile boolean closed,bluetoothClosed;
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility)throws Exception {
        this(context,device,compatibility,false);
    }
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility,boolean resetCounters)throws Exception {
        this.resetCounters=resetCounters;
        try{if(device==null || device.getName()==null || !device.getName().startsWith("OBDLink MX"))throw new IOException("Select the OBDLink MX+ for this firmware session");}
        catch(SecurityException denied){throw new IOException("Allow Nearby devices before connecting MX+",denied);}
        byte[] random=new byte[16];new SecureRandom().nextBytes(random);StringBuilder t=new StringBuilder();for(byte b:random)t.append(String.format(java.util.Locale.ROOT,"%02x",b&255));token=t.toString();
        server=new ServerSocket();server.bind(new InetSocketAddress(InetAddress.getByName("127.0.0.1"),35676));server.setSoTimeout(12000);
        VlinkerSppTransport effect=null;
        try{effect=new VlinkerSppTransport(context,device,compatibility);effect.open();effect.receive(1000);}catch(Exception e){if(effect!=null)effect.shutdown();server.close();throw e;}transport=effect;
        worker=new Thread(this::serve,"mx-native-owner");worker.start();
    }
    public String token(){return token;}
    private static String line(InputStream input)throws IOException {
        ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=input.read())!=-1&&n!=10){if(b.size()>=256)throw new IOException("MX bridge request too large");b.write(n);}if(n<0)throw new EOFException();return b.toString("US-ASCII");
    }
    private void serve(){
        try(Socket socket=server.accept()){
            client=socket;socket.setSoTimeout(5000);InputStream in=socket.getInputStream();OutputStream out=socket.getOutputStream();
            if(!line(in).equals("HELLO "+token))throw new IOException("MX bridge authentication rejected");reply(out,resetCounters?"READY obdlink-native-reset-counters":"READY obdlink-native-read");
            while(!closed){
                String request=line(in);
                if(request.equals("CLOSE")){transport.shutdown();bluetoothClosed=transport.closeConfirmed();if(!bluetoothClosed)throw new IOException("Bluetooth release unconfirmed");reply(out,"CLOSED");break;}
                if(request.equals("READ")){
                    try{JSONObject packet;do{packet=transport.receive(10);}while(packet.optString("type").equals("write_completed"));if(!packet.optString("type").equals("rx"))throw new IOException("Unexpected Bluetooth event");reply(out,packet.toString());}
                    catch(SocketTimeoutException idle){reply(out,"EMPTY");}
                }else if(request.equals("INTERRUPT")){transport.send(new JSONObject().put("op","interrupt"));reply(out,"WRITTEN");}
                else if(request.startsWith("WRITE ")){transport.send(new JSONObject().put("op","write").put("command",request.substring(6)));reply(out,"WRITTEN");}
                else throw new IOException("Unknown MX byte effect");
            }
        }catch(Exception failure){if(!closed)android.util.Log.e("OpenSaabMx","Native byte bridge ended: "+failure.getClass().getSimpleName());}
        finally{transport.shutdown();bluetoothClosed=transport.closeConfirmed();client=null;try{server.close();}catch(IOException ignored){}}
    }
    private static void reply(OutputStream out,String value)throws IOException{out.write((value+"\n").getBytes(StandardCharsets.UTF_8));out.flush();}
    public boolean bluetoothClosed(){return bluetoothClosed;}
    public void close(){closed=true;transport.shutdown();bluetoothClosed=transport.closeConfirmed();try{server.close();}catch(IOException ignored){}Socket c=client;if(c!=null)try{c.close();}catch(IOException ignored){}if(worker!=null&&Thread.currentThread()!=worker)try{worker.join(1500);}catch(InterruptedException e){Thread.currentThread().interrupt();}}
}
