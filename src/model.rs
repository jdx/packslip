//! The packslip documents: in-toto statements whose predicates say what a
//! release shipped (`release/v1`) and which releases a project has
//! (`releases/v1`). See `docs/spec/packslip.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
pub const PREDICATE_TYPE: &str = "https://packslip.dev/release/v1";
pub const RELEASES_PREDICATE_TYPE: &str = "https://packslip.dev/releases/v1";

/// What the specification has no field for, keyed by who defines it: a
/// consumer by its name (`mise`), a vendor by a domain it controls
/// (`example.com`). packslip assigns no meaning to anything inside, so a
/// key here never collides with a field a later revision adds. The
/// signature covers it like everything else.
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// An in-toto statement carrying predicate `P`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Envelope<P> {
    #[serde(rename = "_type")]
    pub kind: String,
    /// What the predicate is about, by name and digest.
    pub subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: P,
}

/// A release: the packslip proper.
pub type Statement = Envelope<Predicate>;

/// A project's recent releases, for discovery.
pub type ReleaseListStatement = Envelope<ReleaseList>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Subject {
    pub name: String,
    pub digest: Digest,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Digest {
    /// Lowercase hex.
    #[schemars(regex(pattern = r"^[0-9a-f]{64}$"))]
    pub sha256: String,
    /// Lowercase hex, for consumers that want it (electron-updater,
    /// Balrog, Scoop).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = r"^[0-9a-f]{128}$"))]
    pub sha512: Option<String>,
}

/// Semver 2.0.0, as its specification spells the grammar.
pub const SEMVER_PATTERN: &str = r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$";

/// The shape of an `os`, `arch`, `libc`, `format`, or `variant` value: a
/// lowercase word of letters, digits, `_`, `-`, and `.`, starting with a
/// letter or digit. The documented vocabularies below are the values
/// consumers know; a value outside them is well-formed but matches no
/// host and unpacks with nothing, so a vendor uses one only for a
/// platform or format the specification has not named yet.
pub const TOKEN_PATTERN: &str = r"^[a-z0-9][a-z0-9_.-]*$";

/// Documented `os` values, after Rust's target triples.
pub const OS_VALUES: &[&str] = &[
    "linux", "darwin", "windows", "freebsd", "netbsd", "openbsd", "illumos", "android", "ios",
];

/// Documented `arch` values, after Rust's target triples.
pub const ARCH_VALUES: &[&str] = &[
    "x86_64",
    "aarch64",
    "armv7",
    "armv6",
    "riscv64",
    "i686",
    "powerpc64le",
    "s390x",
    "loongarch64",
];

/// Documented `libc` values, for Linux builds.
pub const LIBC_VALUES: &[&str] = &["gnu", "musl"];

/// Documented `format` values: archives, then single compressed
/// executables, then installers, then a bare executable.
pub const FORMAT_VALUES: &[&str] = &[
    "tar.xz", "tar.gz", "tar.zst", "tar.bz2", "tgz", "tar", "zip", "7z", "gz", "xz", "zst", "bz2",
    "deb", "rpm", "dmg", "pkg", "msi", "msix", "exe", "appimage", "raw",
];

/// Formats whose artifact is one executable rather than an archive: `raw`
/// is the file itself, the others are that file compressed. Their `bin`
/// names the artifact, without any compression suffix.
pub const BARE_FORMATS: &[&str] = &["raw", "gz", "xz", "zst", "bz2"];

/// Whether `format` names a single executable rather than an archive or
/// installer.
pub fn is_bare_format(format: &str) -> bool {
    BARE_FORMATS.contains(&format)
}

/// The file inside a bare-format artifact: the artifact's own name, minus
/// the compression suffix for `gz`, `xz`, `zst`, and `bz2`.
pub fn bare_file_name<'a>(artifact_name: &'a str, format: &str) -> &'a str {
    match format {
        "raw" => artifact_name,
        compressed if is_bare_format(compressed) => artifact_name
            .strip_suffix(&format!(".{compressed}"))
            .unwrap_or(artifact_name),
        _ => artifact_name,
    }
}

/// Whether a value has the shape [`TOKEN_PATTERN`] describes.
pub fn valid_token(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes
        .first()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Predicate {
    /// The project's name: a host path such as `github.com/jdx/mise` or
    /// `mise.jdx.dev`. The host is where a
    /// consumer discovers releases and, for forge hosts, what identity is
    /// expected to have signed them. A tool in a monorepo adds a subpath:
    /// `github.com/oxc-project/oxc/oxlint`.
    pub project: String,
    /// Semver 2.0.0 (calver such as `2026.9.1` qualifies). Its prerelease
    /// part, if any, marks a prerelease, and the first identifier of that
    /// part names the channel: see [`channel`]. On a forge, the release
    /// tag names this version: see [`tag_version`].
    #[schemars(regex(pattern = SEMVER_PATTERN))]
    pub version: String,
    /// RFC 3339 UTC.
    #[schemars(extend("format" = "date-time"))]
    pub published_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    pub artifacts: Vec<Artifact>,
    /// What ships beyond the executables: completions, man pages, CLI
    /// specs, skills, desktop entries, icons, app bundles. Each names its
    /// kind and one source. See [`Resource`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<Resource>,
    pub identity: Identity,
    /// Who is making the claim: the vendor itself, or a repackager that
    /// checked the vendor's evidence and signed a document about it.
    #[serde(default, skip_serializing_if = "Attestor::is_vendor")]
    pub attested_by: Attestor,
    /// What a repackager checked before signing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_url: Option<String>,
    /// Vendor- or consumer-defined data about the release. See
    /// [`Extensions`].
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

/// Who signed the claim.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Attestor {
    /// The project's own publisher.
    #[default]
    Vendor,
    /// A repository or mirror describing a vendor's artifacts on the
    /// vendor's behalf, having checked whatever evidence the vendor gave.
    Repackager,
}

impl Attestor {
    pub fn is_vendor(&self) -> bool {
        *self == Attestor::Vendor
    }
}

impl std::fmt::Display for Attestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Attestor::Vendor => "vendor",
            Attestor::Repackager => "repackager",
        })
    }
}

/// Parse a version as packslip requires: semver 2.0.0, so that precedence
/// ranks releases and the prerelease part says the rest.
pub fn parse_version(version: &str) -> Result<semver::Version, InvalidDocument> {
    if version.is_empty() {
        return Err(InvalidDocument::Version);
    }
    semver::Version::parse(version).map_err(|_| InvalidDocument::NotSemver(version.to_string()))
}

/// The channel a version is on: the first identifier of its prerelease
/// part, when that is not a number. `1.3.0-nightly.20260904` is on
/// `nightly` and `1.2.0-beta.2` on `beta`; `1.2.0` and `1.2.0-1` are on
/// none.
pub fn channel(version: &semver::Version) -> Option<&str> {
    let first = version.pre.as_str().split('.').next()?;
    (!first.is_empty() && !first.bytes().all(|b| b.is_ascii_digit())).then_some(first)
}

/// The version a forge release tag names, if it names one: the version
/// itself, optionally after a `v`, and optionally after a prefix that is
/// the tool's subpath, the last segment of it, or the repository name,
/// followed by `/`, `-`, `_`, or `@`. `v1.2.3` names `1.2.3`;
/// `oxlint_v1.0.0` names `1.0.0` for `github.com/oxc-project/oxc/oxlint`;
/// `jq-1.7.1` names `1.7.1` for `github.com/jqlang/jq`; `cli/v1.9.4`
/// names `1.9.4` for `github.com/biomejs/biome/crates/cli`.
///
/// This is how a consumer lists a forge project's versions without
/// downloading a bundle per release. The packslip inside the release is
/// the authority: its `version` must equal what the tag names, or the
/// consumer refuses it. A tag that names no version is skipped, and the
/// release is reachable only through a signed release list.
pub fn tag_version(tag: &str, project: &str) -> Option<String> {
    let mut rest = tag;
    let mut prefixes: Vec<&str> = Vec::new();
    if let Some(sub) = repository_subpath(project) {
        prefixes.push(sub);
        if let Some(last) = sub.rsplit('/').next()
            && last != sub
        {
            prefixes.push(last);
        }
    }
    if let Some((_, _, repo)) = repository(project) {
        prefixes.push(repo);
    }
    'strip: for prefix in prefixes {
        for sep in ['/', '-', '_', '@'] {
            if let Some(after) = rest.strip_prefix(prefix)
                && let Some(after) = after.strip_prefix(sep)
            {
                rest = after;
                break 'strip;
            }
        }
    }
    let version = normalize_version(rest.strip_prefix('v').unwrap_or(rest))?;
    parse_version(&version).ok()?;
    Some(version)
}

/// The semver spelling of a version a vendor writes loosely: a missing
/// patch component becomes `.0` (`4.1` is `4.1.0`) and leading zeros go
/// (`25.07.1` is `25.7.1`). A date is calver already once its zeros go
/// (`2026.08.31` is `2026.8.31`); one with dashes, `2026-08-31`, has to be
/// respelled by the vendor. Prerelease and build parts pass through.
pub fn normalize_version(text: &str) -> Option<String> {
    let (core, tail) = match text.find(['-', '+']) {
        Some(i) => (&text[..i], &text[i..]),
        None => (text, ""),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut numbers: Vec<&str> = parts
        .iter()
        .map(|p| {
            let trimmed = p.trim_start_matches('0');
            if trimmed.is_empty() { "0" } else { trimmed }
        })
        .collect();
    if numbers.len() == 2 {
        numbers.push("0");
    }
    Some(format!("{}{tail}", numbers.join(".")))
}

/// Whether a resource applies to an artifact: each of the resource's
/// `os`, `arch`, and `libc` is absent or equal to the artifact's. A
/// consumer choosing among entries for the same thing takes the one
/// naming the most of those fields; [`select_resources`] does that.
pub fn resource_fits(resource: &Resource, artifact: &Artifact) -> bool {
    [
        (&resource.os, &artifact.os),
        (&resource.arch, &artifact.arch),
        (&resource.libc, &artifact.libc),
    ]
    .into_iter()
    .all(|(scope, value)| scope.is_none() || scope == value)
}

/// The resources to use with one artifact, as the specification's
/// Resources section says: for each thing the entries describe (see
/// [`Resource::identities`]), those that fit the artifact and, of them,
/// the most specific. Entries for different things never hide one
/// another. Document order is kept, and an entry that describes several
/// things, such as an `exec` completion for several shells, appears
/// once.
pub fn select_resources<'a>(statement: &'a Statement, artifact: &Artifact) -> Vec<&'a Resource> {
    let sole_bin = statement.sole_bin();
    select_among(&statement.predicate.resources, artifact, |r| {
        r.identities_for(sole_bin)
    })
}

/// [`select_resources`] over a slice, with the identities of each entry
/// given by `identities_of`.
fn select_among<'a>(
    resources: &'a [Resource],
    artifact: &Artifact,
    identities_of: impl Fn(&Resource) -> Vec<ResourceIdentity>,
) -> Vec<&'a Resource> {
    let specificity = |r: &Resource| {
        [&r.os, &r.arch, &r.libc]
            .iter()
            .filter(|f| f.is_some())
            .count()
    };
    let identities: Vec<Vec<ResourceIdentity>> = resources.iter().map(&identities_of).collect();
    let mut keep = vec![false; resources.len()];
    let mut seen = std::collections::BTreeSet::new();
    for own in &identities {
        for identity in own {
            if !seen.insert(identity.clone()) {
                continue;
            }
            let fitting: Vec<usize> = resources
                .iter()
                .enumerate()
                .filter(|(i, r)| identities[*i].contains(identity) && resource_fits(r, artifact))
                .map(|(i, _)| i)
                .collect();
            let best = fitting
                .iter()
                .map(|&i| specificity(&resources[i]))
                .max()
                .unwrap_or(0);
            for i in fitting {
                if specificity(&resources[i]) == best {
                    keep[i] = true;
                }
            }
        }
    }
    resources
        .iter()
        .zip(keep)
        .filter_map(|(r, keep)| keep.then_some(r))
        .collect()
}

