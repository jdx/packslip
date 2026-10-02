//! The conformance vectors under `tests/conformance/` against the
//! reference implementation.
//!
//! The vectors are the specification's rules in executable form, meant to
//! be read and run by any implementation, not only this one. They cover
//! the parts of packslip that are packslip's own: which artifact a host
//! installs, which resource entries apply to it, which version a tag
//! names, whether a statement is structurally valid, and whether a
//! verified release is the forge project the consumer pinned. Signature
//! verification is sigstore's and is not restated here.
//!
//! These run with `--no-default-features` too: everything they touch is
//! in the crate's always-compiled core.

use std::path::Path;

use packslip::forge::{self, Continuity, Expected, ForgePin, IdentityError};
use packslip::model::{
    Artifact, Host, Selection, Statement, select_artifact, select_resources, tag_version,
};
use packslip::sigstore::{GITHUB_ISSUER, GITLAB_ISSUER, SourceRepository};

fn vectors(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/conformance")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn cases(name: &str) -> Vec<serde_json::Value> {
    vectors(name)["cases"]
        .as_array()
        .expect("cases is an array")
        .clone()
}

/// The case's name, for a failure message that says which vector broke.
fn case_name(case: &serde_json::Value) -> &str {
    case["name"].as_str().expect("every case is named")
}

#[test]
fn artifact_selection() {
    for case in cases("artifact-selection.json") {
        let name = case_name(&case);
        let artifacts: Vec<Artifact> = serde_json::from_value(case["artifacts"].clone())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let host = Host {
            os: case["host"]["os"].as_str().expect("host.os"),
            arch: case["host"]["arch"].as_str().expect("host.arch"),
            libc: case["host"]["libc"].as_str(),
        };
        let variant = case["variant"].as_str();
        let formats: Vec<&str> = case["formats"]
            .as_array()
            .expect("formats")
            .iter()
            .map(|f| f.as_str().expect("a format is a string"))
            .collect();

        let got = select_artifact(&artifacts, &host, variant, &formats);
        match (&case["expect"]["artifact"], &case["expect"]["error"]) {
            (serde_json::Value::String(want), _) => {
                let chosen = got.unwrap_or_else(|e| panic!("{name}: expected {want}, got {e}"));
                assert_eq!(&chosen.name, want, "{name}");
            }
            (_, serde_json::Value::String(want)) => match (want.as_str(), got) {
                ("no-match", Err(Selection::NoMatch)) => {}
                ("ambiguous", Err(Selection::Ambiguous(..))) => {}
                (want, got) => panic!("{name}: expected error {want}, got {got:?}"),
            },
            _ => panic!("{name}: expect names neither an artifact nor an error"),
        }
    }
}

#[test]
fn resource_selection() {
    for case in cases("resource-selection.json") {
        let name = case_name(&case);
        let statement: Statement = serde_json::from_value(case["statement"].clone())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        statement
            .validate()
            .unwrap_or_else(|e| panic!("{name}: the vector's own statement is invalid: {e}"));

        let wanted = case["artifact"].as_str().expect("artifact");
        let artifact = statement
            .predicate
            .artifacts
            .iter()
            .find(|a| a.name == wanted)
            .unwrap_or_else(|| panic!("{name}: no artifact named {wanted}"));

        let selected = select_resources(&statement, artifact);
        let got: Vec<usize> = selected
            .iter()
            .map(|r| {
                statement
                    .predicate
                    .resources
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, *r))
                    .expect("a selected resource comes from the statement")
            })
            .collect();
        let want: Vec<usize> = serde_json::from_value(case["expect"].clone())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn tag_versions() {
    for case in cases("tag-versions.json") {
        let name = case_name(&case);
        let tag = case["tag"].as_str().expect("tag");
        let project = case["project"].as_str().expect("project");
        let want = case["expect"].as_str();
        assert_eq!(tag_version(tag, project).as_deref(), want, "{name}");
    }
}

#[test]
fn statement_validity() {
    for case in cases("statement-validity.json") {
        let name = case_name(&case);
        let expect = case["expect"].as_str().expect("expect");
        // A document a consumer cannot even parse into the schema is one
        // it refuses, so a deserialization failure is a rejection.
        let outcome = serde_json::from_value::<Statement>(case["statement"].clone())
            .map_err(|e| e.to_string())
            .and_then(|s| s.validate().map_err(|e| e.to_string()));
        match (expect, outcome) {
            ("accept", Ok(())) => {}
            ("reject", Err(_)) => {}
            ("accept", Err(e)) => panic!("{name}: expected accept, was rejected: {e}"),
            ("reject", Ok(())) => panic!("{name}: expected reject, was accepted"),
            (other, _) => panic!("{name}: expect must be accept or reject, got {other:?}"),
        }
    }
}

#[test]
fn forge_identity() {
    for case in cases("forge-identity.json") {
        let name = case_name(&case);
        let text = |value: &serde_json::Value, key: &str| -> String {
            value[key]
                .as_str()
                .unwrap_or_else(|| panic!("{name}: {key} is a string"))
                .to_string()
        };
        let pin = ForgePin::of(
            text(&case["pin"], "project"),
            text(&case["pin"], "repository_id"),
        );
        let requested = text(&case, "requested");
        let release = &case["release"];
        let project = text(release, "project");

        let certificate = &release["certificate"];
        let mut source = SourceRepository::new(text(certificate, "uri"));
        if let Some(id) = certificate["id"].as_str() {
            source = source.with_id(id);
        }
        if let Some(owner_uri) = certificate["owner_uri"].as_str() {
            source = source.with_owner(owner_uri, text(certificate, "owner_id"));
        }

        let issuer = if project.starts_with("gitlab.com/") {
            GITLAB_ISSUER
        } else {
            GITHUB_ISSUER
        };
        let identity = format!("{}@refs/tags/v1.0.0", text(release, "signer"));
        let got = forge::check(
            &Expected::new(&requested).pinned(Some(&pin)),
            &project,
            &identity,
            Some(issuer),
            Some(&source),
        );

        let expect = &case["expect"];
        let outcome = text(expect, "outcome");
        match (outcome.as_str(), &got) {
            ("same", Ok(check)) => assert_eq!(check.continuity, Continuity::Same, "{name}"),
            ("renamed", Ok(check)) => assert_eq!(
                check.continuity,
                Continuity::Renamed {
                    requested: requested.clone(),
                    signed: project.clone(),
                },
                "{name}"
            ),
            ("refuse", Err(err)) => {
                let reason = match err {
                    IdentityError::DifferentRepository { .. } => "different-repository",
                    IdentityError::ProjectMismatch { .. } => "project-mismatch",
                    other => {
                        panic!("{name}: refused for a reason the vectors do not name: {other}")
                    }
                };
                assert_eq!(reason, text(expect, "reason"), "{name}");
            }
            (want, got) => panic!("{name}: expected {want}, got {got:?}"),
        }

        if let (Some(previous), Some(want), Ok(check)) = (
            case["previous_signer"].as_str(),
            expect["signer_continues"].as_bool(),
            &got,
        ) {
            assert_eq!(check.continues_signer(previous), want, "{name}: signer");
        }
    }
}
