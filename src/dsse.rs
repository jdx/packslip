//! DSSE envelopes: the signing envelope in-toto statements travel in.
//! A signature covers the pre-authentication encoding (PAE) of the payload
//! type and payload, so the type cannot be swapped under a signature.
//!
//! Keys are the same Ed25519 keys as `minisign`, identified by the
//! minisign key id, signing the PAE bytes directly (not prehashed).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Signer as _, Verifier as _};
use serde::{Deserialize, Serialize};

use crate::minisign::{PublicKey, SecretKey, key_id_hex};

pub const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    /// base64 of the payload bytes.
    pub payload: String,
    pub signatures: Vec<EnvelopeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeSignature {
    pub keyid: String,
    /// base64 of the raw Ed25519 signature over the PAE.
    pub sig: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("envelope payload is not valid base64")]
    Payload,
    #[error("envelope has no signature by key {0}")]
    NoSignatureBy(String),
    #[error("envelope signature by {0} is malformed")]
    Malformed(String),
    #[error("envelope signature by {0} does not verify")]
    BadSignature(String),
}

/// `DSSEv1 <len(type)> <type> <len(payload)> <payload>`.
pub fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + payload_type.len() + 32);
    out.extend_from_slice(b"DSSEv1 ");
    out.extend_from_slice(payload_type.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload_type.as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload);
    out
}

impl Envelope {
    /// Wrap and sign `payload`.
    pub fn sign(payload_type: &str, payload: &[u8], key: &SecretKey) -> Envelope {
        let signature = key.signing_key().sign(&pae(payload_type, payload));
        Envelope {
            payload_type: payload_type.to_string(),
            payload: BASE64.encode(payload),
            signatures: vec![EnvelopeSignature {
                keyid: key_id_hex(&key.public_key().key_id),
                sig: BASE64.encode(signature.to_bytes()),
            }],
        }
    }

    /// The payload bytes.
    pub fn payload_bytes(&self) -> Result<Vec<u8>, Error> {
        BASE64.decode(&self.payload).map_err(|_| Error::Payload)
    }

    /// Verify the signature made by `key`, returning the payload.
    pub fn verify(&self, key: &PublicKey) -> Result<Vec<u8>, Error> {
        let keyid = key_id_hex(&key.key_id);
        let entry = self
            .signatures
            .iter()
            .find(|s| s.keyid.eq_ignore_ascii_case(&keyid))
            .ok_or_else(|| Error::NoSignatureBy(keyid.clone()))?;
        let sig = BASE64
            .decode(&entry.sig)
            .ok()
            .and_then(|b| <[u8; 64]>::try_from(b).ok())
            .map(|b| Signature::from_bytes(&b))
            .ok_or_else(|| Error::Malformed(keyid.clone()))?;
        let payload = self.payload_bytes()?;
        key.key
            .verify(&pae(&self.payload_type, &payload), &sig)
            .map_err(|_| Error::BadSignature(keyid))?;
        Ok(payload)
    }

    /// Verify with whichever of `keys` signed it.
    pub fn verify_any<'k>(
        &self,
        keys: impl IntoIterator<Item = &'k PublicKey>,
    ) -> Option<(Vec<u8>, &'k PublicKey)> {
        keys.into_iter()
            .find_map(|key| self.verify(key).ok().map(|payload| (payload, key)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pae_matches_the_spec_example() {
        assert_eq!(
            pae("http://example.com/HelloWorld", b"hello world"),
            b"DSSEv1 29 http://example.com/HelloWorld 11 hello world"
        );
    }

    #[test]
    fn sign_and_verify() {
        let key = SecretKey::from_seed([5u8; 32]);
        let env = Envelope::sign(IN_TOTO_PAYLOAD_TYPE, b"{\"x\":1}", &key);
        let json = serde_json::to_string(&env).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.verify(&key.public_key()).unwrap(), b"{\"x\":1}");
        let other = SecretKey::from_seed([6u8; 32]).public_key();
        assert!(matches!(
            parsed.verify(&other),
            Err(Error::NoSignatureBy(_))
        ));
        let mut swapped = parsed.clone();
        swapped.payload_type = "text/plain".into();
        assert!(matches!(
            swapped.verify(&key.public_key()),
            Err(Error::BadSignature(_))
        ));
        let mut bad = parsed.clone();
        bad.signatures[0].sig = "AAAA".into();
        assert!(matches!(
            bad.verify(&key.public_key()),
            Err(Error::Malformed(_))
        ));
        let keys = [other, key.public_key()];
        assert!(parsed.verify_any(keys.iter()).is_some());
    }
}
