//! Host preflight after deterministic artifact selection. Never execute an
//! artifact, install dependencies, or switch builds to make a requirement pass.
use crate::{Artifact, Host, Requires};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    pub os: String,
    pub arch: String,
    pub libc: Option<String>,
    pub os_version: Option<String>,
    pub glibc_version: Option<String>,
    /// Some(true/false) is an established result; None is uncheckable.
    pub libraries: BTreeMap<String, Option<bool>>,
    /// Missing command, or present with a known/unknown numeric version.
    pub commands: BTreeMap<String, Option<Option<String>>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub incompatible: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported bootstrap host {0}/{1}")]
    Host(String, String),
    #[error(
        "selected artifact is incompatible with this host: {0:?}; use --allow-incompatible-host to accept this artifact's requirements"
    )]
    Incompatible(Vec<String>),
}

fn numeric(value: &str) -> Option<Vec<u64>> {
    let values: Vec<_> = value
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (!values.is_empty()).then_some(values)
}

fn compare(actual: &str, minimum: &str) -> Option<std::cmp::Ordering> {
    let mut actual = numeric(actual)?;
    let mut minimum = numeric(minimum)?;
    let count = actual.len().max(minimum.len());
    actual.resize(count, 0);
    minimum.resize(count, 0);
    Some(actual.cmp(&minimum))
}

fn baseline(report: &mut Report, label: &str, actual: Option<&str>, minimum: Option<&str>) {
    let Some(minimum) = minimum else {
        return;
    };
    match actual.and_then(|a| compare(a, minimum)) {
        Some(std::cmp::Ordering::Less) => report
            .incompatible
            .push(format!("{label} {} is below {minimum}", actual.unwrap())),
        Some(_) => {}
        None => report
            .warnings
            .push(format!("cannot establish {label}>={minimum}")),
    }
}

impl Snapshot {
    pub fn host(&self) -> Host<'_> {
        Host {
            os: &self.os,
            arch: &self.arch,
            libc: self.libc.as_deref(),
        }
    }

    /// Inspect native host data and existing OS utilities. Utility paths are
    /// absolute and protected; no command is resolved through the caller's PATH.
    pub fn detect() -> Result<Self, Error> {
        let os = match std::env::consts::OS {
            "macos" => "darwin",
            other => other,
        };
        let arch = std::env::consts::ARCH;
        if !matches!(
            (os, arch),
            ("linux" | "windows", "x86_64" | "aarch64") | ("darwin", "aarch64")
        ) {
            return Err(Error::Host(os.into(), arch.into()));
        }
        let mut host = Self {
            os: os.into(),
            arch: arch.into(),
            ..Self::default()
        };
        #[cfg(target_os = "linux")]
        {
            host.os_version = std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .ok()
                .map(|v| kernel_version(v.trim()).to_owned());
            host.glibc_version = utility(Path::new("/usr/bin/getconf"), &["GNU_LIBC_VERSION"])
                .and_then(|v| v.trim().strip_prefix("glibc ").map(str::to_owned));
            let loader = match arch {
                "aarch64" => "/lib/ld-linux-aarch64.so.1",
                _ => "/lib64/ld-linux-x86-64.so.2",
            };
            let musl = format!("/lib/ld-musl-{arch}.so.1");
            if host.glibc_version.is_some() || Path::new(loader).is_file() {
                host.libc = Some("gnu".into());
            } else if Path::new(&musl).is_file() {
                host.libc = Some("musl".into());
            }
        }
        #[cfg(target_os = "macos")]
        {
            host.os_version =
                std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
                    .ok()
                    .and_then(|s| {
                        s.split("<key>ProductVersion</key>")
                            .nth(1)
                            .and_then(|s| s.split("<string>").nth(1))
                            .and_then(|s| s.split("</string>").next())
                            .map(str::to_owned)
                    });
        }
        #[cfg(windows)]
        {
            use winreg::{
                RegKey,
                enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
            };
            if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
                KEY_READ | KEY_WOW64_64KEY,
            ) {
                let major: Result<u32, _> = key.get_value("CurrentMajorVersionNumber");
                let minor: Result<u32, _> = key.get_value("CurrentMinorVersionNumber");
                let build: Result<String, _> = key.get_value("CurrentBuildNumber");
                if let (Ok(major), Ok(minor), Ok(build)) = (major, minor, build) {
                    host.os_version = Some(format!("{major}.{minor}.{build}"));
                }
            }
        }
        Ok(host)
    }

    /// Inspect only the chosen artifact's requirements. On platforms without a
    /// complete library-resolution view, absence is unknown rather than a guess.
    pub fn observe(&mut self, artifact: &Artifact) {
        let Some(requirements) = &artifact.requires else {
            return;
        };
        if let Some(libraries) = &requirements.libs {
            #[cfg(target_os = "linux")]
            let cache = (self.libc.as_deref() == Some("gnu") && !libraries.is_empty())
                .then(|| {
                    ["/sbin/ldconfig", "/usr/sbin/ldconfig"]
                        .iter()
                        .find_map(|p| utility(Path::new(p), &["-p"]))
                        .and_then(|s| loader_cache(&s, &self.arch))
                })
                .flatten();
            #[cfg(not(target_os = "linux"))]
            let cache: Option<BTreeSet<String>> = None;
            let paths = library_paths(&self.arch);
            for library in libraries {
                let found = paths
                    .iter()
                    .any(|p| library_matches(&p.join(library), &self.arch))
                    || cache.as_ref().is_some_and(|names| names.contains(library));
                let result = if found {
                    Some(true)
                } else if cache.is_some() {
                    Some(false)
                } else {
                    None
                };
                self.libraries.insert(library.clone(), result);
            }
        }
        for required in &requirements.bin {
            let found = std::env::var_os("PATH").is_some_and(|paths| {
                std::env::split_paths(&paths).any(|dir| command_exists(&dir, &required.name))
            });
            // There is no universal command-version interface. Do not execute an
            // arbitrary PATH program merely because a release names it.
            self.commands
                .insert(required.name.clone(), found.then_some(None));
        }
    }

    pub fn check(&self, requirements: Option<&Requires>) -> Report {
        let mut report = Report::default();
        let Some(requirements) = requirements else {
            report
                .warnings
                .push("publisher did not declare host requirements".into());
            return report;
        };
        baseline(
            &mut report,
            "OS",
            self.os_version.as_deref(),
            requirements.os_min.as_deref(),
        );
        baseline(
            &mut report,
            "glibc",
            self.glibc_version.as_deref(),
            requirements.glibc_min.as_deref(),
        );
        if let Some(libraries) = &requirements.libs {
            for library in libraries {
                match self.libraries.get(library).copied().flatten() {
                    Some(true) => {}
                    Some(false) => report
                        .incompatible
                        .push(format!("missing shared library {library}")),
                    None => report.warnings.push(format!(
                        "cannot establish availability of shared library {library}"
                    )),
                }
            }
        } else {
            report
                .warnings
                .push("publisher did not declare shared-library requirements".into());
        }
        for command in &requirements.bin {
            match self.commands.get(&command.name).and_then(Option::as_ref) {
                None => report
                    .warnings
                    .push(format!("missing command {}", command.name)),
                Some(version) => {
                    if let Some(minimum) = &command.min {
                        match version.as_deref().and_then(|v| compare(v, minimum)) {
                            Some(std::cmp::Ordering::Less) => report
                                .warnings
                                .push(format!("command {} is below {minimum}", command.name)),
                            Some(_) => {}
                            None => report
                                .warnings
                                .push(format!("cannot establish {}>={minimum}", command.name)),
                        }
                    }
                }
            }
        }
        report
    }
}

