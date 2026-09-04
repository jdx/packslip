//! What an artifact's executables load from the host: the shared
//! libraries named in an ELF's `DT_NEEDED` entries, a Mach-O's load
//! commands, or a PE's import table, read out of the artifact so that
//! `requires.libs` is a fact about the bytes rather than a claim.
//! Archives are decoded by [`crate::archive`].

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use crate::archive;
use crate::model::Bin;

/// The executables read out of one artifact.
#[derive(Debug, Default)]
pub struct Executables {
    /// Each executable's bytes, by its `bin` path.
    pub found: Vec<(String, Vec<u8>)>,
    /// The base name of every file in the artifact, so a library the
    /// artifact ships beside its executables is not asked of the host.
    pub shipped: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Archive(#[from] archive::Error),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{artifact} has no {path:?}; an executable's path counts from the archive root")]
    BinNotFound { artifact: String, path: String },
}

/// How many symbolic links to follow inside an archive before giving up.
const LINK_DEPTH: usize = 8;

/// Read the executables named by `bins` out of the artifact at `path`.
/// `None` when the format is one this does not open (an installer, a
/// disk image, a 7z) or the file is not a readable instance of its
/// format. `Err` when the archive opens but lacks a listed executable.
pub fn read_executables(
    path: &Path,
    format: Option<&str>,
    bins: &[Bin],
) -> Result<Option<Executables>, Error> {
    if bins.is_empty() {
        return Ok(None);
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let io = |source| Error::Io {
        path: path.display().to_string(),
        source,
    };
    let wanted: Vec<String> = bins.iter().map(|b| archive::normalize(&b.path)).collect();
    let executables = match format {
        Some(f) if crate::model::is_bare_format(f) => match read_bare(path, f, &wanted) {
            Ok(executables) => Some(executables),
            Err(archive::Error::Undecodable { .. }) => None,
            Err(err) => return Err(err.into()),
        },
        Some("zip") => read_zip(path, &wanted).map_err(io)?,
        Some(f) if archive::can_list(f) => read_tar(path, f, &wanted)?,
        _ => None,
    };
    let Some(executables) = executables else {
        return Ok(None);
    };
    if let Some(missing) = wanted
        .iter()
        .find(|w| !executables.found.iter().any(|(p, _)| p == *w))
    {
        return Err(Error::BinNotFound {
            artifact: name,
            path: missing.clone(),
        });
    }
    Ok(Some(executables))
}

/// The shared libraries `executables` need from the host, by loader
/// name, across every executable that is an ELF, Mach-O, or PE binary.
/// `None` when none of them is one: a script, say.
pub fn host_libraries(executables: &Executables) -> Option<Vec<String>> {
    let mut libs = BTreeSet::new();
    let mut parsed = false;
    for (_, bytes) in &executables.found {
        let Some(needed) = needed_libraries(bytes) else {
            continue;
        };
        parsed = true;
        libs.extend(needed);
    }
    if !parsed {
        return None;
    }
    let shipped: BTreeSet<String> = executables
        .shipped
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    Some(
        libs.into_iter()
            .filter(|lib| !shipped.contains(&lib.to_ascii_lowercase()))
            .collect(),
    )
}

/// The libraries one binary asks the host for, after the baseline for its
/// platform is removed. `None` when the bytes are not a binary.
pub fn needed_libraries(bytes: &[u8]) -> Option<Vec<String>> {
    use goblin::mach::{Mach, SingleArch};
    match goblin::Object::parse(bytes).ok()? {
        goblin::Object::Elf(elf) => Some(elf_host_libraries(&elf.libraries)),
        goblin::Object::PE(pe) => Some(pe_host_libraries(&pe.libraries)),
        goblin::Object::Mach(Mach::Binary(macho)) => Some(macho_host_libraries(&macho.libs)),
        goblin::Object::Mach(Mach::Fat(fat)) => {
            let mut libs = BTreeSet::new();
            let mut any = false;
            for arch in &fat {
                if let Ok(SingleArch::MachO(macho)) = arch {
                    any = true;
                    libs.extend(macho_host_libraries(&macho.libs));
                }
            }
            any.then(|| libs.into_iter().collect())
        }
        _ => None,
    }
}

/// ELF: everything in `DT_NEEDED` except the C runtime and its loader,
/// which `libc` and `glibc_min` already describe.
pub fn elf_host_libraries(needed: &[&str]) -> Vec<String> {
    const BASELINE: &[&str] = &[
        "libc",
        "libm",
        "libdl",
        "libpthread",
        "librt",
        "libutil",
        "libresolv",
        "libthr",
        "libgcc_s",
    ];
    let mut out = BTreeSet::new();
    for lib in needed {
        let stem = lib.split(".so").next().unwrap_or(lib);
        let baseline =
            lib.starts_with("ld-") || lib.starts_with("libc.musl") || BASELINE.contains(&stem);
        if !baseline {
            out.insert((*lib).to_string());
        }
    }
    out.into_iter().collect()
}

/// Mach-O: libraries outside the system's own, by file name. Anything
/// under `/usr/lib` or `/System/Library` ships with macOS, and a path
/// through `@rpath`, `@executable_path`, or `@loader_path` is found
/// relative to the artifact, not on the host.
pub fn macho_host_libraries(libs: &[&str]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for lib in libs {
        if *lib == "self"
            || lib.starts_with("/usr/lib/")
            || lib.starts_with("/System/Library/")
            || lib.starts_with('@')
        {
            continue;
        }
        let name = lib.rsplit('/').next().unwrap_or(lib);
        if !name.is_empty() {
            out.insert(name.to_string());
        }
    }
    out.into_iter().collect()
}

/// PE: imported DLLs that are not part of Windows itself, lowercased as
/// the loader treats them. The Visual C++ and MinGW runtimes stay: they
/// are the ones a fresh machine lacks.
pub fn pe_host_libraries(libraries: &[&str]) -> Vec<String> {
    const BASELINE: &[&str] = &[
        "advapi32.dll",
        "avrt.dll",
        "bcrypt.dll",
        "bcryptprimitives.dll",
        "cabinet.dll",
        "cfgmgr32.dll",
        "comctl32.dll",
        "comdlg32.dll",
        "credui.dll",
        "crypt32.dll",
        "cryptui.dll",
        "d2d1.dll",
        "d3d11.dll",
        "d3d12.dll",
        "d3d9.dll",
        "d3dcompiler_47.dll",
        "dbghelp.dll",
        "dnsapi.dll",
        "dwmapi.dll",
        "dwrite.dll",
        "dxgi.dll",
        "dxva2.dll",
        "fwpuclnt.dll",
        "gdi32.dll",
        "gdiplus.dll",
        "hid.dll",
        "imm32.dll",
        "iphlpapi.dll",
        "kernel32.dll",
        "kernelbase.dll",
        "mpr.dll",
        "mscoree.dll",
        "msimg32.dll",
        "msvcrt.dll",
        "mswsock.dll",
        "ncrypt.dll",
        "netapi32.dll",
        "normaliz.dll",
        "ntdll.dll",
        "ole32.dll",
        "oleacc.dll",
        "oleaut32.dll",
        "opengl32.dll",
        "pdh.dll",
        "powrprof.dll",
        "propsys.dll",
        "psapi.dll",
        "rpcrt4.dll",
        "rstrtmgr.dll",
        "secur32.dll",
        "setupapi.dll",
        "shcore.dll",
        "shell32.dll",
        "shlwapi.dll",
        "synchronization.dll",
        "ucrtbase.dll",
        "user32.dll",
        "userenv.dll",
        "usp10.dll",
        "uxtheme.dll",
        "version.dll",
        "winhttp.dll",
        "wininet.dll",
        "winmm.dll",
        "winspool.drv",
        "wintrust.dll",
        "wldap32.dll",
        "ws2_32.dll",
        "wtsapi32.dll",
    ];
    let mut out = BTreeSet::new();
    for lib in libraries {
        let name = lib.to_ascii_lowercase();
        if name.starts_with("api-ms-win-")
            || name.starts_with("ext-ms-")
            || BASELINE.contains(&name.as_str())
        {
            continue;
        }
        out.insert(name);
    }
    out.into_iter().collect()
}

/// A bare executable, possibly compressed: the file is the one
/// executable, whatever its `bin` entry calls it.
fn read_bare(path: &Path, format: &str, wanted: &[String]) -> Result<Executables, archive::Error> {
    let mut bytes = Vec::new();
    archive::decoder(path, format)?
        .read_to_end(&mut bytes)
        .map_err(|e| archive::Error::Undecodable {
            path: path.display().to_string(),
            format: format.to_string(),
            why: e.to_string(),
        })?;
    let mut executables = Executables::default();
    for w in wanted {
        executables.found.push((w.clone(), bytes.clone()));
    }
    Ok(executables)
}

/// Where a symbolic link inside an archive points, as an archive path:
/// relative to the link's own directory unless absolute, with `.` and
/// `..` resolved.
fn resolve_link(link: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        link.rsplit_once('/')
            .map(|(dir, _)| dir.split('/').collect())
            .unwrap_or_default()
    };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// One archive member as the readers see it.
struct Member {
    path: String,
    /// The link target, for a symbolic link.
    link: Option<String>,
    bytes: Option<Vec<u8>>,
}

/// Assemble the executables from members, following links by reading the
/// archive again for their targets. `open` yields every member, reading
/// the bytes of those whose path is in the given set.
fn assemble<F>(wanted: &[String], mut open: F) -> std::io::Result<Option<Executables>>
where
    F: FnMut(&BTreeSet<String>) -> std::io::Result<Option<Vec<Member>>>,
{
    let mut executables = Executables::default();
    let mut targets: BTreeSet<String> = wanted.iter().cloned().collect();
    let mut resolved: Vec<(String, String)> =
        wanted.iter().map(|w| (w.clone(), w.clone())).collect();
    for _ in 0..=LINK_DEPTH {
        let Some(members) = open(&targets)? else {
            return Ok(None);
        };
        executables.shipped.extend(members.iter().filter_map(|m| {
            m.path
                .rsplit('/')
                .next()
                .filter(|n| !n.is_empty())
                .map(str::to_string)
        }));
        let mut next = BTreeSet::new();
        let mut found = Vec::new();
        for (bin, current) in &mut resolved {
            if executables.found.iter().any(|(p, _)| p == bin) {
                continue;
            }
            let Some(member) = members.iter().find(|m| m.path == *current) else {
                continue;
            };
            if let Some(target) = &member.link {
                *current = resolve_link(current, target);
                next.insert(current.clone());
            } else if let Some(bytes) = &member.bytes {
                found.push((bin.clone(), bytes.clone()));
            }
        }
        executables.found.extend(found);
        if next.is_empty() {
            break;
        }
        targets = next;
    }
    Ok(Some(executables))
}

fn read_tar(path: &Path, format: &str, wanted: &[String]) -> Result<Option<Executables>, Error> {
    let mut failed = None;
    let result = assemble(wanted, |targets| {
        let reader = match archive::decoder(path, format) {
            Ok(reader) => reader,
            Err(archive::Error::Undecodable { .. }) => return Ok(None),
            Err(err) => {
                failed = Some(err);
                return Ok(None);
            }
        };
        let mut archive = tar::Archive::new(reader);
        let Ok(entries) = archive.entries() else {
            return Ok(None);
        };
        let mut members = Vec::new();
        for entry in entries {
            let Ok(mut entry) = entry else {
                return Ok(None);
            };
            let Ok(entry_path) = entry.path() else {
                continue;
            };
            let member_path = archive::normalize(&entry_path.to_string_lossy());
            let kind = entry.header().entry_type();
            // A symbolic link's target is relative to the link; a hard
            // link's is another member's path from the archive root, so it
            // is marked absolute for `resolve_link`.
            let link = if kind.is_symlink() {
                entry
                    .link_name()
                    .ok()
                    .flatten()
                    .map(|t| t.to_string_lossy().into_owned())
            } else if kind.is_hard_link() {
                entry
                    .link_name()
                    .ok()
                    .flatten()
                    .map(|t| format!("/{}", archive::normalize(&t.to_string_lossy())))
            } else {
                None
            };
            let bytes = if link.is_none() && kind.is_file() && targets.contains(&member_path) {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                Some(bytes)
            } else {
                None
            };
            members.push(Member {
                path: member_path,
                link,
                bytes,
            });
        }
        Ok(Some(members))
    });
    if let Some(err) = failed {
        return Err(err.into());
    }
    result.map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

fn read_zip(path: &Path, wanted: &[String]) -> std::io::Result<Option<Executables>> {
    assemble(wanted, |targets| {
        let file = std::fs::File::open(path)?;
        let Ok(mut archive) = zip::ZipArchive::new(file) else {
            return Ok(None);
        };
        let mut members = Vec::new();
        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else {
                return Ok(None);
            };
            if entry.is_dir() {
                continue;
            }
            let member_path = archive::normalize(entry.name());
            let mut bytes = None;
            let mut link = None;
            if entry.is_symlink() {
                let mut target = String::new();
                entry.read_to_string(&mut target)?;
                link = Some(target);
            } else if targets.contains(&member_path) {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                bytes = Some(data);
            }
            members.push(Member {
                path: member_path,
                link,
                bytes,
            });
        }
        Ok(Some(members))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elf_baseline_is_dropped() {
        assert_eq!(
            elf_host_libraries(&[
                "libssl.so.3",
                "libc.so.6",
                "libm.so.6",
                "libgcc_s.so.1",
                "ld-linux-x86-64.so.2",
                "libz.so.1",
                "libstdc++.so.6",
                "libcrypt.so.1",
            ]),
            [
                "libcrypt.so.1",
                "libssl.so.3",
                "libstdc++.so.6",
                "libz.so.1"
            ]
        );
        assert!(elf_host_libraries(&["libc.so", "libdl.so", "ld-musl-x86_64.so.1"]).is_empty());
        assert_eq!(
            elf_host_libraries(&["libc.so.7", "libthr.so.3", "libgmp.so.10"]),
            ["libgmp.so.10"]
        );
    }

    #[test]
    fn macho_system_and_rpath_are_dropped() {
        assert_eq!(
            macho_host_libraries(&[
                "self",
                "/usr/lib/libSystem.B.dylib",
                "/usr/lib/libc++.1.dylib",
                "/System/Library/Frameworks/Security.framework/Versions/A/Security",
                "@rpath/libonnxruntime.dylib",
                "@executable_path/../lib/libfoo.dylib",
                "/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib",
            ]),
            ["libssl.3.dylib"]
        );
    }

    #[test]
    fn pe_windows_dlls_are_dropped_and_runtimes_kept() {
        assert_eq!(
            pe_host_libraries(&[
                "KERNEL32.dll",
                "api-ms-win-crt-runtime-l1-1-0.dll",
                "VCRUNTIME140.dll",
                "msvcp140.dll",
                "libssl-3-x64.dll",
                "ws2_32.dll",
            ]),
            ["libssl-3-x64.dll", "msvcp140.dll", "vcruntime140.dll"]
        );
    }

    #[test]
    fn links_resolve_relative_to_their_directory() {
        assert_eq!(
            resolve_link("tool/bin/tool", "../lib/tool-real"),
            "tool/lib/tool-real"
        );
        assert_eq!(resolve_link("tool", "./real"), "real");
        assert_eq!(resolve_link("a/b/c", "/x/y"), "x/y");
        assert_eq!(archive::normalize("./tool/bin/tool"), "tool/bin/tool");
    }

    #[test]
    fn scripts_are_not_binaries() {
        assert!(needed_libraries(b"#!/bin/sh\nexec java -jar x.jar\n").is_none());
        let fixture = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/needs-z"
        ))
        .unwrap();
        assert_eq!(needed_libraries(&fixture).unwrap(), ["libz.so.1"]);
    }

    #[test]
    fn shipped_libraries_are_not_required() {
        let fixture = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/needs-z"
        ))
        .unwrap();
        let mut executables = Executables {
            found: vec![("tool".into(), fixture)],
            shipped: BTreeSet::new(),
        };
        assert_eq!(host_libraries(&executables).unwrap(), ["libz.so.1"]);
        executables.shipped.insert("libz.so.1".into());
        assert!(host_libraries(&executables).unwrap().is_empty());
        let script = Executables {
            found: vec![("tool".into(), b"#!/bin/sh\n".to_vec())],
            shipped: BTreeSet::new(),
        };
        assert!(host_libraries(&script).is_none());
    }
}
