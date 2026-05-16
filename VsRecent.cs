// VsRecent — fast launcher for VSCode "Open Recent" projects.
// Reads %USERPROFILE%\.vscode-shared\sharedStorage\state.vscdb and shows a
// type-to-filter list of recent folders. Enter / double-click launches a
// detached VSCode window for the selected entry, then exits.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Net;
using System.Text.Json;
using System.Windows.Forms;

namespace VsRecent
{
    internal sealed class Entry
    {
        public string FolderUri;
        public string DisplayMain;
        public string DisplaySub;
        public string SearchKey;
        public string PillText;
        public Color PillColor;
    }

    internal sealed class MainForm : Form
    {
        private readonly TextBox _filter;
        private readonly ListBox _list;
        private readonly List<Entry> _all;
        private List<Entry> _shown;

        public MainForm(List<Entry> entries)
        {
            _all = entries;
            _shown = entries;

            Text = "VS Recent";
            ShowIcon = true;
            try
            {
                string exePath = Environment.ProcessPath;
                if (!string.IsNullOrEmpty(exePath))
                    Icon = System.Drawing.Icon.ExtractAssociatedIcon(exePath);
            }
            catch { /* ignore - falls back to default */ }
            ShowInTaskbar = true;
            FormBorderStyle = FormBorderStyle.Sizable;
            MinimumSize = new Size(360, 240);
            ClientSize = new Size(560, 480);
            StartPosition = FormStartPosition.Manual;
            Location = ComputeStartLocation(ClientSize);
            KeyPreview = true;
            BackColor = Color.FromArgb(30, 30, 30);
            ForeColor = Color.FromArgb(220, 220, 220);
            Font = new Font("Segoe UI", 10f);
            Padding = new Padding(8);

            _filter = new TextBox
            {
                Dock = DockStyle.Fill,
                BorderStyle = BorderStyle.FixedSingle,
                BackColor = Color.FromArgb(45, 45, 48),
                ForeColor = Color.White,
                Font = new Font("Segoe UI", 12f),
            };

            var topPanel = new Panel
            {
                Dock = DockStyle.Top,
                Height = 34,
                Padding = new Padding(0, 0, 0, 6),
                BackColor = BackColor,
            };
            topPanel.Controls.Add(_filter);

            _list = new ListBox
            {
                Dock = DockStyle.Fill,
                BorderStyle = BorderStyle.None,
                BackColor = Color.FromArgb(30, 30, 30),
                ForeColor = Color.White,
                Font = new Font("Segoe UI", 10f),
                IntegralHeight = false,
                DrawMode = DrawMode.OwnerDrawFixed,
                ItemHeight = 40,
                SelectionMode = SelectionMode.One,
                TabStop = false,
            };
            _list.DrawItem += List_DrawItem;
            _list.MouseDoubleClick += (s, e) => Launch();
            _list.MouseUp += (s, e) => _filter.Focus();
            _list.KeyDown += (s, e) =>
            {
                if (e.KeyCode == Keys.Enter) { Launch(); e.Handled = true; e.SuppressKeyPress = true; }
                else if (e.KeyCode == Keys.Escape) { Close(); e.Handled = true; e.SuppressKeyPress = true; }
            };

            // Fill control must be added BEFORE the docked-top control so it
            // ends up filling the remaining space below it.
            Controls.Add(_list);
            Controls.Add(topPanel);

            _filter.TextChanged += (s, e) => ApplyFilter();
            _filter.KeyDown += Filter_KeyDown;
            KeyDown += (s, e) =>
            {
                if (e.KeyCode == Keys.Escape) { Close(); e.Handled = true; }
            };

            Load += (s, e) => _filter.Focus();
            Shown += (s, e) => _filter.Focus();

            ApplyFilter();
        }

        private static Point ComputeStartLocation(Size size)
        {
            var screen = Screen.FromPoint(Cursor.Position).WorkingArea;
            int x = screen.Left + (screen.Width - size.Width) / 2;
            int y = screen.Top + (screen.Height - size.Height) / 3;
            return new Point(Math.Max(screen.Left, x), Math.Max(screen.Top, y));
        }

