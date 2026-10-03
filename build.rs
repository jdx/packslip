use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/launcher_windows.rs");
    println!("cargo:rerun-if-changed=src/launcher_payload.rs");
    if std::env::var_os("CARGO_FEATURE_INSTALL_LAUNCHER").is_none() {
        return;
    }
    let target = std::env::var("TARGET").expect("Cargo supplies TARGET");
    if !target.contains("windows") {
        return;
    }
    let source = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("src/launcher_windows.rs");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("packslip-launcher.exe");
    let status = Command::new(std::env::var_os("RUSTC").expect("Cargo supplies RUSTC"))
        .arg(source)
        .args([
            "--edition=2024",
            "--crate-name=packslip_launcher",
            "--target",
        ])
        .arg(target)
        .args([
            "-Copt-level=z",
            "-Clto=thin",
            "-Cpanic=abort",
            "-Cstrip=symbols",
            "-Dwarnings",
        ])
        .arg("-Ctarget-feature=+crt-static")
        .arg("-o")
        .arg(output)
        .status()
        .expect("compile the native Windows launcher");
    assert!(
        status.success(),
        "native Windows launcher compilation failed"
    );
}
