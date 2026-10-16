// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;

static class TransportTests
{
    static void Need(bool value, string name) { if (!value) throw new Exception("Transport test: " + name); }
    static void Reject(Action action, string name)
    {
        bool rejected = false;
        try { action(); } catch (ArgumentException) { rejected = true; }
        catch (InvalidOperationException) { rejected = true; } catch (IOException) { rejected = true; }
        catch (InvalidDataException) { rejected = true; }
        Need(rejected, name);
    }
    public static void Run()
    {
        Need(AdapterProfile.NanoCandidate("USB-Enhanced-SERIAL CH343 (COM10)", @"USB\VID_1A86&PID_55D3\SYNTHETIC", true), "CH343 candidate with installed vendor driver");
        Need(AdapterProfile.NanoCandidate(null, @"usb\vid_1a86&pid_55d3&mi_00\SYNTHETIC", true), "case-insensitive composite candidate");
        Need(AdapterProfile.NanoCandidate("VCX Nano", null, true), "legacy vendor name retained");
        Need(!AdapterProfile.NanoCandidate("CH343", @"USB\VID_1A86&PID_55D3\SYNTHETIC", false), "generic candidate requires vendor driver");
        foreach (string id in new[] { null, @"USB\VID_1A86&PID_55D4\SYNTHETIC", @"USB\VID_1A86&PID_55D30\SYNTHETIC", @"PCI\VID_1A86&PID_55D3\SYNTHETIC", @"USB\VID_0483&PID_5740\SYNTHETIC" })
            Need(!AdapterProfile.NanoCandidate("CH343", id, true), "unrelated hardware is not a Nano candidate");
        var identity = new ProbeResult { Code = 0, Text = "ADAPTER_IDENTITY=VCX-NANO\nPassThruClose rc=0x00000000\n" };
        Need(identity.NanoIdentityConfirmed, "identity requires successful close");
        identity.Code = 124;
        Need(!identity.NanoIdentityConfirmed, "timed-out identity cannot authorize a VIN query");
        identity.Code = 0;
        foreach (string text in new[] { "ADAPTER_IDENTITY=VCX-NANO\n", "PassThruClose rc=0x00000000\n", "ADAPTER_IDENTITY=VCX-NANO-OTHER\nPassThruClose rc=0x00000000\n", "ADAPTER_IDENTITY=VCX-NANO\nPassThruClose rc=0x00000000\nPassThruClose rc=0x00000001\n", "ADAPTER_IDENTITY=VCX-NANO\nCALL PassThruConnect\nPassThruClose rc=0x00000000\n", "ADAPTER_IDENTITY=VCX-NANO\nREQUEST CAN=7E0\nPassThruClose rc=0x00000000\n" })
        {
            identity.Text = text;
            Need(!identity.NanoIdentityConfirmed, "missing identity, cleanup failure or CAN activity rejects identity-only preflight");
        }
        Need(AdapterFailure.Parse("{\"adapter_failure\":{\"kind\":\"single_wire_wake_failed\",\"title\":\"Single-wire CAN wake-up failed\",\"message\":\"Check pin 1\"}}").Kind == "single_wire_wake_failed", "shared adapter failure presented from report.json");
        Need(AdapterFailure.Parse("{\"status\":\"incomplete\",\"adapter_failure\":null}") == null, "report without adapter failure is a plain session end");
        Need(AdapterFailure.Parse("not json") == null && AdapterFailure.Parse("{\"adapter_failure\":{\"kind\":1}}") == null, "malformed failure is not misreported");
        string snapshots = Path.Combine(Path.GetTempPath(), "opensaab-snapshot-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(snapshots);
        try
        {
            string current = Path.Combine(snapshots, "live.ppm"), pending = current + ".tmp";
            File.WriteAllText(current, "previous");
            using (var reader = SessionSnapshot.Open(current))
            {
                File.WriteAllText(pending, "next");
                File.Replace(pending, current, null);
                using (var text = new StreamReader(reader))
                    Need(text.ReadToEnd() == "previous", "open snapshot remains consistent during replacement");
                Need(SessionSnapshot.Text(current) == "next", "concurrent emulator replacement is allowed");
            }
        }
        finally { Directory.Delete(snapshots, true); }
        Need(TransportPlan.Default == WindowsTransport.ChipsoftDirect, "direct transport is the default");
        Need(TransportPlan.Label(WindowsTransport.VendorJ2534).StartsWith(TransportPlan.OemComparisonLabel), "vendor route labelled Experimental OEM comparison");
        var oem = TransportPlan.Arguments("chipsoft-pro", null, WindowsTransport.VendorJ2534);
        Need(oem.Contains("--candi-j2534-local") && !oem.Contains("--candi-chipsoft-port"), "OEM path never selects direct VIN/firmware route");
        var direct = TransportPlan.Arguments("chipsoft-pro", "COM7", WindowsTransport.ChipsoftDirect);
        Need(direct.Contains("--candi-chipsoft-port COM7") && direct.Contains("--interactive-headless") && !direct.Contains("--test-harness") && !direct.Contains("j2534"), "direct path uses interactive CDC route only");
        Need(TransportPlan.Arguments("nano", null, WindowsTransport.VendorJ2534).EndsWith("--candi-j2534-adapter nano"), "Nano OEM profile retained");
        Need(TransportPlan.Arguments(null, null, WindowsTransport.ChipsoftDirect) == "--interactive-headless", "offline ignores transport selection");
        Reject(() => TransportPlan.Arguments("nano", "COM7", WindowsTransport.ChipsoftDirect), "Nano cannot silently use Chipsoft direct");
        Reject(() => TransportPlan.Arguments("other", null, WindowsTransport.VendorJ2534), "unknown adapter rejected");
        foreach (var port in new[] { null, "COM7 --extra", "COM7\n", "COM1000", "C:\\file" })
            Reject(() => TransportPlan.Arguments("chipsoft-pro", port, WindowsTransport.ChipsoftDirect), "invalid port rejected");
        Need(!TransportPlan.UsesNativeMailbox(true, WindowsTransport.VendorJ2534), "OEM uses the shared interactive mailbox");
        Need(oem.Contains("--interactive-headless") && !oem.Contains("--test-harness"), "OEM publishes live frames and accepts context-sensitive Enter without automatic navigation");
        Need(!TransportPlan.UsesNativeMailbox(true, WindowsTransport.ChipsoftDirect), "direct mailbox");
        Need(!TransportPlan.UsesNativeMailbox(false, WindowsTransport.VendorJ2534), "offline mailbox");
        string file = Path.Combine(Path.GetTempPath(), "opensaab-pe-" + Guid.NewGuid().ToString("N"));
        try
        {
            Reject(() => TransportPlan.RequireX86(file), "missing driver does not fall back");
            File.WriteAllBytes(file, new byte[64]);
            Reject(() => TransportPlan.RequireX86(file), "non-PE rejected");
            using (var w = new BinaryWriter(File.Create(file)))
            {
                w.Write((ushort)0x5a4d); w.BaseStream.Position = 0x3c; w.Write(64);
                w.Write((uint)0x4550); w.Write((ushort)0x14c);
            }
            TransportPlan.RequireX86(file);
            foreach (ushort machine in new ushort[] { 0x8664, 0xaa64 })
            {
                using (var w = new BinaryWriter(File.OpenWrite(file))) { w.BaseStream.Position = 68; w.Write(machine); }
                Reject(() => TransportPlan.RequireX86(file), "non-x86 DLL rejected");
            }
            using (var w = new BinaryWriter(File.OpenWrite(file))) { w.BaseStream.Position = 0x3c; w.Write(int.MaxValue); }
            Reject(() => TransportPlan.RequireX86(file), "truncated PE rejected");
        }
        finally { if (File.Exists(file)) File.Delete(file); }
    }
}
