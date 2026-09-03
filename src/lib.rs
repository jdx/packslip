//! packslip: a vendor publishes one signed, machine-readable document per
//! release that says what the artifacts are and how to verify them.
//! Consumers pin one identity and get checksums, platform mapping,
//! executables, provenance links, and an evidence level without per-vendor
//! logic.
//!
//! The document is an in-toto statement. It is signed either keylessly
//! through sigstore with a CI job's OIDC identity ([`sigstore`]), or with a
//! long-lived minisign key ([`minisign`]). This crate holds the schema, the
//! verifier, and the generator. See <https://packslip.dev/release/v1/>.

#![forbid(unsafe_code)]

pub mod cli;
pub mod create;
pub mod dsse;
pub mod minisign;
pub mod model;
pub mod sigstore;
pub mod verify;

pub use model::{Artifact, Identity, Level, Predicate, Scheme, Source, Statement, Subject};
pub use verify::{Signature, Trust, Verified, verify, verify_minisign};

/// The sha256 of a file, lowercase hex, and its size.
pub fn digest_file(path: &std::path::Path) -> std::io::Result<(String, u64)> {
    use sha2::Digest as _;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let size = std::io::copy(&mut file, &mut hasher)?;
    Ok((format!("{:x}", hasher.finalize()), size))
}
