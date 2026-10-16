// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Devices.Bluetooth;
using Windows.Devices.Bluetooth.Advertisement;
using Windows.Devices.Bluetooth.GenericAttributeProfile;
using Windows.Devices.Bluetooth.Rfcomm;
using Windows.Devices.Enumeration;
using Windows.Networking.Sockets;
using Windows.Storage.Streams;

// Native I/O only. The Rust worker grants each write and validates all replies.
// An endpoint is selected explicitly; a name is only a discovery hint.
sealed class WindowsBluetoothLink : IWindowsBluetoothLink
{
    readonly Func<string,bool> allowed;
    readonly Dictionary<string,BluetoothEndpoint> endpoints = new Dictionary<string,BluetoothEndpoint>();
    readonly List<byte[]> deferred = new List<byte[]>();
    SynchronizationContext ui; Action<Dictionary<string,object>> callback;
    BluetoothLEAdvertisementWatcher watcher; CancellationTokenSource lifetime = new CancellationTokenSource();
    BluetoothLEDevice ble; BluetoothDevice classic; GattDeviceService service; GattSession gatt;
    GattCharacteristic tx, rx; RfcommDeviceService serial; StreamSocket socket; DataReader reader; DataWriter writer;
    BluetoothEndpoint selected; Task opening, receiving, discovery; bool closing, closed, ready, writing, closeInProgress; string closeError = "";
    int generation; bool everOpened;
    Semaphore owner;
    public WindowsBluetoothLink(Func<string,bool> allowed) { this.allowed = allowed; }
    void Post(Action action) { ui.Post(delegate { action(); }, null); }
    void Emit(Dictionary<string,object> value) { if (!closed || BluetoothJson.S(value,"type") == "closed") callback(value); }
    void Error(string message) { if (!closing && !closed) Emit(BluetoothJson.V("type","error","message",message)); }
    public void Open(Action<Dictionary<string,object>> callback)
    {
        if (ui != null) throw new InvalidOperationException("Bluetooth transport already started");
        ui = SynchronizationContext.Current;
        if (ui == null) throw new InvalidOperationException("Windows Bluetooth must start on the UI thread");
        owner=new Semaphore(1,1,@"Local\OpenSAAB-Native-Bluetooth-Session");
        if(!owner.WaitOne(0)) {owner.Dispose();owner=null;throw new InvalidOperationException("Another OpenSAAB process owns Bluetooth. Finish its operation before connecting.");}
        this.callback = callback; Scan();
    }
    async void Scan()
    {
        if (closing || selected != null || discovery != null && !discovery.IsCompleted) return;
        int scan = ++generation;
        discovery = Discover(scan);
        try { await discovery; } catch (OperationCanceledException) { } catch (Exception e) { Error("Windows Bluetooth discovery unavailable: " + e.Message); }
    }
    async Task Discover(int scan)
    {
        StopScan();
        var adapter = await BluetoothAdapter.GetDefaultAsync().AsTask(lifetime.Token);
        if (closing || scan != generation) return;
        if (adapter == null) throw new InvalidOperationException("No Bluetooth radio is exposed to Windows. Fusion requires a guest Bluetooth device and compatible driver.");
        var radio = await adapter.GetRadioAsync().AsTask(lifetime.Token);
        if (radio == null || radio.State != Windows.Devices.Radios.RadioState.On) throw new InvalidOperationException("Turn on Bluetooth in Windows, then start a fresh connection.");
        endpoints.Clear(); Candidates();
        watcher = new BluetoothLEAdvertisementWatcher { ScanningMode = BluetoothLEScanningMode.Active };
        watcher.Received += delegate(BluetoothLEAdvertisementWatcher sender, BluetoothLEAdvertisementReceivedEventArgs args)
        {
            string name = args.Advertisement.LocalName;
            if (!WindowsBluetoothProfile.Known(name,true)) return;
            ulong address = args.BluetoothAddress; int addressType=(int)args.BluetoothAddressType;
            Post(delegate {
                if (closing || selected != null || scan != generation) return;
                string id = "ble:" + address.ToString("X12");
                endpoints[id] = new BluetoothEndpoint { Id=id, Name=name, Transport="ble", Address=address, AddressType=addressType }; Candidates();
            });
        };
        watcher.Stopped += delegate(BluetoothLEAdvertisementWatcher sender, BluetoothLEAdvertisementWatcherStoppedEventArgs args)
        {
            if (args.Error != BluetoothError.Success) Post(delegate { if (scan == generation) Error("Windows BLE scan stopped: " + args.Error); });
        };
        watcher.Start();
        Emit(BluetoothJson.V("type","discovery_status","message","Searching for Carista EVO and vLinker MC+… Select the exact adapter and transport."));
        var paired = await DeviceInformation.FindAllAsync(BluetoothDevice.GetDeviceSelectorFromPairingState(true)).AsTask(lifetime.Token);
        if (closing || scan != generation || selected != null) return;
        foreach (var device in paired) if (WindowsBluetoothProfile.Known(device.Name,false))
            endpoints["classic:"+device.Id] = new BluetoothEndpoint { Id="classic:"+device.Id, Name=device.Name, Transport="windows-classic-spp" };
        Candidates();
        await Task.Delay(15000,lifetime.Token);
        if (closing || scan != generation || selected != null) return;
        StopScan(); Emit(BluetoothJson.V("type","discovery_status","message",endpoints.Count == 0 ? "No supported Bluetooth adapter found. Pair serial adapters in Windows settings; close other diagnostic apps, then Scan again." : "Scan complete. Choose an adapter or Cancel."));
    }
    void Candidates() { Emit(BluetoothJson.V("type","candidates","devices",endpoints.Values.OrderBy(e=>e.Id).ToArray())); }
    void StopScan() { if (watcher != null) { if (watcher.Status == BluetoothLEAdvertisementWatcherStatus.Started) watcher.Stop(); watcher = null; } }
    public async void Send(Dictionary<string,object> request)
    {
        try
        {
            string op = BluetoothJson.S(request,"op");
            if (op == "close") { await Close(); return; }
            if (closing || closed) return;
            if (op == "scan") { Scan(); return; }
            if (op == "select")
            {
                BluetoothEndpoint candidate;
                if (selected != null || !endpoints.TryGetValue(BluetoothJson.S(request,"id"),out candidate)) throw new InvalidOperationException("Select a currently discovered Bluetooth endpoint");
                selected = candidate; ++generation; StopScan();
                Emit(BluetoothJson.V("type","selected","id",selected.Id));
                opening = Connect(); if(await Task.WhenAny(opening,Task.Delay(15000))!=opening)throw new InvalidOperationException("Windows Bluetooth opening timed out; adapter cleanup must finish before retrying"); await opening; return;
            }
            if (op != "write") throw new InvalidOperationException("Unsupported Bluetooth transport request");
            string command = BluetoothJson.S(request,"command");
            if (!ready || writing || !allowed(command) || command.Contains("\r") || command.Contains("\n")) throw new InvalidOperationException("Unsupported or overlapping Bluetooth write");
            writing = true;
            byte[] bytes = Encoding.ASCII.GetBytes(command+"\r");
            try
            {
                if (ble != null)
                {
                    int maximum = gatt.MaxPduSize-3;
                    if(maximum<1)throw new InvalidOperationException("Bluetooth write size is unavailable");
                    for (int at=0;at<bytes.Length;at+=maximum)
                    {
                        var chunk = bytes.Skip(at).Take(maximum).ToArray();
                        var response = await tx.WriteValueWithResultAsync(chunk.AsBuffer(),GattWriteOption.WriteWithResponse).AsTask(lifetime.Token);
                        if (response.Status != GattCommunicationStatus.Success) throw new InvalidOperationException("Bluetooth GATT write failed: " + response.Status);
                    }
                }
                else { writer.WriteBytes(bytes); uint count = await writer.StoreAsync().AsTask(lifetime.Token); if (count != bytes.Length) throw new InvalidOperationException("Incomplete Bluetooth serial write"); }
                if (!closing)
                {
                    writing = false;
                    Emit(BluetoothJson.V("type","write_completed","command",command,"completion_scope",ble != null ? "gatt-response" : "socket-store"));
                    var queued = deferred.ToArray(); deferred.Clear(); foreach(var packet in queued) { if (closing) break; Receive(packet); }
                }
            }
            finally { writing = false; }
        }
        catch (OperationCanceledException) { if (!closing) Error("Windows Bluetooth operation cancelled; start a fresh connection"); }
        catch (Exception e) { Error(e.Message); }
    }
    async Task Connect()
    {
        if (selected.Transport == "ble")
        {
            ble = await BluetoothLEDevice.FromBluetoothAddressAsync(selected.Address,(BluetoothAddressType)selected.AddressType).AsTask(lifetime.Token);
            if (ble == null) throw new InvalidOperationException("BLE access was not granted or adapter is unavailable");
            if (closing) return;
            if (ble.ConnectionStatus == BluetoothConnectionStatus.Connected) throw new InvalidOperationException("Selected adapter already has a Windows connection. Close other diagnostic apps first.");
            gatt = await GattSession.FromDeviceIdAsync(ble.BluetoothDeviceId).AsTask(lifetime.Token);
            if (closing) return;
            everOpened = true;
            var services = await ble.GetGattServicesForUuidAsync(WindowsBluetoothProfile.UUID(WindowsBluetoothProfile.Service),BluetoothCacheMode.Uncached).AsTask(lifetime.Token);
            if (services.Status != GattCommunicationStatus.Success || services.Services.Count != 1) { foreach(var s in services.Services) s.Dispose(); throw new InvalidOperationException("Selected BLE service unavailable"); }
            service = services.Services[0];
            if (closing) return;
            var values = await service.GetCharacteristicsAsync(BluetoothCacheMode.Uncached).AsTask(lifetime.Token);
            if (values.Status != GattCommunicationStatus.Success) throw new InvalidOperationException("BLE characteristics unavailable");
            tx = values.Characteristics.FirstOrDefault(c=>c.Uuid == WindowsBluetoothProfile.UUID(WindowsBluetoothProfile.Write) && c.CharacteristicProperties.HasFlag(GattCharacteristicProperties.Write));
            rx = values.Characteristics.FirstOrDefault(c=>c.Uuid == WindowsBluetoothProfile.UUID(WindowsBluetoothProfile.Notify) && c.CharacteristicProperties.HasFlag(GattCharacteristicProperties.Notify));
            if (tx == null || rx == null) throw new InvalidOperationException("Selected adapter lacks acknowledged writes or notifications");
            rx.ValueChanged += Notification;
            var subscription = await rx.WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue.Notify).AsTask(lifetime.Token);
            if (subscription != GattCommunicationStatus.Success) throw new InvalidOperationException("BLE notification subscription failed");
            if (closing) return;
            ready = true;
            Emit(BluetoothJson.V("type","ready","native_api","Windows.Devices.Bluetooth","transport","ble","device_name",selected.Name,"device_id",selected.Id,"service",WindowsBluetoothProfile.Service,"write",WindowsBluetoothProfile.Write,"notify",WindowsBluetoothProfile.Notify,"write_type","withResponse"));
        }
        else
        {
            classic = await BluetoothDevice.FromIdAsync(selected.Id.Substring(8)).AsTask(lifetime.Token);
            if (classic == null) throw new InvalidOperationException("Paired Bluetooth adapter unavailable");
            selected.Address = classic.BluetoothAddress;
            if (closing) return;
            if (classic.ConnectionStatus == BluetoothConnectionStatus.Connected) throw new InvalidOperationException("Selected adapter is already connected. Close other diagnostic apps first.");
            var services = await classic.GetRfcommServicesForIdAsync(RfcommServiceId.SerialPort,BluetoothCacheMode.Uncached).AsTask(lifetime.Token);
            if (services.Error != BluetoothError.Success || services.Services.Count != 1) { foreach(var s in services.Services) s.Dispose(); throw new InvalidOperationException("Paired Serial Port service unavailable"); }
            serial = services.Services[0]; socket = new StreamSocket();
            if (closing) return;
            everOpened = true;
            await socket.ConnectAsync(serial.ConnectionHostName,serial.ConnectionServiceName,SocketProtectionLevel.BluetoothEncryptionAllowNullAuthentication).AsTask(lifetime.Token);
            if (closing) return;
            writer = new DataWriter(socket.OutputStream); reader = new DataReader(socket.InputStream) { InputStreamOptions=InputStreamOptions.Partial };
            ready = true; receiving = ReadSerial();
            Emit(BluetoothJson.V("type","ready","native_api","Windows.Devices.Bluetooth.Rfcomm","transport","windows-classic-spp","device_name",selected.Name,"device_id",selected.Id,"uuid",WindowsBluetoothProfile.Serial));
        }
    }
    void Notification(GattCharacteristic sender,GattValueChangedEventArgs args)
    {
        byte[] bytes = args.CharacteristicValue.ToArray(); Post(delegate { if (!closing && sender == rx) Receive(bytes); });
    }
    async Task ReadSerial()
    {
        try { while (!closing) { uint count = await reader.LoadAsync(4096).AsTask(lifetime.Token); if (count == 0) throw new InvalidOperationException("Bluetooth serial disconnected"); byte[] bytes = new byte[count]; reader.ReadBytes(bytes); Receive(bytes); } }
        catch (OperationCanceledException) { } catch(Exception e) { Error(e.Message); }
    }
    void Receive(byte[] bytes)
    {
        if (closing || !ready) return;
        if (bytes.Length > 16384 || writing && deferred.Sum(v=>v.Length)+bytes.Length > 16384) { Error("Bluetooth receive buffer exceeded limit"); return; }
        if (writing) deferred.Add(bytes); else Emit(BluetoothJson.V("type","rx","bytes",bytes.Select(b=>(int)b).ToArray()));
    }
    async Task Close()
    {
        if (closed || closeInProgress) return;
        closeInProgress = true; closing = true; ++generation; StopScan(); lifetime.Cancel();
        try
        {
            // Await native operations before disposing their handles. Late events never reach Rust.
            var pending = new[]{opening,receiving,discovery}.Where(t=>t != null && !t.IsCompleted).ToArray();
            if (pending.Length > 0) { var all = Task.WhenAll(pending); if (await Task.WhenAny(all,Task.Delay(10000)) != all) { Unconfirmed("Windows Bluetooth operation has not finished cancelling"); return; } try { await all; } catch { } }
            ready = false; deferred.Clear();
            if (rx != null) rx.ValueChanged -= Notification; rx = null; tx = null;
            if (reader != null) { reader.DetachStream(); reader.Dispose(); reader = null; }
            if (writer != null) { writer.DetachStream(); writer.Dispose(); writer = null; }
            if (socket != null) { socket.Dispose(); socket = null; }
            if (serial != null) { serial.Dispose(); serial = null; }
            if (service != null) { service.Dispose(); service = null; }
            if (gatt != null) { gatt.MaintainConnection = false; gatt.Dispose(); gatt = null; }
            if (ble != null) { ble.Dispose(); ble = null; }
            if (classic != null) { classic.Dispose(); classic = null; }
            if (everOpened)
            {
                // Query endpoint metadata without constructing a new connection-owning device.
                // Dispose alone does not prove Windows released the physical link.
                string address = selected.Transport == "ble" ? BluetoothLEDevice.GetDeviceSelectorFromBluetoothAddress(selected.Address,(BluetoothAddressType)selected.AddressType) : BluetoothDevice.GetDeviceSelectorFromBluetoothAddress(selected.Address);
                string disconnected = selected.Transport == "ble" ? BluetoothLEDevice.GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus.Disconnected) : BluetoothDevice.GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus.Disconnected);
                bool confirmed = false;
                for(int i=0;i<16;i++)
                {
                    var query = DeviceInformation.FindAllAsync("("+address+") AND ("+disconnected+")").AsTask();
                    if (await Task.WhenAny(query,Task.Delay(2000)) != query) { Unconfirmed("Windows disconnect observation timed out"); return; }
                    if ((await query).Count > 0) { confirmed = true; break; }
                    await Task.Delay(500);
                }
                if (!confirmed) { Unconfirmed("Windows has not confirmed Bluetooth disconnection. Close other diagnostic apps, then Stop to check again."); return; }
            }
            closed = true; if(owner!=null){owner.Release();owner.Dispose();owner=null;} Emit(BluetoothJson.V("type","closed","bluetooth_closed",true,"native_handles_closed",true,"error",closeError)); lifetime.Dispose();
        }
        catch(Exception e) { Unconfirmed("Windows Bluetooth closure is unconfirmed: " + e.Message); }
        finally { closeInProgress = false; }
    }
    void Unconfirmed(string message) { Emit(BluetoothJson.V("type","closure_pending","bluetooth_closed",false,"message",message)); }
}
