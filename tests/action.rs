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