        private void List_DrawItem(object sender, DrawItemEventArgs e)
        {
            if (e.Index < 0 || e.Index >= _shown.Count) return;
            var entry = _shown[e.Index];
            bool selected = (e.State & DrawItemState.Selected) != 0;
            Color bg = selected ? Color.FromArgb(0, 122, 204) : Color.FromArgb(30, 30, 30);
            Color fgMain = Color.White;
            Color fgSub = selected ? Color.FromArgb(220, 235, 255) : Color.FromArgb(140, 140, 140);

            var g = e.Graphics;

            using (var bgBrush = new SolidBrush(bg))
                g.FillRectangle(bgBrush, e.Bounds);

            const int leftPad = 8;
            const int rightPad = 8;
            const int pillHPad = 7;
            const int pillH = 18;
            const int pillRadius = 5;

            using (var fontMain = new Font("Segoe UI", 10f, FontStyle.Regular))
            using (var fontSub = new Font("Segoe UI", 8.25f, FontStyle.Regular))
            using (var fontPill = new Font("Segoe UI", 8f, FontStyle.Bold))
            using (var bMain = new SolidBrush(fgMain))
            using (var bSub = new SolidBrush(fgSub))
            {
                string pillText = entry.PillText ?? "";
                Size pillTextSize = TextRenderer.MeasureText(g, pillText, fontPill,
                    Size.Empty, TextFormatFlags.NoPadding);
                int pillW = pillTextSize.Width + pillHPad * 2;
                int pillX = e.Bounds.Right - rightPad - pillW;
                int pillY = e.Bounds.Top + 4;

                int textLeft = e.Bounds.Left + leftPad;
                int mainMaxW = Math.Max(0, pillX - 6 - textLeft);
                int subMaxW = Math.Max(0, e.Bounds.Right - rightPad - textLeft);

                string main = TruncateToFit(g, entry.DisplayMain ?? "", fontMain, mainMaxW);
                g.DrawString(main, fontMain, bMain, textLeft, e.Bounds.Top + 3);

                string sub = TruncateToFit(g, entry.DisplaySub ?? "", fontSub, subMaxW);
                g.DrawString(sub, fontSub, bSub, textLeft, e.Bounds.Top + 22);

                SmoothingMode prevSmooth = g.SmoothingMode;
                g.SmoothingMode = SmoothingMode.AntiAlias;
                using (var path = RoundedRect(pillX, pillY, pillW, pillH, pillRadius))
                using (var pillBrush = new SolidBrush(entry.PillColor))
                {
                    g.FillPath(pillBrush, path);
                }
                g.SmoothingMode = prevSmooth;

                int pillTextY = pillY + (pillH - pillTextSize.Height) / 2;
                TextRenderer.DrawText(g, pillText, fontPill,
                    new Point(pillX + pillHPad, pillTextY), Color.White,
                    TextFormatFlags.NoPadding);
            }
        }

        private static string TruncateToFit(Graphics g, string text, Font font, float maxWidth)
        {
            if (string.IsNullOrEmpty(text) || maxWidth <= 0) return text;
            if (g.MeasureString(text, font).Width <= maxWidth) return text;
            const string ellipsis = "…";
            int lo = 0, hi = text.Length;
            while (lo < hi)
            {
                int mid = (lo + hi + 1) / 2;
                string candidate = text.Substring(0, mid) + ellipsis;
                if (g.MeasureString(candidate, font).Width <= maxWidth) lo = mid;
                else hi = mid - 1;
            }
            return lo > 0 ? text.Substring(0, lo) + ellipsis : ellipsis;
        }

