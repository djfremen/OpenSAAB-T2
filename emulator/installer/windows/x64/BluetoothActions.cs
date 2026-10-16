// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using System.Windows.Forms;

sealed partial class SessionWindow
{
    WindowsBluetoothWorkflow bluetoothJob;
    Dictionary<string,object> bluetoothVehicle=BluetoothJson.V();
    Form bluetoothPicker;
    bool closeAfterBluetooth;
    void ClearBluetoothVehicle() { bluetoothVehicle=BluetoothJson.V(); }
    async Task ChooseConnection()
    {
        if(busy || Running)return;
        Popup(new[]{"Carista EVO / vLinker MC+ · Bluetooth","Chipsoft Pro / VCX Nano · USB"},new Action[]{async()=>await BluetoothRead("startup"),async()=>await Connect()});
        await Task.FromResult(0);
    }
    async Task BluetoothRead(string mode)
    {
        if(busy || Running)return;
        if(mode=="startup" && firmware.Missing()!="") {FirmwareDialog();return;}
        adapter=null;vin="";engineCodes="";ClearBluetoothVehicle();vehicle.Text="No current vehicle identity · Connect for a fresh VIN";
        SetBusy(true);
        var job=new WindowsBluetoothWorkflow(state,Path.Combine(root,"opensaab-connection.exe"),mode);bluetoothJob=job;
        var picker=Dialog("Choose Bluetooth adapter",620,365);bluetoothPicker=picker;
        var list=new ListBox {Dock=DockStyle.Fill,AccessibleName="Bluetooth adapters"};
        var message=Label("Searching for Carista EVO and vLinker MC+…");message.Dock=DockStyle.Top;message.Height=90;
        var buttons=new FlowLayoutPanel {Dock=DockStyle.Bottom,Height=60,FlowDirection=FlowDirection.LeftToRight};
        var select=Button("Use this adapter",()=>{var endpoint=list.SelectedItem as BluetoothEndpoint;if(endpoint==null)return;list.Enabled=false;job.Select(endpoint.Id);});select.Width=185;
        var scan=Button("Scan again",()=>job.Find());scan.Width=155;
        var cancel=Button("Cancel",async()=>await job.Stop());cancel.Width=125;
        buttons.Controls.Add(select);buttons.Controls.Add(scan);buttons.Controls.Add(cancel);
        picker.Controls.Add(list);picker.Controls.Add(message);picker.Controls.Add(buttons);
        picker.FormClosing += async delegate(object sender,FormClosingEventArgs e) {if(bluetoothJob==job){e.Cancel=true;await job.Stop();}};
        job.OnDevices=endpoints=>{if(picker.IsDisposed)return;string id=list.SelectedItem is BluetoothEndpoint?((BluetoothEndpoint)list.SelectedItem).Id:"";list.BeginUpdate();list.Items.Clear();list.Items.AddRange(endpoints);var same=endpoints.FirstOrDefault(d=>d.Id==id);if(same!=null)list.SelectedItem=same;list.EndUpdate();};
        job.OnStatus=text=>{status.Text=text;if(!picker.IsDisposed)message.Text=text;};
        job.OnVehicle=value=>{
            bluetoothVehicle=BluetoothJson.S(value,"state")=="identified"?BluetoothJson.D(value,"vehicle"):BluetoothJson.V();
            string fresh=BluetoothJson.S(bluetoothVehicle,"vin");
            vehicle.Text=fresh==""?"No current vehicle identity · Connect for a fresh VIN":"VIN: "+fresh+"\n"+BluetoothJson.S(bluetoothVehicle,"adapter_label")+" · "+BluetoothJson.S(bluetoothVehicle,"adapter_voltage");
        };
        job.OnComplete=report=>{
            bluetoothJob=null;bluetoothPicker=null;SetBusy(false);picker.Close();picker.Dispose();
            status.Text=BluetoothJson.S(report,"message");if(BluetoothJson.S(report,"storage_error")!="")status.Text+=" · Report could not be saved: "+BluetoothJson.S(report,"storage_error");
            recentConsole=BluetoothJson.JSON.Serialize(report);
            if(closeAfterBluetooth) {closing=true;Close();return;}
            if(mode=="ecm_info"||mode=="ecm_dtc") {string text=BluetoothJson.S(BluetoothJson.D(report,"result"),"display");if(text!="")TextDialog(mode=="ecm_info"?"Read ECM information — HS-CAN":"Read engine codes — HS-CAN",text);}
        };
        picker.Show(this);
        try {await job.Start();}
        catch(Exception e) {bluetoothJob=null;bluetoothPicker=null;SetBusy(false);picker.Close();picker.Dispose();Error(e);}
    }
    void BluetoothTools()
    {
        Popup(new[]{"Carista EVO / vLinker MC+ · Bluetooth","Read ECM VIN","Read ECM information — HS-CAN","Read engine codes — HS-CAN","Chipsoft Pro / VCX Nano · USB","Saved DTC reports"},new Action[]{
            async()=>await BluetoothRead("identity"),async()=>await BluetoothRead("vin"),async()=>await BluetoothRead("ecm_info"),async()=>await BluetoothRead("ecm_dtc"),AdapterTools,async()=>await SavedBluetoothReports()});
    }
    async Task SavedBluetoothReports()
    {
        if(busy)return;
        string directory=Path.Combine(state,"dtc-reports");
        var ids=Directory.Exists(directory)?Directory.GetFiles(directory,"hscan_ecm_*.json").Select(Path.GetFileNameWithoutExtension).OrderByDescending(s=>s).ToArray():new string[0];
        if(ids.Length==0) {if(lastReport!="")TextDialog("Saved DTC reports",lastReport);else {Directory.CreateDirectory(Path.Combine(state,"reports"));System.Diagnostics.Process.Start("explorer.exe",EmulatorSession.Quote(Path.Combine(state,"reports")));}return;}
        using(var picker=Dialog("Saved DTC reports",680,330))
        {
            var list=new ListBox {Dock=DockStyle.Fill};list.Items.Add("Original firmware and USB reports");list.Items.AddRange(ids);picker.Controls.Add(list);
            var open=Button("Open saved report",async()=>{string id=list.SelectedItem as string;if(id==null)return;if(list.SelectedIndex==0){Directory.CreateDirectory(Path.Combine(state,"reports"));System.Diagnostics.Process.Start("explorer.exe",EmulatorSession.Quote(Path.Combine(state,"reports")));return;}try {TextDialog("Saved DTC reports",await WindowsBluetoothWorkflow.ReadSaved(state,Path.Combine(root,"opensaab-connection.exe"),id));}catch(Exception e){Error(e);}});open.Dock=DockStyle.Bottom;open.Height=52;picker.Controls.Add(open);picker.ShowDialog(this);
        }
        await Task.FromResult(0);
    }
    string BluetoothDetails()
    {
        if(bluetoothVehicle.Count==0)return "";
        var lines=new[]{"vin","adapter_label","adapter_voltage","model_year","platform","engine","gearbox","body_display"}.Select(key=>{object value;return bluetoothVehicle.TryGetValue(key,out value)?key.Replace('_',' ')+": "+Convert.ToString(value):"";}).Where(s=>s!="");
        return "\n\nCurrent Bluetooth identification\n"+string.Join("\n",lines)+"\nAdapter connection closed. Original firmware transport, SW-CAN/I-bus and ECM add/remove are unavailable through this Bluetooth preview.";
    }
}
