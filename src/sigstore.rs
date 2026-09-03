//! Signing and verification through sigstore. A packslip is a sigstore
//! bundle carrying the statement as an in-toto DSSE envelope. It is signed
//! either keylessly, with an ephemeral key certified by Fulcio for an OIDC
//! identity, or with a long-lived Ed25519 key; either way the signature is
//! logged to Rekor, and the bundle carries the log entry.

use std::borrow::Cow;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use ed25519_dalek::Signer as _;
use sha2::Digest as _;
use sigstore_bundle::{BundleV03, TlogEntryBuilder, VerificationMaterialV03};
use sigstore_oidc::IdentityToken;
use sigstore_rekor::{DsseEntry, RekorClient};
use sigstore_sign::SigningContext;
use sigstore_trust_root::{SIGSTORE_PRODUCTION_TRUSTED_ROOT, TrustedRoot};
use sigstore_types::{
    Artifact, Bundle, DerPublicKey, DsseEnvelope, DsseSignature, KeyId, PayloadBytes, Sha256Hash,
    SignatureBytes, SignatureContent,
};
use sigstore_verify::VerificationPolicy;

use crate::minisign::{PublicKey, SecretKey, key_id_hex};

pub const GITHUB_ISSUER: &str = "https://token.actions.githubusercontent.com";
pub const GITLAB_ISSUER: &str = "https://gitlab.com";
pub const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

/// The environment variable a caller may set to hand over an OIDC token
/// directly, when no ambient CI credential is available.
pub const TOKEN_ENV: &str = "SIGSTORE_ID_TOKEN";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "no OIDC identity available: run inside a CI job with an id-token permission, or set {TOKEN_ENV}"
    )]
    NoIdentity,
    #[error("identity token: {0}")]
    Token(String),
    #[error("signing: {0}")]
    Sign(String),
    #[error("transparency log: {0}")]
    Log(String),
    #[error("bundle is not valid JSON: {0}")]
    BundleJson(String),
    #[error("bundle does not carry a DSSE envelope")]
    NotDsse,
    #[error("bundle payload type is {0:?}, expected {IN_TOTO_PAYLOAD_TYPE}")]
    PayloadType(String),
    #[error("bundle payload is not an in-toto statement: {0}")]
    Payload(String),
    #[error("trusted root: {0}")]
    TrustedRoot(String),
    #[error("bundle does not verify: {0}")]
    Verification(String),
    #[error("signature does not verify with the pinned key {0}")]
    BadKeySignature(String),
    #[error("bundle carries a certificate but a key was pinned; pin an identity instead")]
    KeyPinnedForCertificate,
    #[error("bundle carries a public key hint but an identity was pinned; pin the key instead")]
    IdentityPinnedForKey,
    #[error("signed by {actual:?}, expected an identity starting with {expected:?}")]
    IdentityPrefix { actual: String, expected: String },
    #[error("bundle has no transparency log entry; pass --allow-unlogged to accept one")]
    Unlogged,
    #[error(
        "no identity to verify against for {0:?}: pass --identity, --identity-prefix, or --issuer"
    )]
    NoPolicy(String),
}

fn runtime() -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Sign(format!("tokio runtime: {e}")))
}

/// An OIDC identity ready to sign with, and what Fulcio will put in the
/// certificate for it.
pub struct OidcIdentity {
    token: IdentityToken,
    /// The certificate subject identity: a workflow URI for CI, an email
    /// for a person.
    pub identity: String,
    pub issuer: String,
}

impl std::fmt::Debug for OidcIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcIdentity")
            .field("identity", &self.identity)
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

impl OidcIdentity {
    fn from_token(token: IdentityToken) -> Result<OidcIdentity, Error> {
        let identity = certificate_identity(token.raw())?;
        Ok(OidcIdentity {
            issuer: token.issuer().to_string(),
            identity,
            token,
        })
    }
}

/// The identity from `SIGSTORE_ID_TOKEN`, or the ambient CI credential
/// (GitHub Actions, GitLab CI, Buildkite, and the others sigstore knows).
pub fn ambient_identity() -> Result<OidcIdentity, Error> {
    if let Ok(raw) = std::env::var(TOKEN_ENV)
        && !raw.trim().is_empty()
    {
        let token = IdentityToken::from_jwt(raw.trim()).map_err(|e| Error::Token(e.to_string()))?;
        return OidcIdentity::from_token(token);
    }
    let rt = runtime()?;
    let detected = rt
        .block_on(IdentityToken::detect_ambient())
        .map_err(|e| Error::Token(e.to_string()))?;
    match detected {
        Some(token) => OidcIdentity::from_token(token),
        None => Err(Error::NoIdentity),
    }
}

