//! The packslip documents: in-toto statements whose predicates say what a
//! release shipped (`release/v1`) and which releases a project has
//! (`releases/v1`). See `docs/spec/packslip.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
pub const PREDICATE_TYPE: &str = "https://packslip.dev/release/v1";
pub const RELEASES_PREDICATE_TYPE: &str = "https://packslip.dev/releases/v1";

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
    pub sha256: String,
    /// Lowercase hex, for consumers that want it (electron-updater,
    /// Balrog, Scoop).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Predicate {
    /// The project's name: a host path such as `github.com/jdx/mise` or
    /// `mise.jdx.dev`, the way Go names modules. The host is where a
    /// consumer discovers releases and, for forge hosts, what identity is
    /// expected to have signed them. A tool in a monorepo adds a subpath:
    /// `github.com/oxc-project/oxc/oxlint`.
    pub project: String,
    pub version: String,
    /// RFC 3339 UTC.
    pub published_at: String,
    /// A release not meant for general use.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prerelease: bool,
    /// `stable`, `beta`, `nightly`, or whatever the vendor calls it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// How consumers order this project's versions.
    #[serde(default, skip_serializing_if = "VersionOrder::is_source")]
    pub version_order: VersionOrder,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sbom: Option<String>,
    /// The version this release replaces, for ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
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

/// How a project's versions are ordered, in mise's vocabulary. The vendor
/// declares it; consumers never infer it from the strings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum VersionOrder {
    /// The order the release list gives, newest first: GitHub's releases
    /// endpoint, or the vendor's signed list. For date versions,
    /// two-component versions, mixed histories, and anything uncertain.
    #[default]
    Source,
    /// Versions are strict `MAJOR.MINOR.PATCH` (calver such as `2026.9.1`
    /// included) and sort as semver, so the highest is the latest and
    /// range constraints have meaning.
    Semver,
}

impl VersionOrder {
    pub fn is_source(&self) -> bool {
        *self == VersionOrder::Source
    }
}

impl std::fmt::Display for VersionOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            VersionOrder::Source => "source",
            VersionOrder::Semver => "semver",
        })
    }
}

/// One thing a repackager checked. Documented kinds:
/// `pkgbuild-checksums`, `checksum-file-over-tls`, `apt-release-gpg`,
/// `vendor-signature`, `github-attestation`, `none`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub kind: String,
    /// A key id, URL, or note that lets a reader check the claim.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libc: Option<String>,
    /// Tells apart artifacts that share os, arch, libc, and format:
    /// `fips`, `baseline`, `debug`, `installer`, `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Executables inside the artifact, as paths relative to the archive
    /// root, or the artifact's own name when it is a bare executable. A
    /// consumer puts these on PATH under `name`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin: Vec<Bin>,
    /// What the artifact needs from the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Requires>,
    /// URLs of build provenance statements (SLSA) for this artifact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<String>,
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

/// Host requirements a consumer can check before installing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Requires {
    /// Minimum OS version, in the OS's own terms: `12` for macOS Monterey,
    /// `10.0.17763` for Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_min: Option<String>,
    /// Minimum glibc for a `gnu` Linux build, such as `2.31`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glibc_min: Option<String>,
}

