//! The signer fingerprint: a short, pasteable name for the repository a
//! keyless project is signed from, such as
//! `ps1_snirenkjwr7m5ozgcufameodnm`.
//!
//! A README or a Dockerfile can carry the fingerprint where a consumer
//! reads it, so the consumer does not have to trust the first release it
//! sees (see [`crate::forge`]). It is the first 128 bits of the SHA-256 of
//! the OIDC issuer and the forge's repository ID, and nothing else: not the
//! owner, the workflow, the ref, or a monorepo tool's subpath. A rename or a
//! transfer leaves it unchanged, and a repository that took over a freed
//! name has a different one. [`Fingerprint::verify`] checks a pin against
//! the certificate of a release that has already verified.

use std::fmt;
use std::str::FromStr;

use sha2::{Digest as _, Sha256};

use crate::sigstore::SourceRepository;

/// What every fingerprint starts with. The `1` is the version of how the
/// rest is derived.
pub const PREFIX: &str = "ps1_";

/// Domain separation, so the hash cannot be confused with another use of
/// the same two strings.
const DOMAIN: &[u8] = b"packslip-signer-v1\0";

/// RFC 4648 base32 with lowercase letters.
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// The characters after the prefix: 128 bits take 26 base32 characters, the
/// last of which carries 3 bits and 2 bits of padding that must be zero.
const ENCODED_LEN: usize = 26;

/// The repository a keyless signer's certificate comes from, as 128 bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 16]);

/// Why text is not a fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("a signer fingerprint starts with {PREFIX}")]
    Prefix,
    #[error("a signer fingerprint has {ENCODED_LEN} characters after {PREFIX}, not {actual}")]
    Length { actual: usize },
    #[error("{character:?} is not a lowercase base32 character (a-z, 2-7)")]
    Character { character: char },
    #[error("the last character of a signer fingerprint has bits set past its 128th")]
    TrailingBits,
}

/// Why a certificate does not match a fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Mismatch {
    /// The certificate has no issuer or no repository ID to derive one from.
    #[error(
        "the certificate records no repository ID, so it cannot be matched to a signer fingerprint"
    )]
    NoRepositoryId,
    /// A different repository, or a different issuer.
    #[error("the release is signed by {actual}, but the pin is {expected}")]
    Different {
        expected: Fingerprint,
        actual: Fingerprint,
    },
}

impl Fingerprint {
    /// The fingerprint of a repository ID under an issuer: the issuer's URL
    /// and the repository ID exactly as the certificate records them. None
    /// when there is nothing to fingerprint: an empty issuer, or a
    /// repository ID that is not a decimal string.
    pub fn of(issuer: &str, repository_id: &str) -> Option<Fingerprint> {
        if issuer.is_empty()
            || issuer.contains('\0')
            || repository_id.is_empty()
            || !repository_id.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut hash = Sha256::new();
        hash.update(DOMAIN);
        hash.update(issuer.as_bytes());
        hash.update([0]);
        hash.update(repository_id.as_bytes());
        let digest = hash.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Some(Fingerprint(bytes))
    }

    /// The fingerprint of the signer a certificate records: its OIDC issuer
    /// and its Source Repository Identifier. None when the project is not
    /// signed keylessly, or the certificate carries no repository ID, as
    /// one from before Fulcio recorded them.
    pub fn of_signer(
        issuer: Option<&str>,
        source: Option<&SourceRepository>,
    ) -> Option<Fingerprint> {
        Fingerprint::of(issuer?, source?.id.as_deref()?)
    }

    /// Check this pin against the certificate of a release, given the
    /// certificate's issuer and source repository, as
    /// [`crate::verify::Verified::issuer`] and
    /// [`crate::sigstore::source_repository`] give them.
    ///
    /// Read both only from a bundle that verified: the pin says which
    /// repository to expect, and the certificate is what proves it. This
    /// does not check that the signer is a workflow of the repository the
    /// statement names; [`crate::forge::check`] does.
    pub fn verify(
        &self,
        issuer: Option<&str>,
        source: Option<&SourceRepository>,
    ) -> Result<(), Mismatch> {
        let actual = Fingerprint::of_signer(issuer, source).ok_or(Mismatch::NoRepositoryId)?;
        if actual == *self {
            Ok(())
        } else {
            Err(Mismatch::Different {
                expected: *self,
                actual,
            })
        }
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::with_capacity(PREFIX.len() + ENCODED_LEN);
        out.push_str(PREFIX);
        let mut buffer = 0u16;
        let mut bits = 0;
        for byte in self.0 {
            buffer = buffer << 8 | u16::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(char::from(ALPHABET[usize::from(buffer >> bits & 31)]));
            }
        }
        if bits > 0 {
            out.push(char::from(ALPHABET[usize::from(buffer << (5 - bits) & 31)]));
        }
        f.write_str(&out)
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

impl FromStr for Fingerprint {
    type Err = ParseError;

