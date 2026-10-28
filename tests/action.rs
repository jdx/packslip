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

#[test]
fn release_promotions_queue_without_replacing_pending_runs() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    // Workflows are deliberately excluded from the crates.io/distro source
    // package. Assert repository wiring where the workflow is available.
    if !path.exists() {
        return;
    }
    let workflow: Value = serde_yaml_bw::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(workflow["concurrency"]["queue"].as_str(), Some("max"));
    assert_eq!(
        workflow["concurrency"]["cancel-in-progress"].as_bool(),
        Some(false)
    );
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains("needs.release.outputs.current-")
    );
    for step in workflow["jobs"]["release"]["steps"].as_sequence().unwrap() {
        if matches!(
            step["name"].as_str(),
            Some("Rewrite the release body with Communiqué")
                | Some("Publish this release's packslip")
                | Some("Publish the release")
        ) {
            assert_eq!(
                step["if"].as_str(),
                Some("env.CANDIDATE_PUBLISHED != 'true'")
            );
        }
    }
    let release_plz_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release-plz.yml");
    let release_plz: Value =
        serde_yaml_bw::from_str(&fs::read_to_string(release_plz_path).unwrap()).unwrap();
    for name in ["release", "release-pr"] {
        assert!(
            release_plz["jobs"][name]["if"]
                .as_str()
                .unwrap()
                .contains("github.ref == 'refs/heads/main'")
        );
    }
    for step in release_plz["jobs"]["release"]["steps"]
        .as_sequence()
        .unwrap()
    {
        if step["uses"].as_str().is_some_and(|value| {
            value.starts_with("rust-lang/crates-io-auth-action@")
                || value.starts_with("release-plz/action@")
        }) {
            assert_eq!(
                step["if"].as_str(),
                Some("steps.gate.outputs.publish == 'true'")
            );
        }
    }
    for (file, jobs) in [
        ("distro-source.yml", vec!["ppa", "copr"]),
        ("package-repositories.yml", vec!["publish"]),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".github/workflows")
            .join(file);
        let guarded: Value = serde_yaml_bw::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(guarded["concurrency"]["queue"].as_str(), Some("max"));
        assert!(
            guarded["concurrency"]["group"]
                .as_str()
                .unwrap()
                .contains("packslip-release")
        );
        for job in jobs {
            let steps = guarded["jobs"][job]["steps"].as_sequence().unwrap();
            let guard = steps
                .iter()
                .position(|step| step["id"].as_str() == Some("freshness"))
                .unwrap();
            assert!(
                steps[guard]["run"]
                    .as_str()
                    .unwrap()
                    .contains("freshness --tag")
            );
            assert_eq!(
                steps[guard + 1]["if"].as_str(),
                Some("steps.freshness.outputs.current-latest == 'true'")
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn old_release_recovery_never_pushes_moving_image_aliases() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    if !path.exists() {
        return;
    }
    let workflow: Value = serde_yaml_bw::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let step = workflow["jobs"]["image"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["id"].as_str() == Some("push"))
        .unwrap();
    for (major, latest, expected) in [
        (false, false, vec!["ghcr.io/jdx/packslip:1.6.0"]),
        (
            true,
            false,
            vec![
                "ghcr.io/jdx/packslip:1.6.0",
                "ghcr.io/jdx/packslip:1.6",
                "ghcr.io/jdx/packslip:1",
            ],
        ),
        (
            true,
            true,
            vec![
                "ghcr.io/jdx/packslip:1.6.0",
                "ghcr.io/jdx/packslip:1.6",
                "ghcr.io/jdx/packslip:1",
                "ghcr.io/jdx/packslip:latest",
            ],
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("container")).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        let builder = root.join("container/build.sh");
        fs::write(
            &builder,
            "#!/usr/bin/env bash\nprintf '%s\\0' \"$@\" > \"$CALL_LOG\"\nprintf '{\"containerimage.digest\":\"sha256:fixture\"}' > meta.json\n",
        )
        .unwrap();
        fs::set_permissions(&builder, fs::Permissions::from_mode(0o755)).unwrap();
        let docker = root.join("bin/docker");
        fs::write(
            &docker,
            "#!/usr/bin/env bash\nif [ \"$1\" = login ]; then cat >/dev/null; fi\nif [ \"$1\" = run ]; then printf 'packslip 1.6.0\\n'; fi\n",
        )
        .unwrap();
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![root.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        // Model a fresh forge result, not cached needs outputs, at the exact
        // image write boundary. Deliberately supply stale true env values.
        let python = root.join("bin/python3");
        fs::write(
            &python,
            format!(
                "#!/usr/bin/env bash\nprintf 'CURRENT_MAJOR={}\\nCURRENT_LATEST={}\\n'\n",
                major, latest
            ),
        )
        .unwrap();
        fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
        let result = Command::new("bash")
            .args(["-eu", "-c", step["run"].as_str().unwrap()])
            .current_dir(root)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("GH_TOKEN", "credential-free-fixture")
            .env("GITHUB_REF_NAME", "v1.6.0")
            .env("GITHUB_REPOSITORY", "jdx/packslip")
            .env("GITHUB_ACTOR", "fixture")
            .env("GITHUB_OUTPUT", root.join("output"))
            .env("CURRENT_MAJOR", "true")
            .env("CURRENT_LATEST", "true")
            .env("CALL_LOG", root.join("arguments"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let arguments = fs::read(root.join("arguments")).unwrap();
        let args: Vec<&str> = arguments
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| std::str::from_utf8(part).unwrap())
            .collect();
        let tags: Vec<&str> = args
            .windows(2)
            .filter(|pair| pair[0] == "--tag")
            .map(|pair| pair[1])
            .collect();
        assert_eq!(tags, expected);
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

/// Run the releases action's bundle and self-check steps for `project` in
/// `repository`, with `inputs` set beyond the required ones and a mock CLI
/// whose `verify --help` names --allow-other-repository when
/// `knows_opt_out` is set. Return the arguments of each `packslip verify`
/// they ran, or the stderr of the step that failed.
fn run_releases_verify(
    project: &str,
    repository: &str,
    inputs: &[(&str, &str)],
    knows_opt_out: bool,
) -> Result<Vec<Vec<String>>, String> {
    run_releases_verify_under("v1.0.0", project, repository, inputs, knows_opt_out)
}

fn run_releases_verify_under(
    tag: &str,
    project: &str,
    repository: &str,
    inputs: &[(&str, &str)],
    knows_opt_out: bool,
) -> Result<Vec<Vec<String>>, String> {
    let action: Value = serde_yaml_bw::from_str(include_str!("../releases/action.yml")).unwrap();
    let steps = action["runs"]["steps"].as_sequence().unwrap();
    let step = |name: &str| {
        steps
            .iter()
            .find(|step| step["name"].as_str() == Some(name))
            .unwrap()
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("lists").join(tag)).unwrap();
    fs::write(
        root.join("lists").join(tag).join("packslip.sigstore.json"),
        "",
    )
    .unwrap();
    let cli = root.join("packslip");
    fs::write(
        &cli,
        r#"#!/usr/bin/env bash
set -eu
case "$1" in
  verify)
    if [ "${2:-}" = --help ]; then printf '%s\n' "$VERIFY_HELP"; exit 0; fi
    printf '%s\0' "$@" >> "$CALL_LOG"
    printf '\n' >> "$CALL_LOG"
    ;;
  show) printf '{"predicate":{"project":"%s"}}\n' "$STATEMENT_PROJECT" ;;
  *) exit 88 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
    let help = if knows_opt_out {
        "--pin <PIN>\n--allow-other-repository\n--allow-unlogged"
    } else {
        "--pin <PIN>\n--allow-unlogged"
    };

    for name in ["Verify the published bundles", "Verify what was written"] {
        let step = step(name);
        let mut command = Command::new("bash");
        command
            .args(["-eu", "-c", step["run"].as_str().unwrap()])
            .current_dir(root)
            .env_clear()
            .env("RUNNER_TEMP", root)
            .env("GITHUB_OUTPUT", root.join("outputs"))
            .env("CALL_LOG", root.join("calls"))
            .env("VERIFY_HELP", help)
            .env("STATEMENT_PROJECT", project);
        let mut paths = vec![root.to_path_buf()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        command.env("PATH", std::env::join_paths(paths).unwrap());
        for (name, expression) in step["env"].as_mapping().unwrap() {
            let expression = expression.as_str().unwrap();
            let value = if let Some(input) = expression
                .strip_prefix("${{ inputs.")
                .and_then(|s| s.strip_suffix(" }}"))
            {
                match input {
                    "project" => project.to_owned(),
                    "dir" => "lists".to_owned(),
                    "url-base" => inputs
                        .iter()
                        .find(|(name, _)| *name == input)
                        .map_or("https://lists.example.com", |(_, value)| *value)
                        .to_owned(),
                    _ => inputs
                        .iter()
                        .find(|(name, _)| *name == input)
                        .map(|(_, value)| *value)
                        .unwrap_or_else(|| {
                            action["inputs"][input]["default"].as_str().unwrap_or("")
                        })
                        .to_owned(),
                }
            } else {
                match expression {
                    "${{ github.repository }}" => repository.to_owned(),
                    "${{ github.server_url }}" => "https://github.com".to_owned(),
                    "${{ steps.build.outputs.list }}" => "list.sigstore.json".to_owned(),
                    other => panic!("unhandled action expression: {other}"),
                }
            };
            command.env(name.as_str().unwrap(), value);
        }
        let output = command.output().unwrap();
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
    }
    Ok(fs::read_to_string(root.join("calls"))
        .unwrap()
        .lines()
        .map(|call| {
            call.trim_end_matches('\0')
                .split('\0')
                .map(str::to_owned)
                .collect()
        })
        .collect())
}

#[test]
fn releases_action_passes_allow_other_repository_when_needed() {
    let issuer = "https://token.actions.githubusercontent.com";
    let pin = |prefix: &str, opt_out: bool| {
        let mut pin = vec![
            "--issuer".to_owned(),
            issuer.to_owned(),
            "--identity-prefix".to_owned(),
            prefix.to_owned(),
        ];
        if opt_out {
            pin.push("--allow-other-repository".to_owned());
        }
        pin
    };
    let calls = |pin: Vec<String>| {
        ["lists/v1.0.0/packslip.sigstore.json", "list.sigstore.json"]
            .map(|doc| [vec!["verify".to_owned(), doc.to_owned()], pin.clone()].concat())
            .to_vec()
    };
    for (project, repository, opt_out) in [
        // The project's own repository, or a tool in it: verify holds the
        // signer to that repository as well as to the pin.
        ("github.com/owner/tool", "owner/tool", false),
        ("github.com/owner/tool/cli", "owner/tool", false),
        // GitHub resolves the repository's name in any case.
        ("github.com/Owner/Tool", "owner/tool", false),
        // A project on its own domain names no repository.
        ("tool.example.com", "owner/tool", false),
        // A registry's or repackager's list for another repository's
        // project: the pin alone says who signed.
        ("github.com/vendor/tool", "registry/lists", true),
        ("github.com/vendor/tool/cli", "registry/lists", true),
        ("gitlab.com/group/sub/tool", "registry/lists", true),
    ] {
        assert_eq!(
            run_releases_verify(project, repository, &[], true).unwrap(),
            calls(pin(&format!("https://github.com/{repository}/"), opt_out)),
            "{project} in {repository}"
        );
    }

    // The project's own repository listing bundles another repository of
    // its owner signed opts out with the input.
    let owner_wide = [
        ("identity-prefix", "https://github.com/owner/"),
        ("allow-other-repository", "true"),
    ];
    assert_eq!(
        run_releases_verify("github.com/owner/tool", "owner/tool", &owner_wide, true).unwrap(),
        calls(pin("https://github.com/owner/", true))
    );
    assert_eq!(
        run_releases_verify(
            "github.com/owner/tool",
            "owner/tool",
            &owner_wide[..1],
            true
        )
        .unwrap(),
        calls(pin("https://github.com/owner/", false))
    );
    let err = run_releases_verify(
        "github.com/owner/tool",
        "owner/tool",
        &[("allow-other-repository", "yes")],
        true,
    )
    .unwrap_err();
    assert!(
        err.contains("allow-other-repository must be true or false, not \"yes\""),
        "{err}"
    );

    // A packslip from before the flag takes the pin as the whole policy
    // already, and would refuse a flag it does not know.
    for (repository, inputs) in [("registry/lists", &[][..]), ("owner/tool", &owner_wide[..])] {
        let calls =
            run_releases_verify("github.com/owner/tool", repository, inputs, false).unwrap();
        assert_eq!(calls.len(), 2);
        for call in calls {
            assert!(
                !call.contains(&"--allow-other-repository".to_owned()),
                "{call:?}"
            );
        }
    }
}

#[test]
fn releases_action_refuses_a_bundle_name_or_url_base_the_release_argument_would_split() {
    for input in [
        ("bundle", "foo=bar.json"),
        ("url-base", "https://example.com/?x=y"),
    ] {
        let err =
            run_releases_verify("github.com/owner/tool", "owner/tool", &[input], true).unwrap_err();
        assert!(err.contains("cannot contain an ="), "{input:?}: {err}");
    }
}

#[test]
fn releases_action_refuses_a_tag_the_release_argument_would_split() {
    let err = run_releases_verify_under("v1=x", "github.com/owner/tool", "owner/tool", &[], true)
        .unwrap_err();
    assert!(err.contains("a tag with an = in it"), "{err}");
}