        private static GraphicsPath RoundedRect(int x, int y, int w, int h, int r)
        {
            var path = new GraphicsPath();
            if (r <= 0 || w <= 0 || h <= 0)
            {
                if (w > 0 && h > 0) path.AddRectangle(new Rectangle(x, y, w, h));
                return path;
            }
            int d = Math.Min(r * 2, Math.Min(w, h));
            path.AddArc(x, y, d, d, 180, 90);
            path.AddArc(x + w - d, y, d, d, 270, 90);
            path.AddArc(x + w - d, y + h - d, d, d, 0, 90);
            path.AddArc(x, y + h - d, d, d, 90, 90);
            path.CloseFigure();
            return path;
        }

        private void ApplyFilter()
        {
            string q = _filter.Text.Trim();
            if (q.Length == 0)
            {
                _shown = _all;
            }
            else
            {
                string[] tokens = q.ToLowerInvariant().Split(
                    new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries);
                var matched = new List<Entry>(_all.Count);
                foreach (var entry in _all)
                {
                    bool ok = true;
                    foreach (var t in tokens)
                    {
                        if (entry.SearchKey.IndexOf(t, StringComparison.Ordinal) < 0)
                        {
                            ok = false;
                            break;
                        }
                    }
                    if (ok) matched.Add(entry);
                }
                _shown = matched;
            }

            _list.BeginUpdate();
            _list.Items.Clear();
            // ListBox.Items.Count drives owner-draw; the boxed values aren't shown.
            for (int i = 0; i < _shown.Count; i++) _list.Items.Add(i);
            if (_shown.Count > 0) _list.SelectedIndex = 0;
            _list.EndUpdate();
        }

        private void Filter_KeyDown(object sender, KeyEventArgs e)
        {
            switch (e.KeyCode)
            {
                case Keys.Down:
                    if (_shown.Count > 0 && _list.SelectedIndex < _shown.Count - 1)
                        _list.SelectedIndex++;
                    e.Handled = true; e.SuppressKeyPress = true;
                    break;
                case Keys.Up:
                    if (_list.SelectedIndex > 0) _list.SelectedIndex--;
                    e.Handled = true; e.SuppressKeyPress = true;
                    break;
                case Keys.PageDown:
                    if (_shown.Count > 0)
                        _list.SelectedIndex = Math.Min(_shown.Count - 1, _list.SelectedIndex + 8);
                    e.Handled = true; e.SuppressKeyPress = true;
                    break;
                case Keys.PageUp:
                    if (_shown.Count > 0)
                        _list.SelectedIndex = Math.Max(0, _list.SelectedIndex - 8);
                    e.Handled = true; e.SuppressKeyPress = true;
                    break;
                case Keys.Home:
                    if (e.Control && _shown.Count > 0)
                    {
                        _list.SelectedIndex = 0;
                        e.Handled = true; e.SuppressKeyPress = true;
                    }
                    break;
                case Keys.End:
                    if (e.Control && _shown.Count > 0)
                    {
                        _list.SelectedIndex = _shown.Count - 1;
                        e.Handled = true; e.SuppressKeyPress = true;
                    }
                    break;
                case Keys.Enter:
                    Launch();
                    e.Handled = true; e.SuppressKeyPress = true;
                    break;
                case Keys.Escape:
                    Close();
                    e.Handled = true; e.SuppressKeyPress = true;
                    break;
            }
        }

