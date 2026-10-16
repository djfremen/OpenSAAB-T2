// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;

static class BluetoothJson
{
    public static JavaScriptSerializer JSON {get {return new JavaScriptSerializer { MaxJsonLength=512000, RecursionLimit=64 };}}
    public static Dictionary<string,object> V(params object[] fields) { var v=new Dictionary<string,object>(); for(int i=0;i<fields.Length;i+=2) v[(string)fields[i]]=fields[i+1]; return v; }
    public static string S(Dictionary<string,object> v,string key) { object o; return v != null && v.TryGetValue(key,out o) ? o as string ?? "" : ""; }
    public static bool B(Dictionary<string,object> v,string key) { object o; return v != null && v.TryGetValue(key,out o) && o is bool && (bool)o; }
    public static Dictionary<string,object> D(Dictionary<string,object> v,string key) { object o; return v != null && v.TryGetValue(key,out o) ? o as Dictionary<string,object> ?? V() : V(); }
}
// Serial JSON-lines binding to the pinned shared Rust implementation, never a decoder.
sealed class WindowsConnectionWorker : IDisposable
{
    readonly Process process; readonly SemaphoreSlim gate=new SemaphoreSlim(1,1);
    readonly List<byte> pending=new List<byte>(); bool disposed;
    public WindowsConnectionWorker(string binary,string state)
    {
        process = new Process { StartInfo=new ProcessStartInfo(binary,EmulatorSession.Quote(state)) {
            UseShellExecute=false,CreateNoWindow=true,RedirectStandardInput=true,RedirectStandardOutput=true,RedirectStandardError=true,
            StandardOutputEncoding=Encoding.UTF8,StandardErrorEncoding=Encoding.UTF8 } };
        process.ErrorDataReceived += delegate { }; // Drain stderr; private reports retain bounded workflow effects.
        if (!process.Start()) throw new IOException("Shared Bluetooth worker failed to start"); process.BeginErrorReadLine();
    }
    public async Task<Dictionary<string,object>> Call(string scope,Dictionary<string,object> fields)
    {
        await gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if(disposed || process.HasExited) throw new IOException("Shared Bluetooth worker stopped");
            var request=new Dictionary<string,object>(fields); request["scope"]=scope;
            byte[] bytes=Encoding.UTF8.GetBytes(BluetoothJson.JSON.Serialize(request)+"\n");
            if(bytes.Length>300000) throw new IOException("Shared Bluetooth request exceeded limit");
            var write=process.StandardInput.BaseStream.WriteAsync(bytes,0,bytes.Length);
            if(await Task.WhenAny(write,Task.Delay(2000)).ConfigureAwait(false)!=write) throw new IOException("Shared Bluetooth worker write timed out");
            await write.ConfigureAwait(false); await process.StandardInput.BaseStream.FlushAsync().ConfigureAwait(false);
            DateTime deadline=DateTime.UtcNow.AddSeconds(2);
            while(DateTime.UtcNow<deadline)
            {
                int end=pending.IndexOf(10);
                if(end>=0) { byte[] line=pending.Take(end).ToArray(); pending.RemoveRange(0,end+1); return BluetoothJson.JSON.Deserialize<Dictionary<string,object>>(Encoding.UTF8.GetString(line)); }
                var buffer=new byte[4096]; var read=process.StandardOutput.BaseStream.ReadAsync(buffer,0,buffer.Length);
                int left=Math.Max(1,(int)(deadline-DateTime.UtcNow).TotalMilliseconds);
                if(await Task.WhenAny(read,Task.Delay(left)).ConfigureAwait(false)!=read) throw new IOException("Shared Bluetooth worker response timed out");
                int count=await read.ConfigureAwait(false);
                if(count==0 || pending.Count+count>512000) throw new IOException("Shared Bluetooth worker closed or exceeded limit"); pending.AddRange(buffer.Take(count));
            }
            throw new IOException("Shared Bluetooth worker response timed out");
        }
        catch { Dispose(); throw; }
        finally { gate.Release(); }
    }
    public void Dispose()
    {
        if(disposed)return;disposed=true;
        try { process.StandardInput.Close(); if(!process.WaitForExit(300))process.Kill(); } catch { }
        process.Dispose();
    }
}
sealed class WindowsBluetoothWorkflow
{
    static WindowsBluetoothWorkflow active;
    readonly string state,binary,mode; readonly Func<Func<string,bool>,IWindowsBluetoothLink> factory;
    readonly Func<Uri,int,double,CancellationToken,Task<KeyValuePair<int,string>>> lookup;
    WindowsConnectionWorker worker; IWindowsBluetoothLink link;
    Dictionary<string,object> profile=BluetoothJson.V(), pending; string expected;
    readonly Dictionary<string,int> attempted=new Dictionary<string,int>(), written=new Dictionary<string,int>();
    readonly StringBuilder trace=new StringBuilder(); readonly SemaphoreSlim events=new SemaphoreSlim(1,1);
    CancellationTokenSource clock=new CancellationTokenSource(), http=new CancellationTokenSource();
    bool closing,finished,stopping; public bool ClosurePending { get; private set; }
    public Action<BluetoothEndpoint[]> OnDevices;
    public Action<string> OnStatus;
    public Action<Dictionary<string,object>> OnComplete,OnVehicle;
    public WindowsBluetoothWorkflow(string state,string binary,string mode,Func<Func<string,bool>,IWindowsBluetoothLink> factory=null,
        Func<Uri,int,double,CancellationToken,Task<KeyValuePair<int,string>>> lookup=null)
    { this.state=state;this.binary=binary;this.mode=mode;this.factory=factory;this.lookup=lookup; }
    public async Task Start()
    {
        if(active!=null)throw new InvalidOperationException("Finish the active Bluetooth workflow before connecting"); active=this;
        try
        {
            worker=new WindowsConnectionWorker(binary,state); Record("start",await Call("manifest",BluetoothJson.V()));
            await Call("vlinker",BluetoothJson.V("op","configure","mode",mode)); string owner="";
            if(mode=="startup")
            {
                var begun=await Call("connection",BluetoothJson.V("op","begin","firmware_ready",true));
                if(BluetoothJson.S(begun,"state")!="selecting")throw new IOException(BluetoothJson.S(begun,"message"));
                var selected=await Call("connection",BluetoothJson.V("op","select_adapter","adapter","bluetooth_mic","transport_available",true));
                if(BluetoothJson.S(selected,"effect")!="discover_vin")throw new IOException("Bluetooth VIN discovery unavailable");owner=BluetoothJson.S(selected,"attempt_id");
            }
            var claim=BluetoothJson.V("op","claim");if(owner!="")claim["owner"]=owner;
            var claimed=await Call("vlinker",claim);
            if(BluetoothJson.S(claimed,"state")!="idle")throw new IOException("Bluetooth ownership unavailable: "+BluetoothJson.S(claimed,"error"));
            if(stopping) {Complete(BluetoothJson.V("status","cancelled","message","Connection stopped · No vehicle connection","bluetooth_closed",true));return;}
            link=factory!=null ? factory(command=>expected==command) : new WindowsBluetoothLink(command=>expected==command);
            link.Open(Message); Tick();
        }
        catch { if(worker!=null)worker.Dispose();worker=null;if(active==this)active=null;if(stopping || finished)return;throw; }
    }
    async void Tick()
    {
        try { while(!finished && !closing) { await Task.Delay(200,clock.Token); await Core(BluetoothJson.V("op","tick")); } } catch(OperationCanceledException) { }
    }
    public void Select(string id) { if(!closing && !finished)link.Send(BluetoothJson.V("op","select","id",id)); }
    public void Find() { if(!closing && !finished)link.Send(BluetoothJson.V("op","scan")); }
    public async Task Stop()
    {
        if(finished)return;stopping=true;
        http.Cancel();
        if(mode=="startup" && worker!=null)try { await Call("connection",BluetoothJson.V("op","cancel")); }catch { }
        if(link==null) { Complete(BluetoothJson.V("status","cancelled","message","Connection stopped · No vehicle connection","bluetooth_closed",true));return; }
        if(closing) { link.Send(BluetoothJson.V("op","close"));return; }
        await Core(BluetoothJson.V("op","cancel","message","Bluetooth read cancelled; no automatic replay")); if(!closing)Close();
    }
    Task<Dictionary<string,object>> Call(string scope,Dictionary<string,object> fields) { return worker.Call(scope,fields); }
    void Status(string text) { if(OnStatus!=null)OnStatus(text); }
    void Record(string name,Dictionary<string,object> value)
    { if(trace.Length>256*1024)return;var entry=new Dictionary<string,object>(value);entry["event"]=name;entry["utc"]=DateTimeOffset.UtcNow.ToString("o");string line=BluetoothJson.JSON.Serialize(entry);if(trace.Length+line.Length<256*1024)trace.AppendLine(line); }
    async void Message(Dictionary<string,object> v)
    {
        await events.WaitAsync();
        try
        {
            if(finished)return;Record("transport",v);
            switch(BluetoothJson.S(v,"type"))
            {
                case "candidates": if(!closing && OnDevices!=null)OnDevices((BluetoothEndpoint[])v["devices"]);break;
                case "discovery_status": if(!closing)Status(BluetoothJson.S(v,"message"));break;
                case "selected":Status("Connecting to selected Bluetooth adapter…");break;
                case "ready":
                    if(closing)break;
                    if(!WindowsBluetoothProfile.Accepts(v)) {pending=BluetoothJson.V("state","failed","error","Unsupported Bluetooth profile");Close();break;}
                    profile=v;await Core(BluetoothJson.V("op","start"));break;
                case "write_completed": string command=BluetoothJson.S(v,"command");Increment(written,command);if(expected==command)expected=null;break;
                case "rx":if(!closing)await Core(BluetoothJson.V("op","rx","bytes",v["bytes"]));break;
                case "error":if(!closing)await Core(BluetoothJson.V("op","error","message",BluetoothJson.S(v,"message")));break;
                case "closure_pending":ClosurePending=true;Status(BluetoothJson.S(v,"message"));break;
                case "closed":await Released(v);break;
            }
        }
        catch(Exception e) { pending=BluetoothJson.V("state","failed","error",e.Message);if(link==null)Complete(BluetoothJson.V("status","failed","message",e.Message,"bluetooth_closed",true));else Close(); }
        finally {events.Release();}
    }
    static void Increment(Dictionary<string,int> map,string key) { int n;map[key]=map.TryGetValue(key,out n)?n+1:1; }
    async Task Core(Dictionary<string,object> v)
    {
        if(closing || finished)return;
        try
        {
            var result=await Call("vlinker",v);if(closing || finished)return;var completed=BluetoothJson.D(result,"completed");if(completed.Count>0)Record("prompt_complete",completed);
            string status=BluetoothJson.S(result,"state");
            if(status=="send") {string command=BluetoothJson.S(result,"command");if(command=="")throw new IOException("Missing shared Bluetooth command");expected=command;Increment(attempted,command);Record("write_attempt",BluetoothJson.V("command",command));Status("Reading through selected Bluetooth adapter…");link.Send(BluetoothJson.V("op","write","command",command));}
            else if(status=="complete" || status=="failed") {pending=result;Close();}
        }
        catch(Exception e) {pending=BluetoothJson.V("state","failed","error",e.Message);Close();}
    }
    void Close() { if(closing)return;closing=true;clock.Cancel();Status("Closing Bluetooth connection…");link.Send(BluetoothJson.V("op","close")); }
    async Task Released(Dictionary<string,object> v)
    {
        closing=true;clock.Cancel();expected=null;
        bool confirmed=BluetoothJson.B(v,"bluetooth_closed");
        if(!confirmed) {ClosurePending=true;Status("Adapter cleanup is unconfirmed. Stop to check again.");return;}
        ClosurePending=false;
        bool success=!stopping && BluetoothJson.S(v,"error")=="" && BluetoothJson.S(pending,"state")=="complete";
        var result=success ? BluetoothJson.D(pending,"result") : BluetoothJson.V();
        int attempts,writes;attempted.TryGetValue("021A900000000000",out attempts);written.TryGetValue("021A900000000000",out writes);
        var report=BluetoothJson.V("schema",1,"status",success ? (BluetoothJson.S(result,"vin")==""?"identified":"vin_received") : stopping?"cancelled":"failed",
            "observed_utc",DateTimeOffset.UtcNow.ToString("o"),"transport",BluetoothJson.S(profile,"transport"),"transport_profile",profile,"ble_profile",profile,
            "probe_mode",mode=="ecm_info"||mode=="ecm_dtc"?mode:"","app_version",ProductIdentity.Version,"app_build",ProductIdentity.Revision,"result",result,"bluetooth_closed",true,
            "automatic_replay",false,"firmware_emulation",false,"command_attempts",attempted,"command_writes",written,
            "host_vehicle_request_attempts",attempts,"host_vehicle_requests_written",writes,
            "message",success?"Read complete · Adapter connection closed":stopping?"Connection stopped · No vehicle connection":BluetoothJson.S(pending,"error"));
        link=null;
        if(worker==null) {Complete(report);return;}
        await Call("vlinker",BluetoothJson.V("op","configure","mode","release"));
        try
        {
            if(success && (mode=="ecm_info"||mode=="ecm_dtc")) {var save=new Dictionary<string,object>(report);save["op"]="save";var saved=await Call("reports",save);if(BluetoothJson.S(saved,"error")!="")throw new IOException(BluetoothJson.S(saved,"error"));report["saved_report"]=saved;}
            string directory=Path.Combine(state,"vlinker-tests",Guid.NewGuid().ToString("N"));Directory.CreateDirectory(directory);
            File.WriteAllText(Path.Combine(directory,"result.json"),BluetoothJson.JSON.Serialize(report));File.WriteAllText(Path.Combine(directory,"transcript.jsonl"),trace.ToString());
        }
        catch(Exception e) {report["storage_error"]=e.Message;}
        if(mode=="startup")
        {
            var eventValue=stopping ? BluetoothJson.V("op","transport_closed","bluetooth_closed",true) : BluetoothJson.V("op","probe_result","report",report);
            await ApplyConnection(await Call("connection",eventValue));
        }
        else Complete(report);
    }
    async Task ApplyConnection(Dictionary<string,object> value)
    {
        if(finished)return;
        if(stopping && BluetoothJson.S(value,"effect")=="lookup") {value=await Call("connection",BluetoothJson.V("op","cancel"));}
        if(BluetoothJson.S(value,"effect")=="lookup")
        {
            Status(BluetoothJson.S(value,"message"));
            KeyValuePair<int,string> result=new KeyValuePair<int,string>(0,null);
            try {var address=new Uri(BluetoothJson.S(value,"url"));int limit=Convert.ToInt32(value["maximum_bytes"]);double timeout=Convert.ToDouble(value["timeout_seconds"]);result=await (lookup!=null?lookup(address,limit,timeout,http.Token):Lookup(address,limit,timeout,http.Token));}catch { }
            if(finished)return;
            value=await Call("connection",result.Value!=null?BluetoothJson.V("op","lookup_result","http_status",result.Key,"body",result.Value):BluetoothJson.V("op","lookup_unavailable"));
        }
        if(BluetoothJson.B(value,"busy")) {await Task.Delay(200);await ApplyConnection(await Call("connection",BluetoothJson.V("op","tick")));return;}
        if(OnVehicle!=null)OnVehicle(value);
        Complete(BluetoothJson.V("status",BluetoothJson.S(value,"state"),"message",BluetoothJson.S(value,"message"),"vehicle",BluetoothJson.D(value,"vehicle"),"bluetooth_closed",true));
    }
    internal static async Task<KeyValuePair<int,string>> Lookup(Uri uri,int maximum,double timeout,CancellationToken cancellation)
    {
        if(uri.Scheme!="https" || maximum<1 || maximum>256*1024 || timeout<=0 || timeout>15)throw new IOException("Unsupported shared lookup effect");
        using(var deadline=CancellationTokenSource.CreateLinkedTokenSource(cancellation))
        using(var handler=new HttpClientHandler { AllowAutoRedirect=false,UseCookies=false,UseDefaultCredentials=false })
        using(var client=new HttpClient(handler))
        {
            deadline.CancelAfter(TimeSpan.FromSeconds(timeout));client.DefaultRequestHeaders.Accept.ParseAdd("application/json");
            using(var response=await client.GetAsync(uri,HttpCompletionOption.ResponseHeadersRead,deadline.Token))
            {
                if(response.Content.Headers.ContentLength.HasValue && response.Content.Headers.ContentLength.Value>maximum)throw new IOException("Vehicle lookup exceeded limit");
                using(var stream=await response.Content.ReadAsStreamAsync())using(var bytes=new MemoryStream())
                {var buffer=new byte[4096];int n;while((n=await stream.ReadAsync(buffer,0,buffer.Length,deadline.Token))>0){if(bytes.Length+n>maximum)throw new IOException("Vehicle lookup exceeded limit");bytes.Write(buffer,0,n);}return new KeyValuePair<int,string>((int)response.StatusCode,Encoding.UTF8.GetString(bytes.ToArray()));}
            }
        }
    }
    void Complete(Dictionary<string,object> report)
    { if(finished)return;finished=true;clock.Cancel();http.Cancel();if(worker!=null)worker.Dispose();worker=null;active=null;if(OnComplete!=null)OnComplete(report); }
    public static async Task<string> ReadSaved(string state,string binary,string id)
    {
        using(var worker=new WindowsConnectionWorker(binary,state)){var v=await worker.Call("reports",BluetoothJson.V("op","read","id",id));if(BluetoothJson.S(v,"error")!="")throw new IOException(BluetoothJson.S(v,"error"));return BluetoothJson.S(v,"text");}
    }
}
