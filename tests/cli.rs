//! The packslip binary end to end with a key: keygen, create, verify,
//! show, releases, schema. Keyless signing needs a CI identity and Rekor,
//! so the CI workflow covers it.

use std::process::Command;

fn packslip(cwd: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_packslip"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn keygen_create_verify_show_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let (code, out, err) = packslip(d, &["keygen", "-o", "release.key"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("wrote release.key and release.pub"), "{out}");
    let (code, _, err) = packslip(d, &["keygen", "-o", "release.key"]);
    assert_ne!(code, 0, "never overwrite a key");
    assert!(err.contains("not overwriting"), "{err}");
    std::fs::write(d.join("occupied.pub"), "keep").unwrap();
    let (code, _, _err) = packslip(d, &["keygen", "-o", "occupied.key"]);
    assert_ne!(code, 0);
    assert_eq!(
        std::fs::read_to_string(d.join("occupied.pub")).unwrap(),
        "keep"
    );
    assert!(!d.join("occupied.key").exists());
    let (code, _, err) = packslip(d, &["keygen", "-o", "same.pub"]);
    assert_ne!(code, 0);
    assert!(err.contains("both resolve"), "{err}");

    std::fs::write(d.join("tool-v1.2.3-linux-x64.tar.xz"), b"linux").unwrap();
    std::fs::write(d.join("tool-v1.2.3-darwin-arm64.tar.xz"), b"mac").unwrap();
    std::fs::write(d.join("weird.bin"), b"?").unwrap();
    let (code, out, err) = packslip(
        d,
        &[
            "create",
            "--project",
            "tool.example.com",
            "--version",
            "1.2.3",
            "--key",
            "release.key",
            "--no-log",
            "--out",
            "dist",
            "--url-base",
            "https://dl.example.com/tool/1.2.3",
            "--source-repo",
            "https://github.com/example/tool",
            "--tag",
            "v1.2.3",
            "--published-at",
            "2026-09-01T00:00:00Z",
            "--bin",
            "tool",
            "tool-v1.2.3-linux-x64.tar.xz",
            "tool-v1.2.3-darwin-arm64.tar.xz",
            "weird.bin:freebsd/riscv64",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("3 artifact(s), signed by "), "{out}");
    assert!(out.contains(", unlogged)"), "{out}");
    assert!(d.join("dist/packslip.sigstore.json").exists());
    assert!(!d.join("dist/packslip.json").exists(), "one file");

    let (code, out, err) = packslip(d, &["show", "dist/packslip.sigstore.json"]);
    assert_eq!(code, 0, "{err}");
    let doc: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(doc["predicateType"], "https://packslip.dev/release/v1");
    assert_eq!(doc["predicate"]["artifacts"][0]["os"], "linux");
    assert_eq!(doc["predicate"]["artifacts"][0]["arch"], "x86_64");
    assert_eq!(doc["predicate"]["artifacts"][0]["bin"][0], "tool");
    assert_eq!(
        doc["predicate"]["artifacts"][0]["url"],
        "https://dl.example.com/tool/1.2.3/tool-v1.2.3-linux-x64.tar.xz"
    );
    assert_eq!(doc["predicate"]["artifacts"][2]["os"], "freebsd");
    assert_eq!(doc["predicate"]["artifacts"][2]["arch"], "riscv64");
    assert_eq!(doc["predicate"]["identity"]["scheme"], "sigstore-key");
    assert_eq!(doc["predicate"]["source"]["tag"], "v1.2.3");
    let (code, raw, _) = packslip(d, &["show", "--raw", "dist/packslip.sigstore.json"]);
    assert_eq!(code, 0);
    assert!(
        raw.starts_with("{\"_type\":\"https://in-toto.io/Statement/v1\""),
        "{raw}"
    );

    // Unlogged bundles need explicit consent.
    let (code, _, err) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.sigstore.json",
            "--pubkey",
            "release.pub",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("no transparency log entry"), "{err}");

    let (code, out, err) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.sigstore.json",
            "--pubkey",
            "release.pub",
            "--allow-unlogged",
            "--artifact",
            "tool-v1.2.3-linux-x64.tar.xz",
            "--artifact",
            "weird.bin",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.starts_with("ok: tool.example.com 1.2.3 published 2026-09-01T00:00:00Z signed by "),
        "{out}"
    );
    assert!(out.contains("(sigstore-key) unlogged"), "{out}");
    assert!(out.contains("(2 of 3 artifact(s) checked)"), "{out}");

    let (_, out, _) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.sigstore.json",
            "--pubkey",
            "release.pub",
            "--allow-unlogged",
            "--json",
        ],
    );
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["scheme"], "sigstore-key");
    assert_eq!(json["provenance_linked"], false);
    assert_eq!(json["artifact_count"], 3);

    // Without a pin, a non-forge project has nothing to verify against.
    let (code, _, err) = packslip(
        d,
        &["verify", "dist/packslip.sigstore.json", "--allow-unlogged"],
    );
    assert_ne!(code, 0);
    assert!(err.contains("no identity to verify against"), "{err}");

    // A modified artifact and a wrong key fail.
    std::fs::write(d.join("tool-v1.2.3-linux-x64.tar.xz"), b"linux!").unwrap();
    let (code, _, err) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.sigstore.json",
            "--pubkey",
            "release.pub",
            "--allow-unlogged",
            "--artifact",
            "tool-v1.2.3-linux-x64.tar.xz",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("sha256 is"), "{err}");
    packslip(d, &["keygen", "-o", "other.key"]);
    let (code, _, err) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.sigstore.json",
            "--pubkey",
            "other.pub",
            "--allow-unlogged",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("does not verify with the pinned key"), "{err}");

    // A release list over the bundle, verified with the same key.
    let (code, out, err) = packslip(
        d,
        &[
            "releases",
            "--project",
            "tool.example.com",
            "--sequence",
            "4",
            "--valid-for",
            "2w",
            "--generated-at",
            "2026-09-02T00:00:00Z",
            "--release",
            "https://dl.example.com/tool/1.2.3/packslip.sigstore.json=dist/packslip.sigstore.json",
            "--key",
            "release.key",
            "--no-log",
            "--out",
            "site/.well-known/packslip.json",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("1 release(s), sequence 4, expires 2026-09-16T00:00:00Z"),
        "{out}"
    );
    let (code, out, err) = packslip(
        d,
        &[
            "verify",
            "site/.well-known/packslip.json",
            "--pubkey",
            "release.pub",
            "--allow-unlogged",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.starts_with(
            "ok: release list for tool.example.com sequence 4 expires 2026-09-16T00:00:00Z signed by "
        ),
        "{out}"
    );
    let (code, _, err) = packslip(
        d,
        &[
            "releases",
            "--project",
            "other.example.com",
            "--sequence",
            "1",
            "--release",
            "https://x/packslip.sigstore.json=dist/packslip.sigstore.json",
            "--key",
            "release.key",
            "--no-log",
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("the list is for"), "{err}");
}

#[test]
fn source_revision_requires_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let (code, _, err) = packslip(d, &["keygen", "-o", "release.key"]);
    assert_eq!(code, 0, "{err}");
    std::fs::write(d.join("tool.tar.xz"), b"tool").unwrap();
    let (code, _, err) = packslip(
        d,
        &[
            "create",
            "--project",
            "tool.example.com",
            "--version",
            "1",
            "--key",
            "release.key",
            "--no-log",
            "--commit",
            "abc123",
            "tool.tar.xz",
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("require --source-repo"), "{err}");
    let (code, _, err) = packslip(
        d,
        &[
            "create",
            "--project",
            "tool.example.com",
            "--version",
            "1",
            "--no-log",
            "tool.tar.xz",
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("--no-log is for --key"), "{err}");
}

#[test]
fn schemas_are_json() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, _) = packslip(dir.path(), &["schema"]);
    assert_eq!(code, 0);
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(schema["properties"]["predicate"].is_object(), "{out}");
    let (code, out, _) = packslip(dir.path(), &["schema", "--releases"]);
    assert_eq!(code, 0);
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        out.contains("sequence") && schema["properties"]["predicate"].is_object(),
        "{out}"
    );
}
