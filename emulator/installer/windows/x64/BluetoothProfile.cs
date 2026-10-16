// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;

sealed class BluetoothEndpoint
{
    public string Id, Name, Transport; public ulong Address; public int AddressType;
    public override string ToString() { return Name + " · " + (Transport == "ble" ? "Bluetooth LE" : "Paired Bluetooth serial"); }
}
interface IWindowsBluetoothLink
{
    void Open(Action<Dictionary<string, object>> callback);
    void Send(Dictionary<string, object> request);
}
static class WindowsBluetoothProfile
{
    public const string Service = "18F0", Write = "2AF1", Notify = "2AF0";
    public const string Serial = "00001101-0000-1000-8000-00805f9b34fb";
    public static Guid UUID(string value) { return new Guid("0000" + value + "-0000-1000-8000-00805f9b34fb"); }
    public static bool Known(string name, bool ble)
    {
        string value = (name ?? "").Trim().ToLowerInvariant();
        return value == "carista evo" || (ble ? value == "vlinker mc-ios" : value == "vlinker mc" || value == "vlinker mc-android");
    }
    public static bool Accepts(Dictionary<string, object> v)
    {
        bool ble = BluetoothJson.S(v, "transport") == "ble";
        return Known(BluetoothJson.S(v, "device_name"), ble) && (ble
            ? BluetoothJson.S(v,"service") == Service && BluetoothJson.S(v,"write") == Write && BluetoothJson.S(v,"notify") == Notify && BluetoothJson.S(v,"write_type") == "withResponse"
            : BluetoothJson.S(v,"transport") == "windows-classic-spp" && BluetoothJson.S(v,"uuid") == Serial);
    }
}
