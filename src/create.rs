//! Creating the statements: a release from its artifact files, and a
//! release list from released packslips. Signing is `sigstore::sign`.

use std::path::Path;

use crate::model::{
    Artifact, Digest, Envelope, Identity, PREDICATE_TYPE, Predicate, RELEASES_PREDICATE_TYPE,
    ReleaseList, ReleaseListStatement, ReleaseRef, STATEMENT_TYPE, Source, Statement, Subject,
};

/// What `create` needs.
pub struct Request<'a> {
    pub project: &'a str,
    pub version: &'a str,
    pub published_at: Option<&'a str>,
    pub source: Option<Source>,
    pub artifacts: Vec<ArtifactInput<'a>>,
    /// Prepended to artifact names for their download URL, when given.
    pub url_base: Option<&'a str>,
    pub sbom: Option<&'a str>,
    pub supersedes: Option<&'a str>,
    /// Who will sign the document.
    pub identity: Identity,
}

/// One artifact file, with optional overrides for what the name implies.
pub struct ArtifactInput<'a> {
    pub path: &'a Path,
    pub os: Option<&'a str>,
    pub arch: Option<&'a str>,
    pub libc: Option<&'a str>,
    /// Executables inside the artifact. On Windows an entry without an
    /// extension gets `.exe`.
    pub bin: Vec<String>,
    pub provenance: Vec<String>,
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
    #[error("{path}: not a packslip bundle: {why}")]
    NotAPackslip { path: String, why: String },
}

/// The `(os, arch, libc, format)` a file name implies.
pub type Platform = (
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
);

/// Infer `(os, arch, libc, format)` from a release file name.
pub fn infer_platform(name: &str) -> Platform {
    let lower = name.to_ascii_lowercase();
    let os = if lower.contains("linux")
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
        || lower.ends_with(".exe")
        || lower.ends_with(".msi")
    {
        Some("windows")
    } else if lower.contains("freebsd") {
        Some("freebsd")
    } else {
        None
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
    } else if lower.contains("riscv64") {
        Some("riscv64")
    } else if lower.contains("i686") || lower.contains("x86") {
        Some("i686")
    } else {
        None
    };
    let libc = if lower.contains("musl") {
        Some("musl")
    } else if os == Some("linux") {
        Some("gnu")
    } else {
        None
    };
    let format = [
        "tar.xz", "tar.gz", "tar.zst", "tar.bz2", "tgz", "zip", "deb", "rpm", "dmg", "pkg", "msi",
        "exe", "AppImage",
    ]
    .into_iter()
    .find(|ext| lower.ends_with(&format!(".{}", ext.to_ascii_lowercase())));
    (os, arch, libc, format)
}

/// Build and validate the release statement.
pub fn create(request: &Request<'_>) -> Result<Created, Error> {
    let mut subject = Vec::new();
    let mut artifacts = Vec::new();
    for input in &request.artifacts {
        let name = input
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let (sha256, size) = crate::digest_file(input.path).map_err(|source| Error::Io {
            path: input.path.display().to_string(),
            source,
        })?;
        let (inferred_os, arch, inferred_libc, format) = infer_platform(&name);
        let os = input.os.or(inferred_os);
        let libc = input.libc.map(str::to_string).or_else(|| match os {
            Some("linux") => Some(inferred_libc.unwrap_or("gnu").to_string()),
            _ => None,
        });
        let bin = input
            .bin
            .iter()
            .map(|b| {
                let last = b.rsplit('/').next().unwrap_or(b);
                if os == Some("windows") && !last.contains('.') {
                    format!("{b}.exe")
                } else {
                    b.clone()
                }
            })
            .collect();
        subject.push(Subject {
            name: name.clone(),
            digest: Digest { sha256 },
        });
        let url = request
            .url_base
            .map(|base| format!("{}/{name}", base.trim_end_matches('/')));
        artifacts.push(Artifact {
            url,
            name,
            os: os.map(str::to_string),
            arch: input.arch.or(arch).map(str::to_string),
            libc,
            size,
            format: format.map(str::to_string),
            bin,
            provenance: input.provenance.clone(),
        });
    }
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
            identity: request.identity.clone(),
            sbom: request.sbom.map(str::to_string),
            supersedes: request.supersedes.map(str::to_string),
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
}

