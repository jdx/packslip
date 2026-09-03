//! minisign-compatible Ed25519 signatures, so a vendor can sign a packslip
//! with the `minisign` they already use and a consumer can verify it with
//! `minisign -V` as well as with this crate.
//!
//! Formats, from minisign's documentation:
//! - public key file: an `untrusted comment:` line, then base64 of
//!   `Ed` + key id (8 bytes) + public key (32 bytes);
//! - signature file: an `untrusted comment:` line, base64 of the signature
//!   algorithm (`ED` prehashed with BLAKE2b-512, or legacy `Ed`) + key id +
//!   signature (64 bytes), a `trusted comment:` line, then base64 of the
//!   global signature over signature + trusted comment.
//!
//! Secret keys here are a raw 32-byte seed in hex, not minisign's
//! password-protected format; signing with a real `minisign -S` works the
//! same for verification.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use blake2::{Blake2b512, Digest as _};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};

/// A minisign public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub key_id: [u8; 8],
    pub key: VerifyingKey,
}

/// A secret key: the Ed25519 seed.
#[derive(Clone)]
pub struct SecretKey {
    key: SigningKey,
    key_id: [u8; 8],
}

/// A detached signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sig {
    pub prehashed: bool,
    pub key_id: [u8; 8],
    pub signature: Signature,
    pub trusted_comment: String,
    pub global_signature: Signature,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("malformed {what}: {why}")]
    Malformed { what: &'static str, why: String },
    #[error("signature is by key {sig}, not the expected {expected}")]
    KeyIdMismatch { sig: String, expected: String },
    #[error("signature does not verify")]
    BadSignature,
    #[error("global signature over the trusted comment does not verify")]
    BadGlobalSignature,
}

fn malformed(what: &'static str, why: impl std::fmt::Display) -> Error {
    Error::Malformed {
        what,
        why: why.to_string(),
    }
}

/// The key id as minisign prints it: uppercase hex of the little-endian
/// 8 bytes read as a number.
pub fn key_id_hex(id: &[u8; 8]) -> String {
    format!("{:016X}", u64::from_le_bytes(*id))
}

fn base64_line(text: &str, which: usize, what: &'static str) -> Result<Vec<u8>, Error> {
    let line = text
        .lines()
        .nth(which)
        .ok_or_else(|| malformed(what, format!("missing line {}", which + 1)))?;
    BASE64
        .decode(line.trim())
        .map_err(|e| malformed(what, format!("line {}: {e}", which + 1)))
}

impl PublicKey {
    /// Parse a public key file's text.
    pub fn parse(text: &str) -> Result<PublicKey, Error> {
        let bytes = if text.lines().count() >= 2 {
            base64_line(text, 1, "public key")?
        } else {
            BASE64
                .decode(text.trim())
                .map_err(|e| malformed("public key", e))?
        };
        if bytes.len() != 42 || &bytes[..2] != b"Ed" {
            return Err(malformed(
                "public key",
                "expected 42 bytes starting with Ed",
            ));
        }
        let mut key_id = [0u8; 8];
        key_id.copy_from_slice(&bytes[2..10]);
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[10..42]);
        let key = VerifyingKey::from_bytes(&key).map_err(|e| malformed("public key", e))?;
        Ok(PublicKey { key_id, key })
    }

    /// The public key file's text.
    pub fn to_file(&self) -> String {
        let mut bytes = Vec::with_capacity(42);
        bytes.extend_from_slice(b"Ed");
        bytes.extend_from_slice(&self.key_id);
        bytes.extend_from_slice(self.key.as_bytes());
        format!(
            "untrusted comment: minisign public key {}\n{}\n",
            key_id_hex(&self.key_id),
            BASE64.encode(bytes)
        )
    }

    /// Verify `sig` over `message`.
    pub fn verify(&self, message: &[u8], sig: &Sig) -> Result<(), Error> {
        if sig.key_id != self.key_id {
            return Err(Error::KeyIdMismatch {
                sig: key_id_hex(&sig.key_id),
                expected: key_id_hex(&self.key_id),
            });
        }
        let signed: Vec<u8> = if sig.prehashed {
            Blake2b512::digest(message).to_vec()
        } else {
            message.to_vec()
        };
        self.key
            .verify(&signed, &sig.signature)
            .map_err(|_| Error::BadSignature)?;
        let mut global = Vec::with_capacity(64 + sig.trusted_comment.len());
        global.extend_from_slice(&sig.signature.to_bytes());
        global.extend_from_slice(sig.trusted_comment.as_bytes());
        self.key
            .verify(&global, &sig.global_signature)
            .map_err(|_| Error::BadGlobalSignature)?;
        Ok(())
    }
}