/// This host, in the packslip's vocabulary, for [`select_artifact`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host<'a> {
    pub os: &'a str,
    pub arch: &'a str,
    /// The C library a Linux host reports, if the consumer knows it.
    pub libc: Option<&'a str>,
}

/// Why [`select_artifact`] found nothing to install.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Selection {
    #[error("no artifact fits this host")]
    NoMatch,
    #[error("artifacts {0:?} and {1:?} both fit this host and nothing tells them apart")]
    Ambiguous(String, String),
}

/// The one artifact for a host, as the specification's consumer rules
/// say. `formats` is what the consumer can unpack, best first.
///
/// An artifact fits when each of its `os`, `arch`, and `libc` is either
/// absent or equal to the host's; when its `variant` is the one asked
/// for, or absent when none was; and when its `format` is one the
/// consumer handles. Among those that fit, the one naming the most of
/// `os`, `arch`, and `libc` wins, so a build for the host beats a
/// portable one; among equally specific artifacts the consumer's format
/// preference decides. Two artifacts that still tie are a vendor error
/// and the consumer refuses to guess.
pub fn select_artifact<'a>(
    artifacts: &'a [Artifact],
    host: &Host<'_>,
    variant: Option<&str>,
    formats: &[&str],
) -> Result<&'a Artifact, Selection> {
    let fits = |value: Option<&str>, wanted: Option<&str>| match (value, wanted) {
        (None, _) => true,
        (Some(v), Some(w)) => v == w,
        (Some(_), None) => false,
    };
    let rank = |artifact: &Artifact| {
        let specificity = [&artifact.os, &artifact.arch, &artifact.libc]
            .into_iter()
            .filter(|field| field.is_some())
            .count();
        let format = artifact
            .format
            .as_deref()
            .and_then(|f| formats.iter().position(|known| *known == f));
        format.map(|f| (specificity, f))
    };
    let mut best: Option<(&Artifact, (usize, usize))> = None;
    let mut tied: Option<&Artifact> = None;
    for artifact in artifacts {
        if !fits(artifact.os.as_deref(), Some(host.os))
            || !fits(artifact.arch.as_deref(), Some(host.arch))
            || !fits(artifact.libc.as_deref(), host.libc)
            || artifact.variant.as_deref() != variant
        {
            continue;
        }
        let Some((specificity, format)) = rank(artifact) else {
            continue;
        };
        // Higher specificity first, then the lower format index.
        let key = (specificity, formats.len() - format);
        match best {
            Some((_, current)) if key < current => {}
            Some((_, current)) if key == current => tied = Some(artifact),
            _ => {
                best = Some((artifact, key));
                tied = None;
            }
        }
    }
    match (best, tied) {
        (Some((chosen, _)), None) => Ok(chosen),
        (Some((chosen, _)), Some(other)) => Err(Selection::Ambiguous(
            chosen.name.clone(),
            other.name.clone(),
        )),
        (None, _) => Err(Selection::NoMatch),
    }
}

/// One thing a repackager or a release list's publisher checked.
/// Documented kinds: `pkgbuild-checksums`, `checksum-file-over-tls`,
/// `apt-release-gpg`, `vendor-signature`, `vendor-packslip`,
/// `github-attestation`, `provenance-verified`, `scan`, `none`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub kind: String,
    /// A key id, URL, or note that lets a reader check the claim: the
    /// vendor packslip's digest, a scan report's URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    pub name: String,
    /// `linux`, `darwin`, `windows`, `freebsd`, `netbsd`, `openbsd`,
    /// `illumos`, `android`, `ios`. Absent when the artifact runs on any
    /// OS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub os: Option<String>,
    /// `x86_64`, `aarch64`, `armv7`, `armv6`, `riscv64`, `i686`,
    /// `powerpc64le`, `s390x`, `loongarch64`. Absent when the artifact
    /// runs on any architecture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub arch: Option<String>,
    /// `gnu` or `musl`, for a Linux build. Absent when the artifact does
    /// not depend on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub libc: Option<String>,
    /// Tells apart builds that share os, arch, and libc: `fips`,
    /// `baseline`, `debug`, `installer`, `source`. A consumer selects only
    /// artifacts without a variant unless asked for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub variant: Option<String>,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The archive type (`tar.xz`, `tar.gz`, `tar.zst`, `tar.bz2`, `tgz`,
    /// `tar`, `zip`, `7z`), a single compressed executable (`gz`, `xz`,
    /// `zst`, `bz2`), an installer (`deb`, `rpm`, `dmg`, `pkg`, `msi`,
    /// `msix`, `exe`, `appimage`), or `raw` for a bare executable. Two
    /// artifacts that differ only in format carry the same build, and a
    /// consumer takes whichever it prefers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub format: Option<String>,
    /// Executables inside the artifact, as paths relative to the archive
    /// root, or the artifact's own name (minus any compression suffix)
    /// when it is a bare executable. A consumer puts these on PATH under
    /// `name`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin: Vec<Bin>,
    /// What the artifact needs from the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Requires>,
    /// URLs of build provenance statements (SLSA) for this artifact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<String>,
    /// Vendor- or consumer-defined data about this artifact. See
    /// [`Extensions`].
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

/// An executable inside an artifact. Serialises as the bare path when the
/// PATH name is the file's own name, else as `{ "path", "name" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(from = "BinRepr", into = "BinRepr")]
pub struct Bin {
    /// Path inside the archive, relative to its root.
    pub path: String,
    /// The name to put on PATH.
    pub name: String,
}

impl Bin {
    pub fn new(path: impl Into<String>) -> Bin {
        let path = path.into();
        Bin {
            name: file_name(&path).to_string(),
            path,
        }
    }

    pub fn named(path: impl Into<String>, name: impl Into<String>) -> Bin {
        Bin {
            path: path.into(),
            name: name.into(),
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum BinRepr {
    Path(String),
    Named { path: String, name: String },
}

impl From<BinRepr> for Bin {
    fn from(repr: BinRepr) -> Bin {
        match repr {
            BinRepr::Path(path) => Bin::new(path),
            BinRepr::Named { path, name } => Bin { path, name },
        }
    }
}

impl From<Bin> for BinRepr {
    fn from(bin: Bin) -> BinRepr {
        if bin.name == file_name(&bin.path) {
            BinRepr::Path(bin.path)
        } else {
            BinRepr::Named {
                path: bin.path,
                name: bin.name,
            }
        }
    }
}

/// What the host must already provide, by names the operating system
/// defines, so a consumer can check before installing. Nothing here names
/// another project or where to get it; see the specification's Host
/// requirements section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Requires {
    /// Minimum OS version, in the OS's own terms: `12` for macOS Monterey,
    /// `10.0.17763` for Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_min: Option<String>,
    /// Minimum glibc for a `gnu` Linux build, such as `2.31`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glibc_min: Option<String>,
    /// Shared libraries the executables load from the host, by the name
    /// the loader resolves: a soname (`libssl.so.3`), a DLL name
    /// (`vcruntime140.dll`), or a dylib name (`libssl.3.dylib`). Excludes
    /// the C runtime baseline that `libc` and `glibc_min` cover and any
    /// library the artifact ships itself. `packslip create` reads it from
    /// the executables; an empty list means they were read and need
    /// nothing, an absent one that nothing was checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libs: Option<Vec<String>>,
    /// Commands the executables run and cannot work without, by the bare
    /// name on PATH, with an optional minimum version. Declared by the
    /// vendor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin: Vec<RequiredBin>,
}

impl Requires {
    pub fn is_empty(&self) -> bool {
        self.os_min.is_none()
            && self.glibc_min.is_none()
            && self.libs.is_none()
            && self.bin.is_empty()
    }

    /// One line for a report: `os>=12; glibc>=2.31; libs libz.so.1; bin
    /// java>=17`, with `libs none` for executables that were read and
    /// need nothing.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(os) = &self.os_min {
            parts.push(format!("os>={os}"));
        }
        if let Some(glibc) = &self.glibc_min {
            parts.push(format!("glibc>={glibc}"));
        }
        match &self.libs {
            Some(libs) if libs.is_empty() => parts.push("libs none".to_string()),
            Some(libs) => parts.push(format!("libs {}", libs.join(" "))),
            None => {}
        }
        if !self.bin.is_empty() {
            let bins: Vec<String> = self.bin.iter().map(|b| b.to_string()).collect();
            parts.push(format!("bin {}", bins.join(" ")));
        }
        parts.join("; ")
    }
}

/// A command an executable needs on PATH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RequiredBin {
    /// The name as the executable invokes it, bare: `java`, `python3`,
    /// `git`. No directory and no `.exe`.
    pub name: String,
    /// The lowest version that works, matched as a prefix on
    /// dot-separated components like a requested version: `17` means
    /// 17.0.0 and later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
}

impl RequiredBin {
    pub fn new(name: impl Into<String>) -> RequiredBin {
        RequiredBin {
            name: name.into(),
            min: None,
        }
    }

    pub fn at_least(name: impl Into<String>, min: impl Into<String>) -> RequiredBin {
        RequiredBin {
            name: name.into(),
            min: Some(min.into()),
        }
    }
}

impl std::fmt::Display for RequiredBin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.min {
            Some(min) => write!(f, "{}>={min}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// A shared library name as a loader resolves it: one path segment, no
/// whitespace.
fn valid_lib_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\'])
        && !name
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
}

/// A command name as typed at a shell: one segment, no `.exe`.
fn valid_required_bin_name(name: &str) -> bool {
    valid_lib_name(name) && !name.to_ascii_lowercase().ends_with(".exe")
}

/// A minimum version: dot-separated components starting with a digit.
fn valid_min_version(min: &str) -> bool {
    min.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && min
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

/// Something the release ships besides its executables, and where to get
/// it. The kind says what it is; exactly one of `archive`, `asset`,
/// `repo`, and `exec` says where it comes from. Documented kinds:
/// `completion`, `man`, `cli-spec`, `skill`, `desktop`, `icon`, `app`,
/// `sbom`. Consumers ignore kinds they do not know.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Resource {
    pub kind: String,
    /// Limits the entry to artifacts of this `os`, when layouts differ by
    /// platform. Absent means any artifact. See [`resource_fits`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub os: Option<String>,
    /// Likewise for `arch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub arch: Option<String>,
    /// Likewise for `libc`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = TOKEN_PATTERN))]
    pub libc: Option<String>,
    /// For `completion` with a static source: the shell, such as `bash`,
    /// `zsh`, `fish`, `powershell`, `nushell`, `elvish`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// For `completion` with an `exec` source: every shell the command
    /// generates, substituted for `{shell}` in the argv.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shells: Vec<String>,
    /// For `skill`: the skill's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The executable this entry is for, by its `bin` name. Required for
    /// `cli-spec`; for `completion` and `man`, required when the release
    /// has more than one executable, and meaning that one when it has
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    /// For `cli-spec`: the spec format, of which `usage` is documented.
    /// For `sbom`: `cyclonedx` or `spdx`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// A path inside the selected artifact, relative to the archive root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
    /// The name of a separate release file, listed in `subject` with its
    /// digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Download URL of the asset, when the source is `asset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A path in the source repository at `source.commit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// A command whose stdout is the file: an argv whose first element is
    /// a `bin` name. A consumer may decline to run it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exec: Vec<String>,
    /// Vendor- or consumer-defined data about this resource. See
    /// [`Extensions`].
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

/// Where a resource comes from, in order of how much a consumer can
/// verify about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceSource {
    Archive,
    Asset,
    Repo,
    Exec,
}

impl std::fmt::Display for ResourceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ResourceSource::Archive => "archive",
            ResourceSource::Asset => "asset",
            ResourceSource::Repo => "repo",
            ResourceSource::Exec => "exec",
        })
    }
}

