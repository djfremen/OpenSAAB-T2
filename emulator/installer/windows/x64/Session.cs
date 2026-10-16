// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;

/// Readers of atomically replaced emulator output must allow Windows delete sharing.
/// Holding an old snapshot open must not stop publication of the next one.
static class SessionSnapshot
{
    public static FileStream Open(string path)
    {
        return new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
    }
    public static byte[] Bytes(string path)
    {
        using (var file = Open(path))
        using (var memory = new MemoryStream())
        {
            file.CopyTo(memory);
            return memory.ToArray();
        }
    }
    public static string Text(string path)
    {
        using (var file = Open(path))
        using (var reader = new StreamReader(file)) return reader.ReadToEnd();
    }
    public static string Hash(string path)
    {
        using (var file = Open(path))
        using (var hash = System.Security.Cryptography.SHA256.Create())
            return BitConverter.ToString(hash.ComputeHash(file)).Replace("-", "").ToLowerInvariant();
    }
}

/// Presentation of the shared Rust adapter-stop classification written to a run's report.json.
sealed class AdapterFailure
{
    public string Kind, Title, Message;
    public static AdapterFailure Parse(string json)
    {
        try
        {
            var report = new System.Web.Script.Serialization.JavaScriptSerializer().Deserialize<Dictionary<string, object>>(json);
            object raw;
            if (report == null || !report.TryGetValue("adapter_failure", out raw)) return null;
            var f = raw as Dictionary<string, object>;
            if (f == null) return null;
            object kind, title, message;
            f.TryGetValue("kind", out kind); f.TryGetValue("title", out title); f.TryGetValue("message", out message);
            if (!(kind is string) || !(title is string) || !(message is string)) return null;
            return new AdapterFailure { Kind = (string)kind, Title = (string)title, Message = (string)message };
        }
        catch (ArgumentException) { return null; }
        catch (InvalidOperationException) { return null; }
    }
    public static AdapterFailure Read(string runDirectory)
    {
        try
        {
            var file = new FileInfo(Path.Combine(runDirectory, "report.json"));
            if (!file.Exists || file.Length > 4000000) return null;
            return Parse(File.ReadAllText(file.FullName));
        }
        catch (IOException) { return null; }
        catch (UnauthorizedAccessException) { return null; }
    }
}

