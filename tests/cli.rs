//! The packslip binary end to end: keygen, create, verify, schema.

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
fn keygen_create_verify() {
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
    assert!(!d.join("same.pub").exists());

    std::fs::write(d.join("tool-v1.2.3-linux-x64.tar.xz"), b"linux").unwrap();
    std::fs::write(d.join("tool-v1.2.3-darwin-arm64.tar.xz"), b"mac").unwrap();
    std::fs::write(d.join("weird.bin"), b"?").unwrap();
    let (code, out, err) = packslip(
        d,
        &[
            "create",
            "--project",
            "github.com/example/tool",
            "--version",
            "1.2.3",
            "--key",
            "release.key",
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
    assert!(out.contains(", level L2)"), "{out}");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(d.join("dist/packslip.json")).unwrap())
            .unwrap();
    assert_eq!(doc["predicateType"], "https://packslip.dev/release/v1");
    assert_eq!(doc["predicate"]["artifacts"][0]["os"], "linux");
    assert_eq!(doc["predicate"]["artifacts"][0]["arch"], "x86_64");
    assert_eq!(
        doc["predicate"]["artifacts"][0]["url"],
        "https://dl.example.com/tool/1.2.3/tool-v1.2.3-linux-x64.tar.xz"
    );
    assert_eq!(doc["predicate"]["artifacts"][2]["os"], "freebsd");
    assert_eq!(doc["predicate"]["artifacts"][2]["arch"], "riscv64");
    assert_eq!(doc["predicate"]["identity"]["scheme"], "minisign");
    assert_eq!(doc["predicate"]["artifacts"][0]["bin"][0], "tool");
    assert_eq!(doc["predicate"]["artifacts"][1]["bin"][0], "tool");
    assert_eq!(doc["predicate"]["source"]["tag"], "v1.2.3");

    let (code, out, err) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.json",
            "--pubkey",
            "release.pub",
            "--artifact",
            "tool-v1.2.3-linux-x64.tar.xz",
            "--artifact",
            "weird.bin",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.starts_with(
            "ok: github.com/example/tool 1.2.3 published 2026-09-01T00:00:00Z signed by "
        ),
        "{out}"
    );
    assert!(
        out.contains("level L2 (2 of 3 artifact(s) checked)"),
        "{out}"
    );

    let (_, out, _) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.json",
            "--pubkey",
            "release.pub",
            "--json",
        ],
    );
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["level"], "l2");
    assert_eq!(json["artifact_count"], 3);

    // A modified artifact, a modified document, and a wrong key all fail.
    std::fs::write(d.join("tool-v1.2.3-linux-x64.tar.xz"), b"linux!").unwrap();
    let (code, _, err) = packslip(
        d,
        &[
            "verify",
            "dist/packslip.json",
            "--pubkey",
            "release.pub",
            "--artifact",
            "tool-v1.2.3-linux-x64.tar.xz",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("sha256 is"), "{err}");
    let text = std::fs::read_to_string(d.join("dist/packslip.json")).unwrap();
    std::fs::write(d.join("dist/packslip.json"), text.replace("1.2.3", "1.2.4")).unwrap();
    let (code, _, err) = packslip(
        d,
        &["verify", "dist/packslip.json", "--pubkey", "release.pub"],
    );
    assert_eq!(code, 1);
    assert!(err.contains("does not verify"), "{err}");
    packslip(d, &["keygen", "-o", "other.key"]);
    std::fs::write(d.join("dist/packslip.json"), text).unwrap();
    let (code, _, err) = packslip(
        d,
        &["verify", "dist/packslip.json", "--pubkey", "other.pub"],
    );
    assert_eq!(code, 1);
    assert!(err.contains("not the expected"), "{err}");
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
            "github.com/example/tool",
            "--version",
            "1",
            "--key",
            "release.key",
            "--commit",
            "abc123",
            "tool.tar.xz",
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("require --source-repo"), "{err}");
}

#[test]
fn schema_is_json() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, _) = packslip(dir.path(), &["schema"]);
    assert_eq!(code, 0);
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(schema["properties"]["predicate"].is_object(), "{out}");
}
