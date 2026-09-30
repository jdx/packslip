use packslip::dsse::{Envelope, EnvelopeSignature, IN_TOTO_PAYLOAD_TYPE};
use packslip::minisign::{SecretKey, key_id_hex};

fn independently_verify(envelope: &Envelope, key: &packslip::minisign::PublicKey) -> Vec<u8> {
    let wire = serde_json::to_vec(envelope).unwrap();
    let envelope = dsse::Envelope::from_json(&wire).unwrap();
    let verifier = dsse::Ed25519Verifier::from_bytes(key.key.as_bytes())
        .unwrap()
        .with_key_id(key_id_hex(&key.key_id));
    dsse::verify(&envelope, &[&verifier], 1).unwrap().payload
}

#[test]
fn a_bad_entry_cannot_hide_a_valid_signature() {
    let key = SecretKey::from_seed([5; 32]);
    let public = key.public_key();
    let payload = br#"{"subject":[]}"#;
    let mut envelope = Envelope::sign(IN_TOTO_PAYLOAD_TYPE, payload, &key);
    envelope.signatures.insert(
        0,
        EnvelopeSignature {
            keyid: key_id_hex(&public.key_id),
            sig: "!!!!".into(),
        },
    );

    assert_eq!(independently_verify(&envelope, &public), payload);
    assert_eq!(envelope.verify(&public).unwrap(), payload);
}

#[test]
fn a_key_id_hint_cannot_hide_a_valid_signature() {
    let key = SecretKey::from_seed([5; 32]);
    let public = key.public_key();
    let payload = br#"{"subject":[]}"#;
    let mut envelope = Envelope::sign(IN_TOTO_PAYLOAD_TYPE, payload, &key);
    envelope.signatures[0].keyid = "wrong-key".into();

    assert_eq!(independently_verify(&envelope, &public), payload);
    assert_eq!(envelope.verify(&public).unwrap(), payload);

    envelope.payload_type = "text/plain".into();
    assert!(envelope.verify(&public).is_err());
}
