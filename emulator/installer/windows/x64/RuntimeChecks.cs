// SPDX-License-Identifier: MPL-2.0
// Build separately with /main:RuntimeChecks; never invoked by the user launcher.
using System;
using System.IO;
using System.Diagnostics;
using System.Linq;
using System.Threading.Tasks;
static class RuntimeChecks
{
    static void Need(bool okay, string message)
    {
        if (!okay)
            throw new Exception(message);
        Console.WriteLine("PASS " + message);
    }
    static async Task WaitFor(Func<bool> check, string name)
    {
        var watch = Stopwatch.StartNew();
        while (watch.ElapsedMilliseconds < 30000)
        {
            if (check())
            {
                Console.WriteLine("PASS " + name + " " + watch.ElapsedMilliseconds + "ms");
                return;
            }
            await Task.Delay(100);
        }
        throw new Exception("Timeout: " + name);
    }
    static bool Screen(EmulatorSession session, string value)
    {
        if (!session.Running) throw new Exception("Engine exited while waiting for " + value + ": " + session.ConsoleText);
        string path = Path.Combine(session.RunDirectory, "screen.txt");
        try
        {
            return File.Exists(path) && SessionSnapshot.Text(path).Contains(value);
        }
        catch (IOException) { return false; }
    }
    public static int Main(string[] args)
    {
        try
        {
            Run(args.Contains("--oem-menus")).GetAwaiter().GetResult();
            return 0;
        }
        catch (Exception e) { Console.WriteLine("FAIL " + e); return 1; }
    }
    static async Task Run(bool oemMenus)
    {
        string root = AppDomain.CurrentDomain.BaseDirectory, state = Path.Combine(root, "runtime-check-" + DateTime.UtcNow.ToString("yyyyMMdd-HHmmss"));
        Directory.CreateDirectory(state);
        var firmware = new FirmwareStore(root, state);
        string english = firmware.Entries[0].id;
        foreach (var card in firmware.Entries.Where(e => e.runnable))
        {
            string language = card.id;
            await firmware.Download(card, s => { }, System.Threading.CancellationToken.None);
            using (var session = new EmulatorSession())
            {
                session.Start(root, state, firmware, null);
                await WaitFor(() => { if (!session.Running) throw new Exception("Engine exited: " + session.ConsoleText); using (var frame = session.Frame()) { return frame != null; } }, language + " framebuffer");
                await WaitFor(() => {try{return SessionSnapshot.Text(Path.Combine(session.RunDirectory,"startup.json")).Contains("\"verified_welcome\":true");}catch(IOException){return false;}}, language + " verified welcome");
                string welcomeHash=SessionSnapshot.Hash(Path.Combine(session.RunDirectory,"live.ppm"));
                session.Key(16);
                session.Pump();
                await WaitFor(() => SessionSnapshot.Hash(Path.Combine(session.RunDirectory,"live.ppm")) != welcomeHash, language + " Enter changes framebuffer");
                using (var frame = EmulatorSession.DecodeFrame(SessionSnapshot.Bytes(Path.Combine(session.RunDirectory, "live.ppm"))))
                    frame.Save(Path.Combine(state, language + "-main.png"));
                if (card.id == english)
                {
                    await WaitFor(() => Screen(session, "Main Menu"), "NAO main menu");
                    session.Key(16);
                    session.Pump();
                    await WaitFor(() => Screen(session, "Model Year"), "year menu");
                    session.Key(1);
                    session.Pump();
                    await WaitFor(() => Screen(session, "Main Menu"), "EXIT navigates back");
                }
                Need(await session.Stop(), language + " graceful stop");
            }
            Need(FirmwareStore.Hash(firmware.CardPath(card)) == card.imageSha, language + " original card unchanged");
        }
        var before = firmware.Selected.id;
        using (var cancel = new System.Threading.CancellationTokenSource())
        {
            cancel.Cancel();
            bool cancelled = false;
            try
            {
                await firmware.Download(firmware.Entries.First(e => e.language == "fr"), s => { }, cancel.Token);
            }
            catch (OperationCanceledException) { cancelled = true; }
            catch (System.Net.WebException) { cancelled = true; }
            Need(cancelled && firmware.Selected.id == before, "cancel preserves active card");
        }
        if (oemMenus)
        {
            await firmware.Download(firmware.Entries.First(e => e.id == english), s => { }, System.Threading.CancellationToken.None);
            using (var session = new EmulatorSession())
            {
                session.Start(root, state, firmware, "chipsoft-pro", WindowsTransport.VendorJ2534, null, AdapterProfile.Chipsoft.Driver);
                await WaitFor(() => Screen(session, "Press [ENTER]"), "OEM welcome");
                string welcome = SessionSnapshot.Hash(Path.Combine(session.RunDirectory, "live.ppm"));
                session.Key(16); session.Pump();
                await WaitFor(() => Screen(session, "Main Menu"), "OEM Enter reaches main menu");
                Need(welcome != SessionSnapshot.Hash(Path.Combine(session.RunDirectory, "live.ppm")), "OEM framebuffer refreshes beyond splash");
                session.Key(16); session.Pump();
                await WaitFor(() => Screen(session, "Model Year"), "OEM Enter selects Diagnostics");
                session.Key(1); session.Pump();
                await WaitFor(() => Screen(session, "Main Menu"), "OEM EXIT returns");
                Need(await session.Stop(), "OEM graceful stop");
            }
        }
        Console.WriteLine("EVIDENCE=" + state);
    }
}
