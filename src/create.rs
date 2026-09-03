//! Creating the statements: a release from its artifact files, and a
//! release list from released packslips. Signing is `sigstore::sign`.

use std::path::Path;

use crate::model::{
    Artifact, Attestor, Bin, Digest, Envelope, Evidence, Identity, PREDICATE_TYPE, Predicate,
    RELEASES_PREDICATE_TYPE, ReleaseList, ReleaseListStatement, ReleaseRef, Requires,
    STATEMENT_TYPE, Source, Statement, Subject,
};

/// What `create` needs.
pub struct Request<'a> {
    pub project: &'a str,
    pub version: &'a str,
    pub published_at: Option<&'a str>,
    pub prerelease: bool,
    pub channel: Option<&'a str>,
    pub source: Option<Source>,
    pub artifacts: Vec<ArtifactInput<'a>>,
    /// Prepended to artifact names for their download URL, when given and
    /// the artifact has no URL of its own.
    pub url_base: Option<&'a str>,
    pub notes_url: Option<&'a str>,
    pub sbom: Option<&'a str>,
    pub supersedes: Option<&'a str>,
    /// Who will sign the document.
    pub identity: Identity,
    pub attested_by: Attestor,
    pub evidence: Vec<Evidence>,
    /// Also record sha512 digests.
    pub sha512: bool,
}

impl<'a> Request<'a> {
    /// A vendor-attested request with nothing optional set.
    pub fn new(project: &'a str, version: &'a str, identity: Identity) -> Request<'a> {
        Request {
            project,
            version,
            published_at: None,
            prerelease: false,
            channel: None,
            source: None,
            artifacts: Vec::new(),
            url_base: None,
            notes_url: None,
            sbom: None,
            supersedes: None,
            identity,
            attested_by: Attestor::Vendor,
            evidence: Vec::new(),
            sha512: true,
        }
    }
}

/// One artifact file, with optional overrides for what the name implies.
pub struct ArtifactInput<'a> {
    pub path: &'a Path,
    pub os: Option<&'a str>,
    pub arch: Option<&'a str>,
    pub libc: Option<&'a str>,
    pub variant: Option<String>,
    /// The download URL, when it is not `url_base/name`.
    pub url: Option<String>,
    /// Executables inside the artifact. On Windows an entry without an
    /// extension gets `.exe`, on both path and name.
    pub bin: Vec<Bin>,
    pub requires: Option<Requires>,
    pub provenance: Vec<String>,
}

