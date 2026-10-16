// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
static class SecurityWorkflowTests
{
    static void Need(bool ok,string name){if(!ok)throw new Exception("Security workflow: "+name);}
    public static void Run()
    {
        using(var probe=new System.Diagnostics.Process()) {
            probe.StartInfo=new System.Diagnostics.ProcessStartInfo(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"chipsoft-probe.exe"),"--help"){UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true};probe.Start();string help=probe.StandardOutput.ReadToEnd();Need(probe.WaitForExit(5000)&&probe.ExitCode==0&&help.Contains("Windows CDC GET_INFO only"),"packaged Windows identification entry point");
        }
        string worker=Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"opensaab-security.exe");
        var endpoints=DesktopWorker.Call(worker,new Dictionary<string,object>{{"op","endpoints"}});
        string primary=(string)endpoints["primary"],fallback=(string)endpoints["fallback"];
        var p=SecurityHTTP.Request(primary,primary,fallback,"synthetic-test-password",new byte[1]);
        Need(p.Headers["Authorization"]=="Bearer synthetic-test-password"&&p.Headers["X-OpenSAAB-Consent"]=="security-storage-24h-v1","primary credentials and retention consent");
        Need(!p.AllowAutoRedirect&&!p.UseDefaultCredentials&&p.CookieContainer==null,"no redirects/cookies/ambient credentials");p.Abort();
        var f=SecurityHTTP.Request(fallback,primary,fallback,null,new byte[1]);Need(f.Headers["Authorization"]==null&&f.Headers["X-OpenSAAB-Consent"]==null,"fallback has no primary credentials");f.Abort();
        bool rejected=false;try{SecurityHTTP.Request(fallback,primary,fallback,"synthetic",new byte[1]);}catch(IOException){rejected=true;}Need(rejected,"reject accidental fallback credentials");
        var cancel=new SecurityJob();Need(cancel.Cancel(),"cancel before commit");rejected=false;try{cancel.BeginCommit();}catch(IOException){rejected=true;}Need(rejected,"cancel prevents import");
        var commit=new SecurityJob();commit.BeginCommit();Need(!commit.Cancel(),"activation cannot be relabelled cancelled");
        string temp=Path.Combine(Path.GetTempPath(),"opensaab-security-binding-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(temp);
        try{
            SecurityPassword.Save(temp,"synthetic-test-password");Need(SecurityPassword.Read(temp)=="synthetic-test-password","DPAPI round trip");Need(!Encoding.UTF8.GetString(File.ReadAllBytes(Path.Combine(temp,"security-password.dpapi"))).Contains("synthetic-test-password"),"password not plaintext");SecurityPassword.Forget(temp);Need(SecurityPassword.Read(temp)==null,"password forget");
            var run=Path.Combine(temp,"runs","synthetic");Directory.CreateDirectory(run);
            File.WriteAllText(Path.Combine(temp,"latest-session.txt"),"pid=1234\nrun="+run+"\nfirmware=test\n");
            Need(SecurityWorkspace.Previous(temp,"test")==run&&SecurityWorkspace.Previous(temp,"other")==null,"legacy exact-session migration");
            var latest=SecurityWorkspace.Latest(temp);Need(Convert.ToInt32(latest["pid"])==1234,"migration preserves process identity");
            cancel.SetAttempt(run);Need(File.Exists(Path.Combine(run,"cancel")),"cancel before prepared attempt stays cancelled");
        }finally{Directory.Delete(temp,true);}
    }
}
