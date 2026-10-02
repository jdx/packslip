//! Verifying a packslip: the bundle against what the consumer pinned, then
//! the statement, then the digests of any artifacts at hand. And the same
//! for a release list.

use std::path::Path;

use sigstore_trust_root::TrustedRoot;

use crate::forge;
use crate::model::{InvalidDocument, ReleaseListStatement, Scheme, Statement};
use crate::sigstore::{self, Policy, SignedBy, Trust};

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
    /// Whether the version has a prerelease part.
    pub prerelease: bool,
    /// The channel the version's prerelease part names, if any.
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
    /// The kinds of resources declared (completions, man pages, CLI
    /// specs, ...), one label per entry, in document order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    /// What each artifact requires of the host, one line per artifact
    /// that declares anything: `name: glibc>=2.31; libs libz.so.1; bin
    /// java>=17`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
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
    let version = crate::model::parse_version(&statement.predicate.version)?;
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
        prerelease: !version.pre.is_empty(),
        channel: crate::model::channel(&version).map(str::to_string),
        issuer,
        logged_at: logged_at(verified.integrated_time),
        provenance_linked: statement.provenance_linked(),
        checked_artifacts: checked,
        artifact_count: statement.predicate.artifacts.len(),
        resources: statement
            .predicate
            .resources
            .iter()
            .map(|r| {
                format!(
                    "{} ({})",
                    r.label(),
                    r.source().map_or("?".to_string(), |s| s.to_string())
                )
            })
            .collect(),
        requires: statement
            .predicate
            .artifacts
            .iter()
            .filter_map(|a| {
                let requires = a.requires.as_ref().filter(|r| !r.is_empty())?;
                Some(format!("{}: {}", a.name, requires.summary()))
            })
            .collect(),
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

/// What [`verify_forge`] or [`verify_forge_release_list`] established: the
/// verification itself, and how the signing repository relates to the
/// one the consumer expected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ForgeVerified<T> {
    /// A [`Verified`] or a [`VerifiedList`].
    pub verified: T,
    /// The continuity, the certificate's source repository, and the pin
    /// to remember once the release is accepted.
    pub check: forge::Check,
}

/// Why [`verify_forge`] or [`verify_forge_release_list`] refused a bundle.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ForgeError {
    /// The bundle or its statement did not verify.
    #[error(transparent)]
    Verify(#[from] Error),
    /// It verified, but not as the project the consumer expected.
    #[error(transparent)]
    Identity(#[from] forge::IdentityError),
}

/// What a bundle's statement claims, read by [`peek_unverified`] without
/// verifying anything. Nothing in it is established: an attacker can put
/// any project and version in a bundle. Use it only to decide how to
/// verify, such as whether a forge lookup is worth making, and take the
/// project and version from [`Verified`] afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Claimed {
    /// The statement's `predicateType`: [`crate::model::PREDICATE_TYPE`]
    /// for a release, [`crate::model::RELEASES_PREDICATE_TYPE`] for a
    /// release list.
    pub predicate_type: String,
    /// The project the statement names.
    pub project: String,
    /// The version it names; none for a release list.
    pub version: Option<String>,
}

/// Read the project and version a bundle's statement claims, **without
/// verifying the bundle**. The result is untrusted: see [`Claimed`].
/// Fails when the bundle is not a DSSE-wrapped in-toto statement or the
/// statement names no project.
pub fn peek_unverified(bundle: &str) -> Result<Claimed, Error> {
    #[derive(serde::Deserialize)]
    struct Peek {
        #[serde(rename = "predicateType")]
        predicate_type: String,
        predicate: PeekPredicate,
    }
    #[derive(serde::Deserialize)]
    struct PeekPredicate {
        project: String,
        version: Option<String>,
    }
    let payload = sigstore::peek_statement(bundle)?;
    let peek: Peek = serde_json::from_slice(&payload)?;
    let is_release = peek.predicate_type == crate::model::PREDICATE_TYPE;
    Ok(Claimed {
        predicate_type: peek.predicate_type,
        project: peek.predicate.project,
        version: peek.predicate.version.filter(|_| is_release),
    })
}

