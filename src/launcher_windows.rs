//! Standalone native command launcher, built for the same target as packslip.
#![forbid(unsafe_code)]
mod launcher_payload;
use std::ffi::OsString;
use std::fs::File;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;
use std::process::Command;

fn run() -> std::io::Result<i32> {
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