#[cfg(target_os = "linux")]
fn kernel_version(release: &str) -> &str {
    release.split(['-', '+']).next().unwrap_or_default()
}

fn library_matches(path: &Path, arch: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::io::Read as _;
        let mut header = [0u8; 20];
        let Ok(mut file) = std::fs::File::open(path) else {
            return false;
        };
        if file.read_exact(&mut header).is_err() {
            return false;
        }
        elf_library_matches(&header, arch)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = arch;
        path.is_file()
    }
}

#[cfg(target_os = "linux")]
fn elf_library_matches(header: &[u8; 20], arch: &str) -> bool {
    let machine = match arch {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return false,
    };
    // Supported Linux hosts use ELF64, little endian, and ET_DYN libraries.
    header[..6] == [0x7f, b'E', b'L', b'F', 2, 1]
        && u16::from_le_bytes([header[16], header[17]]) == 3
        && u16::from_le_bytes([header[18], header[19]]) == machine
}

impl Report {
    pub fn require_compatible(&self, allow_incompatible: bool) -> Result<(), Error> {
        if !allow_incompatible && !self.incompatible.is_empty() {
            return Err(Error::Incompatible(self.incompatible.clone()));
        }
        Ok(())
    }
}

fn library_paths(arch: &str) -> Vec<PathBuf> {
    #[cfg(unix)]
    {
        let triple = match arch {
            "aarch64" => "aarch64-linux-gnu",
            _ => "x86_64-linux-gnu",
        };
        let mut paths: Vec<PathBuf> =
            ["/lib", "/lib64", "/usr/lib", "/usr/lib64", "/usr/local/lib"]
                .into_iter()
                .map(PathBuf::from)
                .collect();
        paths.extend([
            PathBuf::from(format!("/lib/{triple}")),
            PathBuf::from(format!("/usr/lib/{triple}")),
        ]);
        if let Some(extra) = std::env::var_os("LD_LIBRARY_PATH") {
            paths.extend(std::env::split_paths(&extra));
        }
        paths
    }
    #[cfg(windows)]
    {
        let _ = arch;
        let mut paths: Vec<_> =
            known_folders::get_known_folder_path(known_folders::KnownFolder::System)
                .into_iter()
                .collect();
        if let Some(extra) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&extra));
        }
        paths
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = arch;
        Vec::new()
    }
}