    /// The canonical spelling only: the prefix, then 26 lowercase base32
    /// characters. Text with other letters, padding, or a different length
    /// is not a fingerprint, so each fingerprint has one spelling.
    fn from_str(text: &str) -> Result<Fingerprint, ParseError> {
        let encoded = text.strip_prefix(PREFIX).ok_or(ParseError::Prefix)?;
        let actual = encoded.chars().count();
        if actual != ENCODED_LEN {
            return Err(ParseError::Length { actual });
        }
        let mut bytes = [0u8; 16];
        let mut buffer = 0u16;
        let mut bits = 0;
        let mut at = 0;
        for character in encoded.chars() {
            let value = ALPHABET
                .iter()
                .position(|&b| char::from(b) == character)
                .ok_or(ParseError::Character { character })?;
            buffer = buffer << 5 | value as u16;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                bytes[at] = (buffer >> bits) as u8;
                at += 1;
            }
        }
        // 26 characters hold 130 bits: the two left over must be zero.
        if buffer & ((1 << bits) - 1) != 0 {
            return Err(ParseError::TrailingBits);
        }
        Ok(Fingerprint(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigstore::{GITHUB_ISSUER, GITLAB_ISSUER};

    // Each of these was computed independently of this module with
    // `hashlib.sha256` and `base64.b32encode`.
    const HK: &str = "ps1_snirenkjwr7m5ozgcufameodnm";

    #[test]
    fn the_fingerprint_of_a_repository_id_is_the_documented_one() {
        for (issuer, id, want) in [
            (GITHUB_ISSUER, "922514152", HK),
            (GITHUB_ISSUER, "1", "ps1_kwhjac5qpc45qetfh6ppwisi6a"),
            (GITLAB_ISSUER, "922514152", "ps1_5ay4zeayy432fq5mkpu6qzenxy"),
            (GITLAB_ISSUER, "278964", "ps1_5q3uiurqwpvl5pdvb4bo4ochre"),
        ] {
            let got = Fingerprint::of(issuer, id).unwrap();
            assert_eq!(got.to_string(), want, "{issuer} {id}");
            assert_eq!(want.parse::<Fingerprint>().unwrap(), got);
        }
    }

    #[test]
    fn the_issuer_is_part_of_the_fingerprint() {
        assert_ne!(
            Fingerprint::of(GITHUB_ISSUER, "922514152"),
            Fingerprint::of(GITLAB_ISSUER, "922514152"),
        );
    }

    #[test]
    fn nothing_is_fingerprinted_without_an_issuer_and_a_decimal_id() {
        assert_eq!(Fingerprint::of("", "1"), None);
        assert_eq!(Fingerprint::of(GITHUB_ISSUER, ""), None);
        assert_eq!(Fingerprint::of(GITHUB_ISSUER, "jdx/hk"), None);
        assert_eq!(Fingerprint::of(GITHUB_ISSUER, "-1"), None);
        assert_eq!(Fingerprint::of(GITHUB_ISSUER, "1\0"), None);
        assert_eq!(Fingerprint::of("https://a\0b", "1"), None);
    }

    #[test]
    fn a_fingerprint_is_prefix_and_26_characters() {
        let text = Fingerprint::of(GITHUB_ISSUER, "7").unwrap().to_string();
        assert_eq!(text.len(), PREFIX.len() + ENCODED_LEN);
        assert!(text.starts_with("ps1_"));
    }

    #[test]
    fn text_that_is_not_the_canonical_spelling_is_refused() {
        let parse = |text: &str| text.parse::<Fingerprint>();
        assert_eq!(parse("snirenkjwr7m5ozgcufameodnm"), Err(ParseError::Prefix));
        assert_eq!(
            parse("ps2_snirenkjwr7m5ozgcufameodnm"),
            Err(ParseError::Prefix)
        );
        assert_eq!(
            parse("PS1_snirenkjwr7m5ozgcufameodnm"),
            Err(ParseError::Prefix)
        );
        assert_eq!(parse("ps1_"), Err(ParseError::Length { actual: 0 }));
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodn"),
            Err(ParseError::Length { actual: 25 })
        );
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodnma"),
            Err(ParseError::Length { actual: 27 })
        );
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodnm="),
            Err(ParseError::Length { actual: 27 })
        );
        assert_eq!(
            parse("ps1_SNIRENKJWR7M5OZGCUFAMEODNM"),
            Err(ParseError::Character { character: 'S' })
        );
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodn1"),
            Err(ParseError::Character { character: '1' })
        );
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodn\u{e9}"),
            Err(ParseError::Character {
                character: '\u{e9}'
            })
        );
        // The 26th character holds three bits of the hash and two of
        // padding; "n" sets one of the padding bits where "m" does not.
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodnn"),
            Err(ParseError::TrailingBits)
        );
        assert_eq!(
            parse(" ps1_snirenkjwr7m5ozgcufameodnm"),
            Err(ParseError::Prefix)
        );
        assert_eq!(
            parse("ps1_snirenkjwr7m5ozgcufameodnm\n"),
            Err(ParseError::Length { actual: 27 })
        );
    }

    #[test]
    fn every_trailing_character_but_one_in_four_is_refused() {
        let stem = &HK[..HK.len() - 1];
        let accepted: Vec<char> = ALPHABET
            .iter()
            .map(|&b| char::from(b))
            .filter(|c| format!("{stem}{c}").parse::<Fingerprint>().is_ok())
            .collect();
        assert_eq!(accepted, ['a', 'e', 'i', 'm', 'q', 'u', 'y', '4']);
    }

    #[test]
    fn every_fingerprint_round_trips() {
        for id in 1..200u32 {
            let pin = Fingerprint::of(GITHUB_ISSUER, &id.to_string()).unwrap();
            assert_eq!(pin.to_string().parse::<Fingerprint>(), Ok(pin));
        }
    }

    fn certificate(uri: &str, id: Option<&str>) -> SourceRepository {
        let source = SourceRepository::new(uri).with_owner("https://github.com/jdx", "216188");
        match id {
            Some(id) => source.with_id(id),
            None => source,
        }
    }

    #[test]
    fn a_pin_matches_the_certificate_of_its_repository() {
        let pin: Fingerprint = HK.parse().unwrap();
        let source = certificate("https://github.com/jdx/hk", Some("922514152"));
        assert_eq!(pin.verify(Some(GITHUB_ISSUER), Some(&source)), Ok(()));
    }

    #[test]
    fn a_rename_or_transfer_keeps_the_pin() {
        let pin: Fingerprint = HK.parse().unwrap();
        // The name and the owner are not part of it.
        let moved = SourceRepository::new("https://github.com/someone-else/hook")
            .with_id("922514152")
            .with_owner("https://github.com/someone-else", "1");
        assert_eq!(pin.verify(Some(GITHUB_ISSUER), Some(&moved)), Ok(()));
    }

    #[test]
    fn a_recreated_name_does_not_match() {
        let pin: Fingerprint = HK.parse().unwrap();
        let recreated = certificate("https://github.com/jdx/hk", Some("5"));
        let Err(Mismatch::Different { expected, actual }) =
            pin.verify(Some(GITHUB_ISSUER), Some(&recreated))
        else {
            panic!("a different repository ID must not match")
        };
        assert_eq!(expected, pin);
        assert_eq!(actual, Fingerprint::of(GITHUB_ISSUER, "5").unwrap());
    }

    #[test]
    fn another_issuer_does_not_match() {
        let pin: Fingerprint = HK.parse().unwrap();
        let source = certificate("https://gitlab.com/jdx/hk", Some("922514152"));
        assert!(matches!(
            pin.verify(Some(GITLAB_ISSUER), Some(&source)),
            Err(Mismatch::Different { .. })
        ));
    }

    #[test]
    fn a_certificate_without_an_id_matches_no_pin() {
        let pin: Fingerprint = HK.parse().unwrap();
        let old = certificate("https://github.com/jdx/hk", None);
        assert_eq!(
            pin.verify(Some(GITHUB_ISSUER), Some(&old)),
            Err(Mismatch::NoRepositoryId)
        );
        assert_eq!(
            pin.verify(Some(GITHUB_ISSUER), None),
            Err(Mismatch::NoRepositoryId)
        );
        let source = certificate("https://github.com/jdx/hk", Some("922514152"));
        assert_eq!(
            pin.verify(None, Some(&source)),
            Err(Mismatch::NoRepositoryId)
        );
    }
}
