//! Go-produced release statements must remain accepted by the Rust verifier.

use std::path::Path;
use std::process::Command;

use base64::Engine as _;
use packslip::model::Statement;

#[test]
fn validates_a_statement_emitted_by_packslip_go() {
    if std::env::var_os("PACKSLIP_GO_CONFORMANCE").is_none() {
        return;
    }
    let output = Command::new("go")
        .args(["run", "./cmd/conformance"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("go"))
        .output()
        .expect("Go must be installed by the Go module CI job");
    assert!(
        output.status.success(),
        "Go conformance helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let statement: Statement =
        serde_json::from_slice(&output.stdout).expect("Go emitted statement JSON");
    statement.validate().expect("Rust accepts Go statement");
}

#[test]
fn rust_verifies_an_offline_sigstore_go_bundle_and_rejects_tampering() {
    if std::env::var_os("PACKSLIP_GO_CONFORMANCE").is_none() {
        return;
    }
    let output = Command::new("go")
        .args(["run", "./cmd/conformance", "-signed"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("go"))
        .output()
        .expect("Go must be installed by the Go module CI job");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let generated: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("release.sigstore.json");
    let public = dir.path().join("release.pub");
    std::fs::write(&bundle, generated["bundle"].as_str().unwrap()).unwrap();
    std::fs::write(&public, generated["public_key"].as_str().unwrap()).unwrap();
    let verify = |path: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_packslip"))
            .args([
                "verify",
                path.to_str().unwrap(),
                "--pubkey",
                public.to_str().unwrap(),
                "--allow-unlogged",
            ])
            .output()
            .unwrap()
    };
    let verified = verify(&bundle);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let mut value: serde_json::Value =
        serde_json::from_str(generated["bundle"].as_str().unwrap()).unwrap();
    let mut signature = base64::engine::general_purpose::STANDARD
        .decode(value["dsseEnvelope"]["signatures"][0]["sig"].as_str().unwrap())
        .unwrap();
    signature[0] ^= 0x01;
    value["dsseEnvelope"]["signatures"][0]["sig"] =
        serde_json::json!(base64::engine::general_purpose::STANDARD.encode(signature));
    let tampered = dir.path().join("tampered.sigstore.json");
    std::fs::write(&tampered, value.to_string()).unwrap();
    assert!(
        !verify(&tampered).status.success(),
        "tampered Go bundle verified"
    );
}