/// Something the release ships besides its executables, and where to get
/// it. The kind says what it is; exactly one of `archive`, `asset`,
/// `repo`, and `exec` says where it comes from. Documented kinds:
/// `completion`, `man`, `cli-spec`, `skill`, `desktop`, `icon`, `app`.
/// Consumers ignore kinds they do not know.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Resource {
    pub kind: String,
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
    /// For `cli-spec`: the executable the spec describes, by its `bin`
    /// name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    /// For `cli-spec`: the spec format. `usage` is documented.
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
    /// How consumers order the versions listed.
    #[serde(default, skip_serializing_if = "VersionOrder::is_source")]
    pub version_order: VersionOrder,
    /// Newest first: under `source` ordering this order is the ranking.
    pub releases: Vec<ReleaseRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseRef {
    pub version: String,
    /// RFC 3339 UTC, copied from the release's packslip.
    pub published_at: String,
    /// URL of the release's `packslip.sigstore.json`. The statement's
    /// subject of the same name carries that file's digest.
    pub packslip: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prerelease: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
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
    #[error("skill {0:?} needs a name without a slash")]
    SkillName(String),
    #[error("app {0:?} must come from an archive")]
    AppSource(String),
    #[error("at least one release is required")]
    NoReleases,
    #[error("release {0:?} appears more than once")]
    DuplicateRelease(String),
    #[error("release {0:?} has no subject carrying its packslip digest")]
    OrphanRelease(String),
    #[error("expires_at is not after generated_at")]
    Expiry,
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
        if p.version.is_empty() {
            return Err(InvalidDocument::Version);
        }
        check_timestamp("published_at", &p.published_at)?;
        if p.artifacts.is_empty() {
            return Err(InvalidDocument::NoArtifacts);
        }
        let mut seen = std::collections::BTreeSet::new();
        for artifact in &p.artifacts {
            if !seen.insert(artifact.name.as_str()) {
                return Err(InvalidDocument::DuplicateArtifact(artifact.name.clone()));
            }
            if !self.subject.iter().any(|s| s.name == artifact.name) {
                return Err(InvalidDocument::OrphanArtifact(artifact.name.clone()));
            }
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
            }
        }
        let bin_names: std::collections::BTreeSet<&str> = p
            .artifacts
            .iter()
            .flat_map(|a| a.bin.iter())
            .map(|b| b.name.strip_suffix(".exe").unwrap_or(&b.name))
            .collect();
        let is_bin = |name: &str| bin_names.contains(name.strip_suffix(".exe").unwrap_or(name));
        let mut assets = std::collections::BTreeSet::new();
        for resource in &p.resources {
            let label = resource.label();
            if resource.kind.is_empty() {
                return Err(InvalidDocument::ResourceKind);
            }
            let Some(source) = resource.source() else {
                return Err(InvalidDocument::ResourceSource(label));
            };
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
        let mut seen = std::collections::BTreeSet::new();
        for release in &p.releases {
            if release.version.is_empty() {
                return Err(InvalidDocument::Version);
            }
            if !seen.insert(release.version.as_str()) {
                return Err(InvalidDocument::DuplicateRelease(release.version.clone()));
            }
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
                prerelease: false,
                channel: None,
                version_order: VersionOrder::Semver,
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
                sbom: None,
                supersedes: Some("2026.9.0".into()),
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
                version_order: VersionOrder::Source,
                releases: vec![ReleaseRef {
                    version: "2026.9.1".into(),
                    published_at: "2026-09-01T12:00:00Z".into(),
                    packslip: "https://dl.example/2026.9.1/packslip.sigstore.json".into(),
                    ..ReleaseRef::default()
                }],
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
        assert!(!json.contains("prerelease"), "defaults are omitted: {json}");
        assert!(json.contains(r#""version_order":"semver""#), "{json}");
        assert!(!json.contains("attested_by"), "{json}");
        assert!(json.contains(r#""bin":["mise/bin/mise"]"#), "{json}");
        assert_eq!(serde_json::from_str::<Statement>(&json).unwrap(), s);
    }

    #[test]
    fn a_v0_2_document_round_trips_byte_for_byte() {
        let old = r#"{"_type":"https://in-toto.io/Statement/v1","subject":[{"name":"t-linux-x64.tar.xz","digest":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}],"predicateType":"https://packslip.dev/release/v1","predicate":{"project":"github.com/o/r","version":"1","published_at":"2026-09-01T00:00:00Z","artifacts":[{"name":"t-linux-x64.tar.xz","os":"linux","arch":"x86_64","libc":"gnu","size":5,"format":"tar.xz","bin":["t"]}],"identity":{"scheme":"sigstore-oidc","key_id":"https://github.com/o/r/.github/workflows/r.yml@refs/tags/v1","issuer":"https://token.actions.githubusercontent.com"}}}"#;
        let parsed: Statement = serde_json::from_str(old).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed.predicate.attested_by, Attestor::Vendor);
        assert_eq!(parsed.predicate.version_order, VersionOrder::Source);
        assert!(!parsed.predicate.prerelease);
        assert_eq!(parsed.predicate.artifacts[0].bin, [Bin::new("t")]);
        assert_eq!(String::from_utf8(parsed.canonical_bytes()).unwrap(), old);
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
