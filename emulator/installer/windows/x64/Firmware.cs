// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using System.Threading;
using System.Web.Script.Serialization;

sealed class FirmwareEntry
{
    public string id
    {
        get; set;
    }
    public string label
    {
        get; set;
    }
    public string zipSha
    {
        get; set;
    }
    public string imageSha
    {
        get; set;
    }
    public string language
    {
        get; set;
    }
    public string url
    {
        get; set;
    }
    public long downloadBytes
    {
        get; set;
    }
    public bool runnable
    {
        get; set;
    }
    public override string ToString()
    {
        return label;
    }
}
sealed class FirmwareCatalog
{
    public int schema
    {
        get; set;
    }
    public FirmwareEntry[] entries
    {
        get; set;
    }
}
sealed class FirmwareStore
{
    public readonly string DirectoryPath;
    public readonly FirmwareEntry[] Entries;
    readonly string active;
    public FirmwareStore(string root, string state)
    {
        DirectoryPath = Path.Combine(state, "firmware");
        Directory.CreateDirectory(DirectoryPath);
        active = Path.Combine(DirectoryPath, "active.txt");
        using (var stream = typeof(FirmwareStore).Assembly.GetManifestResourceStream("firmware-catalog.json"))
        using (var reader = new StreamReader(stream))
            Entries = new JavaScriptSerializer().Deserialize<FirmwareCatalog>(reader.ReadToEnd()).entries;
        if (Entries.Length != 11)
            throw new InvalidDataException("Invalid firmware catalog");
        // Import the existing private install once. Never modify the installed source files.
        foreach (string name in new[] { "eprom.bin", "opsys.dwn", "candi.bin" })
        {
            string from = Path.Combine(root, "runtime", name), to = Path.Combine(DirectoryPath, name);
            if (!File.Exists(to) && File.Exists(from))
                File.Copy(from, to);
        }
        string previous = Path.Combine(root, "runtime", "Saab NAO.bin");
        if (!File.Exists(active) && File.Exists(previous))
        {
            string hash = Hash(previous);
            FirmwareEntry entry = Entries.FirstOrDefault(e => e.runnable && e.imageSha == hash);
            if (entry != null)
            {
                string to = CardPath(entry);
                if (!File.Exists(to))
                    File.Copy(previous, to);
                Activate(entry);
            }
        }
    }
    public static string Hash(string path)
    {
        using (var s = File.OpenRead(path))
        using (var h = SHA256.Create())
            return BitConverter.ToString(h.ComputeHash(s)).Replace("-", "").ToLowerInvariant();
    }
    public string CardPath(FirmwareEntry entry)
    {
        return Path.Combine(DirectoryPath, entry.id + ".bin");
    }
    public FirmwareEntry Selected
    {
        get
        {
            string id = File.Exists(active) ? File.ReadAllText(active).Trim() : "";
            return Entries.FirstOrDefault(e => e.id == id && e.runnable);
        }
    }
    public string Missing()
    {
        foreach (string name in new[] { "eprom.bin", "opsys.dwn", "candi.bin" })
            if (!File.Exists(Path.Combine(DirectoryPath, name)))
                return name;
        var entry = Selected;
        return entry == null || !File.Exists(CardPath(entry)) ? "firmware card" : "";
    }
    public void Activate(FirmwareEntry entry)
    {
        if (!entry.runnable)
            throw new InvalidOperationException("This firmware is download-only; the active card is preserved.");
        if (Hash(CardPath(entry)) != entry.imageSha)
            throw new InvalidDataException("Firmware checksum failed; the active card is preserved.");
        string pending = active + ".tmp";
        File.WriteAllText(pending, entry.id);
        if (File.Exists(active))
            File.Replace(pending, active, active + ".previous");
        else
            File.Move(pending, active);
    }
    public async Task Download(FirmwareEntry entry, Action<string> progress, CancellationToken cancel)
    {
        cancel.ThrowIfCancellationRequested();
        if (!entry.runnable)
            throw new InvalidOperationException("Russian 140.500 cannot boot with this emulator. Choose Russian 148.000.");
        if (File.Exists(CardPath(entry)) && Hash(CardPath(entry)) == entry.imageSha)
        {
            Activate(entry);
            return;
        }
        string temp = Path.Combine(DirectoryPath, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(temp);
        try
        {
            string zip = Path.Combine(temp, "card.zip"), bin = Path.Combine(temp, "card.bin");
            ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12;
            using (var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancel))
            using (var client = new WebClient())
            using (timeout.Token.Register(() => client.CancelAsync()))
            {
                timeout.CancelAfter(TimeSpan.FromSeconds(60));
                client.DownloadProgressChanged += delegate (object sender, DownloadProgressChangedEventArgs e)
                {
                    if (e.BytesReceived > entry.downloadBytes)
                        client.CancelAsync();
                    progress("Downloading firmware · " + e.ProgressPercentage + "%");
                };
                await client.DownloadFileTaskAsync(new Uri(entry.url), zip);
            }
            cancel.ThrowIfCancellationRequested();
            progress("Verifying firmware…");
            await Task.Run(delegate
            {
                if (new FileInfo(zip).Length != entry.downloadBytes || Hash(zip) != entry.zipSha)
                    throw new InvalidDataException("Downloaded archive failed verification; active firmware preserved.");
                using (var archive = ZipFile.OpenRead(zip))
                {
                    var candidates = archive.Entries.Where(e => e.Length == 32L * 1024 * 1024).ToArray();
                    if (candidates.Length != 1)
                        throw new InvalidDataException("Expected exactly one 32 MB firmware card");
                    // Fixed destination: archive filenames are never used as paths.
                    candidates[0].ExtractToFile(bin);
                }
                if (Hash(bin) != entry.imageSha)
                    throw new InvalidDataException("Firmware image checksum failed");
                string target = CardPath(entry);
                if (File.Exists(target))
                    File.Replace(bin, target, target + ".previous");
                else
                    File.Move(bin, target);
                cancel.ThrowIfCancellationRequested();
                Activate(entry);
            });
        }
        finally { if (Directory.Exists(temp)) Directory.Delete(temp, true); }
    }
}