/// The policy to verify a forge project's bundle under: see
/// [`forge::policy`].
fn forge_policy(bundle: &str, expected: &forge::Expected<'_>) -> Result<Policy, ForgeError> {
    let payload = sigstore::peek_statement(bundle).map_err(Error::from)?;
    let peeked: serde_json::Value = serde_json::from_slice(&payload).map_err(Error::from)?;
    let signed = peeked["predicate"]["project"].as_str().unwrap_or_default();
    forge::policy(expected.project, signed)
        .ok_or_else(|| forge::IdentityError::NotForge(expected.project.to_string()).into())
}

/// Verify a GitHub or GitLab project's packslip under the policy its forge
/// implies, then check that it was signed by the repository the consumer
/// expected, by the forge's immutable repository ID where the certificate
/// and the consumer's pin or forge lookup give one ([`forge::check`]). A
/// renamed or transferred repository's releases verify under the new name,
/// and its older releases under the old one; a recreated name does not.
///
/// The caller still applies signer continuity, with
/// [`forge::Check::continues_signer`], and records
/// [`forge::Check::pin`] once it accepts the release.
pub fn verify_forge(
    bundle: &str,
    expected: &forge::Expected<'_>,
    options: Options<'_>,
    artifacts: &[&Path],
) -> Result<ForgeVerified<Verified>, ForgeError> {
    let policy = forge_policy(bundle, expected)?;
    let verified = verify(bundle, &Trust::Identity(&policy), options, artifacts)?;
    let source = sigstore::source_repository(bundle).map_err(Error::from)?;
    let check = forge::check(
        expected,
        &verified.project,
        &verified.key_id,
        verified.issuer.as_deref(),
        source.as_ref(),
    )?;
    Ok(ForgeVerified { verified, check })
}

