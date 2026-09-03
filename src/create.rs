//! Creating a packslip from release artifacts: digests, sizes, platform
//! inference from file names, and the canonical bytes to sign.

use std::path::Path;

use crate::minisign::SecretKey;
use crate::model::{
    Artifact, Digest, Identity, PREDICATE_TYPE, Predicate, STATEMENT_TYPE, Source, Statement,
    Subject,
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

/// The document, ready to sign.
pub struct Created {
    /// The canonical bytes: what `packslip.json` holds and what is signed.
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

/// Build and validate the document.
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

/// The minisign trusted comment: enough to tell signatures apart in a
/// directory listing.
pub fn trusted_comment(statement: &Statement) -> String {
    format!(
        "packslip {} {} published_at:{}",
        statement.predicate.project, statement.predicate.version, statement.predicate.published_at
    )
}

/// Sign the document with a minisign key; the text of `packslip.json.minisig`.
pub fn sign_minisign(created: &Created, key: &SecretKey) -> String {
    key.sign(&created.document, &trusted_comment(&created.statement))
        .to_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minisign::key_id_hex;
    use crate::model::Scheme;

    fn minisign_identity(key: &SecretKey) -> Identity {
        Identity {
            scheme: Scheme::Minisign,
            key_id: key_id_hex(&key.public_key().key_id),
            issuer: None,
        }
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
    fn create_then_verify_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("tool-v1.0.0-linux-x64.tar.xz");
        let b = dir.path().join("tool-v1.0.0-darwin-arm64.tar.xz");
        let w = dir.path().join("tool-v1.0.0-windows-x64.zip");
        std::fs::write(&a, b"linux bytes").unwrap();
        std::fs::write(&b, b"darwin bytes").unwrap();
        std::fs::write(&w, b"windows bytes").unwrap();
        let key = SecretKey::from_seed([1u8; 32]);
        let base = Request {
            project: "github.com/example/tool",
            version: "1.0.0",
            published_at: Some("2026-09-01T00:00:00Z"),
            source: None,
            artifacts: Vec::new(),
            url_base: None,
            sbom: None,
            supersedes: None,
            identity: minisign_identity(&key),
        };
        let input = |path, os, bin: &[&str], provenance: &[&str]| ArtifactInput {
            path,
            os,
            arch: None,
            libc: None,
            bin: bin.iter().map(|s| s.to_string()).collect(),
            provenance: provenance.iter().map(|s| s.to_string()).collect(),
        };
        let created = create(&Request {
            source: Some(Source {
                repo: "https://github.com/example/tool".into(),
                commit: None,
                tag: Some("v1.0.0".into()),
            }),
            artifacts: vec![
                input(
                    &a,
                    None,
                    &["tool"],
                    &["https://example.com/a.sigstore.json"],
                ),
                input(&b, None, &["tool"], &[]),
                input(&w, None, &["tool"], &[]),
            ],
            url_base: Some("https://github.com/example/tool/releases/download/v1.0.0/"),
            supersedes: Some("0.9.0"),
            ..base
        })
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
        assert_eq!(
            created.statement.declared_level(),
            crate::Level::L2,
            "one artifact lacks provenance"
        );

        let key2 = SecretKey::from_seed([1u8; 32]);
        let overridden = create(&Request {
            artifacts: vec![input(&a, Some("darwin"), &[], &[])],
            identity: minisign_identity(&key2),
            ..Request {
                project: "github.com/example/tool",
                version: "1.0.0",
                published_at: Some("2026-09-01T00:00:00Z"),
                source: None,
                artifacts: Vec::new(),
                url_base: None,
                sbom: None,
                supersedes: None,
                identity: minisign_identity(&key2),
            }
        })
        .unwrap();
        assert_eq!(overridden.statement.predicate.artifacts[0].libc, None);
        let overridden = create(&Request {
            artifacts: vec![input(&b, Some("linux"), &[], &[])],
            identity: minisign_identity(&key2),
            ..Request {
                project: "github.com/example/tool",
                version: "1.0.0",
                published_at: Some("2026-09-01T00:00:00Z"),
                source: None,
                artifacts: Vec::new(),
                url_base: None,
                sbom: None,
                supersedes: None,
                identity: minisign_identity(&key2),
            }
        })
        .unwrap();
        assert_eq!(
            overridden.statement.predicate.artifacts[0].libc.as_deref(),
            Some("gnu")
        );

        let signature = sign_minisign(&created, &key);
        let verified = crate::verify::verify_minisign(
            &created.document,
            &signature,
            &key.public_key(),
            &[&a, &b],
        )
        .unwrap();
        assert_eq!(verified.version, "1.0.0");
        assert_eq!(verified.scheme, Scheme::Minisign);
        assert_eq!(
            verified.checked_artifacts,
            [
                "tool-v1.0.0-linux-x64.tar.xz",
                "tool-v1.0.0-darwin-arm64.tar.xz"
            ]
        );
        assert_eq!(verified.level, crate::Level::L2);

        // Tampering with the document, the artifact, or the key fails.
        let mut tampered = created.document.clone();
        let last = tampered.len() - 2;
        tampered[last] = b' ';
        assert!(
            crate::verify::verify_minisign(&tampered, &signature, &key.public_key(), &[]).is_err()
        );
        std::fs::write(&a, b"other bytes").unwrap();
        let err =
            crate::verify::verify_minisign(&created.document, &signature, &key.public_key(), &[&a])
                .unwrap_err();
        assert!(err.to_string().contains("sha256 is"), "{err}");
        let other = SecretKey::from_seed([2u8; 32]).public_key();
        assert!(
            crate::verify::verify_minisign(&created.document, &signature, &other, &[]).is_err()
        );
        let unknown = dir.path().join("unknown.tar.gz");
        std::fs::write(&unknown, b"").unwrap();
        let err = crate::verify::verify_minisign(
            &created.document,
            &signature,
            &key.public_key(),
            &[&unknown],
        )
        .unwrap_err();
        assert!(err.to_string().contains("not listed"), "{err}");

        // A minisign signature for a document that declares sigstore is refused.
        let mut wrong_scheme = created.statement.clone();
        wrong_scheme.predicate.identity.scheme = Scheme::SigstoreOidc;
        let bytes = wrong_scheme.canonical_bytes();
        let err =
            crate::verify::verify_minisign(&bytes, &signature, &key.public_key(), &[]).unwrap_err();
        assert!(
            matches!(err, crate::verify::Error::SchemeMismatch { .. }),
            "{err}"
        );
    }
}
