//! Export declared commands without exposing the artifact's runtime directory.
//! Windows uses a small native executable with an embedded absolute UTF-16
//! target. Each launcher is a single transaction entry: no sidecar, symlink
//! privilege, shell, or Developer Mode is required.
use crate::install_fs::Error;
use std::path::Path;

pub fn write_export(path: &Path, target: &Path) -> Result<(), Error> {
    if !target.is_absolute() {
        return Err(Error::Conflict("launcher target must be absolute".into()));
    }
    #[cfg(unix)]
    return crate::install_fs::symlink_export(path, target);
    #[cfg(windows)]
    {
        use std::io::Write as _;
        use std::os::windows::ffi::OsStrExt as _;
        let mut file = std::fs::File::create_new(path)?;
        file.write_all(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/packslip-launcher.exe"
        )))?;
        crate::launcher_payload::write(
            &mut file,
            &target.as_os_str().encode_wide().collect::<Vec<_>>(),
        )?;
        file.sync_all()?;
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    Err(Error::Conflict("unsupported installation platform".into()))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Command;
    #[test]
    fn forwards_arguments_environment_cwd_and_exit_status() {
        let root = tempfile::tempdir().unwrap();
        let tree = root.path().join("installation with spaces");
        std::fs::create_dir(&tree).unwrap();
        let target = tree.join("probe.exe");
        let source = tree.join("probe.rs");
        std::fs::write(&source, r#"
            use std::io::Write;
            use std::os::windows::ffi::OsStrExt;
            fn main() {
                let mut out = std::fs::File::create(std::env::var_os("PACKSLIP_LAUNCHER_RESULT").unwrap()).unwrap();
                for arg in std::env::args_os().skip(1).chain([
                    std::env::var_os("PACKSLIP_LAUNCHER_ENV").unwrap(),
                    std::env::current_dir().unwrap().into_os_string(),
                ]) {
                    let units: Vec<_> = arg.encode_wide().collect();
                    out.write_all(&(units.len() as u32).to_le_bytes()).unwrap();
                    for unit in units { out.write_all(&unit.to_le_bytes()).unwrap(); }
                }
                std::process::exit(37);
            }
        "#).unwrap();
        let output = Command::new("rustc")
            .arg(&source)
            .args(["--crate-name=launcher_probe", "-o"])
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let launcher = root.path().join("export.exe");
        write_export(&launcher, &target).unwrap();
        let arguments = [
            "",
            "two words",
            "quote\"inside",
            "trailing\\",
            "☃ unicode",
            "&|<>^%!",
        ];
        let direct_result = root.path().join("direct.bin");
        let shim_result = root.path().join("shim.bin");
        for (exe, result) in [(&target, &direct_result), (&launcher, &shim_result)] {
            let status = Command::new(exe)
                .args(arguments)
                .current_dir(&tree)
                .env("PACKSLIP_LAUNCHER_RESULT", result)
                .env("PACKSLIP_LAUNCHER_ENV", "unchanged ☃")
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(37));
        }
        assert_eq!(
            std::fs::read(direct_result).unwrap(),
            std::fs::read(shim_result).unwrap()
        );
        // Changing the executable inside the tree changes future invocations:
        // the export retains its target rather than copying the executable.
        std::fs::remove_file(&target).unwrap();
        assert_eq!(Command::new(launcher).status().unwrap().code(), Some(126));
    }
}