/// [`verify_forge`] for a forge project's supplementary release list.
pub fn verify_forge_release_list(
    bundle: &str,
    expected: &forge::Expected<'_>,
    options: Options<'_>,
) -> Result<ForgeVerified<VerifiedList>, ForgeError> {
    let policy = forge_policy(bundle, expected)?;
    let verified = verify_release_list(bundle, &Trust::Identity(&policy), options)?;
    let source = sigstore::source_repository(bundle).map_err(Error::from)?;
    let check = forge::check(
        expected,
        &verified.list.predicate.project,
        &verified.key_id,
        verified.issuer.as_deref(),
        source.as_ref(),
    )?;
    Ok(ForgeVerified { verified, check })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::{Continuity, Evidence, Expected, ForgePin, IdentityError, PinSource};

    /// jdx/hk's v2.3.0 packslip, as its release workflow published it.
    const HK: &str = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");

    fn options(root: &TrustedRoot) -> Options<'_> {
        Options {
            require_log: true,
            trusted_root: root,
        }
    }

    #[test]
    fn a_published_certificate_records_the_repository_ids() {
        let source = sigstore::source_repository(HK).unwrap().unwrap();
        assert_eq!(source.uri, "https://github.com/jdx/hk");
        assert_eq!(source.id.as_deref(), Some("922514152"));
        assert_eq!(source.owner_uri.as_deref(), Some("https://github.com/jdx"));
        assert_eq!(source.owner_id.as_deref(), Some("216188"));
    }

    #[test]
    fn a_published_release_verifies_by_repository_id() {
        let root = sigstore::trusted_root(None).unwrap();
        let ok =
            verify_forge(HK, &Expected::new("github.com/jdx/hk"), options(&root), &[]).unwrap();
        assert_eq!(ok.verified.project, "github.com/jdx/hk");
        assert_eq!(ok.check.continuity, Continuity::Same);
        let pin = ok.check.pin.clone().unwrap();
        assert_eq!(pin, ForgePin::of("github.com/jdx/hk", "922514152"));
        assert!(
            ok.check
                .continues_signer("https://github.com/jdx/hk/.github/workflows/release.yml")
        );

        // Were hk renamed to hook, a user asking for the new name with the
        // pin would still take this release, signed under the old name.
        let pinned = ForgePin::of("github.com/jdx/hook", "922514152");
        let ok = verify_forge(
            HK,
            &Expected::new("github.com/jdx/hook").pinned(Some(&pinned)),
            options(&root),
            &[],
        )
        .unwrap();
        assert_eq!(
            ok.check.continuity,
            Continuity::Renamed {
                requested: "github.com/jdx/hook".into(),
                signed: "github.com/jdx/hk".into(),
            }
        );
        // Without the pin or a forge lookup, another name is refused.
        let err = verify_forge(
            HK,
            &Expected::new("github.com/jdx/hook"),
            options(&root),
            &[],
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ForgeError::Identity(IdentityError::ProjectMismatch { .. })
            ),
            "{err}"
        );
        // A pin for another repository under the same name is a recreated name.
        let other = ForgePin::of("github.com/jdx/hk", "1");
        let err = verify_forge(
            HK,
            &Expected::new("github.com/jdx/hk").pinned(Some(&other)),
            options(&root),
            &[],
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ForgeError::Identity(IdentityError::DifferentRepository {
                    evidence: Evidence::Pin,
                    ..
                })
            ),
            "{err}"
        );
        // A monorepo tool of the repository is not the repository.
        let err = verify_forge(
            HK,
            &Expected::new("github.com/jdx/hk/tool"),
            options(&root),
            &[],
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ForgeError::Identity(IdentityError::ProjectMismatch { .. })
            ),
            "{err}"
        );
        // Only forge projects.
        let err = verify_forge(HK, &Expected::new("hk.jdx.dev"), options(&root), &[]).unwrap_err();
        assert!(
            matches!(err, ForgeError::Identity(IdentityError::NotForge(_))),
            "{err}"
        );
    }

    #[test]
    fn a_published_release_is_held_to_every_pin() {
        let root = sigstore::trusted_root(None).unwrap();
        let pins = [
            (
                PinSource::Local,
                ForgePin::of("github.com/jdx/hk", "922514152"),
            ),
            (PinSource::Lockfile, ForgePin::of("github.com/jdx/hk", "1")),
        ];
        let err = verify_forge(
            HK,
            &Expected::new("github.com/jdx/hk").pinned_by(&pins),
            options(&root),
            &[],
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ForgeError::Identity(IdentityError::DifferentRepository {
                    evidence: Evidence::LockfilePin,
                    ..
                })
            ),
            "{err}"
        );
        let ok = verify_forge(
            HK,
            &Expected::new("github.com/jdx/hk").pinned_by(&pins[..1]),
            options(&root),
            &[],
        )
        .unwrap();
        assert_eq!(ok.check.continuity, Continuity::Same);
    }

    #[test]
    fn a_bundle_claims_a_project_before_it_is_verified() {
        let claimed = peek_unverified(HK).unwrap();
        assert_eq!(
            claimed,
            Claimed {
                predicate_type: crate::model::PREDICATE_TYPE.into(),
                project: "github.com/jdx/hk".into(),
                version: Some("2.3.0".into()),
            }
        );

        // The claim is whatever the statement says, signed or not: here a
        // release list for another project, under hk's signature.
        use base64::Engine as _;
        let mut bundle: serde_json::Value = serde_json::from_str(HK).unwrap();
        let list = serde_json::json!({
            "_type": crate::model::STATEMENT_TYPE,
            "subject": [],
            "predicateType": crate::model::RELEASES_PREDICATE_TYPE,
            "predicate": { "project": "github.com/evil/x", "version": "9.9.9" },
        });
        bundle["dsseEnvelope"]["payload"] = base64::engine::general_purpose::STANDARD
            .encode(list.to_string())
            .into();
        let forged = bundle.to_string();
        let claimed = peek_unverified(&forged).unwrap();
        assert_eq!(claimed.project, "github.com/evil/x");
        assert_eq!(claimed.version, None, "a release list has no version");
        let root = sigstore::trusted_root(None).unwrap();
        assert!(
            verify_forge_release_list(&forged, &Expected::new("github.com/evil/x"), options(&root))
                .is_err()
        );

        assert!(peek_unverified("not a bundle").is_err());
    }
}
