//! Creating the statements: a release from its artifact files, and a
//! release list from released packslips. Signing is `sigstore::sign`.

use std::path::Path;

use crate::model::{
    Artifact, Attestor, Bin, Digest, Envelope, Evidence, Extensions, Identity, PREDICATE_TYPE,
    Predicate, RELEASES_PREDICATE_TYPE, ReleaseList, ReleaseListStatement, ReleaseRef, RequiredBin,
    Requires, Resource, STATEMENT_TYPE, Source, Statement, Subject,
};

/// What `create` needs.
pub struct Request<'a> {
    pub project: &'a str,
    pub version: &'a str,
    pub published_at: Option<&'a str>,
    pub source: Option<Source>,
    pub artifacts: Vec<ArtifactInput<'a>>,
    /// Completions, man pages, CLI specs, skills, desktop entries, icons,
    /// app bundles. An `asset` source names one of `assets` by file name.
    pub resources: Vec<Resource>,
    /// Separate release files that resources come from, digested into
    /// `subject` like artifacts.
    pub assets: Vec<AssetInput<'a>>,
    /// Prepended to artifact and asset names for their download URL, when
    /// given and the file has no URL of its own.
    pub url_base: Option<&'a str>,
    pub notes_url: Option<&'a str>,
    /// Release-level extensions, keyed by who defines them.
    pub extensions: Extensions,
    /// Who will sign the document.
    pub identity: Identity,
    pub attested_by: Attestor,
    pub evidence: Vec<Evidence>,
    /// Also record sha512 digests.
    pub sha512: bool,
    /// Commands the executables need on PATH, recorded on every artifact
    /// that has executables.
    pub requires_bin: Vec<RequiredBin>,
    /// Open the artifacts and record the shared libraries their
    /// executables load from the host as `requires.libs`.
    pub read_executables: bool,
}

impl<'a> Request<'a> {
    /// A vendor-attested request with nothing optional set.
    pub fn new(project: &'a str, version: &'a str, identity: Identity) -> Request<'a> {
        Request {
            project,
            version,
            published_at: None,
            source: None,
            artifacts: Vec::new(),
            resources: Vec::new(),
            assets: Vec::new(),
            url_base: None,
            notes_url: None,
            extensions: Extensions::new(),
            identity,
            attested_by: Attestor::Vendor,
            evidence: Vec::new(),
            sha512: true,
            requires_bin: Vec::new(),
            read_executables: true,
        }
    }
}

/// One artifact file, with optional overrides for what the name implies.
pub struct ArtifactInput<'a> {
    pub path: &'a Path,
    pub os: Option<&'a str>,
    /// [`ANY`] for a build that runs on every architecture of its OS.
    /// Without one, the name must say, unless the OS is macOS or Windows.
    pub arch: Option<&'a str>,
    /// [`ANY`] for a build that loads no C library from the host.
    /// Without one, the name says, or the executables do.
    pub libc: Option<&'a str>,
    /// Runs on any host: `os`, `arch`, and `libc` are left out whatever
    /// the file name says.
    pub portable: bool,
    pub variant: Option<String>,
    /// The download URL, when it is not `url_base/name`.
    pub url: Option<String>,
    /// The format, when it is not what the file name implies: `raw` for
    /// an `.exe` that is the program rather than an installer.
    pub format: Option<String>,
    /// Executables inside the artifact. On Windows a path without an
    /// extension gets `.exe`; the name never has one. For a bare
    /// executable (`raw`, `gz`, `xz`, `zst`, `bz2`), an entry that is a
    /// plain name becomes the artifact's own file under that name.
    pub bin: Vec<Bin>,
    pub requires: Option<Requires>,
    pub provenance: Vec<String>,
    /// Artifact-level extensions, keyed by who defines them.
    pub extensions: Extensions,
}

impl<'a> ArtifactInput<'a> {
    pub fn new(path: &'a Path) -> ArtifactInput<'a> {
        ArtifactInput {
            path,
            os: None,
            arch: None,
            libc: None,
            portable: false,
            variant: None,
            url: None,
            format: None,
            bin: Vec::new(),
            requires: None,
            provenance: Vec::new(),
            extensions: Extensions::new(),
        }
    }
}

/// A separate release file a resource comes from.
pub struct AssetInput<'a> {
    pub path: &'a Path,
    /// The download URL, when it is not `url_base/name`.
    pub url: Option<String>,
}

/// The statement, ready to sign.
#[derive(Debug)]
pub struct Created {
    /// The payload bytes that get signed.
    pub document: Vec<u8>,
    pub statement: Statement,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Invalid(#[from] crate::model::InvalidDocument),
    #[error("{0}")]
    Executables(#[from] crate::linkage::Error),
    #[error(
        "artifact {artifact:?} says its executables need libraries [{given}], but they load [{read}]; drop the list and let create read it"
    )]
    LibsMismatch {
        artifact: String,
        given: String,
        read: String,
    },
    #[error("{0}")]
    Archive(#[from] crate::archive::Error),
    #[error("{path}: not a packslip bundle: {why}")]
    NotAPackslip { path: String, why: String },
    #[error(
        "asset {path} is named {name:?}, but a different file with that name was already given"
    )]
    AssetCollision { name: String, path: String },
    #[error(
        "artifacts {a:?} and {b:?} both describe {platform}; give one a variant so consumers can tell them apart"
    )]
    Ambiguous {
        a: String,
        b: String,
        platform: String,
    },
    #[error(
        "artifact {name:?}: nothing in the name says what kind of file it is; give it a format, or leave it out of the artifacts if it is not one"
    )]
    UnknownFormat { name: String },
    #[error(
        "artifact {name:?} is for {os}, but nothing in the name says which architecture; give it as PATH:{os}/x86_64 or arch = \"x86_64\" in the manifest, with any in place of x86_64 if the build runs on every architecture"
    )]
    UnknownArch { name: String, os: String },
}

/// The `arch` or `libc` a vendor gives for a build that runs on every
/// architecture or C library: the field is left out.
pub const ANY: &str = "any";

/// The `(os, arch, libc, format)` a file name implies.
pub type Platform = (
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
);

