# VS Recent repository instructions

## Build and run

- Use the .NET 9 SDK on Windows.
- `build.cmd` is the canonical local release build. It detects the host architecture, publishes a self-contained single-file executable, and writes it to `publish\<rid>\vsrecent.exe`.
- To target a RID explicitly, run:
  - `dotnet publish vsrecent.csproj -c Release -r win-x64 --self-contained=true -o publish\win-x64`
  - `dotnet publish vsrecent.csproj -c Release -r win-arm64 --self-contained=true -o publish\win-arm64`
- For a faster compile check, run `dotnet build vsrecent.csproj -c Release`.
- For a UI smoke test that does not depend on local VS Code history, run `dotnet run --project vsrecent.csproj -- --demo`.
- Releases are built by `.github\workflows\release.yml` for both Windows RIDs. A `v*` tag creates a GitHub Release, and the tag version overrides `<Version>` from the project file.
- WinGet uses the portable package ID `calebrob6.VSRecent`. Submission-ready manifests live under `winget\manifests`; each release needs a new version directory with both release URLs and SHA-256 hashes.

## Architecture

- `Program.Main` owns startup and the data-loading pipeline. It reads `history.recentlyOpenedPathsList` from `%USERPROFILE%\.vscode-shared\sharedStorage\state.vscdb`, parses only entries containing `folderUri`, classifies each URI, and precomputes its lowercase search index before constructing `MainForm`.
- `Sqlite` is a deliberately minimal P/Invoke wrapper around Windows' built-in `winsqlite3.dll`. The live VS Code database is opened read-only. If that read fails, `Program.ReadJson` snapshots the database together with any `-wal` and `-shm` files into a process-specific temp directory and retries there.
- `MainForm` is an imperative, code-only WinForms UI. It combines an owner-drawn recent-folder list, an AND-of-tokens text filter, and a normalized remote-kind dropdown. `_all` is the source collection and `_shown` is the filtered view. `ListBox.Items` contains the matching `Entry` objects so their `ToString()` values provide accessible item text while owner drawing controls the visuals.
- `RemoteClassifier` has two related views of a URI: `Classify` produces the detailed pill text/color, while `GetKind` produces the stable normalized key and display name used for grouping and searching.
- `Launcher` resolves per-user and system VS Code installs before falling back to `code` on `PATH`, then starts VS Code with `--folder-uri` and restores an existing matching hidden window.

## Repository-specific conventions

- Keep the application dependency-free and compatible with self-contained, single-file `net9.0-windows` publishing. Prefer Windows/.NET APIs already used by the project over adding managed or native packages.
- The project is MIT licensed under Caleb Robinson's copyright. Keep `LICENSE`, project metadata, and distribution manifests consistent.
- Preserve folder URIs end-to-end; do not convert remote URIs into filesystem paths. Only `DefaultLabel` decodes local `file:///` URIs for fallback display text.
- When adding a remote URI type, update both `RemoteClassifier.Classify` and `RemoteClassifier.GetKind`. Ensure `RemoteClassifier.Apply` runs before building `Entry.SearchKey`, which must continue to include the label, original URI, and remote-kind display name.
- Filtering is case-insensitive and uses AND semantics across whitespace-separated tokens. The dropdown filter composes with, rather than replaces, text filtering.
- Preserve the owner-drawn list invariants documented in `FlickerFreeListBox`: `ResizeRedraw` prevents stale right-anchored pills, while WinForms double buffering makes the pills disappear. Docking order in `MainForm` is also intentional. Scale custom-drawn dimensions and pixel-unit fonts from the form's `DeviceDpi` through `ScalePx` and `CreateDrawFont`; `DrawItemEventArgs.Graphics.DpiX` can report the system DPI instead of the monitor DPI on mixed-DPI desktops.
- UI changes are made directly in `MainForm`; there are no designer or `.resx` files. Continue using explicit `System.*` imports because implicit usings and nullable annotations are disabled.
- Native SQLite handles must always be finalized and closed. Keep database access read-only so VS Code can continue using the live database concurrently.
- Keep VS Code launch detached, but never set `ProcessStartInfo.WindowStyle` to `Hidden` or `CreateNoWindow`; either setting can leave newly created WSL windows invisible.
- Startup failures are surfaced through a top-level message box; launch failures stay attached to the form. Preserve these user-visible error paths rather than silently returning.
