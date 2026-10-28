using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

namespace Miy.Setup
{
    internal static class Json
    {
        internal static readonly JavaScriptSerializer Serializer = new JavaScriptSerializer { MaxJsonLength = 1048576 };
        internal static Dictionary<string, object> Parse(string text) { return Serializer.Deserialize<Dictionary<string, object>>(text); }
        internal static string Text(Dictionary<string, object> value, string key)
        {
            object found;
            return value != null && value.TryGetValue(key, out found) && found != null ? Convert.ToString(found) : "";
        }
        internal static bool Flag(Dictionary<string, object> value, string key)
        {
            object found;
            return value != null && value.TryGetValue(key, out found) && found is bool && (bool)found;
        }
        internal static Dictionary<string, object> Object(Dictionary<string, object> value, string key)
        {
            object found;
            return value != null && value.TryGetValue(key, out found) ? found as Dictionary<string, object> : null;
        }
        internal static void Write(string path, object value) { File.WriteAllText(path, Serializer.Serialize(value), new UTF8Encoding(false)); }
    }

    internal sealed class Reply
    {
        internal int Code;
        internal Dictionary<string, object> Envelope;
        internal Dictionary<string, object> Data { get { return Json.Object(Envelope, "data"); } }
        internal string Error { get { return Json.Text(Json.Object(Envelope, "error"), "message"); } }
    }