impl<'a> ArtifactInput<'a> {
    pub fn new(path: &'a Path) -> ArtifactInput<'a> {
        ArtifactInput {
            path,
            os: None,
            arch: None,
            libc: None,
            variant: None,
            url: None,
            bin: Vec::new(),
            requires: None,
            provenance: Vec::new(),
        }
    }
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
    #[error(
        "artifacts {a:?} and {b:?} both describe {platform}; give one a variant so consumers can tell them apart"
    )]
    Ambiguous {
        a: String,
        b: String,
        platform: String,
    },
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
        || lower.ends_with(".msix")
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
        "tar.xz", "tar.gz", "tar.zst", "tar.bz2", "tgz", "zip", "7z", "deb", "rpm", "dmg", "pkg",
        "msix", "msi", "exe", "appimage",
    ]
    .into_iter()
    .find(|ext| lower.ends_with(&format!(".{ext}")));
    (os, arch, libc, format)
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
    let mut subject = Vec::new();
    let mut artifacts: Vec<Artifact> = Vec::new();
    for input in &request.artifacts {
        let name = input
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let digests = crate::digest_file_all(input.path).map_err(|source| Error::Io {
            path: input.path.display().to_string(),
            source,
        })?;
        let (inferred_os, arch, inferred_libc, inferred_format) = infer_platform(&name);
        let os = input.os.or(inferred_os);
        let libc = input.libc.map(str::to_string).or_else(|| match os {
            Some("linux") => Some(inferred_libc.unwrap_or("gnu").to_string()),
            _ => None,
        });
        // A file with no archive or installer extension is the executable itself.
        let format = inferred_format.map(str::to_string).or_else(|| {
            let has_extension = name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| !ext.is_empty());
            (!has_extension && os.is_some()).then(|| "raw".to_string())
        });
        let bin = input
            .bin
            .iter()
            .map(|b| {
                if os == Some("windows") {
                    Bin::named(windows_exe(&b.path), windows_exe(&b.name))
                } else {
                    b.clone()
                }
            })
            .collect();
        let url = input.url.clone().or_else(|| {
            request
                .url_base
                .map(|base| format!("{}/{name}", base.trim_end_matches('/')))
        });
        let artifact = Artifact {
            url,
            name: name.clone(),
            os: os.map(str::to_string),
            arch: input.arch.or(arch).map(str::to_string),
            libc,
            variant: input.variant.clone(),
            size: digests.size,
            format,
            bin,
            requires: input.requires.clone(),
            provenance: input.provenance.clone(),
        };
        if let (Some(_), Some(_), Some(_)) = (&artifact.os, &artifact.arch, &artifact.format)
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
            prerelease: request.prerelease,
            channel: request.channel.map(str::to_string),
            source: request.source.clone(),
            artifacts,
            identity: request.identity.clone(),
            attested_by: request.attested_by,
            evidence: request.evidence.clone(),
            notes_url: request.notes_url.map(str::to_string),
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
    /// Withdrawn by the vendor, with the reason.
    pub yanked: Option<String>,
    pub security: bool,
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
            published_at: statement.predicate.published_at,
            packslip: listed.url.to_string(),
            prerelease: statement.predicate.prerelease,
            channel: statement.predicate.channel,
            status: listed
                .yanked
                .is_some()
                .then_some(crate::model::ReleaseStatus::Yanked),
            status_reason: listed.yanked.clone(),
            security: listed.security,
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
            infer_platform("Tool-x64.msix"),
            (Some("windows"), Some("x86_64"), None, Some("msix"))
        );
        assert_eq!(
            infer_platform("LM-Studio-0.3.0-x64.AppImage"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("appimage"))
        );
        assert_eq!(
            infer_platform("tool-linux-x86-64.tar.zst"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("tar.zst"))
        );
        assert_eq!(
            infer_platform("tool-linux-x64.7z"),
            (Some("linux"), Some("x86_64"), Some("gnu"), Some("7z"))
        );
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
        let request = |artifacts, source, url_base, supersedes| Request {
            published_at: Some("2026-09-01T00:00:00Z"),
            source,
            artifacts,
            url_base,
            supersedes,
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
        assert_eq!(arts[0].bin, [Bin::new("tool")]);
        assert_eq!(
            arts[2].bin,
            [Bin::named("bin/tool.exe", "tool.exe")],
            "windows gets .exe on path and name"
        );
        assert_eq!(arts[3].format.as_deref(), Some("raw"));
        assert_eq!(
            arts[3].url.as_deref(),
            Some("https://cdn.example.com/tool-linux-arm64")
        );
        assert_eq!(arts[3].bin[0].name, "tool");
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

        // Two artifacts for one platform need a variant.
        let fips = dir.path().join("tool-fips-v1.0.0-linux-x64.tar.xz");
        std::fs::write(&fips, b"fips bytes").unwrap();
        let err = create(&request(
            vec![input(&a, None, &[], &[]), input(&fips, None, &[], &[])],
            None,
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
            ..request(vec![input(&b, None, &[], &[])], None, None, None)
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
            releases: vec![ListedRelease {
                url: "https://dl.example.com/1.0.0/packslip.sigstore.json",
                bundle_path: &bundle_path,
                yanked: Some("bad build".into()),
                security: true,
            }],
            identity: key_identity(&key),
        })
        .unwrap();
        assert_eq!(list.statement.predicate.expires_at, "2026-10-01T01:00:00Z");
        let entry = &list.statement.predicate.releases[0];
        assert_eq!(entry.version, "1.0.0");
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
            releases: vec![
                ListedRelease {
                    url: "https://x/a.sigstore.json",
                    bundle_path: &bundle_path,
                    yanked: None,
                    security: false,
                },
                ListedRelease {
                    url: "https://x/b.sigstore.json",
                    bundle_path: &bundle_path,
                    yanked: None,
                    security: false,
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
            releases: vec![ListedRelease {
                url: "https://x/packslip.sigstore.json",
                bundle_path: &bundle_path,
                yanked: None,
                security: false,
            }],
            identity: key_identity(&key),
        })
        .unwrap_err();
        assert!(err.to_string().contains("the list is for"), "{err}");
    }
}
