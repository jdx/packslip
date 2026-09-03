//! Verifying a packslip: the bundle against what the consumer pinned, then
//! the statement, then the digests of any artifacts at hand. And the same
//! for a release list.

use std::path::Path;

use sigstore_trust_root::TrustedRoot;

use crate::model::{InvalidDocument, ReleaseListStatement, Scheme, Statement};
use crate::sigstore::{self, SignedBy, Trust};

/// How strict to be.
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    /// Refuse a bundle without a Rekor entry. On by default; a consumer
    /// turns it off only for a vendor it has agreed to accept unlogged.
    pub require_log: bool,
    pub trusted_root: &'a TrustedRoot,
}

/// What a successful verification established.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Verified {
    pub project: String,
    pub version: String,
    pub published_at: String,
    pub scheme: Scheme,
    /// Who signed: the certificate identity, or the key id.
    pub key_id: String,
    /// Whether the vendor or a repackager made the claim.
    pub attested_by: crate::model::Attestor,
    pub version_order: crate::model::VersionOrder,
    pub prerelease: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// The OIDC issuer, for `sigstore-oidc`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// When the transparency log recorded the signature, RFC 3339. None
    /// only for an unlogged bundle the consumer chose to accept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logged_at: Option<String>,
    /// Whether every artifact links build provenance for the consumer to
    /// verify. The packslip proves the manifest; SLSA provenance, if
    /// verified, proves the build.
    pub provenance_linked: bool,
    /// Artifacts whose digests were checked against files.
    pub checked_artifacts: Vec<String>,
    pub artifact_count: usize,
}

/// What verifying a release list established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedList {
    pub list: ReleaseListStatement,
    pub scheme: Scheme,
    pub key_id: String,
    pub issuer: Option<String>,
    pub logged_at: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("statement is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("statement is invalid: {0}")]
    Invalid(#[from] InvalidDocument),
    #[error("statement declares scheme {declared} but the bundle was signed by {actual}")]
    SchemeMismatch {
        declared: Scheme,
        actual: &'static str,
    },
    #[error("statement declares signer {declared:?}, the bundle was signed by {actual:?}")]
    DeclaredSignerMismatch { declared: String, actual: String },
    #[error("statement declares issuer {declared:?}, the certificate says {actual:?}")]
    DeclaredIssuerMismatch { declared: String, actual: String },
    #[error("{0}")]
    Sigstore(#[from] sigstore::Error),
    #[error("artifact {name}: {why}")]
    Artifact { name: String, why: String },
}

/// Check the statement's own `identity` block against who actually signed.
fn check_declared(
    identity: &crate::model::Identity,
    signed_by: &SignedBy,
) -> Result<(Scheme, String, Option<String>), Error> {
    match (identity.scheme, signed_by) {
        (
            Scheme::SigstoreOidc,
            SignedBy::Identity {
                identity: actual,
                issuer,
            },
        ) => {
            if identity.key_id != *actual {
                return Err(Error::DeclaredSignerMismatch {
                    declared: identity.key_id.clone(),
                    actual: actual.clone(),
                });
            }
            if let Some(declared) = &identity.issuer
                && declared != issuer
            {
                return Err(Error::DeclaredIssuerMismatch {
                    declared: declared.clone(),
                    actual: issuer.clone(),
                });
            }
            Ok((Scheme::SigstoreOidc, actual.clone(), Some(issuer.clone())))
        }
        (Scheme::SigstoreKey, SignedBy::Key { key_id }) => {
            if !identity.key_id.eq_ignore_ascii_case(key_id) {
                return Err(Error::DeclaredSignerMismatch {
                    declared: identity.key_id.clone(),
                    actual: key_id.clone(),
                });
            }
            Ok((Scheme::SigstoreKey, key_id.clone(), None))
        }
        (declared, SignedBy::Identity { .. }) => Err(Error::SchemeMismatch {
            declared,
            actual: "an OIDC identity",
        }),
        (declared, SignedBy::Key { .. }) => Err(Error::SchemeMismatch {
            declared,
            actual: "a key",
        }),
    }
}

fn logged_at(integrated_time: Option<i64>) -> Option<String> {
    integrated_time
        .and_then(|t| jiff::Timestamp::from_second(t).ok())
        .map(|t| t.to_string())
}

/// Verify a packslip bundle against what the consumer trusts, then check
/// any local artifacts by file name.
pub fn verify(
    bundle: &str,
    trust: &Trust<'_>,
    options: Options<'_>,
    artifacts: &[&Path],
) -> Result<Verified, Error> {
    let verified = sigstore::verify(bundle, trust, options.require_log, options.trusted_root)?;
    let statement: Statement = serde_json::from_slice(&verified.statement)?;
    statement.validate()?;
    let (scheme, key_id, issuer) =
        check_declared(&statement.predicate.identity, &verified.signed_by)?;
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
        scheme,
        key_id,
        attested_by: statement.predicate.attested_by,
        version_order: statement.predicate.version_order,
        prerelease: statement.predicate.prerelease,
        channel: statement.predicate.channel.clone(),
        issuer,
        logged_at: logged_at(verified.integrated_time),
        provenance_linked: statement.provenance_linked(),
        checked_artifacts: checked,
        artifact_count: statement.predicate.artifacts.len(),
    })
}

/// Verify a release-list bundle against what the consumer trusts. The
/// caller checks expiry and sequence against what it has seen.
pub fn verify_release_list(
    bundle: &str,
    trust: &Trust<'_>,
    options: Options<'_>,
) -> Result<VerifiedList, Error> {
    let verified = sigstore::verify(bundle, trust, options.require_log, options.trusted_root)?;
    let list: ReleaseListStatement = serde_json::from_slice(&verified.statement)?;
    list.validate()?;
    let (scheme, key_id, issuer) = check_declared(&list.predicate.identity, &verified.signed_by)?;
    Ok(VerifiedList {
        list,
        scheme,
        key_id,
        issuer,
        logged_at: logged_at(verified.integrated_time),
    })
}