    // All scripts and the native CLI are extracted into a fresh, private directory.
    // Never execute a helper downloaded from the server entered in the UI.
    internal sealed class Package : IDisposable
    {
        internal readonly string DirectoryPath;
        internal readonly Dictionary<string, object> Metadata;
        internal static string Hash(string path)
        {
            using (SHA256 sha = SHA256.Create())
            using (FileStream stream = File.OpenRead(path))
                return BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
        }
        private static void NoReparse(string path)
        {
            string current = Path.GetFullPath(path);
            if (current.StartsWith(@"\\", StringComparison.Ordinal)) throw new InvalidOperationException("Use a local Windows drive for installer files.");
            while (!String.IsNullOrEmpty(current))
            {
                if ((File.Exists(current) || Directory.Exists(current)) && (File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0)
                    throw new InvalidOperationException("Installer files must not traverse a symbolic link or junction.");
                current = Path.GetDirectoryName(current);
            }
        }
        private void Resource(string name, string output)
        {
            using (Stream input = Assembly.GetExecutingAssembly().GetManifestResourceStream(name))
            {
                if (input == null) throw new InvalidOperationException("The installer resource is missing.");
                using (FileStream target = new FileStream(Path.Combine(DirectoryPath, output), FileMode.CreateNew, FileAccess.Write, FileShare.None)) input.CopyTo(target);
            }
        }
        internal Package()
        {
            if (Environment.OSVersion.Platform != PlatformID.Win32NT || !Environment.Is64BitProcess)
                throw new InvalidOperationException("Use this installer on Windows x64.");
            string root = Path.GetTempPath();
            NoReparse(root);
            DirectoryPath = Path.Combine(root, "MiySetup-" + Guid.NewGuid().ToString("N"));
            DirectorySecurity access = new DirectorySecurity();
            SecurityIdentifier user = WindowsIdentity.GetCurrent().User;
            access.SetOwner(user);
            access.SetAccessRuleProtection(true, false);
            access.AddAccessRule(new FileSystemAccessRule(user, FileSystemRights.FullControl,
                InheritanceFlags.ContainerInherit | InheritanceFlags.ObjectInherit, PropagationFlags.None, AccessControlType.Allow));
            Directory.CreateDirectory(DirectoryPath, access);
            try
            {
                Resource("Miy.Installer.json", "installer.json");
                Metadata = Json.Parse(File.ReadAllText(Path.Combine(DirectoryPath, "installer.json"), new UTF8Encoding(false, true)));
                Resource("Miy.Cli.zip", "cli.zip");
                if (Hash(Path.Combine(DirectoryPath, "cli.zip")) != Json.Text(Metadata, "archiveSha256"))
                    throw new InvalidOperationException("The embedded CLI archive failed its SHA-256 check.");
                Resource("Miy.Bridge.ps1", "bridge.ps1");
                string[] expected = { "miy.exe", "release.json", "install-cli.ps1", "update-cli.ps1", "uninstall-cli.ps1", "windows-cli-common.ps1" };
                HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
                using (ZipArchive archive = ZipFile.OpenRead(Path.Combine(DirectoryPath, "cli.zip")))
                {
                    foreach (ZipArchiveEntry entry in archive.Entries)
                    {
                        if (Array.IndexOf(expected, entry.FullName) < 0 || !names.Add(entry.FullName) || entry.Length > 268435456)
                            throw new InvalidOperationException("The embedded ZIP has an unexpected, duplicated or oversized entry.");
                        entry.ExtractToFile(Path.Combine(DirectoryPath, entry.FullName));
                    }
                }
                if (names.Count != expected.Length) throw new InvalidOperationException("The embedded ZIP is incomplete.");
                if (Hash(Path.Combine(DirectoryPath, "miy.exe")) != Json.Text(Metadata, "cliBinarySha256"))
                    throw new InvalidOperationException("The embedded CLI executable failed its SHA-256 check.");
                Dictionary<string, object> release = Json.Parse(File.ReadAllText(Path.Combine(DirectoryPath, "release.json")));
                if (Json.Text(release, "commit") != Json.Text(Metadata, "cliCommit") || Json.Text(release, "version") != Json.Text(Metadata, "version") ||
                    Json.Text(release, "target") != "x86_64-pc-windows-msvc" || Json.Text(release, "binarySha256") != Json.Text(Metadata, "cliBinarySha256"))
                    throw new InvalidOperationException("The embedded release provenance is inconsistent.");
            }
            catch { Dispose(); throw; }
        }
        internal Reply Run(Dictionary<string, object> request)
        {
            string requestPath = Path.Combine(DirectoryPath, "request-" + Guid.NewGuid().ToString("N") + ".json");
            Json.Write(requestPath, request);
            ProcessStartInfo start = new ProcessStartInfo();
            start.FileName = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), @"WindowsPowerShell\v1.0\powershell.exe");
            start.Arguments = "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File " + Quote(Path.Combine(DirectoryPath, "bridge.ps1")) + " -RequestPath " + Quote(requestPath);
            start.WorkingDirectory = DirectoryPath;
            start.UseShellExecute = false;
            start.CreateNoWindow = true;
            start.RedirectStandardOutput = true;
            start.RedirectStandardError = true;
            start.StandardOutputEncoding = Encoding.UTF8;
            start.StandardErrorEncoding = Encoding.UTF8;
            try
            {
                using (Process process = new Process { StartInfo = start })
                {
                    if (!process.Start()) throw new InvalidOperationException("Windows PowerShell could not start.");
                    Task<string> output = process.StandardOutput.ReadToEndAsync();
                    Task<string> error = process.StandardError.ReadToEndAsync();
                    // Do not kill a live installation transaction. Network and native
                    // config operations have their own bounded timeouts in the bridge.
                    process.WaitForExit();
                    string text = output.Result;
                    string ignored = error.Result; // Never expose raw native output or secrets.
                    Dictionary<string, object> envelope;
                    try { envelope = Json.Parse(text); }
                    catch { throw new InvalidOperationException("The installer helper returned an unreadable result. Existing settings were not reset."); }
                    if (Json.Flag(envelope, "ok") != (process.ExitCode == 0)) throw new InvalidOperationException("The installer helper exit code could not be verified.");
                    return new Reply { Code = process.ExitCode, Envelope = envelope };
                }
            }
            finally { File.Delete(requestPath); }
        }
        internal static string Quote(string value)
        {
            StringBuilder result = new StringBuilder("\"");
            int slashes = 0;
            foreach (char c in value)
            {
                if (c == '\\') { slashes++; continue; }
                if (c == '"') { result.Append('\\', slashes * 2 + 1); result.Append(c); }
                else { result.Append('\\', slashes); result.Append(c); }
                slashes = 0;
            }
            result.Append('\\', slashes * 2); result.Append('"');
            return result.ToString();
        }
        public void Dispose()
        {
            try { if (DirectoryPath != null && Directory.Exists(DirectoryPath)) Directory.Delete(DirectoryPath, true); }
            catch { /* A locked helper is left private; never delete outside this UUID directory. */ }
        }
    }

    internal sealed class SetupForm : Form
    {
        private readonly Package package;
        private readonly string installDirectory;
        private readonly TextBox server = new TextBox();
        private readonly Label existing = new Label();
        private readonly Label status = new Label();
        private readonly Button probe = new Button();
        private readonly Button install = new Button();
        private readonly Button remove = new Button();
        private readonly ProgressBar progress = new ProgressBar();
        private Dictionary<string, object> settings;
        private Dictionary<string, object> installation;
        private bool busy;
        private bool configurationReady;
        private bool installationReady;
        private readonly bool renderOnly;

        private sealed class HelperResult
        {
            internal Reply Reply;
            internal string Error;
            internal bool Success { get { return Reply != null && Reply.Code == 0; } }
        }
        private sealed class Inspection
        {
            internal HelperResult Installation;
            internal HelperResult Settings;
        }

        internal SetupForm(Package package, bool renderOnly)
            : this(package, renderOnly, Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), @"Programs\miy\bin")) { }

        // Only the read-only native renderer supplies a fixture directory.
        // The interactive installer always uses the fixed per-user directory.
        private SetupForm(Package package, bool renderOnly, string installDirectory)
        {
            this.package = package; this.renderOnly = renderOnly; this.installDirectory = installDirectory;
            Text = "MIY CLI 설치 · Windows x64";
            Font = new Font("Malgun Gothic", 9.5F);
            AutoScaleMode = AutoScaleMode.Dpi;
            ClientSize = new Size(760, Math.Min(552, Screen.PrimaryScreen.WorkingArea.Height - 90));
            MinimumSize = new Size(720, 550);
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.White;
            TableLayoutPanel main = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(24), ColumnCount = 1, RowCount = 11, AutoScroll = true, AutoScrollMinSize = new Size(0, 545) };
            main.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            Controls.Add(main);
            Add(main, new Label { Text = "회사 서버에 연결할 MIY CLI 설치", Font = new Font(Font.FontFamily, 17, FontStyle.Bold), AutoSize = true }, 44);
            Add(main, new Label { Text = "Windows x64 · CLI " + Json.Text(package.Metadata, "version") + " · 현재 사용자 계정에 설치", AutoSize = true }, 28);
            existing.Text = "CLI 설치 상태와 서버 설정을 확인하고 있습니다…";
            existing.Dock = DockStyle.Fill;
            existing.ForeColor = Color.DimGray;
            existing.AutoEllipsis = true;
            Add(main, existing, 42);
            Add(main, new Label { Text = "대상 서버 주소", AutoSize = true }, 25);
            TableLayoutPanel address = new TableLayoutPanel { ColumnCount = 2, Dock = DockStyle.Fill, Margin = Padding.Empty };
            address.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); address.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 115));
            server.Name = "ServerUrl"; server.Dock = DockStyle.Fill; server.Margin = new Padding(0, 3, 10, 3); server.MaxLength = 2048;
            probe.Name = "Probe"; probe.Text = "연결 확인"; probe.Dock = DockStyle.Fill; probe.Margin = new Padding(0);
            address.Controls.Add(server, 0, 0); address.Controls.Add(probe, 1, 0);
            Add(main, address, 34);
            Add(main, new Label { Text = "http://서버 IP:포트 또는 https://회사 도메인 주소를 입력하세요.", AutoSize = true, ForeColor = Color.DimGray }, 37);
            status.Text = "CLI 설치 상태를 확인하고 있습니다…";
            status.Dock = DockStyle.Fill; status.Padding = new Padding(9); status.BackColor = Color.FromArgb(242, 245, 249);
            Add(main, status, 90);
            progress.Dock = DockStyle.Fill; progress.Height = 7; progress.Visible = false;
            Add(main, progress, 12);
            FlowLayoutPanel actions = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.RightToLeft, WrapContents = false, Padding = new Padding(0, 5, 0, 0) };
            install.Name = "Install"; install.Text = "설치"; install.Size = new Size(115, 36);
            remove.Name = "Remove"; remove.Text = "CLI 제거"; remove.Size = new Size(110, 36);
            actions.Controls.Add(install); actions.Controls.Add(remove);
            Add(main, actions, 50);
            Add(main, new Label { Text = "설치 후 새 터미널에서 miy auth login 으로 로그인하세요.\r\nCLI를 제거해도 서버 설정과 로그인 정보는 보존됩니다.", AutoSize = true, ForeColor = Color.DimGray }, 40);
            main.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            probe.Click += async delegate { await Probe(); };
            install.Click += async delegate { await Install(); };
            remove.Click += async delegate { await Remove(); };
            server.TextChanged += delegate { UpdateControls(); };
            FormClosing += delegate(object sender, FormClosingEventArgs args) { if (busy) { args.Cancel = true; status.Text = "작업이 끝날 때까지 기다려 주세요. 파일 교체 중에는 창을 닫을 수 없습니다."; } };
            Shown += async delegate { if (!renderOnly) await Inspect(); };
            UpdateControls();
        }
        private static void Add(TableLayoutPanel panel, Control child, int height)
        {
            int row = panel.Controls.Count;
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, height));
            panel.Controls.Add(child, 0, row);
        }
        private void UpdateControls()
        {
            bool editable = !busy && configurationReady;
            bool present = Json.Flag(installation, "present");
            bool current = Json.Flag(installation, "current");
            bool recovery = Json.Flag(installation, "recoveryRequired");
            bool hasServer = !String.IsNullOrWhiteSpace(server.Text);
            server.Enabled = editable;
            probe.Enabled = editable && hasServer;
            install.Enabled = editable && installationReady && !current && hasServer;
            remove.Enabled = !busy && installationReady && (present || recovery);
            install.Text = recovery ? "복구 설치" : present && !current ? "업데이트" : "설치";
            progress.Visible = busy; progress.Style = busy ? ProgressBarStyle.Marquee : ProgressBarStyle.Blocks;
            UseWaitCursor = busy;
        }
        private HelperResult Invoke(Dictionary<string, object> request)
        {
            try
            {
                Reply reply = package.Run(request);
                return new HelperResult { Reply = reply, Error = reply.Code == 0 ? null : reply.Error };
            }
            catch (Exception error) { return new HelperResult { Error = error.Message }; }
        }
        private Inspection ReadState()
        {
            // Installation ownership and settings are independent: damaged
            // configuration must not prevent removal of an owned CLI.
            return new Inspection {
                Installation = Invoke(new Dictionary<string, object> { { "action", "installation" }, { "installDir", installDirectory } }),
                Settings = Invoke(new Dictionary<string, object> { { "action", "inspect" } })
            };
        }
        private string ApplyState(Inspection snapshot, bool prefillServer)
        {
            installationReady = snapshot.Installation.Success;
            installation = installationReady ? snapshot.Installation.Reply.Data : null;
            configurationReady = snapshot.Settings.Success;
            settings = configurationReady ? snapshot.Settings.Reply.Data : null;
            string origin = Json.Text(settings, "activeOrigin");
            existing.Text = !configurationReady ? "기존 서버 설정을 확인하지 못했습니다. CLI가 설치되어 있으면 제거할 수 있습니다." : origin.Length > 0 ? "현재 서버: " + origin : "회사에서 안내한 서버 주소를 입력하세요.";
            if (prefillServer) server.Text = origin; // Only a real saved selection may prefill this field.
            UpdateControls();
            string error = snapshot.Installation.Error ?? "";
            if (!String.IsNullOrEmpty(snapshot.Settings.Error)) error += (error.Length > 0 ? "\r\n" : "") + snapshot.Settings.Error;
            return error;
        }
        private string DefaultStatus()
        {
            if (Json.Flag(installation, "recoveryRequired")) return "이전 설치가 완료되지 않았습니다. 서버 주소를 확인하고 복구 설치를 진행하세요.";
            if (Json.Flag(installation, "current")) return "MIY CLI " + Json.Text(package.Metadata, "version") + "이 설치되어 있습니다.\r\n새 터미널에서 miy auth login 으로 로그인할 수 있습니다.";
            if (Json.Flag(installation, "present")) return "설치된 CLI를 이 설치 파일의 버전으로 업데이트할 수 있습니다. 서버 주소를 확인하세요.";
            return "서버 주소를 입력한 뒤 설치를 누르세요. 연결 상태를 먼저 확인합니다.";
        }
        private void SetStatus(string message, bool failed)
        {
            status.ForeColor = failed ? Color.DarkRed : Color.Black;
            status.Text = message;
        }
        private async Task RefreshState(bool prefillServer, string message, string operationError)
        {
            Inspection snapshot = await Task.Run(delegate { return ReadState(); });
            string inspectionError = ApplyState(snapshot, prefillServer);
            string error = operationError ?? "";
            if (inspectionError.Length > 0 && inspectionError != error) error += (error.Length > 0 ? "\r\n" : "") + inspectionError;
            SetStatus(error.Length > 0 ? error : message ?? DefaultStatus(), error.Length > 0);
        }
        private async Task Perform(Func<Task> action, string message)
        {
            if (busy) return;
            busy = true; SetStatus(message, false); UpdateControls();
            try { await action(); }
            catch (Exception error) { SetStatus(error.Message, true); }
            finally { busy = false; UpdateControls(); }
        }
        private async Task Inspect()
        {
            await Perform(async delegate { await RefreshState(true, null, null); }, "CLI 설치 상태와 서버 설정을 확인하고 있습니다…");
        }
        private async Task Probe()
        {
            if (!probe.Enabled) return;
            string origin = server.Text;
            await Perform(async delegate {
                HelperResult result = await Task.Run(delegate { return Invoke(new Dictionary<string, object> { { "action", "probe" }, { "server", origin } }); });
                SetStatus(result.Success ? "연결 확인 완료: " + Json.Text(result.Reply.Data, "origin") + "\r\nMIY CLI와 호환되는 서버입니다." : result.Error, !result.Success);
            }, "서버 연결을 확인하고 있습니다…");
        }
        private async Task Install()
        {
            if (!install.Enabled) return;
            Dictionary<string, object> request = new Dictionary<string, object> {
                { "action", "install" }, { "server", server.Text }, { "installDir", installDirectory }, { "addToPath", true },
                { "autoSelectContext", true }, { "expectedConfigHash", Json.Text(settings, "configHash") }
            };
            await Perform(async delegate {
                HelperResult result = await Task.Run(delegate { return Invoke(request); });
                string message = result.Success ? "설치 완료 · 서버: " + Json.Text(result.Reply.Data, "origin") + "\r\n새 터미널에서 miy auth login 으로 로그인하세요." : null;
                // A configuration error can occur after the binary commit.
                // Always reread disk state, without hiding the original error.
                await RefreshState(false, message, result.Error);
            }, "서버 확인 → CLI 설치 → 서버 설정 등록 중입니다…");
        }
        private async Task Remove()
        {
            if (!remove.Enabled) return;
            if (MessageBox.Show(this, "MIY CLI를 제거할까요?\r\n\r\n서버 설정, 로그인 정보, 프로젝트와 백업은 보존됩니다.", "MIY CLI 제거", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
            await Perform(async delegate {
                HelperResult result = await Task.Run(delegate { return Invoke(new Dictionary<string, object> { { "action", "uninstall" }, { "installDir", installDirectory }, { "removeFromPath", true } }); });
                await RefreshState(false, result.Success ? "CLI 제거 완료. 서버 설정과 로그인 정보는 보존했습니다." : null, result.Error);
            }, "CLI를 제거하고 있습니다…");
        }
        private void Capture(string output)
        {
            Show(); Application.DoEvents();
            if (!String.IsNullOrEmpty(output))
                using (Bitmap bitmap = new Bitmap(Width, Height)) { DrawToBitmap(bitmap, new Rectangle(Point.Empty, Size)); bitmap.Save(output, System.Drawing.Imaging.ImageFormat.Png); }
        }
        private static int CountControls<T>(Control root) where T : Control
        {
            int count = 0;
            foreach (Control child in root.Controls) { if (child is T) count++; count += CountControls<T>(child); }
            return count;
        }
        internal void Render(string output)
        {
            settings = new Dictionary<string, object>(); installation = new Dictionary<string, object>();
            configurationReady = true; installationReady = true;
            existing.Text = "회사에서 안내한 서버 주소를 입력하세요.";
            SetStatus(DefaultStatus(), false); UpdateControls();
            Capture(output);
            if (server.Text.Length != 0 || install.Enabled || remove.Enabled) throw new InvalidOperationException("A fresh installer must have a blank server field and disabled install/remove buttons.");
            Hide();
        }
        internal static void RenderState(Package package, Dictionary<string, object> request, string output)
        {
            // This mode only inspects fixture files and renders the production
            // controls. It cannot install, remove, or rewrite configuration.
            string directory = Json.Text(request, "installDir");
            if (String.IsNullOrWhiteSpace(directory)) throw new InvalidOperationException("A native UI fixture installation directory is required.");
            using (SetupForm form = new SetupForm(package, true, directory))
            {
                string error = form.ApplyState(form.ReadState(), true);
                form.SetStatus(error.Length == 0 ? form.DefaultStatus() : error, error.Length != 0);
                if (request.ContainsKey("serverOverride")) form.server.Text = Json.Text(request, "serverOverride");
                form.busy = Json.Flag(request, "busy"); form.UpdateControls();
                form.Capture(Json.Text(request, "screenshot"));
                Json.Write(output, new { ok = true, data = new {
                    installEnabled = form.install.Enabled, removeEnabled = form.remove.Enabled,
                    serverEnabled = form.server.Enabled, probeEnabled = form.probe.Enabled,
                    server = form.server.Text, installText = form.install.Text,
                    present = Json.Flag(form.installation, "present"), current = Json.Flag(form.installation, "current"),
                    recoveryRequired = Json.Flag(form.installation, "recoveryRequired"),
                    configurationReady = form.configurationReady, installationReady = form.installationReady,
                    busy = form.busy, error = error,
                    buttonCount = CountControls<Button>(form), textBoxCount = CountControls<TextBox>(form), checkBoxCount = CountControls<CheckBox>(form)
                } });
                form.busy = false; form.Hide();
            }
        }
    }

    internal static class Program
    {
        [STAThread]
        private static int Main(string[] args)
        {
            string resultPath = null;
            try
            {
                // A relative override is relative to the caller, never to the
                // disposable extraction directory used by helper processes.
                string configDirectory = Environment.GetEnvironmentVariable("MIY_CONFIG_DIR");
                if (!String.IsNullOrEmpty(configDirectory)) Environment.SetEnvironmentVariable("MIY_CONFIG_DIR", Path.GetFullPath(configDirectory));
                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);
                using (Package package = new Package())
                {
                    // Test automation uses the same packaged bridge, hashes and
                    // validation as the GUI. No capability or integrity bypass.
                    if (args.Length == 2 && args[0] == "--verify")
                    {
                        resultPath = args[1]; Json.Write(resultPath, package.Metadata); return 0;
                    }
                    if (args.Length == 3 && args[0] == "--bridge")
                    {
                        resultPath = args[2];
                        if (new FileInfo(args[1]).Length > 1048576) throw new InvalidOperationException("Installer request is too large.");
                        Reply reply = package.Run(Json.Parse(File.ReadAllText(args[1], new UTF8Encoding(false, true))));
                        Json.Write(resultPath, reply.Envelope); return reply.Code;
                    }
                    if (args.Length == 3 && args[0] == "--render-state")
                    {
                        resultPath = args[2];
                        if (new FileInfo(args[1]).Length > 1048576) throw new InvalidOperationException("Native UI fixture request is too large.");
                        SetupForm.RenderState(package, Json.Parse(File.ReadAllText(args[1], new UTF8Encoding(false, true))), resultPath);
                        return 0;
                    }
                    if (args.Length == 2 && args[0] == "--render")
                    {
                        using (SetupForm form = new SetupForm(package, true)) form.Render(args[1]);
                        return 0;
                    }
                    if (args.Length != 0) return 2;
                    using (SetupForm form = new SetupForm(package, false)) Application.Run(form);
                }
                return 0;
            }
            catch (Exception error)
            {
                string message = error is InvalidOperationException ? error.Message : "The installer could not start. Check Windows x64, available disk space and access to your user temporary directory.";
                if (resultPath != null)
                {
                    try { Json.Write(resultPath, new { ok = false, error = new { code = 7, message = message } }); } catch { }
                }
                else if (args.Length == 0) MessageBox.Show(message, "MIY CLI 설치", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return 7;
            }
        }
    }
}
