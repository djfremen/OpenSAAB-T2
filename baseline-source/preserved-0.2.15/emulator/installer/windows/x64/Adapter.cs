// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Management;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

sealed class AdapterProfile
{
    public string Id, Name, Driver;
    /// COM port assigned by the inbox usbser driver (Chipsoft only); set by Detect.
    public string Port;
    public override string ToString()
    {
        return Name;
    }
    public static AdapterProfile Chipsoft = new AdapterProfile { Id = "chipsoft-pro", Name = "Chipsoft Pro", Driver = @"C:\Program Files (x86)\CHIPSOFT_J2534_Pro_Driver\j2534_interface.dll" };
    public static AdapterProfile Nano = new AdapterProfile { Id = "nano", Name = "VCX Nano", Driver = @"C:\ProgramData\VCX\Apps\PASSTHRU\VCXPT32.dll" };
    public static AdapterProfile[] Detect()
    {
        Chipsoft.Port = null; // Never reuse a disconnected device's COM port.
        var found = new List<AdapterProfile>();
        using (var query = new ManagementObjectSearcher("SELECT Name, PNPDeviceID, ConfigManagerErrorCode FROM Win32_PnPEntity WHERE ConfigManagerErrorCode = 0"))
        using (var devices = query.Get())
            foreach (ManagementObject item in devices)
            {
                string id = Convert.ToString(item["PNPDeviceID"]).ToUpperInvariant(), name = Convert.ToString(item["Name"]);
                // This STM32 VID/PID is a candidate, not proof of Chipsoft identity or ECU connectivity.
                if (id.Contains("VID_0483&PID_5740"))
                {
                    var port = Regex.Match(name ?? "", @"\((COM\d{1,3})\)");
                    if (port.Success)
                        Chipsoft.Port = port.Groups[1].Value;
                    if (!found.Contains(Chipsoft))
                        found.Add(Chipsoft);
                }
                if ((name.IndexOf("VCX", StringComparison.OrdinalIgnoreCase) >= 0) && !found.Contains(Nano))
                    found.Add(Nano);
            }
        return found.ToArray();
    }
}
sealed class ProbeResult
{
    public int Code; public string Text, LogDirectory; public string Vin
    {
        get
        {
            var m = Regex.Match(Text, @"(?:ECU_VIN|VIN)=([A-HJ-NPR-Z0-9]{17})(?:\r?\n|$)");
            return m.Success ? m.Groups[1].Value : null;
        }
    }
    public bool Clean
    {
        get
        {
            if (Text.Contains("CHIPSOFT_VIN_RESULT"))
                return Code == 0 && Text.Contains("\"channel_closed\":true");
            return Code == 0 && Text.Contains("PassThruStopMsgFilter rc=0x00000000") && Text.Contains("PassThruDisconnect rc=0x00000000") && Text.Contains("PassThruClose rc=0x00000000") && !Regex.IsMatch(Text, @"PassThru(?:StopMsgFilter|Disconnect|Close) rc=0x(?!00000000)[0-9A-Fa-f]{8}");
        }
    }
}
static class AdapterProbe
{
    public static bool NativeAvailable(string root)
    {
        return File.Exists(Path.Combine(root, "chipsoft-vin.exe"));
    }
    /// Native Chipsoft protocol over the usbser COM port: the Android design, no vendor J2534 DLL.
    public static async Task<ProbeResult> RunNative(string root, string state, AdapterProfile adapter)
    {
        string exe = Path.Combine(root, "chipsoft-vin.exe");
        if (!File.Exists(exe))
            throw new IOException("chipsoft-vin.exe is missing from this installation. Repair or reinstall OpenSAAB.");
        if (string.IsNullOrEmpty(adapter.Port))
            throw new IOException("Chipsoft Pro is connected but Windows has not assigned it a COM port yet. Unplug and reconnect the adapter, then retry.");
        string dir = Path.Combine(state, "reports", DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N").Substring(0, 8));
        Directory.CreateDirectory(dir);
        var start = new ProcessStartInfo(exe, "--port " + adapter.Port + " --timeout-ms 8000") { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
        using (var p = Process.Start(start))
        {
            var output = p.StandardOutput.ReadToEndAsync();
            var error = p.StandardError.ReadToEndAsync();
            if (!await Task.Run(() => p.WaitForExit(20000)))
            {
                try { p.Kill(); } catch { }
                throw new IOException("The native Chipsoft helper did not finish within 20 s. Check the adapter and retry.");
            }
            var result = new ProbeResult { Code = p.ExitCode, Text = await output + "\n" + await error, LogDirectory = dir };
            File.WriteAllText(Path.Combine(dir, "report.log"), result.Text);
            return result;
        }
    }
    public static async Task<ProbeResult> Run(string root, string state, AdapterProfile adapter, bool dtc, WindowsTransport mode = TransportPlan.Default)
    {
        TransportPlan.Arguments(adapter.Id, adapter.Port, mode);
        if (mode == WindowsTransport.ChipsoftDirect)
        {
            if (dtc) throw new InvalidOperationException("The standalone Windows DTC reader uses the OEM DLL. Use the original firmware menus for this direct-transport test; no automatic transport switch was made.");
            return await RunNative(root, state, adapter);
        }
        if (!File.Exists(adapter.Driver))
            throw new IOException("Install the manufacturer's " + adapter.Name + " Windows driver. This app does not bundle vendor drivers.");
        TransportPlan.RequireX86(adapter.Driver);
        if (dtc && adapter.Id != "chipsoft-pro")
            throw new InvalidOperationException("HS-CAN engine code reading is currently qualified for Chipsoft Pro only.");
        string script = dtc ? "read_t8_dtc.ps1" : (adapter.Id == "chipsoft-pro" ? "read_t8_identity.ps1" : "vcx_t8_vin.ps1");
        string dir = Path.Combine(state, "reports", DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N").Substring(0, 8));
        Directory.CreateDirectory(dir);
        string wrapper = "& ([scriptblock]::Create((Get-Content -LiteralPath '" + Path.Combine(root, "adapter-tools", "invoke_vcx_t8_vin.ps1").Replace("'", "''") + "' -Raw))) -ProbePath '" + Path.Combine(root, "adapter-tools", script).Replace("'", "''") + "' -LogDirectory '" + dir.Replace("'", "''") + "' -TimeoutSeconds 25";
        string args = "-NoLogo -NoProfile -NonInteractive -EncodedCommand " + Convert.ToBase64String(System.Text.Encoding.Unicode.GetBytes(wrapper));
        var start = new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), @"WindowsPowerShell\v1.0\powershell.exe"), args) { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
        using (var p = Process.Start(start))
        {
            var output = p.StandardOutput.ReadToEndAsync();
            var error = p.StandardError.ReadToEndAsync();
            await Task.Run(() => p.WaitForExit());
            var result = new ProbeResult { Code = p.ExitCode, Text = await output + "\n" + await error, LogDirectory = dir };
            File.WriteAllText(Path.Combine(dir, "report.log"), result.Text);
            return result;
        }
    }
    public static string DtcSummary(ProbeResult result)
    {
        if (!result.Clean || !Regex.IsMatch(result.Text, @"DTC_REPORT complete=1 records=\d+ cleanup=1"))
            return "Engine code read incomplete. No empty-code result claimed.\r\n\r\n" + result.Text;
        var codes = Regex.Matches(result.Text, @"DTC_SUMMARY ([^\r\n]+)").Cast<Match>().Select(m => m.Groups[1].Value).ToArray();
        return "Trionic 8 engine · HS-CAN · " + DateTime.Now.ToString("g") + "\r\n" + (codes.Length == 0 ? "No codes reported (complete ECU response)." : string.Join("\r\n", codes)) + "\r\n\r\nRead-only. Other vehicle modules are not included.\r\nSaved: " + result.LogDirectory;
    }
}