/// What Fulcio writes as the certificate's subject for this token: for CI
/// tokens the workflow URI, otherwise the verified email or the subject.
pub fn certificate_identity(jwt: &str) -> Result<String, Error> {
    let payload = jwt
        .split('.')
        .nth(1)
        .ok_or_else(|| Error::Token("not a JWT".into()))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| Error::Token("payload is not base64url".into()))?;
    let claims: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| Error::Token(e.to_string()))?;
    let str_claim = |name: &str| claims.get(name).and_then(|v| v.as_str());
    let issuer = str_claim("iss").unwrap_or_default();
    if issuer == GITHUB_ISSUER
        && let Some(workflow) = str_claim("job_workflow_ref")
    {
        return Ok(format!("https://github.com/{workflow}"));
    }
    if let Some(ci_config) = str_claim("ci_config_ref_uri") {
        return Ok(format!("https://{ci_config}"));
    }
    if let Some(email) = str_claim("email")
        && claims
            .get("email_verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    {
        return Ok(email.to_string());
    }
    str_claim("sub")
        .map(str::to_string)
        .ok_or_else(|| Error::Token("no sub claim".into()))
}

/// Who signs.
pub enum Signer {
    /// Keyless, with an OIDC identity.
    Oidc(OidcIdentity),
    /// A long-lived Ed25519 key. With `log`, the signature is recorded in
    /// Rekor; without it the bundle carries no log entry and consumers
    /// must opt in to accept it.
    Key { key: SecretKey, log: bool },
}

impl Signer {
    /// The `identity` block a document signed by this signer declares.
    pub fn identity(&self) -> crate::model::Identity {
        match self {
            Signer::Oidc(oidc) => crate::model::Identity {
                scheme: crate::model::Scheme::SigstoreOidc,
                key_id: oidc.identity.clone(),
                issuer: Some(oidc.issuer.clone()),
            },
            Signer::Key { key, .. } => crate::model::Identity {
                scheme: crate::model::Scheme::SigstoreKey,
                key_id: key_id_hex(&key.public_key().key_id),
                issuer: None,
            },
        }
    }
}

/// The Ed25519 public key as DER SubjectPublicKeyInfo, which is how
/// sigstore and Rekor want keys.
pub fn spki_der(key: &PublicKey) -> Vec<u8> {
    const PREFIX: [u8; 12] = [
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let mut der = PREFIX.to_vec();
    der.extend_from_slice(key.key.as_bytes());
    der
}

fn spki_pem(key: &PublicKey) -> String {
    let b64 = BASE64.encode(spki_der(key));
    let mut pem = String::from("-----BEGIN PUBLIC KEY-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        pem.push('\n');
    }
    pem.push_str("-----END PUBLIC KEY-----\n");
    pem
}

/// The key hint a bundle carries for a pinned key: base64 of the sha256
/// of the DER key, the convention cosign uses.
pub fn key_hint(key: &PublicKey) -> String {
    BASE64.encode(sha2::Sha256::digest(spki_der(key)))
}

/// Sign `statement` (the payload bytes) into a sigstore bundle, returned
/// as JSON.
pub fn sign(signer: Signer, statement: &[u8]) -> Result<String, Error> {
    let rt = runtime()?;
    let bundle = match signer {
        Signer::Oidc(oidc) => {
            let context = SigningContext::production();
            let signer = context.signer(oidc.token);
            rt.block_on(signer.sign_raw_statement(statement))
                .map_err(|e| Error::Sign(e.to_string()))?
        }
        Signer::Key { key, log } => {
            let pae = sigstore_types::pae(IN_TOTO_PAYLOAD_TYPE, statement);
            let signature = key.signing_key().sign(&pae);
            let envelope = DsseEnvelope::new(
                IN_TOTO_PAYLOAD_TYPE.to_string(),
                PayloadBytes::from_bytes(statement),
                vec![DsseSignature {
                    sig: SignatureBytes::from_bytes(&signature.to_bytes()),
                    keyid: KeyId::default(),
                }],
            );
            let public = key.public_key();
            let mut bundle = BundleV03::new(
                VerificationMaterialV03::PublicKey {
                    hint: key_hint(&public),
                },
                SignatureContent::DsseEnvelope(envelope.clone()),
            );
            if log {
                let entry = DsseEntry {
                    api_version: "0.0.1".into(),
                    kind: "dsse".into(),
                    spec: sigstore_rekor::entry::DsseEntrySpec {
                        proposed_content: Some(sigstore_rekor::entry::DsseProposedContent {
                            envelope: serde_json::to_string(&envelope)
                                .map_err(|e| Error::Sign(e.to_string()))?,
                            verifiers: vec![BASE64.encode(spki_pem(&public))],
                        }),
                        signatures: Vec::new(),
                    },
                };
                let rekor = RekorClient::public();
                let log_entry = rt
                    .block_on(rekor.create_dsse_entry(entry))
                    .map_err(|e| Error::Log(e.to_string()))?;
                bundle = bundle.with_tlog_entry(
                    TlogEntryBuilder::from_log_entry(&log_entry, "dsse", "0.0.1").build(),
                );
            }
            bundle.into_bundle()
        }
    };
    bundle.to_json().map_err(|e| Error::Sign(e.to_string()))
}

/// Who a consumer accepts as a keyless signer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    /// The exact OIDC issuer, when pinned.
    pub issuer: Option<String>,
    /// The exact certificate identity, when pinned.
    pub identity: Option<String>,
    /// A prefix the certificate identity must start with, for pinning a
    /// repository rather than one workflow file and ref.
    pub identity_prefix: Option<String>,
}

