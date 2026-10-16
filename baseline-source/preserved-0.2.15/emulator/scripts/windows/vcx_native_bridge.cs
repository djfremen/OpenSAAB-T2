using System;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using System.Diagnostics;
using System.Collections.Concurrent;
using System.Runtime.InteropServices;
public static class VcxNativeBridge {
#if CHIPSOFT_PRO
 const string Dll=@"C:\Program Files (x86)\CHIPSOFT_J2534_Pro_Driver\j2534_interface.dll";
 const string Adapter="chipsoft-pro";
#else
 const string Dll=@"C:\ProgramData\VCX\Apps\PASSTHRU\VCXPT32.dll";
 const string Adapter="nano";
#endif
 [DllImport("winmm.dll")] static extern uint timeBeginPeriod(uint period);
 [DllImport("winmm.dll")] static extern uint timeEndPeriod(uint period);
 [StructLayout(LayoutKind.Sequential)] public struct Msg {
  public uint ProtocolID,RxStatus,TxFlags,Timestamp,DataSize,ExtraDataIndex;
  [MarshalAs(UnmanagedType.ByValArray,SizeConst=4128)] public byte[] Data;
 }
 [StructLayout(LayoutKind.Sequential)] struct Config { public uint Parameter,Value; }
 [StructLayout(LayoutKind.Sequential)] struct ConfigList { public uint Count; public IntPtr Items; }
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruOpen(IntPtr name,out uint device);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruClose(uint device);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruConnect(uint device,uint protocol,uint flags,uint baud,out uint channel);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruDisconnect(uint channel);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruStartMsgFilter(uint channel,uint type,ref Msg mask,ref Msg pattern,IntPtr flow,out uint filter);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruStopMsgFilter(uint channel,uint filter);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruReadMsgs(uint channel,ref Msg message,ref uint count,uint timeout);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruWriteMsgs(uint channel,ref Msg message,ref uint count,uint timeout);
 [DllImport(Dll,EntryPoint="PassThruIoctl",CallingConvention=CallingConvention.StdCall)] static extern uint SetConfig(uint channel,uint ioctl,ref ConfigList list,IntPtr output);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruReadVersion(uint device,StringBuilder firmware,StringBuilder dll,StringBuilder api);
 [DllImport(Dll,CallingConvention=CallingConvention.StdCall)] static extern uint PassThruGetLastError(StringBuilder description);
 static void Log(string s){Console.WriteLine("BRIDGE "+s);Console.Out.Flush();}
 static void Check(string call,uint rc){Log("CALL "+call+" rc="+rc.ToString("X8"));if(rc!=0){var error=new StringBuilder(1024);uint erc=PassThruGetLastError(error);throw new Exception(call+" failed "+rc.ToString("X8")+" last_error_rc="+erc.ToString("X8")+" detail="+error);}}
 static Msg Message(uint protocol,uint id,byte[] payload){var m=new Msg{ProtocolID=protocol,Data=new byte[4128],DataSize=(uint)(payload.Length+4)};m.Data[0]=(byte)(id>>24);m.Data[1]=(byte)(id>>16);m.Data[2]=(byte)(id>>8);m.Data[3]=(byte)id;Array.Copy(payload,0,m.Data,4,payload.Length);return m;}
 static void Configure(uint channel,uint parameter,uint value){IntPtr p=Marshal.AllocHGlobal(8);try{Marshal.StructureToPtr(new Config{Parameter=parameter,Value=value},p,false);var list=new ConfigList{Count=1,Items=p};Check("SET_CONFIG "+parameter.ToString("X")+"="+value.ToString("X"),SetConfig(channel,2,ref list,IntPtr.Zero));}finally{Marshal.FreeHGlobal(p);}}
 static byte[] ParseHex(string text){if(text=="-")return new byte[0];if(text.Length>16 || text.Length%2!=0)throw new Exception("Invalid frame hex");byte[] data=new byte[text.Length/2];for(int i=0;i<data.Length;i++)data[i]=Convert.ToByte(text.Substring(i*2,2),16);return data;}
 public static bool Allowed(uint controller,uint flags,uint id,byte[] data){return AllowedForSession(controller,flags,id,data,false);}
 public static bool AllowedForSession(uint controller,uint flags,uint id,byte[] data,bool seeds){
  if(controller!=0 && controller!=2)return false;
  if(id>0x7ff || data.Length>8)return false;
  if(flags==0x400)return controller==2 && id==0x100 && data.Length==0;
  if(flags!=0 || data.Length==0)return false;
  // Diagnostic requests only; reject programming/security/clear and arbitrary bus writes.
  // TCM read diagnostics use HS 7E1, as captured from the original firmware.
  // Address admission does not change the service restrictions below.
  if(id!=0x101 && id!=0x7e0 && !(controller==0 && id==0x7e1) && (id<0x240 || id>0x25f))return false;
  int offset=data[0]==0xfe ? 1 : 0;
  if(offset>=data.Length)return false;
  if(data[offset]==0x30)return data.Length-offset>=3; // guest-owned ISO-TP flow control
  int n=data[offset];offset++;
  if(n<1 || n>7 || offset+n>data.Length)return false;
  byte service=data[offset];
  // Exact original Tech2Win SAS preamble, confirmed in Chipsoft USB capture
  // and the native Nano guest after the physical LOCK transition.
  if(service==0xae)return seeds && controller==2 && id==0x241 && offset==1 && n==2 && data[offset+1]==0;
  if(service==0x27)return seeds && n==2 && offset==1 &&
   ((data[offset+1]==1 && ((controller==0 && (id==0x7e0 || id==0x7e1 || id==0x245)) || (controller==2 && id>=0x240 && id<=0x25f))) ||
    (data[offset+1]==0x0b && controller==2 && id==0x241));
  // GMLAN $2C defines a diagnostic data packet, not ECU calibration. Permit
  // only the two original 1367 engine-entry/readiness definitions observed in traces.
  if(service==0x2c)return controller==0 && id==0x7e0 && n==4 && data[offset+1]==0xf3 && data[offset+2]==0 && (data[offset+3]==0x1c || data[offset+3]==0x01);
  if(service==0x1a)return n==2;
  if(service==0x3e || service==0x20)return n==1;
  // One-shot ReadDataByPacketIdentifier. The original vehicle check also
  // requests DPID03 (transport fuse); the identifier selects data, not a write.
  if(service==0xaa)return n==3 && data[offset+1]==1;
  if(service==0xa9)return n==3 && data[offset+1]==0x81 && (data[offset+2]==0x10 || data[offset+2]==0x12 || data[offset+2]==0x0c);
  return service==0x10 && n==2 && (data[offset+1]==1 || data[offset+1]==2);
 }
 public static bool IsChipsoftEmptyRead(bool chipsoft,uint rc,uint count,uint errorRc,string detail){
  return chipsoft && rc==7 && count==0 && errorRc==0 && detail=="ERR_EMPTY_MESSAGES_QUEUE";
 }
 // Per-operation policy. API keys are loaded from a short-lived local authority;
 // a current matching BCM challenge is required before a single key attempt.
 public sealed class SeatbeltPolicy {
  readonly ushort expectedSeed,key; bool requested,matched,keyAttempted,authorized,readPending,baselineKnown,writeAttempted,writeConfirmed,currentAudible;
  public SeatbeltPolicy(ushort seed,ushort response){if(seed==0xffff || response==0xffff)throw new Exception("Unfilled BCM authority");expectedSeed=seed;key=response;}
  public bool Authorized {get{return authorized;}}
  public bool WriteConfirmed {get{return writeConfirmed;}}
  public bool CurrentAudible {get{return currentAudible;}}
  public static SeatbeltPolicy Load(string path){
   // Historical private bench authority is not a public Windows feature.
   // Keep this legacy route closed; normal firmware uses the shared security workflow.
   throw new NotSupportedException("Legacy bench-only seatbelt authority is unavailable in this build.");
  }
  static bool Bcm(uint c,uint flags,uint id){return c==2 && flags==0 && id==0x242;}
  public bool Allows(uint c,uint flags,uint id,byte[] d){
   if(d.Length>8)return false;
   if(Allowed(c,flags,id,d))return true;
   if(!Bcm(c,flags,id) || d.Length<3)return false;
   if(d[0]==2 && d[1]==0x27 && d[2]==1)return !keyAttempted;
   if(matched && !keyAttempted && d.Length>=5 && d[0]==4 && d[1]==0x27 && d[2]==2 && d[3]==(byte)(key>>8) && d[4]==(byte)key)return true;
   // Observed original BCM seatbelt operation only: 9900 -> 99C0.
   return authorized && baselineKnown && !writeAttempted && d.Length==8 && d[0]==4 && d[1]==0x3b && d[2]==1 && d[3]==0x99 && d[4]==0xc0 && d[5]==0 && d[6]==0 && d[7]==0;
  }
  public void BeforeTransmit(uint c,uint flags,uint id,byte[] d){
   if(!Allows(c,flags,id,d))throw new Exception("Seatbelt policy rejected request");
   if(Bcm(c,flags,id) && d.Length>=3){
    if(d[1]==0x27){if(d[2]==1)requested=true;else if(d[2]==2)keyAttempted=true;}
    if(d[0]==2 && d[1]==0x1a && d[2]==1)readPending=true;
    if(d[1]==0x3b)writeAttempted=true;
   }
  }
  public void Observe(uint c,uint id,byte[] d){
   if(c!=2 || id!=0x642 || d.Length<3)return;
   if(d[0]==4 && d[1]==0x67 && d[2]==1 && d.Length>=5){
    if(!requested || ((d[3]<<8)|d[4])!=expectedSeed)throw new Exception("BCM seed differs from processed SSA; recollect before authorization");
    matched=true;
   }
   if(d[0]==2 && d[1]==0x67 && d[2]==2){if(!keyAttempted)throw new Exception("Unsolicited BCM authorization");authorized=true;}
   if(d[0]==4 && d[1]==0x5a && d[2]==1 && d.Length>=5 && readPending){
    readPending=false;baselineKnown=authorized && d[3]==0x99 && d[4]==0;
    currentAudible=d[3]==0x99 && d[4]==0xc0;
   }
   if(d[0]==2 && d[1]==0x7b && d[2]==1){if(!writeAttempted)throw new Exception("Unsolicited BCM write confirmation");writeConfirmed=true;}
   if(writeAttempted && d.Length>=4 && d[0]==3 && d[1]==0x7f && d[2]==0x3b && d[3]!=0x78)throw new Exception("BCM rejected seatbelt write; no retry");
   if(keyAttempted && d.Length>=4 && d[0]==3 && d[1]==0x7f && d[2]==0x27 && d[3]!=0x78)throw new Exception("BCM rejected security access; no retry");
  }
 }
 public static void Main(string[] args){
  bool seeds=false,seatbelt=false;int port=35672;
  for(int i=0;i<args.Length;i++){
   if(args[i]=="--seed-read-only" && !seeds && !seatbelt)seeds=true;
   else if(args[i]=="--seatbelt-audible" && !seeds && !seatbelt)seatbelt=true;
   else if(args[i]=="--port" && i+1<args.Length && int.TryParse(args[++i],out port) && port>0 && port<=65535){}
   else {Environment.Exit(2);return;}
  }
  Environment.Exit(Run(seeds,seatbelt,port));
 }
 public static int Run(bool seeds){return Run(seeds,false);}
 public static int Run(bool seeds,bool seatbelt){return Run(seeds,seatbelt,35672);}
 public static int Run(bool seeds,bool seatbelt,int port){
  if(seatbelt && (seeds || Adapter!="chipsoft-pro")){Log("ERROR invalid seatbelt mode");return 2;}
  SeatbeltPolicy policy=null;
  uint device=0;uint[] channels=new uint[3],filters=new uint[3];bool opened=false;bool[] connected=new bool[3],filtered=new bool[3],emptyLogged=new bool[3];bool cleanup=true;int result=1;
  TcpListener listener=null;TcpClient client=null;bool timerResolution=false;
  FileStream guard=null;var queue=new BlockingCollection<string>(16);bool done=false;
  var watch=Stopwatch.StartNew();
  // Covers hangs inside the OEM DLL as well as the command loop.
  int runLimit=300000; // Native DTC and security menus both have physical ignition prompts.
  var timer=new Timer(delegate(object unused){Environment.Exit(124);},null,315000,Timeout.Infinite);
  try{
   guard=new FileStream(Path.Combine(Path.GetTempPath(),"tech2-vcx-read.lock"),FileMode.OpenOrCreate,FileAccess.ReadWrite,FileShare.None);
   if(IntPtr.Size!=4 || Marshal.SizeOf(typeof(Msg))!=4152)throw new Exception("Bad J2534 ABI");
   if(seatbelt)policy=SeatbeltPolicy.Load(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),"chipsoft-seatbelt-authority.txt"));
   Check("TIMER_RESOLUTION_1MS",timeBeginPeriod(1));timerResolution=true;
   listener=new TcpListener(IPAddress.Loopback,port);listener.Start(1);
   Log("ADAPTER "+Adapter+" dll="+Dll);
   Log("CALL SESSION seed_reads="+seeds+" seatbelt="+seatbelt+" key_submission="+(seatbelt?"matched-BCM-only":"false"));
   Check("OPEN",PassThruOpen(IntPtr.Zero,out device));opened=true;
   var firmware=new StringBuilder(256);var dllVersion=new StringBuilder(256);var api=new StringBuilder(256);
   Check("VERSION",PassThruReadVersion(device,firmware,dllVersion,api));
   Log("IDENTITY firmware="+firmware+" dll="+dllVersion+" api="+api);
   foreach(int c in new int[]{0,2}){
    uint protocol=c==0?5u:0x8008u;uint baud=c==0?500000u:33333u;
    Check("CONNECT controller="+c+" protocol="+protocol+" baud="+baud,PassThruConnect(device,protocol,0,baud,out channels[c]));connected[c]=true;
    if(c==2)Configure(channels[c],0x8001,0x100);
    Configure(channels[c],3,1); // Driver loopback indication; not a measurement of physical CAN ACK.
    Msg mask=Message(protocol,0,new byte[0]),pattern=Message(protocol,0,new byte[0]);
    Check("FILTER controller="+c,PassThruStartMsgFilter(channels[c],1,ref mask,ref pattern,IntPtr.Zero,out filters[c]));filtered[c]=true;
   }
   var reader=new Thread(delegate(){try{client=listener.AcceptTcpClient();var input=new StreamReader(client.GetStream(),Encoding.ASCII);string line;while((line=input.ReadLine())!=null){if(line.Length>256 || !queue.TryAdd(line))break;}}catch(Exception){ }finally{queue.CompleteAdding();}});reader.IsBackground=true;reader.Start();
   Log("READY version=1 raw=true generated_requests=0");long lastInput=watch.ElapsedMilliseconds;
   while(!done && watch.ElapsedMilliseconds<runLimit){
    string line;
    while(queue.TryTake(out line)){
     lastInput=watch.ElapsedMilliseconds;
     if(line=="PING")continue;
     if(line=="QUIT"){done=true;break;}
     string[] parts=line.Split(' ');if(parts.Length!=6 || parts[0]!="TX")throw new Exception("Invalid bridge command");
     uint controller=uint.Parse(parts[1]);ulong ticket=ulong.Parse(parts[2]);uint flags=Convert.ToUInt32(parts[3],16),id=Convert.ToUInt32(parts[4],16);byte[] data=ParseHex(parts[5]);
     if(policy!=null){if(!policy.Allows(controller,flags,id,data))throw new Exception("Seatbelt policy rejected "+line);policy.BeforeTransmit(controller,flags,id,data);}
     else if(!AllowedForSession(controller,flags,id,data,seeds))throw new Exception("Read-only gate rejected "+line);
     uint protocol=controller==0?5u:0x8008u;Msg tx=Message(protocol,id,data);tx.TxFlags=flags;uint n=1;
     Log("TX "+controller+" "+ticket+" "+parts[3]+" "+parts[4]+" "+parts[5]+" origin=native-candi");
     Check("WRITE",PassThruWriteMsgs(channels[controller],ref tx,ref n,100));
     if(n!=1)throw new Exception("Incomplete write count");Log("ACCEPT "+controller+" "+ticket);
    }
    if(done)break;
    if(queue.IsCompleted || watch.ElapsedMilliseconds-lastInput>5000)throw new Exception("Host disconnected or heartbeat expired");
    foreach(int c in new int[]{0,2}){
     // Bound each pass so a busy bus cannot starve commands or the other bus.
     for(int i=0;i<32;i++){
      Msg rx=new Msg{Data=new byte[4128]};uint n=1;
      uint rc=PassThruReadMsgs(channels[c],ref rx,ref n,0);
      if(rc==9 || rc==0x10)break;
      if(rc==7 && Adapter=="chipsoft-pro"){
       var detail=new StringBuilder(1024);uint errorRc=PassThruGetLastError(detail);
       if(IsChipsoftEmptyRead(true,rc,n,errorRc,detail.ToString())){
        if(!emptyLogged[c]){Log("CALL EMPTY_READ_COMPAT controller="+c+" rc=00000007 count=0 detail="+detail);emptyLogged[c]=true;}
        break;
       }
      }
      if(rc!=0)Check("READ controller="+c+" count="+n,rc);
      if(n==0)break;
      if(n!=1 || rx.DataSize<4 || rx.DataSize>12)throw new Exception("Invalid raw receive size");
      if(policy!=null && rx.RxStatus==0){
       uint can=((uint)rx.Data[0]<<24)|((uint)rx.Data[1]<<16)|((uint)rx.Data[2]<<8)|rx.Data[3];
       byte[] payload=new byte[rx.DataSize-4];Array.Copy(rx.Data,4,payload,0,payload.Length);bool was=policy.Authorized,wrote=policy.WriteConfirmed,audible=policy.CurrentAudible;policy.Observe((uint)c,can,payload);
       if(!was && policy.Authorized)Log("CALL BCM_SECURITY_ACCESS_CONFIRMED response=6702");
       if(!wrote && policy.WriteConfirmed)Log("CALL BCM_SEATBELT_WRITE_CONFIRMED response=7B01");
       if(!audible && policy.CurrentAudible)Log("CALL BCM_SEATBELT_READBACK config=99C0");
      }
      Log("RX "+c+" "+rx.ProtocolID+" "+rx.RxStatus.ToString("X8")+" "+rx.Timestamp+" "+BitConverter.ToString(rx.Data,0,(int)rx.DataSize).Replace("-",""));
     }
    }
    Thread.Sleep(1);
   }
   if(!done)throw new Exception("Bridge run deadline expired");result=0;
  }catch(Exception e){Log("ERROR "+e.Message);}
  finally{
   foreach(int c in new int[]{2,0}){
    if(filtered[c]){uint rc=PassThruStopMsgFilter(channels[c],filters[c]);Log("CLEAN FILTER "+c+" "+rc);cleanup &= rc==0;}
    if(connected[c]){uint rc=PassThruDisconnect(channels[c]);Log("CLEAN CHANNEL "+c+" "+rc);cleanup &= rc==0;}
   }
   if(opened){uint rc=PassThruClose(device);Log("CLEAN DEVICE "+rc);cleanup &= rc==0;}
   if(timerResolution){uint rc=timeEndPeriod(1);Log("CLEAN TIMER "+rc);cleanup &= rc==0;}
   if(client!=null)client.Close();if(listener!=null)listener.Stop();
   if(guard!=null)guard.Dispose();timer.Dispose();Log("EXIT "+(cleanup?result:5));
  }
  return cleanup?result:5;
 }
}