/// What a resource is an entry for. Entries with equal identities compete
/// over platform scope and entries with different ones never do; see
/// [`Resource::identities`] and [`select_resources`]. Structured rather
/// than joined into one string, so a kind of the vendor's own containing
/// a slash cannot collide with a documented kind's parts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceIdentity {
    pub kind: String,
    /// The naming fields of the kind, in the order Resources gives them.
    pub parts: Vec<String>,
}

impl std::fmt::Display for ResourceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.kind)?;
        for part in &self.parts {
            write!(f, "/{part}")?;
        }
        Ok(())
    }
}

impl Resource {
    /// A resource of `kind` with no source or qualifiers yet.
    pub fn new(kind: impl Into<String>) -> Resource {
        Resource {
            kind: kind.into(),
            ..Resource::default()
        }
    }

    /// The one source this resource declares, or `None` when it declares
    /// none or several.
    pub fn source(&self) -> Option<ResourceSource> {
        let mut sources = Vec::new();
        if self.archive.is_some() {
            sources.push(ResourceSource::Archive);
        }
        if self.asset.is_some() {
            sources.push(ResourceSource::Asset);
        }
        if self.repo.is_some() {
            sources.push(ResourceSource::Repo);
        }
        if !self.exec.is_empty() {
            sources.push(ResourceSource::Exec);
        }
        match sources.as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }

    /// What this entry is an entry for, so that only entries for the same
    /// thing compete over platform scope: `bin` and the shell of a
    /// completion (one per shell an `exec` entry generates), `format` and
    /// `bin` of a cli-spec, the name of a skill, the format of an SBOM,
    /// and for every other kind `bin`, if any, and the file name of the
    /// source, or nothing for an `exec` source. A completion or man page
    /// that leaves `bin` out is for the release's only executable; see
    /// [`Resource::identities_for`], which [`select_resources`] uses.
    pub fn identities(&self) -> Vec<ResourceIdentity> {
        self.identities_for(None)
    }

    /// [`Resource::identities`] in a release whose only executable is
    /// `sole_bin`: a completion or man page that leaves `bin` out is for
    /// that one, and competes with an entry that names it.
    pub fn identities_for(&self, sole_bin: Option<&str>) -> Vec<ResourceIdentity> {
        // The bare name, as `sole_bin` and validation compare it, so an
        // entry naming the Windows form is for the same executable.
        let bin = match self.kind.as_str() {
            "cli-spec" | "skill" | "sbom" => None,
            _ => self
                .bin
                .as_deref()
                .or(sole_bin)
                .map(|b| b.strip_suffix(".exe").unwrap_or(b)),
        };
        let id = |parts: Vec<&str>| ResourceIdentity {
            kind: self.kind.clone(),
            parts: bin.into_iter().chain(parts).map(str::to_string).collect(),
        };
        match self.kind.as_str() {
            "completion" => {
                let shells: Vec<&str> = self
                    .shell
                    .iter()
                    .chain(&self.shells)
                    .map(String::as_str)
                    .collect();
                if shells.is_empty() {
                    vec![id(vec![])]
                } else {
                    shells.into_iter().map(|s| id(vec![s])).collect()
                }
            }
            "cli-spec" => vec![id(vec![
                self.format.as_deref().unwrap_or_default(),
                self.bin.as_deref().unwrap_or_default(),
            ])],
            "skill" => vec![id(vec![self.name.as_deref().unwrap_or_default()])],
            "sbom" => vec![id(vec![self.format.as_deref().unwrap_or_default()])],
            _ => {
                let path = self
                    .archive
                    .as_deref()
                    .or(self.asset.as_deref())
                    .or(self.repo.as_deref());
                match path {
                    Some(path) => vec![id(vec![file_name(path)])],
                    None => vec![id(vec![])],
                }
            }
        }
    }

    /// A short label for messages: `completion/zsh`, `skill/mise`,
    /// `cli-spec/usage/mise`, `man`.
    pub fn label(&self) -> String {
        let mut label = self.kind.clone();
        for part in [
            self.format.as_deref(),
            self.bin.as_deref(),
            self.shell.as_deref(),
            self.name.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            label.push('/');
            label.push_str(part);
        }
        if !self.shells.is_empty() {
            label.push('/');
            label.push_str(&self.shells.join(","));
        }
        label
    }
}

/// A shell name is a plain lowercase word: `bash`, `zsh`, `powershell`.
fn valid_shell(shell: &str) -> bool {
    !shell.is_empty()
        && shell
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A path inside an archive or a repository: relative, with no empty or
/// `..` segments.
fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "..")
}

/// How the document is signed, so a consumer can check what it pinned
/// against what it received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Identity {
    pub scheme: Scheme,
    /// For `sigstore-oidc`, the certificate's subject identity: the
    /// workflow URI for a CI identity, or the email for a human one. For
    /// `sigstore-key`, the key id in uppercase hex.
    pub key_id: String,
    /// For `sigstore-oidc`, the OIDC issuer that vouched for the identity,
    /// such as `https://token.actions.githubusercontent.com`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme {
    /// A sigstore bundle signed keylessly with a workload or human OIDC
    /// identity, certified by Fulcio and logged to Rekor.
    SigstoreOidc,
    /// A sigstore bundle signed with a long-lived Ed25519 key, logged to
    /// Rekor.
    SigstoreKey,
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Scheme::SigstoreOidc => "sigstore-oidc",
            Scheme::SigstoreKey => "sigstore-key",
        })
    }
}

/// The `releases/v1` predicate: which releases a project has, so a
/// consumer can find them without a registry, and cannot be shown a stale
/// or truncated view without noticing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseList {
    pub project: String,
    /// When the list was produced, RFC 3339 UTC.
    pub generated_at: String,
    /// After this the list is stale and a consumer refuses it, RFC 3339 UTC.
    pub expires_at: String,
    /// Increases with every list published; a consumer refuses a lower one
    /// than it has seen.
    pub sequence: u64,
    pub identity: Identity,
    /// In any order; consumers rank by semver precedence.
    pub releases: Vec<ReleaseRef>,
    /// Vendor- or consumer-defined data about the list. See
    /// [`Extensions`].
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseRef {
    #[schemars(regex(pattern = SEMVER_PATTERN))]
    pub version: String,
    /// The vendor's own spelling of the release, copied from the packslip's
    /// `source.tag`, so a consumer can accept a request in either form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// RFC 3339 UTC, copied from the release's packslip.
    #[schemars(extend("format" = "date-time"))]
    pub published_at: String,
    /// URL of the release's `packslip.sigstore.json`. The statement's
    /// subject of the same name carries that file's digest.
    pub packslip: String,
    /// Set when the vendor withdrew the release. Consumers never select a
    /// yanked release and warn when they hold one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ReleaseStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    /// The release fixes a vulnerability; a consumer's minimum release age
    /// may shorten for it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub security: bool,
    /// Vendor- or consumer-defined data about this release. See
    /// [`Extensions`].
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
    /// What the list's publisher checked about this release beyond the
    /// vendor's own document: a scan, verified provenance. Empty for a
    /// vendor's own list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseStatus {
    Yanked,
}

impl ReleaseRef {
    pub fn is_yanked(&self) -> bool {
        self.status == Some(ReleaseStatus::Yanked)
    }
}

/// Why a document is malformed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvalidDocument {
    #[error("_type must be {STATEMENT_TYPE}, got {0:?}")]
    Kind(String),
    #[error("predicateType must be {expected}, got {actual:?}")]
    PredicateType {
        expected: &'static str,
        actual: String,
    },
    #[error(
        "project must be a host path such as github.com/owner/repo or tool.example.com, got {0:?}"
    )]
    Project(String),
    #[error("version must not be empty")]
    Version,
    #[error(
        "version must be semver 2.0.0 (MAJOR.MINOR.PATCH, with an optional prerelease part), got {0:?}"
    )]
    NotSemver(String),
    #[error("{field} must be RFC 3339, got {value:?}")]
    Timestamp { field: &'static str, value: String },
    #[error("artifact {0:?} appears more than once")]
    DuplicateArtifact(String),
    #[error("subject {0:?} appears more than once")]
    DuplicateSubject(String),
    #[error("subject {0:?} has no matching artifact")]
    OrphanSubject(String),
    #[error("artifact {0:?} has no matching subject")]
    OrphanArtifact(String),
    #[error("sha256 for {0:?} must be 64 lowercase hex characters")]
    Sha256(String),
    #[error("sha512 for {0:?} must be 128 lowercase hex characters")]
    Sha512(String),
    #[error("artifact {0:?} has an executable with an empty path")]
    BinPath(String),
    #[error("artifact {0:?} has an executable name containing a slash: {1:?}")]
    BinName(String, String),
    #[error(
        "artifact {0:?} requires library {1:?}; a library is named as its loader resolves it, such as libssl.so.3"
    )]
    RequiredLib(String, String),
    #[error("artifact {0:?} requires library {1:?} twice")]
    DuplicateRequiredLib(String, String),
    #[error("artifact {0:?} requires command {1:?}; a command is a bare name such as java")]
    RequiredBinName(String, String),
    #[error(
        "artifact {0:?} requires command {1:?} with minimum {2:?}; a minimum starts with a digit, as in 17 or 3.12"
    )]
    RequiredBinMin(String, String, String),
    #[error("artifact {0:?} requires command {1:?} twice")]
    DuplicateRequiredBin(String, String),
    #[error("artifact {0:?} requires command {1:?}, which the release itself provides")]
    RequiredBinIsOwn(String, String),
    #[error(
        "artifact {artifact:?} is a bare executable, so its bin must be {expected:?}, not {path:?}"
    )]
    BareBin {
        artifact: String,
        expected: String,
        path: String,
    },
    #[error(
        "artifact {0:?} has {1} {2:?}; a value is a lowercase word of letters, digits, _, -, and ."
    )]
    Token(String, &'static str, String),
    #[error(
        "artifacts {0:?} and {1:?} describe the same os, arch, libc, variant, and format; give one a variant"
    )]
    AmbiguousArtifacts(String, String),
    #[error("at least one artifact is required")]
    NoArtifacts,
    #[error("a resource has an empty kind")]
    ResourceKind,
    #[error("resource {0:?} must have exactly one of archive, asset, repo, exec")]
    ResourceSource(String),
    #[error("resource {0:?}: {1} path must be relative with no empty or .. segments")]
    ResourcePath(String, ResourceSource),
    #[error("resource {0:?} names asset {1:?}, which is not a subject")]
    OrphanAsset(String, String),
    #[error("resource {0:?} has a url but its source is not an asset")]
    ResourceUrl(String),
    #[error("resource {0:?} comes from the repository, which needs source.commit")]
    RepoNeedsCommit(String),
    #[error("resource {0:?} runs {1:?}, which is not a bin name of any artifact")]
    ExecNotABin(String, String),
    #[error("completion {0:?} needs exactly one of shell or shells")]
    CompletionShell(String),
    #[error(
        "completion {0:?} lists shells, so its source must be exec with a {{shell}} placeholder"
    )]
    CompletionShells(String),
    #[error("completion {0:?} names shell {1:?}; a shell is a lowercase word such as zsh")]
    Shell(String, String),
    #[error("cli-spec {0:?} needs format and bin")]
    CliSpec(String),
    #[error("cli-spec {0:?} describes {1:?}, which is not a bin name of any artifact")]
    CliSpecBin(String, String),
    #[error("resource {0:?} is for {1:?}, which is not a bin name of any artifact")]
    ResourceBin(String, String),
    #[error(
        "resource {0:?} does not say which executable it is for, and the release has several: {1}"
    )]
    ResourceNeedsBin(String, String),
    #[error("skill {0:?} needs a name without a slash")]
    SkillName(String),
    #[error("app {0:?} must come from an archive")]
    AppSource(String),
    #[error("sbom {0:?} needs a format (cyclonedx or spdx) and a static source, not exec")]
    Sbom(String),
    #[error("at least one release is required")]
    NoReleases,
    #[error("release {0:?} appears more than once")]
    DuplicateRelease(String),
    #[error("release {0:?} has no subject carrying its packslip digest")]
    OrphanRelease(String),
    #[error("expires_at is not after generated_at")]
    Expiry,
    #[error("{0} has an extension with an empty key")]
    ExtensionKey(String),
}

