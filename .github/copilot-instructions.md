# VS Recent repository instructions

## Build and run

- Use stable Rust with the MSVC toolchain on Windows, or with the Xcode command line tools on macOS.
- `build.cmd` is the canonical local Windows release build. It runs `cargo build --release --locked` and copies the executable to `publish\vsrecent.exe`.
- `build-macos.sh` is the canonical local macOS release build. It builds `aarch64-apple-darwin` and `x86_64-apple-darwin`, joins them into the universal `publish/VS Recent.app`, signs it ad hoc, and zips it as `publish/vsrecent-macos-universal.zip`.
- For a compile and test check, run `cargo test`.
- From Windows, `cargo check --target aarch64-apple-darwin` type-checks the macOS front end once the target is installed with `rustup target add`. It cannot link, so only a Mac or CI can produce and launch a macOS build.
- For a UI smoke test that does not depend on local VS Code history, run `cargo run --release -- --demo`.
- For every pull request, build optimized Windows binaries from both `main` and the PR branch with the same architecture, toolchain, and machine. Run `scripts\benchmark-startup.ps1` against those binaries and report the median, p95, and change in the PR.
- `.github\workflows\release.yml` builds `x86_64-pc-windows-msvc` on `windows-latest`, `aarch64-pc-windows-msvc` on `windows-11-arm`, and the universal macOS application on `macos-latest`.
- The macOS job also runs `cargo test`, launches the debug build and the application bundle with `--demo` through `scripts/smoke-test-macos.sh`, and uploads screenshots as the `macos-screenshots` artifact. Check those screenshots when a change touches `macos.rs`.
- Pull requests build every artifact. A `v*` tag additionally creates a GitHub Release containing `vsrecent-win-x64.exe`, `vsrecent-win-arm64.exe`, and `vsrecent-macos-universal.zip`.
- WinGet uses the portable package ID `calebrob6.VSRecent`. Submission-ready manifests live under `winget\manifests`; each release needs a new version directory with both release URLs and SHA-256 hashes.

## Architecture

- `main.rs` only selects the platform front end: `windows.rs` (Win32) or `macos.rs` (AppKit through the `objc2` crates).
- `history.rs` and `theme.rs` are shared by both front ends. `history.rs` loads and parses the recent-folder history, classifies remotes, and filters entries; `theme.rs` holds the light and dark palettes and the row colors. Behavior that both platforms need belongs there, not in a front end.
- Both front ends create and show their native controls before starting any database work. A worker thread loads recent folders and hands the result back to the UI thread, through `WM_ENTRIES_LOADED` on Windows and the main dispatch queue on macOS; do not move database access back onto the UI thread.
- `sqlite.rs` is a minimal FFI wrapper around the SQLite library that ships with the system: `winsqlite3.dll` on Windows and `libsqlite3.dylib` on macOS. The live VS Code database is opened read-only. If that read fails, it snapshots the database together with any `-wal` and `-shm` files into a process-specific temporary directory and retries there. Only that private snapshot is opened read-write: the SQLite that ships with macOS cannot open a WAL database read-only once VS Code has exited and removed the `-wal` and `-shm` files, so on macOS the snapshot is the normal path whenever VS Code is closed.
- VS Code keeps the history at `.vscode-shared/sharedStorage/state.vscdb` under the home directory on every platform.
- On Windows the list box is owner-drawn with GDI. On macOS each row is an `NSTableRowView` subclass that fills its own background. Rows use stable colors based on remote type and stronger matching selection colors.
- The front end's `entries` is the source collection and `shown` contains indices for the filtered view.
- `remote_kind` produces both the displayed remote text and searchable remote classification.
- On Windows, `open_folder` resolves per-user and system VS Code installs before falling back to `code` on `PATH`, launches with `--folder-uri`, and restores an existing matching hidden window.
- On macOS, `open_folder` finds VS Code through Launch Services and runs `open -n -g -a <app> --args --folder-uri <uri>`, which is what VS Code's own `code` command does. Do not launch through `code`: it marks VS Code as started from a terminal, so a cold start would skip resolving the user's shell environment.

## Repository-specific conventions

- Keep startup native and minimal. Avoid GUI frameworks, async runtimes, and dependencies that materially increase startup time or executable size.
- Keep the application compatible with self-contained x64 and ARM64 Windows builds and with universal macOS builds. Prefer the Win32 and AppKit APIs already used by the project, and enable only the `objc2` framework features that are used.
- The project is MIT licensed under Caleb Robinson's copyright. Keep `LICENSE`, package metadata, and distribution manifests consistent.
- Preserve folder URIs end-to-end; do not convert remote URIs into filesystem paths. Only `default_label` decodes local `file:///` URIs for fallback display text.
- When adding a remote URI type, update `remote_kind` and `row_colors`. The search index must continue to include the label, original URI, and remote display name.
- Filtering is case-insensitive and uses AND semantics across whitespace-separated tokens.
- Scale owner-drawn Windows dimensions from `GetDpiForWindow`. AppKit lays out in points, so the macOS front end needs no scaling.
- Do not hold Rust references into `AppState` across Win32 calls that can dispatch messages, display modal UI, or destroy the window. On macOS, release the `RefCell` borrow of `AppState` before AppKit calls that call back into the data source or delegate, such as `reloadData`.
- On macOS, handle picker keys in `control:textView:doCommandBySelector:` rather than by intercepting key events, so input methods see every key first.
- Native SQLite statements and database handles must always be finalized and closed. Keep access to VS Code's own database read-only so VS Code can continue using the live database concurrently.
- Keep VS Code launch detached and preserve the user-visible startup and launch error paths.