impl SecretKey {
    /// A fresh random key.
    pub fn generate() -> SecretKey {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("the OS provides randomness");
        SecretKey::from_seed(seed)
    }

    /// A key from its 32-byte seed. The key id is derived from the public
    /// key so it is stable.
    pub fn from_seed(seed: [u8; 32]) -> SecretKey {
        let key = SigningKey::from_bytes(&seed);
        let digest = Blake2b512::digest(key.verifying_key().as_bytes());
        let mut key_id = [0u8; 8];
        key_id.copy_from_slice(&digest[..8]);
        SecretKey { key, key_id }
    }

    /// Parse the hex seed this crate writes.
    pub fn parse(text: &str) -> Result<SecretKey, Error> {
        let hex = text.trim();
        if hex.len() != 64 || !hex.is_ascii() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(malformed("secret key", "expected 64 hex characters"));
        }
        let mut seed = [0u8; 32];
        for (i, byte) in seed.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| malformed("secret key", e))?;
        }
        Ok(SecretKey::from_seed(seed))
    }

    /// The hex seed.
    pub fn to_file(&self) -> String {
        let mut out = String::with_capacity(65);
        for byte in self.key.to_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
        out.push('\n');
        out
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            key_id: self.key_id,
            key: self.key.verifying_key(),
        }
    }

    /// The underlying Ed25519 key, for raw signatures such as DSSE.
    pub fn signing_key(&self) -> &SigningKey {
        &self.key
    }

    /// Sign `message` the prehashed way (`ED`), which is what current
    /// minisign produces.
    pub fn sign(&self, message: &[u8], trusted_comment: &str) -> Sig {
        let digest = Blake2b512::digest(message);
        let signature = self.key.sign(&digest);
        let mut global = Vec::with_capacity(64 + trusted_comment.len());
        global.extend_from_slice(&signature.to_bytes());
        global.extend_from_slice(trusted_comment.as_bytes());
        let global_signature = self.key.sign(&global);
        Sig {
            prehashed: true,
            key_id: self.key_id,
            signature,
            trusted_comment: trusted_comment.to_string(),
            global_signature,
        }
    }
}