impl Policy {
    /// The policy a project name implies on a forge the tooling knows:
    /// `github.com/owner/repo` must be signed by a workflow of that
    /// repository through GitHub's issuer, and likewise for gitlab.com.
    pub fn for_project(project: &str) -> Option<Policy> {
        // A GitHub project may name a tool inside a monorepo
        // (`github.com/owner/repo/tool`); the workflow that signs it
        // belongs to the repository, so only owner and repo form the pin.
        if let Some((host, owner, repo)) = crate::model::repository(project) {
            return Some(Policy {
                issuer: Some(GITHUB_ISSUER.into()),
                identity: None,
                identity_prefix: Some(format!("https://{host}/{owner}/{repo}/")),
            });
        }
        let (host, path) = project.split_once('/')?;
        if path.is_empty() || path.split('/').count() < 2 {
            return None;
        }
        match host {
            // GitLab subgroups make project paths arbitrary depth, so the
            // whole path is the pin and subpaths are not distinguished.
            "gitlab.com" => Some(Policy {
                issuer: Some(GITLAB_ISSUER.into()),
                identity: None,
                identity_prefix: Some(format!("https://gitlab.com/{path}//")),
            }),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.issuer.is_none() && self.identity.is_none() && self.identity_prefix.is_none()
    }

    /// One line saying what the policy pins.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(id) = &self.identity {
            parts.push(format!("identity {id}"));
        }
        if let Some(prefix) = &self.identity_prefix {
            parts.push(format!("identity under {prefix}"));
        }
        if let Some(issuer) = &self.issuer {
            parts.push(format!("issuer {issuer}"));
        }
        parts.join(", ")
    }
}

/// What the consumer pinned.
pub enum Trust<'a> {
    /// An identity policy for a keyless bundle.
    Identity(&'a Policy),
    /// A long-lived public key for a key-signed bundle.
    Key(&'a PublicKey),
}

impl Trust<'_> {
    /// One line saying what is pinned.
    pub fn describe(&self) -> String {
        match self {
            Trust::Identity(policy) => policy.describe(),
            Trust::Key(key) => format!("key {}", key_id_hex(&key.key_id)),
        }
    }
}

/// Who signed, as verification established it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedBy {
    /// A certificate identity and its issuer.
    Identity { identity: String, issuer: String },
    /// The pinned key, by minisign key id.
    Key { key_id: String },
}

/// What verifying a bundle established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleVerified {
    /// The signed statement bytes, exactly as they were signed.
    pub statement: Vec<u8>,
    pub signed_by: SignedBy,
    /// When Rekor integrated the entry, as a Unix timestamp; none for an
    /// unlogged bundle a consumer chose to accept.
    pub integrated_time: Option<i64>,
}

/// The trusted root: a file's contents, or the production root embedded in
/// this build.
pub fn trusted_root(json: Option<&str>) -> Result<TrustedRoot, Error> {
    TrustedRoot::from_json(json.unwrap_or(SIGSTORE_PRODUCTION_TRUSTED_ROOT))
        .map_err(|e| Error::TrustedRoot(e.to_string()))
}

/// The statement bytes inside a bundle, without verifying anything. For
/// `show`, and for deciding what to pin before verifying.
pub fn peek_statement(bundle_json: &str) -> Result<Vec<u8>, Error> {
    let bundle = Bundle::from_json(bundle_json).map_err(|e| Error::BundleJson(e.to_string()))?;
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err(Error::NotDsse);
    };
    if envelope.payload_type != IN_TOTO_PAYLOAD_TYPE {
        return Err(Error::PayloadType(envelope.payload_type.clone()));
    }
    Ok(envelope.decode_payload())
}