/// Extension keys name who defines them, so an empty one is a mistake.
fn check_extensions(owner: &str, extensions: &Extensions) -> Result<(), InvalidDocument> {
    if extensions.contains_key("") {
        return Err(InvalidDocument::ExtensionKey(owner.to_string()));
    }
    Ok(())
}

fn check_timestamp(field: &'static str, value: &str) -> Result<jiff::Timestamp, InvalidDocument> {
    value.parse().map_err(|_| InvalidDocument::Timestamp {
        field,
        value: value.to_string(),
    })
}

fn lowercase_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn check_digest(name: &str, digest: &Digest) -> Result<(), InvalidDocument> {
    if !lowercase_hex(&digest.sha256, 64) {
        return Err(InvalidDocument::Sha256(name.to_string()));
    }
    if let Some(sha512) = &digest.sha512
        && !lowercase_hex(sha512, 128)
    {
        return Err(InvalidDocument::Sha512(name.to_string()));
    }
    Ok(())
}

impl<P> Envelope<P> {
    fn check_envelope(&self, predicate_type: &'static str) -> Result<(), InvalidDocument> {
        if self.kind != STATEMENT_TYPE {
            return Err(InvalidDocument::Kind(self.kind.clone()));
        }
        if self.predicate_type != predicate_type {
            return Err(InvalidDocument::PredicateType {
                expected: predicate_type,
                actual: self.predicate_type.clone(),
            });
        }
        let mut subjects = std::collections::BTreeSet::new();
        for subject in &self.subject {
            if !subjects.insert(subject.name.as_str()) {
                return Err(InvalidDocument::DuplicateSubject(subject.name.clone()));
            }
            check_digest(&subject.name, &subject.digest)?;
        }
        Ok(())
    }

    /// The digest recorded for `name`.
    pub fn digest_of(&self, name: &str) -> Option<&str> {
        self.subject
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.digest.sha256.as_str())
    }

    /// The bytes as `create` writes them into the signed payload.
    pub fn canonical_bytes(&self) -> Vec<u8>
    where
        P: Serialize,
    {
        serde_json::to_vec(self).expect("a statement serialises")
    }
}

impl Statement {
    /// Structural validation beyond what serde enforces.
    pub fn validate(&self) -> Result<(), InvalidDocument> {
        self.check_envelope(PREDICATE_TYPE)?;
        let p = &self.predicate;
        if !valid_project(&p.project) {
            return Err(InvalidDocument::Project(p.project.clone()));
        }
        parse_version(&p.version)?;
        check_timestamp("published_at", &p.published_at)?;
        if p.artifacts.is_empty() {
            return Err(InvalidDocument::NoArtifacts);
        }
        check_extensions("the release", &p.extensions)?;
        let mut seen = std::collections::BTreeSet::new();
        for (i, artifact) in p.artifacts.iter().enumerate() {
            if !seen.insert(artifact.name.as_str()) {
                return Err(InvalidDocument::DuplicateArtifact(artifact.name.clone()));
            }
            check_extensions(
                &format!("artifact {:?}", artifact.name),
                &artifact.extensions,
            )?;
            if !self.subject.iter().any(|s| s.name == artifact.name) {
                return Err(InvalidDocument::OrphanArtifact(artifact.name.clone()));
            }
            for (field, value) in [
                ("os", &artifact.os),
                ("arch", &artifact.arch),
                ("libc", &artifact.libc),
                ("variant", &artifact.variant),
                ("format", &artifact.format),
            ] {
                if let Some(value) = value
                    && !valid_token(value)
                {
                    return Err(InvalidDocument::Token(
                        artifact.name.clone(),
                        field,
                        value.clone(),
                    ));
                }
            }
            let bare = artifact
                .format
                .as_deref()
                .filter(|f| is_bare_format(f))
                .map(|f| bare_file_name(&artifact.name, f));
            for bin in &artifact.bin {
                if bin.path.is_empty() {
                    return Err(InvalidDocument::BinPath(artifact.name.clone()));
                }
                if bin.name.contains('/') {
                    return Err(InvalidDocument::BinName(
                        artifact.name.clone(),
                        bin.name.clone(),
                    ));
                }
                if let Some(expected) = bare
                    && bin.path != expected
                {
                    return Err(InvalidDocument::BareBin {
                        artifact: artifact.name.clone(),
                        expected: expected.to_string(),
                        path: bin.path.clone(),
                    });
                }
            }
            // Two artifacts for one platform, variant, and format leave a
            // consumer nothing to choose by.
            if let Some(other) = p.artifacts[..i].iter().find(|a| {
                a.os == artifact.os
                    && a.arch == artifact.arch
                    && a.libc == artifact.libc
                    && a.variant == artifact.variant
                    && a.format == artifact.format
                    && a.format.is_some()
            }) {
                return Err(InvalidDocument::AmbiguousArtifacts(
                    other.name.clone(),
                    artifact.name.clone(),
                ));
            }
        }
        let bin_names: std::collections::BTreeSet<&str> = p
            .artifacts
            .iter()
            .flat_map(|a| a.bin.iter())
            .map(|b| b.name.strip_suffix(".exe").unwrap_or(&b.name))
            .collect();
        let is_bin = |name: &str| bin_names.contains(name.strip_suffix(".exe").unwrap_or(name));
        for artifact in &p.artifacts {
            let Some(requires) = &artifact.requires else {
                continue;
            };
            let name = || artifact.name.clone();
            let mut libs = std::collections::BTreeSet::new();
            for lib in requires.libs.iter().flatten() {
                if !valid_lib_name(lib) {
                    return Err(InvalidDocument::RequiredLib(name(), lib.clone()));
                }
                if !libs.insert(lib.as_str()) {
                    return Err(InvalidDocument::DuplicateRequiredLib(name(), lib.clone()));
                }
            }
            let mut commands = std::collections::BTreeSet::new();
            for required in &requires.bin {
                if !valid_required_bin_name(&required.name) {
                    return Err(InvalidDocument::RequiredBinName(
                        name(),
                        required.name.clone(),
                    ));
                }
                if let Some(min) = &required.min
                    && !valid_min_version(min)
                {
                    return Err(InvalidDocument::RequiredBinMin(
                        name(),
                        required.name.clone(),
                        min.clone(),
                    ));
                }
                if !commands.insert(required.name.as_str()) {
                    return Err(InvalidDocument::DuplicateRequiredBin(
                        name(),
                        required.name.clone(),
                    ));
                }
                if is_bin(&required.name) {
                    return Err(InvalidDocument::RequiredBinIsOwn(
                        name(),
                        required.name.clone(),
                    ));
                }
            }
        }
        let mut assets = std::collections::BTreeSet::new();
        for resource in &p.resources {
            let label = resource.label();
            if resource.kind.is_empty() {
                return Err(InvalidDocument::ResourceKind);
            }
            check_extensions(&format!("resource {label:?}"), &resource.extensions)?;
            let Some(source) = resource.source() else {
                return Err(InvalidDocument::ResourceSource(label));
            };
            for (field, value) in [
                ("os", &resource.os),
                ("arch", &resource.arch),
                ("libc", &resource.libc),
            ] {
                if let Some(value) = value
                    && !valid_token(value)
                {
                    return Err(InvalidDocument::Token(label, field, value.clone()));
                }
            }
            match source {
                ResourceSource::Archive => {
                    if !valid_relative_path(resource.archive.as_deref().unwrap_or_default()) {
                        return Err(InvalidDocument::ResourcePath(label, source));
                    }
                }
                ResourceSource::Asset => {
                    let asset = resource.asset.as_deref().unwrap_or_default();
                    if self.digest_of(asset).is_none() {
                        return Err(InvalidDocument::OrphanAsset(label, asset.to_string()));
                    }
                    assets.insert(asset);
                }
                ResourceSource::Repo => {
                    if !valid_relative_path(resource.repo.as_deref().unwrap_or_default()) {
                        return Err(InvalidDocument::ResourcePath(label, source));
                    }
                    if p.source
                        .as_ref()
                        .and_then(|s| s.commit.as_deref())
                        .is_none()
                    {
                        return Err(InvalidDocument::RepoNeedsCommit(label));
                    }
                }
                ResourceSource::Exec => {
                    let program = resource.exec[0].as_str();
                    if !is_bin(program) {
                        return Err(InvalidDocument::ExecNotABin(label, program.to_string()));
                    }
                }
            }
            if resource.url.is_some() && source != ResourceSource::Asset {
                return Err(InvalidDocument::ResourceUrl(label));
            }
            if matches!(resource.kind.as_str(), "completion" | "man") {
                match &resource.bin {
                    Some(bin) if !is_bin(bin) => {
                        return Err(InvalidDocument::ResourceBin(label, bin.clone()));
                    }
                    None if bin_names.len() > 1 => {
                        let names: Vec<&str> = bin_names.iter().copied().collect();
                        return Err(InvalidDocument::ResourceNeedsBin(label, names.join(", ")));
                    }
                    _ => {}
                }
            }
            match resource.kind.as_str() {
                "completion" => {
                    if resource.shell.is_some() == !resource.shells.is_empty() {
                        return Err(InvalidDocument::CompletionShell(label));
                    }
                    if let Some(bad) = resource
                        .shell
                        .iter()
                        .chain(&resource.shells)
                        .find(|s| !valid_shell(s))
                    {
                        return Err(InvalidDocument::Shell(label, bad.clone()));
                    }
                    if !resource.shells.is_empty()
                        && (source != ResourceSource::Exec
                            || !resource.exec.iter().any(|arg| arg.contains("{shell}")))
                    {
                        return Err(InvalidDocument::CompletionShells(label));
                    }
                }
                "cli-spec" => {
                    let (Some(_), Some(bin)) = (&resource.format, &resource.bin) else {
                        return Err(InvalidDocument::CliSpec(label));
                    };
                    if !is_bin(bin) {
                        return Err(InvalidDocument::CliSpecBin(label, bin.clone()));
                    }
                }
                "skill" => {
                    if !resource
                        .name
                        .as_deref()
                        .is_some_and(|n| !n.is_empty() && !n.contains('/'))
                    {
                        return Err(InvalidDocument::SkillName(label));
                    }
                }
                "app" if source != ResourceSource::Archive => {
                    return Err(InvalidDocument::AppSource(label));
                }
                "sbom" if resource.format.is_none() || source == ResourceSource::Exec => {
                    return Err(InvalidDocument::Sbom(label));
                }
                _ => {}
            }
        }
        for subject in &self.subject {
            if !seen.contains(subject.name.as_str()) && !assets.contains(subject.name.as_str()) {
                return Err(InvalidDocument::OrphanSubject(subject.name.clone()));
            }
        }
        Ok(())
    }

