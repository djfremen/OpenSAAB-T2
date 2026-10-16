# SPDX-License-Identifier: MPL-2.0
# Chipsoft-specific driver bucket; Nano behavior is unchanged.
# Run in 32-bit PowerShell under an external watchdog.
# Allowlisted T8 identification reads: service 1A; ISO15765 @ 500000.
# No session changes, security access, writes, erase, reset or programming.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
if ([IntPtr]::Size -ne 4) { throw '32-bit process required' }
Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
public static class ChipsoftT8Vin {
 const string Dll = @"C:\Program Files (x86)\CHIPSOFT_J2534_Pro_Driver\j2534_interface.dll";
 [StructLayout(LayoutKind.Sequential)]
 public struct Msg {
  public uint ProtocolID, RxStatus, TxFlags, Timestamp, DataSize, ExtraDataIndex;
  [MarshalAs(UnmanagedType.ByValArray, SizeConst=4128)] public byte[] Data;
 }
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruOpen(IntPtr name, out uint id);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruClose(uint id);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruConnect(uint device, uint protocol, uint flags, uint baud, out uint channel);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruDisconnect(uint channel);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruStartMsgFilter(uint channel, uint type, ref Msg mask, ref Msg pattern, ref Msg flow, out uint filter);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruStopMsgFilter(uint channel, uint filter);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruWriteMsgs(uint channel, ref Msg msg, ref uint count, uint timeout);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruReadMsgs(uint channel, [In,Out] ref Msg msg, ref uint count, uint timeout);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruGetLastError(StringBuilder text);
 [DllImport(Dll, EntryPoint="PassThruIoctl", CallingConvention=CallingConvention.StdCall)] static extern uint ReadVoltage(uint device, uint ioctl, IntPtr input, out uint millivolts);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruReadVersion(uint device, StringBuilder firmware, StringBuilder dll, StringBuilder api);
 static List<string> summaries = new List<string>();
 static Stopwatch clock = Stopwatch.StartNew();
 static void Log(string s) { Console.WriteLine("{0:o} +{1}ms {2}", DateTime.UtcNow, clock.ElapsedMilliseconds, s); Console.Out.Flush(); }
 static void Check(string call, uint rc) {
  Log(call+" rc=0x"+rc.ToString("X8"));
  if(rc!=0) { var s=new StringBuilder(1024); PassThruGetLastError(s); throw new Exception(call+": "+s); }
 }
 static Msg Message(uint id, params byte[] payload) {
  var m=new Msg { ProtocolID=6, Data=new byte[4128], DataSize=(uint)(4+payload.Length) };
  m.Data[0]=(byte)(id>>24);m.Data[1]=(byte)(id>>16);m.Data[2]=(byte)(id>>8);m.Data[3]=(byte)id;
  Array.Copy(payload,0,m.Data,4,payload.Length);return m;
 }
 static bool Query(uint channel,byte pid,string name) {
   Msg tx=Message(0x7E0,0x1A,pid);tx.TxFlags=0x40;uint n=1;
   Log("REQUEST CAN=7E0 payload=1A "+pid.ToString("X2")+"; command="+name+"; ISO-TP framing/flow-control handled by driver");
   Log("CALL PassThruWriteMsgs channel="+channel+" count=1 timeout_ms=500 tx_flags=0x40 bytes="+BitConverter.ToString(tx.Data,0,(int)tx.DataSize));
   Check("PassThruWriteMsgs",PassThruWriteMsgs(channel,ref tx,ref n,500));
   Log("DRIVER_ACCEPTED_MESSAGES="+n+" (not evidence of ECU response)");
   if(n!=1)throw new Exception("Request was not accepted");
   long deadline=clock.ElapsedMilliseconds+4000;
   while(clock.ElapsedMilliseconds<deadline) {
    Msg rx=new Msg { Data=new byte[4128] };n=1;
    Log("CALL PassThruReadMsgs channel="+channel+" capacity=1 timeout_ms=250");
    uint rc=PassThruReadMsgs(channel,ref rx,ref n,250);
    Log("RETURN PassThruReadMsgs rc=0x"+rc.ToString("X8")+" count="+n);
    if(rc==7 && n==0) {
     var detail=new StringBuilder(1024);uint er=PassThruGetLastError(detail);
     if(er==0 && detail.ToString()=="ERR_EMPTY_MESSAGES_QUEUE") {System.Threading.Thread.Sleep(5);continue;}
    }
    if((rc==0x10 || rc==0x09) && n==0)continue; // J2534 ERR_BUFFER_EMPTY / ERR_TIMEOUT.
    if(rc!=0)Check("PassThruReadMsgs",rc);
    if(n==0)continue;
    if(n!=1 || rx.DataSize>4128)throw new Exception("Invalid received message size/count");
    Log("RX status=0x"+rx.RxStatus.ToString("X8")+" protocol="+rx.ProtocolID+" timestamp_us="+rx.Timestamp+" bytes="+BitConverter.ToString(rx.Data,0,(int)rx.DataSize));
    if((rx.RxStatus & 0x0B)!=0) { Log("INDICATION only: not a complete ECU reply");continue; }
    if(rx.RxStatus!=0 || rx.ProtocolID!=6 || rx.DataSize<6)continue;
    uint id=((uint)rx.Data[0]<<24)|((uint)rx.Data[1]<<16)|((uint)rx.Data[2]<<8)|rx.Data[3];
    if(id!=0x7E8)continue;
    if(rx.Data[4]==0x5A && rx.Data[5]==pid) {
     if(pid==0x90) {
      if(rx.DataSize!=23)throw new Exception("VIN reply has invalid length");
      string vin=Encoding.ASCII.GetString(rx.Data,6,17);
      if(!Regex.IsMatch(vin,"^[A-HJ-NPR-Z0-9]{17}$"))throw new Exception("VIN reply has invalid characters");
      Log("ECU_VIN="+vin);
     }
     string value=Encoding.ASCII.GetString(rx.Data,6,(int)rx.DataSize-6);
     bool printable=true;
     for(int i=6;i<(int)rx.DataSize;i++)if(rx.Data[i]<0x20 || rx.Data[i]>0x7E)printable=false;
     value=printable?value.Trim():BitConverter.ToString(rx.Data,6,(int)rx.DataSize-6).Replace("-"," ")+" (hex)";
     summaries.Add("SUMMARY TX 7E0: 1A "+pid.ToString("X2")+" -> RX 7E8: 5A "+pid.ToString("X2")+" "+name+"="+value);
     Log("IDENTITY pid="+pid.ToString("X2")+" name="+name+" value="+value+" payload="+BitConverter.ToString(rx.Data,6,(int)rx.DataSize-6));
     return true;
    }
    if(rx.DataSize>=7 && rx.Data[4]==0x7F && rx.Data[5]==0x1A) {
     Log("ECU_NEGATIVE_RESPONSE NRC=0x"+rx.Data[6].ToString("X2"));
     if(rx.Data[6]!=0x78)return false;
    }
   }
   Log("TIMEOUT command="+name+" pid="+pid.ToString("X2")+": no matching reply within 4000ms");return false;
 }
 public static int Run() {
  uint device=0,channel=0,filter=0;bool opened=false,connected=false,filtered=false;
  try {
   if(IntPtr.Size!=4 || Marshal.SizeOf(typeof(Msg))!=4152) throw new Exception("Unexpected native ABI");
   Log("LIVE scope=read-identification-only; process=32-bit; J2534-message-bytes=4152");
   Log("CALL PassThruOpen name=NULL");
   Check("PassThruOpen",PassThruOpen(IntPtr.Zero,out device));opened=true;
   var fw=new StringBuilder(256);var dll=new StringBuilder(256);var api=new StringBuilder(256);
   Log("CALL PassThruReadVersion device="+device);
   uint versionRc=PassThruReadVersion(device,fw,dll,api);
   Log("PassThruReadVersion rc=0x"+versionRc.ToString("X8"));
   if(versionRc==0)Log("ADAPTER_VERSION firmware="+fw+" dll="+dll+" api="+api);
   Log("CALL PassThruIoctl READ_VBATT device="+device+" ioctl=3");
   uint mv;uint voltageRc=ReadVoltage(device,3,IntPtr.Zero,out mv);
   Log("READ_VBATT rc=0x"+voltageRc.ToString("X8")+(voltageRc==0?" millivolts="+mv:""));
   Log("CALL PassThruConnect device="+device+" protocol=6 flags=0 baud=500000");
   Check("PassThruConnect ISO15765 flags=0 baud=500000 default-HSCAN",PassThruConnect(device,6,0,500000,out channel));connected=true;
   Msg mask=Message(0x7FF),pattern=Message(0x7E8),flow=Message(0x7E0);
   Log("CALL PassThruStartMsgFilter channel="+channel+" type=3 mask=000007FF pattern=000007E8 flow=000007E0");
   Check("FLOW_CONTROL_FILTER mask=7FF response=7E8 flow=7E0",PassThruStartMsgFilter(channel,3,ref mask,ref pattern,ref flow,out filter));filtered=true;
   bool identity=Environment.GetEnvironmentVariable("VCX_READ_PROFILE")=="Identity";
   if(!Query(channel,0x90,"VIN"))return 2;
   if(identity) {
    byte[] pids={0x71,0x08,0x74,0xB4,0x95,0x73};
    string[] names={"ECU hardware","Software version","Calibration set","ECU serial number","ECU SW version number","ECU codefile version"};
    int succeeded=1;
    for(int i=0;i<pids.Length;i++) {
     if(Query(channel,pids[i],names[i]))succeeded++;
     else { Log("IDENTITY_SUITE incomplete: stopping after failed reply to avoid stale response ambiguity"); break; }
    }
    Log("IDENTITY_SUITE successful="+succeeded+" requested=7");
    if(succeeded!=7)return 4;
   }
   return 0;
  } catch(Exception e) { Log("ERROR "+e.Message);return 1; }
  finally {
   if(filtered) { Log("CALL PassThruStopMsgFilter channel="+channel+" filter="+filter); Log("PassThruStopMsgFilter rc=0x"+PassThruStopMsgFilter(channel,filter).ToString("X8")); }
   if(connected) { Log("CALL PassThruDisconnect channel="+channel); Log("PassThruDisconnect rc=0x"+PassThruDisconnect(channel).ToString("X8")); }
   if(opened) { Log("CALL PassThruClose device="+device); Log("PassThruClose rc=0x"+PassThruClose(device).ToString("X8")); }
   foreach(string summary in summaries)Log(summary);
  }
 }
}
'@
$result=[ChipsoftT8Vin]::Run()
Write-Output ('PROBE_RESULT='+$result)
exit $result