/// Verify a bundle against what the consumer pinned, requiring a Rekor
/// entry unless `require_log` is off. Returns the signed statement bytes.
///
/// For an identity: the certificate chains to the trusted root and was
/// valid when the log recorded the entry, the entry is in the log, the
/// DSSE signature checks out, and the signer satisfies the policy. For a
/// key: the DSSE signature checks out with that key, and the log entry, if
/// present or required, is verified against the trusted root.
pub fn verify(
    bundle_json: &str,
    trust: &Trust<'_>,
    require_log: bool,
    trusted_root: &TrustedRoot,
) -> Result<BundleVerified, Error> {
    let bundle = Bundle::from_json(bundle_json).map_err(|e| Error::BundleJson(e.to_string()))?;
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err(Error::NotDsse);
    };
    if envelope.payload_type != IN_TOTO_PAYLOAD_TYPE {
        return Err(Error::PayloadType(envelope.payload_type.clone()));
    }
    let statement = envelope.decode_payload();
    // sigstore binds a DSSE bundle to an artifact by subject digest; the
    // statement's own first subject is that artifact. The caller checks
    // the statement's contents and any real files afterwards.
    let parsed: serde_json::Value =
        serde_json::from_slice(&statement).map_err(|e| Error::Payload(e.to_string()))?;
    let first = parsed["subject"][0]["digest"]["sha256"]
        .as_str()
        .ok_or_else(|| Error::Payload("no subject with a sha256".into()))?;
    let digest = Sha256Hash::from_hex(first).map_err(|e| Error::Payload(e.to_string()))?;
    let artifact = Artifact::Digest(Cow::Borrowed(digest.as_bytes()));
    let logged = !bundle.verification_material.tlog_entries.is_empty();
    if require_log && !logged {
        return Err(Error::Unlogged);
    }
    let has_certificate = bundle.signing_certificate().is_some();

    match trust {
        Trust::Identity(policy) => {
            if !has_certificate {
                return Err(Error::IdentityPinnedForKey);
            }
            let mut verification = VerificationPolicy::default();
            if !require_log {
                verification = verification.skip_tlog();
            }
            if let Some(issuer) = &policy.issuer {
                verification = verification.require_issuer(issuer.clone());
            }
            if let Some(identity) = &policy.identity {
                verification = verification.require_identity(identity.clone());
            }
            let result = sigstore_verify::verify(artifact, &bundle, &verification, trusted_root)
                .map_err(|e| Error::Verification(e.to_string()))?;
            let identity = result
                .identity
                .ok_or_else(|| Error::Verification("certificate has no subject identity".into()))?;
            let issuer = result
                .issuer
                .ok_or_else(|| Error::Verification("certificate has no issuer".into()))?;
            if let Some(prefix) = &policy.identity_prefix
                && !identity.starts_with(prefix)
            {
                return Err(Error::IdentityPrefix {
                    actual: identity,
                    expected: prefix.clone(),
                });
            }
            Ok(BundleVerified {
                statement,
                signed_by: SignedBy::Identity { identity, issuer },
                integrated_time: result.integrated_time,
            })
        }
        Trust::Key(key) => {
            if has_certificate {
                return Err(Error::KeyPinnedForCertificate);
            }
            let key_id = key_id_hex(&key.key_id);
            // The signature itself, with our own key type.
            let pae = envelope.pae();
            let ok = envelope.signatures.iter().any(|s| {
                <[u8; 64]>::try_from(s.sig.as_bytes())
                    .ok()
                    .map(|b| ed25519_dalek::Signature::from_bytes(&b))
                    .is_some_and(|sig| {
                        ed25519_dalek::Verifier::verify(&key.key, &pae, &sig).is_ok()
                    })
            });
            if !ok {
                return Err(Error::BadKeySignature(key_id));
            }
            // The log entry, when there is one: inclusion, checkpoint, and
            // consistency with the envelope, by sigstore's verifier.
            let integrated_time = if logged {
                let der = DerPublicKey::from_bytes(&spki_der(key));
                sigstore_verify::verify_with_key(artifact, &bundle, &der, trusted_root)
                    .map_err(|e| Error::Verification(e.to_string()))?;
                bundle
                    .verification_material
                    .tlog_entries
                    .first()
                    .map(|e| e.integrated_time)
            } else {
                None
            };
            Ok(BundleVerified {
                statement,
                signed_by: SignedBy::Key { key_id },
                integrated_time,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(claims: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn github_actions_identity_is_the_workflow_uri() {
        let token = jwt(serde_json::json!({
            "iss": GITHUB_ISSUER,
            "sub": "repo:jdx/mise:ref:refs/tags/v1.0.0",
            "job_workflow_ref": "jdx/mise/.github/workflows/release.yml@refs/tags/v1.0.0",
            "aud": "sigstore", "exp": 4102444800u64,
        }));
        assert_eq!(
            certificate_identity(&token).unwrap(),
            "https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v1.0.0"
        );
        let email = jwt(serde_json::json!({
            "iss": "https://accounts.google.com", "sub": "123",
            "email": "me@example.com", "email_verified": true,
            "aud": "sigstore", "exp": 4102444800u64,
        }));
        assert_eq!(certificate_identity(&email).unwrap(), "me@example.com");
        let unverified = jwt(serde_json::json!({
            "iss": "https://x", "sub": "123", "email": "me@example.com",
            "aud": "sigstore", "exp": 4102444800u64,
        }));
        assert_eq!(certificate_identity(&unverified).unwrap(), "123");
        assert!(certificate_identity("garbage").is_err());
    }

    #[test]
    fn forge_projects_imply_a_policy() {
        let p = Policy::for_project("github.com/jdx/mise").unwrap();
        assert_eq!(p.issuer.as_deref(), Some(GITHUB_ISSUER));
        assert_eq!(
            p.identity_prefix.as_deref(),
            Some("https://github.com/jdx/mise/")
        );
        let sub = Policy::for_project("github.com/oxc-project/oxc/oxlint").unwrap();
        assert_eq!(
            sub.identity_prefix.as_deref(),
            Some("https://github.com/oxc-project/oxc/")
        );
        assert!(Policy::for_project("mise.jdx.dev").is_none());
        assert!(Policy::for_project("github.com/jdx").is_none());
        assert!(Policy::for_project("gitlab.com/group/proj").is_some());
        assert_eq!(
            p.describe(),
            "identity under https://github.com/jdx/mise/, issuer https://token.actions.githubusercontent.com"
        );
    }

    #[test]
    fn embedded_trusted_root_parses() {
        trusted_root(None).unwrap();
    }

    #[test]
    fn key_signed_bundles_round_trip_unlogged() {
        let root = trusted_root(None).unwrap();
        let key = SecretKey::from_seed([7u8; 32]);
        let statement = br#"{"_type":"https://in-toto.io/Statement/v1","subject":[{"name":"a","digest":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}],"predicateType":"x","predicate":{}}"#;
        let bundle = sign(
            Signer::Key {
                key: key.clone(),
                log: false,
            },
            statement,
        )
        .unwrap();
        assert!(bundle.contains("\"publicKey\""), "{bundle}");
        assert_eq!(peek_statement(&bundle).unwrap(), statement);

        let public = key.public_key();
        let err = verify(&bundle, &Trust::Key(&public), true, &root).unwrap_err();
        assert!(matches!(err, Error::Unlogged), "{err}");
        let ok = verify(&bundle, &Trust::Key(&public), false, &root).unwrap();
        assert_eq!(ok.statement, statement);
        assert_eq!(
            ok.signed_by,
            SignedBy::Key {
                key_id: key_id_hex(&public.key_id)
            }
        );
        assert_eq!(ok.integrated_time, None);

        let other = SecretKey::from_seed([8u8; 32]).public_key();
        let err = verify(&bundle, &Trust::Key(&other), false, &root).unwrap_err();
        assert!(matches!(err, Error::BadKeySignature(_)), "{err}");
        let err = verify(&bundle, &Trust::Identity(&Policy::default()), false, &root).unwrap_err();
        assert!(matches!(err, Error::IdentityPinnedForKey), "{err}");

        let tampered = bundle.replace("\"payload\":\"", "\"payload\":\"e");
        assert!(verify(&tampered, &Trust::Key(&public), false, &root).is_err());
        assert!(matches!(
            verify("nope", &Trust::Key(&public), false, &root).unwrap_err(),
            Error::BundleJson(_)
        ));
    }

    #[test]
    fn spki_and_hint_are_stable() {
        let key = SecretKey::from_seed([1u8; 32]).public_key();
        let der = spki_der(&key);
        assert_eq!(der.len(), 44);
        assert_eq!(&der[12..], key.key.as_bytes());
        assert_eq!(key_hint(&key).len(), 44);
        assert!(spki_pem(&key).starts_with("-----BEGIN PUBLIC KEY-----\n"));
    }
}
