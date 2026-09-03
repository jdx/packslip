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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Digest {
    /// Lowercase hex.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Predicate {
    /// The project's name: a host path such as `github.com/jdx/mise` or
    /// `mise.jdx.dev`, the way Go names modules. The host is where a
    /// consumer discovers releases and, for forge hosts, what identity is
    /// expected to have signed them.
    pub project: String,
    pub version: String,
    /// RFC 3339 UTC.
    pub published_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    pub artifacts: Vec<Artifact>,
    pub identity: Identity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sbom: Option<String>,
    /// The version this release replaces, for ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
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
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Executables inside the artifact, as paths relative to the archive
    /// root, or the artifact's own name when it is a bare executable. A
    /// consumer puts these on PATH.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin: Vec<String>,
    /// URLs of build provenance statements (SLSA) for this artifact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<String>,
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
    /// Newest first is conventional but not required.
    pub releases: Vec<ReleaseRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseRef {
    pub version: String,
    /// RFC 3339 UTC, copied from the release's packslip.
    pub published_at: String,
    /// URL of the release's `packslip.sigstore.json`. The statement's
    /// subject of the same name carries that file's digest.
    pub packslip: String,
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
    #[error("at least one artifact is required")]
    NoArtifacts,
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

fn check_sha256(name: &str, hex: &str) -> Result<(), InvalidDocument> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(InvalidDocument::Sha256(name.to_string()));
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
            check_sha256(&subject.name, &subject.digest.sha256)?;
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
        }
        for subject in &self.subject {
            if !seen.contains(subject.name.as_str()) {
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
                    size: 12345678,
                    url: Some("https://github.com/jdx/mise/releases/download/v2026.9.1/mise-v2026.9.1-linux-x64.tar.xz".into()),
                    format: Some("tar.xz".into()),
                    bin: vec!["mise/bin/mise".into()],
                    provenance: vec![],
                }],
                identity: Identity {
                    scheme: Scheme::SigstoreKey,
                    key_id: "5A0A0B8B9C6D7E1F".into(),
                    issuer: None,
                },
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
        assert_eq!(serde_json::from_str::<Statement>(&json).unwrap(), s);
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
        s.subject.push(Subject {
            name: "other".into(),
            digest: Digest {
                sha256: "c".repeat(64),
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
    fn release_lists_validate_and_expire() {
        let l = sample_list();
        l.validate().unwrap();
        assert!(l.is_current("2026-09-15T00:00:00Z".parse().unwrap()));
        assert!(!l.is_current("2026-10-02T00:00:00Z".parse().unwrap()));
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
    fn schema_has_the_required_fields() {
        let schema = Statement::schema();
        let required = schema["required"].as_array().unwrap();
        for field in ["_type", "subject", "predicateType", "predicate"] {
            assert!(required.iter().any(|r| r == field), "{field}");
        }
    }
}
