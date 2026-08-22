# VS Recent repository instructions

## Build and run

- Use stable Rust with the MSVC toolchain on Windows.
- `build.cmd` is the canonical local release build. It runs `cargo build --release --locked` and copies the executable to `publish\vsrecent.exe`.
- For a compile and test check, run `cargo test`.
- For a UI smoke test that does not depend on local VS Code history, run `cargo run --release -- --demo`.
- `.github\workflows\release.yml` builds `x86_64-pc-windows-msvc` on `windows-latest` and `aarch64-pc-windows-msvc` on `windows-11-arm`.
- Pull requests build both architectures. A `v*` tag additionally creates a GitHub Release containing `vsrecent-win-x64.exe` and `vsrecent-win-arm64.exe`.
- WinGet uses the portable package ID `calebrob6.VSRecent`. Submission-ready manifests live under `winget\manifests`; each release needs a new version directory with both release URLs and SHA-256 hashes.

## Architecture

- `main.rs` creates and paints raw Win32 controls before starting any database work. A worker thread loads recent folders and posts the result back through `WM_ENTRIES_LOADED`; do not move database access back onto the UI thread.
- `sqlite.rs` is a minimal FFI wrapper around Windows' built-in `winsqlite3.dll`. The live VS Code database is opened read-only. If that read fails, it snapshots the database together with any `-wal` and `-shm` files into a process-specific temporary directory and retries there.
- The list box is owner-drawn with GDI. Rows use stable colors based on remote type, stronger matching selection colors, and DPI-scaled dimensions.
- `AppState.entries` is the source collection and `AppState.shown` contains indices for the filtered view.
- `remote_kind` produces both the displayed remote text and searchable remote classification.
- `open_folder` resolves per-user and system VS Code installs before falling back to `code` on `PATH`, launches with `--folder-uri`, and restores an existing matching hidden window.

## Repository-specific conventions

- Keep startup native and minimal. Avoid GUI frameworks, async runtimes, and dependencies that materially increase startup time or executable size.
- Keep the application compatible with self-contained x64 and ARM64 Windows builds. Prefer Win32 APIs already used by the project.
- The project is MIT licensed under Caleb Robinson's copyright. Keep `LICENSE`, package metadata, and distribution manifests consistent.
- Preserve folder URIs end-to-end; do not convert remote URIs into filesystem paths. Only `default_label` decodes local `file:///` URIs for fallback display text.
- When adding a remote URI type, update `remote_kind` and `row_colors`. The search index must continue to include the label, original URI, and remote display name.
- Filtering is case-insensitive and uses AND semantics across whitespace-separated tokens.
- Scale owner-drawn dimensions from `GetDpiForWindow`.
- Do not hold Rust references into `AppState` across Win32 calls that can dispatch messages, display modal UI, or destroy the window.
- Native SQLite statements and database handles must always be finalized and closed. Keep database access read-only so VS Code can continue using the live database concurrently.
- Keep VS Code launch detached and preserve the user-visible startup and launch error paths.
