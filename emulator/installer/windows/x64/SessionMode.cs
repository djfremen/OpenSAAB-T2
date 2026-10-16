// SPDX-License-Identifier: MPL-2.0
using System;
using System.Diagnostics;
using System.IO;

// Windows binding to the same shared Chipsoft modes used by Android and macOS.
enum ChipsoftSessionMode { FullNative, ReadOnly, Seeds }
static class SessionMode
{
    public const ChipsoftSessionMode Default = ChipsoftSessionMode.FullNative;
    public static string Label(ChipsoftSessionMode mode)
    {
        switch (mode) {
            case ChipsoftSessionMode.FullNative: return "Full native control";
            case ChipsoftSessionMode.Seeds: return "Collect security seeds";
            default: return "Read-only diagnostics";
        }
    }
    public static string Arguments(bool live, WindowsTransport transport, ChipsoftSessionMode mode)
    {
        if (!live) return "";
        if (transport == WindowsTransport.VendorJ2534 && mode != ChipsoftSessionMode.ReadOnly)
            throw new InvalidOperationException("The OEM comparison route supports Read-only diagnostics. Select OpenSAAB direct USB/CDC for seed collection or full native control.");
        return mode == ChipsoftSessionMode.Seeds ? " --candi-chipsoft-seeds" : "";
    }
    public static void EnvironmentFor(ProcessStartInfo info, bool live, WindowsTransport transport, ChipsoftSessionMode mode)
    {
        Arguments(live, transport, mode); // reject unsupported combinations before opening hardware
        info.EnvironmentVariables["TECH2_CHIPSOFT_FULL_NATIVE"] = live && transport == WindowsTransport.ChipsoftDirect && mode == ChipsoftSessionMode.FullNative ? "1" : "0";
        info.EnvironmentVariables["TECH2_CHIPSOFT_KEY_STATUS"] = "0";
    }
    public static ChipsoftSessionMode Load(string state)
    {
        try { if (File.ReadAllText(Path.Combine(state, "chipsoft-session-mode.txt")).Trim() == "ReadOnly") return ChipsoftSessionMode.ReadOnly; } catch (IOException) { }
        return Default;
    }
    public static void Save(string state, ChipsoftSessionMode mode)
    {
        if (mode != ChipsoftSessionMode.Seeds) File.WriteAllText(Path.Combine(state, "chipsoft-session-mode.txt"), mode.ToString());
    }
}
