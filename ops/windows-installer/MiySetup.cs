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
        private readonly TextBox server = new TextBox();
        private readonly TextBox contextName = new TextBox();
        private readonly TextBox installDir = new TextBox();
        private readonly CheckBox chooseContext = new CheckBox();
        private readonly CheckBox path = new CheckBox();
        private readonly Label existing = new Label();
        private readonly Label status = new Label();
        private readonly Button probe = new Button();
        private readonly Button install = new Button();
        private readonly Button login = new Button();
        private readonly Button remove = new Button();
        private readonly Button browse = new Button();
        private readonly ProgressBar progress = new ProgressBar();
        private Dictionary<string, object> settings;
        private Dictionary<string, object> installed;
        private bool busy;
        private bool ready;
        private readonly bool renderOnly;

        internal SetupForm(Package package, bool renderOnly)
        {
            this.package = package; this.renderOnly = renderOnly;
            Text = "MIY CLI 설치 · Windows x64";
            Font = new Font("Malgun Gothic", 9.5F);
            AutoScaleMode = AutoScaleMode.Dpi;
            ClientSize = new Size(780, Math.Min(740, Screen.PrimaryScreen.WorkingArea.Height - 90));
            MinimumSize = new Size(720, 600);
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.White;
            TableLayoutPanel main = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(26), ColumnCount = 1, RowCount = 16, AutoScroll = true, AutoScrollMinSize = new Size(0, 717) };
            main.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            Controls.Add(main);
            Add(main, new Label { Text = "회사 서버에 연결할 MIY CLI 설치", Font = new Font(Font.FontFamily, 17, FontStyle.Bold), AutoSize = true }, 48);
            Add(main, new Label { Text = "Windows x64 · CLI " + Json.Text(package.Metadata, "version") + " · 현재 사용자 계정에 설치", AutoSize = true }, 30);
            existing.Text = "기존 서버 설정을 확인하고 있습니다…";
            existing.Dock = DockStyle.Fill;
            existing.ForeColor = Color.DimGray;
            existing.AutoEllipsis = true;
            Add(main, existing, 46);
            Add(main, new Label { Text = "대상 서버 주소", AutoSize = true }, 25);
            TableLayoutPanel address = new TableLayoutPanel { ColumnCount = 2, Dock = DockStyle.Fill, Margin = Padding.Empty };
            address.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); address.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 115));
            server.Name = "ServerUrl"; server.Dock = DockStyle.Fill; server.Margin = new Padding(0, 3, 10, 3); server.MaxLength = 2048;
            probe.Text = "연결 확인"; probe.Dock = DockStyle.Fill; probe.Margin = new Padding(0);
            address.Controls.Add(server, 0, 0); address.Controls.Add(probe, 1, 0);
            Add(main, address, 34);
            Add(main, new Label { Text = "http://서버 IP:포트 또는 https://회사 도메인 주소를 입력하세요. 기본 서버는 없습니다.", AutoSize = true, ForeColor = Color.DimGray }, 32);
            chooseContext.Text = "기존 설정을 보존하고 이 서버의 컨텍스트를 추가하거나 선택";
            chooseContext.Dock = DockStyle.Fill; chooseContext.Enabled = false;
            Add(main, chooseContext, 31);
            TableLayoutPanel contextRow = new TableLayoutPanel { ColumnCount = 2, Dock = DockStyle.Fill, Margin = Padding.Empty };
            contextRow.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 120)); contextRow.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            contextRow.Controls.Add(new Label { Text = "컨텍스트 이름", Dock = DockStyle.Fill, TextAlign = ContentAlignment.MiddleLeft }, 0, 0);
            contextName.Text = "work"; contextName.Dock = DockStyle.Fill; contextName.Enabled = false; contextName.MaxLength = 48;
            contextRow.Controls.Add(contextName, 1, 0);
            Add(main, contextRow, 34);
            Add(main, new Label { Text = "설치 폴더", AutoSize = true }, 29);
            TableLayoutPanel directoryRow = new TableLayoutPanel { ColumnCount = 2, Dock = DockStyle.Fill, Margin = Padding.Empty };
            directoryRow.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); directoryRow.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 90));
            installDir.Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), @"Programs\miy\bin");
            installDir.Dock = DockStyle.Fill; installDir.Margin = new Padding(0, 3, 10, 3);
            browse.Text = "찾아보기"; browse.Dock = DockStyle.Fill; browse.Margin = Padding.Empty;
            directoryRow.Controls.Add(installDir, 0, 0); directoryRow.Controls.Add(browse, 1, 0);
            Add(main, directoryRow, 34);
            path.Text = "사용자 PATH에 추가 (새 터미널에서 miy 명령 사용)"; path.Checked = true; path.Dock = DockStyle.Fill;
            Add(main, path, 33);
            status.Text = "서버 주소를 입력한 뒤 설치를 누르세요. 연결 상태를 먼저 확인합니다.";
            status.Dock = DockStyle.Fill; status.Padding = new Padding(9); status.BackColor = Color.FromArgb(242, 245, 249);
            Add(main, status, 94);
            progress.Dock = DockStyle.Fill; progress.Height = 7; progress.Visible = false;
            Add(main, progress, 15);
            FlowLayoutPanel actions = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.RightToLeft, WrapContents = false, Padding = new Padding(0, 5, 0, 0) };
            install.Text = "설치"; install.Size = new Size(115, 36); install.Enabled = false;
            login.Text = "브라우저 로그인"; login.Size = new Size(150, 36); login.Enabled = false;
            remove.Text = "CLI 제거"; remove.Size = new Size(110, 36); remove.Enabled = false;
            actions.Controls.Add(install); actions.Controls.Add(login); actions.Controls.Add(remove);
            Add(main, actions, 50);
            Add(main, new Label { Text = "업데이트 / 제거: 이 설치 파일을 다시 실행하세요. 서버 설정과 로그인 정보는 유지됩니다.", AutoSize = true, ForeColor = Color.DimGray }, 30);
            main.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            probe.Click += async delegate { await Probe(); };
            install.Click += async delegate { await Install(); };
            login.Click += async delegate { await Login(); };
            remove.Click += async delegate { await Remove(); };
            browse.Click += delegate { using (FolderBrowserDialog dialog = new FolderBrowserDialog { SelectedPath = installDir.Text, Description = "현재 사용자 계정의 MIY CLI 설치 폴더" }) if (dialog.ShowDialog(this) == DialogResult.OK) installDir.Text = dialog.SelectedPath; };
            chooseContext.CheckedChanged += delegate { UpdateControls(); };
            server.TextChanged += delegate { installed = null; UpdateControls(); };
            installDir.TextChanged += delegate { installed = null; UpdateControls(); };
            FormClosing += delegate(object sender, FormClosingEventArgs args) { if (busy) { args.Cancel = true; status.Text = "설치 작업이 끝날 때까지 기다려 주세요. 파일 교체 중에는 창을 닫을 수 없습니다."; } };
            Shown += async delegate { if (!renderOnly) await Inspect(); };
        }
        private static void Add(TableLayoutPanel panel, Control child, int height)
        {
            int row = panel.Controls.Count;
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, height));
            panel.Controls.Add(child, 0, row);
        }
        private void UpdateControls()
        {
            bool editable = !busy && ready;
            server.Enabled = editable; installDir.Enabled = !busy; browse.Enabled = !busy; path.Enabled = !busy;
            probe.Enabled = editable; install.Enabled = editable; remove.Enabled = !busy;
            chooseContext.Enabled = editable && Json.Flag(settings, "exists");
            contextName.Enabled = chooseContext.Enabled && chooseContext.Checked;
            login.Enabled = editable && installed != null;
            progress.Visible = busy; progress.Style = busy ? ProgressBarStyle.Marquee : ProgressBarStyle.Blocks;
            UseWaitCursor = busy;
        }
        private async Task<Reply> Execute(Dictionary<string, object> request, string message)
        {
            busy = true; status.ForeColor = Color.Black; status.Text = message; UpdateControls();
            try
            {
                Reply reply = await Task.Run(delegate { return package.Run(request); });
                if (reply.Code != 0) { status.ForeColor = Color.DarkRed; status.Text = reply.Error; }
                return reply;
            }
            catch (Exception error)
            {
                status.ForeColor = Color.DarkRed; status.Text = error.Message;
                return null;
            }
            finally { busy = false; UpdateControls(); }
        }
        private async Task Inspect()
        {
            Reply reply = await Execute(new Dictionary<string, object> { { "action", "inspect" } }, "기존 서버 설정을 확인하고 있습니다…");
            if (reply == null || reply.Code != 0) { existing.Text = "설정 확인에 실패했습니다. 오류를 해결한 뒤 설치 파일을 다시 실행하세요."; return; }
            settings = reply.Data; ready = true;
            string origin = Json.Text(settings, "activeOrigin");
            existing.Text = origin.Length > 0 ? "현재 서버 (" + Json.Text(settings, "active") + "): " + origin : Json.Flag(settings, "exists") ? "기존 설정에 선택된 서버가 없습니다. 아래에서 새 컨텍스트를 명시적으로 추가하세요." : "처음 설치합니다. 회사에서 안내한 서버 주소를 입력하세요.";
            server.Text = origin; // Only a real saved selection may prefill this field.
            status.Text = "서버 주소를 입력한 뒤 설치를 누르세요. 연결 상태를 먼저 확인합니다.";
            UpdateControls();
        }
        private async Task Probe()
        {
            Reply reply = await Execute(new Dictionary<string, object> { { "action", "probe" }, { "server", server.Text } }, "서버 연결과 MIY 로그인 API를 확인하고 있습니다…");
            if (reply != null && reply.Code == 0) status.Text = "연결 확인 완료: " + Json.Text(reply.Data, "origin") + "\r\nMIY CLI와 호환되는 서버입니다.";
        }
        private async Task Install()
        {
            Dictionary<string, object> request = new Dictionary<string, object> {
                { "action", "install" }, { "server", server.Text }, { "installDir", installDir.Text }, { "addToPath", path.Checked },
                { "selectContext", chooseContext.Checked }, { "contextName", contextName.Text }, { "expectedConfigHash", Json.Text(settings, "configHash") }
            };
            Reply reply = await Execute(request, "서버 확인 → CLI 파일 설치 → 서버 설정 등록 중입니다…");
            if (reply != null && reply.Code == 0)
            {
                installed = reply.Data;
                status.Text = "설치 완료 · 서버: " + Json.Text(installed, "origin") + "\r\n새 터미널에서 miy --help 를 실행하세요. 브라우저 로그인 버튼으로 인증할 수 있습니다.";
                // Refresh the expected snapshot without changing the chosen origin.
                Reply snapshot = await Execute(new Dictionary<string, object> { { "action", "inspect" } }, status.Text);
                if (snapshot != null && snapshot.Code == 0) settings = snapshot.Data;
                UpdateControls();
            }
        }
        private async Task Login()
        {
            if (installed == null) return;
            Reply reply = await Execute(new Dictionary<string, object> {
                { "action", "login" }, { "installDir", Json.Text(installed, "directory") },
                { "contextName", Json.Text(installed, "context") }, { "server", Json.Text(installed, "origin") }
            }, "CLI 로그인 창을 열고 있습니다…");
            if (reply != null && reply.Code == 0) status.Text = "CLI 로그인 창이 열렸습니다. 브라우저에서 코드를 승인하고 CLI의 인증 완료 메시지를 확인하세요.";
        }
        private async Task Remove()
        {
            if (MessageBox.Show(this, "이 폴더의 MIY CLI를 제거할까요?\r\n\r\n" + installDir.Text + "\r\n\r\n서버 설정, 로그인 정보, 프로젝트와 백업은 보존됩니다." + (path.Checked ? "\r\n사용자 PATH에서도 이 폴더를 제거합니다." : ""), "MIY CLI 제거", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
            Reply reply = await Execute(new Dictionary<string, object> { { "action", "uninstall" }, { "installDir", installDir.Text }, { "removeFromPath", path.Checked } }, "CLI를 제거하고 있습니다…");
            if (reply != null && reply.Code == 0) { installed = null; status.Text = "CLI 제거 완료. 서버 설정과 로그인 정보는 보존했습니다."; UpdateControls(); }
        }
        internal void Render(string output)
        {
            existing.Text = "처음 설치합니다. 회사에서 안내한 서버 주소를 입력하세요.";
            ready = true; UpdateControls();
            Show(); Application.DoEvents();
            if (server.Text.Length != 0) throw new InvalidOperationException("A fresh installer must have a blank server field.");
            using (Bitmap bitmap = new Bitmap(Width, Height)) { DrawToBitmap(bitmap, new Rectangle(Point.Empty, Size)); bitmap.Save(output, System.Drawing.Imaging.ImageFormat.Png); }
            Hide();
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
