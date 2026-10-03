//! Whole-artifact extraction into a destination-local, private staging tree.
//! No archive entry is trusted to choose an absolute path or permission mode.
use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("archive I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsafe archive: {0}")]
    Unsafe(String),
    #[error("archive exceeds local extraction limits")]
    Limit,
    #[error("cannot extract artifact format {0:?}")]
    Format(String),
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub bytes: u64,
    pub entries: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 10 * 1024 * 1024 * 1024,
            entries: 100_000,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
    Symlink(String),
    Hardlink(String),
}
#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    kind: Kind,
    size: u64,
    executable: bool,
}

/// A stage and the exactly mapped declared commands. Holding the TempDir
/// removes an uncommitted stage on ordinary errors; the transaction keeps it.
pub struct Extracted {
    pub tree: tempfile::TempDir,
    pub bins: Vec<(String, PathBuf)>,
}

fn path(input: &str) -> Result<PathBuf, Error> {
    if input.len() > 4096
        || input.split('/').count() > 128
        || input.starts_with('/')
        || input.contains(['\\', ':', '\0'])
    {
        return Err(Error::Unsafe(format!("invalid path {input:?}")));
    }
    let mut result = PathBuf::new();
    for component in input.split('/').filter(|c| !c.is_empty() && *c != ".") {
        if component == ".." || component.ends_with([' ', '.']) {
            return Err(Error::Unsafe(format!("invalid path {input:?}")));
        }
        let base = component.split('.').next().unwrap().to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&base.as_str())
            || base
                .strip_prefix("COM")
                .or_else(|| base.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    (suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit())
                        || ["¹", "²", "³"].contains(&suffix)
                })
        {
            return Err(Error::Unsafe(format!("reserved path {input:?}")));
        }
        if component.to_ascii_lowercase().starts_with(".packslip") {
            return Err(Error::Unsafe(
                "archive uses a reserved ownership path".into(),
            ));
        }
        result.push(component);
    }
    if result.as_os_str().is_empty() {
        return Err(Error::Unsafe("empty archive path".into()));
    }
    Ok(result)
}
fn link_path(parent: &Path, target: &str) -> Result<PathBuf, Error> {
    if target.len() > 4096
        || target.split('/').count() > 128
        || target.starts_with('/')
        || target.contains(['\\', ':', '\0'])
    {
        return Err(Error::Unsafe(
            "absolute or platform-specific link target".into(),
        ));
    }
    let mut normalized = parent.to_path_buf();
    for part in target.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." {
            if !normalized.pop() {
                return Err(Error::Unsafe("link escapes extraction root".into()));
            }
        } else {
            normalized.push(path(part)?);
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(Error::Unsafe("link targets extraction root".into()));
    }
    Ok(normalized)
}
fn prefix(entries: &[Entry]) -> Option<PathBuf> {
    let first = entries.first()?.path.components().next()?;
    let base = PathBuf::from(first.as_os_str());
    let mut child = false;
    for entry in entries {
        if !entry.path.starts_with(&base) {
            return None;
        }
        if entry.path == base && entry.kind != Kind::Dir {
            return None;
        }
        child |= entry.path != base;
    }
    child.then_some(base)
}
fn mapped(input: &Path, strip: Option<&Path>) -> PathBuf {
    strip
        .and_then(|p| input.strip_prefix(p).ok())
        .unwrap_or(input)
        .to_path_buf()
}
fn validate(entries: &[Entry], limits: Limits, strip: Option<&Path>) -> Result<(), Error> {
    if entries.len() as u64 > limits.entries {
        return Err(Error::Limit);
    }
    let mut seen = BTreeMap::new();
    let mut total = 0u64;
    for entry in entries {
        total = total.checked_add(entry.size).ok_or(Error::Limit)?;
        if total > limits.bytes {
            return Err(Error::Limit);
        }
        let map = mapped(&entry.path, strip);
        if map.as_os_str().is_empty() {
            continue;
        }
        if seen
            .insert(
                PathBuf::from(
                    map.to_str()
                        .ok_or_else(|| Error::Unsafe("non-UTF8 mapped path".into()))?
                        .to_lowercase(),
                ),
                &entry.kind,
            )
            .is_some()
        {
            return Err(Error::Unsafe(format!(
                "duplicate mapped path {}",
                map.display()
            )));
        }
        match &entry.kind {
            Kind::Symlink(target) => {
                link_path(entry.path.parent().unwrap_or(Path::new("")), target)?;
                link_path(map.parent().unwrap_or(Path::new("")), target)?;
            }
            Kind::Hardlink(target) => {
                let target = path(target)?;
                let target = mapped(&target, strip);
                if target.as_os_str().is_empty() {
                    return Err(Error::Unsafe("hard link targets extraction root".into()));
                }
            }
            _ => {}
        }
    }
    for name in seen.keys() {
        for ancestor in name.ancestors().skip(1) {
            if let Some(kind) = seen.get(ancestor)
                && **kind != Kind::Dir
            {
                return Err(Error::Unsafe(
                    "archive entry has a non-directory ancestor".into(),
                ));
            }
        }
    }
    Ok(())
}
struct Bounded<'a, W: Write> {
    inner: &'a mut W,
    remaining: u64,
}
impl<W: Write> Write for Bounded<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("decompressed archive exceeds limit"));
        }
        let n = self.inner.write(bytes)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn decoded(
    input: &Path,
    format: &str,
    parent: &Path,
    limit: u64,
) -> Result<tempfile::NamedTempFile, Error> {
    let mut spool = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = Bounded {
        inner: &mut spool,
        remaining: limit,
    };
    if crate::archive::compression_of(format) == Some("xz") {
        let mut reader = std::io::BufReader::new(std::fs::File::open(input)?);
        use std::io::BufRead as _;
        let mut stream = lzma_rust2::XzStream::new_mem_limit(true, 128 * 1024);
        let mut out = [0u8; 64 * 1024];
        loop {
            let input = reader.fill_buf()?;
            let eof = input.is_empty();
            let result = stream.process(
                input,
                &mut out,
                if eof {
                    lzma_rust2::Action::Finish
                } else {
                    lzma_rust2::Action::Run
                },
            )?;
            reader.consume(result.bytes_consumed);
            writer.write_all(&out[..result.bytes_produced])?;
            if result.status == lzma_rust2::Status::StreamEnd {
                break;
            }
            if result.bytes_consumed == 0 && result.bytes_produced == 0 {
                return Err(Error::Unsafe("truncated or stalled xz stream".into()));
            }
        }
    } else {
        let mut reader =
            crate::archive::decoder(input, format).map_err(|e| Error::Unsafe(e.to_string()))?;
        std::io::copy(&mut reader, &mut writer)?;
    }
    spool.as_file_mut().rewind()?;
    Ok(spool)
}
fn tar_entries(file: &mut std::fs::File, limits: Limits) -> Result<Vec<Entry>, Error> {
    file.rewind()?;
    {
        // The tar reader normally allocates GNU/PAX extension bodies before
        // yielding the entry. Inspect raw headers first to cap those allocations.
        let mut archive = tar::Archive::new(&mut *file);
        for (index, entry) in archive.entries()?.raw(true).enumerate() {
            if index as u64 >= limits.entries {
                return Err(Error::Limit);
            }
            let entry = entry?;
            let kind = entry.header().entry_type();
            if (kind.is_gnu_longname()
                || kind.is_gnu_longlink()
                || kind.is_pax_local_extensions()
                || kind.is_pax_global_extensions())
                && entry.size() > 16 * 1024
            {
                return Err(Error::Limit);
            }
        }
    }
    file.rewind()?;
    let mut archive = tar::Archive::new(file);
    let mut result = Vec::new();
    let mut size = 0u64;
    let mut count = 0u64;
    for entry in archive.entries()? {
        count += 1;
        if count > limits.entries {
            return Err(Error::Limit);
        }
        let entry = entry?;
        let p = entry.path()?;
        let p = p
            .to_str()
            .ok_or_else(|| Error::Unsafe("non-UTF8 archive path".into()))?;
        // Root directory placeholders are structural, not files.
        if (p == "." || p == "./") && entry.header().entry_type().is_dir() {
            continue;
        }
        let p = path(p)?;
        let kind = entry.header().entry_type();
        let link = || {
            let target = entry
                .link_name()?
                .ok_or_else(|| std::io::Error::other("invalid archive link"))?;
            let target = target
                .to_str()
                .ok_or_else(|| std::io::Error::other("non-UTF8 archive link"))?;
            if target.len() > 4096 || target.split('/').count() > 128 {
                return Err(std::io::Error::other("archive link exceeds path limits"));
            }
            Ok(target.to_owned())
        };
        let kind = if kind.is_file() {
            Kind::File
        } else if kind.is_dir() {
            Kind::Dir
        } else if kind.is_symlink() {
            Kind::Symlink(link()?)
        } else if kind.is_hard_link() {
            Kind::Hardlink(link()?)
        } else {
            return Err(Error::Unsafe("special archive entry".into()));
        };
        let bytes = match &kind {
            Kind::File => entry.size(),
            Kind::Symlink(target) | Kind::Hardlink(target) => target.len() as u64,
            _ => 0,
        };
        size = size.checked_add(bytes).ok_or(Error::Limit)?;
        if size > limits.bytes || result.len() as u64 >= limits.entries {
            return Err(Error::Limit);
        }
        result.push(Entry {
            path: p,
            kind,
            size: bytes,
            executable: entry.header().mode()? & 0o111 != 0,
        });
    }
    Ok(result)
}
fn output(root: &Path, relative: &Path) -> Result<std::fs::File, Error> {
    let out = root.join(relative);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)?)
}
fn mode(path: &Path, executable: bool) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
        )?;
    }
    #[cfg(not(unix))]
    let _ = (path, executable);
    Ok(())
}
// Resolve existing symlinks without requiring the final target to exist.
// Preserve .. until after expanding links; normalizing first could hide an
// escape through a directory alias. Missing, still-confined targets are valid.
#[cfg(unix)]
fn resolved_link(root: &Path, relative: &Path) -> Result<PathBuf, Error> {
    use std::path::Component;
    let mut pending: std::collections::VecDeque<_> = relative
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let mut resolved = PathBuf::new();
    let mut expansions = 0;
    while let Some(component) = pending.pop_front() {
        match Path::new(&component).components().next() {
            Some(Component::CurDir) => continue,
            Some(Component::ParentDir) => {
                if !resolved.pop() {
                    return Err(Error::Unsafe(
                        "resolved link escapes extraction root".into(),
                    ));
                }
                continue;
            }
            Some(Component::Normal(_)) => resolved.push(&component),
            _ => return Err(Error::Unsafe("absolute link target".into())),
        }
        match std::fs::symlink_metadata(root.join(&resolved)) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                expansions += 1;
                if expansions > 128 {
                    return Err(Error::Unsafe("cyclic or excessively deep link".into()));
                }
                let target = std::fs::read_link(root.join(&resolved))?;
                resolved.pop();
                for component in target.components().rev() {
                    pending.push_front(component.as_os_str().to_owned());
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(resolved)
}

#[cfg(unix)]
fn links(
    root: &Path,
    entries: &[Entry],
    strip: Option<&Path>,
    limits: Limits,
) -> Result<(), Error> {
    let _ = limits;
    let mut hard = BTreeMap::new();
    for entry in entries {
        let out = mapped(&entry.path, strip);
        if out.as_os_str().is_empty() {
            continue;
        }
        match &entry.kind {
            Kind::Symlink(target) => {
                let destination = root.join(&out);
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, destination)?;
            }
            Kind::Hardlink(target) => {
                hard.insert(out, mapped(&path(target)?, strip));
            }
            _ => {}
        }
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, Kind::Symlink(_)))
    {
        resolved_link(root, &mapped(&entry.path, strip))?;
    }
    // Resolve each dependency once. Retrying every uncreated hard link each
    // round makes a reverse-ordered chain quadratic in filesystem operations.
    let mut resolved: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    for out in hard.keys() {
        let mut current = out.clone();
        let mut chain = Vec::new();
        let mut visited = std::collections::BTreeSet::new();
        let anchor = loop {
            if let Some(anchor) = resolved.get(&current) {
                break anchor.clone();
            }
            if !visited.insert(current.clone()) {
                return Err(Error::Unsafe("cyclic archive hard link".into()));
            }
            if let Some(target) = hard.get(&current) {
                chain.push(current);
                current = resolved_link(root, target)?;
            } else if root.join(&current).is_file() {
                break current;
            } else {
                return Err(Error::Unsafe("unresolved archive hard link".into()));
            }
        };
        for item in chain {
            resolved.insert(item, anchor.clone());
        }
    }
    for (out, anchor) in resolved {
        let destination = root.join(out);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::hard_link(root.join(anchor), destination)?;
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, Kind::Symlink(_) | Kind::Hardlink(_)))
    {
        resolved_link(root, &mapped(&entry.path, strip))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn links(
    root: &Path,
    entries: &[Entry],
    strip: Option<&Path>,
    limits: Limits,
) -> Result<(), Error> {
    materialized_links(root, entries, strip, limits)
}

// Resolve the link graph to staged regular files before copying any link.
// Memoization makes long, reverse-ordered chains linear rather than repeatedly
// scanning the archive. No native Windows symlink privilege is required.
#[cfg(any(not(unix), test))]
fn materialized_links(
    root: &Path,
    entries: &[Entry],
    strip: Option<&Path>,
    limits: Limits,
) -> Result<(), Error> {
    let mut physical_size = entries
        .iter()
        .try_fold(0u64, |sum, e| sum.checked_add(e.size))
        .ok_or(Error::Limit)?;
    let fold = |path: &Path| -> Result<PathBuf, Error> {
        Ok(PathBuf::from(
            path.to_str()
                .ok_or_else(|| Error::Unsafe("non-UTF8 link path".into()))?
                .to_lowercase(),
        ))
    };
    let mut targets = BTreeMap::new();
    let mut files = BTreeMap::new();
    for entry in entries {
        let out = mapped(&entry.path, strip);
        let key = fold(&out)?;
        let target = match &entry.kind {
            Kind::Symlink(target) => link_path(out.parent().unwrap_or(Path::new("")), target)?,
            Kind::Hardlink(target) => mapped(&path(target)?, strip),
            Kind::File => {
                files.insert(key, out);
                continue;
            }
            _ => continue,
        };
        targets.insert(
            key,
            (out, fold(&target)?, matches!(entry.kind, Kind::Symlink(_))),
        );
    }
    // Resolve all links to the bytes to copy; then resolve hard-link inode
    // anchors separately, stopping at materialized symlinks. This preserves
    // h -> a when a is a copied symlink rather than linking h to a's source.
    fn resolve(
        targets: &BTreeMap<PathBuf, (PathBuf, PathBuf, bool)>,
        files: &BTreeMap<PathBuf, PathBuf>,
        stop_at_copy: bool,
    ) -> Result<BTreeMap<PathBuf, PathBuf>, Error> {
        let mut resolved: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        for key in targets.keys() {
            let mut current = key.clone();
            let mut chain = Vec::new();
            let mut visited = std::collections::BTreeSet::new();
            let final_path = loop {
                if let Some(final_path) = resolved.get(&current) {
                    break final_path.clone();
                }
                if !visited.insert(current.clone()) {
                    return Err(Error::Unsafe("cyclic archive link".into()));
                }
                if let Some((out, target, is_copy)) = targets.get(&current) {
                    if stop_at_copy && *is_copy {
                        break out.clone();
                    }
                    chain.push(current);
                    current = target.clone();
                } else {
                    break files
                        .get(&current)
                        .ok_or_else(|| {
                            Error::Unsafe("Windows archive link is not a regular file".into())
                        })?
                        .clone();
                }
            };
            for item in chain {
                resolved.insert(item, final_path.clone());
            }
        }
        Ok(resolved)
    }
    let copied = resolve(&targets, &files, false)?;
    let anchors = resolve(&targets, &files, true)?;
    for (out, _, is_copy) in targets.values() {
        if !is_copy {
            continue;
        }
        let target = root.join(&copied[&fold(out)?]);
        physical_size = physical_size
            .checked_add(std::fs::metadata(&target)?.len())
            .ok_or(Error::Limit)?;
        if physical_size > limits.bytes {
            return Err(Error::Limit);
        }
        let destination = root.join(out);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(target, destination)?;
    }
    for (key, (out, _, is_copy)) in &targets {
        if *is_copy {
            continue;
        }
        let destination = root.join(out);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::hard_link(root.join(&anchors[key]), destination)?;
    }
    Ok(())
}

/// Unpack every ordinary archive file beside the final destination, strip one
/// common enclosing directory, then validate only the declared command paths.
/// Never run a command or install a resource while extracting.
pub fn extract(
    input: &Path,
    artifact: &crate::Artifact,
    parent: &Path,
    limits: Limits,
) -> Result<Extracted, Error> {
    if limits.bytes == 0 || limits.entries == 0 {
        return Err(Error::Limit);
    }
    std::fs::create_dir_all(parent)?;
    let tree = tempfile::Builder::new()
        .prefix(".packslip-stage-")
        .tempdir_in(parent)?;
    let format = artifact
        .format
        .as_deref()
        .ok_or_else(|| Error::Format("absent".into()))?;
    let (entries, strip) = if format.starts_with("tar") || format == "tgz" {
        let spool_limit = limits
            .bytes
            .checked_add(limits.entries.saturating_mul(2048))
            .and_then(|n| n.checked_add(1024 * 1024))
            .ok_or(Error::Limit)?;
        let mut spool = decoded(input, format, parent, spool_limit)?;
        let entries = tar_entries(spool.as_file_mut(), limits)?;
        let strip = prefix(&entries);
        validate(&entries, limits, strip.as_deref())?;
        spool.as_file_mut().rewind()?;
        let mut archive = tar::Archive::new(spool.as_file_mut());
        for entry in archive.entries()? {
            let mut entry = entry?;
            let raw = entry.path()?.into_owned();
            let raw = raw
                .to_str()
                .ok_or_else(|| Error::Unsafe("non-UTF8 archive path".into()))?;
            if (raw == "." || raw == "./") && entry.header().entry_type().is_dir() {
                continue;
            }
            let relative = mapped(&path(raw)?, strip.as_deref());
            if relative.as_os_str().is_empty() {
                continue;
            }
            if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(tree.path().join(&relative))?;
            } else if entry.header().entry_type().is_file() {
                let mut file = output(tree.path(), &relative)?;
                let n = std::io::copy(&mut entry, &mut file)?;
                if n != entry.size() {
                    return Err(Error::Unsafe("truncated archive file".into()));
                }
                file.sync_all()?;
                mode(
                    &tree.path().join(relative),
                    entry.header().mode()? & 0o111 != 0,
                )?;
            }
        }
        (entries, strip)
    } else if format == "zip" {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(input)?)
            .map_err(|e| Error::Unsafe(e.to_string()))?;
        if zip.len() as u64 > limits.entries {
            return Err(Error::Limit);
        }
        let mut entries = Vec::new();
        let mut total = 0u64;
        for index in 0..zip.len() {
            let mut file = zip
                .by_index(index)
                .map_err(|e| Error::Unsafe(e.to_string()))?;
            // `create` reads `\` in a zip entry name as `/`, so extraction
            // does too; `path` still refuses `..\`, `\abs`, and `C:\` names.
            let file_path = path(&crate::archive::zip_path(file.name()))?;
            let file_mode = file.unix_mode().unwrap_or(0o100644);
            let file_type = file_mode & 0o170000;
            let size = if file.is_dir() { 0 } else { file.size() };
            total = total.checked_add(size).ok_or(Error::Limit)?;
            if total > limits.bytes {
                return Err(Error::Limit);
            }
            let kind = if file.is_dir() {
                Kind::Dir
            } else if file_type == 0o120000 {
                if file.size() > 16384 {
                    return Err(Error::Limit);
                }
                let mut target = String::new();
                // One sentinel byte detects contents exceeding the declared size.
                file.by_ref()
                    .take(size.checked_add(1).ok_or(Error::Limit)?)
                    .read_to_string(&mut target)?;
                if target.len() as u64 != size {
                    return Err(Error::Unsafe("ZIP link size differs from directory".into()));
                }
                Kind::Symlink(target)
            } else if file_type == 0 || file_type == 0o100000 {
                Kind::File
            } else {
                return Err(Error::Unsafe("special ZIP entry".into()));
            };
            entries.push(Entry {
                path: file_path,
                kind,
                size,
                executable: file_mode & 0o111 != 0,
            });
        }
        let strip = prefix(&entries);
        validate(&entries, limits, strip.as_deref())?;
        for (index, entry) in entries.iter().enumerate() {
            let relative = mapped(&entry.path, strip.as_deref());
            if relative.as_os_str().is_empty() {
                continue;
            }
            match entry.kind {
                Kind::Dir => std::fs::create_dir_all(tree.path().join(relative))?,
                Kind::File => {
                    let file = zip
                        .by_index(index)
                        .map_err(|e| Error::Unsafe(e.to_string()))?;
                    let mut out = output(tree.path(), &relative)?;
                    let n = std::io::copy(
                        &mut file.take(entry.size.checked_add(1).ok_or(Error::Limit)?),
                        &mut out,
                    )?;
                    if n != entry.size {
                        return Err(Error::Unsafe("ZIP size differs from directory".into()));
                    }
                    out.sync_all()?;
                    mode(&tree.path().join(relative), entry.executable)?;
                }
                _ => {}
            }
        }
        (entries, strip)
    } else if ["raw", "gz", "xz", "zst", "bz2"].contains(&format) {
        let name = artifact
            .name
            .strip_suffix(&format!(
                ".{}",
                crate::archive::compression_of(format).unwrap_or("")
            ))
            .unwrap_or(&artifact.name);
        let name = path(name)?;
        let mut decoded = decoded(input, format, parent, limits.bytes)?;
        let mut out = output(tree.path(), &name)?;
        std::io::copy(decoded.as_file_mut(), &mut out)?;
        out.sync_all()?;
        mode(&tree.path().join(name), true)?;
        (vec![], None)
    } else {
        return Err(Error::Format(format.into()));
    };
    links(tree.path(), &entries, strip.as_deref(), limits)?;
    let mut bins = Vec::new();
    let canonical = tree.path().canonicalize()?;
    for bin in &artifact.bin {
        let raw = path(&bin.path)?;
        let mapped = mapped(&raw, strip.as_deref());
        let target = tree.path().join(&mapped).canonicalize()?;
        if !target.starts_with(&canonical) || !target.is_file() {
            return Err(Error::Unsafe(
                "declared command is not a file within the stage".into(),
            ));
        }
        path(&bin.name)?;
        if bin.name.contains('/') {
            return Err(Error::Unsafe("command name contains a path".into()));
        }
        mode(&target, true)?;
        bins.push((bin.name.clone(), mapped));
    }
    if bins.is_empty() {
        return Err(Error::Unsafe("artifact declares no commands".into()));
    }
    Ok(Extracted { tree, bins })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn artifact(name: &str, format: &str, bins: Vec<crate::Bin>) -> crate::Artifact {
        serde_json::from_value(serde_json::json!({"name":name,"url":"https://example.invalid/tool","size":1,"format":format,"bin":bins})).unwrap()
    }
    fn tar_file(parent: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = parent.join("archive.tar");
        let mut archive = tar::Builder::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o4755);
            header.set_cksum();
            archive.append_data(&mut header, name, *bytes).unwrap();
        }
        archive.finish().unwrap();
        path
    }
    #[test]
    fn full_tree_strips_one_directory_and_keeps_runtime_off_path() {
        let parent = tempfile::tempdir().unwrap();
        let input = tar_file(
            parent.path(),
            &[("pkg/bin/tool", b"tool"), ("pkg/lib/runtime", b"runtime")],
        );
        let extracted = extract(
            &input,
            &artifact("archive.tar", "tar", vec![crate::Bin::new("pkg/bin/tool")]),
            parent.path(),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(
            extracted.bins,
            vec![("tool".into(), PathBuf::from("bin/tool"))]
        );
        assert_eq!(
            std::fs::read(extracted.tree.path().join("lib/runtime")).unwrap(),
            b"runtime"
        );
        assert!(!extracted.tree.path().join("pkg").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(extracted.tree.path().join("bin/tool"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777,
                0o755
            );
        }
    }
    #[test]
    fn paths_limits_special_entries_and_duplicates_fail() {
        for name in [
            "../escape",
            "/escape",
            "C:/escape",
            "x\\escape",
            "dir/NUL",
            "dir/conin$",
            "dir/CONOUT$.txt",
            "dir/com¹.txt",
            "dir/LPT²",
            "dir/COM³",
            "dir/tool.",
            ".packslip-owner.json",
        ] {
            assert!(path(name).is_err(), "{name}");
        }
        let parent = tempfile::tempdir().unwrap();
        let input = tar_file(parent.path(), &[("tool", b"too large")]);
        assert!(
            extract(
                &input,
                &artifact("archive.tar", "tar", vec![crate::Bin::new("tool")]),
                parent.path(),
                Limits {
                    bytes: 2,
                    entries: 100
                }
            )
            .is_err()
        );
        let input = tar_file(parent.path(), &[("tool", b"a"), ("Tool", b"b")]);
        assert!(
            extract(
                &input,
                &artifact("archive.tar", "tar", vec![crate::Bin::new("tool")]),
                parent.path(),
                Limits::default()
            )
            .is_err()
        );
        assert!(
            extract(
                &input,
                &artifact("archive.tar", "tar", vec![crate::Bin::new("tool")]),
                parent.path(),
                Limits {
                    bytes: 100,
                    entries: 1
                }
            )
            .is_err()
        );
        let mut archive = tar::Builder::new(std::fs::File::create(&input).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Fifo);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, "fifo", std::io::empty())
            .unwrap();
        archive.finish().unwrap();
        assert!(
            extract(
                &input,
                &artifact("archive.tar", "tar", vec![crate::Bin::new("fifo")]),
                parent.path(),
                Limits::default()
            )
            .is_err()
        );
    }
    #[test]
    fn tar_extension_allocations_and_link_targets_are_bounded() {
        let parent = tempfile::tempdir().unwrap();
        for kind in [
            tar::EntryType::GNULongLink,
            tar::EntryType::GNULongName,
            tar::EntryType::XHeader,
        ] {
            let input = parent.path().join("extension.tar");
            let mut builder = tar::Builder::new(std::fs::File::create(&input).unwrap());
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(16 * 1024 + 1);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "metadata",
                    vec![b'a'; 16 * 1024 + 1].as_slice(),
                )
                .unwrap();
            builder.finish().unwrap();
            let mut file = std::fs::File::open(&input).unwrap();
            assert!(matches!(
                tar_entries(&mut file, Limits::default()),
                Err(Error::Limit)
            ));
        }
        for target in ["a/".repeat(129), "a".repeat(4097)] {
            assert!(link_path(Path::new(""), &target).is_err());
            let input = parent.path().join("long-link.tar");
            let mut builder = tar::Builder::new(std::fs::File::create(&input).unwrap());
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            builder.append_link(&mut header, "link", &target).unwrap();
            builder.finish().unwrap();
            let mut file = std::fs::File::open(&input).unwrap();
            assert!(tar_entries(&mut file, Limits::default()).is_err());
        }
        let input = parent.path().join("link.tar");
        let mut builder = tar::Builder::new(std::fs::File::create(&input).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        builder
            .append_link(&mut header, "optional", "runtime")
            .unwrap();
        builder.finish().unwrap();
        let mut file = std::fs::File::open(&input).unwrap();
        assert!(matches!(
            tar_entries(
                &mut file,
                Limits {
                    bytes: 6,
                    entries: 100
                }
            ),
            Err(Error::Limit)
        ));
        assert_eq!(
            tar_entries(
                &mut file,
                Limits {
                    bytes: 7,
                    entries: 100
                }
            )
            .unwrap()[0]
                .size,
            7
        );
    }
    #[test]
    fn links_are_checked_before_and_after_stripping() {
        let entries = vec![Entry {
            path: PathBuf::from("pkg/link"),
            kind: Kind::Symlink("../escape".into()),
            size: 0,
            executable: false,
        }];
        assert!(validate(&entries, Limits::default(), Some(Path::new("pkg"))).is_err());
        let entries = vec![
            Entry {
                path: PathBuf::from("pkg/link"),
                kind: Kind::Symlink("target".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: PathBuf::from("pkg/link/child"),
                kind: Kind::File,
                size: 1,
                executable: true,
            },
        ];
        assert!(validate(&entries, Limits::default(), Some(Path::new("pkg"))).is_err());
    }
    #[test]
    fn zip_and_compressed_bare_executables_are_supported() {
        let parent = tempfile::tempdir().unwrap();
        let input = parent.path().join("tool.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&input).unwrap());
        zip.start_file("pkg/tool.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"tool").unwrap();
        zip.start_file("pkg/runtime.dll", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"runtime").unwrap();
        zip.finish().unwrap();
        let extracted = extract(
            &input,
            &artifact("tool.zip", "zip", vec![crate::Bin::new("pkg/tool.exe")]),
            parent.path(),
            Limits::default(),
        )
        .unwrap();
        assert!(extracted.tree.path().join("runtime.dll").is_file());
        let input = parent.path().join("tool.gz");
        let mut gzip = flate2::write::GzEncoder::new(
            std::fs::File::create(&input).unwrap(),
            flate2::Compression::default(),
        );
        gzip.write_all(b"tool").unwrap();
        gzip.finish().unwrap();
        let extracted = extract(
            &input,
            &artifact("tool.gz", "gz", vec![crate::Bin::new("tool")]),
            parent.path(),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(extracted.tree.path().join("tool")).unwrap(),
            b"tool"
        );
        assert!(
            extract(
                &input,
                &artifact("tool.gz", "gz", vec![crate::Bin::new("tool")]),
                parent.path(),
                Limits {
                    bytes: 2,
                    entries: 100
                }
            )
            .is_err()
        );
    }
    #[test]
    fn zip_entries_may_separate_directories_with_backslashes() {
        let parent = tempfile::tempdir().unwrap();
        let zip_of = |names: &[&str]| {
            let input = parent.path().join("tool.zip");
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&input).unwrap());
            for name in names {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(name.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
            input
        };
        let tool = || artifact("tool.zip", "zip", vec![crate::Bin::new("pkg/bin/tool.exe")]);
        let input = zip_of(&["pkg\\bin\\tool.exe", "pkg\\lib\\runtime.dll"]);
        let extracted = extract(&input, &tool(), parent.path(), Limits::default()).unwrap();
        assert_eq!(
            extracted.bins,
            vec![("tool".into(), PathBuf::from("bin/tool.exe"))]
        );
        assert!(extracted.tree.path().join("lib/runtime.dll").is_file());
        for escape in [
            "..\\escape",
            "pkg\\..\\..\\escape",
            "\\escape",
            "C:\\escape",
        ] {
            let input = zip_of(&["pkg\\bin\\tool.exe", escape]);
            assert!(
                matches!(
                    extract(&input, &tool(), parent.path(), Limits::default()),
                    Err(Error::Unsafe(_))
                ),
                "{escape}"
            );
        }
    }
    #[test]
    fn zip_link_targets_count_toward_aggregate_bytes_before_retention() {
        let parent = tempfile::tempdir().unwrap();
        let input = parent.path().join("links.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&input).unwrap());
        zip.start_file("pkg/tool", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        zip.add_symlink("pkg/a", "tool", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.add_symlink("pkg/b", "tool", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert!(matches!(
            extract(
                &input,
                &artifact("links.zip", "zip", vec![crate::Bin::new("pkg/tool")]),
                parent.path(),
                Limits {
                    bytes: 5,
                    entries: 100
                }
            ),
            Err(Error::Limit)
        ));
    }
    #[test]
    fn windows_file_link_graph_is_independent_of_archive_order_and_bounded() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"file").unwrap();
        let entries = vec![
            Entry {
                path: "a".into(),
                kind: Kind::Symlink("b".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "b".into(),
                kind: Kind::Symlink("c".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "c".into(),
                kind: Kind::Hardlink("file".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "file".into(),
                kind: Kind::File,
                size: 4,
                executable: false,
            },
        ];
        materialized_links(
            root.path(),
            &entries,
            None,
            Limits {
                bytes: 12,
                entries: 100,
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"file");
        assert_eq!(std::fs::read(root.path().join("b")).unwrap(), b"file");
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"file").unwrap();
        assert!(matches!(
            materialized_links(
                root.path(),
                &entries,
                None,
                Limits {
                    bytes: 11,
                    entries: 100
                }
            ),
            Err(Error::Limit)
        ));
        let cyclic = vec![
            entries[0].clone(),
            Entry {
                path: "b".into(),
                kind: Kind::Symlink("a".into()),
                size: 0,
                executable: false,
            },
        ];
        assert!(materialized_links(root.path(), &cyclic, None, Limits::default()).is_err());
        let overflowing = vec![
            Entry {
                size: u64::MAX,
                ..entries[3].clone()
            },
            entries[3].clone(),
        ];
        assert!(matches!(
            materialized_links(root.path(), &overflowing, None, Limits::default()),
            Err(Error::Limit)
        ));
    }
    #[test]
    fn windows_casefolded_link_chains_preserve_materialized_hardlink_targets() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"file").unwrap();
        let entries = vec![
            Entry {
                path: "a".into(),
                kind: Kind::Symlink("B".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "b".into(),
                kind: Kind::Symlink("FILE".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "h".into(),
                kind: Kind::Hardlink("A".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "file".into(),
                kind: Kind::File,
                size: 4,
                executable: false,
            },
        ];
        materialized_links(root.path(), &entries, None, Limits::default()).unwrap();
        std::fs::write(root.path().join("h"), b"changed").unwrap();
        assert_eq!(std::fs::read(root.path().join("a")).unwrap(), b"changed");
        assert_eq!(std::fs::read(root.path().join("file")).unwrap(), b"file");
        assert_eq!(std::fs::read(root.path().join("b")).unwrap(), b"file");
    }
    #[test]
    #[cfg(unix)]
    fn unix_hardlink_chains_resolve_once_and_reject_cycles() {
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"runtime").unwrap();
        let entries: Vec<_> = (0..4096)
            .map(|i| Entry {
                path: format!("h{i:04}").into(),
                kind: Kind::Hardlink(if i == 4095 {
                    "file".into()
                } else {
                    format!("h{:04}", i + 1)
                }),
                size: 0,
                executable: false,
            })
            .collect();
        links(root.path(), &entries, None, Limits::default()).unwrap();
        let inode = std::fs::metadata(root.path().join("file")).unwrap().ino();
        for entry in &entries {
            assert_eq!(
                std::fs::metadata(root.path().join(&entry.path))
                    .unwrap()
                    .ino(),
                inode
            );
        }
        let root = tempfile::tempdir().unwrap();
        let cyclic = vec![
            Entry {
                path: "a".into(),
                kind: Kind::Hardlink("b".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "b".into(),
                kind: Kind::Hardlink("a".into()),
                size: 0,
                executable: false,
            },
        ];
        assert!(links(root.path(), &cyclic, None, Limits::default()).is_err());
        assert!(!root.path().join("a").exists());
    }
    #[test]
    #[cfg(unix)]
    fn dangling_links_are_preserved_but_directory_alias_escapes_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let dangling = vec![Entry {
            path: "optional".into(),
            kind: Kind::Symlink("missing/runtime".into()),
            size: 0,
            executable: false,
        }];
        validate(&dangling, Limits::default(), None).unwrap();
        links(root.path(), &dangling, None, Limits::default()).unwrap();
        assert_eq!(
            std::fs::read_link(root.path().join("optional")).unwrap(),
            PathBuf::from("missing/runtime")
        );
        let escaped = vec![
            Entry {
                path: "deep/path/alias".into(),
                kind: Kind::Symlink("../../top".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "deep/path/escape".into(),
                kind: Kind::Symlink("alias/../../outside".into()),
                size: 0,
                executable: false,
            },
        ];
        std::fs::create_dir(root.path().join("top")).unwrap();
        validate(&escaped, Limits::default(), None).unwrap();
        assert!(matches!(
            links(root.path(), &escaped, None, Limits::default()),
            Err(Error::Unsafe(_))
        ));
        let cyclic = vec![
            Entry {
                path: "a".into(),
                kind: Kind::Symlink("b".into()),
                size: 0,
                executable: false,
            },
            Entry {
                path: "b".into(),
                kind: Kind::Symlink("a".into()),
                size: 0,
                executable: false,
            },
        ];
        assert!(matches!(
            links(root.path(), &cyclic, None, Limits::default()),
            Err(Error::Unsafe(_))
        ));
    }
    #[test]
    fn non_utf8_validation_returns_an_error() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let entries = vec![Entry {
                path: std::ffi::OsString::from_vec(vec![0xff]).into(),
                kind: Kind::File,
                size: 0,
                executable: false,
            }];
            assert!(matches!(
                validate(&entries, Limits::default(), None),
                Err(Error::Unsafe(_))
            ));
        }
    }
    #[test]
    fn xz_streams_are_bounded_and_do_not_buffer_the_payload() {
        let parent = tempfile::tempdir().unwrap();
        let input = parent.path().join("tool.xz");
        let mut compressed = std::fs::File::create(&input).unwrap();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(b"tool"), &mut compressed).unwrap();
        let extracted = extract(
            &input,
            &artifact("tool.xz", "xz", vec![crate::Bin::new("tool")]),
            parent.path(),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(extracted.tree.path().join("tool")).unwrap(),
            b"tool"
        );
        assert!(
            extract(
                &input,
                &artifact("tool.xz", "xz", vec![crate::Bin::new("tool")]),
                parent.path(),
                Limits {
                    bytes: 2,
                    entries: 100
                }
            )
            .is_err()
        );
    }
}
