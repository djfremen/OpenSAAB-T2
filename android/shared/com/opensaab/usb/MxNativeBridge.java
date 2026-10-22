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
    private final SessionModes.Mode mode;
    private Thread worker;
    private volatile Socket client;
    private volatile boolean closed,bluetoothClosed;
    private final Object handoff=new Object();
    private boolean securityArmed,parked,detached;
    public static final class PreparedTransport implements AutoCloseable {
        private final NativeTransportLease<VlinkerSppTransport> lease;
        private final VlinkerSppTransport original;
        private final String attempt;
        PreparedTransport(String attempt,String address,boolean compatibility,VlinkerSppTransport link){
            this.attempt=attempt;original=link;lease=new NativeTransportLease<>(attempt,address,compatibility,link,VlinkerSppTransport::shutdown);
        }
        boolean available(){return lease.available()&&original.readyForHandoff();}
        boolean bluetoothClosed(){return original.closeConfirmed();}
        VlinkerSppTransport claim(BluetoothDevice device,boolean compatibility){return claim(device.getAddress(),compatibility);}
        VlinkerSppTransport claim(String address,boolean compatibility){return lease.claim(attempt,address,compatibility);}
        public void close(){lease.close();}
    }
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility)throws Exception {
        this(context,device,compatibility,false);
    }
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility,boolean resetCounters)throws Exception {
        this(context,device,compatibility,resetCounters,false);
    }
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility,boolean resetCounters,boolean clearDtc)throws Exception {
        this(context,device,compatibility,legacyMode(resetCounters,clearDtc));
    }
    private static SessionModes.Mode legacyMode(boolean reset,boolean clear){if(reset&&clear)throw new IllegalArgumentException("Choose one MX+ profile");return reset?SessionModes.Mode.RESET:clear?SessionModes.Mode.CLEAR:SessionModes.Mode.READ;}
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility,SessionModes.Mode mode)throws Exception {
        this(context,device,compatibility,mode,null);
    }
    public MxNativeBridge(Context context,BluetoothDevice device,boolean compatibility,SessionModes.Mode mode,PreparedTransport prepared)throws Exception {
        if(mode==null)throw new IllegalArgumentException("Choose one MX+ profile");this.mode=mode;
        try{if(device==null || device.getName()==null || !device.getName().startsWith("OBDLink MX"))throw new IOException("Select the OBDLink MX+ for this firmware session");}
        catch(SecurityException denied){throw new IOException("Allow Nearby devices before connecting MX+",denied);}
        byte[] random=new byte[16];new SecureRandom().nextBytes(random);StringBuilder t=new StringBuilder();for(byte b:random)t.append(String.format(java.util.Locale.ROOT,"%02x",b&255));token=t.toString();
        server=new ServerSocket();
        try{server.bind(new InetSocketAddress(InetAddress.getByName("127.0.0.1"),35676));server.setSoTimeout(12000);}
        catch(Exception failure){server.close();if(prepared!=null)prepared.close();throw failure;}
        VlinkerSppTransport effect=null;
        try{
            if(prepared!=null)effect=prepared.claim(device,compatibility);
            else{effect=new VlinkerSppTransport(context,device,compatibility);effect.open();effect.receive(1000);}
        }catch(Exception e){if(effect!=null)effect.shutdown();if(prepared!=null)prepared.close();server.close();throw e;}transport=effect;
        worker=new Thread(this::serve,"mx-native-owner");worker.start();
    }
    /** Emulator-only fixture uses the production authenticated ownership protocol. */
    MxNativeBridge(Context context,VlinkerSppTransport synthetic,SessionModes.Mode mode)throws Exception{
        if(!SimulatorVehicleConnection.available(context))throw new IllegalArgumentException("Synthetic bridge requires a Debug emulator");
        if(mode==null||synthetic==null||!synthetic.readyForHandoff())throw new IOException("Synthetic transport is not ready");
        this.mode=mode;transport=synthetic;byte[] random=new byte[16];new SecureRandom().nextBytes(random);
        StringBuilder t=new StringBuilder();for(byte b:random)t.append(String.format(java.util.Locale.ROOT,"%02x",b&255));token=t.toString();
        server=new ServerSocket();try{server.bind(new InetSocketAddress(InetAddress.getByName("127.0.0.1"),35676));server.setSoTimeout(12000);}
        catch(Exception failure){server.close();synthetic.shutdown();throw failure;}
        worker=new Thread(this::serve,"mx-native-owner-fixture");worker.start();
    }
    public JSONObject connectionEvidence()throws org.json.JSONException{return transport.connectionEvidence();}
    public String token(){return token;}
    public void armSecurityHandoff(){synchronized(handoff){if(closed||parked||detached)throw new IllegalStateException("MX+ connection is already ending");securityArmed=true;}}
    public void cancelSecurityHandoff(){boolean release;synchronized(handoff){securityArmed=false;release=parked&&!detached;}if(release)close();}
    public MxSecurityTransport takeSecurityTransport(Context context,MxSecurityTransport.Binding binding,
            boolean processDead,boolean cleanExit,boolean protocolClean)throws Exception{
        synchronized(handoff){if(!securityArmed||!parked||closed||detached||!processDead||!cleanExit||!protocolClean)
            throw new IOException("MX+ transport was not retained after clean native shutdown");}
        if(worker!=null)worker.join(1500);
        synchronized(handoff){if(worker!=null&&worker.isAlive()||closed||!securityArmed||!transport.readyForHandoff())
            throw new IOException("MX+ byte worker has not finished its handoff");detached=true;closed=true;}
        try{return new MxSecurityTransport(context,binding,transport,processDead,cleanExit,protocolClean);}
        catch(Exception error){transport.shutdown();throw error;}
    }
    private static String line(InputStream input)throws IOException {
        ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=input.read())!=-1&&n!=10){if(b.size()>=256)throw new IOException("MX bridge request too large");b.write(n);}if(n<0)throw new EOFException();return b.toString("US-ASCII");
    }
    private void serve(){
        try(Socket socket=server.accept()){
            client=socket;socket.setSoTimeout(5000);InputStream in=socket.getInputStream();OutputStream out=socket.getOutputStream();
            if(!line(in).equals("HELLO "+token))throw new IOException("MX bridge authentication rejected");reply(out,mode==SessionModes.Mode.FULL?"READY obdlink-native-full-native":mode==SessionModes.Mode.SEEDS?"READY obdlink-native-seeds":mode==SessionModes.Mode.CLEAR?"READY obdlink-native-clear-dtc":mode==SessionModes.Mode.RESET?"READY obdlink-native-reset-counters":"READY obdlink-native-read");
            while(!closed){
                String request=line(in);
                if(request.equals("FINISH CLEAN")){
                    boolean retain;synchronized(handoff){retain=securityArmed&&!closed;parked=retain;}
                    if(retain){drainParkReceipts();if(!transport.readyForHandoff())throw new IOException("MX+ transport was not quiet after cleanup");reply(out,"PARKED");break;}
                    transport.shutdown();bluetoothClosed=transport.closeConfirmed();if(!bluetoothClosed)throw new IOException("Bluetooth release unconfirmed");reply(out,"CLOSED");break;
                }
                if(request.equals("CLOSE")){transport.shutdown();bluetoothClosed=transport.closeConfirmed();if(!bluetoothClosed)throw new IOException("Bluetooth release unconfirmed");reply(out,"CLOSED");break;}
                if(request.equals("READ")){
                    try{JSONObject packet;do{packet=transport.receive(10);}while(packet.optString("type").equals("write_completed"));if(!packet.optString("type").equals("rx"))throw new IOException("Unexpected Bluetooth event");reply(out,packet.toString());}
                    catch(SocketTimeoutException idle){reply(out,"EMPTY");}
                }else if(request.equals("INTERRUPT")){transport.send(new JSONObject().put("op","interrupt"));reply(out,"WRITTEN");}
                else if(request.startsWith("WRITE ")){transport.send(new JSONObject().put("op","write").put("command",request.substring(6)));reply(out,"WRITTEN");}
                else throw new IOException("Unknown MX byte effect");
            }
        }catch(Exception failure){synchronized(handoff){parked=false;}if(!closed)android.util.Log.e("OpenSaabMx","Native byte bridge ended: "+failure.getClass().getSimpleName());}
        finally{boolean retain;synchronized(handoff){retain=parked&&securityArmed&&!closed;}if(!retain)transport.shutdown();bluetoothClosed=transport.closeConfirmed();client=null;try{server.close();}catch(IOException ignored){}}
    }
    private void drainParkReceipts()throws Exception{
        boolean receipt=false;
        while(true)try{
            JSONObject packet=transport.receive(1);
            if(receipt||!packet.optString("type").equals("write_completed")||!packet.optString("command").equals("STPC"))
                throw new IOException("Unexpected bytes after native cleanup; connection not retained");
            receipt=true;
        }catch(SocketTimeoutException quiet){return;}
    }
    private static void reply(OutputStream out,String value)throws IOException{out.write((value+"\n").getBytes(StandardCharsets.UTF_8));out.flush();}
    public boolean bluetoothClosed(){return bluetoothClosed;}
    public void close(){boolean release;synchronized(handoff){closed=true;securityArmed=false;release=!detached;}if(release)transport.shutdown();bluetoothClosed=transport.closeConfirmed();try{server.close();}catch(IOException ignored){}Socket c=client;if(c!=null)try{c.close();}catch(IOException ignored){}if(worker!=null&&Thread.currentThread()!=worker)try{worker.join(1500);}catch(InterruptedException e){Thread.currentThread().interrupt();}}
}
