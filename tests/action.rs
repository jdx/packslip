//! Execute the action's create step with a mock CLI, without signing services.
#![cfg(unix)]

use serde_yaml_bw::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn run_create(commit: Option<&str>) -> Vec<String> {
    let action: Value = serde_yaml_bw::from_str(include_str!("../action.yml")).unwrap();
    let create = action["runs"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["id"].as_str() == Some("create"))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let cli = root.join("packslip");
    fs::write(
        &cli,
        "#!/usr/bin/env bash\nprintf '%s\\0' \"$@\" > \"$CALL_LOG\"\nmkdir -p out\n: > out/packslip.sigstore.json\n",
    )
    .unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("packslip-files"), "dist/tool-linux-x64.tar.xz\n").unwrap();

    let mut command = Command::new("bash");
    command
        .args(["-eu", "-c", create["run"].as_str().unwrap()])
        .current_dir(root)
        .env_clear()
        .env("RUNNER_TEMP", root)
        .env("GITHUB_OUTPUT", root.join("outputs"))
        .env("CALL_LOG", root.join("args"));
    let mut paths = vec![root.to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    command.env("PATH", std::env::join_paths(paths).unwrap());

    // Read the real YAML input wiring and defaults. A missing or miswired
    // commit input must fail rather than being supplied directly by the test.
    for (name, expression) in create["env"].as_mapping().unwrap() {
        let expression = expression.as_str().unwrap();
        let value = if let Some(input) = expression
            .strip_prefix("${{ inputs.")
            .and_then(|s| s.strip_suffix(" }}"))
        {
            let supplied = match input {
                "commit" => commit,
                "tag" => Some("v1.2.3"),
                "out" => Some("out"),
                "attest" => Some("false"),
                _ => None,
            };
            supplied
                .unwrap_or_else(|| action["inputs"][input]["default"].as_str().unwrap_or(""))
                .to_owned()
        } else {
            match expression {
                "${{ github.repository }}" => "owner/tool".to_owned(),
                "${{ github.ref_name }}" => "main".to_owned(),
                "${{ github.ref_type }}" => "branch".to_owned(),
                "${{ github.sha }}" => "a".repeat(40),
                "${{ github.server_url }}" => "https://github.com".to_owned(),
                "${{ github.api_url }}" => "https://api.github.com".to_owned(),
                other => panic!("unhandled action expression: {other}"),
            }
        };
        command.env(name.as_str().unwrap(), value);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fs::read(root.join("args")).unwrap();
    assert_eq!(args.last(), Some(&0));
    let args: Vec<String> = args[..args.len() - 1]
        .split(|byte| *byte == 0)
        .map(|arg| String::from_utf8(arg.to_vec()).unwrap())
        .collect();
    assert_eq!(value_after(&args, "--tag"), "v1.2.3");
    assert_eq!(value_after(&args, "--version"), "1.2.3");
    args
}

fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
    &args[args.iter().position(|arg| arg == flag).unwrap() + 1]
}

#[test]
fn default_preserves_workflow_commit() {
    assert_eq!(value_after(&run_create(None), "--commit"), "a".repeat(40));
}

#[test]
fn empty_commit_preserves_workflow_commit() {
    assert_eq!(
        value_after(&run_create(Some("")), "--commit"),
        "a".repeat(40)
    );
}

#[test]
fn manual_release_can_override_workflow_commit() {
    let commit = "b".repeat(40);
    assert_eq!(value_after(&run_create(Some(&commit)), "--commit"), commit);
}

#[test]
fn override_is_one_literal_argument_not_shell_code() {
    // The real CLI validates commits; the action must pass this literally.
    let commit = "$(exit 77) two words";
    assert_eq!(value_after(&run_create(Some(commit)), "--commit"), commit);
}

#[test]
fn both_actions_wire_the_optional_download_digest_to_the_installer() {
    for source in [
        include_str!("../action.yml"),
        include_str!("../releases/action.yml"),
    ] {
        let action: Value = serde_yaml_bw::from_str(source).unwrap();
        assert_eq!(
            action["inputs"]["packslip-sha256"]["default"].as_str(),
            Some("")
        );
        let install = action["runs"]["steps"]
            .as_sequence()
            .unwrap()
            .iter()
            .find(|step| step["name"].as_str() == Some("Install packslip"))
            .unwrap();
        assert_eq!(
            install["env"]["PACKSLIP_SHA256"].as_str(),
            Some("${{ inputs.packslip-sha256 }}")
        );
    }
}

#[cfg(target_os = "linux")]
fn run_install(expected_digest: Option<&str>, version_override: &str) -> std::process::Output {
    run_install_fixture(expected_digest, version_override, None, true)
}

