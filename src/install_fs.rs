//! Locked, journaled replacement of an installation and its command exports.
//!
//! The caller authenticates and stages the complete artifact before committing.
//! A receipt is committed with the tree and exports. Recovery pointers in every
//! destination directory let a later command recover even with a different state
//! directory. Ordinary errors roll back; a process interruption leaves the same
//! journal for the next invocation. No operation follows a destination symlink.
use crate::install_extract::Extracted;
use crate::install_policy::Record;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

const POINTER: &str = ".packslip-recovery.json";
const MARKER: &str = ".packslip-owned.json";
const STATE_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("installation I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("installation state: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Conflict(String),
}

/// Security history survives relocation and failed installation attempts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub release: Option<Record>,
    pub list: Option<Record>,
    pub sequence: Option<u64>,
}

/// A command is owned only while its current entry matches this identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Entry {
    Link(PathBuf),
    File(String),
    Directory {
        created: Option<u64>,
        #[cfg(unix)]
        device: u64,
        #[cfg(unix)]
        inode: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u32,
    pub project: String,
    pub version: String,
    pub tree: PathBuf,
    pub exports: BTreeMap<PathBuf, Entry>,
    pub history: History,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    destination: PathBuf,
    slot: PathBuf,
    before: Option<Entry>,
    after: Option<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    committed: bool,
    active: bool,
    scopes: BTreeSet<PathBuf>,
    operations: Vec<Operation>,
}

/// Holds all locks through policy revalidation, staging and commit. Destinations
/// are normalized once; locks are ordered to avoid cross-project deadlocks.
pub struct Session {
    pub state: PathBuf,
    pub tree: PathBuf,
    pub bin: PathBuf,
    scopes: BTreeSet<PathBuf>,
    _locks: Vec<File>,
    #[cfg(windows)]
    _directories: Vec<File>,
}

fn conflict(message: impl Into<String>) -> Error {
    Error::Conflict(message.into())
}

