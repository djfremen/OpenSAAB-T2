// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

// Synthetic endpoint only. No radio or network lookup is opened by these tests.
sealed class FixtureWindowsBluetooth : IWindowsBluetoothLink
{
    readonly Func<string,bool> allowed; public readonly List<string> Sent=new List<string>();
    public string Model,Transport,Firmware=""; public bool BadProfile,Disconnect,ConfirmClose=true,Truncated;
    Action<Dictionary<string,object>> callback;
    public FixtureWindowsBluetooth(string model,string transport,Func<string,bool> allowed) {Model=model;Transport=transport;this.allowed=allowed;}
    public void Open(Action<Dictionary<string,object>> callback) {this.callback=callback;callback(BluetoothJson.V("type","candidates","devices",new[]{new BluetoothEndpoint {Id="fixture",Name=Model,Transport=Transport}}));}
    public void Send(Dictionary<string,object> v)
    {
        string op=BluetoothJson.S(v,"op");
        if(op=="close") {callback(BluetoothJson.V("type","closed","bluetooth_closed",ConfirmClose,"error",""));return;}
        if(op=="scan")return;
        if(op=="select")
        {
            callback(Transport=="ble"?BluetoothJson.V("type","ready","transport","ble","device_name",Model,"service",BadProfile?"FFFF":"18F0","write","2AF1","notify","2AF0","write_type","withResponse")
                :BluetoothJson.V("type","ready","transport","windows-classic-spp","device_name",Model,"uuid",WindowsBluetoothProfile.Serial));return;
        }
        string command=BluetoothJson.S(v,"command");if(!allowed(command))throw new Exception("Fixture received a command outside the shared effect");
        Sent.Add(command);callback(BluetoothJson.V("type","write_completed","command",command));
        if(Disconnect && command=="021A900000000000") {callback(BluetoothJson.V("type","closed","bluetooth_closed",true,"error","Synthetic disconnect"));return;}
        string reply;
        switch(command)
        {
            case "ATZ":reply="ELM327";break;
            case "VTI":reply=Model=="Carista EVO"?"Carista EVO v2.2.84":"vLinker MC+ v2.2.92";break;
            case "VTVERS":reply=Firmware!=""?Firmware:Model=="Carista EVO"?"MIC3413 V2.0.84":"MIC3313 V2.0.92";break;
            case "ATRV":reply="12.0V";break;
            case "VTPRON":reply="6 [AT]";break;
            case "VTPBRD":reply="500000";break;
            case "021A900000000000":reply="7E8 10 13 5A 90 59 53 33 46\r7E8 21 44 34 39 59 58 34 31\r7E8 22 30 30 30 30 30 31 00";break;
            case "0210020000000000":reply="7E8 01 50 00 00 00 00 00 00";break;
            case "0120000000000000":reply="7E8 01 60 00 00 00 00 00 00";break;
            case "03A9811200000000":reply="7E8 03 7F A9 78 00 00 00 00\r5E8 81 01 07 00 6F 00 00 00"+(Truncated?"":"\r5E8 81 00 00 00 FF 00 00 00");break;
            default:
                if(command.StartsWith("021A") && command.Length==16) {string pid=command.Substring(4,2);reply=pid=="95"?"7E8 04 5A 95 00 0B 00 00 00":"7E8 03 5A "+pid+" 41 00 00 00 00";}else reply="OK";break;
        }
        int[] bytes=Encoding.ASCII.GetBytes(reply+"\r>").Select(b=>(int)b).ToArray();callback(BluetoothJson.V("type","rx","bytes",bytes.Take(2).ToArray()));callback(BluetoothJson.V("type","rx","bytes",bytes.Skip(2).ToArray()));
    }
}
static class BluetoothTests
{
    static int checks;
    static void Need(bool condition,string description) {checks++;if(!condition)throw new Exception("Bluetooth fixture failed: "+description);}
    static async Task<Dictionary<string,object>> Finished(TaskCompletionSource<Dictionary<string,object>> result)
    {if(await Task.WhenAny(result.Task,Task.Delay(8000))!=result.Task)throw new Exception("Bluetooth fixture completion timed out");return await result.Task;}
    public static async Task<int> RunAsync(string binary,string root)
    {
        checks=0;
        Need(WindowsBluetoothProfile.Known(" Carista EVO ",true) && WindowsBluetoothProfile.Known("vLinker MC-IOS",true) && WindowsBluetoothProfile.Known("vLinker MC-Android",false) && !WindowsBluetoothProfile.Known("vLinker FD",true) && !WindowsBluetoothProfile.Known("Carista EVOLUTION",false),"exact transport name hints");
        foreach(string product in new[]{"evo","vlinker"})foreach(string transport in new[]{"ble","classic"})foreach(string mode in new[]{"vin","ecm_info","ecm_dtc"})
        {
            string path=Path.Combine(root,Guid.NewGuid().ToString("N"));FixtureWindowsBluetooth endpoint=null;
            string name=product=="evo"?"Carista EVO":transport=="ble"?"vLinker MC-IOS":"vLinker MC-Android";
            var result=new TaskCompletionSource<Dictionary<string,object>>();
            var job=new WindowsBluetoothWorkflow(path,binary,mode,allowed=>endpoint=new FixtureWindowsBluetooth(name,transport,allowed));job.OnComplete=v=>result.TrySetResult(v);
            await job.Start();Need(endpoint.Sent.Count==0,"discovery has no adapter command");job.Select("fixture");
            var report=await Finished(result);var observed=BluetoothJson.D(report,"result");
            Need(BluetoothJson.S(report,"status")=="vin_received" && BluetoothJson.B(report,"bluetooth_closed") && !report.ContainsKey("storage_error"),"fresh complete result and confirmed cleanup");
            Need(BluetoothJson.S(observed,"adapter_model")== (product=="evo"?"carista_evo":"vlinker_mc_plus") && endpoint.Sent.Count(c=>c=="021A900000000000")==1 && endpoint.Sent.Last()=="VTPC","shared identity; one VIN read; protocol close");
            if(mode!="vin") {string id=BluetoothJson.S(BluetoothJson.D(report,"saved_report"),"id");string saved=await WindowsBluetoothWorkflow.ReadSaved(path,binary,id);Need(saved.Contains(mode=="ecm_dtc"?"P0107 · failure 00 · status 6F":"Seven ECM identifiers"),"cold report read through shared worker");}
        }
        foreach(string fault in new[]{"cancel","overlap","profile","firmware","disconnect","truncated","release"})
        {
            string path=Path.Combine(root,fault);FixtureWindowsBluetooth endpoint=null;var result=new TaskCompletionSource<Dictionary<string,object>>();
            var job=new WindowsBluetoothWorkflow(path,binary,"ecm_dtc",allowed=>endpoint=new FixtureWindowsBluetooth("Carista EVO","ble",allowed));job.OnComplete=v=>result.TrySetResult(v);await job.Start();
            if(fault=="cancel")await job.Stop();
            else if(fault=="overlap") {bool rejected=false;try {await new WindowsBluetoothWorkflow(path,binary,"vin").Start();}catch(InvalidOperationException){rejected=true;}Need(rejected,"overlapping workflow rejected before I/O");await job.Stop();}
            else
            {
                endpoint.BadProfile=fault=="profile";endpoint.Firmware=fault=="firmware"?"MIC3413 V2.1.0":"";endpoint.Disconnect=fault=="disconnect";endpoint.Truncated=fault=="truncated";endpoint.ConfirmClose=fault!="release";job.Select("fixture");
                if(fault=="release") {for(int i=0;i<100 && !job.ClosurePending;i++)await Task.Delay(20);Need(job.ClosurePending && !result.Task.IsCompleted,"unconfirmed disconnect retains ownership and does not publish success");bool rejected=false;try {await new WindowsBluetoothWorkflow(path,binary,"vin").Start();}catch(InvalidOperationException){rejected=true;}Need(rejected,"unconfirmed native release blocks a new workflow");endpoint.ConfirmClose=true;await job.Stop();}
            }
            var report=await Finished(result);
            Need(BluetoothJson.S(report,"status")!="vin_received" && BluetoothJson.D(report,"result").Count==0 && !Directory.Exists(Path.Combine(path,"dtc-reports")),fault+" cannot create a successful engine report");
            if(fault=="cancel"||fault=="overlap"||fault=="profile"||fault=="firmware")Need(!endpoint.Sent.Contains("021A900000000000"),fault+" sends no ECU request");
        }
        foreach(bool cancel in new[]{false,true})
        {
            string path=Path.Combine(root,cancel?"startup-cancel":"startup");FixtureWindowsBluetooth endpoint=null;var result=new TaskCompletionSource<Dictionary<string,object>>();Dictionary<string,object> vehicle=null;int lookups=0;
            var job=new WindowsBluetoothWorkflow(path,binary,"startup",allowed=>endpoint=new FixtureWindowsBluetooth("Carista EVO","ble",allowed),(uri,maximum,timeout,token)=>{
                Need(endpoint.Sent.Last()=="VTPC","lookup follows protocol close");lookups++;
                string payload="{\"vin\":\"YS3FD49YX41000001\",\"success\":true,\"vehicle\":{\"model_year\":2004,\"platform\":\"Saab 9-3 Sport Sedan\",\"engine\":\"B207R\",\"gearbox\":\"FA57\",\"body_display\":\"Sedan\"}}";
                return Task.FromResult(new KeyValuePair<int,string>(200,payload));});job.OnComplete=v=>result.TrySetResult(v);job.OnVehicle=v=>vehicle=v;
            await job.Start();if(cancel)await job.Stop();else job.Select("fixture");var report=await Finished(result);
            Need(cancel?BluetoothJson.S(report,"status")=="cancelled" && lookups==0 && endpoint.Sent.Count==0:BluetoothJson.S(report,"status")=="identified" && lookups==1 && !BluetoothJson.B(vehicle,"vehicle_connected") && !BluetoothJson.B(vehicle,"firmware_transport_available"),"startup cancellation/fresh identity scope");
            using(var worker=new WindowsConnectionWorker(binary,path)) {var reopened=await worker.Call("connection",BluetoothJson.V("op","tick"));Need(BluetoothJson.D(reopened,"vehicle").Count==0,"reopen does not restore a current vehicle from history");}
            var receipts=Directory.GetFiles(Path.Combine(path,"connection-attempts"),"*.json");Need(receipts.Length>0 && !File.ReadAllText(receipts[0]).Contains("IN_PROGRESS"),"terminal startup receipt");
        }
        {
            var done=new TaskCompletionSource<Dictionary<string,object>>();bool opened=false;
            var job=new WindowsBluetoothWorkflow(Path.Combine(root,"cancel-starting"),binary,"startup",allowed=>{opened=true;return new FixtureWindowsBluetooth("Carista EVO","ble",allowed);});job.OnComplete=v=>done.TrySetResult(v);
            var starting=job.Start();await job.Stop();await starting;
            Need(BluetoothJson.S(await Finished(done),"status")=="cancelled" && !opened,"cancel during worker startup cannot open native I/O or publish a late vehicle");
        }
        return checks;
    }
}