#[cfg(target_os = "linux")]
fn run_install_fixture(
    expected_digest: Option<&str>,
    version_override: &str,
    lock_digest: Option<&str>,
    write_lock: bool,
) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let fixture = root.join("packslip-fixture");
    fs::create_dir(&fixture).unwrap();
    let executable = fixture.join("packslip");
    fs::write(&executable, "#!/usr/bin/env bash\nprintf invoked\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => panic!("unsupported test architecture: {other}"),
    };
    let version = if version_override.is_empty() {
        env!("CARGO_PKG_VERSION")
    } else {
        version_override
    };
    let asset = format!("packslip-v{version}-linux-{arch}.tar.xz");
    let archive = root.join(&asset);
    assert!(
        Command::new("tar")
            .args([
                "-cJf",
                archive.to_str().unwrap(),
                "-C",
                root.to_str().unwrap(),
                "packslip-fixture",
            ])
            .status()
            .unwrap()
            .success()
    );
    let actual = String::from_utf8(
        Command::new("sha256sum")
            .arg(&archive)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .split_whitespace()
    .next()
    .unwrap()
    .to_owned();

    let action_root = root.join("action-root");
    fs::create_dir_all(action_root.join("action")).unwrap();
    fs::write(
        action_root.join("Cargo.toml"),
        format!("[package]\nversion = \"{version}\"\n"),
    )
    .unwrap();
    if write_lock {
        let metadata = serde_json::json!({
            "schema": 1, "version": version, "source_commit": "a".repeat(40),
            "assets": {asset.clone(): {"sha256": lock_digest.unwrap_or(&actual)}}
        });
        fs::write(
            action_root.join("action/release.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
    }

    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        "#!/usr/bin/env bash\nset -eu\nif [ \"$1\" = release ]; then\n  while [ \"$#\" -gt 0 ]; do\n    if [ \"$1\" = -D ]; then\n      cp \"$FIXTURE\" \"$2/$ASSET\"\n      exit 0\n    fi\n    shift\n  done\nfi\nif [ \"$1\" = attestation ]; then printf 'attested %s\\n' \"$*\"; exit 0; fi\nexit 88\n",
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();

    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    Command::new("bash")
        .args(["scripts/install-packslip.sh"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUNNER_TEMP", root)
        .env("GITHUB_PATH", root.join("github-path"))
        .env("PACKSLIP_ACTION_ROOT", action_root)
        .env("PACKSLIP_VERSION", version_override)
        .env("PACKSLIP_SHA256", expected_digest.unwrap_or(&actual))
        .env("PACKSLIP_PATH", "")
        .env("FIXTURE", archive)
        .env("ASSET", asset)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .output()
        .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn install_checks_the_pinned_archive_digest_before_running_it() {
    let accepted = run_install(None, "1.5.1");
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("invoked"));
    let rejected = run_install(Some("not-a-digest"), "1.5.1");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("packslip-sha256 must be"));
}

#[cfg(target_os = "linux")]
#[test]
fn install_rejects_a_tampered_pinned_archive_before_provenance_or_execution() {
    let rejected = run_install(Some(&"0".repeat(64)), "1.5.1");
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("packslip archive SHA-256 mismatch")
    );
    assert!(!String::from_utf8_lossy(&rejected.stdout).contains("invoked"));
    assert!(!String::from_utf8_lossy(&rejected.stdout).contains("attested"));
}

#[cfg(target_os = "linux")]
#[test]
fn default_uses_internal_digest_and_actual_candidate_provenance() {
    let accepted = run_install(Some(""), "");
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("invoked"));
    let pinned = run_install(None, "");
    assert!(pinned.status.success());
    let output = String::from_utf8_lossy(&accepted.stdout);
    assert!(output.contains("--source-digest aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(output.contains("--source-ref refs/heads/release-plz"));
    assert!(output.contains("--signer-digest aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(output.contains("--signer-workflow jdx/packslip/.github/workflows/release.yml"));
    assert!(output.contains("--deny-self-hosted-runners"));
}

#[cfg(target_os = "linux")]
#[test]
fn default_rejects_missing_lock_and_tampered_bytes_before_attesting() {
    let missing = run_install_fixture(Some(""), "", None, false);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no release digest map"));
    let tampered = run_install_fixture(Some(""), "", Some(&"0".repeat(64)), true);
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("SHA-256 mismatch"));
    assert!(!String::from_utf8_lossy(&tampered.stdout).contains("attested"));
    assert!(!String::from_utf8_lossy(&tampered.stdout).contains("invoked"));
}
