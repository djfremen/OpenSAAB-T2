// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Text;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

// Native process/HTTPS/UI binding only. Collection, redaction and receipt decisions are
// shared Rust support_reports.rs, ported from Android SupportReports/SupportUpload.
static class DesktopWorker
{
    public static Dictionary<string, object> Call(string binary, Dictionary<string, object> request)
    {
        request["schema"] = 1;
        var json = new JavaScriptSerializer { MaxJsonLength = 2 * 1024 * 1024 };
        using (var p = new Process()) {
            p.StartInfo = new ProcessStartInfo(binary) { UseShellExecute = false, CreateNoWindow = true, RedirectStandardInput = true, RedirectStandardOutput = true, StandardOutputEncoding = Encoding.UTF8 };
            p.Start();
            var output = Task.Run(() => ReadBounded(p.StandardOutput.BaseStream, 2 * 1024 * 1024));
            try {
                var bytes = Encoding.UTF8.GetBytes(json.Serialize(request));
                if (bytes.Length > 2 * 1024 * 1024) throw new IOException("Worker request is too large.");
                p.StandardInput.BaseStream.Write(bytes, 0, bytes.Length); p.StandardInput.Close();
                if (!p.WaitForExit(30000) || !output.Wait(2000)) throw new IOException("Report worker timed out.");
                var envelope = json.Deserialize<Dictionary<string, object>>(Encoding.UTF8.GetString(output.Result));
                if (p.ExitCode != 0 || !envelope.ContainsKey("ok") || !Object.Equals(envelope["ok"], true))
                    throw new IOException(envelope.ContainsKey("error") ? Convert.ToString(envelope["error"]) : "Report worker failed.");
                return (Dictionary<string, object>)envelope["result"];
            } finally { if (!p.HasExited) { p.Kill(); p.WaitForExit(2000); } }
        }
    }
    public static byte[] ReadBounded(Stream stream, int limit)
    {
        using (var output = new MemoryStream()) {
            byte[] buffer = new byte[4096]; int n;
            while ((n = stream.Read(buffer, 0, Math.Min(buffer.Length, limit + 1 - (int)output.Length))) > 0) {
                output.Write(buffer, 0, n);
                if (output.Length > limit) throw new IOException("Reply is too large.");
            }
            return output.ToArray();
        }
    }
}
sealed class SupportReply { public int Status; public string Network = "", Body = ""; }
static class SupportUpload
{
    public static HttpWebRequest Request(string endpoint, string consent, byte[] bytes)
    {
        var url = new Uri(endpoint);
        if (url.Scheme != "https" || url.Host != "www.opensaab.com" || !url.IsDefaultPort || url.AbsolutePath != "/api/support/reports" || url.Query != "" || url.Fragment != "" || url.UserInfo != "" || consent != "support-report-v1" || bytes.Length > 65536)
            throw new IOException("Invalid support report destination or size.");
        ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12;
        var r = (HttpWebRequest)WebRequest.Create(url);
        r.Method = "POST"; r.ContentType = "application/json; charset=utf-8"; r.ContentLength = bytes.Length;
        r.AllowAutoRedirect = false; r.Credentials = null; r.UseDefaultCredentials = false; r.CookieContainer = null;
        r.Timeout = 30000; r.ReadWriteTimeout = 30000; r.Headers["X-OpenSAAB-Consent"] = consent;
        r.CachePolicy = new System.Net.Cache.RequestCachePolicy(System.Net.Cache.RequestCacheLevel.NoCacheNoStore);
        return r;
    }
    public static SupportReply Post(string endpoint, string consent, string text)
    {
        var result = new SupportReply(); HttpWebRequest request = null;
        try {
            byte[] bytes = Encoding.UTF8.GetBytes(text); request = Request(endpoint, consent, bytes);
            // A total deadline covers slow responses in addition to per-operation timeouts.
            using (var deadline = new System.Threading.Timer(_ => request.Abort(), null, 45000, System.Threading.Timeout.Infinite)) {
                using (var s = request.GetRequestStream()) s.Write(bytes, 0, bytes.Length);
                HttpWebResponse response;
                try { response = (HttpWebResponse)request.GetResponse(); }
                catch (WebException e) { if (e.Response == null) throw; response = (HttpWebResponse)e.Response; }
                using (response) {
                    result.Status = (int)response.StatusCode;
                    using (var s = response.GetResponseStream()) result.Body = Encoding.UTF8.GetString(DesktopWorker.ReadBounded(s, 4096));
                }
            }
        } catch (WebException e) {
            result.Network = e.Status == WebExceptionStatus.Timeout || e.Status == WebExceptionStatus.RequestCanceled ? "timeout" : e.Status == WebExceptionStatus.NameResolutionFailure ? "dns" : e.Status == WebExceptionStatus.ConnectFailure ? "offline" : "invalid";
        } catch (IOException) { result.Network = "oversize"; }
        catch (Exception) { result.Network = "invalid"; }
        finally { if (request != null) request.Abort(); }
        return result;
    }
}
sealed partial class SessionWindow
{
    string ReportsWorker { get { return Path.Combine(root, "opensaab-reports.exe"); } }
    bool reporting;
    const string ReportIntro = "Prepare app/computer details, recent session outcomes, adapter event counts and available error information. Review the JSON, then send it privately to OpenSAAB over the internet. No email app or account is needed. Reports are stored privately for the project administrator until deleted.\r\n\r\nRaw traffic, VIN, security data and screenshots are excluded. Your description is included; avoid private codes. Nothing uploads automatically.";
    Dictionary<string, object> HostFacts()
    {
        var display = Screen.FromControl(this).Bounds;
        var facts = new Dictionary<string, object> { {"platform", "windows"}, {"arch", "x86_64"}, {"version", ProductIdentity.Version}, {"build", ProductIdentity.Revision}, {"os_version", Environment.OSVersion.VersionString}, {"runtime_cpu_count", Environment.ProcessorCount}, {"display_width_px", display.Width}, {"display_height_px", display.Height} };
        try { facts["storage_free_bytes"] = new DriveInfo(Path.GetPathRoot(state)).AvailableFreeSpace; } catch (IOException) { }
        return facts;
    }
    async void ReportIssue()
    {
        if (reporting) return;
        reporting = true;
        try {
            string description;
            using (var f = Dialog("Report a problem", 700, 480)) {
                var box = new TextBox { Multiline = true, MaxLength = 2000, Dock = DockStyle.Fill, ScrollBars = ScrollBars.Vertical };
                var intro = new Label { Text = ReportIntro, Dock = DockStyle.Top, Height = 215, Padding = new Padding(12) };
                var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = 60 };
                bool prepare = false;
                buttons.Controls.Add(Button("Prepare report", () => { prepare = true; f.Close(); }, true));
                buttons.Controls.Add(Button("Back", () => f.Close()));
                foreach (Control b in buttons.Controls) { b.Dock = DockStyle.None; b.Size = new Size(180, 48); }
                f.Controls.Add(box); f.Controls.Add(intro); f.Controls.Add(buttons); f.ShowDialog(this);
                if (!prepare) return; description = box.Text;
            }
            status.Text = "Preparing support report…";
            var host = HostFacts();
            var report = await Task.Run(() => DesktopWorker.Call(ReportsWorker, new Dictionary<string, object> { {"op", "support_collect"}, {"root", state}, {"description", description}, {"host", host} }));
            if (IsDisposed) return;
            string text = (string)report["text"];
            bool review = true;
            while (review) {
                bool send = false;
                using (var f = Dialog("Review support report", 760, 540)) {
                    var preview = new TextBox { Multiline = true, ReadOnly = true, Text = text.Replace("\n", "\r\n"), ScrollBars = ScrollBars.Vertical, Dock = DockStyle.Fill, Font = new Font("Consolas", 10) };
                    var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = 64, WrapContents = false, AutoScroll = true };
                    buttons.Controls.Add(Button("Send to OpenSAAB", () => { send = true; f.Close(); }, true));
                    buttons.Controls.Add(Button("Save JSON", () => { using (var save = new SaveFileDialog { FileName = "OpenSAAB-support.json", Filter = "JSON report|*.json" }) if (save.ShowDialog(f) == DialogResult.OK) File.WriteAllText(save.FileName, text, new UTF8Encoding(false)); }));
                    buttons.Controls.Add(Button("Copy JSON", () => Clipboard.SetText(text)));
                    buttons.Controls.Add(Button("Keep private", () => f.Close()));
                    foreach (Control b in buttons.Controls) { b.Dock = DockStyle.None; b.Size = new Size(172, 48); }
                    f.Controls.Add(preview); f.Controls.Add(buttons); f.ShowDialog(this);
                }
                if (!send) { status.Text = "Report kept privately on this computer"; return; }
                status.Text = "Sending private report to OpenSAAB… Local copy kept.";
                var reply = await Task.Run(() => SupportUpload.Post((string)report["endpoint"], (string)report["consent"], text));
                var decision = await Task.Run(() => DesktopWorker.Call(ReportsWorker, new Dictionary<string, object> { {"op", "support_receipt"}, {"root", state}, {"status", reply.Status}, {"network", reply.Network}, {"body", reply.Body} }));
                if (Object.Equals(decision["stored"], true)) {
                    string id = (string)decision["report_id"]; status.Text = "Report sent · " + id;
                    TextDialog("Report sent", "Stored privately by OpenSAAB.\n\nReport number: " + id, true); return;
                }
                status.Text = "Report not confirmed · Local JSON kept";
                review = MessageBox.Show(this, Convert.ToString(decision["message"]) + "\n\nReturn to the report to retry or save it?", "Report not confirmed", MessageBoxButtons.YesNo, MessageBoxIcon.Warning) == DialogResult.Yes;
            }
        } catch (Exception) { status.Text = "Report not confirmed · Existing local reports kept"; TextDialog("Report a problem", "Could not prepare or confirm the report. Any prepared JSON is kept in the app’s support-reports folder. Check your internet connection and free space, then retry.", true); }
        finally { reporting = false; }
    }
}