    /// The release's only executable, by name, when every artifact's
    /// `bin` entries name the same one (a Windows `.exe` counting as the
    /// same). A completion or man page may then leave `bin` out.
    pub fn sole_bin(&self) -> Option<&str> {
        let names: std::collections::BTreeSet<&str> = self
            .predicate
            .artifacts
            .iter()
            .flat_map(|a| a.bin.iter())
            .map(|b| b.name.strip_suffix(".exe").unwrap_or(&b.name))
            .collect();
        match names.iter().collect::<Vec<_>>().as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    /// The host part of the project name: `github.com` for
    /// `github.com/jdx/mise`.
    pub fn project_host(&self) -> &str {
        project_host(&self.predicate.project)
    }

    /// Whether every artifact links build provenance. A consumer that
    /// verifies those statements can then claim the SLSA build level they
    /// establish; the packslip itself only says they exist.
    pub fn provenance_linked(&self) -> bool {
        self.predicate
            .artifacts
            .iter()
            .all(|a| !a.provenance.is_empty())
    }

    /// The JSON schema for the document.
    pub fn schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(Statement)).expect("schema serialises")
    }
}

impl ReleaseListStatement {
    /// Structural validation beyond what serde enforces.
    pub fn validate(&self) -> Result<(), InvalidDocument> {
        self.check_envelope(RELEASES_PREDICATE_TYPE)?;
        let p = &self.predicate;
        if !valid_project(&p.project) {
            return Err(InvalidDocument::Project(p.project.clone()));
        }
        let generated = check_timestamp("generated_at", &p.generated_at)?;
        let expires = check_timestamp("expires_at", &p.expires_at)?;
        if expires <= generated {
            return Err(InvalidDocument::Expiry);
        }
        if p.releases.is_empty() {
            return Err(InvalidDocument::NoReleases);
        }
        check_extensions("the release list", &p.extensions)?;
        let mut seen = std::collections::BTreeSet::new();
        for release in &p.releases {
            parse_version(&release.version)?;
            if !seen.insert(release.version.as_str()) {
                return Err(InvalidDocument::DuplicateRelease(release.version.clone()));
            }
            check_extensions(
                &format!("release {:?}", release.version),
                &release.extensions,
            )?;
            check_timestamp("published_at", &release.published_at)?;
            if self.digest_of(&release.packslip).is_none() {
                return Err(InvalidDocument::OrphanRelease(release.version.clone()));
            }
        }
        Ok(())
    }

    /// Whether the list is still current at `now`.
    pub fn is_current(&self, now: jiff::Timestamp) -> bool {
        self.predicate
            .expires_at
            .parse::<jiff::Timestamp>()
            .is_ok_and(|expires| now < expires)
    }

    /// The JSON schema for the document.
    pub fn schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(ReleaseListStatement))
            .expect("schema serialises")
    }
}

/// The host of a project name: everything before the first slash.
pub fn project_host(project: &str) -> &str {
    project.split('/').next().unwrap_or_default()
}

/// The forge repository a project lives in, when the tooling knows the
/// host's layout: `(host, owner, repo)` for `github.com/owner/repo[/sub]`.
/// GitLab subgroups make its paths arbitrary depth, so it is not covered.
pub fn repository(project: &str) -> Option<(&str, &str, &str)> {
    let mut parts = project.split('/');
    let host = parts.next()?;
    if host != "github.com" {
        return None;
    }
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((host, owner, repo))
}

/// The path after the forge repository: `Some("oxlint")` for
/// `github.com/oxc-project/oxc/oxlint`, `None` for a repository itself or
/// a non-forge name.
pub fn repository_subpath(project: &str) -> Option<&str> {
    let (host, owner, repo) = repository(project)?;
    let prefix_len = host.len() + owner.len() + repo.len() + 2;
    let rest = project.get(prefix_len..)?;
    rest.strip_prefix('/').filter(|s| !s.is_empty())
}

