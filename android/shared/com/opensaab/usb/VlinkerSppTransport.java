// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.Manifest;
import android.bluetooth.*;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.SystemClock;
import android.provider.Settings;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Direct Android RFCOMM SPP. Sends shared-core commands as ASCII + CR, never JSON. */
final class VlinkerSppTransport implements VlinkerTransport,SppTransportAdmission.Owner {
    static final UUID SPP_UUID=UUID.fromString("00001101-0000-1000-8000-00805f9b34fb");
    interface Endpoint extends SppConnectRecovery.Endpoint {}
    private final Endpoint endpoint;
    private SppConnectRecovery recovery;
    private static final SppTransportAdmission PHYSICAL_ADMISSION=new SppTransportAdmission();
    private static final SppReleaseTiming.Ledger RELEASE_TIMING=new SppReleaseTiming.Ledger();
    private final boolean physical;
    private final SppReleaseTiming.Attempt timing;
    private final String deviceName,deviceAddress,connectionMode;
    private final ArrayBlockingQueue<JSONObject> packets=new ArrayBlockingQueue<>(64);
    private final Object delivery=new Object();
    private final Object ownership=new Object();
    static final long READER_STOP_TIMEOUT_MS=1500;
    private volatile boolean connecting,writing,stopped;
    private volatile IOException failure;
    private volatile String connectPhase="not_started";
    private volatile String connectError="";
    private volatile long connectElapsedMs=-1;
    private final java.util.concurrent.atomic.AtomicLong writeAttempts=new java.util.concurrent.atomic.AtomicLong();
    private int bondState=-1;
    private Boolean discoveryBeforeConnect;
    private volatile OutputStream output;
    private InputStream input;
    private Thread reader;
    private boolean endpointCloseStarted,endpointCloseFinished;
    private int closingResources;
    private IOException closeFailure;
    private final ScheduledExecutorService watchdog=Executors.newSingleThreadScheduledExecutor(r->{Thread t=new Thread(r,"vlinker-spp-deadline");t.setDaemon(true);return t;});
    VlinkerSppTransport(Context c,BluetoothDevice device,boolean compatibility)throws IOException{
        physical=true;
        if(Build.VERSION.SDK_INT>=31&&c.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT)!=PackageManager.PERMISSION_GRANTED)throw new IOException("Allow Nearby devices to connect to the adapter");
        BluetoothManager manager=(BluetoothManager)c.getSystemService(Context.BLUETOOTH_SERVICE);BluetoothAdapter adapter=manager==null?null:manager.getAdapter();
        if(adapter==null||!adapter.isEnabled())throw new IOException("Turn on Bluetooth to connect to the adapter");
        if(device==null||!VlinkerDevicePicker.androidName(device.getName()))throw new IOException("Select a supported Bluetooth adapter");
        bondState=device.getBondState();
        if(Build.VERSION.SDK_INT<31||c.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN)==PackageManager.PERMISSION_GRANTED){discoveryBeforeConnect=adapter.isDiscovering();adapter.cancelDiscovery();}
        deviceName=device.getName();deviceAddress=device.getAddress();connectionMode=compatibility?"unauthenticated-spp":"authenticated-spp";
        String bootId=null;try{String count=Settings.Global.getString(c.getContentResolver(),"boot_count");if(count!=null&&count.matches("[0-9]+"))bootId="boot_count:"+count;}catch(RuntimeException unavailable){}
        timing=RELEASE_TIMING.attempt(deviceAddress,bootId,SystemClock::elapsedRealtime);
        // No automatic fallback: unauthenticated SPP is an explicit picker choice.
        PHYSICAL_ADMISSION.reserve(this);
        Endpoint initial;
        try{initial=socketEndpoint(device,compatibility);}
        catch(IOException|RuntimeException error){PHYSICAL_ADMISSION.constructionFailed(this);throw error;}
        // Scope the measured recovery to the tested bonded MX+ authenticated path.
        // The existing 20s watchdog bounds both opens, not 20s per attempt.
        if(!compatibility&&bondState==BluetoothDevice.BOND_BONDED&&deviceName.startsWith("OBDLink MX+")){
            recovery=new SppConnectRecovery(initial,()->socketEndpoint(device,false),
                ()->adapter.isEnabled()&&device.getBondState()==BluetoothDevice.BOND_BONDED,
                SystemClock::elapsedRealtime);
            endpoint=recoveryEndpoint(recovery);
        }else endpoint=initial;
        PHYSICAL_ADMISSION.constructed(this);
    }
    private static Endpoint socketEndpoint(BluetoothDevice device,boolean compatibility)throws IOException{
        BluetoothSocket socket=compatibility?device.createInsecureRfcommSocketToServiceRecord(SPP_UUID):device.createRfcommSocketToServiceRecord(SPP_UUID);
        return new Endpoint(){public void connect()throws IOException{socket.connect();}public InputStream input()throws IOException{return socket.getInputStream();}public OutputStream output()throws IOException{return socket.getOutputStream();}public void close()throws IOException{socket.close();}};
    }
    static Endpoint recoveryEndpoint(SppConnectRecovery value){return new Endpoint(){
        public void connect()throws IOException{value.connect();}public InputStream input()throws IOException{return value.input();}
        public OutputStream output()throws IOException{return value.output();}public void close()throws IOException{value.close();}
    };}
    VlinkerSppTransport(SppConnectRecovery value){this(recoveryEndpoint(value));recovery=value;}
    VlinkerSppTransport(Endpoint endpoint){this(endpoint,"");}
    VlinkerSppTransport(Endpoint endpoint,String name){this(endpoint,name,new SppReleaseTiming.Ledger().attempt("fixture","fixture-boot",SystemClock::elapsedRealtime));}
    VlinkerSppTransport(Endpoint endpoint,String name,SppReleaseTiming.Attempt timing){this.endpoint=endpoint;this.timing=timing;physical=false;deviceName=name;deviceAddress="";connectionMode="fixture";}
    private void enqueue(JSONObject value)throws IOException{if(!packets.offer(value)){failure=new IOException("Bluetooth receive queue exceeded limit");throw failure;}}
    public void open()throws Exception{
        if(physical)PHYSICAL_ADMISSION.requireOwner(this);
        synchronized(ownership){if(stopped)throw new IOException("Bluetooth connection cancelled");connecting=true;connectPhase="connecting";}
        long started=System.nanoTime();
        BluetoothConnectDeadline limit=new BluetoothConnectDeadline(watchdog,()->{connectPhase="connect_timeout";failure=new IOException("Bluetooth connect timed out");closeEndpoint();});
        try{try{timing.connectStarted();endpoint.connect();}catch(IOException refused){
                connectError=refused.toString();
                // Keep the real reason: the watchdog closes the socket on timeout, which Android reports as a generic read failure.
                if(failure!=null)throw failure;
                if(stopped){connectPhase="cancelled";throw new IOException("Bluetooth connection cancelled",refused);}
                connectPhase="connect_failed";
                boolean retried=recovery!=null&&recovery.attempts().size()>1;
                failure=new IOException("Bluetooth connection was not established"+(retried?" after two socket attempts":"")+". No adapter or vehicle requests were sent. Check adapter power and pairing, then retry.",refused);
                throw failure;
            }
            limit.close();connectElapsedMs=TimeUnit.NANOSECONDS.toMillis(System.nanoTime()-started);
            if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth connection cancelled"):failure;
            OutputStream openedOutput=endpoint.output();synchronized(ownership){output=openedOutput;}
            if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth connection cancelled"):failure;
            InputStream openedInput=endpoint.input();synchronized(ownership){input=openedInput;}
            if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth connection cancelled"):failure;
            connectPhase="connected";
            enqueue(new JSONObject().put("type","ready").put("transport","android-classic-spp").put("uuid",SPP_UUID.toString()).put("device_name",deviceName).put("device_address",deviceAddress).put("connection_mode",connectionMode));
            Thread openedReader=new Thread(()->{
                try{byte[] bytes=new byte[1024];int n;while(!stopped&&(n=openedInput.read(bytes))!=-1){JSONArray rx=new JSONArray();for(int i=0;i<n;i++)rx.put(bytes[i]&255);synchronized(delivery){if(!stopped)enqueue(new JSONObject().put("type","rx").put("bytes",rx));}}if(!stopped)failure=new IOException("Bluetooth adapter disconnected; start a fresh read");}
                catch(Exception error){if(!stopped&&failure==null)failure=new IOException("Bluetooth receive failed",error);}
                finally{if(failure!=null)closeEndpoint();}
            },"vlinker-spp-reader");openedReader.setDaemon(true);
            synchronized(ownership){if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth connection cancelled"):failure;reader=openedReader;reader.start();}
        }catch(Exception error){if(failure==null)failure=error instanceof IOException?(IOException)error:new IOException("Bluetooth open failed",error);stopped=true;throw failure;}
        finally{timing.connectFinished();connecting=false;limit.close();if(stopped&&connectPhase.equals("connecting"))connectPhase="cancelled";if(connectElapsedMs<0)connectElapsedMs=TimeUnit.NANOSECONDS.toMillis(System.nanoTime()-started);if(stopped)closeEndpoint();}
    }
    private JSONObject baseConnectionEvidence()throws JSONException{return new JSONObject().put("phase",connectPhase).put("connect_elapsed_ms",connectElapsedMs).put("connect_deadline_ms",BluetoothConnectDeadline.TIMEOUT_MS).put("device_name",deviceName).put("device_address",deviceAddress).put("connection_mode",connectionMode).put("bond_state",bondState).put("discovery_before_connect",discoveryBeforeConnect==null?JSONObject.NULL:discoveryBeforeConnect).put("bluetooth_closed",closeConfirmed()).put("automatic_replay",false).put("platform_error",connectError).put("transport_write_attempts",writeAttempts.get()).put("failed_before_commands",connectPhase.equals("connect_failed")||connectPhase.equals("connect_timeout")).put("radio_sleep_state","unverified").put("hci_failure_status",JSONObject.NULL).put("failure",failure==null?"":failure.getMessage()).put("release_failure",releaseFailure());}
    JSONObject connectionEvidence()throws JSONException{
        JSONObject value=baseConnectionEvidence();for(Map.Entry<String,Object> field:timing.snapshot().entrySet())value.put(field.getKey(),field.getValue()==null?JSONObject.NULL:field.getValue());
        JSONArray attempts=new JSONArray();
        if(recovery!=null)for(SppConnectRecovery.Attempt a:recovery.attempts())attempts.put(new JSONObject().put("ordinal",a.ordinal).put("started_elapsed_ms",a.started).put("finished_elapsed_ms",a.finished).put("elapsed_ms",a.finished-a.started).put("outcome",a.outcome).put("platform_error",a.error).put("socket_close_confirmed",a.closeConfirmed()));
        return value.put("socket_attempts",attempts).put("connect_retry_count",Math.max(0,attempts.length()-1)).put("connect_recovery_scope","pre_stream_same_bonded_MXplus_authenticated_socket_only");
    }
    public void send(JSONObject request)throws Exception{
        if(request.optString("op").equals("close")){closeEndpoint();packets.clear();enqueue(new JSONObject().put("type","closed").put("bluetooth_closed",closeConfirmed()));return;}
        boolean interrupt=request.optString("op").equals("interrupt");
        String command=request.optString("command");
        if(!interrupt && (!request.optString("op").equals("write")||command.isEmpty()||command.length()>128||!command.matches("[ -~]+")))throw new IOException("Invalid native transport write");
        OutputStream target;ScheduledFuture<?> limit;
        synchronized(ownership){if(stopped||failure!=null||output==null)throw failure==null?new IOException("Bluetooth connection is closed"):failure;target=output;writing=true;limit=watchdog.schedule(()->{failure=new IOException("Bluetooth write timed out");closeEndpoint();},2000,TimeUnit.MILLISECONDS);}
        try{synchronized(delivery){if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth write cancelled"):failure;timing.writeStarted(command,interrupt);writeAttempts.incrementAndGet();target.write((interrupt?"\r":command+"\r").getBytes(StandardCharsets.US_ASCII));target.flush();if(stopped||failure!=null)throw failure==null?new IOException("Bluetooth write cancelled"):failure;timing.writeCompleted();enqueue(new JSONObject().put("type","write_completed").put("command",command));}}
        catch(Exception error){if(failure==null)failure=error instanceof IOException?(IOException)error:new IOException("Bluetooth write failed",error);closeEndpoint();throw failure;}
        finally{writing=false;limit.cancel(false);closeConfirmed();}
    }
    public JSONObject receive(int timeoutMs)throws Exception{
        JSONObject next=packets.peek();if(failure!=null&&(next==null||!next.optString("type").equals("closed")))throw failure;JSONObject value=packets.poll(timeoutMs,TimeUnit.MILLISECONDS);if(failure!=null&&(value==null||!value.optString("type").equals("closed")))throw failure;if(value!=null)return value;if(stopped)throw new IOException("Bluetooth connection is closed");throw new java.net.SocketTimeoutException("Waiting for Bluetooth response");
    }
    private void closeFailure(String resource,Exception error){synchronized(ownership){IOException detail=new IOException("Bluetooth "+resource+" release was not confirmed",error);if(closeFailure==null)closeFailure=detail;else closeFailure.addSuppressed(detail);if(failure==null)failure=new IOException("Bluetooth release was not confirmed",detail);else if(failure!=detail)failure.addSuppressed(detail);}}
    private void closeStream(Closeable stream,String name){if(stream!=null)try{stream.close();}catch(Exception error){closeFailure(name,error);}}
    private void closeEndpoint(){
        OutputStream ownedOutput;InputStream ownedInput;boolean closeSocket;Thread ownedReader;
        synchronized(ownership){
            stopped=true;timing.releaseStarted();closeSocket=!endpointCloseStarted;endpointCloseStarted=true;
            ownedOutput=output;output=null;ownedInput=input;input=null;ownedReader=reader;closingResources++;
        }
        try{
            // The socket first unblocks connect/read/write; never hold delivery or ownership across I/O.
            if(closeSocket)try{endpoint.close();synchronized(ownership){endpointCloseFinished=true;}}catch(Exception error){closeFailure("socket",error);}
            closeStream(ownedOutput,"output stream");closeStream(ownedInput,"input stream");
        }finally{synchronized(ownership){closingResources--;ownership.notifyAll();}}
        // Queue overflow can close on the reader itself. It must not join itself or wait for another closer joining it.
        if(Thread.currentThread()==ownedReader)return;
        long deadline=System.nanoTime()+TimeUnit.MILLISECONDS.toNanos(READER_STOP_TIMEOUT_MS);
        try{
            if(ownedReader!=null){long remaining=deadline-System.nanoTime();if(remaining>0)TimeUnit.NANOSECONDS.timedJoin(ownedReader,remaining);}
            synchronized(ownership){while(closingResources>0){long remaining=deadline-System.nanoTime();if(remaining<=0)break;TimeUnit.NANOSECONDS.timedWait(ownership,remaining);}
                if(closingResources>0||(ownedReader!=null&&ownedReader.isAlive()))closeFailure("reader/streams",new IOException("Bluetooth shutdown did not finish before the deadline"));}
        }catch(InterruptedException error){Thread.currentThread().interrupt();closeFailure("reader/streams",error);}
        closeConfirmed();
    }
    public void cancelPendingIO(){if(connecting||writing)closeEndpoint();}
    public boolean closeConfirmed(){synchronized(ownership){boolean confirmed=stopped&&endpointCloseFinished&&!connecting&&!writing&&closingResources==0&&input==null&&output==null&&(reader==null||!reader.isAlive())&&closeFailure==null;if(confirmed)timing.releaseConfirmed();return confirmed;}}
    private String releaseFailure(){synchronized(ownership){return closeFailure==null?"":closeFailure.toString();}}
    boolean readyForHandoff(){return !stopped&&failure==null&&output!=null&&!connecting&&!writing&&packets.isEmpty();}
    public void shutdown(){closeEndpoint();watchdog.shutdownNow();packets.clear();}
}