impl Sig {
    /// Parse a `.minisig` file's text.
    pub fn parse(text: &str) -> Result<Sig, Error> {
        let bytes = base64_line(text, 1, "signature")?;
        if bytes.len() != 74 {
            return Err(malformed("signature", "expected 74 bytes"));
        }
        let prehashed = match &bytes[..2] {
            b"ED" => true,
            b"Ed" => false,
            other => {
                return Err(malformed(
                    "signature",
                    format!("unknown algorithm {:?}", String::from_utf8_lossy(other)),
                ));
            }
        };
        let mut key_id = [0u8; 8];
        key_id.copy_from_slice(&bytes[2..10]);
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&bytes[10..74]);
        let trusted_line = text
            .lines()
            .nth(2)
            .ok_or_else(|| malformed("signature", "missing trusted comment line"))?;
        let trusted_comment = trusted_line
            .strip_prefix("trusted comment: ")
            .ok_or_else(|| malformed("signature", "line 3 is not a trusted comment"))?
            .to_string();
        let global = base64_line(text, 3, "signature")?;
        let global: [u8; 64] = global
            .try_into()
            .map_err(|_| malformed("signature", "global signature must be 64 bytes"))?;
        Ok(Sig {
            prehashed,
            key_id,
            signature: Signature::from_bytes(&sig),
            trusted_comment,
            global_signature: Signature::from_bytes(&global),
        })
    }

    /// The `.minisig` file's text.
    pub fn to_file(&self) -> String {
        let mut bytes = Vec::with_capacity(74);
        bytes.extend_from_slice(if self.prehashed { b"ED" } else { b"Ed" });
        bytes.extend_from_slice(&self.key_id);
        bytes.extend_from_slice(&self.signature.to_bytes());
        format!(
            "untrusted comment: signature from packslip key {}\n{}\ntrusted comment: {}\n{}\n",
            key_id_hex(&self.key_id),
            BASE64.encode(bytes),
            self.trusted_comment,
            BASE64.encode(self.global_signature.to_bytes())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_round_trip_through_files() {
        let secret = SecretKey::from_seed([7u8; 32]);
        let public = PublicKey::parse(&secret.public_key().to_file()).unwrap();
        assert_eq!(public, secret.public_key());
        let sig = secret.sign(b"hello", "timestamp:1756800000\tfile:packslip.json");
        let parsed = Sig::parse(&sig.to_file()).unwrap();
        assert_eq!(parsed, sig);
        public.verify(b"hello", &parsed).unwrap();
        assert_eq!(public.verify(b"hellp", &parsed), Err(Error::BadSignature));

        let mut tampered = parsed.clone();
        tampered.trusted_comment.push('!');
        assert_eq!(
            public.verify(b"hello", &tampered),
            Err(Error::BadGlobalSignature)
        );

        let other = SecretKey::from_seed([8u8; 32]).public_key();
        assert!(matches!(
            other.verify(b"hello", &parsed),
            Err(Error::KeyIdMismatch { .. })
        ));

        let reparsed = SecretKey::parse(&secret.to_file()).unwrap();
        assert_eq!(reparsed.public_key(), secret.public_key());
    }

    #[test]
    fn legacy_unhashed_signatures_verify() {
        let secret = SecretKey::from_seed([9u8; 32]);
        let signature = secret.key.sign(b"legacy");
        let mut global = signature.to_bytes().to_vec();
        global.extend_from_slice(b"tc");
        let sig = Sig {
            prehashed: false,
            key_id: secret.key_id,
            signature,
            trusted_comment: "tc".into(),
            global_signature: secret.key.sign(&global),
        };
        secret.public_key().verify(b"legacy", &sig).unwrap();
        assert!(sig.to_file().contains("\n"));
    }

    #[test]
    fn malformed_inputs() {
        assert!(PublicKey::parse("nope").is_err());
        assert!(PublicKey::parse("untrusted comment: x\nAAAA\n").is_err());
        assert!(Sig::parse("untrusted comment: x\nAAAA\n").is_err());
        assert!(SecretKey::parse("zz").is_err());
        let non_ascii_on_an_odd_boundary = format!("{}éa", "a".repeat(61));
        assert_eq!(non_ascii_on_an_odd_boundary.len(), 64);
        assert!(SecretKey::parse(&non_ascii_on_an_odd_boundary).is_err());
        let mut bytes = b"ED".to_vec();
        bytes.extend([0u8; 72]);
        assert!(matches!(
            Sig::parse(&format!("c\n{}\nnot trusted\nAA\n", BASE64.encode(bytes))),
            Err(Error::Malformed { .. })
        ));
    }

    /// Interoperability with the real minisign binary when present.
    #[test]
    fn interop_with_minisign_binary() {
        let Ok(minisign) = which::which("minisign") else {
            eprintln!("skipping: no minisign binary");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let secret = SecretKey::from_seed([3u8; 32]);
        let pubkey = dir.path().join("key.pub");
        std::fs::write(&pubkey, secret.public_key().to_file()).unwrap();
        let file = dir.path().join("packslip.json");
        std::fs::write(&file, b"{\"x\":1}").unwrap();
        let sig = secret.sign(b"{\"x\":1}", "trusted");
        std::fs::write(dir.path().join("packslip.json.minisig"), sig.to_file()).unwrap();
        let status = std::process::Command::new(minisign)
            .args(["-V", "-p"])
            .arg(&pubkey)
            .arg("-m")
            .arg(&file)
            .status()
            .unwrap();
        assert!(status.success(), "minisign -V should accept our signature");
    }
}
