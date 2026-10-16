# SPDX-License-Identifier: MPL-2.0
# Bounded read-only DTC report. Diagnostic session 10 02, report A9 81 12,
# then stop diagnostic operation 20. Never clears codes or requests security access.
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
public static class ChipsoftT8Dtc {
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
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruStartMsgFilter(uint channel, uint type, ref Msg mask, ref Msg pattern, IntPtr flow, out uint filter);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruStopMsgFilter(uint channel, uint filter);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruWriteMsgs(uint channel, ref Msg msg, ref uint count, uint timeout);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruReadMsgs(uint channel, [In,Out] ref Msg msg, ref uint count, uint timeout);
 [DllImport(Dll, CallingConvention=CallingConvention.StdCall)] static extern uint PassThruGetLastError(StringBuilder text);
 [DllImport(Dll, EntryPoint="PassThruIoctl", CallingConvention=CallingConvention.StdCall)] static extern uint ReadVoltage(uint device, uint ioctl, IntPtr input, out uint millivolts);
 static Stopwatch clock=Stopwatch.StartNew();
 static void Log(string s) { Console.WriteLine("{0:o} +{1}ms {2}",DateTime.UtcNow,clock.ElapsedMilliseconds,s);Console.Out.Flush(); }
 static void Check(string call,uint rc) {
  Log(call+" rc=0x"+rc.ToString("X8"));
  if(rc!=0)throw new Exception(call+" failed rc=0x"+rc.ToString("X8"));
 }
 static Msg Message(uint id,params byte[] data) {
  var m=new Msg {ProtocolID=5,Data=new byte[4128],DataSize=(uint)(4+data.Length)};
  m.Data[0]=(byte)(id>>24);m.Data[1]=(byte)(id>>16);m.Data[2]=(byte)(id>>8);m.Data[3]=(byte)id;
  Array.Copy(data,0,m.Data,4,data.Length);return m;
 }
 static void Send(uint channel,string name,params byte[] payload) {
  var tx=Message(0x7E0,payload);uint n=1;
  Log("REQUEST "+name+" CAN=7E0 raw="+BitConverter.ToString(payload));
  Log("CALL PassThruWriteMsgs channel="+channel+" protocol=5 timeout_ms=500 bytes="+BitConverter.ToString(tx.Data,0,(int)tx.DataSize));
  Check("PassThruWriteMsgs",PassThruWriteMsgs(channel,ref tx,ref n,500));
  Log("DRIVER_ACCEPTED_MESSAGES="+n+" (not evidence of ECU response)");
  if(n!=1)throw new Exception("Driver did not accept request");
 }
 public static bool EmptyRead(uint rc,uint count,uint errorRc,string detail) {
  return rc==7 && count==0 && errorRc==0 && detail=="ERR_EMPTY_MESSAGES_QUEUE";
 }
 public static void SelfTest() {
  if(!EmptyRead(7,0,0,"ERR_EMPTY_MESSAGES_QUEUE") || EmptyRead(7,1,0,"ERR_EMPTY_MESSAGES_QUEUE") || EmptyRead(7,0,1,"ERR_EMPTY_MESSAGES_QUEUE") || EmptyRead(7,0,0,"other") || EmptyRead(8,0,0,"ERR_EMPTY_MESSAGES_QUEUE"))throw new Exception("Empty queue gate test failed");
  if(Marshal.SizeOf(typeof(Msg))!=4152)throw new Exception("Unexpected J2534 ABI");
  Console.WriteLine("SELF_TEST_PASS hardware_opened=false");
 }
 static bool Receive(uint channel,out Msg rx) {
  rx=new Msg {Data=new byte[4128]};uint n=1;
  Log("CALL PassThruReadMsgs channel="+channel+" capacity=1 timeout_ms=250");
  uint rc=PassThruReadMsgs(channel,ref rx,ref n,250);
  Log("RETURN PassThruReadMsgs rc=0x"+rc.ToString("X8")+" count="+n);
  if((rc==0x10 || rc==0x09) && n==0)return false;
  if(rc==7) {
   var detail=new StringBuilder(1024);uint er=PassThruGetLastError(detail);
   if(EmptyRead(rc,n,er,detail.ToString())) { Log("CHIPSOFT_EMPTY_QUEUE");System.Threading.Thread.Sleep(5);return false; }
  }
  Check("PassThruReadMsgs",rc);
  if(n==0)return false;
  if(n!=1 || rx.DataSize>4128)throw new Exception("Invalid native message size/count");
  Log("RX status=0x"+rx.RxStatus.ToString("X8")+" protocol="+rx.ProtocolID+" timestamp_us="+rx.Timestamp+" bytes="+BitConverter.ToString(rx.Data,0,(int)rx.DataSize));
  return rx.ProtocolID==5 && rx.RxStatus==0 && rx.DataSize>=6 && rx.DataSize<=12;
 }
 static uint Id(Msg m) {return ((uint)m.Data[0]<<24)|((uint)m.Data[1]<<16)|((uint)m.Data[2]<<8)|m.Data[3];}
 static bool SessionReply(uint channel,byte service,int timeout) {
  long end=clock.ElapsedMilliseconds+timeout;
  while(clock.ElapsedMilliseconds<end) {
   Msg rx;if(!Receive(channel,out rx) || Id(rx)!=0x7E8)continue;
   if(rx.Data[4]==1 && rx.Data[5]==(byte)(service+0x40))return true;
   if(rx.DataSize>=8 && rx.Data[4]==3 && rx.Data[5]==0x7F && rx.Data[6]==service) {
    Log("SESSION_NEGATIVE service="+service.ToString("X2")+" NRC="+rx.Data[7].ToString("X2"));
    if(rx.Data[7]!=0x78)return false;
   }
  }
  return false;
 }
 public static int Run() {
  uint device=0,channel=0;var filters=new List<uint>();
  bool opened=false,connected=false,sessionAttempted=false,complete=false,cleanup=true;
  var codes=new List<string>();int result=1;
  try {
   if(Marshal.SizeOf(typeof(Msg))!=4152)throw new Exception("Unexpected J2534 ABI");
   Log("ADAPTER=chipsoft-pro DLL="+Dll);
   Log("LIVE scope=read-DTC; mask=12 current/history; no clearing/security/programming");
   Log("CALL PassThruOpen");Check("PassThruOpen",PassThruOpen(IntPtr.Zero,out device));opened=true;
   Log("CALL PassThruConnect protocol=5 CAN flags=0 baud=500000");
   Check("PassThruConnect CAN flags=0 baud=500000",PassThruConnect(device,5,0,500000,out channel));connected=true;
   foreach(uint id in new uint[]{0x7E8,0x5E8}) {
    var mask=Message(0x7FF);var pattern=Message(id);uint filter;
    Log("CALL PassThruStartMsgFilter PASS_FILTER mask=7FF pattern="+id.ToString("X3"));
    Check("PassThruStartMsgFilter",PassThruStartMsgFilter(channel,1,ref mask,ref pattern,IntPtr.Zero,out filter));filters.Add(filter);
   }
   // Drain stale frames before starting a new session; never interpret them as this report.
   long drainEnd=clock.ElapsedMilliseconds+500;
   while(clock.ElapsedMilliseconds<drainEnd) {Msg old;if(!Receive(channel,out old))break;Log("DISCARD before session");}
   sessionAttempted=true;
   Send(channel,"start diagnostic operation",2,0x10,2,0,0,0,0,0);
   if(!SessionReply(channel,0x10,2500))throw new Exception("No positive diagnostic-session reply");
   Log("SESSION_STARTED=1");
   Send(channel,"read DTC current/history",3,0xA9,0x81,0x12,0,0,0,0);
   long deadline=clock.ElapsedMilliseconds+8000;
   while(clock.ElapsedMilliseconds<deadline) {
    Msg rx;if(!Receive(channel,out rx))continue;
    uint id=Id(rx);
    if(id==0x7E8 && rx.DataSize>=8 && rx.Data[4]==3 && rx.Data[5]==0x7F && rx.Data[6]==0xA9) {
     Log("DTC_NEGATIVE NRC="+rx.Data[7].ToString("X2"));
     if(rx.Data[7]==0x78)continue;
     throw new Exception("DTC request rejected");
    }
    if(id!=0x5E8 || rx.Data[4]!=0x81)continue;
    if(rx.DataSize<9)throw new Exception("Truncated 5E8 DTC record");
    if(rx.Data[5]==0 && rx.Data[6]==0 && rx.Data[7]==0) {
     complete=true;Log("DTC_END_OF_REPORT=1");break;
    }
    byte a=rx.Data[5],b=rx.Data[6];
    string code="PCBU"[a>>6].ToString()+((a>>4)&3).ToString()+(a&15).ToString("X1")+b.ToString("X2");
    string record=code+" failure_type="+rx.Data[7].ToString("X2")+" status="+rx.Data[8].ToString("X2");
    codes.Add(record);Log("DTC_RECORD "+record);
    if(codes.Count>128)throw new Exception("DTC report exceeds 128 records");
   }
   if(!complete)throw new Exception("No end-of-report within 8000ms; partial report is not an empty result");
   result=0;
  } catch(Exception e) {Log("ERROR "+e.Message);}
  finally {
   if(connected && sessionAttempted) {
    try {Send(channel,"stop diagnostic operation",1,0x20,0,0,0,0,0,0);bool stopped=SessionReply(channel,0x20,1500);Log("SESSION_STOPPED="+(stopped?1:0));cleanup &= stopped;}
    catch(Exception e) {Log("SESSION_STOP_ERROR "+e.Message);cleanup=false;}
   }
   foreach(uint filter in filters) {Log("CALL PassThruStopMsgFilter filter="+filter);uint rc=PassThruStopMsgFilter(channel,filter);Log("PassThruStopMsgFilter rc=0x"+rc.ToString("X8"));cleanup &= rc==0;}
   if(connected) {Log("CALL PassThruDisconnect");uint rc=PassThruDisconnect(channel);Log("PassThruDisconnect rc=0x"+rc.ToString("X8"));cleanup &= rc==0;}
   if(opened) {Log("CALL PassThruClose");uint rc=PassThruClose(device);Log("PassThruClose rc=0x"+rc.ToString("X8"));cleanup &= rc==0;}
   foreach(string code in codes)Log("DTC_SUMMARY "+code);
   Log("DTC_REPORT complete="+(complete?1:0)+" records="+codes.Count+" cleanup="+(cleanup?1:0));
  }
  return cleanup?result:5;
 }
}
'@
if($env:OPENSAAB_CHIPSOFT_DTC_SELFTEST -eq "1"){[ChipsoftT8Dtc]::SelfTest();exit 0}
$result=[ChipsoftT8Dtc]::Run()
Write-Output ('PROBE_RESULT='+$result)
exit $result
