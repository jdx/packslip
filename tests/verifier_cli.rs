//! The distribution verifier works without any publishing features.

use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_packslip"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn commands_follow_enabled_features() {
    let output = run(&["usage"]);
    assert!(output.status.success());
    let spec = String::from_utf8(output.stdout).unwrap();
    for command in ["verify", "pin", "show", "completion", "version"] {
        assert!(spec.contains(&format!("cmd {command}")), "{spec}");
    }
    for (command, enabled) in [
        (
            "create",
            cfg!(all(feature = "create", feature = "manifest")),
        ),
        (
            "releases",
            cfg!(all(feature = "create", feature = "manifest")),
        ),
        ("keygen", cfg!(feature = "sign")),
        ("schema", cfg!(feature = "schema")),
    ] {
        assert_eq!(spec.contains(&format!("cmd {command}")), enabled, "{spec}");
    }
}

#[test]
fn verifier_checks_real_historical_bundle_and_repository_pin() {
    let bundle = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/hk-v2.3.0.sigstore.json"
    );
    let output = run(&[
        "verify",
        bundle,
        "--pin",
        "ps1_snirenkjwr7m5ozgcufameodnm",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["project"], "github.com/jdx/hk");
    assert_eq!(report["version"], "2.3.0");
    let output = run(&["verify", bundle, "--pin", "ps1_aaaaaaaaaaaaaaaaaaaaaaaaaa"]);
    assert!(!output.status.success());
}

#[cfg(not(feature = "create"))]
#[test]
fn verifier_refuses_publishing_commands() {
    for command in ["create", "releases", "keygen"] {
        let output = run(&[command]);
        assert!(!output.status.success(), "{command} unexpectedly available");
    }
}