/// Infer `(os, arch, libc, format)` from a release file name.
///
/// `libc` is only what the name says (`musl`, or `gnu` in a triple such as
/// `x86_64-unknown-linux-gnu`); [`create`] reads the rest from the
/// executables. An architecture the vocabulary lacks, such as goreleaser's
/// `mips64le`, is given as Rust's triples spell it (`mips64el`).
///
/// A `.exe` is taken for the program itself (`raw`) unless its name says
/// `setup`, `install`, or `installer`, which makes it an `exe` installer;
/// a manifest or `--format` settles any case the name leaves open.
pub fn infer_platform(name: &str) -> Platform {
    let lower = name.to_ascii_lowercase();
    // Android and iOS are read from whole words, since `helios` is not
    // iOS and `android-tools` is not an Android build. Their Rust triples
    // also say `linux` and `apple` (`aarch64-linux-android`,
    // `aarch64-apple-ios`), so the word wins when it follows that one, or
    // when the name says nothing else.
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let has = |word: &str| tokens.contains(&word);
    let follows = |first: &str, second: &str| {
        tokens
            .windows(2)
            .any(|pair| pair[0] == first && pair[1] == second)
    };
    let android = |word: &str| word == "android" || word == "androideabi";
    let os = if tokens
        .windows(2)
        .any(|pair| pair[0] == "linux" && android(pair[1]))
        || (tokens.iter().any(|t| android(t)) && !has("linux"))
    {
        Some("android")
    } else if follows("apple", "ios")
        || (has("ios") && !has("apple") && !has("darwin") && !has("macos"))
    {
        Some("ios")
    } else if lower.contains("linux")
        || lower.ends_with(".deb")
        || lower.ends_with(".rpm")
        || lower.ends_with(".appimage")
    {
        Some("linux")
    } else if lower.contains("darwin")
        || lower.contains("macos")
        || lower.contains("apple")
        || lower.ends_with(".dmg")
        || lower.ends_with(".pkg")
    {
        Some("darwin")
    } else if lower.contains("windows")
        || lower.contains("win64")
        || lower.contains("win32")
        || lower.ends_with(".exe")
        || lower.ends_with(".exe.gz")
        || lower.ends_with(".exe.zip")
        || lower.ends_with(".msi")
        || lower.ends_with(".msix")
    {
        Some("windows")
    } else if lower.contains("freebsd") {
        Some("freebsd")
    } else if lower.contains("netbsd") {
        Some("netbsd")
    } else if lower.contains("openbsd") {
        Some("openbsd")
    } else if lower.contains("illumos") {
        Some("illumos")
    } else {
        None
    };
    // A number stands for an architecture only as a word of its own, not
    // as a part of a dotted version: goreleaser's `386` is i686 in
    // `tool_1.2.0_linux_386.tar.gz`, but nothing in `tool-1.386.0.tar.gz`.
    let bytes = lower.as_bytes();
    let digit_at = |i: Option<usize>| i.and_then(|i| bytes.get(i)).is_some_and(u8::is_ascii_digit);
    let word = |w: &str| {
        lower.match_indices(w).any(|(start, _)| {
            let end = start + w.len();
            let before = start.checked_sub(1).map(|i| bytes[i]);
            let after = bytes.get(end).copied();
            let bounded = |c: Option<u8>| !c.is_some_and(|c| c.is_ascii_alphanumeric());
            let versioned = w.bytes().all(|b| b.is_ascii_digit())
                && ((before == Some(b'.') && digit_at(start.checked_sub(2)))
                    || (after == Some(b'.') && digit_at(Some(end + 1))));
            bounded(before) && bounded(after) && !versioned
        })
    };
    let arch = if lower.contains("x86_64")
        || lower.contains("x86-64")
        || lower.contains("x64")
        || lower.contains("amd64")
    {
        Some("x86_64")
    } else if lower.contains("aarch64") || lower.contains("arm64") {
        Some("aarch64")
    } else if lower.contains("armv7") || lower.contains("armhf") {
        Some("armv7")
    } else if lower.contains("armv6") {
        Some("armv6")
    } else if lower.contains("riscv64") {
        Some("riscv64")
    } else if lower.contains("ppc64le")
        || lower.contains("ppc64el")
        || lower.contains("powerpc64le")
    {
        Some("powerpc64le")
    } else if lower.contains("s390x") {
        Some("s390x")
    } else if lower.contains("loongarch64") || lower.contains("loong64") {
        Some("loongarch64")
    } else if lower.contains("i686")
        || lower.contains("i386")
        || lower.contains("x86")
        || word("ia32")
        || word("386")
    {
        Some("i686")
    } else if word("arm") {
        // Plain `arm` is ARMv6 in both of the conventions that write it:
        // goreleaser builds `GOARCH=arm` with `GOARM=6` unless told
        // otherwise, and Rust's `arm-unknown-linux-*` targets are ARMv6.
        Some("armv6")
    } else if word("mips64el") || word("mips64le") {
        // Architectures outside the vocabulary, spelled as Rust's triples
        // spell them, so no consumer takes them for another one.
        // goreleaser writes the little-endian ones `mipsle`, `mips64le`.
        Some("mips64el")
    } else if word("mips64") {
        Some("mips64")
    } else if word("mipsel") || word("mipsle") {
        Some("mipsel")
    } else if word("mips") {
        Some("mips")
    } else if word("ppc64") || word("powerpc64") {
        Some("powerpc64")
    } else if word("win64") {
        // Only when nothing else in the name gives the architecture.
        Some("x86_64")
    } else {
        None
    };
    // Only what the name says. `create` falls back to what the
    // executables load, then to `gnu`, for a Linux build.
    let libc = if !matches!(os, None | Some("linux")) {
        None
    } else if lower.contains("musl") {
        Some("musl")
    } else if tokens.iter().any(|t| {
        *t == "gnu"
            || t.starts_with("gnueabi")
            || t.starts_with("gnuabi")
            || t.starts_with("gnux32")
            || t.starts_with("glibc")
    }) {
        Some("gnu")
    } else {
        None
    };
    // Compound suffixes first, so `.tar.gz` is not taken for `.gz`.
    let format = [
        "tar.xz", "tar.gz", "tar.zst", "tar.bz2", "tgz", "tar", "zip", "7z", "gz", "xz", "zst",
        "bz2", "deb", "rpm", "dmg", "pkg", "msix", "msi", "exe", "appimage",
    ]
    .into_iter()
    .find(|ext| lower.ends_with(&format!(".{ext}")))
    .map(|ext| match ext {
        "exe" if !(lower.contains("setup") || lower.contains("install")) => "raw",
        other => other,
    });
    (os, arch, libc, format)
}

/// What the vendor gave for `arch` or `libc`: `Some(None)` for `any`,
/// which leaves the field out because the build runs on every one.
fn given(value: Option<&str>) -> Option<Option<&str>> {
    value.map(|v| (v != ANY).then_some(v))
}

/// Whether a file name ends in something that reads as an extension: a
/// short alphanumeric tail after the last dot with a letter in it. A
/// dotted version leaves a tail of its own, so `tool-1.2.3-linux-x64` is
/// a bare executable rather than a file of type `3-linux-x64`, and
/// `tool-linux-x64-1.2.3` is not one of type `3`. Every documented format
/// clears the bar: the longest is `appimage`, and `7z` is the only one
/// that starts with a digit.
fn has_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        ext.len() <= 9
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

/// Add `.exe` to a Windows executable name that has no extension.
fn windows_exe(value: &str) -> String {
    let last = value.rsplit('/').next().unwrap_or(value);
    if last.contains('.') {
        value.to_string()
    } else {
        format!("{value}.exe")
    }
}