fn regular(path: &Path, write: bool, create: bool) -> Result<File, Error> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags};
        let mut flags = if write { OFlags::RDWR } else { OFlags::RDONLY };
        flags |= OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        if create {
            flags |= OFlags::CREATE;
        }
        File::from(
            rustix::fs::open(path, flags, Mode::RUSR | Mode::WUSR).map_err(std::io::Error::from)?,
        )
    };
    #[cfg(not(unix))]
    let file = {
        let mut options = fs::OpenOptions::new();
        options.read(true).write(write).create(create);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        options.open(path)?
    };
    let meta = file.metadata()?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(conflict(format!(
            "{} is not a regular state file",
            path.display()
        )));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if meta.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(conflict("state files must not be reparse points"));
        }
    }
    Ok(file)
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, Error> {
    let file = match regular(path, false, false) {
        Ok(file) => file,
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let meta = file.metadata()?;
        if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o022 != 0 {
            return Err(conflict(format!(
                "{} has unsafe state-file ownership",
                path.display()
            )));
        }
    }
    #[cfg(windows)]
    windows_access(&file, path, false)?;
    #[cfg(target_os = "macos")]
    macos_acl(path, rustix::process::geteuid().as_raw(), false)?;
    let mut bytes = Vec::new();
    file.take(STATE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > STATE_LIMIT {
        return Err(conflict("installation state exceeds its size limit"));
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn sync_dir(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    // Windows does not expose directory flushing through the safe standard API.
    // Files and journals are flushed before their names are published; process
    // interruption recovery is tested on NTFS. Power-loss guarantees are FS-specific.
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| conflict("state file has no parent"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(staged.as_file_mut(), value)?;
    staged.as_file_mut().write_all(b"\n")?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|e| Error::Io(e.error))?;
    sync_dir(parent)
}

#[cfg(unix)]
fn directory(path: &Path, private: bool) -> Result<PathBuf, Error> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags};
    use std::os::unix::fs::MetadataExt as _;
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let uid = rustix::process::geteuid().as_raw();
    // Traversing a protected ancestor needs search permission, not permission
    // to list it (for example, a 0711 /home).
    #[cfg(target_os = "linux")]
    let access = OFlags::PATH;
    #[cfg(target_os = "macos")]
    let access = OFlags::from_bits_retain(libc::O_SEARCH as _);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let access = OFlags::RDONLY;
    let flags = access | OFlags::DIRECTORY | OFlags::CLOEXEC;
    protected(Path::new("/"), uid, true)?;
    let mut parent =
        File::from(rustix::fs::open("/", flags, Mode::empty()).map_err(std::io::Error::from)?);
    let mut current = PathBuf::from("/");
    for component in absolute.components() {
        let name = match component {
            std::path::Component::RootDir | std::path::Component::CurDir => continue,
            std::path::Component::Normal(name) => name,
            _ => return Err(conflict("installation paths must not contain '..'")),
        };
        current.push(name);
        // Relative mkdir/open bind each component to the already validated
        // directory handle. A raced symlink is never accepted by this open.
        let mut opened = rustix::fs::openat(&parent, name, flags | OFlags::NOFOLLOW, Mode::empty());
        if opened
            .as_ref()
            .is_err_and(|e| *e == rustix::io::Errno::NOENT)
        {
            match rustix::fs::mkdirat(&parent, name, Mode::from_raw_mode(0o755)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(e) => return Err(std::io::Error::from(e).into()),
            }
            opened = rustix::fs::openat(&parent, name, flags | OFlags::NOFOLLOW, Mode::empty());
        }
        let opened = match opened {
            Ok(fd) => fd,
            Err(original) => {
                // macOS /var and common distro /lib paths are trusted symlinks.
                // Check the link's owner and its entire resolved parent chain
                // before following it or creating anything beneath its target.
                let link = rustix::fs::statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(std::io::Error::from)?;
                if FileType::from_raw_mode(link.st_mode) != FileType::Symlink
                    || (link.st_uid != uid && link.st_uid != 0)
                {
                    return Err(std::io::Error::from(original).into());
                }
                protected(&fs::canonicalize(&current)?, uid, true)?;
                rustix::fs::openat(&parent, name, flags, Mode::empty())
                    .map_err(std::io::Error::from)?
            }
        };
        let file = File::from(opened);
        let meta = file.metadata()?;
        #[cfg(target_os = "macos")]
        macos_acl(&current, uid, private && current == absolute)?;
        if (meta.uid() != uid && meta.uid() != 0)
            || (meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0)
        {
            return Err(conflict(format!(
                "{} has unsafe directory ownership or permissions",
                current.display()
            )));
        }
        parent = file;
    }
    let canonical = fs::canonicalize(&current)?;
    protected(&canonical, uid, false)?;
    if private {
        if parent.metadata()?.uid() != uid {
            return Err(conflict("state directory belongs to another user"));
        }
        rustix::fs::chmodat(
            &parent,
            ".",
            Mode::RUSR | Mode::WUSR | Mode::XUSR,
            AtFlags::empty(),
        )
        .map_err(std::io::Error::from)?;
    }
    Ok(canonical)
}

#[cfg(unix)]
fn protected(path: &Path, uid: u32, allow_sticky: bool) -> Result<(), Error> {
    use std::os::unix::fs::MetadataExt as _;
    for ancestor in path.ancestors() {
        let meta = fs::metadata(ancestor)?;
        #[cfg(target_os = "macos")]
        macos_acl(ancestor, uid, false)?;
        if (meta.uid() != uid && meta.uid() != 0)
            || (meta.mode() & 0o022 != 0
                && ((ancestor == path && !allow_sticky) || meta.mode() & 0o1000 == 0))
        {
            return Err(conflict(format!(
                "{} is writable by another user",
                ancestor.display()
            )));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_acl(path: &Path, uid: u32, private: bool) -> Result<(), Error> {
    use exacl::{AclEntryKind, Perm};
    let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .map_err(std::io::Error::from)?
        .ok_or_else(|| conflict("cannot resolve the installation user"))?;
    let writing = Perm::WRITE
        | Perm::APPEND
        | Perm::DELETE
        | Perm::DELETE_CHILD
        | Perm::WRITEATTR
        | Perm::WRITEEXTATTR
        | Perm::WRITESECURITY
        | Perm::CHOWN;
    let prohibited = writing | if private { Perm::all() } else { Perm::empty() };
    for ace in exacl::getfacl(path, None)? {
        // Check inherited grants too: they must not make newly created state
        // or staging files writable by another principal.
        if ace.allow
            && ace.perms.intersects(prohibited)
            && !(ace.kind == AclEntryKind::User
                && (ace.name == user.name
                    || ace.name == "root"
                    || ace.name == uid.to_string()
                    || ace.name == "0"))
        {
            return Err(conflict(format!(
                "{} has an unsafe extended ACL",
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_access(file: &File, path: &Path, ancestor: bool) -> Result<(), Error> {
    use windows_permissions::{
        LocalBox, Sid,
        constants::{AceFlags, AceType, SeObjectType, SecurityInformation},
        wrappers,
    };
    let user = windows_permissions::utilities::current_process_sid()?;
    let trusted: Vec<LocalBox<Sid>> = [
        // SYSTEM, Administrators and the Windows servicing account.
        "S-1-5-18",
        "S-1-5-32-544",
        "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
    ]
    .into_iter()
    .map(str::parse)
    .collect::<Result<_, _>>()?;
    let safe = |sid: &Sid| sid == &*user || trusted.iter().any(|t| sid == &**t);
    let sd = wrappers::GetSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )?;
    if sd.owner().is_none_or(|owner| !safe(owner)) {
        return Err(conflict(format!(
            "{} belongs to another user",
            path.display()
        )));
    }
    let acl = sd
        .dacl()
        .ok_or_else(|| conflict("installation paths must have a DACL"))?;
    // Generic write/all, delete, ACL/owner changes, data/EA/attribute writes,
    // and deletion of children. Only ancestor ADD_SUBDIRECTORY is harmless:
    // it permits creation, but cannot replace an already protected child.
    let writing = 0x500d_0152u32 | if ancestor { 0 } else { 0x4 };
    for i in 0..acl.len() {
        let ace = acl
            .get_ace(i)
            .ok_or_else(|| conflict("invalid directory ACL"))?;
        if ace.flags().contains(AceFlags::InheritOnly) {
            continue;
        }
        match ace.ace_type() {
            AceType::ACCESS_DENIED_ACE_TYPE
            | AceType::ACCESS_DENIED_OBJECT_ACE_TYPE
            | AceType::ACCESS_DENIED_CALLBACK_ACE_TYPE
            | AceType::ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE => continue,
            AceType::ACCESS_ALLOWED_ACE_TYPE
            | AceType::ACCESS_ALLOWED_OBJECT_ACE_TYPE
            | AceType::ACCESS_ALLOWED_CALLBACK_ACE_TYPE
            | AceType::ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE => {
                if ace.mask().bits() & writing != 0 && ace.sid().is_none_or(|sid| !safe(sid)) {
                    return Err(conflict(format!(
                        "{} is writable by another user",
                        path.display()
                    )));
                }
            }
            _ => return Err(conflict("unsupported installation directory ACL")),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_directory(path: &Path, ancestor: bool, private: bool) -> Result<File, Error> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    use windows_sys::Win32::Storage::FileSystem::*;
    let file = fs::OpenOptions::new()
        .access_mode(0x0002_0080 | if private { 0x0004_0000 } else { 0 }) // READ_CONTROL, READ_ATTRIBUTES, optional WRITE_DAC
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE) // hold directory identity; deny delete/rename
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_dir() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(conflict(format!(
            "{} is not an ordinary installation directory",
            path.display()
        )));
    }
    windows_access(&file, path, ancestor)?;
    if private {
        use windows_permissions::{
            LocalBox, SecurityDescriptor,
            constants::{SeObjectType, SecurityInformation},
            wrappers,
        };
        let sid = windows_permissions::utilities::current_process_sid()?;
        let sd: LocalBox<SecurityDescriptor> =
            format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)").parse()?;
        let mut file = file;
        wrappers::SetSecurityInfo(
            &mut file,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            sd.dacl(),
            None,
        )?;
        return Ok(file);
    }
    Ok(file)
}

#[cfg(windows)]
fn directory(path: &Path, private: bool) -> Result<PathBuf, Error> {
    // Validate each existing component before creating beneath it. In particular,
    // root must not follow an attacker's pre-created symlink in a sticky directory.
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut current = PathBuf::new();
    let mut guards = Vec::new();
    let components: Vec<_> = absolute.components().collect();
    for (index, component) in components.iter().enumerate() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(conflict("installation paths must not contain '..'"));
        }
        current.push(component.as_os_str());
        // A drive/UNC prefix by itself is not an absolute filesystem object.
        // In particular, verbatim "\\\\?\\C:" cannot be queried until its root
        // separator has been appended by the next component.
        #[cfg(windows)]
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        let ancestor = index + 1 != components.len();
        let file = match windows_directory(&current, ancestor, private && !ancestor) {
            Ok(file) => file,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
                windows_directory(&current, ancestor, private && !ancestor)?
            }
            Err(e) => return Err(e),
        };
        guards.push(file);
    }
    Ok(fs::canonicalize(&current)?)
}

fn entry(path: &Path) -> Result<Option<Entry>, Error> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(Some(if meta.file_type().is_symlink() {
        Entry::Link(fs::read_link(path)?)
    } else if meta.is_file() {
        let mut file = regular(path, false, false)?;
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            hash.update(&bytes[..n]);
        }
        Entry::File(hex::encode(hash.finalize()))
    } else if meta.is_dir() {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt as _;
        Entry::Directory {
            created: meta
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|t| u64::try_from(t.as_nanos()).ok()),
            #[cfg(unix)]
            device: meta.dev(),
            #[cfg(unix)]
            inode: meta.ino(),
        }
    } else {
        return Err(conflict(format!(
            "{} is a special filesystem entry",
            path.display()
        )));
    }))
}

fn remove(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => fs::remove_dir_all(path)?,
        #[cfg(windows)]
        Ok(m)
            if {
                use std::os::windows::fs::MetadataExt as _;
                m.file_attributes()
                    & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY
                    != 0
            } =>
        {
            fs::remove_dir(path)?
        }
        Ok(_) => fs::remove_file(path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    sync_dir(
        path.parent()
            .ok_or_else(|| conflict("entry has no parent"))?,
    )
}

fn rename(from: &Path, to: &Path) -> Result<(), Error> {
    fs::rename(from, to)?;
    sync_dir(
        from.parent()
            .ok_or_else(|| conflict("source has no parent"))?,
    )?;
    sync_dir(
        to.parent()
            .ok_or_else(|| conflict("destination has no parent"))?,
    )
}

fn validate(journal: &Journal) -> Result<(), Error> {
    if journal.schema != 1 || journal.operations.is_empty() || journal.operations.len() > 10_000 {
        return Err(conflict("unsupported or empty recovery journal"));
    }
    let mut destinations = BTreeSet::new();
    for op in &journal.operations {
        let parent = op
            .destination
            .parent()
            .ok_or_else(|| conflict("journal destination has no parent"))?;
        if !journal.scopes.contains(parent)
            || op.slot.parent() != Some(parent)
            || !op
                .slot
                .file_name()
                .is_some_and(|s| s.to_string_lossy().starts_with(".packslip-txn-"))
            || !destinations.insert(&op.destination)
            || op
                .destination
                .file_name()
                .is_none_or(|s| s == POINTER || s == ".packslip.lock")
        {
            return Err(conflict("invalid recovery journal paths"));
        }
        if fs::symlink_metadata(&op.slot).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir()) {
            return Err(conflict("recovery staging directory was replaced"));
        }
    }
    Ok(())
}

fn recover(path: &Path, journal: &Journal) -> Result<(), Error> {
    validate(journal)?;
    if journal.active && !journal.committed {
        for op in journal.operations.iter().rev() {
            let backup = op.slot.join("backup");
            let staged = op.slot.join("new");
            let current = entry(&op.destination)?;
            let saved = entry(&backup)?;
            if saved.is_some() {
                if saved != op.before {
                    return Err(conflict("recovery backup was modified"));
                }
                if current.is_some() {
                    if current != op.after || entry(&staged)?.is_some() {
                        return Err(conflict(format!(
                            "{} changed during recovery",
                            op.destination.display()
                        )));
                    }
                    // Move the installed new entry back to its empty staging slot.
                    // The rollback remains idempotent if interrupted again here.
                    rename(&op.destination, &staged)?;
                }
                rename(&backup, &op.destination)?;
            } else if op.before.is_none() && current.is_some() {
                if current != op.after || entry(&staged)?.is_some() {
                    return Err(conflict("new installation entry changed during recovery"));
                }
                rename(&op.destination, &staged)?;
            } else if current != op.before {
                return Err(conflict("prior installation entry changed during recovery"));
            }
        }
    }
    // Rollback has finished: subsequent cleanup must never revisit destinations.
    if !journal.committed {
        let mut cleaned = journal.clone();
        cleaned.committed = true;
        atomic(path, &cleaned)?;
    }
    // Remove pointers before the journal. An interrupted cleanup can find a
    // committed journal, but never a pointer to a journal already deleted.
    for scope in &journal.scopes {
        let pointer = scope.join(POINTER);
        if read::<PathBuf>(&pointer)?.as_deref() == Some(path) {
            remove(&pointer)?;
        }
    }
    // The journal stays until every slot is removed. Recovery of a committed
    // transaction does not touch its published destinations.
    for op in &journal.operations {
        remove(&op.slot)?;
    }
    remove(path)
}

impl Session {
    pub fn open(state: &Path, tree: &Path, bin: &Path) -> Result<Self, Error> {
        Self::open_under(state, tree, bin, None)
    }

    /// Include still-owned previous command directories when relocating exports.
    pub fn open_for(state: &Path, tree: &Path, bin: &Path, project: &str) -> Result<Self, Error> {
        Self::open_under(state, tree, bin, Some(project))
    }

    fn open_under(
        state: &Path,
        tree: &Path,
        bin: &Path,
        project: Option<&str>,
    ) -> Result<Self, Error> {
        let state = directory(state, true)?;
        let parent = directory(
            tree.parent()
                .ok_or_else(|| conflict("installation needs a parent directory"))?,
            false,
        )?;
        let name = tree
            .file_name()
            .ok_or_else(|| conflict("installation cannot replace a filesystem root"))?;
        if name
            .to_string_lossy()
            .to_ascii_lowercase()
            .starts_with(".packslip")
        {
            return Err(conflict("installation uses a reserved path"));
        }
        let tree = parent.join(name);
        let bin = directory(bin, false)?;
        if tree == state || bin.starts_with(&tree) || state.starts_with(&tree) {
            return Err(conflict(
                "installation, command, and state directories must not overlap",
            ));
        }
        let mut scopes = BTreeSet::from([state.clone(), parent, bin.clone()]);
        loop {
            if scopes.len() > 128 {
                return Err(conflict("too many shared recovery directories"));
            }
            let mut locks = Vec::new();
            #[cfg(windows)]
            let mut directories = Vec::new();
            for scope in &scopes {
                directory(scope, false)?;
                #[cfg(windows)]
                for ancestor in scope.ancestors() {
                    directories.push(windows_directory(ancestor, ancestor != scope, false)?);
                }
                let lock = regular(&scope.join(".packslip.lock"), true, true)?;
                #[cfg(windows)]
                windows_access(&lock, &scope.join(".packslip.lock"), false)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt as _;
                    let meta = lock.metadata()?;
                    #[cfg(target_os = "macos")]
                    macos_acl(
                        &scope.join(".packslip.lock"),
                        rustix::process::geteuid().as_raw(),
                        false,
                    )?;
                    if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o022 != 0
                    {
                        return Err(conflict("destination lock has unsafe ownership"));
                    }
                }
                lock.lock()?;
                locks.push(lock);
            }
            let mut journals = BTreeMap::new();
            let mut extra = BTreeSet::new();
            if let Some(project) = project {
                let receipt_path = state.join(format!(
                    "receipt-{}.json",
                    hex::encode(Sha256::digest(project))
                ));
                if let Some(receipt) = read::<Receipt>(&receipt_path)? {
                    if receipt.schema != 1 || receipt.project != project {
                        return Err(conflict("invalid prior installation receipt"));
                    }
                    for path in receipt.exports.keys() {
                        let parent = path
                            .parent()
                            .ok_or_else(|| conflict("prior export has no parent"))?;
                        extra.insert(directory(parent, false)?);
                    }
                }
            }
            // A journal with no pointers is still recoverable from its own state.
            let own = state.join("journal.json");
            if let Some(journal) = read::<Journal>(&own)? {
                extra.extend(journal.scopes.iter().cloned());
                journals.insert(own, journal);
            }
            for scope in &scopes {
                if let Some(path) = read::<PathBuf>(&scope.join(POINTER))? {
                    let journal: Journal =
                        read(&path)?.ok_or_else(|| conflict("recovery pointer has no journal"))?;
                    if !journal.scopes.contains(scope)
                        || path.file_name().is_none_or(|s| s != "journal.json")
                    {
                        return Err(conflict("recovery pointer does not match its destination"));
                    }
                    validate(&journal)?;
                    extra.extend(journal.scopes.iter().cloned());
                    extra.insert(
                        path.parent()
                            .ok_or_else(|| conflict("journal has no parent"))?
                            .to_owned(),
                    );
                    journals.insert(path, journal);
                }
            }
            if !extra.is_subset(&scopes) {
                scopes.extend(extra);
                drop(locks);
                continue;
            }
            for (path, journal) in journals {
                recover(&path, &journal)?;
            }
            return Ok(Self {
                state,
                tree,
                bin,
                scopes,
                _locks: locks,
                #[cfg(windows)]
                _directories: directories,
            });
        }
    }

    fn receipt_path(&self, project: &str) -> PathBuf {
        self.state.join(format!(
            "receipt-{}.json",
            hex::encode(Sha256::digest(project))
        ))
    }

    pub fn receipt(&self, project: &str) -> Result<Option<Receipt>, Error> {
        let receipt: Option<Receipt> = read(&self.receipt_path(project))?;
        if receipt
            .as_ref()
            .is_some_and(|r| r.schema != 1 || r.project != project)
        {
            return Err(conflict(
                "receipt has an unsupported schema or a different project",
            ));
        }
        Ok(receipt)
    }

    pub fn history(&self, project: &str) -> Result<History, Error> {
        let path = self.state.join(format!(
            "history-{}.json",
            hex::encode(Sha256::digest(project))
        ));
        Ok(read(&path)?.unwrap_or_default())
    }

    /// Persist accepted list history before artifact work, so a later failed
    /// install cannot make an older signed list acceptable again.
    pub fn remember(&self, project: &str, history: &History) -> Result<(), Error> {
        let path = self.state.join(format!(
            "history-{}.json",
            hex::encode(Sha256::digest(project))
        ));
        atomic(&path, history)
    }

    /// `make_export` writes one native launcher or link at the private staging
    /// path, pointing at an absolute executable inside the complete final tree.
    pub fn commit<F>(
        &self,
        extracted: Extracted,
        project: &str,
        version: &str,
        force: bool,
        history: History,
        make_export: F,
    ) -> Result<Receipt, Error>
    where
        F: Fn(&Path, &Path) -> Result<(), Error>,
    {
        self.commit_inner(
            extracted,
            project,
            version,
            force,
            history,
            make_export,
            |_| Ok(()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_inner<F, H>(
        &self,
        extracted: Extracted,
        project: &str,
        version: &str,
        force: bool,
        history: History,
        make_export: F,
        mut boundary: H,
    ) -> Result<Receipt, Error>
    where
        F: Fn(&Path, &Path) -> Result<(), Error>,
        H: FnMut(usize) -> Result<(), Error>,
    {
        let previous = self.receipt(project)?;
        if entry(&self.tree)?.is_some() && !force {
            let owned = matches!(entry(&self.tree)?, Some(Entry::Directory { .. }))
                && read::<String>(&self.tree.join(MARKER))?.as_deref() == Some(project);
            if !owned {
                return Err(conflict(format!(
                    "{} is unmarked or belongs to another project; use --force to replace it",
                    self.tree.display()
                )));
            }
        }
        let mut slots = Vec::new();
        let mut operations = Vec::new();
        let mut prepare = |destination: PathBuf, source: Option<&Path>| -> Result<(), Error> {
            let slot = tempfile::Builder::new()
                .prefix(".packslip-txn-")
                .tempdir_in(
                    destination
                        .parent()
                        .ok_or_else(|| conflict("entry has no parent"))?,
                )?;
            let before = entry(&destination)?;
            if let Some(source) = source {
                rename(source, &slot.path().join("new"))?;
            }
            let after = entry(&slot.path().join("new"))?;
            operations.push(Operation {
                destination,
                slot: slot.path().to_owned(),
                before,
                after,
            });
            slots.push(slot);
            Ok(())
        };
        atomic(&extracted.tree.path().join(MARKER), &project)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(extracted.tree.path(), fs::Permissions::from_mode(0o755))?;
        }
        sync_tree(extracted.tree.path())?;
        let stage = extracted.tree.keep();
        if let Err(e) = prepare(self.tree.clone(), Some(&stage)) {
            remove(&stage)?;
            return Err(e);
        }
        let mut exports = BTreeMap::new();
        let mut folded = BTreeSet::new();
        for (name, relative) in extracted.bins {
            let filename = export_name(&name)?;
            if !folded.insert(filename.to_lowercase()) {
                return Err(conflict("duplicate exported command name"));
            }
            let destination = self.bin.join(&filename);
            let existing = entry(&destination)?;
            let owned = previous.as_ref().and_then(|r| r.exports.get(&destination));
            if existing.is_some() && existing.as_ref() != owned && !force {
                return Err(conflict(format!(
                    "{} conflicts with an existing command; use --force to replace it",
                    destination.display()
                )));
            }
            if !relative.is_relative()
                || relative
                    .components()
                    .any(|p| !matches!(p, std::path::Component::Normal(_)))
            {
                return Err(conflict("command path escapes the installation"));
            }
            let temp = tempfile::Builder::new()
                .prefix(".packslip-export-")
                .tempdir_in(&self.bin)?;
            let path = temp.path().join("command");
            make_export(&path, &self.tree.join(relative))?;
            let identity =
                entry(&path)?.ok_or_else(|| conflict("export builder did not write a command"))?;
            if !matches!(identity, Entry::Link(_) | Entry::File(_)) {
                return Err(conflict("export is not a link or launcher"));
            }
            exports.insert(destination.clone(), identity);
            prepare(destination, Some(&path))?;
        }
        if let Some(previous) = previous {
            for (path, owned) in previous.exports {
                if path.parent().is_some_and(|p| self.scopes.contains(p))
                    && !exports.contains_key(&path)
                    && entry(&path)?.as_ref() == Some(&owned)
                {
                    prepare(path, None)?;
                }
            }
        }
        let receipt = Receipt {
            schema: 1,
            project: project.to_owned(),
            version: version.to_owned(),
            tree: self.tree.clone(),
            exports,
            history: history.clone(),
        };
        let temp = tempfile::tempdir_in(&self.state)?;
        let receipt_stage = temp.path().join("receipt");
        atomic(&receipt_stage, &receipt)?;
        prepare(self.receipt_path(project), Some(&receipt_stage))?;
        let history_stage = temp.path().join("history");
        atomic(&history_stage, &history)?;
        prepare(
            self.state.join(format!(
                "history-{}.json",
                hex::encode(Sha256::digest(project))
            )),
            Some(&history_stage),
        )?;
        let mut journal = Journal {
            schema: 1,
            committed: false,
            active: false,
            scopes: self.scopes.clone(),
            operations,
        };
        validate(&journal)?;
        let path = self.state.join("journal.json");
        atomic(&path, &journal)?;
        // From here onward, journal recovery owns staging cleanup, including on
        // interruption. No TempDir guard may discard a backup needed to recover.
        for slot in slots {
            let _ = slot.keep();
        }
        let result = (|| {
            for scope in &self.scopes {
                atomic(&scope.join(POINTER), &path)?;
            }
            journal.active = true;
            atomic(&path, &journal)?;
            boundary(0)?;
            for (index, op) in journal.operations.iter().enumerate() {
                if entry(&op.destination)? != op.before {
                    return Err(conflict("destination changed after preparation"));
                }
                if op.before.is_some() {
                    rename(&op.destination, &op.slot.join("backup"))?;
                }
                boundary(index * 2 + 1)?;
                if op.after.is_some() {
                    rename(&op.slot.join("new"), &op.destination)?;
                }
                boundary(index * 2 + 2)?;
            }
            let mut committed = journal.clone();
            committed.committed = true;
            atomic(&path, &committed)?;
            journal = committed;
            boundary(journal.operations.len() * 2 + 1)?;
            Ok(())
        })();
        if let Err(e) = result {
            // Simulated interruption tests terminate the subprocess in the hook;
            // ordinary I/O errors instead perform the same idempotent recovery.
            // Publication can succeed before its directory flush reports an
            // error. Recover from the record actually published, never from a
            // proposed in-memory phase or a guess about how far atomic got.
            let persisted: Journal =
                read(&path)?.ok_or_else(|| conflict("transaction journal disappeared"))?;
            recover(&path, &persisted)?;
            return Err(e);
        }
        recover(&path, &journal)?;
        Ok(receipt)
    }
}

fn export_name(name: &str) -> Result<String, Error> {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\', ':', '\0'])
        || name.ends_with([' ', '.'])
        || name == ".."
    {
        return Err(conflict("unsafe exported command name"));
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&base.as_str())
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|s| {
                (s.len() == 1 && s.as_bytes()[0].is_ascii_digit()) || ["¹", "²", "³"].contains(&s)
            })
    {
        return Err(conflict("reserved exported command name"));
    }
    #[cfg(windows)]
    return Ok(format!("{name}.exe"));
    #[cfg(not(windows))]
    Ok(name.to_owned())
}

fn sync_tree(path: &Path) -> Result<(), Error> {
    for item in fs::read_dir(path)? {
        let item = item?;
        let kind = item.file_type()?;
        if kind.is_dir() {
            sync_tree(&item.path())?;
        } else if kind.is_file() {
            // Windows FlushFileBuffers requires a handle opened for writing;
            // the extraction stage contains only our newly written files.
            #[cfg(windows)]
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(item.path())?
                .sync_all()?;
            #[cfg(not(windows))]
            File::open(item.path())?.sync_all()?;
        }
    }
    sync_dir(path)
}

#[cfg(unix)]
pub fn symlink_export(path: &Path, target: &Path) -> Result<(), Error> {
    std::os::unix::fs::symlink(target, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn traverses_search_only_ancestors() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let prefix = root.path().join("prefix");
        fs::create_dir_all(prefix.join("inner")).unwrap();
        fs::set_permissions(&prefix, fs::Permissions::from_mode(0o111)).unwrap();
        let result = directory(&prefix.join("inner/state"), true);
        fs::set_permissions(&prefix, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_ok(), "{result:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rejects_extended_acl_writers_even_with_private_mode_bits() {
        use exacl::{AclEntry, Perm};
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let unsafe_dir = root.path().join("unsafe");
        fs::create_dir(&unsafe_dir).unwrap();
        fs::set_permissions(&unsafe_dir, fs::Permissions::from_mode(0o700)).unwrap();
        exacl::setfacl(
            &[&unsafe_dir],
            &[AclEntry::allow_group(
                "everyone",
                Perm::WRITE | Perm::DELETE_CHILD,
                None,
            )],
            None,
        )
        .unwrap();
        let result = directory(&unsafe_dir.join("new-state"), true);
        exacl::setfacl(&[&unsafe_dir], &[], None).unwrap();
        assert!(result.is_err());
        assert!(!unsafe_dir.join("new-state").exists());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_foreign_acl_writers_before_creating_children() {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_permissions::{
            LocalBox, SecurityDescriptor,
            constants::{SeObjectType, SecurityInformation},
            wrappers,
        };
        let root = tempfile::tempdir().unwrap();
        let unsafe_dir = root.path().join("unsafe");
        fs::create_dir(&unsafe_dir).unwrap();
        let sid = windows_permissions::utilities::current_process_sid().unwrap();
        let mut handle = fs::OpenOptions::new()
            .access_mode(0x0006_0080)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
            .open(&unsafe_dir)
            .unwrap();
        let set = |handle: &mut File, text: &str| {
            let sd: LocalBox<SecurityDescriptor> = text.parse().unwrap();
            wrappers::SetSecurityInfo(
                handle,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                None,
                None,
                sd.dacl(),
                None,
            )
            .unwrap();
        };
        set(
            &mut handle,
            &format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;WD)"),
        );
        let result = directory(&unsafe_dir.join("new-state"), true);
        set(
            &mut handle,
            &format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"),
        );
        drop(handle);
        assert!(result.is_err());
        assert!(!unsafe_dir.join("new-state").exists());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_junctions_without_creating_children_at_the_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = root.path().join("junction");
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(directory(&link.join("state"), true).is_err());
        assert!(!target.join("state").exists());
        fs::remove_dir(&link).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_trusted_symlink_to_an_unprotected_target_before_creating_children() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let unsafe_target = root.path().join("unsafe");
        fs::create_dir(&unsafe_target).unwrap();
        fs::set_permissions(&unsafe_target, fs::Permissions::from_mode(0o777)).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&unsafe_target, &link).unwrap();
        assert!(directory(&link.join("must-not-be-created"), false).is_err());
        assert!(!unsafe_target.join("must-not-be-created").exists());
        fs::set_permissions(unsafe_target, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn staged(root: &Path, commands: &[&str]) -> Extracted {
        let tree = tempfile::tempdir_in(root).unwrap();
        let bins = commands
            .iter()
            .map(|name| {
                fs::write(tree.path().join(name), b"new command").unwrap();
                ((*name).to_owned(), PathBuf::from(name))
            })
            .collect();
        fs::create_dir(tree.path().join("runtime")).unwrap();
        fs::write(tree.path().join("runtime/data"), b"runtime dependency").unwrap();
        Extracted { tree, bins }
    }

    fn export(path: &Path, target: &Path) -> Result<(), Error> {
        #[cfg(unix)]
        return symlink_export(path, target);
        #[cfg(not(unix))]
        {
            fs::write(path, target.to_string_lossy().as_bytes())?;
            Ok(())
        }
    }

    #[test]
    fn replacement_and_obsolete_exports_respect_ownership() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::open(
            &root.path().join("state"),
            &root.path().join("tree/tool"),
            &root.path().join("bin"),
        )
        .unwrap();
        let first = session
            .commit(
                staged(session.tree.parent().unwrap(), &["tool", "extra", "keep"]),
                "example.test/tool",
                "1",
                false,
                History::default(),
                export,
            )
            .unwrap();
        let extra = session.bin.join(export_name("extra").unwrap());
        let keep = session.bin.join(export_name("keep").unwrap());
        remove(&keep).unwrap();
        fs::write(&keep, b"another owner").unwrap();
        session
            .commit(
                staged(session.tree.parent().unwrap(), &["tool"]),
                "example.test/tool",
                "2",
                false,
                History::default(),
                export,
            )
            .unwrap();
        assert!(entry(&extra).unwrap().is_none());
        assert_eq!(fs::read(&keep).unwrap(), b"another owner");
        assert_eq!(
            fs::read(session.tree.join("runtime/data")).unwrap(),
            b"runtime dependency"
        );
        assert_eq!(first.exports.len(), 3);
        assert_eq!(
            session
                .receipt("example.test/tool")
                .unwrap()
                .unwrap()
                .version,
            "2"
        );
    }

    #[test]
    fn force_replaces_unmarked_tree_and_command_entry() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::open(
            &root.path().join("state"),
            &root.path().join("tree/tool"),
            &root.path().join("bin"),
        )
        .unwrap();
        fs::create_dir(&session.tree).unwrap();
        fs::write(session.tree.join("prior"), b"prior installation").unwrap();
        let command = session.bin.join(export_name("tool").unwrap());
        fs::write(&command, b"prior command").unwrap();
        assert!(
            session
                .commit(
                    staged(session.tree.parent().unwrap(), &["tool"]),
                    "example.test/tool",
                    "1",
                    false,
                    History::default(),
                    export
                )
                .is_err()
        );
        assert!(session.tree.join("prior").exists());
        session
            .commit(
                staged(session.tree.parent().unwrap(), &["tool"]),
                "example.test/tool",
                "1",
                true,
                History::default(),
                export,
            )
            .unwrap();
        assert!(!session.tree.join("prior").exists());
    }

    #[cfg(unix)]
    #[test]
    fn force_replaces_symlinks_without_touching_their_targets() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::open(
            &root.path().join("state"),
            &root.path().join("tree/tool"),
            &root.path().join("bin"),
        )
        .unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("valuable"), b"untouched").unwrap();
        std::os::unix::fs::symlink(&outside, &session.tree).unwrap();
        std::os::unix::fs::symlink(outside.join("valuable"), session.bin.join("tool")).unwrap();
        session
            .commit(
                staged(session.tree.parent().unwrap(), &["tool"]),
                "example.test/tool",
                "1",
                true,
                History::default(),
                export,
            )
            .unwrap();
        assert_eq!(fs::read(outside.join("valuable")).unwrap(), b"untouched");
    }

    #[test]
    fn interruption_child() {
        let Some(root) = std::env::var_os("PACKSLIP_TEST_TRANSACTION_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let boundary: usize = std::env::var("PACKSLIP_TEST_TRANSACTION_BOUNDARY")
            .unwrap()
            .parse()
            .unwrap();
        let session = Session::open(
            &root.join("state"),
            &root.join("tree/tool"),
            &root.join("bin"),
        )
        .unwrap();
        let result = session.commit_inner(
            staged(session.tree.parent().unwrap(), &["tool"]),
            "example.test/tool",
            "2",
            true,
            History::default(),
            export,
            |step| {
                if step == boundary {
                    std::process::exit(73);
                }
                Ok(())
            },
        );
        panic!("interruption point was not reached: {result:?}");
    }

    #[test]
    fn ordinary_failure_rolls_back_tree_commands_and_receipt() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::open(
            &root.path().join("state"),
            &root.path().join("tree/tool"),
            &root.path().join("bin"),
        )
        .unwrap();
        session
            .commit(
                staged(session.tree.parent().unwrap(), &["old"]),
                "example.test/tool",
                "1",
                false,
                History::default(),
                export,
            )
            .unwrap();
        let result = session.commit_inner(
            staged(session.tree.parent().unwrap(), &["new"]),
            "example.test/tool",
            "2",
            false,
            History::default(),
            export,
            |step| {
                if step == 4 {
                    Err(std::io::Error::other("injected publish failure").into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert!(session.tree.join("old").is_file());
        assert!(!session.tree.join("new").exists());
        assert!(
            entry(&session.bin.join(export_name("old").unwrap()))
                .unwrap()
                .is_some()
        );
        assert!(
            entry(&session.bin.join(export_name("new").unwrap()))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            session
                .receipt("example.test/tool")
                .unwrap()
                .unwrap()
                .version,
            "1"
        );
        assert!(!session.state.join("journal.json").exists());
    }

    #[cfg(windows)]
    #[test]
    fn failed_commit_record_keeps_the_prior_installation_recoverable() {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let tree = root.path().join("tree/tool");
        let bin = root.path().join("bin");
        let session = Session::open(&state, &tree, &bin).unwrap();
        session
            .commit(
                staged(session.tree.parent().unwrap(), &["old"]),
                "example.test/tool",
                "1",
                false,
                History::default(),
                export,
            )
            .unwrap();
        let mut deny_delete = None;
        let result = session.commit_inner(
            staged(session.tree.parent().unwrap(), &["new"]),
            "example.test/tool",
            "2",
            false,
            History::default(),
            export,
            |step| {
                if step == 0 {
                    // A real sharing violation prevents atomic publication of
                    // the commit record, while leaving the destinations writable.
                    deny_delete = Some(
                        fs::OpenOptions::new()
                            .read(true)
                            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                            .open(session.state.join("journal.json"))?,
                    );
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(session.tree.join("old").is_file());
        assert!(!session.tree.join("new").exists());
        assert_eq!(
            session
                .receipt("example.test/tool")
                .unwrap()
                .unwrap()
                .version,
            "1"
        );
        drop(deny_delete);
        drop(session);
        let session = Session::open(&state, &tree, &bin).unwrap();
        assert_eq!(
            session
                .receipt("example.test/tool")
                .unwrap()
                .unwrap()
                .version,
            "1"
        );
        assert!(!session.state.join("journal.json").exists());
    }

    #[test]
    fn relocating_commands_removes_only_still_owned_previous_exports() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let tree = root.path().join("tree/tool");
        let old_bin = root.path().join("old-bin");
        {
            let session = Session::open(&state, &tree, &old_bin).unwrap();
            session
                .commit(
                    staged(session.tree.parent().unwrap(), &["tool", "keep"]),
                    "example.test/tool",
                    "1",
                    false,
                    History::default(),
                    export,
                )
                .unwrap();
            let keep = session.bin.join(export_name("keep").unwrap());
            remove(&keep).unwrap();
            fs::write(keep, b"another owner").unwrap();
        }
        let session = Session::open_for(
            &state,
            &tree,
            &root.path().join("new-bin"),
            "example.test/tool",
        )
        .unwrap();
        session
            .commit(
                staged(session.tree.parent().unwrap(), &["tool"]),
                "example.test/tool",
                "2",
                false,
                History::default(),
                export,
            )
            .unwrap();
        assert!(
            entry(&old_bin.join(export_name("tool").unwrap()))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fs::read(old_bin.join(export_name("keep").unwrap())).unwrap(),
            b"another owner"
        );
        assert!(
            entry(&session.bin.join(export_name("tool").unwrap()))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn shared_lock_child() {
        let Some(root) = std::env::var_os("PACKSLIP_TEST_LOCK_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let _session = Session::open(
            &root.join("other-state"),
            &root.join("tree/other"),
            &root.join("bin"),
        )
        .unwrap();
        fs::write(root.join("acquired"), b"locked").unwrap();
    }

    #[test]
    fn destination_locks_serialize_different_state_directories() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::open(
            &root.path().join("state"),
            &root.path().join("tree/tool"),
            &root.path().join("bin"),
        )
        .unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "install_fs::tests::shared_lock_child"])
            .env("PACKSLIP_TEST_LOCK_ROOT", root.path())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!root.path().join("acquired").exists());
        drop(session);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if std::time::Instant::now() > deadline {
                child.kill().unwrap();
                panic!("shared destination lock did not release");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(root.path().join("acquired").exists());
    }

    #[test]
    fn every_mutation_boundary_recovers_across_state_directories() {
        // Tree, command, receipt and history each have backup/publish boundaries,
        // plus prepared and committed boundaries. Exit skips every destructor.
        for boundary in 0..=9 {
            let root = tempfile::tempdir().unwrap();
            {
                let session = Session::open(
                    &root.path().join("state"),
                    &root.path().join("tree/tool"),
                    &root.path().join("bin"),
                )
                .unwrap();
                session
                    .commit(
                        staged(session.tree.parent().unwrap(), &["tool"]),
                        "example.test/tool",
                        "1",
                        false,
                        History::default(),
                        export,
                    )
                    .unwrap();
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "install_fs::tests::interruption_child",
                    "--nocapture",
                ])
                .env("PACKSLIP_TEST_TRANSACTION_ROOT", root.path())
                .env("PACKSLIP_TEST_TRANSACTION_BOUNDARY", boundary.to_string())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(73));
            // A different state directory sees and recovers the original journal
            // through the shared destination pointers, then releases its locks.
            drop(
                Session::open(
                    &root.path().join("other-state"),
                    &root.path().join("tree/tool"),
                    &root.path().join("bin"),
                )
                .unwrap(),
            );
            let session = Session::open(
                &root.path().join("state"),
                &root.path().join("tree/tool"),
                &root.path().join("bin"),
            )
            .unwrap();
            let receipt = session.receipt("example.test/tool").unwrap().unwrap();
            assert_eq!(
                receipt.version,
                if boundary == 9 { "2" } else { "1" },
                "boundary {boundary}"
            );
            assert!(!session.state.join("journal.json").exists());
            assert!(!session.bin.join(POINTER).exists());
        }
    }
}