sealed class EmulatorSession : IDisposable
{
    public string RunDirectory
    {
        get; private set;
    }
    public bool Live
    {
        get; private set;
    }
    public bool Running
    {
        get
        {
            return process != null && started && !process.HasExited;
        }
    }
    public int ExitCode
    {
        get
        {
            return process == null ? -1 : process.ExitCode;
        }
    }
    bool started, nativeMailbox; long logBytes; Process process; StreamWriter log;
    readonly Queue<string> keys = new Queue<string>(); readonly object logLock = new object();
    readonly StringBuilder console = new StringBuilder(); DateTime frameTime, sendAfter;
    string Mailbox
    {
        get
        {
            return Path.Combine(RunDirectory, nativeMailbox ? "native-key.txt" : "interactive-key.txt");
        }
    }
    public string ConsoleText
    {
        get
        {
            lock (logLock)
                return console.ToString();
        }
    }
    public static string Quote(string s)
    {
        if (s.Contains("\"") || s.Contains("\n"))
            throw new ArgumentException("Invalid path");
        return "\"" + s + "\"";
    }
    public void Start(string root, string state, FirmwareStore firmware, string adapter, WindowsTransport transport = TransportPlan.Default, string port = null, string driver = null, ChipsoftSessionMode mode = ChipsoftSessionMode.ReadOnly, bool freshSecurity = false)
    {
        if (Running)
            throw new InvalidOperationException("Stop the current session first");
        string missing = firmware.Missing();
        if (missing != "")
            throw new IOException("Firmware setup needed: " + missing);
        var selected = firmware.Selected;
        if (FirmwareStore.Hash(firmware.CardPath(selected)) != selected.imageSha)
            throw new IOException("Active firmware checksum failed");
        Live = adapter != null;
        string transportArgs = TransportPlan.Arguments(adapter, port, transport) + SessionMode.Arguments(Live, transport, mode);
        nativeMailbox = TransportPlan.UsesNativeMailbox(Live, transport);
        if (Live && transport == WindowsTransport.VendorJ2534)
        {
            TransportPlan.RequireX86(driver);
            TransportPlan.RequireX86(Path.Combine(root, adapter == "chipsoft-pro" ? "chipsoft-native-bridge.exe" : "vcx-native-bridge.exe"));
        }
        RunDirectory = Path.Combine(state, "runs", DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N").Substring(0, 8));
        Directory.CreateDirectory(RunDirectory);
        try
        {
            string card = Path.Combine(RunDirectory, "card.bin");
            File.Copy(firmware.CardPath(selected), card);
            if(freshSecurity) {
                using(var f=new FileStream(card,FileMode.Open,FileAccess.Write,FileShare.None)){f.Seek(SecurityStatus.Offset,SeekOrigin.Begin);byte[] clean=new byte[SecurityStatus.Size];for(int i=0;i<clean.Length;i++)clean[i]=255;f.Write(clean,0,clean.Length);f.Flush(true);}
            } else if(SecurityWorkspace.Latest(state)!=null) {
                DesktopWorker.Call(Path.Combine(root,"opensaab-security.exe"),new Dictionary<string,object>{{"op","restore"},{"state",state},{"firmware",selected.id},{"card",card}});
            }
            string args = transportArgs;
            args += " --research-harness --candi-native-link --candi-on-demand --max-insns 50000000000 --output-dir " + Quote(RunDirectory);
            foreach (var pair in new[] { new[] { "--boot", "eprom.bin" }, new[] { "--opsys", "opsys.dwn" }, new[] { "--candi-firmware", "candi.bin" } })
                args += " " + pair[0] + " " + Quote(Path.Combine(firmware.DirectoryPath, pair[1]));
            args += " " + Quote(card);
            log = new StreamWriter(Path.Combine(RunDirectory, "console.log")) { AutoFlush = true };
            var evidence = new System.Collections.Generic.Dictionary<string, object> {
                { "schema", 1 }, { "app_version", ProductIdentity.Version }, { "source_revision", ProductIdentity.Revision }, { "architecture", ProductIdentity.Architecture }, { "started_utc", DateTimeOffset.UtcNow.ToString("o") },
                { "adapter", adapter ?? "offline" }, { "transport", Live ? transport.ToString() : "offline" },
                { "driver_sha256", Live && transport == WindowsTransport.VendorJ2534 ? FirmwareStore.Hash(driver) : null },
                { "driver_version", Live && transport == WindowsTransport.VendorJ2534 ? FileVersionInfo.GetVersionInfo(driver).FileVersion : null },
                { "helper_sha256", Live && transport == WindowsTransport.VendorJ2534 ? FirmwareStore.Hash(Path.Combine(root, adapter == "chipsoft-pro" ? "chipsoft-native-bridge.exe" : "vcx-native-bridge.exe")) : null },
                { "emulator_sha256", FirmwareStore.Hash(Path.Combine(root, "tech2-emu.exe")) },
                { "firmware_sha256", selected.imageSha }, { "command_policy", Live ? SessionMode.Label(mode) : "offline" },
                { "mailbox", nativeMailbox ? "native-key.txt" : "interactive-key.txt" }
            };
            File.WriteAllText(Path.Combine(RunDirectory, "transport.json"), new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(evidence));
            process = new Process();
            process.StartInfo = new ProcessStartInfo(Path.Combine(root, "tech2-emu.exe"), args) { WorkingDirectory = RunDirectory, UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
            process.StartInfo.EnvironmentVariables["LOAD_TEST_ACCELERATE"] = "1";
            process.StartInfo.EnvironmentVariables["OPENSAAB_PERFORMANCE_DIR"] = RunDirectory;
            // Explicit per-session mode; never inherit a previous process environment.
            SessionMode.EnvironmentFor(process.StartInfo, Live, transport, mode);
            process.OutputDataReceived += OnLine;
            process.ErrorDataReceived += OnLine;
            process.Start();
            started = true;
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
            SecurityWorkspace.Write(state,new Dictionary<string,object>{{"pid",process.Id},{"run",RunDirectory},{"firmware",selected.id},{"mode",adapter??"offline"}});
            File.WriteAllText(Path.Combine(state, "latest-session.txt"), "pid=" + process.Id + "\nmode=" + (adapter ?? "offline") + "\nrun=" + RunDirectory + "\nfirmware=" + selected.id + "\n");
        }
        catch { Dispose(); throw; }
    }
    void OnLine(object sender, DataReceivedEventArgs e)
    {
        if (e.Data == null)
            return;
        lock (logLock)
        {
            if (log != null && logBytes < 16 * 1024 * 1024)
            {
                log.WriteLine(e.Data);
                logBytes += e.Data.Length + 2;
            }
            console.AppendLine(e.Data);
            if (console.Length > 24000)
                console.Remove(0, console.Length - 20000);
        }
    }
    public bool CanAcceptMenuKey {get{return Running && keys.Count==0 && !File.Exists(Mailbox);}}
    public void Key(int code)
    {
        if (Running && keys.Count < 16)
            keys.Enqueue(code == 0x10 && !nativeMailbox ? "enter" : "0x" + code.ToString("x2"));
    }
    public void Pump()
    {
        if (!Running || keys.Count == 0 || DateTime.UtcNow < sendAfter || File.Exists(Mailbox))
            return;
        Publish(keys.Dequeue());
        sendAfter = DateTime.UtcNow.AddMilliseconds(nativeMailbox ? 150 : 30);
    }
    void Publish(string value)
    {
        string tmp = Mailbox + ".tmp";
        File.WriteAllText(tmp, value + "\n");
        if (File.Exists(Mailbox))
            File.Delete(Mailbox);
        File.Move(tmp, Mailbox);
    }
    public Bitmap Frame()
    {
        string path = Path.Combine(RunDirectory, "live.ppm");
        if (!File.Exists(path))
            return null;
        DateTime stamp = File.GetLastWriteTimeUtc(path);
        if (stamp == frameTime)
            return null;
        byte[] bytes;
        try
        {
            bytes = SessionSnapshot.Bytes(path);
        }
        catch (IOException) { return null; }
        Bitmap bitmap = DecodeFrame(bytes);
        frameTime = stamp;
        return bitmap;
    }
    public static Bitmap DecodeFrame(byte[] bytes)
    {
        byte[] header = Encoding.ASCII.GetBytes("P6\n320 240\n255\n");
        if (bytes.Length != header.Length + 320 * 240 * 3)
            throw new InvalidDataException("Incomplete LCD frame");
        for (int i = 0; i < header.Length; i++)
            if (bytes[i] != header[i])
                throw new InvalidDataException("Unexpected LCD format");
        var bitmap = new Bitmap(320, 240, PixelFormat.Format24bppRgb);
        var data = bitmap.LockBits(new Rectangle(0, 0, 320, 240), ImageLockMode.WriteOnly, PixelFormat.Format24bppRgb);
        try
        {
            byte[] rgb = new byte[320 * 3];
            for (int y = 0; y < 240; y++)
            {
                for (int x = 0; x < 320; x++)
                {
                    int p = header.Length + (y * 320 + x) * 3;
                    rgb[x * 3] = bytes[p + 2];
                    rgb[x * 3 + 1] = bytes[p + 1];
                    rgb[x * 3 + 2] = bytes[p];
                }
                Marshal.Copy(rgb, 0, IntPtr.Add(data.Scan0, y * data.Stride), rgb.Length);
            }
        }
        finally { bitmap.UnlockBits(data); }
        return bitmap;
    }
    public async Task<bool> Stop()
    {
        keys.Clear();
        if (!Running)
            return true;
        Publish("stop");
        bool clean = await Task.Run(() => process.WaitForExit(3000));
        if (!clean)
        {
            process.Kill();
            await Task.Run(() => process.WaitForExit());
        }
        else
            process.WaitForExit();
        return clean;
    }
    public void Dispose()
    {
        if (process != null)
        {
            if (started && !process.HasExited)
            {
                process.Kill();
                process.WaitForExit();
            }
            process.Dispose();
            process = null;
        }
        lock (logLock)
        {
            if (log != null)
            {
                log.Dispose();
                log = null;
            }
        }
    }
}
