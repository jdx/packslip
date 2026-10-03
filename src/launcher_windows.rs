//! Standalone native command launcher, built for the same target as packslip.
#![deny(unsafe_code)]
mod launcher_payload;
use std::ffi::OsString;
use std::fs::File;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;
use std::process::Command;

// The standard library has no console-control API. Keep the single native
// call isolated from target parsing and process creation; the library and
// installer continue to forbid unsafe code.
#[allow(unsafe_code)]
mod console {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }

    extern "system" fn handle(event: u32) -> i32 {
        // Consume only Ctrl+C and Ctrl+Break. The child receives the same
        // console broadcast independently and decides how to handle it.
        i32::from(event == 0 || event == 1)
    }

    pub fn install() -> std::io::Result<()> {
        // SAFETY: the callback has the exact Win32 ABI, lives for the process
        // lifetime, and performs no allocation, locking, or pointer access.
        if unsafe { SetConsoleCtrlHandler(Some(handle), 1) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

fn run() -> std::io::Result<i32> {
    console::install()?;
    let mut executable = File::open(std::env::current_exe()?)?;
    let target = PathBuf::from(OsString::from_wide(&launcher_payload::read(
        &mut executable,
    )?));
    if !target.is_absolute() {
        return Err(std::io::Error::other("launcher target must be absolute"));
    }
    let status = Command::new(target)
        .args(std::env::args_os().skip(1))
        .status()?;
    Ok(status.code().unwrap_or(1))
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("packslip launcher: {error}");
            std::process::exit(126);
        }
    }
}
