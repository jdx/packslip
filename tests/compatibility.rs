//! A released verifier and the current verifier read the same v1 documents.
//! CI supplies the baseline built from the pinned v1.4.0 source commit.
use packslip::minisign::SecretKey;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};

fn verifiers() -> Vec<PathBuf> {
    let mut bins = vec![PathBuf::from(env!("CARGO_BIN_EXE_packslip"))];
    if let Some(baseline) = std::env::var_os("PACKSLIP_BASELINE_BIN") {
        let baseline = PathBuf::from(baseline);
        assert!(baseline.is_absolute() && baseline.is_file());
        bins.push(baseline);
    }
    bins
}

fn signed(value: &Value, key: &SecretKey) -> String {
    let envelope = packslip::dsse::Envelope::sign(
        packslip::dsse::IN_TOTO_PAYLOAD_TYPE,
        &serde_json::to_vec(value).unwrap(),
        key,
    );
    json!({"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json",
        "verificationMaterial":{"publicKey":{"hint":packslip::sigstore::key_hint(&key.public_key())}},
        "dsseEnvelope":envelope}).to_string()
}

#[test]
fn historical_logged_release_remains_verifiable() {
    for binary in verifiers() {
        let output = Command::new(&binary)
            .args([
                "verify",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/hk-v2.3.0.sigstore.json"
                ),
                "--issuer",
                "https://token.actions.githubusercontent.com",
                "--identity",
                "https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0",
                "--json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["project"], "github.com/jdx/hk");
        assert_eq!(report["version"], "2.3.0");
    }
}

#[test]
fn newer_v1_documents_with_optional_fields_keep_their_signed_meaning() {
    let dir = tempfile::tempdir().unwrap();
    let artifact = dir.path().join("tool");
    std::fs::write(&artifact, b"complete artifact\n").unwrap();
    let (digest, size) = packslip::digest_file(&artifact).unwrap();
    let key = SecretKey::from_seed([81; 32]);
    let public = dir.path().join("release.pub");
    std::fs::write(&public, key.public_key().to_file()).unwrap();
    let identity = json!({"scheme":"sigstore-key","key_id":packslip::minisign::key_id_hex(&key.public_key().key_id)});
    let release = json!({"_type":"https://in-toto.io/Statement/v1", "predicateType":"https://packslip.dev/release/v1",
        "future_optional":{"revision":2},
        "subject":[{"name":"tool","digest":{"sha256":digest}}],
        "predicate":{"project":"compatibility.example","version":"1.2.3","published_at":"2020-01-01T00:00:00Z",
            "identity":identity, "future_optional":true, "extensions":{"compatibility.example":{"new":true}},
            "artifacts":[{"name":"tool","size":size,"format":"raw","bin":[{"name":"tool","path":"tool"}],"future_optional":"ignored"}],
            "resources":[{"kind":"future-resource","archive":"share/future"}]}});
    let bundle = dir.path().join("release.sigstore.json");
    std::fs::write(&bundle, signed(&release, &key)).unwrap();
    let list = json!({"_type":"https://in-toto.io/Statement/v1", "predicateType":"https://packslip.dev/releases/v1",
        "subject":[{"name":"https://compatibility.example/release.sigstore.json","digest":{"sha256":packslip::digest_file(&bundle).unwrap().0}}],
        "predicate":{"project":"compatibility.example","identity":identity,
            "generated_at":"2026-10-03T00:00:00Z","expires_at":"2099-01-01T00:00:00Z","sequence":1,
            "future_optional":true,"releases":[{"version":"1.2.3","published_at":"2020-01-01T00:00:00Z",
                "packslip":"https://compatibility.example/release.sigstore.json","future_optional":true}]}});
    let list_path = dir.path().join("list.sigstore.json");
    std::fs::write(&list_path, signed(&list, &key)).unwrap();
    for binary in verifiers() {
        let verify = |path: &std::path::Path, check_artifact: bool| {
            let mut command = Command::new(&binary);
            command
                .arg("verify")
                .arg(path)
                .arg("--pubkey")
                .arg(&public)
                .arg("--allow-unlogged")
                .arg("--json");
            if check_artifact {
                command.arg("--artifact").arg(&artifact);
            }
            command.output().unwrap()
        };
        for (path, check_artifact) in [(&bundle, true), (&list_path, false)] {
            let output = verify(path, check_artifact);
            assert!(
                output.status.success(),
                "{}: {}",
                binary.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::fs::write(&artifact, b"tampered\n").unwrap();
        assert!(!verify(&bundle, true).status.success());
        std::fs::write(&artifact, b"complete artifact\n").unwrap();
        let mut v2 = release.clone();
        v2["predicateType"] = json!("https://packslip.dev/release/v2");
        let future = dir.path().join("future.sigstore.json");
        std::fs::write(&future, signed(&v2, &key)).unwrap();
        assert!(!verify(&future, false).status.success());
    }
}
