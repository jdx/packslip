//! The packslip document: an in-toto statement whose predicate says what a
//! release shipped and how to verify it. See `docs/spec/packslip.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
pub const PREDICATE_TYPE: &str = "https://packslip.dev/release/v1";

/// The whole document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Statement {
    #[serde(rename = "_type")]
    pub kind: String,
    /// One entry per artifact, name and digests. Mirrors `predicate.artifacts`.
    pub subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: Predicate,
}

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
    /// URLs of build provenance statements for this artifact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<String>,
}

/// How the document is signed, so a consumer can check what it pinned
/// against what it received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Identity {
    pub scheme: Scheme,
    /// For `minisign`, the key id in uppercase hex. For the sigstore
    /// schemes, the certificate's subject identity: the workflow URI for a
    /// CI identity, or the email for a human one.
    pub key_id: String,
    /// For the sigstore schemes, the OIDC issuer that vouched for the
    /// identity, such as `https://token.actions.githubusercontent.com`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme {
    /// A detached minisign signature over the document bytes.
    Minisign,
    /// A sigstore bundle signed with a long-lived key.
    SigstoreKey,
    /// A sigstore bundle signed keylessly with a workload or human OIDC
    /// identity, certified by Fulcio and logged to Rekor.
    SigstoreOidc,
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Scheme::Minisign => "minisign",
            Scheme::SigstoreKey => "sigstore-key",
            Scheme::SigstoreOidc => "sigstore-oidc",
        })
    }
}

/// How much a consumer may conclude from a verified packslip.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// Checksums only, no signature.
    L0,
    /// Signed checksums or artifact signatures.
    L1,
    /// A signed packslip.
    L2,
    /// L2 plus per-artifact build provenance.
    L3,
    /// L3 plus reproducible or independently verified builds.
    L4,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Level::L0 => "L0",
            Level::L1 => "L1",
            Level::L2 => "L2",
            Level::L3 => "L3",
            Level::L4 => "L4",
        })
    }
}

/// Why a document is malformed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvalidDocument {
    #[error("_type must be {STATEMENT_TYPE}, got {0:?}")]
    Kind(String),
    #[error("predicateType must be {PREDICATE_TYPE}, got {0:?}")]
    PredicateType(String),
    #[error(
        "project must be a host path such as github.com/owner/repo or tool.example.com, got {0:?}"
    )]
    Project(String),
    #[error("version must not be empty")]
    Version,
    #[error("published_at must be RFC 3339, got {0:?}")]
    PublishedAt(String),
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
}

impl Statement {
    /// Structural validation beyond what serde enforces.
    pub fn validate(&self) -> Result<(), InvalidDocument> {
        if self.kind != STATEMENT_TYPE {
            return Err(InvalidDocument::Kind(self.kind.clone()));
        }
        if self.predicate_type != PREDICATE_TYPE {
            return Err(InvalidDocument::PredicateType(self.predicate_type.clone()));
        }
        let p = &self.predicate;
        if !valid_project(&p.project) {
            return Err(InvalidDocument::Project(p.project.clone()));
        }
        if p.version.is_empty() {
            return Err(InvalidDocument::Version);
        }
        if jiff::Timestamp::from_str_checked(&p.published_at).is_err() {
            return Err(InvalidDocument::PublishedAt(p.published_at.clone()));
        }
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
        let mut subjects = std::collections::BTreeSet::new();
        for subject in &self.subject {
            if !subjects.insert(subject.name.as_str()) {
                return Err(InvalidDocument::DuplicateSubject(subject.name.clone()));
            }
            if !seen.contains(subject.name.as_str()) {
                return Err(InvalidDocument::OrphanSubject(subject.name.clone()));
            }
            let hex = &subject.digest.sha256;
            if hex.len() != 64
                || !hex
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(InvalidDocument::Sha256(subject.name.clone()));
            }
        }
        Ok(())
    }

    /// The host part of the project name: `github.com` for
    /// `github.com/jdx/mise`.
    pub fn project_host(&self) -> &str {
        project_host(&self.predicate.project)
    }

    /// The digest recorded for `name`.
    pub fn digest_of(&self, name: &str) -> Option<&str> {
        self.subject
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.digest.sha256.as_str())
    }

    /// The level the document itself supports once its signature checks
    /// out: L3 when every artifact links provenance, else L2. L4 needs
    /// evidence a consumer gathers elsewhere.
    pub fn declared_level(&self) -> Level {
        if self
            .predicate
            .artifacts
            .iter()
            .all(|a| !a.provenance.is_empty())
        {
            Level::L3
        } else {
            Level::L2
        }
    }

    /// The canonical bytes that are signed: compact JSON with keys in
    /// serialisation order, exactly as `create` writes the file.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a statement serialises")
    }

    /// The JSON schema for the document.
    pub fn schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(Statement)).expect("schema serialises")
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

trait TimestampExt {
    fn from_str_checked(s: &str) -> Result<jiff::Timestamp, jiff::Error>;
}

impl TimestampExt for jiff::Timestamp {
    fn from_str_checked(s: &str) -> Result<jiff::Timestamp, jiff::Error> {
        s.parse()
    }
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
                    scheme: Scheme::Minisign,
                    key_id: "5A0A0B8B9C6D7E1F".into(),
                    issuer: None,
                },
                sbom: None,
                supersedes: Some("2026.9.0".into()),
            },
        }
    }

    #[test]
    fn valid_sample_and_levels() {
        let s = sample();
        s.validate().unwrap();
        assert_eq!(s.declared_level(), Level::L2);
        let mut with_provenance = s.clone();
        with_provenance.predicate.artifacts[0]
            .provenance
            .push("https://x/y.sigstore.json".into());
        assert_eq!(with_provenance.declared_level(), Level::L3);
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
        s.predicate.project = "pkg:github/jdx/mise".into();
        assert!(matches!(s.validate(), Err(InvalidDocument::Project(_))));
        let mut s = sample();
        s.predicate.published_at = "yesterday".into();
        assert!(matches!(s.validate(), Err(InvalidDocument::PublishedAt(_))));
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
