# VS Recent

Tiny Windows GUI launcher for VSCode "Open Recent" projects. Type to filter,
Enter (or double-click) to open.

![VS Recent demo screenshot](images/screenshot.png)

- Single-file, self-contained .NET 9 WinForms app. No runtime install needed
  on the target machine.
- Native builds for **x64** and **ARM64** Windows.
- Reads VSCode's recent-folders list directly from
  `%USERPROFILE%\.vscode-shared\sharedStorage\state.vscdb` via
  `winsqlite3.dll` (built into Windows). No bundled native libs.
- Includes remote entries: WSL, SSH, Dev Containers — everything from
  *File → Open Recent → More…* in VSCode. Each row is tagged with a colored
  pill showing the remote kind (`LOCAL`, `WSL: Ubuntu`, `SSH: hostname`,
  `DEV CONTAINER`, `CODESPACE`, `GITHUB`, …).
- Launches `Code.exe --folder-uri "<uri>"` detached, then exits. The VSCode
  window keeps running independently of this app.

## Download

Grab the latest release for your architecture from the
[Releases page](../../releases):

- `vsrecent-win-x64.exe` — Intel/AMD 64-bit Windows
- `vsrecent-win-arm64.exe` — Windows on ARM

On first launch, Windows SmartScreen may warn about an unsigned binary —
click *More info → Run anyway*.

## Files

| File          | Purpose                                                       |
| ------------- | ------------------------------------------------------------- |
| `VsRecent.cs` | Form, filter, launch logic, JSON parsing                      |
| `Sqlite.cs`   | P/Invoke wrapper around Windows' built-in `winsqlite3.dll`    |
| `vsrecent.csproj` | SDK-style project (.NET 9, WinForms, single-file publish) |
| `vsrecent.ico`| App icon (multi-size, embedded in EXE)                        |
| `_make_icon.py` | Regenerates `vsrecent.ico` (requires Python + Pillow)       |
| `build.cmd`   | Local-build helper; wraps `dotnet publish` for the host arch  |
| `install_hotkey.ps1` | Creates a Start-Menu shortcut with a global hotkey     |
| `vsrecent_hotkey.ahk`| AutoHotkey v2 alternative for zero-delay / `Win+key` hotkeys |
| `.github/workflows/release.yml` | CI: builds win-x64 + win-arm64 and publishes a GitHub Release on `v*` tag |

## Build locally

Requires the [.NET 9 SDK](https://dotnet.microsoft.com/download).

Double-click `build.cmd`, or from a `cmd` prompt:

```
build.cmd
```

That detects your host architecture and runs the equivalent of:

```
dotnet publish vsrecent.csproj -c Release -r win-x64 --self-contained=true -o publish\win-x64
```

The single-file EXE lands at `publish\<rid>\vsrecent.exe`.

## Cut a release

Bump `<Version>` in `vsrecent.csproj` if you like (CI overrides it from the tag
anyway), then:

```
git tag v0.1.0
git push --tags
```

GitHub Actions builds both architectures and publishes a Release with both
EXEs attached. The workflow file lives at `.github/workflows/release.yml`.

## Run

Double-click `vsrecent.exe`. The launcher opens centered on the active
monitor with focus already in the filter box.

### Keys

| Key                  | Action                                            |
| -------------------- | ------------------------------------------------- |
| Type                 | Live AND-of-tokens substring filter (case-insensitive) over label + URI |
| `Up` / `Down`        | Move highlight up / down without leaving the filter |
| `PgUp` / `PgDn`      | Move highlight by 8 rows                          |
| `Ctrl+Home` / `End`  | Jump to first / last visible entry                |
| `Enter`              | Launch highlighted entry in VSCode, then close    |
| Double-click         | Same as Enter on that row                         |
| `Esc`                | Close without launching                           |

## Suggested fast-launch setup

For the snappiest experience, give yourself a way to invoke `vsrecent.exe`
without hunting for it.

### Option 1 — Built-in Windows hotkey (no install)

A Start-Menu shortcut with a hotkey is already created for you at
`%APPDATA%\Microsoft\Windows\Start Menu\Programs\VS Recent.lnk` bound to
**`Ctrl+Alt+R`**. Press it from anywhere on the desktop.

To change the hotkey, re-run:

```
powershell -ExecutionPolicy Bypass -File install_hotkey.ps1 -Hotkey "CTRL+SHIFT+R"
```

(Built-in shortcut hotkeys must start with `Ctrl+Alt`, `Ctrl+Shift`, or
`Shift+Alt`. Caveat: Windows adds a 200–500 ms delay before launching from
shortcut hotkeys; the second press onward is faster.)

### Option 2 — AutoHotkey (zero-delay, supports `Win+key`)

If you have [AutoHotkey v2](https://www.autohotkey.com/) installed, double-
click `vsrecent_hotkey.ahk` to bind **`Win+Shift+R`** (edit the script to
change). To run it at logon, put a shortcut to the `.ahk` in
`shell:startup`.

### Option 3 — Pin to taskbar

Right-click `vsrecent.exe` → *Pin to taskbar*. Then `Win+1`…`Win+9`
launches whichever taskbar slot it landed in.

## Notes

- The DB is opened **read-only**, so VSCode can keep using it concurrently.
  If a direct read ever fails (e.g. weird WAL state), the app falls back to
  snapshotting `state.vscdb` + `-wal` + `-shm` into `%TEMP%` and reading
  there.
- Only entries with a `folderUri` are shown (projects). Single-file recents
  (`fileUri`) are skipped by design.
- `Code.exe` is located at:
  1. `%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe` (per-user install — what you have)
  2. `%PROGRAMFILES%\Microsoft VS Code\Code.exe` (system install)
  3. `%PROGRAMFILES(X86)%\Microsoft VS Code\Code.exe`
  4. Otherwise falls back to `code` on `PATH`.
