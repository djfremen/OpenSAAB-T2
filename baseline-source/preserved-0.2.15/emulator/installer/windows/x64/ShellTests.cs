// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;
using System.Drawing;
using System.Text;
static class ShellTests
{
    static void Require(bool condition, string name)
    {
        if (!condition)
            throw new Exception("Self-test failed: " + name);
    }
    public static void Run()
    {
        TransportTests.Run();
        SupportTests.Run();
        SecurityWorkflowTests.Run();
        string temp = Path.Combine(Path.GetTempPath(), "opensaab-shell-test-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(temp);
        try
        {
            var store = new FirmwareStore(temp, temp);
            Require(store.Entries.Length == 11, "catalog size");
            Require(store.Missing() == "eprom.bin", "missing runtime");
            var blocked = Array.Find(store.Entries, e => !e.runnable);
            bool rejected = false;
            try
            {
                store.Activate(blocked);
            }
            catch (InvalidOperationException) { rejected = true; }
            Require(rejected, "unsupported firmware fails closed");
            var first = store.Entries[0];
            File.WriteAllText(store.CardPath(first), "corrupt");
            rejected = false;
            try
            {
                store.Activate(first);
            }
            catch (InvalidDataException) { rejected = true; }
            Require(rejected && store.Selected == null, "corrupt firmware does not activate");
            var header = Encoding.ASCII.GetBytes("P6\n320 240\n255\n");
            var bytes = new byte[header.Length + 320 * 240 * 3];
            Array.Copy(header, bytes, header.Length);
            bytes[header.Length] = 255;
            using (var b = EmulatorSession.DecodeFrame(bytes))
            {
                Require(b.Width == 320 && b.Height == 240 && b.GetPixel(0, 0).R == 255 && b.GetPixel(0, 0).B == 0, "real RGB framebuffer");
            }
            rejected = false;
            try
            {
                using (var b = EmulatorSession.DecodeFrame(new byte[10]))
                {
                }
            }
            catch (InvalidDataException) { rejected = true; }
            Require(rejected, "torn frame rejected");
            var partial = new ProbeResult { Code = 0, Text = "DTC_SUMMARY P0107 failure_type=00 status=6F" };
            Require(!partial.Clean && AdapterProbe.DtcSummary(partial).Contains("incomplete"), "partial report never claims complete");
            var clean = new ProbeResult { Code = 0, Text = "ECU_VIN=YS3FD49Y271000001\nPassThruStopMsgFilter rc=0x00000000\nPassThruDisconnect rc=0x00000000\nPassThruClose rc=0x00000000\nDTC_REPORT complete=1 records=0 cleanup=1" };
            Require(clean.Clean && clean.Vin != null && AdapterProbe.DtcSummary(clean).Contains("No codes reported"), "complete empty report");
            clean.Text += "\nPassThruClose rc=0x00000001";
            Require(!clean.Clean, "cleanup error overrides success");
            File.WriteAllText(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "shell-selftest-result.txt"), "PASS: security HTTP credentials/cancel/DPAPI; session modes; bounded HTTPS upload; shared report collection/receipt; working checkpoint; catalog; checksum rejection; unsupported language guard; RGB framebuffer; incomplete frame guard; partial DTC guard; complete DTC; cleanup failure guard\r\n");
        }
        finally { Directory.Delete(temp, true); }
    }
}