/// What `create_release_list` needs.
pub struct ListRequest<'a> {
    pub project: &'a str,
    pub generated_at: Option<&'a str>,
    /// How long the list stays current.
    pub valid_for: std::time::Duration,
    pub sequence: u64,
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
        let (sha256, _) = crate::digest_file(listed.bundle_path).map_err(|source| Error::Io {
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
            digest: Digest { sha256 },
        });
        releases.push(ReleaseRef {
            version: statement.predicate.version,
            published_at: statement.predicate.published_at,
            packslip: listed.url.to_string(),
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
            identity: request.identity.clone(),
            releases,
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
        assert_eq!(
            infer_platform("mise-v2026.9.1-linux-x64.tar.xz"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("tar.xz"))
        );
        assert_eq!(
            infer_platform("mise-v2026.9.1-linux-arm64-musl.tar.gz"),
            (Some("linux"), Some("aarch64"), Some("musl"), Some("tar.gz"))
        );
        assert_eq!(
            infer_platform("tool-macos-aarch64.zip"),
            (Some("darwin"), Some("aarch64"), None, Some("zip"))
        );
        assert_eq!(
            infer_platform("tool-windows-x64.exe"),
            (Some("windows"), Some("x86_64"), None, Some("exe"))
        );
        assert_eq!(infer_platform("SHASUMS256.txt"), (None, None, None, None));
        assert_eq!(
            infer_platform("tool_amd64.deb"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("deb"))
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
            infer_platform("tool-linux-x86-64.tar.gz"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("tar.gz"))
        );
    }

    #[test]
    fn create_sign_verify_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("tool-v1.0.0-linux-x64.tar.xz");
        let b = dir.path().join("tool-v1.0.0-darwin-arm64.tar.xz");
        let w = dir.path().join("tool-v1.0.0-windows-x64.zip");
        std::fs::write(&a, b"linux bytes").unwrap();
        std::fs::write(&b, b"darwin bytes").unwrap();
        std::fs::write(&w, b"windows bytes").unwrap();
        let key = SecretKey::from_seed([1u8; 32]);
        let input = |path, os, bin: &[&str], provenance: &[&str]| ArtifactInput {
            path,
            os,
            arch: None,
            libc: None,
            bin: bin.iter().map(|s| s.to_string()).collect(),
            provenance: provenance.iter().map(|s| s.to_string()).collect(),
        };
        let request = |artifacts, source, url_base, supersedes| Request {
            project: "tool.example.com",
            version: "1.0.0",
            published_at: Some("2026-09-01T00:00:00Z"),
            source,
            artifacts,
            url_base,
            sbom: None,
            supersedes,
            identity: key_identity(&key),
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
                input(&w, None, &["tool"], &[]),
            ],
            Some(Source {
                repo: "https://github.com/example/tool".into(),
                commit: None,
                tag: Some("v1.0.0".into()),
            }),
            Some("https://github.com/example/tool/releases/download/v1.0.0/"),
            Some("0.9.0"),
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
        assert_eq!(arts[0].bin, ["tool"]);
        assert_eq!(arts[2].bin, ["tool.exe"], "windows gets .exe");
        assert!(!created.statement.provenance_linked());

        let overridden = create(&request(
            vec![input(&a, Some("darwin"), &[], &[])],
            None,
            None,
            None,
        ))
        .unwrap();
        assert_eq!(overridden.statement.predicate.artifacts[0].libc, None);
        let overridden = create(&request(
            vec![input(&b, Some("linux"), &[], &[])],
            None,
            None,
            None,
        ))
        .unwrap();
        assert_eq!(
            overridden.statement.predicate.artifacts[0].libc.as_deref(),
            Some("gnu")
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

        // A release list over that bundle.
        let bundle_path = dir.path().join("packslip.sigstore.json");
        std::fs::write(&bundle_path, &bundle).unwrap();
        let list = create_release_list(&ListRequest {
            project: "tool.example.com",
            generated_at: Some("2026-09-01T01:00:00Z"),
            valid_for: std::time::Duration::from_secs(30 * 86_400),
            sequence: 3,
            releases: vec![ListedRelease {
                url: "https://dl.example.com/1.0.0/packslip.sigstore.json",
                bundle_path: &bundle_path,
            }],
            identity: key_identity(&key),
        })
        .unwrap();
        assert_eq!(list.statement.predicate.expires_at, "2026-10-01T01:00:00Z");
        assert_eq!(list.statement.predicate.releases[0].version, "1.0.0");
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
        assert!(
            verified_list
                .list
                .is_current("2026-09-15T00:00:00Z".parse().unwrap())
        );
        let err = create_release_list(&ListRequest {
            project: "other.example.com",
            generated_at: None,
            valid_for: std::time::Duration::from_secs(60),
            sequence: 1,
            releases: vec![ListedRelease {
                url: "https://x/packslip.sigstore.json",
                bundle_path: &bundle_path,
            }],
            identity: key_identity(&key),
        })
        .unwrap_err();
        assert!(err.to_string().contains("the list is for"), "{err}");
    }
}
