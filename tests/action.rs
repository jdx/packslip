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
fn run_install(
    version_override: Option<&str>,
    caller_digest: Option<&str>,
    locked_digest: Option<&str>,
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
    let asset = format!("packslip-v1.5.1-linux-{arch}.tar.xz");
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
    fs::create_dir(&action_root).unwrap();
    fs::write(
        action_root.join("Cargo.toml"),
        "[package]\nversion = \"1.5.1\"\n",
    )
    .unwrap();
    if let Some(digest) = locked_digest {
        let action_dir = action_root.join("action");
        fs::create_dir(&action_dir).unwrap();
        let digest = if digest == "$ACTUAL" { &actual } else { digest };
        fs::write(
            action_dir.join("archives.sha256"),
            format!("{digest}  {asset}\n"),
        )
        .unwrap();
    }

    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        "#!/usr/bin/env bash\nset -eu\nif [ \"$1\" = release ]; then\n  while [ \"$#\" -gt 0 ]; do\n    if [ \"$1\" = -D ]; then\n      cp \"$FIXTURE\" \"$2/$ASSET\"\n      exit 0\n    fi\n    shift\n  done\nfi\nif [ \"$1\" = attestation ]; then exit 0; fi\nexit 88\n",
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
        .env("PACKSLIP_VERSION", version_override.unwrap_or(""))
        .env("PACKSLIP_SHA256", caller_digest.unwrap_or(""))
        .env("PACKSLIP_PATH", "")
        .env("FIXTURE", archive)
        .env("ASSET", asset)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .output()
        .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn install_uses_the_checked_out_action_lock_before_running_the_archive() {
    let accepted = run_install(None, None, Some("$ACTUAL"));
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("invoked"));
    let rejected = run_install(None, None, Some("not-a-digest"));
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("internal archive digest lock"));

    let missing = run_install(None, None, None);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no internal archive digest lock"));

    let disagreement = run_install(None, Some(&"0".repeat(64)), Some("$ACTUAL"));
    assert!(!disagreement.status.success());
    assert!(
        String::from_utf8_lossy(&disagreement.stderr)
            .contains("disagrees with the action's internal archive digest lock")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn install_rejects_a_mismatched_internal_lock_before_provenance_or_execution() {
    let rejected = run_install(None, None, Some(&"0".repeat(64)));
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("packslip archive SHA-256 mismatch")
    );
    assert!(!String::from_utf8_lossy(&rejected.stdout).contains("invoked"));
}

#[cfg(target_os = "linux")]
#[test]
fn explicit_version_override_keeps_compatibility_and_can_add_a_strict_digest() {
    let accepted = run_install(Some("1.5.1"), None, None);
    assert!(accepted.status.success());
    assert!(
        String::from_utf8_lossy(&accepted.stderr)
            .contains("verifying build provenance without an archive SHA-256")
    );

    let rejected = run_install(Some("1.5.1"), Some(&"0".repeat(64)), None);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("packslip archive SHA-256 mismatch")
    );
}
