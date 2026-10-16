// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;
using System.Text.RegularExpressions;

enum WindowsTransport { VendorJ2534, ChipsoftDirect }

// Windows binding only: protocol and diagnostic behavior remain in the shared core.
static class TransportPlan
{
    // Product direction: direct Chipsoft protocol on every platform. The vendor J2534 route exists
    // only for A/B comparison on internal builds and is never a supported adapter path.
    public const WindowsTransport Default = WindowsTransport.ChipsoftDirect;
    public const string OemComparisonLabel = "Experimental \u2014 OEM comparison";
    public static string Label(WindowsTransport mode)
    {
        return mode == WindowsTransport.VendorJ2534 ? OemComparisonLabel + " (vendor 32-bit J2534)" : "OpenSAAB direct USB/CDC";
    }
    public static string Arguments(string adapter, string port, WindowsTransport mode)
    {
        if (adapter == null) return "--interactive-headless";
        if (adapter != "chipsoft-pro" && adapter != "nano") throw new ArgumentException("Unknown adapter");
        if (mode == WindowsTransport.VendorJ2534)
            return "--interactive-headless --harness-target native-manual --candi-j2534-local --candi-j2534-adapter " + adapter;
        if (mode != WindowsTransport.ChipsoftDirect || adapter != "chipsoft-pro")
            throw new InvalidOperationException("Direct transport is available for Chipsoft Pro only. VCX Nano is not supported; the Experimental \u2014 OEM comparison route is for internal A/B testing only.");
        if (!Regex.IsMatch(port ?? "", @"\ACOM[0-9]{1,3}\z"))
            throw new IOException("Select a connected Chipsoft COM port before starting direct transport.");
        return "--interactive-headless --harness-target native-manual --candi-chipsoft-port " + port;
    }
    public static bool UsesNativeMailbox(bool live, WindowsTransport mode)
    {
        // Both transports share live LCD publication and context-sensitive Enter.
        return false;
    }
    public static void RequireX86(string path)
    {
        if (!File.Exists(path)) throw new IOException("Required 32-bit driver/helper is missing: " + Path.GetFileName(path));
        using (var reader = new BinaryReader(File.OpenRead(path)))
        {
            if (reader.BaseStream.Length < 64 || reader.ReadUInt16() != 0x5a4d) throw new InvalidDataException("Invalid Windows driver/helper image");
            reader.BaseStream.Position = 0x3c;
            int offset = reader.ReadInt32();
            if (offset < 64 || offset > reader.BaseStream.Length - 6) throw new InvalidDataException("Invalid Windows image header");
            reader.BaseStream.Position = offset;
            if (reader.ReadUInt32() != 0x4550 || reader.ReadUInt16() != 0x14c)
                throw new InvalidDataException("The OEM driver path requires an x86 DLL and x86 helper; an x64/ARM64 image cannot be loaded there.");
        }
    }
}
