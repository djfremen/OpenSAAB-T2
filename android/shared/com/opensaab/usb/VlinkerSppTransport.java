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
    private OutputStream output;
    private final ScheduledExecutorService watchdog=Executors.newSingleThreadScheduledExecutor(r->{Thread t=new Thread(r,"vlinker-spp-deadline");t.setDaemon(true);return t;});
    VlinkerSppTransport(Context c,BluetoothDevice device,boolean compatibility)throws IOException{
        if(Build.VERSION.SDK_INT>=31&&c.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT)!=PackageManager.PERMISSION_GRANTED)throw new IOException("Allow Nearby devices to connect to the adapter");
        BluetoothManager manager=(BluetoothManager)c.getSystemService(Context.BLUETOOTH_SERVICE);BluetoothAdapter adapter=manager==null?null:manager.getAdapter();
        if(adapter==null||!adapter.isEnabled())throw new IOException("Turn on Bluetooth to connect to the adapter");
        if(device==null||!VlinkerDevicePicker.androidName(device.getName()))throw new IOException("Select a supported Bluetooth adapter");
        if(Build.VERSION.SDK_INT<31||c.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN)==PackageManager.PERMISSION_GRANTED)adapter.cancelDiscovery();
        deviceName=device.getName();deviceAddress=device.getAddress();connectionMode=compatibility?"unauthenticated-spp":"authenticated-spp";
        // No automatic fallback: unauthenticated SPP is an explicit picker choice.
        BluetoothSocket socket=compatibility?device.createInsecureRfcommSocketToServiceRecord(SPP_UUID):device.createRfcommSocketToServiceRecord(SPP_UUID);
        endpoint=new Endpoint(){public void connect()throws IOException{socket.connect();}public InputStream input()throws IOException{return socket.getInputStream();}public OutputStream output()throws IOException{return socket.getOutputStream();}public void close()throws IOException{socket.close();}};
    }
    VlinkerSppTransport(Endpoint endpoint){this.endpoint=endpoint;deviceName="";deviceAddress="";connectionMode="fixture";}
    private void enqueue(JSONObject value)throws IOException{if(!packets.offer(value)){failure=new IOException("Bluetooth receive queue exceeded limit");closeEndpoint();throw failure;}}
    public void open()throws Exception{
        if(stopped)throw new IOException("Bluetooth connection cancelled");connecting=true;
        ScheduledFuture<?> limit=watchdog.schedule(()->{failure=new IOException("Bluetooth connect timed out");closeEndpoint();},8000,TimeUnit.MILLISECONDS);
        try{endpoint.connect();if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth connection cancelled"):failure;output=endpoint.output();InputStream input=endpoint.input();
            enqueue(new JSONObject().put("type","ready").put("transport","android-classic-spp").put("uuid",SPP_UUID.toString()).put("device_name",deviceName).put("device_address",deviceAddress).put("connection_mode",connectionMode));
            Thread reader=new Thread(()->{
                try{byte[] bytes=new byte[1024];int n;while(!stopped&&(n=input.read(bytes))!=-1){JSONArray rx=new JSONArray();for(int i=0;i<n;i++)rx.put(bytes[i]&255);synchronized(delivery){if(!stopped)enqueue(new JSONObject().put("type","rx").put("bytes",rx));}}if(!stopped)failure=new IOException("Bluetooth adapter disconnected; start a fresh read");}
                catch(Exception error){if(!stopped)failure=new IOException("Bluetooth receive failed",error);}
            },"vlinker-spp-reader");reader.setDaemon(true);reader.start();
        }finally{connecting=false;limit.cancel(false);}
    }
    public void send(JSONObject request)throws Exception{
        if(request.optString("op").equals("close")){closeEndpoint();packets.clear();enqueue(new JSONObject().put("type","closed").put("bluetooth_closed",closeConfirmed));return;}
        String command=request.optString("command");
        if(!request.optString("op").equals("write")||command.isEmpty()||command.length()>128||!command.matches("[ -~]+"))throw new IOException("Invalid native transport write");
        if(stopped||failure!=null||output==null)throw failure==null?new IOException("Bluetooth connection is closed"):failure;
        writing=true;ScheduledFuture<?> limit=watchdog.schedule(()->{failure=new IOException("Bluetooth write timed out");closeEndpoint();},2000,TimeUnit.MILLISECONDS);
        try{synchronized(delivery){output.write((command+"\r").getBytes(StandardCharsets.US_ASCII));output.flush();if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth write cancelled"):failure;enqueue(new JSONObject().put("type","write_completed").put("command",command));}}
        finally{writing=false;limit.cancel(false);}
    }
    public JSONObject receive(int timeoutMs)throws Exception{
        JSONObject value=packets.poll(timeoutMs,TimeUnit.MILLISECONDS);if(value!=null)return value;if(failure!=null)throw failure;if(stopped)throw new IOException("Bluetooth connection is closed");throw new java.net.SocketTimeoutException("Waiting for Bluetooth response");
    }
    private void closeEndpoint(){stopped=true;try{endpoint.close();closeConfirmed=true;}catch(IOException error){failure=new IOException("Bluetooth release was not confirmed",error);}}
    public void cancelPendingIO(){if(connecting||writing)closeEndpoint();}
    public void shutdown(){closeEndpoint();watchdog.shutdownNow();packets.clear();}
}