/// Build and validate the release statement.
pub fn create(request: &Request<'_>) -> Result<Created, Error> {
    let file_name = |path: &Path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string()
    };
    // A file named as an asset is an asset, not a platform artifact, even
    // when an artifact glob such as `dist/*` swept it up as well.
    let asset_names: Vec<String> = request.assets.iter().map(|a| file_name(a.path)).collect();
    let mut subject = Vec::new();
    let mut artifacts: Vec<Artifact> = Vec::new();
    for input in &request.artifacts {
        let name = file_name(input.path);
        if asset_names.contains(&name) {
            continue;
        }
        let digests = crate::digest_file_all(input.path).map_err(|source| Error::Io {
            path: input.path.display().to_string(),
            source,
        })?;
        let (inferred_os, inferred_arch, inferred_libc, inferred_format) = infer_platform(&name);
        let os = if input.portable {
            None
        } else {
            input.os.or(inferred_os)
        };
        let given_arch = given(input.arch);
        let given_libc = given(input.libc);
        let arch = if input.portable {
            None
        } else {
            given_arch.unwrap_or(inferred_arch)
        };
        // A file with no archive or installer extension is the executable
        // itself, when the name says which host it is for or the vendor
        // said it runs anywhere. The tail after the last dot only counts
        // as an extension when it looks like one: a released binary is
        // often named for a dotted version, and `mise-v2026.9.1-linux-x64`
        // ends in `1-linux-x64`, which is a platform, not a format.
        let format = input
            .format
            .clone()
            .or_else(|| inferred_format.map(str::to_string))
            .or_else(|| {
                (!has_extension(&name) && (os.is_some() || input.portable))
                    .then(|| "raw".to_string())
            });
        // An artifact with no format is dead weight: rule 3 of Selecting
        // an artifact keeps a consumer from ever choosing one, so it
        // would ship a digest and a platform that nothing can act on.
        // A library or a header swept up by a glob lands here.
        let Some(format) = format else {
            return Err(Error::UnknownFormat { name });
        };
        let format = Some(format);
        // An artifact with no architecture fits every host of its OS, so
        // a name that only fails to say one would hand an x86 build to an
        // ARM host. macOS and Windows are the exceptions: a build named for
        // no architecture there is by convention universal or x86, and
        // both run x86 builds on ARM. A package its own format declares
        // architecture-independent, `.noarch.rpm` or `_all.deb`, is another:
        // the package manager already installs it on every architecture.
        let lower = name.to_ascii_lowercase();
        let arch_independent_package = match format.as_deref() {
            Some("rpm") => lower.ends_with(".noarch.rpm"),
            Some("deb") => lower.ends_with("_all.deb"),
            _ => false,
        };
        if !input.portable
            && arch.is_none()
            && given_arch.is_none()
            && !arch_independent_package
            && let Some(os) = os
            && !matches!(os, "darwin" | "windows")
        {
            return Err(Error::UnknownArch {
                name,
                os: os.to_string(),
            });
        }
        let bare = format
            .as_deref()
            .filter(|f| crate::model::is_bare_format(f))
            .map(|f| crate::model::bare_file_name(&name, f).to_string());
        let windows = os == Some("windows");
        // Inside an archive the executables are where they really are: a
        // plain `--bin tool` is looked up, and a given path is checked.
        // An archive that cannot be read is taken at the vendor's word.
        let listed = match format.as_deref() {
            Some(f) if crate::archive::can_list(f) && !input.bin.is_empty() => {
                match crate::archive::resolve_bins(input.path, f, &input.bin, windows) {
                    Ok(bins) => Some(bins),
                    Err(crate::archive::Error::Undecodable { .. }) => None,
                    Err(err) => return Err(Error::Archive(err)),
                }
            }
            _ => None,
        };
        let verified = listed.is_some();
        let bin: Vec<Bin> = listed
            .unwrap_or_else(|| input.bin.clone())
            .into_iter()
            .map(|b| {
                // `--bin tool` names the program; for a bare executable the
                // program is the artifact itself, under that name.
                let (b, path_is_real) = match &bare {
                    Some(file) if !b.path.contains('/') => (Bin::named(file, &b.name), true),
                    _ => (b, verified),
                };
                if windows {
                    // The path takes `.exe` only when nothing confirmed the
                    // file: a path read from the archive or the artifact's own
                    // name is already exact. The name is the command as typed
                    // and never carries it.
                    let path = if path_is_real {
                        b.path
                    } else {
                        windows_exe(&b.path)
                    };
                    Bin::named(path, b.name)
                } else {
                    b
                }
            })
            .collect();
        let url = input.url.clone().or_else(|| {
            request
                .url_base
                .map(|base| format!("{}/{name}", base.trim_end_matches('/')))
        });
        // What the executables load is read from them, so the document
        // says what the bytes say. An artifact `create` cannot open, or
        // an executable that is a script, records nothing.
        let executables = if request.read_executables {
            crate::linkage::read_executables(input.path, format.as_deref(), &bin)?
        } else {
            None
        };
        let mut requires = input.requires.clone().unwrap_or_default();
        if !bin.is_empty() {
            for required in &request.requires_bin {
                if !requires.bin.contains(required) {
                    requires.bin.push(required.clone());
                }
            }
            // A list the manifest gives must agree with what is read.
            if let Some(read) = executables
                .as_ref()
                .and_then(crate::linkage::host_libraries)
            {
                // The read list is sorted; a given one may be in any order.
                let given_sorted = requires.libs.as_ref().map(|given| {
                    let mut sorted = given.clone();
                    sorted.sort();
                    sorted.dedup();
                    sorted
                });
                match (&requires.libs, given_sorted) {
                    (Some(given), Some(sorted)) if sorted != read => {
                        return Err(Error::LibsMismatch {
                            artifact: name,
                            given: given.join(", "),
                            read: read.join(", "),
                        });
                    }
                    _ => requires.libs = Some(read),
                }
            }
        }
        // A Linux build's C library is what the vendor or the name says,
        // else what the executables load: none for a static build, which
        // runs on a musl host as well as a glibc one. Only when neither
        // can be told is it taken for `gnu`, the common case.
        let libc = if input.portable {
            None
        } else if let Some(libc) = given_libc {
            libc
        } else if os != Some("linux") {
            None
        } else if inferred_libc.is_some() {
            inferred_libc
        } else {
            match executables.as_ref().and_then(crate::linkage::elf_libc) {
                Some(read) => read.value(),
                None => Some("gnu"),
            }
        };
        let artifact = Artifact {
            url,
            name: name.clone(),
            os: os.map(str::to_string),
            arch: arch.map(str::to_string),
            libc: libc.map(str::to_string),
            variant: input.variant.clone(),
            size: digests.size,
            format,
            bin,
            requires: (!requires.is_empty()).then_some(requires),
            provenance: input.provenance.clone(),
            extensions: input.extensions.clone(),
        };
        // Anything with a format is selectable, so two artifacts that agree
        // on platform, variant, and format leave a consumer nothing to
        // choose by, portable ones included.
        if artifact.format.is_some()
            && let Some(other) = artifacts.iter().find(|a| {
                a.os == artifact.os
                    && a.arch == artifact.arch
                    && a.libc == artifact.libc
                    && a.format == artifact.format
                    && a.variant == artifact.variant
            })
        {
            return Err(Error::Ambiguous {
                a: other.name.clone(),
                b: artifact.name.clone(),
                platform: [
                    artifact.os.as_deref(),
                    artifact.arch.as_deref(),
                    artifact.libc.as_deref(),
                    artifact.format.as_deref(),
                    artifact.variant.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("/"),
            });
        }
        subject.push(Subject {
            name,
            digest: Digest {
                sha256: digests.sha256,
                sha512: request.sha512.then_some(digests.sha512),
            },
        });
        artifacts.push(artifact);
    }
    let mut asset_urls = std::collections::BTreeMap::new();
    let mut asset_digests: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for asset in &request.assets {
        let name = file_name(asset.path);
        let digests = crate::digest_file_all(asset.path).map_err(|source| Error::Io {
            path: asset.path.display().to_string(),
            source,
        })?;
        // The same file given twice is one asset; two different files that
        // share a name cannot both be the subject that name would carry.
        match asset_digests.get(&name) {
            Some(seen) if *seen == digests.sha256 => continue,
            Some(_) => {
                return Err(Error::AssetCollision {
                    name,
                    path: asset.path.display().to_string(),
                });
            }
            None => {
                asset_digests.insert(name.clone(), digests.sha256.clone());
            }
        }
        let url = asset.url.clone().or_else(|| {
            request
                .url_base
                .map(|base| format!("{}/{name}", base.trim_end_matches('/')))
        });
        asset_urls.insert(name.clone(), url);
        subject.push(Subject {
            name,
            digest: Digest {
                sha256: digests.sha256,
                sha512: request.sha512.then_some(digests.sha512),
            },
        });
    }
    let resources = request
        .resources
        .iter()
        .map(|resource| {
            let mut resource = resource.clone();
            if resource.url.is_none()
                && let Some(url) = resource.asset.as_ref().and_then(|a| asset_urls.get(a))
            {
                resource.url = url.clone();
            }
            resource
        })
        .collect();
    let published_at = request
        .published_at
        .map(str::to_string)
        .unwrap_or_else(|| jiff::Timestamp::now().to_string());
    let statement = Statement {
        kind: STATEMENT_TYPE.into(),
        subject,
        predicate_type: PREDICATE_TYPE.into(),
        predicate: Predicate {
            project: request.project.into(),
            version: request.version.into(),
            published_at,
            source: request.source.clone(),
            artifacts,
            resources,
            identity: request.identity.clone(),
            attested_by: request.attested_by,
            evidence: request.evidence.clone(),
            notes_url: request.notes_url.map(str::to_string),
            extensions: request.extensions.clone(),
        },
    };
    statement.validate()?;
    let document = statement.canonical_bytes();
    Ok(Created {
        document,
        statement,
    })
}