fn command_exists(dir: &Path, name: &str) -> bool {
    #[cfg(windows)]
    return ["exe", "com", "cmd", "bat"]
        .iter()
        .any(|ext| dir.join(format!("{name}.{ext}")).is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && m.mode() & 0o111 != 0)
    }
    #[cfg(not(any(unix, windows)))]
    dir.join(name).is_file()
}

#[cfg(target_os = "linux")]
fn utility(path: &Path, arguments: &[&str]) -> Option<String> {
    use std::io::Read as _;
    use std::os::unix::fs::MetadataExt as _;
    // Only execute protected OS files, including their resolved parent chain.
    let path = std::fs::canonicalize(path).ok()?;
    for component in path.ancestors() {
        let meta = std::fs::metadata(component).ok()?;
        if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
            return None;
        }
    }
    let mut output = tempfile::tempfile().ok()?;
    let mut child = std::process::Command::new(path)
        .args(arguments)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone().ok()?)
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None)
                if std::time::Instant::now() > deadline
                    || !output.metadata().is_ok_and(|m| m.len() <= 4 * 1024 * 1024) =>
            {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    use std::io::{Seek as _, SeekFrom};
    output.seek(SeekFrom::Start(0)).ok()?;
    let mut bytes = Vec::new();
    output
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(target_os = "linux")]
fn loader_cache(output: &str, arch: &str) -> Option<BTreeSet<String>> {
    let mut lines = output.lines();
    let header = lines.next()?;
    let (count, tail) = header.trim().split_once(' ')?;
    let count: usize = count.parse().ok()?;
    if !tail.starts_with("libs found in cache ") {
        return None;
    }
    let mut names = BTreeSet::new();
    let mut entries = 0;
    for line in lines {
        if let Some((left, path)) = line.split_once(" => ") {
            let name = left.split_whitespace().next()?;
            if library_matches(Path::new(path.trim()), arch) {
                names.insert(name.to_owned());
            }
            entries += 1;
        }
    }
    (entries == count).then_some(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_kernel_local_suffixes_preserve_the_numeric_baseline() {
        assert_eq!(
            compare(kernel_version("6.1.158+"), "6.2"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(kernel_version("6.1.0-18-amd64"), "6.1.0");
        assert_eq!(compare(kernel_version("unknown+"), "6.2"), None);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn loader_cache_requires_native_elf_libraries() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("libfixture.so.1");
        let mut header = [0u8; 20];
        header[..6].copy_from_slice(&[0x7f, b'E', b'L', b'F', 1, 1]);
        header[16] = 3;
        header[18] = 3; // EM_386
        std::fs::write(&file, header).unwrap();
        let output = format!(
            "1 libs found in cache `/etc/ld.so.cache'\n libfixture.so.1 (libc6) => {}\n",
            file.display()
        );
        assert!(loader_cache(&output, "x86_64").unwrap().is_empty());
        header[4] = 2;
        header[18] = 62;
        std::fs::write(&file, header).unwrap();
        assert!(
            loader_cache(&output, "x86_64")
                .unwrap()
                .contains("libfixture.so.1")
        );
        assert!(loader_cache(&output, "aarch64").unwrap().is_empty());
        header[18] = 183;
        std::fs::write(&file, header).unwrap();
        assert!(
            loader_cache(&output, "aarch64")
                .unwrap()
                .contains("libfixture.so.1")
        );
        assert!(loader_cache("unrecognized locale-dependent header", "aarch64").is_none());
    }
    #[test]
    fn numeric_baselines_commands_and_unknown_results_have_distinct_effects() {
        let host = Snapshot {
            os_version: Some("10.0.17763".into()),
            glibc_version: Some("2.30".into()),
            libraries: BTreeMap::from([
                ("available".into(), Some(true)),
                ("absent".into(), Some(false)),
            ]),
            commands: BTreeMap::from([
                ("old".into(), Some(Some("16".into()))),
                ("unknown".into(), Some(None)),
            ]),
            ..Snapshot::default()
        };
        let requirements = Requires {
            os_min: Some("10.0.19041".into()),
            glibc_min: Some("2.31".into()),
            libs: Some(vec!["available".into(), "absent".into(), "unknown".into()]),
            bin: ["old", "unknown", "missing"]
                .into_iter()
                .map(|name| crate::model::RequiredBin {
                    name: name.into(),
                    min: Some("17".into()),
                })
                .collect(),
        };
        let report = host.check(Some(&requirements));
        assert_eq!(report.incompatible.len(), 3);
        assert_eq!(report.warnings.len(), 4);
        assert!(report.require_compatible(false).is_err());
        assert!(report.require_compatible(true).is_ok());
        assert_eq!(compare("17", "17.0.0"), Some(std::cmp::Ordering::Equal));
        assert_eq!(compare("unrecognized", "17"), None);
    }
    #[test]
    fn native_detection_uses_the_format_vocabulary() {
        let host = Snapshot::detect().unwrap();
        assert!(matches!(host.os.as_str(), "linux" | "darwin" | "windows"));
        assert!(matches!(host.arch.as_str(), "x86_64" | "aarch64"));
        #[cfg(any(windows, target_os = "macos"))]
        assert!(
            host.os_version.as_deref().and_then(numeric).is_some(),
            "{host:?}"
        );
    }
}