        private void Launch()
        {
            int idx = _list.SelectedIndex;
            if (idx < 0 || idx >= _shown.Count) return;
            var entry = _shown[idx];
            try
            {
                Launcher.OpenFolder(entry.FolderUri);
                Application.Exit();
            }
            catch (Exception ex)
            {
                MessageBox.Show(this, "Failed to launch VSCode:\n\n" + ex.Message,
                    "VS Recent", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
    }

    internal static class Launcher
    {
        public static void OpenFolder(string folderUri)
        {
            string codeExe = FindCodeExe();
            var psi = new ProcessStartInfo
            {
                FileName = codeExe,
                Arguments = "--folder-uri \"" + folderUri + "\"",
                UseShellExecute = true,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
            };
            Process.Start(psi);
        }

        public static string FindCodeExe()
        {
            string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string p1 = Path.Combine(local, @"Programs\Microsoft VS Code\Code.exe");
            if (File.Exists(p1)) return p1;

            string pf = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            string p2 = Path.Combine(pf, @"Microsoft VS Code\Code.exe");
            if (File.Exists(p2)) return p2;

            string pfx86 = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);
            string p3 = Path.Combine(pfx86, @"Microsoft VS Code\Code.exe");
            if (File.Exists(p3)) return p3;

            return "code"; // last-resort fallback to PATH
        }
    }

    // Maps a VSCode folderUri to a short pill label + color identifying the
    // remote kind (local, WSL distro, SSH host, dev-container, codespace, …).
    internal static class RemoteClassifier
    {
        // Curated palette: distinct hues that read clearly on the dark list bg
        // and avoid pure red/green (which would imply error/success status).
        private static readonly Color[] HashPalette = new[]
        {
            Color.FromArgb(192,  86,  33), // burnt orange
            Color.FromArgb( 31, 111, 235), // blue
            Color.FromArgb(110,  64, 201), // purple
            Color.FromArgb( 45, 164,  78), // forest green
            Color.FromArgb(191, 135,   0), // gold
            Color.FromArgb(163, 113, 247), // lavender
            Color.FromArgb(  9, 105, 218), // sky blue
            Color.FromArgb(204,  75,  72), // muted red
            Color.FromArgb(  0, 137, 123), // teal
            Color.FromArgb(214,  99,   0), // pumpkin
        };

        public static void Apply(Entry e)
        {
            var (text, color) = Classify(e.FolderUri);
            e.PillText = text;
            e.PillColor = color;
        }

        public static (string text, Color color) Classify(string folderUri)
        {
            if (string.IsNullOrEmpty(folderUri))
                return ("?", Color.FromArgb(90, 90, 90));

            if (folderUri.StartsWith("file:", StringComparison.OrdinalIgnoreCase))
                return ("LOCAL", Color.FromArgb(90, 90, 90));

            if (folderUri.StartsWith("vscode-remote://", StringComparison.OrdinalIgnoreCase))
            {
                string rest = folderUri.Substring("vscode-remote://".Length);
                int slash = rest.IndexOf('/');
                string authority = slash >= 0 ? rest.Substring(0, slash) : rest;
                string decoded;
                try { decoded = Uri.UnescapeDataString(authority); }
                catch { decoded = authority; }

                int plus = decoded.IndexOf('+');
                string kind     = plus >= 0 ? decoded.Substring(0, plus)  : decoded;
                string instance = plus >= 0 ? decoded.Substring(plus + 1) : "";

                switch (kind.ToLowerInvariant())
                {
                    case "wsl":
                        return ("WSL: " + Truncate(NonEmpty(instance, "?"), 18),
                                WslColor(instance));
                    case "ssh-remote":
                        return ("SSH: " + Truncate(NonEmpty(instance, "?"), 18),
                                HashColor(instance));
                    case "dev-container":
                        return ("DEV CONTAINER", Color.FromArgb(36, 150, 237));
                    case "attached-container":
                        return ("CONTAINER", Color.FromArgb(36, 150, 237));
                    case "codespaces":
                        return ("CODESPACE", Color.FromArgb(110, 64, 201));
                    case "tunnel":
                        return ("TUNNEL", Color.FromArgb(0, 137, 123));
                    default:
                        return (Truncate(kind.ToUpperInvariant(), 18), HashColor(kind));
                }
            }

            if (folderUri.StartsWith("vscode-vfs://github", StringComparison.OrdinalIgnoreCase))
                return ("GITHUB", Color.FromArgb(110, 64, 201));

            int colon = folderUri.IndexOf(':');
            string scheme = colon > 0 ? folderUri.Substring(0, colon).ToUpperInvariant() : "?";
            return (Truncate(scheme, 18), HashColor(scheme));
        }

        private static Color WslColor(string distro)
        {
            string d = (distro ?? "").ToLowerInvariant();
            if (d.StartsWith("ubuntu"))   return Color.FromArgb(233,  84,  32);
            if (d.StartsWith("debian"))   return Color.FromArgb(168,  29,  51);
            if (d.StartsWith("kali"))     return Color.FromArgb( 70, 124, 190);
            if (d.StartsWith("alpine"))   return Color.FromArgb( 13,  89, 124);
            if (d.StartsWith("arch"))     return Color.FromArgb( 23, 147, 209);
            if (d.StartsWith("fedora"))   return Color.FromArgb( 41,  65, 114);
            if (d.StartsWith("opensuse") || d.StartsWith("suse"))
                                          return Color.FromArgb(115, 186,  37);
            return HashColor(distro);
        }

        private static Color HashColor(string s)
        {
            if (string.IsNullOrEmpty(s)) return Color.FromArgb(90, 90, 90);
            uint h = 2166136261u;
            for (int i = 0; i < s.Length; i++)
            {
                h ^= char.ToLowerInvariant(s[i]);
                h *= 16777619u;
            }
            return HashPalette[h % (uint)HashPalette.Length];
        }

        private static string NonEmpty(string s, string fallback) =>
            string.IsNullOrEmpty(s) ? fallback : s;

        private static string Truncate(string s, int max) =>
            (s != null && s.Length > max) ? s.Substring(0, max - 1) + "…" : s;
    }

    internal static class Program
    {
        private const string DbRel  = @".vscode-shared\sharedStorage\state.vscdb";
        private const string KeyName = "history.recentlyOpenedPathsList";

        [STAThread]
        public static int Main(string[] args)
        {
            try
            {
                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);

                bool demo = args != null && Array.Exists(args,
                    a => string.Equals(a, "--demo", StringComparison.OrdinalIgnoreCase));

                List<Entry> entries = demo ? DemoEntries() : LoadEntries();
                using (var f = new MainForm(entries))
                {
                    if (demo) f.Text = "VS Recent — demo";
                    Application.Run(f);
                }
                return 0;
            }
            catch (Exception ex)
            {
                MessageBox.Show("VS Recent failed to start:\n\n" + ex,
                    "VS Recent", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return 1;
            }
        }

        private static List<Entry> LoadEntries()
        {
            string profile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            string dbPath = Path.Combine(profile, DbRel);
            if (!File.Exists(dbPath))
                throw new FileNotFoundException(
                    "VSCode shared storage DB not found at: " + dbPath);

            string sql = "SELECT value FROM ItemTable WHERE key='" + KeyName + "' LIMIT 1;";
            string json = ReadJson(dbPath, sql);
            if (string.IsNullOrEmpty(json)) return new List<Entry>();

            using var doc = JsonDocument.Parse(json);
            if (doc.RootElement.ValueKind != JsonValueKind.Object) return new List<Entry>();
            if (!doc.RootElement.TryGetProperty("entries", out var entriesEl)) return new List<Entry>();
            if (entriesEl.ValueKind != JsonValueKind.Array) return new List<Entry>();

            var list = new List<Entry>(entriesEl.GetArrayLength());
            foreach (var item in entriesEl.EnumerateArray())
            {
                if (item.ValueKind != JsonValueKind.Object) continue;
                if (!item.TryGetProperty("folderUri", out var folderEl)) continue; // folders only
                string folderUri = folderEl.ValueKind == JsonValueKind.String ? folderEl.GetString() : null;
                if (string.IsNullOrEmpty(folderUri)) continue;

                string label = null;
                if (item.TryGetProperty("label", out var labelEl) && labelEl.ValueKind == JsonValueKind.String)
                    label = labelEl.GetString();

                string displayMain = !string.IsNullOrEmpty(label) ? label : DefaultLabel(folderUri);
                string displaySub  = folderUri;
                string searchKey   = ((displayMain ?? "") + " " + folderUri).ToLowerInvariant();

                var entry = new Entry
                {
                    FolderUri   = folderUri,
                    DisplayMain = displayMain,
                    DisplaySub  = displaySub,
                    SearchKey   = searchKey,
                };
                RemoteClassifier.Apply(entry);
                list.Add(entry);
            }
            return list;
        }

        // Hardcoded entries for `vsrecent.exe --demo`, used to take screenshots
        // and to eyeball pill colors without needing real VSCode history.
        private static List<Entry> DemoEntries()
        {
            var samples = new (string label, string uri)[]
            {
                ("vsrecent",            "file:///c%3A/Users/caleb/apps/vsrecent"),
                ("nature-2026-draft",   "file:///c%3A/Users/caleb/Documents/papers/nature-2026"),
                (null,                  "file:///c%3A/Users/caleb/experiments/quick-test"),
                ("personal-site",       "file:///c%3A/Users/caleb/code/blog"),
                ("dotfiles",            "vscode-remote://wsl%2BUbuntu/home/caleb/dotfiles"),
                ("ml-training",         "vscode-remote://wsl%2BUbuntu-22.04/home/caleb/work/ml-training"),
                ("docker-stack",        "vscode-remote://wsl%2BDebian/home/caleb/srv/stack"),
                ("linux-kernel",        "vscode-remote://wsl%2BArchLinux/home/caleb/src/linux"),
                ("homelab-nas",         "vscode-remote://ssh-remote%2Bhomelab.lan/srv/nas/config"),
                ("training-rig",        "vscode-remote://ssh-remote%2Bgpu-rig-01/data/runs/2026-05"),
                ("uni-cluster",         "vscode-remote://ssh-remote%2Beuler.ethz.ch/scratch/caleb"),
                ("api-service",         "vscode-remote://dev-container%2B7b22686f7374223a22646f636b6572227d/workspaces/api-service"),
                ("experiments",         "vscode-remote://codespaces%2Bbookish-doodle-9a3f12/workspaces/experiments"),
                ("vscode-docs",         "vscode-vfs://github/microsoft/vscode-docs"),
            };

            var list = new List<Entry>(samples.Length);
            foreach (var (label, uri) in samples)
            {
                string main = !string.IsNullOrEmpty(label) ? label : DefaultLabel(uri);
                var entry = new Entry
                {
                    FolderUri   = uri,
                    DisplayMain = main,
                    DisplaySub  = uri,
                    SearchKey   = (main + " " + uri).ToLowerInvariant(),
                };
                RemoteClassifier.Apply(entry);
                list.Add(entry);
            }
            return list;
        }

        // Read the value, handling WAL mode by snapshotting the DB + WAL + SHM
        // files into temp if a direct read-only open against the live files fails.
        private static string ReadJson(string dbPath, string sql)
        {
            try
            {
                return Sqlite.ReadSingleText(dbPath, sql);
            }
            catch
            {
                string tempDir = Path.Combine(Path.GetTempPath(),
                    "vsrecent_" + Process.GetCurrentProcess().Id);
                Directory.CreateDirectory(tempDir);
                try
                {
                    string tempDb = Path.Combine(tempDir, "state.vscdb");
                    File.Copy(dbPath, tempDb, true);
                    foreach (var ext in new[] { "-wal", "-shm" })
                    {
                        string src = dbPath + ext;
                        if (File.Exists(src)) File.Copy(src, tempDb + ext, true);
                    }
                    return Sqlite.ReadSingleText(tempDb, sql);
                }
                finally
                {
                    try { Directory.Delete(tempDir, true); } catch { /* best effort */ }
                }
            }
        }

        private static string DefaultLabel(string folderUri)
        {
            try
            {
                if (folderUri.StartsWith("file:///", StringComparison.OrdinalIgnoreCase))
                {
                    string path = folderUri.Substring("file:///".Length);
                    path = WebUtility.UrlDecode(path).Replace('/', '\\');
                    return path;
                }
                return folderUri;
            }
            catch
            {
                return folderUri;
            }
        }
    }
}