/// One released packslip to list: where consumers fetch it, and the local
/// copy to digest and read the version from.
pub struct ListedRelease<'a> {
    pub url: &'a str,
    pub bundle_path: &'a Path,
    /// Withdrawn by the vendor, with the reason.
    pub yanked: Option<String>,
    pub security: bool,
    /// What the list's publisher checked about the release, when the
    /// publisher is not the vendor.
    pub evidence: Vec<Evidence>,
}

/// What `create_release_list` needs.
pub struct ListRequest<'a> {
    pub project: &'a str,
    pub generated_at: Option<&'a str>,
    /// How long the list stays current.
    pub valid_for: std::time::Duration,
    pub sequence: u64,
    /// Exact listed version recommended for unconstrained latest requests.
    pub latest: Option<&'a str>,
    pub releases: Vec<ListedRelease<'a>>,
    pub identity: Identity,
}

/// The release list, ready to sign.
#[derive(Debug)]
pub struct CreatedList {
    pub document: Vec<u8>,
    pub statement: ReleaseListStatement,
}

/// Build and validate the release list from released bundles. The bundles
/// are read, not verified: the list's signer vouches for them.
pub fn create_release_list(request: &ListRequest<'_>) -> Result<CreatedList, Error> {
    let generated: jiff::Timestamp = match request.generated_at {
        Some(text) => text.parse().map_err(|_| {
            Error::Invalid(crate::model::InvalidDocument::Timestamp {
                field: "generated_at",
                value: text.to_string(),
            })
        })?,
        None => jiff::Timestamp::now(),
    };
    let expires = generated
        .checked_add(
            jiff::SignedDuration::try_from(request.valid_for)
                .map_err(|_| Error::Invalid(crate::model::InvalidDocument::Expiry))?,
        )
        .map_err(|_| Error::Invalid(crate::model::InvalidDocument::Expiry))?;
    let mut subject = Vec::new();
    let mut releases = Vec::new();
    for listed in &request.releases {
        let digests = crate::digest_file_all(listed.bundle_path).map_err(|source| Error::Io {
            path: listed.bundle_path.display().to_string(),
            source,
        })?;
        let text = std::fs::read_to_string(listed.bundle_path).map_err(|source| Error::Io {
            path: listed.bundle_path.display().to_string(),
            source,
        })?;
        let payload = crate::sigstore::peek_statement(&text).map_err(|e| Error::NotAPackslip {
            path: listed.bundle_path.display().to_string(),
            why: e.to_string(),
        })?;
        let statement: Statement =
            serde_json::from_slice(&payload).map_err(|e| Error::NotAPackslip {
                path: listed.bundle_path.display().to_string(),
                why: e.to_string(),
            })?;
        if statement.predicate.project != request.project {
            return Err(Error::NotAPackslip {
                path: listed.bundle_path.display().to_string(),
                why: format!(
                    "it is for {}, the list is for {}",
                    statement.predicate.project, request.project
                ),
            });
        }
        subject.push(Subject {
            name: listed.url.to_string(),
            digest: Digest {
                sha256: digests.sha256,
                sha512: Some(digests.sha512),
            },
        });
        releases.push(ReleaseRef {
            version: statement.predicate.version,
            tag: statement.predicate.source.and_then(|s| s.tag),
            published_at: statement.predicate.published_at,
            packslip: listed.url.to_string(),
            status: listed
                .yanked
                .is_some()
                .then_some(crate::model::ReleaseStatus::Yanked),
            status_reason: listed.yanked.clone(),
            security: listed.security,
            evidence: listed.evidence.clone(),
            extensions: Extensions::new(),
        });
    }
    let statement = Envelope {
        kind: STATEMENT_TYPE.into(),
        subject,
        predicate_type: RELEASES_PREDICATE_TYPE.into(),
        predicate: ReleaseList {
            project: request.project.into(),
            generated_at: generated.to_string(),
            expires_at: expires.to_string(),
            sequence: request.sequence,
            latest: request.latest.map(str::to_owned),
            identity: request.identity.clone(),
            releases,
            extensions: Extensions::new(),
        },
    };
    statement.validate()?;
    let document = statement.canonical_bytes();
    Ok(CreatedList {
        document,
        statement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minisign::SecretKey;
    use crate::model::Scheme;
    use crate::sigstore::{Signer, Trust};

    fn key_identity(key: &SecretKey) -> Identity {
        Signer::Key {
            key: key.clone(),
            log: false,
        }
        .identity()
    }

    #[test]
    fn infers_platforms() {
        // The name gives a libc only when it says one; `create` reads the
        // rest from the executables.
        assert_eq!(
            infer_platform("mise-v2026.9.1-linux-x64.tar.xz"),
            (Some("linux"), Some("x86_64"), None, Some("tar.xz"))
        );
        assert_eq!(
            infer_platform("mise-v2026.9.1-linux-arm64-musl.tar.gz"),
            (Some("linux"), Some("aarch64"), Some("musl"), Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-macos-aarch64.zip"),
            (Some("darwin"), Some("aarch64"), None, Some("zip"))
        );
        // A .exe is the program unless its name says it installs.
        assert_eq!(
            infer_platform("tool-windows-x64.exe"),
            (Some("windows"), Some("x86_64"), None, Some("raw"))
        );
        assert_eq!(
            infer_platform("Tool-Setup-1.2.3.exe"),
            (Some("windows"), None, None, Some("exe"))
        );
        assert_eq!(
            infer_platform("tool-installer-x64.exe"),
            (Some("windows"), Some("x86_64"), None, Some("exe"))
        );
        // Single compressed executables, and a plain tar.
        assert_eq!(
            infer_platform("argo-linux-amd64.gz"),
            (Some("linux"), Some("x86_64"), None, Some("gz"))
        );
        assert_eq!(
            infer_platform("argo-windows-amd64.exe.gz"),
            (Some("windows"), Some("x86_64"), None, Some("gz"))
        );
        assert_eq!(
            infer_platform("restic_0.16.0_linux_arm64.bz2"),
            (Some("linux"), Some("aarch64"), None, Some("bz2"))
        );
        assert_eq!(
            infer_platform("tool-linux-x64.zst"),
            (Some("linux"), Some("x86_64"), None, Some("zst"))
        );
        assert_eq!(
            infer_platform("tool-linux-x64.xz"),
            (Some("linux"), Some("x86_64"), None, Some("xz"))
        );
        assert_eq!(
            infer_platform("mmctl_linux_amd64.tar"),
            (Some("linux"), Some("x86_64"), None, Some("tar"))
        );
        // Less common hosts and architectures.
        // Android and iOS triples also say linux and apple.
        assert_eq!(
            infer_platform("tool-aarch64-linux-android.tar.gz"),
            (Some("android"), Some("aarch64"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-aarch64-apple-ios.zip"),
            (Some("ios"), Some("aarch64"), None, Some("zip"))
        );
        assert_eq!(
            infer_platform("tool-android-arm64.tar.gz"),
            (Some("android"), Some("aarch64"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-armv7-linux-androideabi.tar.gz"),
            (Some("android"), Some("armv7"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-ios-arm64.zip"),
            (Some("ios"), Some("aarch64"), None, Some("zip"))
        );
        // Whole words only: a product name is not a platform.
        assert_eq!(
            infer_platform("helios-linux-amd64.tar.gz"),
            (Some("linux"), Some("x86_64"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("android-tools_linux_amd64.tar.gz"),
            (Some("linux"), Some("x86_64"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("studios-darwin-arm64.tar.gz"),
            (Some("darwin"), Some("aarch64"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-netbsd-i386.tar.gz"),
            (Some("netbsd"), Some("i686"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-linux-ppc64le.tar.gz"),
            (Some("linux"), Some("powerpc64le"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-linux-s390x.tar.gz"),
            (Some("linux"), Some("s390x"), None, Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-linux-armv6.tar.gz"),
            (Some("linux"), Some("armv6"), None, Some("tar.gz"))
        );
        assert_eq!(infer_platform("SHASUMS256.txt"), (None, None, None, None));
        assert_eq!(
            infer_platform("tool_amd64.deb"),
            (Some("linux"), Some("x86_64"), None, Some("deb"))
        );
        assert_eq!(
            infer_platform("Tool.dmg"),
            (Some("darwin"), None, None, Some("dmg"))
        );
        assert_eq!(
            infer_platform("Tool.msi"),
            (Some("windows"), None, None, Some("msi"))
        );
        assert_eq!(
            infer_platform("Tool-x64.msix"),
            (Some("windows"), Some("x86_64"), None, Some("msix"))
        );
        assert_eq!(
            infer_platform("LM-Studio-0.3.0-x64.AppImage"),
            (Some("linux"), Some("x86_64"), None, Some("appimage"))
        );
        assert_eq!(
            infer_platform("tool-linux-x86-64.tar.zst"),
            (Some("linux"), Some("x86_64"), None, Some("tar.zst"))
        );
        assert_eq!(
            infer_platform("tool-linux-x64.7z"),
            (Some("linux"), Some("x86_64"), None, Some("7z"))
        );
        assert_eq!(
            infer_platform("tool-x86_64-unknown-linux-gnu.tar.xz"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("tar.xz"))
        );
        assert_eq!(
            infer_platform("tool-arm-unknown-linux-gnueabihf.tar.gz"),
            (Some("linux"), Some("armv6"), Some("gnu"), Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-x86_64-unknown-linux-musl.tar.gz"),
            (Some("linux"), Some("x86_64"), Some("musl"), Some("tar.gz"))
        );
        // libc is for Linux builds; a windows-gnu triple names none.
        assert_eq!(
            infer_platform("tool-x86_64-pc-windows-gnu.zip"),
            (Some("windows"), Some("x86_64"), None, Some("zip"))
        );
        // goreleaser's architecture names.
        for (name, arch) in [
            ("helmfile_1.8.0_linux_386.tar.gz", "i686"),
            ("tool_1.0.0_windows_386.zip", "i686"),
            ("tool-win32-ia32.zip", "i686"),
            ("tool_1.0.0_linux_arm.tar.gz", "armv6"),
            ("tool_1.0.0_linux_armv6.tar.gz", "armv6"),
            ("tool_1.0.0_linux_armv7.tar.gz", "armv7"),
            ("tool_1.0.0_linux_arm64.tar.gz", "aarch64"),
            ("tool_1.0.0_linux_ppc64le.tar.gz", "powerpc64le"),
            ("tool_1.0.0_linux_s390x.tar.gz", "s390x"),
            ("tool_1.0.0_linux_riscv64.tar.gz", "riscv64"),
            ("tool_1.0.0_linux_loong64.tar.gz", "loongarch64"),
            // Outside the vocabulary, as Rust's triples spell them.
            ("tool_1.0.0_linux_mips.tar.gz", "mips"),
            ("tool_1.0.0_linux_mips_softfloat.tar.gz", "mips"),
            ("tool_1.0.0_linux_mipsle.tar.gz", "mipsel"),
            ("tool_1.0.0_linux_mips64.tar.gz", "mips64"),
            ("tool_1.0.0_linux_mips64le_hardfloat.tar.gz", "mips64el"),
            ("tool-mips64el-unknown-linux-gnuabi64.tar.gz", "mips64el"),
            ("tool_1.0.0_linux_ppc64.tar.gz", "powerpc64"),
            ("tool-powerpc64-unknown-linux-gnu.tar.gz", "powerpc64"),
            ("ffmpeg-win64-gpl.zip", "x86_64"),
        ] {
            assert_eq!(infer_platform(name).1, Some(arch), "{name}");
        }
        // A number is an architecture only as a word of its own, never as
        // part of a version.
        for name in [
            "tool-1.386.0-linux.tar.gz",
            "tool-386.1.0-linux.tar.gz",
            "tool-v2.0.386-linux.tar.gz",
            "tool386-linux.tar.gz",
            "armory-linux.tar.gz",
            "tool-linux-armel.tar.gz",
            "tool-linux.tar.gz",
        ] {
            assert_eq!(infer_platform(name).1, None, "{name}");
        }
    }

    #[test]
    fn libc_is_read_from_the_executables() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = |name: &str| {
            std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(name),
            )
            .unwrap()
        };
        let key = SecretKey::from_seed([1u8; 32]);
        let libc_of = |file: &str, elf: &str, libc: Option<&str>| {
            let path = dir.path().join(file);
            std::fs::write(&path, fixture(elf)).unwrap();
            let created = create(&Request {
                artifacts: vec![ArtifactInput {
                    libc,
                    bin: vec![Bin::new("tool")],
                    ..ArtifactInput::new(&path)
                }],
                ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
            })
            .unwrap();
            created.statement.predicate.artifacts[0].libc.clone()
        };
        let read = |file, elf| libc_of(file, elf, None);
        // A static build, such as a Go binary built with CGO_ENABLED=0,
        // names no libc, so a musl host takes it as well as a glibc one.
        assert_eq!(read("helmfile_linux_amd64", "static"), None);
        assert_eq!(read("tool-linux-x64", "needs-z").as_deref(), Some("gnu"));
        assert_eq!(
            read("tool-linux-arm64", "needs-musl").as_deref(),
            Some("musl")
        );
        // What the name says wins over the bytes.
        assert_eq!(
            read("tool-linux-x64-musl", "static").as_deref(),
            Some("musl")
        );
        assert_eq!(
            read("tool-x86_64-unknown-linux-gnu", "static").as_deref(),
            Some("gnu")
        );
        // And what the vendor says wins over both; `any` leaves it out.
        assert_eq!(
            libc_of("tool-linux-riscv64", "static", Some("gnu")).as_deref(),
            Some("gnu")
        );
        assert_eq!(libc_of("tool-linux-s390x", "needs-z", Some(ANY)), None);
        // Without reading the executables the default is gnu, as before.
        let path = dir.path().join("tool-linux-ppc64le");
        std::fs::write(&path, fixture("static")).unwrap();
        let unread = create(&Request {
            artifacts: vec![ArtifactInput {
                bin: vec![Bin::new("tool")],
                ..ArtifactInput::new(&path)
            }],
            read_executables: false,
            ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
        })
        .unwrap();
        assert_eq!(
            unread.statement.predicate.artifacts[0].libc.as_deref(),
            Some("gnu")
        );
        // A static executable in a non-Linux build still names no libc.
        assert_eq!(read("tool-freebsd-x64", "static"), None);
    }

    #[test]
    fn an_architecture_is_required() {
        let dir = tempfile::tempdir().unwrap();
        let key = SecretKey::from_seed([1u8; 32]);
        let one = |input: ArtifactInput<'_>| {
            create(&Request {
                artifacts: vec![input],
                ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
            })
        };
        let file = |name: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, b"bytes").unwrap();
            path
        };
        // A Linux or BSD build whose name says no architecture would fit
        // every host of its OS, so it is refused.
        for name in [
            "tool_1.0.0_linux_sparc.tar.gz",
            "tool-1.386.0-linux.tar.gz",
            "tool-freebsd.tar.gz",
            "tool-linux.AppImage",
            "tool-linux-noarch-tool.rpm",
            "tool-linux-all-tools.deb",
        ] {
            let path = file(name);
            let err = one(ArtifactInput::new(&path)).unwrap_err();
            assert!(matches!(err, Error::UnknownArch { .. }), "{name}: {err}");
            assert!(err.to_string().contains("any in place of"), "{err}");
        }
        let path = file("tool_1.0.0_linux_sparc.tar.gz");
        let err = one(ArtifactInput {
            os: Some("linux"),
            ..ArtifactInput::new(&path)
        })
        .unwrap_err();
        assert!(matches!(err, Error::UnknownArch { .. }), "{err}");
        // Naming one, saying `any`, or declaring the artifact portable
        // settles it.
        let named = one(ArtifactInput {
            os: Some("linux"),
            arch: Some("sparc64"),
            ..ArtifactInput::new(&path)
        })
        .unwrap();
        assert_eq!(
            named.statement.predicate.artifacts[0].arch.as_deref(),
            Some("sparc64")
        );
        let any = one(ArtifactInput {
            arch: Some(ANY),
            ..ArtifactInput::new(&path)
        })
        .unwrap();
        let artifact = &any.statement.predicate.artifacts[0];
        assert_eq!(
            (artifact.os.as_deref(), artifact.arch.as_deref()),
            (Some("linux"), None)
        );
        let portable = one(ArtifactInput {
            portable: true,
            ..ArtifactInput::new(&path)
        })
        .unwrap();
        assert_eq!(portable.statement.predicate.artifacts[0].os, None);
        // A macOS or Windows build named for no architecture is universal
        // or x86, which both run on ARM, and a file with no OS in its name
        // is not refused either, nor is a package whose format declares it
        // architecture-independent.
        for name in [
            "tool-1.0-1.noarch.rpm",
            "tool_1.0_all.deb",
            "Tool.dmg",
            "tool-macos.zip",
            "Tool-Setup-1.2.3.exe",
            "Tool.msi",
            "tool.tar.gz",
        ] {
            let path = file(name);
            let created = one(ArtifactInput::new(&path)).unwrap();
            assert_eq!(created.statement.predicate.artifacts[0].arch, None);
        }
    }

    #[test]
    fn resources_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("tool-v1.0.0-linux-x64.tar.xz");
        let skill = dir.path().join("tool-skill.tar.gz");
        std::fs::write(&a, b"linux bytes").unwrap();
        std::fs::write(&skill, b"skill bytes").unwrap();
        let key = SecretKey::from_seed([1u8; 32]);
        let request = Request {
            artifacts: vec![ArtifactInput {
                bin: vec![Bin::new("tool")],
                ..ArtifactInput::new(&a)
            }],
            resources: vec![
                Resource {
                    shell: Some("zsh".into()),
                    archive: Some("share/zsh/site-functions/_tool".into()),
                    ..Resource::new("completion")
                },
                Resource {
                    name: Some("tool".into()),
                    asset: Some("tool-skill.tar.gz".into()),
                    ..Resource::new("skill")
                },
                Resource {
                    format: Some("usage".into()),
                    bin: Some("tool".into()),
                    exec: vec!["tool".into(), "usage".into()],
                    ..Resource::new("cli-spec")
                },
            ],
            assets: vec![AssetInput {
                path: &skill,
                url: None,
            }],
            url_base: Some("https://dl.example.com/1.0.0"),
            ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
        };
        let created = create(&request).unwrap();
        let s = &created.statement;
        assert_eq!(s.subject.len(), 2);
        assert_eq!(s.subject[1].name, "tool-skill.tar.gz");
        assert_eq!(
            s.predicate.resources[1].url.as_deref(),
            Some("https://dl.example.com/1.0.0/tool-skill.tar.gz")
        );
        assert_eq!(s.predicate.resources[0].url, None);
        let asset_url = Request {
            assets: vec![AssetInput {
                path: &skill,
                url: Some("https://cdn.example.com/skill.tar.gz".into()),
            }],
            ..request
        };
        let created = create(&asset_url).unwrap();
        assert_eq!(
            created.statement.predicate.resources[1].url.as_deref(),
            Some("https://cdn.example.com/skill.tar.gz")
        );

        // A file given as both an artifact and an asset is the asset, once.
        let swept = create(&Request {
            artifacts: vec![
                ArtifactInput {
                    bin: vec![Bin::new("tool")],
                    ..ArtifactInput::new(&a)
                },
                ArtifactInput::new(&skill),
            ],
            assets: vec![
                AssetInput {
                    path: &skill,
                    url: None,
                },
                AssetInput {
                    path: &skill,
                    url: None,
                },
            ],
            resources: vec![Resource {
                name: Some("tool".into()),
                asset: Some("tool-skill.tar.gz".into()),
                ..Resource::new("skill")
            }],
            ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
        })
        .unwrap();
        assert_eq!(swept.statement.predicate.artifacts.len(), 1);
        assert_eq!(swept.statement.subject.len(), 2);
        assert_eq!(swept.statement.subject[1].name, "tool-skill.tar.gz");

        // Two different files sharing a name are refused, not collapsed.
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let other_skill = elsewhere.join("tool-skill.tar.gz");
        std::fs::write(&other_skill, b"different bytes").unwrap();
        let err = create(&Request {
            artifacts: vec![ArtifactInput {
                bin: vec![Bin::new("tool")],
                ..ArtifactInput::new(&a)
            }],
            assets: vec![
                AssetInput {
                    path: &skill,
                    url: None,
                },
                AssetInput {
                    path: &other_skill,
                    url: None,
                },
            ],
            resources: vec![Resource {
                name: Some("tool".into()),
                asset: Some("tool-skill.tar.gz".into()),
                ..Resource::new("skill")
            }],
            ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
        })
        .unwrap_err();
        assert!(matches!(err, Error::AssetCollision { .. }), "{err}");

        // The asset digest is checked like an artifact's, and a resource
        // whose asset was not given is refused.
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::verify::Options {
            require_log: false,
            trusted_root: &root,
        };
        let bundle = crate::sigstore::sign(
            Signer::Key {
                key: key.clone(),
                log: false,
            },
            &created.document,
        )
        .unwrap();
        let verified =
            crate::verify::verify(&bundle, &Trust::Key(&key.public_key()), options, &[&skill])
                .unwrap();
        assert_eq!(verified.checked_artifacts, ["tool-skill.tar.gz"]);
        assert_eq!(
            verified.resources,
            [
                "completion/zsh (archive)",
                "skill/tool (asset)",
                "cli-spec/usage/tool (exec)"
            ]
        );
        let err = create(&Request {
            assets: vec![],
            ..asset_url
        })
        .unwrap_err();
        assert!(err.to_string().contains("not a subject"), "{err}");
    }

    #[test]
    fn create_sign_verify_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("tool-v1.0.0-linux-x64.tar.xz");
        let b = dir.path().join("tool-v1.0.0-darwin-arm64.tar.xz");
        let w = dir.path().join("tool-v1.0.0-windows-x64.zip");
        let raw = dir.path().join("tool-linux-arm64");
        std::fs::write(&a, b"linux bytes").unwrap();
        std::fs::write(&b, b"darwin bytes").unwrap();
        std::fs::write(&w, b"windows bytes").unwrap();
        std::fs::write(&raw, b"bare").unwrap();
        let key = SecretKey::from_seed([1u8; 32]);
        let input = |path, os, bin: &[&str], provenance: &[&str]| ArtifactInput {
            os,
            bin: bin.iter().map(|s| Bin::new(*s)).collect(),
            provenance: provenance.iter().map(|s| s.to_string()).collect(),
            ..ArtifactInput::new(path)
        };
        let request = |artifacts, source, url_base| Request {
            published_at: Some("2026-09-01T00:00:00Z"),
            source,
            artifacts,
            url_base,
            ..Request::new("tool.example.com", "1.0.0", key_identity(&key))
        };
        let created = create(&request(
            vec![
                input(
                    &a,
                    None,
                    &["tool"],
                    &["https://example.com/a.sigstore.json"],
                ),
                input(&b, None, &["tool"], &[]),
                input(&w, None, &["bin/tool"], &[]),
                ArtifactInput {
                    bin: vec![Bin::named("tool-linux-arm64", "tool")],
                    url: Some("https://cdn.example.com/tool-linux-arm64".into()),
                    ..ArtifactInput::new(&raw)
                },
            ],
            Some(Source {
                repo: "https://github.com/example/tool".into(),
                commit: None,
                tag: Some("v1.0.0".into()),
            }),
            Some("https://github.com/example/tool/releases/download/v1.0.0/"),
        ))
        .unwrap();
        let arts = &created.statement.predicate.artifacts;
        assert_eq!(
            arts[0].url.as_deref(),
            Some(
                "https://github.com/example/tool/releases/download/v1.0.0/tool-v1.0.0-linux-x64.tar.xz"
            )
        );
        assert_eq!(arts[0].size, 11);
        assert_eq!(arts[0].bin, [Bin::new("tool")]);
        assert_eq!(
            arts[2].bin,
            [Bin::named("bin/tool.exe", "tool")],
            "windows gets .exe on the path, never the name"
        );
        assert_eq!(arts[3].format.as_deref(), Some("raw"));
        assert_eq!(
            arts[3].url.as_deref(),
            Some("https://cdn.example.com/tool-linux-arm64")
        );
        assert_eq!(arts[3].bin[0].name, "tool");

        // `--bin tool` on a bare executable names the artifact itself,
        // compressed or not, and on Windows both sides get .exe.
        let gz = dir.path().join("tool-linux-x64.gz");
        let exe = dir.path().join("tool-windows-x64.exe");
        let exe_gz = dir.path().join("tool-windows-arm64.exe.gz");
        for path in [&gz, &exe, &exe_gz] {
            std::fs::write(path, b"bare").unwrap();
        }
        let bare = create(&request(
            vec![
                input(&raw, None, &["tool"], &[]),
                input(&gz, None, &["tool"], &[]),
                input(&exe, None, &["tool"], &[]),
                input(&exe_gz, None, &["tool"], &[]),
            ],
            None,
            None,
        ))
        .unwrap();
        let arts = &bare.statement.predicate.artifacts;
        assert_eq!(arts[0].bin, [Bin::named("tool-linux-arm64", "tool")]);
        assert_eq!(arts[1].format.as_deref(), Some("gz"));
        assert_eq!(arts[1].bin, [Bin::named("tool-linux-x64", "tool")]);
        assert_eq!(arts[2].format.as_deref(), Some("raw"));
        assert_eq!(arts[2].bin, [Bin::named("tool-windows-x64.exe", "tool")]);
        assert_eq!(arts[3].format.as_deref(), Some("gz"));
        assert_eq!(arts[3].bin, [Bin::named("tool-windows-arm64.exe", "tool")]);

        // A format override and a portable artifact.
        let setup = dir.path().join("tool-x64.exe");
        let jar = dir.path().join("tool-linux.jar");
        std::fs::write(&setup, b"installer").unwrap();
        std::fs::write(&jar, b"jar").unwrap();
        let overridden = create(&request(
            vec![
                ArtifactInput {
                    format: Some("exe".into()),
                    ..ArtifactInput::new(&setup)
                },
                ArtifactInput {
                    portable: true,
                    format: Some("jar".into()),
                    ..ArtifactInput::new(&jar)
                },
            ],
            None,
            None,
        ))
        .unwrap();
        let arts = &overridden.statement.predicate.artifacts;
        assert_eq!(arts[0].format.as_deref(), Some("exe"));
        assert!(arts[0].bin.is_empty());
        assert_eq!(
            (&arts[1].os, &arts[1].arch, &arts[1].libc),
            (&None, &None, &None)
        );
        assert_eq!(arts[1].format.as_deref(), Some("jar"));

        // `.jar` is no format the name inference knows, and running it is
        // not something a consumer could guess, so without that override
        // the file is refused rather than published unusable. A header or
        // a shared library swept up by an artifacts glob lands here too.
        let header = dir.path().join("tool.h");
        std::fs::write(&header, b"#pragma once").unwrap();
        for path in [&jar, &header] {
            let err = create(&request(
                vec![ArtifactInput {
                    portable: true,
                    ..ArtifactInput::new(path)
                }],
                None,
                None,
            ))
            .unwrap_err();
            assert!(
                matches!(err, Error::UnknownFormat { .. }),
                "{path:?}: {err}"
            );
        }

        // A portable file with no extension is still a bare executable, and
        // a bare Windows file without .exe keeps its exact name as the path.
        let script = dir.path().join("tool-universal");
        let bare_win = dir.path().join("tool-windows-x64");
        std::fs::write(&script, b"#!/bin/sh").unwrap();
        std::fs::write(&bare_win, b"pe").unwrap();
        let portable = create(&request(
            vec![
                ArtifactInput {
                    portable: true,
                    bin: vec![Bin::new("tool")],
                    ..ArtifactInput::new(&script)
                },
                input(&bare_win, None, &["tool"], &[]),
            ],
            None,
            None,
        ))
        .unwrap();
        let arts = &portable.statement.predicate.artifacts;
        assert_eq!(arts[0].os, None);
        assert_eq!(arts[0].format.as_deref(), Some("raw"));
        assert_eq!(arts[0].bin, [Bin::named("tool-universal", "tool")]);
        assert_eq!(arts[1].format.as_deref(), Some("raw"));
        assert_eq!(arts[1].bin, [Bin::named("tool-windows-x64", "tool.exe")]);

        // A dotted version leaves a tail after the last dot that is not an
        // extension, so a bare executable named for one is still `raw`.
        let dotted = dir.path().join("tool-v1.2.3-linux-x64");
        std::fs::write(&dotted, b"elf").unwrap();
        let versioned = create(&request(
            vec![input(&dotted, None, &["tool"], &[])],
            None,
            None,
        ))
        .unwrap();
        let arts = &versioned.statement.predicate.artifacts;
        assert_eq!(arts[0].format.as_deref(), Some("raw"));
        assert_eq!(arts[0].bin, [Bin::named("tool-v1.2.3-linux-x64", "tool")]);

        // The same when the version comes last and the tail is a bare
        // number: `3` is no more an extension than `3-linux-x64` was.
        let trailing = dir.path().join("tool-linux-x64-v1.2.3");
        std::fs::write(&trailing, b"elf").unwrap();
        let trailing = create(&request(
            vec![input(&trailing, None, &["tool"], &[])],
            None,
            None,
        ))
        .unwrap();
        let arts = &trailing.statement.predicate.artifacts;
        assert_eq!(arts[0].format.as_deref(), Some("raw"));
        assert_eq!(arts[0].bin, [Bin::named("tool-linux-x64-v1.2.3", "tool")]);
        // Two portable artifacts of one format are as ambiguous as two
        // builds for one platform.
        let other_script = dir.path().join("tool-anywhere");
        std::fs::write(&other_script, b"#!/bin/sh").unwrap();
        let err = create(&request(
            vec![
                ArtifactInput {
                    portable: true,
                    ..ArtifactInput::new(&script)
                },
                ArtifactInput {
                    portable: true,
                    ..ArtifactInput::new(&other_script)
                },
            ],
            None,
            None,
        ))
        .unwrap_err();
        assert!(matches!(err, Error::Ambiguous { .. }), "{err}");

        // A path read from a Windows archive is kept exactly; only the PATH
        // name takes .exe.
        let win_zip = dir.path().join("tool-windows-x64.zip");
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for entry in ["bin/tool", "bin/helper.exe"] {
            use std::io::Write as _;
            writer
                .start_file(entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"x").unwrap();
        }
        std::fs::write(&win_zip, writer.finish().unwrap().into_inner()).unwrap();
        let listed = create(&request(
            vec![input(&win_zip, None, &["tool", "helper"], &[])],
            None,
            None,
        ))
        .unwrap();
        assert_eq!(
            listed.statement.predicate.artifacts[0].bin,
            [
                Bin::named("bin/tool", "tool.exe"),
                Bin::named("bin/helper.exe", "helper.exe")
            ]
        );
        assert_eq!(
            created.statement.subject[0]
                .digest
                .sha512
                .as_ref()
                .map(String::len),
            Some(128)
        );
        assert!(!created.statement.provenance_linked());

        let overridden = create(&request(
            vec![input(&a, Some("darwin"), &[], &[])],
            None,
            None,
        ))
        .unwrap();
        assert_eq!(overridden.statement.predicate.artifacts[0].libc, None);
        let overridden = create(&request(
            vec![input(&b, Some("linux"), &[], &[])],
            None,
            None,
        ))
        .unwrap();
        assert_eq!(
            overridden.statement.predicate.artifacts[0].libc.as_deref(),
            Some("gnu")
        );

        // Two artifacts for one platform need a variant.
        let fips = dir.path().join("tool-fips-v1.0.0-linux-x64.tar.xz");
        std::fs::write(&fips, b"fips bytes").unwrap();
        let err = create(&request(
            vec![input(&a, None, &[], &[]), input(&fips, None, &[], &[])],
            None,
            None,
        ))
        .unwrap_err();
        assert!(matches!(err, Error::Ambiguous { .. }), "{err}");
        assert!(err.to_string().contains("linux/x86_64/gnu/tar.xz"), "{err}");
        let ok = create(&request(
            vec![
                input(&a, None, &[], &[]),
                ArtifactInput {
                    variant: Some("fips".into()),
                    ..ArtifactInput::new(&fips)
                },
            ],
            None,
            None,
        ))
        .unwrap();
        assert_eq!(
            ok.statement.predicate.artifacts[1].variant.as_deref(),
            Some("fips")
        );

        // Sign with the key, unlogged, and verify with the key.
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::verify::Options {
            require_log: false,
            trusted_root: &root,
        };
        let bundle = crate::sigstore::sign(
            Signer::Key {
                key: key.clone(),
                log: false,
            },
            &created.document,
        )
        .unwrap();
        let public = key.public_key();
        let verified =
            crate::verify::verify(&bundle, &Trust::Key(&public), options, &[&a, &b]).unwrap();
        assert_eq!(verified.version, "1.0.0");
        assert_eq!(verified.scheme, Scheme::SigstoreKey);
        assert_eq!(verified.attested_by, Attestor::Vendor);
        assert_eq!(
            verified.checked_artifacts,
            [
                "tool-v1.0.0-linux-x64.tar.xz",
                "tool-v1.0.0-darwin-arm64.tar.xz"
            ]
        );
        assert!(!verified.provenance_linked);
        assert_eq!(verified.logged_at, None);

        // A modified artifact, a wrong key, an unlisted file, and a
        // statement declaring a different signer all fail.
        std::fs::write(&a, b"other bytes").unwrap();
        let err = crate::verify::verify(&bundle, &Trust::Key(&public), options, &[&a]).unwrap_err();
        assert!(err.to_string().contains("sha256 is"), "{err}");
        let other = SecretKey::from_seed([2u8; 32]);
        assert!(
            crate::verify::verify(&bundle, &Trust::Key(&other.public_key()), options, &[]).is_err()
        );
        let unknown = dir.path().join("unknown.tar.gz");
        std::fs::write(&unknown, b"").unwrap();
        let err =
            crate::verify::verify(&bundle, &Trust::Key(&public), options, &[&unknown]).unwrap_err();
        assert!(err.to_string().contains("not listed"), "{err}");
        let mut lying = created.statement.clone();
        lying.predicate.identity = key_identity(&other);
        let lying_bundle = crate::sigstore::sign(
            Signer::Key {
                key: key.clone(),
                log: false,
            },
            &lying.canonical_bytes(),
        )
        .unwrap();
        let err =
            crate::verify::verify(&lying_bundle, &Trust::Key(&public), options, &[]).unwrap_err();
        assert!(
            matches!(err, crate::verify::Error::DeclaredSignerMismatch { .. }),
            "{err}"
        );

        // A repackager document says so and lists its evidence.
        let repack = create(&Request {
            attested_by: Attestor::Repackager,
            evidence: vec![Evidence {
                kind: "pkgbuild-checksums".into(),
                detail: None,
            }],
            ..request(vec![input(&b, None, &[], &[])], None, None)
        })
        .unwrap();
        assert!(
            String::from_utf8_lossy(&repack.document).contains(r#""attested_by":"repackager""#)
        );
        let repack_bundle = crate::sigstore::sign(
            Signer::Key {
                key: key.clone(),
                log: false,
            },
            &repack.document,
        )
        .unwrap();
        let verified =
            crate::verify::verify(&repack_bundle, &Trust::Key(&public), options, &[]).unwrap();
        assert_eq!(verified.attested_by, Attestor::Repackager);

        // A release list over the bundle: a yanked, security-flagged entry.
        let bundle_path = dir.path().join("packslip.sigstore.json");
        std::fs::write(&bundle_path, &bundle).unwrap();
        let list = create_release_list(&ListRequest {
            project: "tool.example.com",
            generated_at: Some("2026-09-01T01:00:00Z"),
            valid_for: std::time::Duration::from_secs(30 * 86_400),
            sequence: 3,
            latest: Some("1.0.0"),
            releases: vec![ListedRelease {
                url: "https://dl.example.com/1.0.0/packslip.sigstore.json",
                bundle_path: &bundle_path,
                yanked: Some("bad build".into()),
                security: true,
                evidence: vec![],
            }],
            identity: key_identity(&key),
        })
        .unwrap();
        assert_eq!(list.statement.predicate.expires_at, "2026-10-01T01:00:00Z");
        assert_eq!(list.statement.predicate.latest.as_deref(), Some("1.0.0"));
        let entry = &list.statement.predicate.releases[0];
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.tag.as_deref(), Some("v1.0.0"));
        assert!(entry.is_yanked());
        assert_eq!(entry.status_reason.as_deref(), Some("bad build"));
        assert!(entry.security);
        assert_eq!(
            list.statement.subject[0].name,
            "https://dl.example.com/1.0.0/packslip.sigstore.json"
        );
        let list_bundle = crate::sigstore::sign(
            Signer::Key {
                key: key.clone(),
                log: false,
            },
            &list.document,
        )
        .unwrap();
        let verified_list =
            crate::verify::verify_release_list(&list_bundle, &Trust::Key(&public), options)
                .unwrap();
        assert_eq!(verified_list.list.predicate.sequence, 3);
        assert_eq!(
            verified_list.list.predicate.latest.as_deref(),
            Some("1.0.0")
        );
        assert!(
            verified_list
                .list
                .is_current("2026-09-15T00:00:00Z".parse().unwrap())
        );
        // Two bundles carrying the same version are a duplicate release.
        let err = create_release_list(&ListRequest {
            project: "tool.example.com",
            generated_at: None,
            valid_for: std::time::Duration::from_secs(60),
            sequence: 4,
            latest: None,
            releases: vec![
                ListedRelease {
                    url: "https://x/a.sigstore.json",
                    bundle_path: &bundle_path,
                    yanked: None,
                    security: false,
                    evidence: vec![],
                },
                ListedRelease {
                    url: "https://x/b.sigstore.json",
                    bundle_path: &bundle_path,
                    yanked: None,
                    security: false,
                    evidence: vec![],
                },
            ],
            identity: key_identity(&key),
        })
        .unwrap_err();
        assert!(err.to_string().contains("appears more than once"), "{err}");
        let err = create_release_list(&ListRequest {
            project: "other.example.com",
            generated_at: None,
            valid_for: std::time::Duration::from_secs(60),
            sequence: 1,
            latest: None,
            releases: vec![ListedRelease {
                url: "https://x/packslip.sigstore.json",
                bundle_path: &bundle_path,
                yanked: None,
                security: false,
                evidence: vec![],
            }],
            identity: key_identity(&key),
        })
        .unwrap_err();
        assert!(err.to_string().contains("the list is for"), "{err}");
    }
}
