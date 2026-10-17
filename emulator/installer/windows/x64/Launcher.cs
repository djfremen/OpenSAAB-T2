// SPDX-License-Identifier: MPL-2.0
using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Threading;
using System.Windows.Forms;

[assembly: System.Reflection.AssemblyVersion("0.2.18.5")]
[assembly: System.Reflection.AssemblyFileVersion("0.2.18.5")]

static class Launcher
{
    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
    [STAThread]
    static int Main(string[] args)
    {
        try
        {
            if (args.Length == 1 && args[0] == "--self-test")
            {
                ShellTests.Run();
                return 0;
            }
            if (args.Length > 1 || (args.Length == 1 && args[0] != "--offline" && args[0] != "--nano"))
                throw new ArgumentException("Unsupported launch option");
            SetProcessDPIAware();
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            bool created;
            using (var instance = new Mutex(true, @"Local\OpenSAAB-T2-Desktop-Session", out created))
            {
                if (!created)
                {
                    MessageBox.Show("OpenSAAB is already running. Return to the existing window.", "OpenSAAB T2");
                    return 0;
                }
                Application.Run(new SessionWindow());
            }
            return 0;
        }
        catch (Exception e) { MessageBox.Show(e.Message, "OpenSAAB T2", MessageBoxButtons.OK, MessageBoxIcon.Error); return 1; }
    }
}
sealed class LcdView : Control
{
    public Bitmap Frame;
    // Shown instead of a blank or stale display after a session ends; cleared by a new session.
    string caption;
    public string Caption { get { return caption; } set { caption = value; Invalidate(); } }
    public LcdView()
    {
        DoubleBuffered = true;
        SetStyle(ControlStyles.ResizeRedraw, true);
        BackColor = Color.FromArgb(8, 15, 23);
    }
    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        if (Frame == null)
        {
            TextRenderer.DrawText(e.Graphics, caption ?? "Your Tech 2 screen appears here", Font, ClientRectangle,
                caption != null ? Color.FromArgb(255, 159, 10) : Color.FromArgb(153, 176, 197),
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.WordBreak);
            return;
        }
        float scale = Math.Min(Width / 320f, Height / 240f);
        int w = (int)(320 * scale), h = (int)(240 * scale);
        e.Graphics.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.NearestNeighbor;
        e.Graphics.PixelOffsetMode = System.Drawing.Drawing2D.PixelOffsetMode.Half;
        e.Graphics.DrawImage(Frame, new Rectangle((Width - w) / 2, (Height - h) / 2, w, h));
    }
    public void Replace(Bitmap b)
    {
        var old = Frame;
        Frame = b;
        if (old != null)
            old.Dispose();
        Invalidate();
    }
    protected override void Dispose(bool disposing)
    {
        if (disposing && Frame != null)
        {
            Frame.Dispose();
            Frame = null;
        }
        base.Dispose(disposing);
    }
}
sealed partial class SessionWindow : Form
{
    static readonly Color Background = ColorTranslator.FromHtml("#0d1620"), Foreground = ColorTranslator.FromHtml("#edf4fa");
    readonly string root = AppDomain.CurrentDomain.BaseDirectory;
    readonly string state = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), @"OpenSAAB\WindowsData");
    FirmwareStore firmware; EmulatorSession session; AdapterProfile adapter; bool busy, closing, expanded, portrait; bool ended;
    WindowsTransport transport = TransportPlan.Default;
    ChipsoftSessionMode mode = SessionMode.Default;
    readonly TableLayoutPanel layout = new TableLayoutPanel(), rail = new TableLayoutPanel(), main = new TableLayoutPanel();
    readonly LcdView lcd = new LcdView(); readonly Label status = new Label(), vehicle = new Label(), selection = new Label();
    readonly Button connect, offline, menuButton; Button securityButton; DateTime securityPoll=DateTime.MinValue; SecurityStatus securityView=SecurityStatus.Evaluate(null,null,DateTimeOffset.UtcNow,null); readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer { Interval = 50 };
    string recentConsole = "", lastReport = "", vin = "", engineCodes = ""; Point down; Stopwatch held = new Stopwatch();
    public SessionWindow()
    {
        Text = ProductIdentity.ProgramName;
        Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
        BackColor = Background;
        ForeColor = Foreground;
        Font = new Font("Segoe UI", 11);
        ClientSize = new Size(Math.Min(1050, Screen.PrimaryScreen.WorkingArea.Width - 60), Math.Min(720, Screen.PrimaryScreen.WorkingArea.Height - 80));
        MinimumSize = new Size(500, 600);
        StartPosition = FormStartPosition.CenterScreen;
        KeyPreview = true;
        Directory.CreateDirectory(state);
        mode = SessionMode.Load(state);
        firmware = new FirmwareStore(root, state);
        layout.Dock = DockStyle.Fill;
        layout.Padding = new Padding(8);
        var shell = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 1, RowCount = 2 };
        shell.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        shell.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        shell.RowStyles.Add(new RowStyle(SizeType.Absolute, 56));
        shell.Controls.Add(layout, 0, 0);
        shell.Controls.Add(Button("Donate · Support OpenSAAB", ProjectSupport), 0, 1);
        Controls.Add(shell);
        rail.Dock = DockStyle.Fill;
        rail.ColumnCount = 1;
        rail.RowCount = 9;
        rail.Padding = new Padding(4);
        rail.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        foreach (int height in new[] { 58, 88, 120, 68, 48, 48, 62, 48 })
            rail.RowStyles.Add(new RowStyle(SizeType.Absolute, height));
        rail.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        var title = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 3, RowCount = 1 };
        title.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 48));
        title.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        title.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 48));
        var brand = Label(ProductIdentity.ProgramName);
        brand.Padding = Padding.Empty;
        brand.Font = new Font(Font.FontFamily, 11, FontStyle.Bold);
        var logo = new PictureBox { Dock = DockStyle.Fill, SizeMode = PictureBoxSizeMode.Zoom, AccessibleName = "OpenSAAB logo" };
        using (var stream = typeof(Launcher).Assembly.GetManifestResourceStream("opensaab.png"))
        using (var image = Image.FromStream(stream)) logo.Image = new Bitmap(image);
        logo.Disposed += delegate { if (logo.Image != null) logo.Image.Dispose(); };
        title.Controls.Add(logo, 0, 0);
        title.Controls.Add(brand, 1, 0);
        menuButton = Button("⋮", AppMenu);
        title.Controls.Add(menuButton, 2, 0);
        rail.Controls.Add(title, 0, 0);
        StyleLabel(vehicle);
        vehicle.Text = "Connect an adapter to identify your vehicle";
        rail.Controls.Add(vehicle, 0, 1);
        StyleLabel(status);
        status.Text = "Ready — connect an adapter or explore offline";
        status.ForeColor = ColorTranslator.FromHtml("#9bcbea");
        rail.Controls.Add(status, 0, 2);
        connect = Button("Connect and start", async () => await ChooseConnection(), true);
        rail.Controls.Add(connect, 0, 4);
        offline = Button("Run without an adapter", async () => { adapter = null; vin = ""; ClearBluetoothVehicle(); await Start(false); });
        rail.Controls.Add(offline, 0, 5);
        StyleLabel(selection);
        selection.Font = new Font(Font.FontFamily, 9);
        rail.Controls.Add(selection, 0, 6);
        rail.Controls.Add(Button("Firmware selection", FirmwareDialog), 0, 7);
        securityButton=Button("Security: [N/A]",()=>{if(securityView.State=="PRE_AUTH")SecurityAction();else SecurityDetails();});
        rail.Controls.Add(securityButton,0,3);
        main.Dock = DockStyle.Fill;
        main.ColumnCount = 1;
        main.RowCount = 4;
        main.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        main.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        main.RowStyles.Add(new RowStyle(SizeType.Absolute, 52));
        main.RowStyles.Add(new RowStyle(SizeType.Absolute, 28));
        main.RowStyles.Add(new RowStyle(SizeType.Absolute, 52));
        lcd.Dock = DockStyle.Fill;
        lcd.Margin = new Padding(4);
        main.Controls.Add(lcd, 0, 0);
        main.Controls.Add(Row(new[] { "S1", "S2", "S3", "S4" }, new Action[] { () => Key(0x0a), () => Key(6), () => Key(7), () => Key(8) }), 0, 1);
        var hint = Label("Scroll / swipe ↑ ↓ · Hold for ENTER · EXIT goes back");
        hint.Padding = Padding.Empty;
        hint.Margin = Padding.Empty;
        hint.Font = new Font(Font.FontFamily, 9);
        hint.TextAlign = ContentAlignment.MiddleCenter;
        main.Controls.Add(hint, 0, 2);
        main.Controls.Add(Row(new[] { "EXIT", "Keypad", "Actions" }, new Action[] { () => Key(1), Keypad, Actions }), 0, 3);
        lcd.MouseDown += delegate (object sender, MouseEventArgs e)
        {
            down = e.Location;
            held.Restart();
            lcd.Focus();
        };
        lcd.MouseUp += delegate (object sender, MouseEventArgs e)
        {
            held.Stop();
            int delta = e.Y - down.Y;
            if (Math.Abs(delta) > 35)
                Key(delta < 0 ? 9 : 12);
            else if (held.ElapsedMilliseconds >= 450)
                Key(16);
        };
        lcd.MouseWheel += delegate (object sender, MouseEventArgs e)
        {
            Key(e.Delta > 0 ? 9 : 12);
        };
        Resize += delegate
        {
            Arrange();
        };
        Arrange();
        UpdateSelection();
        timer.Tick += delegate
        {
            Tick();
        };
        timer.Start();
        FormClosing += async delegate (object sender, FormClosingEventArgs e) { if (closing) return; e.Cancel = true; if(bluetoothJob!=null){closeAfterBluetooth=true;await bluetoothJob.Stop();return;} if (busy) { if(securityJob!=null)securityJob.Cancel(); status.Text = "Finishing the current operation…"; return; } busy = true; await Stop(); closing = true; Close(); };
        Shown += delegate
        {
            if (firmware.Missing() != "")
                FirmwareDialog();
        };
    }
    void ProjectSupport()
    {
        if (busy || Running) { MessageBox.Show(this, "Finish the current session before opening project support.", "Support OpenSAAB"); return; }
        try {
            System.Collections.Generic.Dictionary<string, object> spec;
            using (var stream = typeof(Launcher).Assembly.GetManifestResourceStream("interface-contract.json"))
            using (var reader = new StreamReader(stream))
                spec = new System.Web.Script.Serialization.JavaScriptSerializer().Deserialize<System.Collections.Generic.Dictionary<string, object>>(reader.ReadToEnd());
            var branding = (System.Collections.Generic.Dictionary<string, object>)spec["branding"];
            var s = (System.Collections.Generic.Dictionary<string, object>)branding["support"];
            using (var dialog = new Form { Text = (string)s["title"], Size = new Size(510, 430), MinimumSize = new Size(510, 430), StartPosition = FormStartPosition.CenterParent, BackColor = Background, ForeColor = Foreground }) {
                var text = new TextBox { Text = (string)s["message"] + "\r\n\r\n" + (string)s["external"] + "\r\nKo-fi: " + (string)s["url"], Multiline = true, ReadOnly = true, TabStop = false, Dock = DockStyle.Fill, ScrollBars = ScrollBars.Vertical, BackColor = Background, ForeColor = Foreground, BorderStyle = BorderStyle.None, Font = Font };
                var buttons = Row(new[] { (string)s["donate"], (string)s["close"] }, new Action[] { () => { if ((string)s["url"] == "https://ko-fi.com/djfremen") { Process.Start(new ProcessStartInfo((string)s["url"]) { UseShellExecute = true }); dialog.Close(); } }, () => dialog.Close() });
                buttons.Dock = DockStyle.Bottom; buttons.Height = 64;
                dialog.Padding = new Padding(16); dialog.Controls.Add(text); dialog.Controls.Add(buttons); dialog.ShowDialog(this);
            }
        } catch (Exception e) { Error(e); }
    }
    void Arrange()
    {
        bool next = !expanded && !(ClientSize.Width >= 720 && ClientSize.Width > ClientSize.Height);
        if (layout.Controls.Count > 0 && next == portrait && rail.Visible == !expanded)
            return;
        portrait = next;
        layout.SuspendLayout();
        layout.Controls.Clear();
        layout.ColumnStyles.Clear();
        layout.RowStyles.Clear();
        rail.Visible = !expanded;
        layout.ColumnCount = portrait ? 1 : 2;
        layout.RowCount = portrait ? 2 : 1;
        if (portrait)
        {
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 280));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            rail.AutoScroll = true;
            layout.Controls.Add(rail, 0, 0);
            layout.Controls.Add(main, 0, 1);
        }
        else
        {
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, expanded ? 0 : 248));
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            rail.AutoScroll = false;
            layout.Controls.Add(rail, 0, 0);
            layout.Controls.Add(main, 1, 0);
        }
        layout.ResumeLayout();
    }
    void StyleLabel(Label l)
    {
        l.Dock = DockStyle.Fill;
        l.ForeColor = Foreground;
        l.TextAlign = ContentAlignment.MiddleLeft;
        l.Padding = new Padding(8);
        l.Margin = new Padding(2);
    }
    Label Label(string text)
    {
        var l = new Label { Text = text };
        StyleLabel(l);
        return l;
    }
    Button Button(string text, Action action, bool primary = false)
    {
        var b = new Button { Text = text, Dock = DockStyle.Fill, FlatStyle = FlatStyle.Flat, BackColor = ColorTranslator.FromHtml(primary ? "#205c83" : "#1b2b3b"), ForeColor = Foreground, Margin = new Padding(3), MinimumSize = new Size(40, 40), UseVisualStyleBackColor = false };
        b.FlatAppearance.BorderColor = ColorTranslator.FromHtml(primary ? "#4382aa" : "#3b4c5e");
        b.Click += delegate
        {
            try
            {
                action();
            }
            catch (Exception e) { Error(e); }
        };
        return b;
    }
    Control Row(string[] labels, Action[] actions)
    {
        var row = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = labels.Length, RowCount = 1, Margin = Padding.Empty };
        for (int i = 0; i < labels.Length; i++)
        {
            row.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100f / labels.Length));
            row.Controls.Add(Button(labels[i], actions[i]), i, 0);
        }
        return row;
    }
    void Error(Exception e)
    {
        status.Text = e.Message;
        MessageBox.Show(this, e.Message, "OpenSAAB T2", MessageBoxButtons.OK, MessageBoxIcon.Warning);
    }
    void UpdateSelection()
    {
        selection.Text = firmware.Selected == null ? "Firmware setup needed" : firmware.Selected.label;
    }
    void SetBusy(bool value)
    {
        busy = value;
        connect.Enabled = offline.Enabled = !value;
    }
    bool Running
    {
        get
        {
            return session != null && session.Running;
        }
    }
    async Task Stop()
    {
        // Detach before awaiting: the UI timer can observe process exit while
        // Stop is suspended, and must not dispose the process underneath it.
        var stopping = session;
        if (stopping == null)
            return;
        session = null;
        bool clean;
        try { clean = await stopping.Stop(); }
        finally { recentConsole = stopping.ConsoleText; stopping.Dispose(); }
        // Do not leave the last firmware frame looking like a live session.
        lcd.Caption = "Session stopped";
        lcd.Replace(null);
        connect.Visible = offline.Visible = true;
        if (!clean)
            status.Text = "Session stopped forcibly. Adapter cleanup is unconfirmed.";
    }
    async Task Start(bool live, bool freshSecurity = false)
    {
        if (busy)
            return;
        SetBusy(true);
        try
        {
            await Stop();
            if (firmware.Missing() != "")
            {
                FirmwareDialog();
                return;
            }
            lcd.Caption = null;
            lcd.Replace(null);
            session = new EmulatorSession();
            session.Start(root, state, firmware, live ? adapter.Id : null, transport, live ? adapter.Port : null, live ? adapter.Driver : null, transport == WindowsTransport.VendorJ2534 ? ChipsoftSessionMode.ReadOnly : mode, freshSecurity);
            ended = false;
            connect.Visible = offline.Visible = false;
            status.Text = live ? adapter.Name + " · " + TransportPlan.Label(transport) + " · " + SessionMode.Label(transport == WindowsTransport.VendorJ2534 ? ChipsoftSessionMode.ReadOnly : mode) : "Emulation mode · Offline · No vehicle connection";
            vehicle.Text = live ? (vin != "" ? "VIN " + vin + (engineCodes == "" ? "" : "\n" + engineCodes) : "VIN not yet verified") : "Offline exploration · No vehicle connected";
        }
        catch (Exception e) { if (session != null) { session.Dispose(); session = null; } Error(e); }
        finally { SetBusy(false); }
    }
    async Task Connect(bool freshSecurity = false)
    {
        if (busy || Running)
            return;
        SetBusy(true);
        bool ready = false;
        try
        {
            vin = "";
            ClearBluetoothVehicle();
            engineCodes = "";
            status.Text = "Looking for an adapter…";
            var devices = await Task.Run(() => AdapterProfile.Detect());
            if (devices.Length == 0)
            {
                status.Text = "No supported USB adapter detected. Connect Chipsoft Pro or VCX Nano, then retry.";
                return;
            }
            adapter = devices.Length == 1 ? devices[0] : ChooseAdapter(devices);
            if (adapter == null)
                return;
            if (adapter.Id == "nano")
            {
                // Reject an incompatible transport before touching vendor hardware.
                TransportPlan.Arguments(adapter.Id, adapter.Port, transport);
                status.Text = "VCX Nano candidate · Verifying adapter identity…";
                var identity = await AdapterProbe.Run(root, state, adapter, false, transport, true);
                recentConsole = identity.Text;
                if (!identity.NanoIdentityConfirmed)
                    throw new IOException("Could not verify VCX Nano identity and driver cleanup. No ECU request was sent. See Console.");
                adapter.Name = "VCX Nano";
            }
            status.Text = adapter.Name + " · " + TransportPlan.Label(transport) + " · Reading VIN over HS-CAN…";
            var result = await AdapterProbe.Run(root, state, adapter, false, transport);
            recentConsole = result.Text;
            if (!result.Clean || result.Vin == null)
                throw new IOException("Could not verify a fresh VIN and driver cleanup. See Console for the connection log.");
            vin = result.Vin;
            try {File.WriteAllText(Path.Combine(state,"vehicle-status.json"),new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(new System.Collections.Generic.Dictionary<string,string>{{"vin",vin},{"observed_utc",DateTimeOffset.UtcNow.ToString("o")}}));} catch(IOException) {}
            vehicle.Text = "VIN " + vin;
            // Do not run an extra OEM-only diagnostic session before an A/B firmware test.
            // The existing standalone OEM DTC reader remains available through Actions.
            engineCodes = "Transport comparison · Select DTCs in the firmware";
            ready = true;
        }
        catch (Exception e) { Error(e); }
        finally { SetBusy(false); }
        if (ready)
            await Start(true, freshSecurity);
    }
    AdapterProfile ChooseAdapter(AdapterProfile[] devices)
    {
        using (var f = Dialog("Choose adapter", 460, 240))
        {
            var list = new ComboBox { Dock = DockStyle.Top, DropDownStyle = ComboBoxStyle.DropDownList, DataSource = devices };
            f.Controls.Add(list);
            AdapterProfile choice = null;
            var b = Button("Use this adapter", () => { choice = (AdapterProfile)list.SelectedItem; f.Close(); });
            b.Dock = DockStyle.Bottom;
            b.Height = 48;
            f.Controls.Add(b);
            f.ShowDialog(this);
            return choice;
        }
    }
    System.Collections.Generic.Dictionary<string,string> SecurityObservation() {
        try {return new System.Web.Script.Serialization.JavaScriptSerializer().Deserialize<System.Collections.Generic.Dictionary<string,string>>(File.ReadAllText(Path.Combine(state,"vehicle-status.json"))) ?? new System.Collections.Generic.Dictionary<string,string>();} catch {return new System.Collections.Generic.Dictionary<string,string>();}
    }
    void UpdateSecurity() {
        securityPoll=DateTime.UtcNow;
        try {
            var selected=firmware.Selected;string card=selected==null?null:firmware.CardPath(selected);
            if(session!=null)card=Path.Combine(session.RunDirectory,"card.bin");
            else {
                string previous=selected==null?null:SecurityWorkspace.Previous(state,selected.id);
                if(previous!=null)card=Path.Combine(previous,"card.bin");
            }
            string receipt=card==null?"":Path.Combine(Path.GetDirectoryName(card),"security-processing-status.properties");
            if(!File.Exists(receipt))receipt=Path.Combine(state,"security-processing-status.properties");
            var observation=SecurityObservation();securityView=SecurityStatus.Read(card,receipt,observation.ContainsKey("vin")?observation["vin"]:null);
        } catch {securityView=SecurityStatus.Evaluate(null,null,DateTimeOffset.UtcNow,null);}
        securityButton.Text=securityView.Summary+(securityView.State=="PRE_AUTH"?" · Process":"");securityButton.ForeColor=ColorTranslator.FromHtml(securityView.Color);
    }
    void SecurityDetails() {
        UpdateSecurity();var observation=SecurityObservation();
        TextDialog("Vehicle and security status",securityView.Details+BluetoothDetails()+"\n\nLast USB connection · "+(observation.ContainsKey("observed_utc")?observation["observed_utc"]:"Date unknown")+"\nSaved observation; not a live authorization check.\nGet Security Access processes existing PRE-AUTH data, or starts a fresh collection. POST-AUTH records the imported card and its timestamp; follow the original firmware to continue.");
    }
    void Tick()
    {
        if((DateTime.UtcNow-securityPoll).TotalSeconds>=1) UpdateSecurity();
        SecurityWorkflowTick();
        if (session == null)
            return;
        try
        {
            if (!session.Running)
            {
                if (!ended)
                {
                    ended = true;
                    recentConsole = session.ConsoleText;
                    // Classification is shared Rust (adapter_failure.rs) written to report.json;
                    // this shell only presents it, as the Mac shell does.
                    var failure = AdapterFailure.Read(session.RunDirectory);
                    status.Text = failure != null ? failure.Title + " · See Console" : "Session ended · exit " + session.ExitCode + " · See Console";
                    lcd.Caption = failure != null ? "Communication stopped" + Environment.NewLine + failure.Title : "Session ended";
                    lcd.Replace(null);
                    connect.Visible = offline.Visible = true;
                    session.Dispose();
                    session = null;
                    if (failure != null)
                        BeginInvoke(new Action(() => TextDialog(failure.Title, failure.Message, true)));
                }
                return;
            }
            session.Pump();
            var frame = session.Frame();
            if (frame != null)
                lcd.Replace(frame);
        }
        catch (IOException) { }
        catch (Exception e) { status.Text = e.Message; }
    }
    void Key(int code)
    {
        securityNavigating=false;securityNavigationGeneration++;
        if (Running && !busy)
            session.Key(code);
    }
    protected override bool ProcessCmdKey(ref Message msg, Keys key)
    {
        if (Running && !busy)
        {
            int code = -1;
            switch (key)
            {
                case Keys.Enter:
                    code = 16;
                    break;
                case Keys.Escape:
                    code = 1;
                    break;
                case Keys.Up:
                    code = 9;
                    break;
                case Keys.Down:
                    code = 12;
                    break;
                case Keys.Left:
                    code = 11;
                    break;
                case Keys.Right:
                    code = 13;
                    break;
            }
            if (key >= Keys.F1 && key <= Keys.F10)
                code = new[] { 24, 4, 19, 23, 3, 18, 22, 2, 17, 21 }[(int)key - (int)Keys.F1];
            if (code >= 0)
            {
                Key(code);
                return true;
            }
        }
        return base.ProcessCmdKey(ref msg, key);
    }
    Form Dialog(string title, int width, int height)
    {
        return new Form { Text = title, ClientSize = new Size(width, height), BackColor = Background, ForeColor = Foreground, Font = Font, StartPosition = FormStartPosition.CenterParent, MinimizeBox = false, MaximizeBox = false };
    }
    void TextDialog(string title, string text, bool wrap = false)
    {
        using (var f = Dialog(title, 700, 500))
        {
            f.Controls.Add(new TextBox { Text = text.Replace("\r\n", "\n").Replace("\n", "\r\n"), Multiline = true, ReadOnly = true, ScrollBars = wrap ? ScrollBars.Vertical : ScrollBars.Both, Dock = DockStyle.Fill, BackColor = Background, ForeColor = Foreground, Font = wrap ? Font : new Font("Consolas", 10), WordWrap = wrap });
            f.ShowDialog(this);
        }
    }
    void Keypad()
    {
        using (var f = Dialog("Keypad", 390, 450))
        {
            var grid = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 3, RowCount = 7, Padding = new Padding(8) };
            string[] names = { "HELP", "↑", "YES", "←", "ENTER", "→", "", "↓", "NO", "F0", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9" };
            int[] codes = { 25, 9, 14, 11, 16, 13, -1, 12, 20, 24, 4, 19, 23, 3, 18, 22, 2, 17, 21 };
            for (int i = 0; i < 3; i++)
                grid.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 33.33f));
            for (int i = 0; i < 7; i++)
                grid.RowStyles.Add(new RowStyle(SizeType.Percent, 100f / 7));
            for (int i = 0; i < names.Length; i++)
            {
                int c = codes[i];
                if (c >= 0)
                    grid.Controls.Add(Button(names[i], () => Key(c)), i % 3, i / 3);
            }
            f.Controls.Add(grid);
            f.ShowDialog(this);
        }
    }
    void Popup(string[] labels, Action[] actions)
    {
        var menu = new ContextMenuStrip { BackColor = ColorTranslator.FromHtml("#1b2b3b"), ForeColor = Foreground, Font = Font };
        for (int i = 0; i < labels.Length; i++)
        {
            Action action = actions[i];
            menu.Items.Add(labels[i], null, delegate
            {
                try
                {
                    action();
                }
                catch (Exception e) { Error(e); }
            });
        }
        menu.Closed += delegate
        {
            // Dispose after WinForms finishes the close/ItemClicked sequence; a synchronous
            // Dispose here raised ObjectDisposedException (ContextMenuStrip) on every item click.
            BeginInvoke((Action)menu.Dispose);
        };
        menu.Show(Cursor.Position);
    }
    void Unavailable(string action)
    {
        TextDialog(action, "This Windows preview does not yet support this operation through the app interface. No adapter command was sent. Original firmware controls remain available.");
    }
    void ControlsHelp()
    {
        TextDialog("Firmware controls", "Scroll or swipe up/down to move one item. Hold for ENTER. EXIT returns inside the firmware; it does not stop emulation. Keypad contains all firmware keys.");
    }
    void AdapterTools()
    {
        var labels=new System.Collections.Generic.List<string>(new[] { "Transport: " + TransportPlan.Label(transport), "Use OpenSAAB direct USB/CDC (default)", "Use " + TransportPlan.OemComparisonLabel + " (vendor J2534)", "Session mode: " + SessionMode.Label(transport == WindowsTransport.VendorJ2534 ? ChipsoftSessionMode.ReadOnly : mode), "Security Access Password", "Collect fresh security data", "About transport comparison" }); var actions=new System.Collections.Generic.List<Action>(new Action[] {
            () => { }, () => SelectTransport(WindowsTransport.ChipsoftDirect), () => SelectTransport(WindowsTransport.VendorJ2534), SelectSessionMode, PasswordSettings, BeginFreshSecurity,
            () => TextDialog("Transport comparison", "OpenSAAB direct USB/CDC is the default and the supported route. " + TransportPlan.OemComparisonLabel + " is for internal A/B testing against the vendor stack only; it is not a supported adapter path.\r\n\r\nThe x64 app starts an x86 helper for the manufacturer's 32-bit J2534 DLL. Direct USB/CDC uses OpenSAAB's shared Chipsoft backend and is Chipsoft-only.\r\n\r\nEach session stays on its selected path; missing drivers do not cause fallback. Stop emulation before changing transport. The direct route offers Full native control, Read-only diagnostics and Collect security seeds. The OEM comparison stays read-only. A local gate rejection is not an ECU rejection. Direct security collection, API processing, response import and firmware resume use the shared security workflow.\r\n\r\nConnect reads the VIN, then starts firmware without an automatic DTC probe. Each run saves transport.json with the selected route, firmware and executable hashes. The standalone DTC shortcut requires the OEM path.")
        });
        if(PreviewSecurityNoPrompt){labels.RemoveAt(4);actions.RemoveAt(4);}
        Popup(labels.ToArray(),actions.ToArray());
    }
    void SelectTransport(WindowsTransport choice)
    {
        if (busy || Running) { TextDialog("Transport", "Stop the current operation from App menu before changing transport."); return; }
        transport = choice;
        adapter = null; vin = ""; engineCodes = "";
        vehicle.Text = "Connect an adapter to identify your vehicle";
        status.Text = TransportPlan.Label(transport) + " · Ready · " + SessionMode.Label(transport == WindowsTransport.VendorJ2534 ? ChipsoftSessionMode.ReadOnly : mode);
    }
    void SelectSessionMode()
    {
        if (busy || Running) { TextDialog("Session mode", "Stop emulation before changing the session mode."); return; }
        if (transport == WindowsTransport.VendorJ2534) { TextDialog("Session mode", "The OEM comparison route stays read-only. Select direct USB/CDC for seed collection or full native control."); return; }
        Popup(new[] { "Full native control (default)", "Read-only diagnostics", "Collect security seeds" }, new Action[] {
            () => SetSessionMode(ChipsoftSessionMode.FullNative), () => SetSessionMode(ChipsoftSessionMode.ReadOnly), () => SetSessionMode(ChipsoftSessionMode.Seeds)
        });
    }
    void SetSessionMode(ChipsoftSessionMode choice)
    {
        mode = choice; SessionMode.Save(state, mode); status.Text = SessionMode.Label(mode) + " · Ready";
    }
    void CollectSecuritySeeds() { BeginFreshSecurity(); }
    void AppMenu()
    {
        var labels = new[] { "Stop emulation", "Run without an adapter", "Firmware selection", "Adapter & advanced tools", "Report issue", "Console", "Check for updates", "About / Support", "Firmware controls help" };
        var actions = new Action[] {
            async () => { if(bluetoothJob!=null){await bluetoothJob.Stop();return;} if (busy) return; SetBusy(true); await Stop(); status.Text = "Stopped · Ready"; SetBusy(false); },
            async () => { if (busy || Running) return; adapter = null; vin = ""; ClearBluetoothVehicle(); await Start(false); },
            FirmwareDialog,
            BluetoothTools,
            ReportIssue,
            () => TextDialog("Console", Running ? session.ConsoleText : recentConsole),
            () => Unavailable("Check for updates"),
            () => ProjectSupport(),
            ControlsHelp
        };
        if (!Running && bluetoothJob==null) { labels = labels.Skip(1).ToArray(); actions = actions.Skip(1).ToArray(); }
        Popup(labels, actions);
    }
    void Actions()
    {
        Popup(new[] { securityJob!=null ? "Cancel security processing" : securityView.State=="PRE_AUTH" ? "Process security data" : securityView.State=="POST_AUTH" ? "Continue firmware" : "Get security access", "Read engine codes — HS-CAN", "Read ECM information — HS-CAN", "Read DTC", "Clear DTC", "Engine Data", "Saved DTC reports", "Vehicle and security status", expanded ? "Show header" : "Expand screen", "App menu" }, new Action[] {
            SecurityAction,
            async () => await ReadDtc(),
            async () => await BluetoothRead("ecm_info"),
            () => TextDialog("Read DTC", "Use the original firmware keypad to select Diagnostics → model year → vehicle → module → Diagnostic Trouble Codes. Direct app navigation is not implemented here. Live communication requires the appropriate bus. Read engine codes — HS-CAN reads the Trionic 8 ECM directly without SW-CAN."),
            () => Unavailable("Clear DTC"),
            () => TextDialog("Engine Data", "Use the original firmware menu to select Engine → Data Display. Direct app navigation is not implemented here. Live values require an adapter and ECU; offline mode does not supply vehicle data."),
            async () => await SavedBluetoothReports(),
            SecurityDetails,
            () => { expanded = !expanded; Arrange(); }, AppMenu
        });
    }
    async Task ReadDtc()
    {
        if (busy)
            return;
        if (adapter == null || adapter.Id != "chipsoft-pro")
        {
            await BluetoothRead("ecm_dtc");
            return;
        }
        if (Running)
        {
            TextDialog("Engine codes", "Stop emulation from App menu before starting a separate HS-CAN read. The adapter can only serve one session at a time.");
            return;
        }
        SetBusy(true);
        try
        {
            status.Text = "Reading engine codes…";
            var result = await AdapterProbe.Run(root, state, adapter, true, transport);
            recentConsole = result.Text;
            lastReport = AdapterProbe.DtcSummary(result);
            TextDialog("Engine codes", lastReport);
            status.Text = result.Clean ? "Read complete · Adapter released" : "Read incomplete · See Console";
        }
        catch (Exception e) { Error(e); }
        finally { SetBusy(false); }
    }
    void FirmwareDialog()
    {
        if (Running)
        {
            TextDialog("Firmware selection", "Stop emulation before changing firmware. Your current session and card are preserved.");
            return;
        }
        if (busy && firmware.Missing() == "")
            return;
        using (var f = Dialog("Firmware selection", 660, 370))
        {
            var panel = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(16), ColumnCount = 1, RowCount = 5 };
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, 60));
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, 52));
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, 86));
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, 56));
            panel.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            panel.Controls.Add(Label("Choose the language of the original Saab firmware. App controls currently remain in English."), 0, 0);
            var choices = new ComboBox { Dock = DockStyle.Fill, DropDownStyle = ComboBoxStyle.DropDownList, DataSource = firmware.Entries.Where(e => e.runnable).ToArray() };
            if (firmware.Selected != null)
                choices.SelectedItem = firmware.Selected;
            panel.Controls.Add(choices, 0, 1);
            var detail = Label(firmware.Missing() == "" ? "Verified firmware is ready. Switching languages keeps your previous card." : "Missing: " + firmware.Missing() + ". Existing Windows runtime files are imported automatically.");
            panel.Controls.Add(detail, 0, 2);
            bool downloading = false;
            var cancel = new CancellationTokenSource();
            Button download = null;
            download = Button("Download and use", async () => { if (downloading) return; downloading = true; cancel.Dispose(); cancel = new CancellationTokenSource(); download.Enabled = choices.Enabled = false; try { await firmware.Download((FirmwareEntry)choices.SelectedItem, s => detail.Text = s, cancel.Token); UpdateSelection(); detail.Text = "Verified and selected. Ready to start."; } catch (Exception e) { detail.Text = e.Message; } finally { downloading = false; download.Enabled = choices.Enabled = true; } }, true);
            panel.Controls.Add(download, 0, 3);
            panel.Controls.Add(Button("Done / Cancel download", () => { if (downloading) cancel.Cancel(); else f.Close(); }), 0, 4);
            f.FormClosing += delegate (object sender, FormClosingEventArgs e)
            {
                if (downloading)
                    e.Cancel = true;
            };
            f.Controls.Add(panel);
            f.ShowDialog(this);
            cancel.Dispose();
        }
    }
    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            timer.Dispose();
            if (session != null)
                session.Dispose();
        }
        base.Dispose(disposing);
    }
}
