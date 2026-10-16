// SPDX-License-Identifier: MPL-2.0
using System;
using System.Diagnostics;
using System.IO;
using System.Collections.Generic;
using System.Text;
static class SupportTests
{
    static void Check(bool value,string reason) { if (!value) throw new Exception(reason); }
    static void Reject(Action action,string reason) { bool rejected=false;try{action();}catch(IOException){rejected=true;}Check(rejected,reason); }
    public static void Run()
    {
        foreach(ChipsoftSessionMode mode in Enum.GetValues(typeof(ChipsoftSessionMode))) {
            var info=new ProcessStartInfo();info.EnvironmentVariables["TECH2_CHIPSOFT_FULL_NATIVE"]="1";
            SessionMode.EnvironmentFor(info,true,WindowsTransport.ChipsoftDirect,mode);
            Check(info.EnvironmentVariables["TECH2_CHIPSOFT_FULL_NATIVE"]==(mode==ChipsoftSessionMode.FullNative?"1":"0"),"Explicit native policy");
            Check(SessionMode.Arguments(true,WindowsTransport.ChipsoftDirect,mode).Contains("--candi-chipsoft-seeds")== (mode==ChipsoftSessionMode.Seeds),"Seed mode flag");
            SessionMode.EnvironmentFor(info,false,WindowsTransport.ChipsoftDirect,mode);
            Check(info.EnvironmentVariables["TECH2_CHIPSOFT_FULL_NATIVE"]=="0","Offline cannot inherit native mode");
        }
        bool oemRejected=false;try{SessionMode.Arguments(true,WindowsTransport.VendorJ2534,ChipsoftSessionMode.FullNative);}catch(InvalidOperationException){oemRejected=true;}Check(oemRejected,"OEM comparison remains read-only");
        using(var s=new MemoryStream(new byte[4096]))Check(DesktopWorker.ReadBounded(s,4096).Length==4096,"Bounded reply exact limit");
        Reject(()=>{using(var s=new MemoryStream(new byte[4097]))DesktopWorker.ReadBounded(s,4096);},"Reject oversized response");
        const string endpoint="https://www.opensaab.com/api/support/reports";
        var request=SupportUpload.Request(endpoint,"support-report-v1",new byte[1]);
        Check(!request.AllowAutoRedirect&&!request.UseDefaultCredentials&&request.CookieContainer==null,"No redirected or authenticated report upload");request.Abort();
        Reject(()=>SupportUpload.Request("http://www.opensaab.com/api/support/reports","support-report-v1",new byte[1]),"HTTPS required");
        Reject(()=>SupportUpload.Request(endpoint+"?token=secret","support-report-v1",new byte[1]),"No query credentials");
        Reject(()=>SupportUpload.Request(endpoint,"wrong",new byte[1]),"Consent required");
        Reject(()=>SupportUpload.Request(endpoint,"support-report-v1",new byte[65537]),"Bounded upload");
        string temp=Path.Combine(Path.GetTempPath(),"opensaab-report-test-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(temp);
        try {
            SessionMode.Save(temp,ChipsoftSessionMode.ReadOnly);Check(SessionMode.Load(temp)==ChipsoftSessionMode.ReadOnly,"Saved read-only respected");
            SessionMode.Save(temp,ChipsoftSessionMode.Seeds);Check(SessionMode.Load(temp)==ChipsoftSessionMode.ReadOnly,"Temporary seeds mode not persisted");
            string worker=Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"opensaab-reports.exe");
            var report=DesktopWorker.Call(worker,new Dictionary<string,object>{{"op","support_collect"},{"root",temp},{"description","Windows report integration test"},{"host",new Dictionary<string,object>{{"platform","windows"},{"arch","x86_64"},{"version","test"}}}});
            Check(File.Exists(Convert.ToString(report["path"]))&&Convert.ToString(report["text"]).Contains("Windows report integration test"),"Shared report collection creates JSON");
            var decision=DesktopWorker.Call(worker,new Dictionary<string,object>{{"op","support_receipt"},{"root",temp},{"status",200},{"body","{\"stored\":false}"},{"network",""}});
            Check(Object.Equals(decision["stored"],false),"HTTP success alone never confirms storage");
            // A truncated fresh checkpoint must not fall back to old card authorization.
            var card=Path.Combine(temp,"card.bin");using(var f=File.Create(card)){f.SetLength(SecurityStatus.Offset+SecurityStatus.Size);}
            File.WriteAllBytes(Path.Combine(temp,"working-ssa.bin"),new byte[1]);
            Check(SecurityStatus.Read(card,Path.Combine(temp,"none"),null).State=="UNAVAILABLE","Reject truncated working checkpoint");
        } finally { Directory.Delete(temp,true); }
    }
}
