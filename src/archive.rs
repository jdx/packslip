//! Reading the archives `packslip create` is given, so the executables it
//! records are the paths that are really inside them. Paths in a packslip
//! count from the true archive root: a vendor that says `--bin tool` for
//! an archive holding `tool-1.2.3-linux-x64/tool` gets that path written,
//! not the bare name.
//!
//! Decoding is pure Rust (`flate2`, `lzma-rs`, `ruzstd`, `bzip2`, `zip`),
//! so the release tool builds anywhere the rest of the crate does.

use std::io::{BufReader, Read};
use std::path::Path;

use crate::model::Bin;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: cannot list a {format} archive")]
    Unsupported { path: String, format: String },
    #[error("{path}: not a readable {format} archive: {why}")]
    Undecodable {
        path: String,
        format: String,
        why: String,
    },
    #[error("{path}: no file named {name:?} inside the archive")]
    BinNotFound { path: String, name: String },
    #[error(
        "{path}: several files named {name:?} inside the archive ({candidates}); give the path"
    )]
    BinAmbiguous {
        path: String,
        name: String,
        candidates: String,
    },
}

/// Whether [`entries`] can list archives of this format.
pub fn can_list(format: &str) -> bool {
    matches!(
        format,
        "tar" | "tar.gz" | "tgz" | "tar.xz" | "tar.zst" | "tar.bz2" | "zip"
    )
}

/// The compression a format's bytes are under: `gz`, `xz`, `zst`, `bz2`,
/// or empty for none. `None` for a format that is not a tar or a bare
/// executable.
pub fn compression_of(format: &str) -> Option<&'static str> {
    Some(match format {
        "tar" | "raw" => "",
        "tar.gz" | "tgz" | "gz" => "gz",
        "tar.xz" | "xz" => "xz",
        "tar.zst" | "zst" => "zst",
        "tar.bz2" | "bz2" => "bz2",
        _ => return None,
    })
}

