// VsRecent — fast launcher for VSCode "Open Recent" projects.
// Reads %USERPROFILE%\.vscode-shared\sharedStorage\state.vscdb and shows a
// type-to-filter list of recent folders. Enter / double-click launches a
// detached VSCode window for the selected entry, then exits.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
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

            using (var bgBrush = new SolidBrush(bg))
                e.Graphics.FillRectangle(bgBrush, e.Bounds);

            using (var fontMain = new Font("Segoe UI", 10f, FontStyle.Regular))
            using (var fontSub = new Font("Segoe UI", 8.25f, FontStyle.Regular))
            using (var bMain = new SolidBrush(fgMain))
            using (var bSub = new SolidBrush(fgSub))
            {
                e.Graphics.DrawString(entry.DisplayMain, fontMain, bMain,
                    e.Bounds.Left + 8, e.Bounds.Top + 3);
                e.Graphics.DrawString(entry.DisplaySub, fontSub, bSub,
                    e.Bounds.Left + 8, e.Bounds.Top + 21);
            }
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

    internal static class Program
    {
        private const string DbRel  = @".vscode-shared\sharedStorage\state.vscdb";
        private const string KeyName = "history.recentlyOpenedPathsList";

        [STAThread]
        public static int Main()
        {
            try
            {
                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);

                List<Entry> entries = LoadEntries();
                using (var f = new MainForm(entries))
                {
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

                list.Add(new Entry
                {
                    FolderUri   = folderUri,
                    DisplayMain = displayMain,
                    DisplaySub  = displaySub,
                    SearchKey   = searchKey,
                });
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
