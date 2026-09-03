//! Verifying a packslip: the signature against what the consumer pinned,
//! then the document structure, then the digests of any artifacts at hand.

use std::path::Path;

use crate::minisign::{PublicKey, Sig};
use crate::model::{InvalidDocument, Level, Scheme, Statement};
use crate::sigstore;

/// The signature that came with the document.
#[derive(Debug, Clone, Copy)]
pub enum Signature<'a> {
    /// The text of `packslip.json.minisig`.
    Minisign(&'a str),
    /// The JSON of `packslip.sigstore.json`.
    Sigstore(&'a str),
}

/// What the consumer pinned.
pub enum Trust<'a> {
    /// A minisign public key.
    Minisign(&'a PublicKey),
    /// An identity policy for a sigstore bundle, and the trusted root to
    /// verify the bundle against.
    Sigstore {
        policy: &'a sigstore::Policy,
        trusted_root: &'a sigstore_trust_root::TrustedRoot,
    },
}

/// What a successful verification established.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Verified {
    pub project: String,
    pub version: String,
    pub published_at: String,
    pub scheme: Scheme,
    /// Who signed: the minisign key id, or the certificate identity.
    pub key_id: String,
    /// The OIDC issuer, for the sigstore schemes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// When the transparency log recorded the signature, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logged_at: Option<String>,
    pub level: Level,
    /// Artifacts whose digests were checked against files.
    pub checked_artifacts: Vec<String>,
    pub artifact_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("document is invalid: {0}")]
    Invalid(#[from] InvalidDocument),
    #[error("document declares identity scheme {0}, which this build cannot verify")]
    UnsupportedScheme(Scheme),
    #[error("document declares scheme {declared} but a {given} signature was given")]
    SchemeMismatch {
        declared: Scheme,
        given: &'static str,
    },
    #[error("document declares signer {declared:?}, the signature is by {actual:?}")]
    DeclaredSignerMismatch { declared: String, actual: String },
    #[error("document declares issuer {declared:?}, the certificate says {actual:?}")]
    DeclaredIssuerMismatch { declared: String, actual: String },
    #[error("the signed statement in the bundle differs from the document file")]
    PayloadMismatch,
    #[error("signature: {0}")]
    Signature(#[from] crate::minisign::Error),
    #[error("{0}")]
    Sigstore(#[from] sigstore::Error),
    #[error("artifact {name}: {why}")]
    Artifact { name: String, why: String },
}

/// Verify document bytes with their signature against what the consumer
/// trusts, then check any local artifacts by file name.
pub fn verify(
    document: &[u8],
    signature: Signature<'_>,
    trust: &Trust<'_>,
    artifacts: &[&Path],
) -> Result<Verified, Error> {
    let statement: Statement = serde_json::from_slice(document)?;
    statement.validate()?;
    let identity = &statement.predicate.identity;
    let (key_id, issuer, logged_at) = match (identity.scheme, signature, trust) {
        (Scheme::Minisign, Signature::Minisign(sig_text), Trust::Minisign(pubkey)) => {
            let sig = Sig::parse(sig_text)?;
            pubkey.verify(document, &sig)?;
            let actual = crate::minisign::key_id_hex(&pubkey.key_id);
            if !identity.key_id.eq_ignore_ascii_case(&actual) {
                return Err(Error::DeclaredSignerMismatch {
                    declared: identity.key_id.clone(),
                    actual,
                });
            }
            (actual, None, None)
        }
        (
            Scheme::SigstoreOidc,
            Signature::Sigstore(bundle),
            Trust::Sigstore {
                policy,
                trusted_root,
            },
        ) => {
            let verified = sigstore::verify(bundle, policy, trusted_root)?;
            if verified.statement != document {
                return Err(Error::PayloadMismatch);
            }
            if identity.key_id != verified.identity {
                return Err(Error::DeclaredSignerMismatch {
                    declared: identity.key_id.clone(),
                    actual: verified.identity,
                });
            }
            if let Some(declared) = &identity.issuer
                && *declared != verified.issuer
            {
                return Err(Error::DeclaredIssuerMismatch {
                    declared: declared.clone(),
                    actual: verified.issuer,
                });
            }
            let logged_at = verified
                .integrated_time
                .and_then(|t| jiff::Timestamp::from_second(t).ok())
                .map(|t| t.to_string());
            (verified.identity, Some(verified.issuer), logged_at)
        }
        (Scheme::SigstoreKey, _, _) => return Err(Error::UnsupportedScheme(Scheme::SigstoreKey)),
        (declared, Signature::Minisign(_), _) => {
            return Err(Error::SchemeMismatch {
                declared,
                given: "minisign",
            });
        }
        (declared, Signature::Sigstore(_), _) => {
            return Err(Error::SchemeMismatch {
                declared,
                given: "sigstore",
            });
        }
    };
    let mut checked = Vec::new();
    for path in artifacts {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let Some(expected) = statement.digest_of(&name) else {
            return Err(Error::Artifact {
                name,
                why: "not listed in the document".into(),
            });
        };
        let (actual, size) = crate::digest_file(path).map_err(|e| Error::Artifact {
            name: name.clone(),
            why: e.to_string(),
        })?;
        if actual != expected {
            return Err(Error::Artifact {
                name,
                why: format!("sha256 is {actual}, document says {expected}"),
            });
        }
        let declared_size = statement
            .predicate
            .artifacts
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.size);
        if let Some(declared) = declared_size
            && declared != size
        {
            return Err(Error::Artifact {
                name,
                why: format!("size is {size}, document says {declared}"),
            });
        }
        checked.push(name);
    }
    Ok(Verified {
        project: statement.predicate.project.clone(),
        version: statement.predicate.version.clone(),
        published_at: statement.predicate.published_at.clone(),
        scheme: identity.scheme,
        key_id,
        issuer,
        logged_at,
        level: statement.declared_level(),
        checked_artifacts: checked,
        artifact_count: statement.predicate.artifacts.len(),
    })
}

/// The minisign convenience: document, `.minisig` text, pinned key.
pub fn verify_minisign(
    document: &[u8],
    signature: &str,
    pubkey: &PublicKey,
    artifacts: &[&Path],
) -> Result<Verified, Error> {
    verify(
        document,
        Signature::Minisign(signature),
        &Trust::Minisign(pubkey),
        artifacts,
    )
}