/// The decompressed bytes of a file, as a reader, for the compressions
/// [`compression_of`] names.
pub fn decoder(path: &Path, format: &str) -> Result<Box<dyn Read>, Error> {
    let io = |source: std::io::Error| Error::Io {
        path: path.display().to_string(),
        source,
    };
    let undecodable = |why: String| Error::Undecodable {
        path: path.display().to_string(),
        format: format.to_string(),
        why,
    };
    let Some(compression) = compression_of(format) else {
        return Err(Error::Unsupported {
            path: path.display().to_string(),
            format: format.to_string(),
        });
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let reader = BufReader::new(file);
    Ok(match compression {
        "" => Box::new(reader),
        "gz" => Box::new(flate2::read::MultiGzDecoder::new(reader)),
        "bz2" => Box::new(bzip2::read::MultiBzDecoder::new(reader)),
        "zst" => Box::new(
            ruzstd::decoding::StreamingDecoder::new(reader)
                .map_err(|e| undecodable(e.to_string()))?,
        ),
        "xz" => {
            // lzma-rs decodes into memory; release archives fit.
            let mut reader = reader;
            let mut bytes = Vec::new();
            lzma_rs::xz_decompress(&mut reader, &mut bytes)
                .map_err(|e| undecodable(e.to_string()))?;
            Box::new(std::io::Cursor::new(bytes))
        }
        _ => unreachable!("compression_of names only these"),
    })
}

/// The regular files inside an archive, as paths from its root with any
/// leading `./` removed. Directories are not listed.
pub fn entries(path: &Path, format: &str) -> Result<Vec<String>, Error> {
    let io = |source: std::io::Error| Error::Io {
        path: path.display().to_string(),
        source,
    };
    let undecodable = |why: String| Error::Undecodable {
        path: path.display().to_string(),
        format: format.to_string(),
        why,
    };
    let names = match format {
        "zip" => {
            let file = std::fs::File::open(path).map_err(io)?;
            let mut zip = zip::ZipArchive::new(BufReader::new(file))
                .map_err(|e| undecodable(e.to_string()))?;
            let mut names = Vec::new();
            for i in 0..zip.len() {
                let entry = zip
                    .by_index_raw(i)
                    .map_err(|e| undecodable(e.to_string()))?;
                if entry.is_file() {
                    names.push(entry.name().to_string());
                }
            }
            names
        }
        f if can_list(f) => tar_entries(decoder(path, f)?).map_err(|e| match e {
            Error::Undecodable { why, .. } => undecodable(why),
            other => other,
        })?,
        other => {
            return Err(Error::Unsupported {
                path: path.display().to_string(),
                format: other.to_string(),
            });
        }
    };
    Ok(names
        .into_iter()
        .map(|name| normalize(&name))
        .filter(|name| !name.is_empty() && !name.ends_with('/'))
        .collect())
}

/// A path as it is compared: no leading `./`.
pub fn normalize(path: &str) -> String {
    let mut name = path;
    while let Some(rest) = name.strip_prefix("./") {
        name = rest;
    }
    name.to_string()
}

fn tar_entries<R: Read>(reader: R) -> Result<Vec<String>, Error> {
    let mut archive = tar::Archive::new(reader);
    let mut names = Vec::new();
    let entries = archive.entries().map_err(|e| Error::Undecodable {
        path: String::new(),
        format: "tar".into(),
        why: e.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::Undecodable {
            path: String::new(),
            format: "tar".into(),
            why: e.to_string(),
        })?;
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_symlink() || kind.is_hard_link()) {
            continue;
        }
        if let Ok(path) = entry.path() {
            names.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

/// Where each executable is inside the archive. An entry that already
/// carries a path is checked to exist; a plain name is looked up among
/// the archive's files by file name, taking the shallowest match and
/// refusing several at one depth. On Windows a plain `tool` also matches
/// `tool.exe`.
pub fn resolve_bins(
    path: &Path,
    format: &str,
    bins: &[Bin],
    windows: bool,
) -> Result<Vec<Bin>, Error> {
    if bins.is_empty() {
        return Ok(Vec::new());
    }
    let files = entries(path, format)?;
    let mut resolved = Vec::with_capacity(bins.len());
    for bin in bins {
        if bin.path.contains('/') {
            if !files.contains(&bin.path) {
                return Err(Error::BinNotFound {
                    path: path.display().to_string(),
                    name: bin.path.clone(),
                });
            }
            resolved.push(bin.clone());
            continue;
        }
        let wanted = |file_name: &str| {
            file_name == bin.path
                || (windows
                    && !bin.path.contains('.')
                    && file_name.strip_suffix(".exe") == Some(bin.path.as_str()))
        };
        let mut matches: Vec<&String> = files
            .iter()
            .filter(|f| wanted(f.rsplit('/').next().unwrap_or(f)))
            .collect();
        matches.sort_by_key(|f| (f.matches('/').count(), f.len()));
        let shallowest = matches.first().map(|f| f.matches('/').count());
        let at_depth: Vec<&String> = matches
            .iter()
            .copied()
            .filter(|f| Some(f.matches('/').count()) == shallowest)
            .collect();
        match at_depth.as_slice() {
            [] => {
                return Err(Error::BinNotFound {
                    path: path.display().to_string(),
                    name: bin.path.clone(),
                });
            }
            [one] => {
                let name = if windows && !bin.name.contains('.') && one.ends_with(".exe") {
                    format!("{}.exe", bin.name)
                } else {
                    bin.name.clone()
                };
                resolved.push(Bin::named((*one).clone(), name));
            }
            several => {
                return Err(Error::BinAmbiguous {
                    path: path.display().to_string(),
                    name: bin.path.clone(),
                    candidates: several
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn lists_every_supported_format() {
        let dir = tempfile::tempdir().unwrap();
        let files: &[(&str, &[u8])] = &[
            ("tool-1.0/tool", b"bin"),
            ("tool-1.0/README", b"doc"),
            ("./tool-1.0/lib/tool", b"lib"),
        ];
        let tar = tar_bytes(files);
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        let gz = gz.finish().unwrap();
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        bz.write_all(&tar).unwrap();
        let bz = bz.finish().unwrap();
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(&tar), &mut xz).unwrap();
        let cases = [
            ("a.tar", "tar", tar.clone()),
            ("a.tar.gz", "tar.gz", gz.clone()),
            ("a.tgz", "tgz", gz),
            ("a.tar.bz2", "tar.bz2", bz),
            ("a.tar.xz", "tar.xz", xz),
            ("a.zip", "zip", zip_bytes(files)),
        ];
        for (name, format, bytes) in cases {
            let path = write(name, &bytes);
            let mut listed = entries(&path, format).unwrap();
            listed.sort();
            assert_eq!(
                listed,
                ["tool-1.0/README", "tool-1.0/lib/tool", "tool-1.0/tool"],
                "{format}"
            );
            let bins = resolve_bins(&path, format, &[Bin::new("tool")], false).unwrap();
            assert_eq!(bins, [Bin::named("tool-1.0/tool", "tool")], "{format}");
        }
        // zstd is decode-only here; ruzstd cannot write, so skip the
        // round trip and check the format is at least claimed.
        assert!(can_list("tar.zst") && !can_list("7z") && !can_list("raw"));
        let raw = write("raw.bin", b"x");
        assert!(matches!(
            entries(&raw, "7z"),
            Err(Error::Unsupported { .. })
        ));
        assert!(matches!(
            entries(&raw, "tar.gz"),
            Err(Error::Undecodable { .. })
        ));
    }

    #[test]
    fn resolves_bins_by_name_depth_and_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.zip");
        std::fs::write(
            &path,
            zip_bytes(&[
                ("t/bin/tool.exe", b"a"),
                ("t/bin/helper", b"b"),
                ("t/other/helper", b"c"),
                ("top", b"d"),
            ]),
        )
        .unwrap();
        // A Windows lookup for `tool` finds tool.exe and names it so.
        let bins = resolve_bins(&path, "zip", &[Bin::new("tool")], true).unwrap();
        assert_eq!(bins, [Bin::named("t/bin/tool.exe", "tool.exe")]);
        assert!(matches!(
            resolve_bins(&path, "zip", &[Bin::new("tool")], false),
            Err(Error::BinNotFound { .. })
        ));
        // A file at the root wins over nothing; two at one depth are refused.
        assert_eq!(
            resolve_bins(&path, "zip", &[Bin::new("top")], false).unwrap(),
            [Bin::new("top")]
        );
        let err = resolve_bins(&path, "zip", &[Bin::new("helper")], false).unwrap_err();
        assert!(matches!(err, Error::BinAmbiguous { .. }), "{err}");
        // A given path is checked, not searched.
        assert_eq!(
            resolve_bins(&path, "zip", &[Bin::named("t/other/helper", "h")], false).unwrap(),
            [Bin::named("t/other/helper", "h")]
        );
        assert!(matches!(
            resolve_bins(&path, "zip", &[Bin::new("t/bin/nope")], false),
            Err(Error::BinNotFound { .. })
        ));
        assert!(resolve_bins(&path, "zip", &[], false).unwrap().is_empty());
    }
}