/// A project name is a lowercase DNS host, optionally followed by path
/// segments: `github.com/jdx/mise`, `mise.jdx.dev`. No scheme, no empty
/// or dot segments, no trailing slash, and the host has at least one dot.
pub fn valid_project(project: &str) -> bool {
    let mut parts = project.split('/');
    let Some(host) = parts.next() else {
        return false;
    };
    let host_ok = host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    host_ok
        && parts.all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'~'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> Statement {
        Statement {
            kind: STATEMENT_TYPE.into(),
            subject: vec![Subject {
                name: "mise-v2026.9.1-linux-x64.tar.xz".into(),
                digest: Digest {
                    sha256: "a".repeat(64),
                    sha512: None,
                },
            }],
            predicate_type: PREDICATE_TYPE.into(),
            predicate: Predicate {
                project: "github.com/jdx/mise".into(),
                version: "2026.9.1".into(),
                published_at: "2026-09-01T12:00:00Z".into(),
                source: Some(Source {
                    repo: "https://github.com/jdx/mise".into(),
                    commit: Some("b".repeat(40)),
                    tag: Some("v2026.9.1".into()),
                }),
                artifacts: vec![Artifact {
                    name: "mise-v2026.9.1-linux-x64.tar.xz".into(),
                    os: Some("linux".into()),
                    arch: Some("x86_64".into()),
                    libc: Some("gnu".into()),
                    variant: None,
                    size: 12345678,
                    url: Some("https://github.com/jdx/mise/releases/download/v2026.9.1/mise-v2026.9.1-linux-x64.tar.xz".into()),
                    format: Some("tar.xz".into()),
                    bin: vec![Bin::new("mise/bin/mise")],
                    requires: None,
                    provenance: vec![],
                    extensions: Extensions::new(),
                }],
                resources: vec![],
                identity: Identity {
                    scheme: Scheme::SigstoreKey,
                    key_id: "5A0A0B8B9C6D7E1F".into(),
                    issuer: None,
                },
                attested_by: Attestor::Vendor,
                evidence: vec![],
                notes_url: None,
                extensions: Extensions::new(),
            },
        }
    }

    fn sample_list() -> ReleaseListStatement {
        ReleaseListStatement {
            kind: STATEMENT_TYPE.into(),
            subject: vec![Subject {
                name: "https://dl.example/2026.9.1/packslip.sigstore.json".into(),
                digest: Digest {
                    sha256: "c".repeat(64),
                    sha512: None,
                },
            }],
            predicate_type: RELEASES_PREDICATE_TYPE.into(),
            predicate: ReleaseList {
                project: "mise.jdx.dev".into(),
                generated_at: "2026-09-01T12:00:00Z".into(),
                expires_at: "2026-10-01T12:00:00Z".into(),
                sequence: 7,
                identity: Identity {
                    scheme: Scheme::SigstoreKey,
                    key_id: "5A0A0B8B9C6D7E1F".into(),
                    issuer: None,
                },
                releases: vec![ReleaseRef {
                    version: "2026.9.1".into(),
                    published_at: "2026-09-01T12:00:00Z".into(),
                    packslip: "https://dl.example/2026.9.1/packslip.sigstore.json".into(),
                    ..ReleaseRef::default()
                }],
                extensions: Extensions::new(),
            },
        }
    }

    #[test]
    fn valid_sample_and_provenance() {
        let s = sample();
        s.validate().unwrap();
        assert!(!s.provenance_linked());
        let mut with_provenance = s.clone();
        with_provenance.predicate.artifacts[0]
            .provenance
            .push("https://x/y.sigstore.json".into());
        assert!(with_provenance.provenance_linked());
        assert_eq!(
            s.digest_of("mise-v2026.9.1-linux-x64.tar.xz"),
            Some("a".repeat(64).as_str())
        );
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.starts_with(r#"{"_type":"https://in-toto.io/Statement/v1","subject""#),
            "{json}"
        );
        assert!(
            !json.contains("attested_by"),
            "defaults are omitted: {json}"
        );
        assert!(json.contains(r#""bin":["mise/bin/mise"]"#), "{json}");
        assert_eq!(serde_json::from_str::<Statement>(&json).unwrap(), s);
    }

    #[test]
    fn a_v0_2_document_round_trips_byte_for_byte() {
        let old = r#"{"_type":"https://in-toto.io/Statement/v1","subject":[{"name":"t-linux-x64.tar.xz","digest":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}],"predicateType":"https://packslip.dev/release/v1","predicate":{"project":"github.com/o/r","version":"1.0.0","published_at":"2026-09-01T00:00:00Z","artifacts":[{"name":"t-linux-x64.tar.xz","os":"linux","arch":"x86_64","libc":"gnu","size":5,"format":"tar.xz","bin":["t"]}],"identity":{"scheme":"sigstore-oidc","key_id":"https://github.com/o/r/.github/workflows/r.yml@refs/tags/v1","issuer":"https://token.actions.githubusercontent.com"}}}"#;
        let parsed: Statement = serde_json::from_str(old).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed.predicate.attested_by, Attestor::Vendor);
        assert_eq!(parsed.predicate.artifacts[0].bin, [Bin::new("t")]);
        assert_eq!(String::from_utf8(parsed.canonical_bytes()).unwrap(), old);
    }

    #[test]
    fn versions_are_semver_and_name_their_channel() {
        for bad in ["", "1", "1.2", "v1.2.3", "1.2.3.4", "2026-09-04", "nightly"] {
            assert!(parse_version(bad).is_err(), "{bad:?} should be refused");
        }
        let cases = [
            ("1.2.3", false, None),
            ("2026.9.1", false, None),
            ("1.2.3+build.5", false, None),
            ("1.2.0-rc.1", true, Some("rc")),
            ("1.2.0-beta.2", true, Some("beta")),
            ("1.3.0-nightly.20260904", true, Some("nightly")),
            ("1.2.0-1", true, None),
            ("1.2.0-0.3.7", true, None),
        ];
        for (text, prerelease, expected) in cases {
            let version = parse_version(text).unwrap();
            assert_eq!(!version.pre.is_empty(), prerelease, "{text}");
            assert_eq!(channel(&version), expected, "{text}");
        }
        let mut doc = sample();
        doc.predicate.version = "1.2".into();
        assert!(matches!(
            doc.validate(),
            Err(InvalidDocument::NotSemver(v)) if v == "1.2"
        ));
    }

    #[test]
    fn bins_take_both_forms() {
        let named: Vec<Bin> = serde_json::from_str(
            r#"["oxlint-x86_64", {"path":"bin/oxlint-x86_64","name":"oxlint"}]"#,
        )
        .unwrap();
        assert_eq!(named[0], Bin::named("oxlint-x86_64", "oxlint-x86_64"));
        assert_eq!(named[1], Bin::named("bin/oxlint-x86_64", "oxlint"));
        assert_eq!(
            serde_json::to_string(&named).unwrap(),
            r#"["oxlint-x86_64",{"path":"bin/oxlint-x86_64","name":"oxlint"}]"#
        );
        assert_eq!(Bin::new("a/b/tool").name, "tool");
        let mut s = sample();
        s.predicate.artifacts[0].bin = vec![Bin::named("x", "a/b")];
        assert!(matches!(s.validate(), Err(InvalidDocument::BinName(_, _))));
        s.predicate.artifacts[0].bin = vec![Bin::named("", "a")];
        assert!(matches!(s.validate(), Err(InvalidDocument::BinPath(_))));
    }

    #[test]
    fn host_requirements_validate() {
        let requires = |libs: Option<&[&str]>, bin: &[RequiredBin]| Requires {
            libs: libs.map(|l| l.iter().map(|s| s.to_string()).collect()),
            bin: bin.to_vec(),
            ..Requires::default()
        };
        let with = |r: Requires| {
            let mut s = sample();
            s.predicate.artifacts[0].requires = Some(r);
            s
        };
        with(requires(
            Some(&["libssl.so.3", "libz.so.1"]),
            &[RequiredBin::at_least("java", "17"), RequiredBin::new("git")],
        ))
        .validate()
        .unwrap();
        with(requires(Some(&[]), &[])).validate().unwrap();
        assert!(matches!(
            with(requires(Some(&["lib/libz.so.1"]), &[])).validate(),
            Err(InvalidDocument::RequiredLib(_, _))
        ));
        assert!(matches!(
            with(requires(Some(&["libz.so.1", "libz.so.1"]), &[])).validate(),
            Err(InvalidDocument::DuplicateRequiredLib(_, _))
        ));
        assert!(matches!(
            with(requires(None, &[RequiredBin::new("bin/java")])).validate(),
            Err(InvalidDocument::RequiredBinName(_, _))
        ));
        assert!(matches!(
            with(requires(None, &[RequiredBin::new("java.exe")])).validate(),
            Err(InvalidDocument::RequiredBinName(_, _))
        ));
        assert!(matches!(
            with(requires(None, &[RequiredBin::at_least("java", "v17")])).validate(),
            Err(InvalidDocument::RequiredBinMin(_, _, _))
        ));
        assert!(matches!(
            with(requires(
                None,
                &[
                    RequiredBin::new("java"),
                    RequiredBin::at_least("java", "17")
                ]
            ))
            .validate(),
            Err(InvalidDocument::DuplicateRequiredBin(_, _))
        ));
        assert!(matches!(
            with(requires(None, &[RequiredBin::new("mise")])).validate(),
            Err(InvalidDocument::RequiredBinIsOwn(_, _))
        ));
        let r = Requires {
            os_min: Some("12".into()),
            glibc_min: Some("2.31".into()),
            ..requires(Some(&["libz.so.1"]), &[RequiredBin::at_least("java", "17")])
        };
        assert_eq!(
            r.summary(),
            "os>=12; glibc>=2.31; libs libz.so.1; bin java>=17"
        );
        assert_eq!(requires(Some(&[]), &[]).summary(), "libs none");
        assert!(Requires::default().is_empty());
        assert_eq!(
            serde_json::to_string(&requires(Some(&["libz.so.1"]), &[RequiredBin::new("git")]))
                .unwrap(),
            r#"{"libs":["libz.so.1"],"bin":[{"name":"git"}]}"#
        );
    }

    #[test]
    fn validation_errors() {
        let mut s = sample();
        s.kind = "x".into();
        assert!(matches!(s.validate(), Err(InvalidDocument::Kind(_))));
        let mut s = sample();
        s.predicate_type = RELEASES_PREDICATE_TYPE.into();
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::PredicateType { .. })
        ));
        let mut s = sample();
        s.predicate.project = "pkg:github/jdx/mise".into();
        assert!(matches!(s.validate(), Err(InvalidDocument::Project(_))));
        let mut s = sample();
        s.predicate.published_at = "yesterday".into();
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::Timestamp { .. })
        ));
        let mut s = sample();
        s.subject[0].digest.sha256 = "A".repeat(64);
        assert!(matches!(s.validate(), Err(InvalidDocument::Sha256(_))));
        let mut s = sample();
        s.subject[0].digest.sha512 = Some("f".repeat(127));
        assert!(matches!(s.validate(), Err(InvalidDocument::Sha512(_))));
        s.subject[0].digest.sha512 = Some("f".repeat(128));
        s.validate().unwrap();
        let mut s = sample();
        s.subject.push(Subject {
            name: "other".into(),
            digest: Digest {
                sha256: "c".repeat(64),
                sha512: None,
            },
        });
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::OrphanSubject(_))
        ));
        let mut s = sample();
        s.predicate.artifacts.push(s.predicate.artifacts[0].clone());
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::DuplicateArtifact(_))
        ));
        let mut s = sample();
        s.subject.push(s.subject[0].clone());
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::DuplicateSubject(_))
        ));
        let mut s = sample();
        s.predicate.artifacts.clear();
        s.subject.clear();
        assert_eq!(s.validate(), Err(InvalidDocument::NoArtifacts));
    }

    #[test]
    fn resources_validate() {
        let with = |resource: Resource| {
            let mut s = sample();
            s.predicate.resources.push(resource);
            s
        };
        let archive = |kind: &str, path: &str| Resource {
            archive: Some(path.into()),
            ..Resource::new(kind)
        };

        // Every documented kind from every fitting source.
        let mut s = sample();
        s.subject.push(Subject {
            name: "mise-skill.tar.gz".into(),
            digest: Digest {
                sha256: "d".repeat(64),
                sha512: None,
            },
        });
        s.predicate.resources = vec![
            Resource {
                shell: Some("zsh".into()),
                ..archive("completion", "mise/share/zsh/site-functions/_mise")
            },
            Resource {
                shell: Some("fish".into()),
                repo: Some("completions/mise.fish".into()),
                ..Resource::new("completion")
            },
            Resource {
                shells: vec!["bash".into(), "zsh".into(), "fish".into()],
                exec: vec!["mise".into(), "completion".into(), "{shell}".into()],
                ..Resource::new("completion")
            },
            archive("man", "mise/man/man1/mise.1"),
            Resource {
                format: Some("usage".into()),
                bin: Some("mise".into()),
                exec: vec!["mise".into(), "usage".into()],
                ..Resource::new("cli-spec")
            },
            Resource {
                name: Some("mise".into()),
                asset: Some("mise-skill.tar.gz".into()),
                url: Some("https://dl.example/mise-skill.tar.gz".into()),
                ..Resource::new("skill")
            },
            archive("desktop", "share/applications/mise.desktop"),
            archive("icon", "share/icons/hicolor/512x512/apps/mise.png"),
            archive("app", "Mise.app"),
            archive("font", "fonts/Mise.ttf"),
        ];
        s.validate().unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains(r#"{"kind":"completion","shell":"zsh","archive":"mise/share/zsh/site-functions/_mise"}"#),
            "{json}"
        );
        assert!(
            json.contains(
                r#"{"kind":"cli-spec","bin":"mise","format":"usage","exec":["mise","usage"]}"#
            ),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Statement>(&json).unwrap(), s);
        assert_eq!(s.predicate.resources[2].label(), "completion/bash,zsh,fish");
        assert_eq!(s.predicate.resources[4].label(), "cli-spec/usage/mise");
        assert_eq!(s.predicate.resources[5].label(), "skill/mise");
        assert_eq!(s.predicate.resources[3].label(), "man");
        assert_eq!(
            s.predicate.resources[5].source(),
            Some(ResourceSource::Asset)
        );
        // Without the resource, the asset subject is an orphan.
        s.predicate.resources.remove(5);
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::OrphanSubject(_))
        ));

        // A Windows bin name still matches an exec or cli-spec by its bare name.
        let mut windows = with(Resource {
            format: Some("usage".into()),
            bin: Some("mise".into()),
            exec: vec!["mise".into(), "usage".into()],
            ..Resource::new("cli-spec")
        });
        windows.predicate.artifacts[0].bin = vec![Bin::new("mise.exe")];
        windows.validate().unwrap();

        type Expect = fn(&InvalidDocument) -> bool;
        let cases: Vec<(Resource, Expect)> = vec![
            (Resource::new(""), |e| {
                matches!(e, InvalidDocument::ResourceKind)
            }),
            (Resource::new("man"), |e| {
                matches!(e, InvalidDocument::ResourceSource(_))
            }),
            (
                Resource {
                    repo: Some("x".into()),
                    ..archive("man", "y")
                },
                |e| matches!(e, InvalidDocument::ResourceSource(_)),
            ),
            (archive("man", "/etc/passwd"), |e| {
                matches!(e, InvalidDocument::ResourcePath(_, ResourceSource::Archive))
            }),
            (archive("man", "a/../b"), |e| {
                matches!(e, InvalidDocument::ResourcePath(_, ResourceSource::Archive))
            }),
            (
                Resource {
                    repo: Some("".into()),
                    ..Resource::new("man")
                },
                |e| matches!(e, InvalidDocument::ResourcePath(_, ResourceSource::Repo)),
            ),
            (
                Resource {
                    asset: Some("nope.tar.gz".into()),
                    ..Resource::new("man")
                },
                |e| matches!(e, InvalidDocument::OrphanAsset(_, _)),
            ),
            (
                Resource {
                    url: Some("https://x".into()),
                    ..archive("man", "m.1")
                },
                |e| matches!(e, InvalidDocument::ResourceUrl(_)),
            ),
            (
                Resource {
                    exec: vec!["other".into()],
                    ..Resource::new("man")
                },
                |e| matches!(e, InvalidDocument::ExecNotABin(_, _)),
            ),
            (archive("completion", "_mise"), |e| {
                matches!(e, InvalidDocument::CompletionShell(_))
            }),
            (
                Resource {
                    shell: Some("zsh".into()),
                    shells: vec!["bash".into()],
                    ..archive("completion", "_mise")
                },
                |e| matches!(e, InvalidDocument::CompletionShell(_)),
            ),
            (
                Resource {
                    shells: vec!["bash".into()],
                    ..archive("completion", "_mise")
                },
                |e| matches!(e, InvalidDocument::CompletionShells(_)),
            ),
            (
                Resource {
                    shell: Some(" zsh".into()),
                    ..archive("completion", "_mise")
                },
                |e| matches!(e, InvalidDocument::Shell(_, _)),
            ),
            (
                Resource {
                    shells: vec!["bash".into(), "".into()],
                    exec: vec!["mise".into(), "completion".into(), "{shell}".into()],
                    ..Resource::new("completion")
                },
                |e| matches!(e, InvalidDocument::Shell(_, _)),
            ),
            (
                Resource {
                    shells: vec!["bash".into()],
                    exec: vec!["mise".into(), "completion".into()],
                    ..Resource::new("completion")
                },
                |e| matches!(e, InvalidDocument::CompletionShells(_)),
            ),
            (
                Resource {
                    format: Some("usage".into()),
                    ..archive("cli-spec", "mise.kdl")
                },
                |e| matches!(e, InvalidDocument::CliSpec(_)),
            ),
            (
                Resource {
                    format: Some("usage".into()),
                    bin: Some("other".into()),
                    ..archive("cli-spec", "mise.kdl")
                },
                |e| matches!(e, InvalidDocument::CliSpecBin(_, _)),
            ),
            (
                Resource {
                    bin: Some("other".into()),
                    ..archive("man", "mise.1")
                },
                |e| matches!(e, InvalidDocument::ResourceBin(_, _)),
            ),
            (archive("skill", "skills/mise"), |e| {
                matches!(e, InvalidDocument::SkillName(_))
            }),
            (
                Resource {
                    name: Some("a/b".into()),
                    ..archive("skill", "skills/mise")
                },
                |e| matches!(e, InvalidDocument::SkillName(_)),
            ),
            (
                Resource {
                    repo: Some("Mise.app".into()),
                    ..Resource::new("app")
                },
                |e| matches!(e, InvalidDocument::AppSource(_)),
            ),
        ];
        for (resource, expected) in cases {
            let err = with(resource.clone()).validate().unwrap_err();
            assert!(expected(&err), "{resource:?}: {err}");
        }

        // A repo source needs the commit it is pinned by.
        let mut s = with(Resource {
            repo: Some("man/mise.1".into()),
            ..Resource::new("man")
        });
        s.validate().unwrap();
        s.predicate.source.as_mut().unwrap().commit = None;
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::RepoNeedsCommit(_))
        ));
    }

    #[test]
    fn release_lists_validate_and_expire() {
        let l = sample_list();
        l.validate().unwrap();
        assert!(l.is_current("2026-09-15T00:00:00Z".parse().unwrap()));
        assert!(!l.is_current("2026-10-02T00:00:00Z".parse().unwrap()));
        assert!(!l.predicate.releases[0].is_yanked());
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains("security"), "{json}");
        let mut yanked = sample_list();
        yanked.predicate.releases[0].status = Some(ReleaseStatus::Yanked);
        yanked.predicate.releases[0].status_reason = Some("bad build".into());
        assert!(yanked.predicate.releases[0].is_yanked());
        assert!(
            serde_json::to_string(&yanked)
                .unwrap()
                .contains(r#""status":"yanked""#)
        );
        let mut bad = sample_list();
        bad.predicate.expires_at = bad.predicate.generated_at.clone();
        assert_eq!(bad.validate(), Err(InvalidDocument::Expiry));
        let mut bad = sample_list();
        bad.subject.clear();
        assert!(matches!(
            bad.validate(),
            Err(InvalidDocument::OrphanRelease(_))
        ));
        let mut bad = sample_list();
        bad.predicate
            .releases
            .push(bad.predicate.releases[0].clone());
        assert!(matches!(
            bad.validate(),
            Err(InvalidDocument::DuplicateRelease(_))
        ));
        let mut bad = sample_list();
        bad.predicate.releases.clear();
        assert_eq!(bad.validate(), Err(InvalidDocument::NoReleases));
        let schema = ReleaseListStatement::schema();
        assert!(schema["properties"]["predicate"].is_object());
    }

    #[test]
    fn project_names_are_host_paths() {
        for ok in [
            "github.com/jdx/mise",
            "mise.jdx.dev",
            "jdx.dev/mise",
            "gitlab.com/group/sub/proj",
            "example.org/tool_1.2~x",
            "github.com/oxc-project/oxc/oxlint",
        ] {
            assert!(valid_project(ok), "{ok}");
        }
        for bad in [
            "",
            "mise",
            "localhost/x",
            "pkg:github/jdx/mise",
            "https://github.com/jdx/mise",
            "GitHub.com/jdx/mise",
            "github.com/",
            "github.com//mise",
            "github.com/jdx/mise/",
            "github.com/../mise",
            ".github.com/x",
            "git hub.com/x",
        ] {
            assert!(!valid_project(bad), "{bad}");
        }
        assert_eq!(project_host("github.com/jdx/mise"), "github.com");
        assert_eq!(sample().project_host(), "github.com");
    }

    #[test]
    fn extensions_round_trip_and_unknown_fields_are_ignored() {
        let mut doc = serde_json::to_value(sample()).unwrap();
        doc["predicate"]["extensions"] =
            serde_json::json!({ "mise": { "postinstall": "mise reshim" } });
        doc["predicate"]["artifacts"][0]["extensions"] =
            serde_json::json!({ "example.com": { "build_id": "20260901.3" } });
        // A field this revision does not define is tolerated, not kept.
        doc["predicate"]["future_field"] = serde_json::json!(true);
        let parsed: Statement = serde_json::from_value(doc).unwrap();
        parsed.validate().unwrap();
        assert_eq!(
            parsed.predicate.extensions["mise"]["postinstall"],
            "mise reshim"
        );
        assert_eq!(
            parsed.predicate.artifacts[0].extensions["example.com"]["build_id"],
            "20260901.3"
        );
        let again: serde_json::Value = serde_json::to_value(&parsed).unwrap();
        assert_eq!(
            again["predicate"]["extensions"]["mise"]["postinstall"],
            "mise reshim"
        );
        assert!(again["predicate"].get("future_field").is_none());
        // Nothing declared, nothing written.
        let bare = serde_json::to_value(sample()).unwrap();
        assert!(bare["predicate"].get("extensions").is_none());
        assert!(
            bare["predicate"]["artifacts"][0]
                .get("extensions")
                .is_none()
        );

        let mut doc = sample();
        doc.predicate
            .extensions
            .insert("".into(), serde_json::json!(1));
        assert_eq!(
            doc.validate(),
            Err(InvalidDocument::ExtensionKey("the release".into()))
        );
        let mut doc = sample();
        doc.predicate.artifacts[0]
            .extensions
            .insert("".into(), serde_json::json!(1));
        assert!(matches!(
            doc.validate(),
            Err(InvalidDocument::ExtensionKey(_))
        ));
        let mut list = sample_list();
        list.predicate.releases[0]
            .extensions
            .insert("".into(), serde_json::json!(1));
        assert!(matches!(
            list.validate(),
            Err(InvalidDocument::ExtensionKey(_))
        ));
    }

    #[test]
    fn forge_repositories_and_subpaths() {
        assert_eq!(
            repository("github.com/jdx/mise"),
            Some(("github.com", "jdx", "mise"))
        );
        assert_eq!(
            repository("github.com/oxc-project/oxc/oxlint"),
            Some(("github.com", "oxc-project", "oxc"))
        );
        assert_eq!(repository("github.com/jdx"), None);
        assert_eq!(repository("gitlab.com/group/sub/proj"), None);
        assert_eq!(repository("mise.jdx.dev"), None);
        assert_eq!(repository_subpath("github.com/jdx/mise"), None);
        assert_eq!(
            repository_subpath("github.com/oxc-project/oxc/oxlint"),
            Some("oxlint")
        );
        assert_eq!(
            repository_subpath("github.com/biomejs/biome/crates/cli"),
            Some("crates/cli")
        );
    }

    #[test]
    fn tags_name_versions() {
        let cases = [
            ("v1.2.3", "github.com/o/r", Some("1.2.3")),
            ("1.2.3", "github.com/o/r", Some("1.2.3")),
            ("v1.2.3-rc.1", "github.com/o/r", Some("1.2.3-rc.1")),
            ("jq-1.7.1", "github.com/jqlang/jq", Some("1.7.1")),
            ("jq@1.7.1", "github.com/jqlang/jq", Some("1.7.1")),
            (
                "oxlint_v1.0.0",
                "github.com/oxc-project/oxc/oxlint",
                Some("1.0.0"),
            ),
            (
                "cli/v1.9.4",
                "github.com/biomejs/biome/crates/cli",
                Some("1.9.4"),
            ),
            (
                "crates/cli@1.9.4",
                "github.com/biomejs/biome/crates/cli",
                Some("1.9.4"),
            ),
            // Loose spellings normalize.
            ("v4.1", "github.com/Genymobile/scrcpy", Some("4.1.0")),
            ("25.07.1", "github.com/helix-editor/helix", Some("25.7.1")),
            ("2026.08.31", "github.com/o/r", Some("2026.8.31")),
            // Another tool's tag, or no version at all.
            ("web-v2026.8.1", "github.com/bitwarden/clients/cli", None),
            ("rust-v0.1.0", "github.com/openai/codex", None),
            ("2026-08-31", "github.com/o/r", None),
            ("latest", "github.com/o/r", None),
            ("nightly", "github.com/o/r", None),
            ("r42", "github.com/o/r", None),
            ("1.2.3.4", "github.com/o/r", None),
            ("v1.2.3", "tool.example.com", Some("1.2.3")),
        ];
        for (tag, project, expected) in cases {
            assert_eq!(
                tag_version(tag, project).as_deref(),
                expected,
                "{tag} in {project}"
            );
        }
        assert_eq!(
            normalize_version("1.2.3+build.7").as_deref(),
            Some("1.2.3+build.7")
        );
        assert_eq!(normalize_version("1").as_deref(), None);
        assert_eq!(normalize_version("1.x").as_deref(), None);
    }

    #[test]
    fn selection_follows_the_consumer_rules() {
        let artifact = |name: &str,
                        os: Option<&str>,
                        arch: Option<&str>,
                        libc: Option<&str>,
                        format: &str,
                        variant: Option<&str>| Artifact {
            name: name.into(),
            os: os.map(str::to_string),
            arch: arch.map(str::to_string),
            libc: libc.map(str::to_string),
            variant: variant.map(str::to_string),
            size: 1,
            url: None,
            format: Some(format.into()),
            bin: vec![],
            requires: None,
            provenance: vec![],
            extensions: Extensions::new(),
        };
        let linux = Host {
            os: "linux",
            arch: "x86_64",
            libc: Some("gnu"),
        };
        let formats = ["tar.xz", "tar.gz", "zip", "raw"];
        let artifacts = vec![
            artifact(
                "a.tar.gz",
                Some("linux"),
                Some("x86_64"),
                Some("gnu"),
                "tar.gz",
                None,
            ),
            artifact(
                "a.zip",
                Some("linux"),
                Some("x86_64"),
                Some("gnu"),
                "zip",
                None,
            ),
            artifact(
                "a-musl.tar.gz",
                Some("linux"),
                Some("x86_64"),
                Some("musl"),
                "tar.gz",
                None,
            ),
            artifact(
                "a-fips.tar.gz",
                Some("linux"),
                Some("x86_64"),
                Some("gnu"),
                "tar.gz",
                Some("fips"),
            ),
            artifact("a.jar", None, None, None, "zip", None),
            artifact("a-mac", Some("darwin"), None, None, "raw", None),
            artifact(
                "a.deb",
                Some("linux"),
                Some("x86_64"),
                Some("gnu"),
                "deb",
                None,
            ),
        ];
        // Format is a preference, never a tie: tar.gz beats zip.
        assert_eq!(
            select_artifact(&artifacts, &linux, None, &formats)
                .unwrap()
                .name,
            "a.tar.gz"
        );
        // A variant is only taken when asked for.
        assert_eq!(
            select_artifact(&artifacts, &linux, Some("fips"), &formats)
                .unwrap()
                .name,
            "a-fips.tar.gz"
        );
        assert_eq!(
            select_artifact(&artifacts, &linux, Some("debug"), &formats),
            Err(Selection::NoMatch)
        );
        // The universal macOS binary fits either arch; the jar fits any host.
        let mac = Host {
            os: "darwin",
            arch: "aarch64",
            libc: None,
        };
        assert_eq!(
            select_artifact(&artifacts, &mac, None, &formats)
                .unwrap()
                .name,
            "a-mac"
        );
        let bsd = Host {
            os: "freebsd",
            arch: "x86_64",
            libc: None,
        };
        assert_eq!(
            select_artifact(&artifacts, &bsd, None, &formats)
                .unwrap()
                .name,
            "a.jar"
        );
        // A host that reports no libc takes only artifacts that need none.
        let unknown_libc = Host {
            os: "linux",
            arch: "x86_64",
            libc: None,
        };
        assert_eq!(
            select_artifact(&artifacts, &unknown_libc, None, &formats)
                .unwrap()
                .name,
            "a.jar"
        );
        // Only formats the consumer handles count.
        assert_eq!(
            select_artifact(&artifacts, &linux, None, &["deb"])
                .unwrap()
                .name,
            "a.deb"
        );
        assert_eq!(
            select_artifact(&artifacts, &linux, None, &["7z"]),
            Err(Selection::NoMatch)
        );
        // Two artifacts nothing tells apart are refused, not guessed at.
        let twins = vec![
            artifact(
                "x.tar.gz",
                Some("linux"),
                Some("x86_64"),
                Some("gnu"),
                "tar.gz",
                None,
            ),
            artifact(
                "y.tar.gz",
                Some("linux"),
                Some("x86_64"),
                Some("gnu"),
                "tar.gz",
                None,
            ),
        ];
        assert_eq!(
            select_artifact(&twins, &linux, None, &formats),
            Err(Selection::Ambiguous("x.tar.gz".into(), "y.tar.gz".into()))
        );
        let mut doc = sample();
        doc.subject.push(Subject {
            name: "twin".into(),
            digest: doc.subject[0].digest.clone(),
        });
        doc.predicate.artifacts.push(Artifact {
            name: "twin".into(),
            ..doc.predicate.artifacts[0].clone()
        });
        assert!(matches!(
            doc.validate(),
            Err(InvalidDocument::AmbiguousArtifacts(_, _))
        ));

        // Resources apply to the artifacts their scope names.
        let any = Resource {
            archive: Some("man/t.1".into()),
            ..Resource::new("man")
        };
        let windows = Resource {
            os: Some("windows".into()),
            ..any.clone()
        };
        assert!(resource_fits(&any, &artifacts[0]));
        assert!(!resource_fits(&windows, &artifacts[0]));
        assert!(resource_fits(
            &windows,
            &artifact("w.zip", Some("windows"), Some("x86_64"), None, "zip", None)
        ));
        assert!(
            !resource_fits(&windows, &artifacts[4]),
            "a portable artifact is not a Windows one"
        );
    }

    #[test]
    fn a_release_with_several_executables_says_which_one_a_completion_is_for() {
        let mut s = sample();
        s.predicate.artifacts[0].bin.push(Bin::new("bin/other"));
        let completion = |bin: Option<&str>| Resource {
            shell: Some("zsh".into()),
            archive: Some("share/_x".into()),
            bin: bin.map(Into::into),
            ..Resource::new("completion")
        };
        s.predicate.resources = vec![completion(None)];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(&err, InvalidDocument::ResourceNeedsBin(_, names) if names == "mise, other"),
            "{err}"
        );
        s.predicate.resources = vec![completion(Some("other"))];
        s.validate().unwrap();
        s.predicate.resources = vec![completion(Some("mise.exe"))];
        s.validate().unwrap();
        let mut man = Resource::new("man");
        man.archive = Some("share/man/other.1".into());
        s.predicate.resources = vec![man];
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::ResourceNeedsBin(_, _))
        ));
        // Two entries for different executables are different things.
        assert_ne!(
            completion(Some("mise")).identities(),
            completion(Some("other")).identities()
        );
        assert_eq!(
            completion(Some("other"))
                .identities()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["completion/other/zsh"]
        );
        // In a release with one executable, an entry that leaves bin out is
        // for that one and competes with an entry that names it.
        assert_eq!(
            completion(None).identities_for(Some("mise")),
            completion(Some("mise")).identities()
        );
        assert_eq!(
            completion(Some("mise.exe")).identities(),
            completion(Some("mise")).identities(),
            "the Windows form names the same executable"
        );
        assert_eq!(s.sole_bin(), None, "mise and other");
        s.predicate.artifacts[0].bin.pop();
        assert_eq!(s.sole_bin(), Some("mise"));
        s.predicate.resources = vec![
            completion(None),
            Resource {
                os: Some("linux".into()),
                ..completion(Some("mise"))
            },
        ];
        let linux: Artifact = serde_json::from_value(serde_json::json!({
            "name": "t-linux.tar.gz", "os": "linux", "size": 1
        }))
        .unwrap();
        assert_eq!(
            select_resources(&s, &linux),
            [&s.predicate.resources[1]],
            "the scoped entry that names the sole executable hides the unnamed one"
        );
    }

    #[test]
    fn resources_compete_only_with_their_own_kind_and_identity() {
        let artifact =
            |json: serde_json::Value| -> Artifact { serde_json::from_value(json).unwrap() };
        let linux = artifact(serde_json::json!({
            "name": "t-linux.tar.gz", "os": "linux", "arch": "x86_64", "size": 1
        }));
        let mac = artifact(serde_json::json!({
            "name": "t-mac.tar.gz", "os": "darwin", "size": 1
        }));
        let scoped = |r: Resource, os: &str| Resource {
            os: Some(os.into()),
            ..r
        };
        let archive = |kind: &str, path: &str| Resource {
            archive: Some(path.into()),
            ..Resource::new(kind)
        };
        let skill = |name: &str| Resource {
            name: Some(name.into()),
            ..archive("skill", &format!("skills/{name}"))
        };
        let completion = |shell: &str, path: &str| Resource {
            shell: Some(shell.into()),
            ..archive("completion", path)
        };
        let resources = vec![
            skill("everywhere"),
            scoped(skill("linuxonly"), "linux"),
            skill("both"),
            scoped(skill("both"), "linux"),
            completion("zsh", "share/_t"),
            scoped(completion("zsh", "win/_t"), "windows"),
            completion("bash", "share/t.bash"),
            Resource {
                shells: vec!["bash".into(), "zsh".into()],
                exec: vec!["t".into(), "completion".into(), "{shell}".into()],
                ..Resource::new("completion")
            },
            archive("man", "share/man/t.1"),
            scoped(archive("man", "man/t.1"), "darwin"),
        ];
        let mut doc = sample();
        doc.predicate.resources = resources;
        let resources = &doc.predicate.resources;
        let names = |artifact: &Artifact| -> Vec<String> {
            select_resources(&doc, artifact)
                .iter()
                .map(|r| {
                    let mut label = r.label();
                    if let Some(p) = &r.archive {
                        label.push('@');
                        label.push_str(p);
                    }
                    label
                })
                .collect()
        };
        assert_eq!(
            names(&linux),
            [
                "skill/everywhere@skills/everywhere",
                "skill/linuxonly@skills/linuxonly",
                "skill/both@skills/both",
                "completion/zsh@share/_t",
                "completion/bash@share/t.bash",
                "completion/bash,zsh",
                "man@share/man/t.1",
            ],
            "the linux skill hides only the unscoped entry of its own name; the windows zsh entry hides nothing on linux"
        );
        assert_eq!(
            names(&mac),
            [
                "skill/everywhere@skills/everywhere",
                "skill/both@skills/both",
                "completion/zsh@share/_t",
                "completion/bash@share/t.bash",
                "completion/bash,zsh",
                "man@man/t.1",
            ],
            "the darwin man page hides the unscoped one of the same file name"
        );
        let ids = |r: &Resource| -> Vec<String> {
            r.identities().iter().map(ToString::to_string).collect()
        };
        assert_eq!(ids(&resources[7]), ["completion/bash", "completion/zsh"]);
        assert_eq!(ids(&resources[3]), ["skill/both"]);
        assert_eq!(ids(&resources[8]), ["man/t.1"]);
        assert_eq!(ids(&Resource::new("man")), ["man"]);
        // A kind of the vendor's own may contain a slash without colliding
        // with a documented kind's parts.
        let spec = Resource {
            format: Some("usage".into()),
            bin: Some("tool".into()),
            ..Resource::new("cli-spec")
        };
        let custom = Resource {
            archive: Some("tool".into()),
            ..Resource::new("cli-spec/usage")
        };
        assert_eq!(ids(&spec), ids(&custom), "they print alike");
        assert_ne!(
            spec.identities(),
            custom.identities(),
            "but are not the same thing"
        );
    }

    #[test]
    fn vocabularies_and_bare_executables_validate() {
        for ok in ["linux", "x86_64", "tar.gz", "7z", "install_only", "musl"] {
            assert!(valid_token(ok), "{ok}");
        }
        for bad in ["", "Linux", "tar gz", ".hidden", "x86-64!", "ünix"] {
            assert!(!valid_token(bad), "{bad}");
        }
        for (field, set) in [
            ("os", OS_VALUES),
            ("arch", ARCH_VALUES),
            ("libc", LIBC_VALUES),
            ("format", FORMAT_VALUES),
        ] {
            for value in set {
                assert!(valid_token(value), "{field} {value}");
            }
        }
        let mut s = sample();
        s.predicate.artifacts[0].os = Some("Linux".into());
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::Token(_, "os", _))
        ));
        let mut s = sample();
        s.predicate.resources.push(Resource {
            arch: Some("X64".into()),
            archive: Some("m.1".into()),
            ..Resource::new("man")
        });
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::Token(_, "arch", _))
        ));

        assert!(is_bare_format("raw") && is_bare_format("gz") && !is_bare_format("tar.gz"));
        assert_eq!(
            bare_file_name("argo-linux-amd64.gz", "gz"),
            "argo-linux-amd64"
        );
        assert_eq!(
            bare_file_name("argo-linux-amd64", "raw"),
            "argo-linux-amd64"
        );
        assert_eq!(bare_file_name("tool.tar.gz", "tar.gz"), "tool.tar.gz");
        let mut s = sample();
        s.predicate.artifacts[0].format = Some("gz".into());
        s.predicate.artifacts[0].name = "mise-linux-x64.gz".into();
        s.subject[0].name = "mise-linux-x64.gz".into();
        s.predicate.artifacts[0].bin = vec![Bin::named("mise-linux-x64", "mise")];
        s.validate().unwrap();
        s.predicate.artifacts[0].bin = vec![Bin::new("mise")];
        assert!(matches!(s.validate(), Err(InvalidDocument::BareBin { .. })));

        // An SBOM is a resource with a format and a source that verifies.
        let sbom = |source: Resource| {
            let mut s = sample();
            s.subject.push(Subject {
                name: "mise.cdx.json".into(),
                digest: Digest {
                    sha256: "e".repeat(64),
                    sha512: None,
                },
            });
            s.predicate.resources.push(Resource {
                format: Some("cyclonedx".into()),
                ..source
            });
            s
        };
        sbom(Resource {
            asset: Some("mise.cdx.json".into()),
            ..Resource::new("sbom")
        })
        .validate()
        .unwrap();
        let mut no_format = sbom(Resource {
            asset: Some("mise.cdx.json".into()),
            ..Resource::new("sbom")
        });
        no_format.predicate.resources[0].format = None;
        assert!(matches!(
            no_format.validate(),
            Err(InvalidDocument::Sbom(_))
        ));
        let mut by_exec = sbom(Resource {
            asset: Some("mise.cdx.json".into()),
            ..Resource::new("sbom")
        });
        by_exec.predicate.resources[0].asset = None;
        by_exec.predicate.resources[0].exec = vec!["mise".into(), "sbom".into()];
        by_exec.subject.pop();
        assert!(matches!(by_exec.validate(), Err(InvalidDocument::Sbom(_))));

        // A list entry may carry what its publisher checked.
        let mut list = sample_list();
        list.predicate.releases[0].evidence = vec![Evidence {
            kind: "scan".into(),
            detail: Some("https://scans.example/1".into()),
        }];
        list.predicate.releases[0].tag = Some("v2026.9.1".into());
        list.validate().unwrap();
        let json = serde_json::to_string(&list).unwrap();
        assert!(
            json.contains(r#""tag":"v2026.9.1""#) && json.contains(r#""kind":"scan""#),
            "{json}"
        );
    }

    #[test]
    fn portable_duplicates_are_ambiguous() {
        let mut s = sample();
        for artifact in &mut s.predicate.artifacts {
            artifact.os = None;
            artifact.arch = None;
            artifact.libc = None;
            artifact.format = Some("zip".into());
        }
        s.validate().unwrap();
        s.subject.push(Subject {
            name: "twin.zip".into(),
            digest: s.subject[0].digest.clone(),
        });
        s.predicate.artifacts.push(Artifact {
            name: "twin.zip".into(),
            ..s.predicate.artifacts[0].clone()
        });
        assert!(matches!(
            s.validate(),
            Err(InvalidDocument::AmbiguousArtifacts(_, _))
        ));
        // Without a format neither is selectable, so nothing is ambiguous.
        for artifact in &mut s.predicate.artifacts {
            artifact.format = None;
            artifact.bin.clear();
        }
        s.validate().unwrap();
    }

    #[test]
    fn schema_has_the_required_fields() {
        let schema = Statement::schema();
        let required = schema["required"].as_array().unwrap();
        for field in ["_type", "subject", "predicateType", "predicate"] {
            assert!(required.iter().any(|r| r == field), "{field}");
        }
        let text = schema.to_string();
        assert!(
            text.contains("attested_by") && text.contains("variant"),
            "{text}"
        );
    }
}
