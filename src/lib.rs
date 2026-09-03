//! packslip: a vendor publishes one signed, machine-readable document per
//! release that says what the artifacts are and how to verify them.
//! Consumers pin one identity and get checksums, platform mapping,
//! executables, provenance links, and a signed manifest without per-vendor
//! logic or a registry entry.
//!
//! The document is a sigstore bundle carrying an in-toto statement. It is
//! signed keylessly with a CI job's OIDC identity or with a long-lived
//! Ed25519 key, and either way logged to Rekor ([`sigstore`]). The same
//! shape carries a project's release list. This crate holds the schema,
//! the verifier, and the generator. See <https://packslip.dev/release/v1/>.

#![forbid(unsafe_code)]

pub mod cli;
pub mod create;
pub mod dsse;
pub mod minisign;
pub mod model;
pub mod sigstore;
pub mod verify;

pub use model::{
    Artifact, Identity, Predicate, ReleaseList, ReleaseListStatement, ReleaseRef, Scheme, Source,
    Statement, Subject,
};
pub use sigstore::{Policy, Signer, Trust};
pub use verify::{Options, Verified, VerifiedList, verify, verify_release_list};

/// The sha256 of a file, lowercase hex, and its size.
pub fn digest_file(path: &std::path::Path) -> std::io::Result<(String, u64)> {
    use sha2::Digest as _;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let size = std::io::copy(&mut file, &mut hasher)?;
    Ok((format!("{:x}", hasher.finalize()), size))
}
