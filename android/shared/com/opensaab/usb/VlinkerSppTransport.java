// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.Manifest;
import android.bluetooth.*;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Build;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Direct Android RFCOMM SPP. Sends shared-core commands as ASCII + CR, never JSON. */
final class VlinkerSppTransport implements VlinkerTransport {
    static final UUID SPP_UUID=UUID.fromString("00001101-0000-1000-8000-00805f9b34fb");
    interface Endpoint {void connect()throws IOException;InputStream input()throws IOException;OutputStream output()throws IOException;void close()throws IOException;}
    private final Endpoint endpoint;
    private final String deviceName,deviceAddress,connectionMode;
    private final ArrayBlockingQueue<JSONObject> packets=new ArrayBlockingQueue<>(64);
    private final Object delivery=new Object();
    private volatile boolean connecting,writing,stopped,closeConfirmed;
    private volatile IOException failure;
    private volatile String connectPhase="not_started";
    private volatile String connectError="";
    private volatile long connectElapsedMs=-1;
    private int bondState=-1;
    private Boolean discoveryBeforeConnect;
    private OutputStream output;
    private final ScheduledExecutorService watchdog=Executors.newSingleThreadScheduledExecutor(r->{Thread t=new Thread(r,"vlinker-spp-deadline");t.setDaemon(true);return t;});
    VlinkerSppTransport(Context c,BluetoothDevice device,boolean compatibility)throws IOException{
        if(Build.VERSION.SDK_INT>=31&&c.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT)!=PackageManager.PERMISSION_GRANTED)throw new IOException("Allow Nearby devices to connect to the adapter");
        BluetoothManager manager=(BluetoothManager)c.getSystemService(Context.BLUETOOTH_SERVICE);BluetoothAdapter adapter=manager==null?null:manager.getAdapter();
        if(adapter==null||!adapter.isEnabled())throw new IOException("Turn on Bluetooth to connect to the adapter");
        if(device==null||!VlinkerDevicePicker.androidName(device.getName()))throw new IOException("Select a supported Bluetooth adapter");
        bondState=device.getBondState();
        if(Build.VERSION.SDK_INT<31||c.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN)==PackageManager.PERMISSION_GRANTED){discoveryBeforeConnect=adapter.isDiscovering();adapter.cancelDiscovery();}
        deviceName=device.getName();deviceAddress=device.getAddress();connectionMode=compatibility?"unauthenticated-spp":"authenticated-spp";
        // No automatic fallback: unauthenticated SPP is an explicit picker choice.
        BluetoothSocket socket=compatibility?device.createInsecureRfcommSocketToServiceRecord(SPP_UUID):device.createRfcommSocketToServiceRecord(SPP_UUID);
        endpoint=new Endpoint(){public void connect()throws IOException{socket.connect();}public InputStream input()throws IOException{return socket.getInputStream();}public OutputStream output()throws IOException{return socket.getOutputStream();}public void close()throws IOException{socket.close();}};
    }
    VlinkerSppTransport(Endpoint endpoint){this(endpoint,"");}
    VlinkerSppTransport(Endpoint endpoint,String name){this.endpoint=endpoint;deviceName=name;deviceAddress="";connectionMode="fixture";}
    private void enqueue(JSONObject value)throws IOException{if(!packets.offer(value)){failure=new IOException("Bluetooth receive queue exceeded limit");closeEndpoint();throw failure;}}
    public void open()throws Exception{
        if(stopped)throw new IOException("Bluetooth connection cancelled");connecting=true;connectPhase="connecting";
        long started=System.nanoTime();
        BluetoothConnectDeadline limit=new BluetoothConnectDeadline(watchdog,()->{connectPhase="connect_timeout";failure=new IOException("Bluetooth connect timed out");closeEndpoint();});
        try{try{endpoint.connect();}catch(IOException refused){
                connectError=refused.toString();
                // Keep the real reason: the watchdog closes the socket on timeout, which Android reports as a generic read failure.
                if(failure!=null)throw failure;
                if(stopped){connectPhase="cancelled";throw new IOException("Bluetooth connection cancelled",refused);}
                connectPhase="connect_failed";
                throw new IOException("Adapter not reachable over Bluetooth ("+refused.getMessage()+"). Check that it is plugged in with ignition ON, then retry.",refused);
            }
            limit.close();connectElapsedMs=TimeUnit.NANOSECONDS.toMillis(System.nanoTime()-started);
            if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth connection cancelled"):failure;output=endpoint.output();InputStream input=endpoint.input();
            connectPhase="connected";
            enqueue(new JSONObject().put("type","ready").put("transport","android-classic-spp").put("uuid",SPP_UUID.toString()).put("device_name",deviceName).put("device_address",deviceAddress).put("connection_mode",connectionMode));
            Thread reader=new Thread(()->{
                try{byte[] bytes=new byte[1024];int n;while(!stopped&&(n=input.read(bytes))!=-1){JSONArray rx=new JSONArray();for(int i=0;i<n;i++)rx.put(bytes[i]&255);synchronized(delivery){if(!stopped)enqueue(new JSONObject().put("type","rx").put("bytes",rx));}}if(!stopped)failure=new IOException("Bluetooth adapter disconnected; start a fresh read");}
                catch(Exception error){if(!stopped)failure=new IOException("Bluetooth receive failed",error);}
            },"vlinker-spp-reader");reader.setDaemon(true);reader.start();
        }finally{connecting=false;limit.close();if(stopped&&connectPhase.equals("connecting"))connectPhase="cancelled";if(connectElapsedMs<0)connectElapsedMs=TimeUnit.NANOSECONDS.toMillis(System.nanoTime()-started);}
    }
    JSONObject connectionEvidence()throws JSONException{return new JSONObject().put("phase",connectPhase).put("connect_elapsed_ms",connectElapsedMs).put("connect_deadline_ms",BluetoothConnectDeadline.TIMEOUT_MS).put("device_name",deviceName).put("device_address",deviceAddress).put("connection_mode",connectionMode).put("bond_state",bondState).put("discovery_before_connect",discoveryBeforeConnect==null?JSONObject.NULL:discoveryBeforeConnect).put("bluetooth_closed",closeConfirmed).put("automatic_replay",false).put("platform_error",connectError).put("failure",failure==null?"":failure.getMessage());}
    public void send(JSONObject request)throws Exception{
        if(request.optString("op").equals("close")){closeEndpoint();packets.clear();enqueue(new JSONObject().put("type","closed").put("bluetooth_closed",closeConfirmed));return;}
        boolean interrupt=request.optString("op").equals("interrupt");
        String command=request.optString("command");
        if(!interrupt && (!request.optString("op").equals("write")||command.isEmpty()||command.length()>128||!command.matches("[ -~]+")))throw new IOException("Invalid native transport write");
        if(stopped||failure!=null||output==null)throw failure==null?new IOException("Bluetooth connection is closed"):failure;
        writing=true;ScheduledFuture<?> limit=watchdog.schedule(()->{failure=new IOException("Bluetooth write timed out");closeEndpoint();},2000,TimeUnit.MILLISECONDS);
        try{synchronized(delivery){output.write((interrupt?"\r":command+"\r").getBytes(StandardCharsets.US_ASCII));output.flush();if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth write cancelled"):failure;enqueue(new JSONObject().put("type","write_completed").put("command",command));}}
        finally{writing=false;limit.cancel(false);}
    }
    public JSONObject receive(int timeoutMs)throws Exception{
        JSONObject next=packets.peek();if(failure!=null&&(next==null||!next.optString("type").equals("closed")))throw failure;JSONObject value=packets.poll(timeoutMs,TimeUnit.MILLISECONDS);if(failure!=null&&(value==null||!value.optString("type").equals("closed")))throw failure;if(value!=null)return value;if(stopped)throw new IOException("Bluetooth connection is closed");throw new java.net.SocketTimeoutException("Waiting for Bluetooth response");
    }
    private void closeEndpoint(){stopped=true;try{endpoint.close();closeConfirmed=true;}catch(IOException error){failure=new IOException("Bluetooth release was not confirmed",error);}}
    public void cancelPendingIO(){if(connecting||writing)closeEndpoint();}
    public boolean closeConfirmed(){return closeConfirmed;}
    boolean readyForHandoff(){return !stopped&&failure==null&&output!=null&&!connecting;}
    public void shutdown(){closeEndpoint();watchdog.shutdownNow();packets.clear();}
}
