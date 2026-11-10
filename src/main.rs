#![windows_subsystem = "windows"]

mod history;
mod sqlite;
mod theme;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("VS Recent supports Windows and macOS only");

fn main() {
    #[cfg(windows)]
    windows::main();
    #[cfg(target_os = "macos")]
    macos::main();
}
