// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
static class SecurityWorkflowTests
{
    static void Need(bool ok,string name){if(!ok)throw new Exception("Security workflow: "+name);}
    static void PasswordCases()
    {
        var cases=new Dictionary<string,object>();
        SecurityAccessGate.Authorize("OpenSAAB");
        cases["correct_password"]=new Dictionary<string,object>{{"result","accepted"},{"note","Exact packaged shared worker accepted the required case-sensitive credential."}};
        foreach(var item in new[]{new[]{"empty_password",""},new[]{"incorrect_password","wrong"},new[]{"case_changed_password","opensaab"}}){
            bool blocked=false;try{SecurityAccessGate.Authorize(item[1]);}catch(IOException){blocked=true;}Need(blocked,item[0]);
            cases[item[0]]=new Dictionary<string,object>{{"result","blocked"},{"note","Installed shared authorization boundary rejected the supplied invalid credential before processing."}};
        }
        bool cancelled=false;try{SecurityAccessGate.Authorize("OpenSAAB",true);}catch(IOException){cancelled=true;}Need(cancelled,"cancelled credential");
        cases["cancelled_password"]=new Dictionary<string,object>{{"result","blocked"},{"note","Cancelled request was rejected by the installed worker before processing."}};
        string worker=Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"opensaab-security.exe");
        foreach(var item in new[]{new[]{"fallback_without_password","http_result"},new[]{"import_without_password","import"}}){
            bool blocked=false;try{DesktopWorker.Call(worker,new Dictionary<string,object>{{"op",item[1]},{"primary",false},{"allow_fallback",true},{"status",503}});}catch(IOException e){blocked=e.Message.Contains("password");}Need(blocked,item[0]);
            cases[item[0]]=new Dictionary<string,object>{{"result","blocked"},{"note","Direct automation request without a credential was blocked by the packaged shared worker before fallback or import."}};
        }
        string state=Path.Combine(Path.GetTempPath(),"opensaab-password-check-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(state);
        try{SecurityPassword.Save(state,"wrong");Need(SecurityPassword.ReadValidated(state)==null&&SecurityPassword.Read(state)==null,"invalid remembered credential removed");}
        finally{Directory.Delete(state,true);}
        cases["invalid_remembered_password"]=new Dictionary<string,object>{{"result","blocked"},{"note","An invalid DPAPI credential in an isolated temporary store was rejected and removed."}};
        var proof=new Dictionary<string,object>{{"version",ProductIdentity.Version},{"source_revision",ProductIdentity.Revision},{"platform","windows"},{"architecture",ProductIdentity.Architecture},{"worker_sha256",SessionSnapshot.Hash(worker)},{"cases",cases}};
        File.WriteAllText(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"security-password-selftest.json"),new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(proof));
    }
    public static void Run()
    {
        using(var probe=new System.Diagnostics.Process()) {
            probe.StartInfo=new System.Diagnostics.ProcessStartInfo(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"chipsoft-probe.exe"),"--help"){UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true};probe.Start();string help=probe.StandardOutput.ReadToEnd();Need(probe.WaitForExit(5000)&&probe.ExitCode==0&&help.Contains("Windows CDC GET_INFO only"),"packaged Windows identification entry point");
        }
        PasswordCases();
        Need(ProductIdentity.PreviewSecurityNoPrompt,"explicit embedded preview policy");
        Need(!ProductIdentity.SelectPreviewNoPrompt(null),"missing policy retains password behavior");
        Need(!ProductIdentity.SelectPreviewNoPrompt(new Dictionary<string,object>{{"release_channel","qualified"},{"security_access_mode","preview_no_prompt"}}),"qualified metadata retains password behavior");
        Need(!ProductIdentity.SelectPreviewNoPrompt(new Dictionary<string,object>{{"release_channel","developer_preview"}}),"incomplete preview metadata retains password behavior");
        // Invoke the actual installed prompt entry point without a constructed form.
        // A dialog path cannot succeed here; explicit preview metadata returns internally.
        var window=(SessionWindow)System.Runtime.Serialization.FormatterServices.GetUninitializedObject(typeof(SessionWindow));
        var ask=typeof(SessionWindow).GetMethod("AskSecurityPassword",System.Reflection.BindingFlags.Instance|System.Reflection.BindingFlags.NonPublic);
        Need(Object.Equals(ask.Invoke(window,null),"OpenSAAB"),"preview entry point returns internal credential without operator dialog");
        File.WriteAllText(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"preview-security-selftest.json"),new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(new {version=ProductIdentity.Version,source_revision=ProductIdentity.Revision,mode="preview_no_prompt",prompt_entry_point="returned_without_dialog",unknown_metadata="password_behavior",qualified_metadata="password_behavior"}));
        string worker=Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"opensaab-security.exe");
        var endpoints=DesktopWorker.Call(worker,new Dictionary<string,object>{{"op","endpoints"}});
        string primary=(string)endpoints["primary"],fallback=(string)endpoints["fallback"];
        var p=SecurityHTTP.Request(primary,primary,fallback,"OpenSAAB",new byte[1],"OpenSAAB");
        Need(p.Headers["Authorization"]=="Bearer OpenSAAB"&&p.Headers["X-OpenSAAB-Consent"]=="security-storage-24h-v1","primary credentials and retention consent");
        Need(!p.AllowAutoRedirect&&!p.UseDefaultCredentials&&p.CookieContainer==null,"no redirects/cookies/ambient credentials");p.Abort();
        var f=SecurityHTTP.Request(fallback,primary,fallback,null,new byte[1],"OpenSAAB");Need(f.Headers["Authorization"]==null&&f.Headers["X-OpenSAAB-Consent"]==null,"fallback has no primary credentials");f.Abort();
        bool rejected=false;try{SecurityHTTP.Request(fallback,primary,fallback,"synthetic",new byte[1],"OpenSAAB");}catch(IOException){rejected=true;}Need(rejected,"reject accidental fallback credentials");
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
