//! Keyless signing and verification through sigstore: the document travels
//! as an in-toto DSSE envelope inside a sigstore bundle, signed with an
//! ephemeral key certified by Fulcio for an OIDC identity and logged to
//! Rekor. This is the scheme a CI job uses; there is no key to keep.

use std::borrow::Cow;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sigstore_oidc::IdentityToken;
use sigstore_sign::SigningContext;
use sigstore_trust_root::{SIGSTORE_PRODUCTION_TRUSTED_ROOT, TrustedRoot};
use sigstore_types::{Artifact, Bundle, Sha256Hash, SignatureContent};
use sigstore_verify::VerificationPolicy;

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
    #[error("bundle is not valid JSON: {0}")]
    BundleJson(String),
    #[error("bundle does not carry a DSSE envelope")]
    NotDsse,
    #[error("bundle payload type is {0:?}, expected {IN_TOTO_PAYLOAD_TYPE}")]
    PayloadType(String),
    #[error("bundle payload is not a packslip statement: {0}")]
    Payload(String),
    #[error("trusted root: {0}")]
    TrustedRoot(String),
    #[error("bundle does not verify: {0}")]
    Verification(String),
    #[error("signed by {actual:?}, expected an identity starting with {expected:?}")]
    IdentityPrefix { actual: String, expected: String },
    #[error("no identity to verify against for {0:?}: pass --identity or --issuer")]
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

/// Sign `statement` (the canonical document bytes) into a sigstore bundle,
/// returned as JSON.
pub fn sign(identity: OidcIdentity, statement: &[u8]) -> Result<String, Error> {
    let rt = runtime()?;
    let context = SigningContext::production();
    let signer = context.signer(identity.token);
    let bundle = rt
        .block_on(signer.sign_raw_statement(statement))
        .map_err(|e| Error::Sign(e.to_string()))?;
    bundle.to_json().map_err(|e| Error::Sign(e.to_string()))
}

/// Who a consumer accepts as the signer.
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
        let (host, path) = project.split_once('/')?;
        if path.is_empty() || path.split('/').count() < 2 {
            return None;
        }
        match host {
            "github.com" => Some(Policy {
                issuer: Some(GITHUB_ISSUER.into()),
                identity: None,
                identity_prefix: Some(format!("https://github.com/{path}/")),
            }),
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

/// What verifying a bundle established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleVerified {
    /// The signed statement bytes, exactly as they were signed.
    pub statement: Vec<u8>,
    pub identity: String,
    pub issuer: String,
    /// When Rekor integrated the entry, as a Unix timestamp.
    pub integrated_time: Option<i64>,
}

/// The trusted root: a file's contents, or the production root embedded in
/// this build.
pub fn trusted_root(json: Option<&str>) -> Result<TrustedRoot, Error> {
    TrustedRoot::from_json(json.unwrap_or(SIGSTORE_PRODUCTION_TRUSTED_ROOT))
        .map_err(|e| Error::TrustedRoot(e.to_string()))
}

/// Verify a bundle: the certificate chains to the trusted root and was
/// valid when Rekor logged the entry, the entry is in the log, the DSSE
/// signature checks out, and the signer satisfies `policy`. Returns the
/// signed statement bytes.
pub fn verify(
    bundle_json: &str,
    policy: &Policy,
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
    // packslip's own first subject is that artifact. The caller checks the
    // statement's contents and any real files afterwards.
    let parsed: crate::model::Statement =
        serde_json::from_slice(&statement).map_err(|e| Error::Payload(e.to_string()))?;
    let first = parsed
        .subject
        .first()
        .ok_or_else(|| Error::Payload("no subject".into()))?;
    let digest =
        Sha256Hash::from_hex(&first.digest.sha256).map_err(|e| Error::Payload(e.to_string()))?;
    let artifact = Artifact::Digest(Cow::Borrowed(digest.as_bytes()));

    let mut verification = VerificationPolicy::default();
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
        identity,
        issuer,
        integrated_time: result.integrated_time,
    })
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
        assert!(
            "https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v1"
                .starts_with(p.identity_prefix.as_deref().unwrap())
        );
        assert!(
            !"https://github.com/jdx/mise-evil/.github/workflows/release.yml@refs/tags/v1"
                .starts_with(p.identity_prefix.as_deref().unwrap())
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
    fn bundle_must_be_dsse_in_toto() {
        let root = trusted_root(None).unwrap();
        let err = verify("nope", &Policy::default(), &root).unwrap_err();
        assert!(matches!(err, Error::BundleJson(_)), "{err}");
    }
}
