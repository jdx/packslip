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

pub mod archive;
pub mod cli;
pub mod create;
pub mod dsse;
pub mod linkage;
pub mod manifest;
pub mod minisign;
pub mod model;
pub mod sigstore;
pub mod verify;

pub use manifest::Manifest;
pub use model::{
    Artifact, Attestor, Bin, Evidence, Host, Identity, Predicate, ReleaseList,
    ReleaseListStatement, ReleaseRef, ReleaseStatus, Requires, Resource, ResourceSource, Scheme,
    Selection, Source, Statement, Subject, select_artifact, select_resources, tag_version,
};
pub use sigstore::{Policy, Signer, Trust};
pub use verify::{Options, Verified, VerifiedList, verify, verify_release_list};

/// The sha256 of a file, lowercase hex, and its size.
pub fn digest_file(path: &std::path::Path) -> std::io::Result<(String, u64)> {
    let all = digest_file_all(path)?;
    Ok((all.sha256, all.size))
}

/// Every digest of a file, in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digests {
    /// Lowercase hex.
    pub sha256: String,
    /// Lowercase hex.
    pub sha512: String,
    pub size: u64,
}

/// The sha256 and sha512 of a file, lowercase hex, and its size.
pub fn digest_file_all(path: &std::path::Path) -> std::io::Result<Digests> {
    use sha2::Digest as _;
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)?;
    let mut sha256 = sha2::Sha256::new();
    let mut sha512 = sha2::Sha512::new();
    let mut size = 0u64;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha256.update(&buf[..n]);
        sha512.update(&buf[..n]);
        size += n as u64;
    }
    Ok(Digests {
        sha256: format!("{:x}", sha256.finalize()),
        sha512: format!("{:x}", sha512.finalize()),
        size,
    })
}
