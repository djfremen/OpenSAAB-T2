// SPDX-License-Identifier: MPL-2.0
// Windows OS bindings for the shared Android-derived security worker.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

static class SecurityWorkspace
{
    public static Dictionary<string,object> Latest(string state)
    {
        string path=Path.Combine(state,"latest-session.json");
        if(!File.Exists(path)) Migrate(state);
        if(!File.Exists(path)) return null;
        if(new FileInfo(path).Length>8192) throw new IOException("Invalid session metadata");
        return new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(SessionSnapshot.Text(path));
    }
    // Upgrade the exact legacy pointer; never search history for a substitute seed.
    public static void Migrate(string state)
    {
        string path=Path.Combine(state,"latest-session.txt");
        if(!File.Exists(path)||new FileInfo(path).Length>8192) return;
        var m=new Dictionary<string,object>();
        foreach(string line in File.ReadAllLines(path)){int n=line.IndexOf('=');if(n>0)m[line.Substring(0,n)]=line.Substring(n+1);}
        if(!m.ContainsKey("run")||!m.ContainsKey("firmware"))return;
        string run=Path.GetFullPath(Convert.ToString(m["run"]));
        if(Path.GetDirectoryName(run)!=Path.Combine(state,"runs"))throw new IOException("Invalid session location");
        if(m.ContainsKey("pid"))m["pid"]=Int32.Parse(Convert.ToString(m["pid"]));
        Write(state,m);
    }
    public static void Write(string state,Dictionary<string,object> meta)
    {
        string path=Path.Combine(state,"latest-session.json"),temp=path+"."+Guid.NewGuid().ToString("N")+".tmp";
        File.WriteAllText(temp,new JavaScriptSerializer().Serialize(meta),new UTF8Encoding(false));
        if(File.Exists(path))File.Replace(temp,path,null);else File.Move(temp,path);
    }
    public static string Previous(string state,string firmware)
    {
        var m=Latest(state);if(m==null||!m.ContainsKey("firmware")||Convert.ToString(m["firmware"])!=firmware||!m.ContainsKey("run"))return null;
        string run=Path.GetFullPath(Convert.ToString(m["run"]));
        return Path.GetDirectoryName(run)==Path.Combine(state,"runs")?run:null;
    }
}
static class SecurityPassword
{
    static string PathFor(string state){return Path.Combine(state,"security-password.dpapi");}
    public static string Read(string state){try{return Encoding.UTF8.GetString(ProtectedData.Unprotect(File.ReadAllBytes(PathFor(state)),null,DataProtectionScope.CurrentUser));}catch(IOException){return null;}catch(CryptographicException){return null;}}
    public static string ReadValidated(string state) { var value=Read(state); if(value==null)return null; try{SecurityAccessGate.Authorize(value);return value;}catch(IOException){Forget(state);return null;} }
    public static void Save(string state,string password){File.WriteAllBytes(PathFor(state),ProtectedData.Protect(Encoding.UTF8.GetBytes(password),null,DataProtectionScope.CurrentUser));}
    public static void Forget(string state){if(File.Exists(PathFor(state)))File.Delete(PathFor(state));}
}
static class SecurityAccessGate
{
    public static void Authorize(string password, bool cancelled=false)
    {
        var result=DesktopWorker.Call(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"opensaab-security.exe"),new Dictionary<string,object>{{"op","authorize"},{"password",password},{"cancelled",cancelled}});
        if(!Object.Equals(result["authorized"],true))throw new IOException("Security access password was not accepted.");
    }
}
sealed class SecurityJob
{
    readonly object gate=new object();bool cancelled,committing;HttpWebRequest request;string attempt;
    public bool Cancelled {get{lock(gate)return cancelled;}}
    public void Attach(HttpWebRequest r){lock(gate){request=r;if(cancelled)r.Abort();}}
    public void SetAttempt(string a){lock(gate){attempt=a;if(cancelled)File.WriteAllText(Path.Combine(a,"cancel"),"cancel\n");}}
    public void BeginCommit(){lock(gate){if(cancelled)throw new IOException("Security processing cancelled; no data imported.");committing=true;}}
    public bool Cancel(){lock(gate){if(committing)return false;cancelled=true;if(request!=null)request.Abort();if(attempt!=null)File.WriteAllText(Path.Combine(attempt,"cancel"),"cancel\n");return true;}}
}
static class SecurityHTTP
{
    public static HttpWebRequest Request(string endpoint,string primary,string fallback,string password,byte[] body,string accessPassword)
    {
        SecurityAccessGate.Authorize(accessPassword);
        if(endpoint!=primary&&endpoint!=fallback)throw new IOException("Unexpected security endpoint");
        var uri=new Uri(endpoint);
        if(uri.Scheme!="https"||!uri.IsDefaultPort||uri.UserInfo!=""||uri.Query!=""||uri.Fragment!=""||body.Length>65536)throw new IOException("Invalid security request");
        if(endpoint!=primary&&password!=null)throw new IOException("Password must never go to fallback");
        ServicePointManager.SecurityProtocol|=SecurityProtocolType.Tls12;
        var r=(HttpWebRequest)WebRequest.Create(uri);r.Method="POST";r.ContentType="application/json";r.Accept="application/json";r.ContentLength=body.Length;
        r.AllowAutoRedirect=false;r.Credentials=null;r.UseDefaultCredentials=false;r.CookieContainer=null;r.Timeout=30000;r.ReadWriteTimeout=30000;
        r.CachePolicy=new System.Net.Cache.RequestCachePolicy(System.Net.Cache.RequestCacheLevel.NoCacheNoStore);
        if(password!=null){r.Headers["Authorization"]="Bearer "+password;r.Headers["X-OpenSAAB-Consent"]="security-storage-24h-v1";}
        return r;
    }
    public static SupportReply Post(string endpoint,string primary,string fallback,Dictionary<string,object> payload,string password,SecurityJob job,string accessPassword)
    {
        var result=new SupportReply();HttpWebRequest request=null;
        try {
            if(job.Cancelled)throw new OperationCanceledException();
            byte[] body=Encoding.UTF8.GetBytes(new JavaScriptSerializer().Serialize(payload));request=Request(endpoint,primary,fallback,password,body,accessPassword);job.Attach(request);
            using(var deadline=new System.Threading.Timer(_=>request.Abort(),null,45000,System.Threading.Timeout.Infinite)){
                using(var stream=request.GetRequestStream())stream.Write(body,0,body.Length);
                HttpWebResponse response;try{response=(HttpWebResponse)request.GetResponse();}catch(WebException e){if(e.Response==null)throw;response=(HttpWebResponse)e.Response;}
                using(response){result.Status=(int)response.StatusCode;using(var stream=response.GetResponseStream())result.Body=Encoding.UTF8.GetString(DesktopWorker.ReadBounded(stream,1024*1024));}
            }
        }catch(WebException e){result.Network=job.Cancelled?"cancelled":e.Status==WebExceptionStatus.Timeout||e.Status==WebExceptionStatus.RequestCanceled?"timeout":e.Status==WebExceptionStatus.NameResolutionFailure?"dns":e.Status==WebExceptionStatus.ConnectFailure?"connect":"tls_or_invalid";}
        catch(OperationCanceledException){result.Network="cancelled";}catch(IOException){result.Network="oversize";}catch(Exception){result.Network="invalid";}
        finally{if(request!=null)request.Abort();}
        return result;
    }
}
sealed partial class SessionWindow
{
    bool PreviewSecurityNoPrompt {get{return ProductIdentity.PreviewSecurityNoPrompt;}}
    string SecurityWorker {get{return Path.Combine(root,"opensaab-security.exe");}}
    SecurityJob securityJob;bool securityWorkflowChecking;DateTime securityWorkflowPoll=DateTime.MinValue;string transferRun; bool securityNavigating; long securityNavigationGeneration; Dictionary<string,object> securityNavigation=new Dictionary<string,object>();
    Dictionary<string,object> SecurityCall(string op,params object[] pairs)
    {
        var v=new Dictionary<string,object>{{"op",op}};for(int i=0;i<pairs.Length;i+=2)v[(string)pairs[i]]=pairs[i+1];return DesktopWorker.Call(SecurityWorker,v);
    }
    async void SecurityWorkflowTick()
    {
        if(securityWorkflowChecking||busy||!Running||(DateTime.UtcNow-securityWorkflowPoll).TotalSeconds<1)return;
        securityWorkflowPoll=DateTime.UtcNow;securityWorkflowChecking=true;var current=session;long generation=securityNavigationGeneration;bool navigate=securityNavigating&&current.CanAcceptMenuKey;var nav=securityNavigation;string vehicleVin=SecurityObservation().ContainsKey("vin")?SecurityObservation()["vin"]:"";
        try{
            var observed=await Task.Run(()=>{string path=Path.Combine(current.RunDirectory,"screen.txt");if(new FileInfo(path).Length>8192)throw new IOException();string screen=SessionSnapshot.Text(path);var result=SecurityCall("observe","screen",screen);
                if(navigate)result["navigation_result"]=SecurityCall("navigate","screen",screen,"vin",vehicleVin,"navigation",nav,"now_ms",(long)(DateTime.UtcNow-new DateTime(1970,1,1)).TotalMilliseconds);return result;});
            if(session==current&&!busy&&Object.Equals(observed["transfer"],true)){
                securityNavigating=false;if(transferRun!=current.RunDirectory){transferRun=current.RunDirectory;status.Text="[PRE-AUTH] · Collection ready. Actions → Process security data.";}
            }
            if(session==current&&!busy&&securityNavigating&&generation==securityNavigationGeneration&&current.CanAcceptMenuKey&&observed.ContainsKey("navigation_result")){
                var next=(Dictionary<string,object>)observed["navigation_result"];securityNavigation=(Dictionary<string,object>)next["navigation"];securityNavigating=Object.Equals(next["active"],true);if(next["key"]!=null)current.Key(Convert.ToInt32(next["key"]));
            }
        }catch(IOException){}catch(Exception){}finally{securityWorkflowChecking=false;}
    }
    async void BeginFreshSecurity()
    {
        if(busy)return;
        if(transport!=WindowsTransport.ChipsoftDirect){TextDialog("Get Security Access","Select OpenSAAB direct USB/CDC first. The OEM comparison stays read-only.",true);return;}
        if(MessageBox.Show(this,"Restart the original firmware with fresh security data? Existing sessions and downloaded firmware are preserved. OpenSAAB will navigate to Get Security Access. Follow the physical ignition-key prompts yourself.","Collect fresh security data",MessageBoxButtons.OKCancel,MessageBoxIcon.Information)!=DialogResult.OK)return;
        SetBusy(true);try{await Stop();}finally{SetBusy(false);}
        mode=ChipsoftSessionMode.Seeds;
        if(adapter==null||String.IsNullOrEmpty(adapter.Port))await Connect(true);else await Start(true,true);
        if(Running){securityNavigation=new Dictionary<string,object>();securityNavigationGeneration++;securityNavigating=true;transferRun=null;status.Text="Collecting security data · Follow the original ignition prompts.";}
    }
    string AskSecurityPassword()
    {
        if(PreviewSecurityNoPrompt)return "OpenSAAB";
        using(var f=Dialog("Security Access Password",540,260)){
            var field=new TextBox{UseSystemPasswordChar=true,Dock=DockStyle.Top,MaxLength=512};
            var remember=new CheckBox{Text="Remember for this Windows user (encrypted)",Dock=DockStyle.Top,Height=36};
            var help=new Label{Text="Enter your existing OpenSAAB security-service password. Case matters. This is separate from your Windows login.",Dock=DockStyle.Top,Height=82,Padding=new Padding(10)};
            string password=null;var buttons=new FlowLayoutPanel{Dock=DockStyle.Bottom,Height=54};
            buttons.Controls.Add(Button("Continue",()=>{try{SecurityAccessGate.Authorize(field.Text);}catch(IOException){MessageBox.Show(f,"Enter the case-sensitive OpenSAAB security access password.","Security access",MessageBoxButtons.OK,MessageBoxIcon.Warning);return;}password=field.Text;if(remember.Checked)SecurityPassword.Save(state,password);field.Clear();f.Close();},true));
            buttons.Controls.Add(Button("Cancel",()=>f.Close()));foreach(Control b in buttons.Controls){b.Dock=DockStyle.None;b.Size=new Size(160,45);}
            f.Controls.Add(remember);f.Controls.Add(field);f.Controls.Add(help);f.Controls.Add(buttons);f.Shown+=delegate{field.Focus();};f.ShowDialog(this);return password;
        }
    }
    void PasswordSettings(){if(busy||PreviewSecurityNoPrompt)return;Popup(new[]{"Enter security service password","Forget saved password"},new Action[]{()=>{AskSecurityPassword();},()=>{SecurityPassword.Forget(state);status.Text="Saved service password removed";}});}
    bool? SecurityConsent()
    {
        using(var f=Dialog("Share VIN and process security data",730,400)){
            var text=new Label{Dock=DockStyle.Fill,Padding=new Padding(15),Text="Send the collected VIN and 714-byte security file to OpenSAAB? It is stored privately for processing and troubleshooting, with deletion scheduled after one day. Request outcome/timing records are kept for seven days. No processed response is archived by the service.\r\n\r\nOpenSAAB is contacted first. Allow fallback also permits sending the same data to Bojer (sas.mysaab.info), a separate service, if OpenSAAB is unreachable or returns an outage. Bojer never receives OpenSAAB authentication credentials.\r\n\r\nThe firmware stops and releases USB before processing. A validated response updates only the security region in a new working card, then firmware restarts automatically when the same adapter is available. Keep the same vehicle and adapter connected."};
            bool? choice=null;var buttons=new FlowLayoutPanel{Dock=DockStyle.Bottom,Height=60};
            buttons.Controls.Add(Button("OpenSAAB only",()=>{choice=false;f.Close();},true));buttons.Controls.Add(Button("Allow fallback",()=>{choice=true;f.Close();}));buttons.Controls.Add(Button("Cancel",()=>f.Close()));foreach(Control b in buttons.Controls){b.Dock=DockStyle.None;b.Size=new Size(210,48);}
            f.Controls.Add(text);f.Controls.Add(buttons);f.ShowDialog(this);return choice;
        }
    }
    async Task<string> ProbeIdentity(string port)
    {
        return await Task.Run(()=>{
            using(var p=new Process()){
                p.StartInfo=new ProcessStartInfo(Path.Combine(root,"chipsoft-probe.exe"),"--port "+EmulatorSession.Quote(port)+" --timeout-ms 2000"){UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true,RedirectStandardError=true};p.Start();
                var stdout=Task.Run(()=>DesktopWorker.ReadBounded(p.StandardOutput.BaseStream,8192));var stderr=Task.Run(()=>DesktopWorker.ReadBounded(p.StandardError.BaseStream,8192));
                try{if(!p.WaitForExit(6000)||!stdout.Wait(1000)||!stderr.Wait(1000))throw new IOException("Adapter identification timed out");if(p.ExitCode!=0)throw new IOException("Adapter identification failed");
                    string identity=String.Join("\n",Encoding.UTF8.GetString(stdout.Result).Split('\n').Where(l=>l.StartsWith("Identified: CHIPSOFT J2534 Pro v. ")).Select(l=>l.Trim()));if(identity.Length==0)throw new IOException("Chipsoft identity unavailable");return identity;
                }finally{if(!p.HasExited){p.Kill();p.WaitForExit();}}
            }
        });
    }
    async Task ResumeProcessedSecurity()
    {
        if(busy)return;
        if(Running){lcd.Focus();return;}
        SetBusy(true);
        try {
            status.Text="Checking Chipsoft before resuming processed security data…";
            var devices=await Task.Run(()=>AdapterProfile.Detect().Where(a=>a.Id=="chipsoft-pro"&&!String.IsNullOrEmpty(a.Port)).ToArray());
            if(devices.Length!=1)throw new IOException("Keep the original Chipsoft and vehicle connected, then choose Continue firmware. Processed data is saved.");
            var chosen=devices[0];await ProbeIdentity(chosen.Port);
            adapter=new AdapterProfile{Id=chosen.Id,Name=chosen.Name,Port=chosen.Port,Driver=chosen.Driver};
            transport=WindowsTransport.ChipsoftDirect;mode=ChipsoftSessionMode.FullNative;
            var observation=SecurityObservation();vin=observation.ContainsKey("vin")?observation["vin"]:"";
            SetBusy(false);await Start(true);
            if(Running)status.Text="[POST-AUTH] · Firmware resumed · Follow the original menus.";
        }catch(Exception e){status.Text="Processed security data saved · Resume needs attention";TextDialog("Continue firmware",e.Message,true);}
        finally{SetBusy(false);UpdateSecurity();}
    }
    async void SecurityAction()
    {
        if(securityJob!=null){status.Text=securityJob.Cancel()?"Cancelling security processing…":"Finishing verified import…";return;}
        if(busy)return;
        UpdateSecurity();
        if(securityView.State=="POST_AUTH"){await ResumeProcessedSecurity();return;}
        string selected=firmware.Selected==null?null:firmware.Selected.id;
        string run=selected==null?null:session!=null?session.RunDirectory:SecurityWorkspace.Previous(state,selected);
        if(run==null){CollectSecuritySeeds();return;}
        var current=session;SetBusy(true);
        Dictionary<string,object> available;
        try{available=await Task.Run(()=>SecurityCall("inspect_existing","state",state,"run",run,"firmware",selected));}
        catch(Exception e){SetBusy(false);if(MessageBox.Show(this,e.Message+"\n\nStart a fresh collection through the original firmware? Existing sessions are preserved.","Security collection",MessageBoxButtons.YesNo,MessageBoxIcon.Information)==DialogResult.Yes)BeginFreshSecurity();return;}
        SetBusy(false);if(session!=current||firmware.Selected.id!=selected)return;
        string password=PreviewSecurityNoPrompt?"OpenSAAB":SecurityPassword.ReadValidated(state)??AskSecurityPassword();if(String.IsNullOrEmpty(password))return;
        bool? consent=SecurityConsent();if(!consent.HasValue)return;
        var job=new SecurityJob();securityJob=job;SetBusy(true);string inputHash=(string)available["input_hash"];
        try{
            securityNavigating=false;securityNavigationGeneration++;status.Text="Closing firmware and preserving collected seeds…";await Stop();
            // Capture a read-only adapter identity for automatic resume; collection can also be processed offline.
            AdapterProfile resume=null;string identity=null;
            var candidates=await Task.Run(()=>AdapterProfile.Detect().Where(a=>a.Id=="chipsoft-pro"&&!String.IsNullOrEmpty(a.Port)).ToArray());
            if(candidates.Length==1){var a=candidates[0];resume=new AdapterProfile{Id=a.Id,Name=a.Name,Port=a.Port,Driver=a.Driver};try{identity=await ProbeIdentity(resume.Port);}catch(IOException){resume=null;}}
            status.Text="Verifying collected data and contacting OpenSAAB… Actions → Cancel security processing.";
            var result=await Task.Run(()=>{
                var prepared=SecurityCall("prepare_existing","state",state,"run",run,"firmware",selected,"allow_fallback",consent.Value,"input_hash",inputHash,"password",password,"cancelled",job.Cancelled);
                string attempt=(string)prepared["attempt"];job.SetAttempt(attempt);string endpoint=(string)prepared["endpoint"];
                var endpoints=SecurityCall("endpoints");string primary=(string)endpoints["primary"],fallback=(string)endpoints["fallback"];
                for(int i=0;i<2;i++){
                    var reply=SecurityHTTP.Post(endpoint,primary,fallback,(Dictionary<string,object>)prepared["payload"],i==0?password:null,job,password);
                    var decision=SecurityCall("http_result","status",reply.Status,"network",reply.Network,"primary",i==0,"allow_fallback",consent.Value,"cancelled",job.Cancelled,"password",password);
                    string action=(string)decision["action"];
                    if(action=="fallback"){endpoint=(string)decision["endpoint"];continue;}
                    if(action=="unauthorized"){
                        if(PreviewSecurityNoPrompt)throw new IOException("The security service rejected this developer preview. Update or report the problem.");
                        SecurityPassword.Forget(state);
                    }
                    if(action!="validate")throw new IOException((string)decision["message"]);
                    var response=new JavaScriptSerializer{MaxJsonLength=1024*1024}.Deserialize<Dictionary<string,object>>(reply.Body);
                    job.BeginCommit();return SecurityCall("import","attempt",attempt,"provider",endpoint,"response",response,"password",password,"cancelled",job.Cancelled);
                }
                throw new IOException("No valid security response. Collected data preserved.");
            });
            password=null;transferRun=null;UpdateSecurity();
            status.Text="[POST-AUTH] · Response imported · "+Convert.ToString(result["imported_utc"]);
            if(resume!=null&&!job.Cancelled){
                bool same=false;try{same=await ProbeIdentity(resume.Port)==identity;}catch(IOException){}
                if(same){adapter=resume;transport=WindowsTransport.ChipsoftDirect;mode=ChipsoftSessionMode.FullNative;var observation=SecurityObservation();vin=observation.ContainsKey("vin")?observation["vin"]:"";securityJob=null;SetBusy(false);await Start(true);if(Running)status.Text="[POST-AUTH] · Firmware restarted · Follow the original menus.";UpdateSecurity();return;}
            }
            TextDialog("Security data ready","The response is saved with its timestamp. Connect the original adapter and vehicle, then Connect and start. Processed card data is preserved.",true);
        }catch(Exception e){status.Text="Security processing stopped · Collection preserved for retry";TextDialog("Security access",e.Message,true);}
        finally{password=null;securityJob=null;SetBusy(false);UpdateSecurity();}
    }
}
