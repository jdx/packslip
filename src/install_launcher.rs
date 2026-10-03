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
    fn waits_for_a_child_that_handles_console_interrupts() {
        let root = tempfile::tempdir().unwrap();
        let probe = root.path().join("probe.exe");
        let driver = root.path().join("driver.exe");
        let sources = [
            (
                &probe,
                r#"
                #[link(name="kernel32")] unsafe extern "system" {
                    fn SetConsoleCtrlHandler(h: Option<extern "system" fn(u32)->i32>, add:i32)->i32;
                }
                extern "system" fn handle(e:u32)->i32 {
                    if e <= 1 {
                        std::fs::write(std::env::var_os("EVENT").unwrap(), e.to_string()).unwrap();
                        1
                    } else { 0 }
                }
                fn main() {
                    // SSH/service parents can pass an inherited ignore-Ctrl+C
                    // attribute. This probe explicitly opts in to both events.
                    assert_ne!(unsafe {SetConsoleCtrlHandler(None,0)},0);
                    assert_ne!(unsafe {SetConsoleCtrlHandler(Some(handle),1)},0);
                    std::fs::write(std::env::var_os("READY").unwrap(), "ready").unwrap();
                    let release=std::path::PathBuf::from(std::env::var_os("RELEASE").unwrap());
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
                    while !release.exists() {
                        assert!(std::time::Instant::now()<deadline);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    std::process::exit(37);
                }
            "#,
            ),
            (
                &driver,
                r#"
                use std::os::windows::process::CommandExt;
                #[link(name="kernel32")] unsafe extern "system" {
                    fn FreeConsole()->i32;
                    fn AttachConsole(pid:u32)->i32;
                    fn GenerateConsoleCtrlEvent(event:u32,group:u32)->i32;
                    fn SetConsoleCtrlHandler(h:Option<extern "system" fn(u32)->i32>,add:i32)->i32;
                }
                extern "system" fn handle(e:u32)->i32 { i32::from(e<=1) }
                fn main() {
                    let mut args=std::env::args_os().skip(1);
                    let launcher=args.next().unwrap();
                    let root=std::path::PathBuf::from(args.next().unwrap());
                    let ready=root.join("ready"); let event=root.join("event"); let release=root.join("release");
                    let mut child=std::process::Command::new(launcher).creation_flags(0x10)
                        .env("READY",&ready).env("EVENT",&event).env("RELEASE",&release).spawn().unwrap();
                    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
                        while !ready.exists() {
                            assert!(child.try_wait().unwrap().is_none()); assert!(std::time::Instant::now()<deadline);
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        unsafe {FreeConsole();}
                        assert_ne!(unsafe {AttachConsole(child.id())},0);
                        assert_ne!(unsafe {SetConsoleCtrlHandler(Some(handle),1)},0);
                        for signal in [0,1] {
                            assert_ne!(unsafe {GenerateConsoleCtrlEvent(signal,0)},0);
                            while std::fs::read_to_string(&event).ok().as_deref()!=Some(&signal.to_string()) {
                                assert!(std::time::Instant::now()<deadline);
                                std::thread::sleep(std::time::Duration::from_millis(10));
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            assert!(child.try_wait().unwrap().is_none(),"launcher exited before its child");
                        }
                        std::fs::write(&release,"release").unwrap();
                        assert_eq!(child.wait().unwrap().code(),Some(37));
                    }));
                    let _=std::fs::write(&release,"release");
                    if result.is_err() {let _=child.kill(); std::process::exit(1);}
                }
            "#,
            ),
        ];
        for (output, source) in sources {
            let input = output.with_extension("rs");
            std::fs::write(&input, source).unwrap();
            let result = Command::new("rustc")
                .arg(input)
                .arg("-o")
                .arg(output)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let launcher = root.path().join("launcher.exe");
        write_export(&launcher, &probe).unwrap();
        assert!(
            Command::new(driver)
                .arg(launcher)
                .arg(root.path())
                .status()
                .unwrap()
                .success()
        );
    }
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
