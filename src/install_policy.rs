//! Conjunctive bootstrap policy and explicit, context-bound trust transitions.
//! Signature verification precedes every use of document policy. Filesystem
//! replacement options have no place in this API.
use crate::{
    forge::ForgePin,
    model::{Attestor, Scheme},
    sigstore::{Policy, Trust},
};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bootstrap signature: {0}")]
    Verify(#[from] crate::verify::Error),
    #[error("bootstrap forge identity: {0}")]
    Forge(#[from] crate::verify::ForgeError),
    #[error("bootstrap bundle: {0}")]
    Sigstore(#[from] crate::sigstore::Error),
    #[error("bootstrap document: {0}")]
    Json(#[from] serde_json::Error),
    #[error("bootstrap pin files: {0}")]
    Io(#[from] std::io::Error),
    #[error("bootstrap pin file TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    Constraint(String),
    #[error(
        "trust change {id}: {reasons:?}; review this proposal and repeat with --accept-trust-change={id}"
    )]
    Change { id: String, reasons: Vec<String> },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    #[serde(default)]
    pub pins: Vec<String>,
    pub pubkey: Option<String>,
    pub issuer: Option<String>,
    pub identity: Option<String>,
    pub identity_prefix: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinFile {
    projects: BTreeMap<String, Constraints>,
}

/// Every matching file is an independent requirement. Pins within one file
/// are alternatives; pins in different files and caller pins must all agree.
pub fn admin_constraints(dirs: &[&Path], project: &str) -> Result<Vec<Constraints>, Error> {
    let mut constraints = Vec::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        let mut paths: Vec<_> = entries
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()?;
        paths.sort();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        {
            let file: PinFile = toml::from_str(&std::fs::read_to_string(path)?)?;
            if let Some(c) = file.projects.get(project) {
                constraints.push(c.clone());
            }
        }
    }
    Ok(constraints)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub project: String,
    pub scheme: Scheme,
    pub signer: String,
    pub issuer: Option<String>,
    pub repository: Option<ForgePin>,
    pub pin_workflow: bool,
    pub attested_by: Attestor,
    // Match build scopes across versions rather than versioned archive names.
    pub provenance_scopes: Vec<String>,
}
fn scopes(statement: &crate::Statement) -> Vec<String> {
    let mut scopes: Vec<_> = statement
        .predicate
        .artifacts
        .iter()
        .filter(|a| !a.provenance.is_empty())
        .map(|a| {
            format!(
                "{}/{}/{}/{}",
                a.os.as_deref().unwrap_or("any"),
                a.arch.as_deref().unwrap_or("any"),
                a.libc.as_deref().unwrap_or("any"),
                a.variant.as_deref().unwrap_or("default")
            )
        })
        .collect();
    scopes.sort();
    scopes.dedup();
    scopes
}
fn changes(previous: &Record, next: &Record) -> Vec<String> {
    let mut changes = Vec::new();
    if previous.scheme == Scheme::SigstoreOidc && next.scheme == Scheme::SigstoreKey {
        changes.push("OIDC identity replaced by a long-lived key".into());
    }
    let signer_continues = match (&previous.repository, &next.repository) {
        (Some(before), Some(now)) if now.continues(before) && previous.issuer == next.issuer => {
            if previous.pin_workflow && next.pin_workflow {
                crate::forge::same_workflow(&previous.signer, Some(before), &next.signer, Some(now))
            } else {
                true
            }
        }
        _ => previous.signer == next.signer && previous.issuer == next.issuer,
    };
    if !signer_continues {
        changes.push(format!(
            "signer changed from {:?} to {:?}",
            previous.signer, next.signer
        ));
    }
    if previous.pin_workflow && !next.pin_workflow {
        changes.push("workflow pinning disabled".into());
    }
    if previous.attested_by == Attestor::Vendor && next.attested_by == Attestor::Repackager {
        changes.push("vendor attestation replaced by repackager".into());
    }
    if previous
        .provenance_scopes
        .iter()
        .any(|scope| !next.provenance_scopes.contains(scope))
    {
        changes.push("previous build provenance links dropped".into());
    }
    changes
}

/// Verify proposed continuity under lock before committing the installation.
/// The approval binds the exact signed document, previous record, project,
/// caller and administrator requirements, and release/list role.
pub fn continuity(
    project: &str,
    role: &str,
    bundle: &str,
    previous: Option<&Record>,
    next: &Record,
    constraints: &[Constraints],
    accepted_ids: &[String],
) -> Result<(), Error> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let reasons = changes(previous, next);
    if reasons.is_empty() {
        return Ok(());
    }
    let context = serde_json::to_vec(&(
        project,
        role,
        hex::encode(sha2::Sha256::digest(bundle.as_bytes())),
        previous,
        next,
        constraints,
    ))?;
    let id = hex::encode(sha2::Sha256::digest(&context));
    if accepted_ids.iter().any(|accepted| accepted == &id) {
        return Ok(());
    }
    Err(Error::Change { id, reasons })
}

fn key(constraints: &[Constraints]) -> Result<Option<crate::minisign::PublicKey>, Error> {
    let mut chosen: Option<crate::minisign::PublicKey> = None;
    for c in constraints {
        if let Some(text) = &c.pubkey {
            let key = crate::minisign::PublicKey::parse(text)
                .map_err(|_| Error::Constraint("invalid pinned public key".into()))?;
            if let Some(previous) = &chosen
                && previous.to_file() != key.to_file()
            {
                return Err(Error::Constraint(
                    "caller and administrator public keys disagree".into(),
                ));
            }
            chosen = Some(key);
        }
    }
    Ok(chosen)
}
fn check_constraints(
    constraints: &[Constraints],
    record: &Record,
    bundle: &str,
) -> Result<(), Error> {
    let source = crate::sigstore::source_repository(bundle)?;
    for (index, c) in constraints.iter().enumerate() {
        if !c.pins.is_empty() {
            let pins: Vec<crate::Fingerprint> = c
                .pins
                .iter()
                .map(|p| {
                    p.parse().map_err(|_| {
                        Error::Constraint(format!("invalid signer pin in constraint {index}"))
                    })
                })
                .collect::<Result<_, _>>()?;
            if !pins
                .iter()
                .any(|p| p.verify(record.issuer.as_deref(), source.as_ref()).is_ok())
            {
                return Err(Error::Constraint(format!(
                    "signer does not match constraint {index}'s repository pins"
                )));
            }
        }
        if c.issuer
            .as_ref()
            .is_some_and(|s| record.issuer.as_ref() != Some(s))
            || c.identity.as_ref().is_some_and(|s| s != &record.signer)
            || c.identity_prefix
                .as_ref()
                .is_some_and(|s| !record.signer.starts_with(s))
        {
            return Err(Error::Constraint(format!(
                "signer does not match constraint {index}'s identity policy"
            )));
        }
    }
    Ok(())
}
fn host_policy(bundle: &str) -> Result<Policy, Error> {
    // For a host project, HTTPS to that host is the first-use authority. Never
    // infer project intent here: the caller still requires an exact project.
    let payload = crate::sigstore::peek_statement(bundle)?;
    let value: serde_json::Value = serde_json::from_slice(&payload)?;
    let id = &value["predicate"]["identity"];
    Ok(Policy {
        issuer: id["issuer"].as_str().map(str::to_owned),
        identity: id["key_id"].as_str().map(str::to_owned),
        identity_prefix: None,
    })
}

pub struct Release {
    pub verified: crate::Verified,
    pub statement: crate::Statement,
    pub record: Record,
}
pub fn release(
    project: &str,
    resolved_id: Option<&str>,
    bundle: &str,
    constraints: &[Constraints],
    options: crate::Options<'_>,
) -> Result<Release, Error> {
    let key = key(constraints)?;
    let (verified, repository) = if let Some(key) = &key {
        (crate::verify(bundle, &Trust::Key(key), options, &[])?, None)
    } else if crate::sigstore::Policy::for_project(project).is_some() {
        let accepted = crate::verify_forge(
            bundle,
            &crate::forge::Expected::new(project).resolved(resolved_id),
            options,
            &[],
        )?;
        (accepted.verified, accepted.check.pin)
    } else {
        (
            crate::verify(
                bundle,
                &Trust::Identity(&host_policy(bundle)?),
                options,
                &[],
            )?,
            None,
        )
    };
    if verified.project != project && repository.is_none() {
        return Err(Error::Constraint(
            "signed project differs from requested project".into(),
        ));
    }
    let statement: crate::Statement =
        serde_json::from_slice(&crate::sigstore::peek_statement(bundle)?)?;
    let record = Record {
        project: project.into(),
        scheme: verified.scheme,
        signer: verified.key_id.clone(),
        issuer: verified.issuer.clone(),
        repository,
        pin_workflow: verified.pin_workflow,
        attested_by: verified.attested_by,
        provenance_scopes: scopes(&statement),
    };
    check_constraints(constraints, &record, bundle)?;
    Ok(Release {
        verified,
        statement,
        record,
    })
}
pub struct List {
    pub verified: crate::VerifiedList,
    pub record: Record,
}
pub fn list(
    project: &str,
    resolved_id: Option<&str>,
    bundle: &str,
    constraints: &[Constraints],
    options: crate::Options<'_>,
) -> Result<List, Error> {
    let key = key(constraints)?;
    let (verified, repository) = if let Some(key) = &key {
        (
            crate::verify_release_list(bundle, &Trust::Key(key), options)?,
            None,
        )
    } else if Policy::for_project(project).is_some() {
        let accepted = crate::verify_forge_release_list(
            bundle,
            &crate::forge::Expected::new(project).resolved(resolved_id),
            options,
        )?;
        (accepted.verified, accepted.check.pin)
    } else {
        (
            crate::verify_release_list(bundle, &Trust::Identity(&host_policy(bundle)?), options)?,
            None,
        )
    };
    if verified.list.predicate.project != project && repository.is_none() {
        return Err(Error::Constraint(
            "signed list project differs from requested project".into(),
        ));
    }
    let record = Record {
        project: project.into(),
        scheme: verified.scheme,
        signer: verified.key_id.clone(),
        issuer: verified.issuer.clone(),
        repository,
        pin_workflow: verified.pin_workflow,
        attested_by: Attestor::Vendor,
        provenance_scopes: vec![],
    };
    check_constraints(constraints, &record, bundle)?;
    Ok(List { verified, record })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(workflow: &str, pin_workflow: bool) -> Record {
        Record {
            project: "github.com/jdx/tool".into(),
            scheme: Scheme::SigstoreOidc,
            signer: format!("https://github.com/jdx/tool/.github/workflows/{workflow}"),
            issuer: Some(crate::sigstore::GITHUB_ISSUER.into()),
            repository: Some(ForgePin::of("github.com/jdx/tool", "123")),
            pin_workflow,
            attested_by: Attestor::Vendor,
            provenance_scopes: vec!["linux/x86_64/gnu/default".into()],
        }
    }
    #[test]
    fn refs_continue_but_workflows_require_approval() {
        let old = record("release.yml@refs/tags/v1", true);
        assert!(changes(&old, &record("release.yml@refs/tags/v2", true)).is_empty());
        assert!(!changes(&old, &record("other.yml@refs/tags/v2", true)).is_empty());
        assert!(!changes(&old, &record("other.yml@refs/tags/v2", false)).is_empty());
        assert!(
            changes(
                &record("release.yml@refs/tags/v1", false),
                &record("other.yml@refs/tags/v2", false)
            )
            .is_empty()
        );
    }
    #[test]
    fn proposal_is_bound_to_policy_and_previous_record() {
        let old = record("release.yml@refs/tags/v1", true);
        let next = record("other.yml@refs/tags/v2", true);
        let Error::Change { id, .. } = continuity(
            &old.project,
            "release",
            "bundle",
            Some(&old),
            &next,
            &[],
            &[],
        )
        .unwrap_err() else {
            panic!()
        };
        continuity(
            &old.project,
            "release",
            "bundle",
            Some(&old),
            &next,
            &[],
            std::slice::from_ref(&id),
        )
        .unwrap();
        assert!(
            continuity(
                &old.project,
                "release",
                "different",
                Some(&old),
                &next,
                &[],
                std::slice::from_ref(&id)
            )
            .is_err()
        );
        assert!(
            continuity(
                &old.project,
                "list",
                "bundle",
                Some(&old),
                &next,
                &[],
                std::slice::from_ref(&id)
            )
            .is_err()
        );
        assert!(
            continuity(
                &old.project,
                "release",
                "bundle",
                Some(&old),
                &next,
                &[Constraints::default()],
                &[id]
            )
            .is_err()
        );
    }
    #[test]
    fn constraints_intersect_and_never_replace_project_intent() {
        let bundle = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::Options {
            require_log: true,
            trusted_root: &root,
        };
        let good = Constraints {
            pins: vec!["ps1_snirenkjwr7m5ozgcufameodnm".into()],
            ..Constraints::default()
        };
        release(
            "github.com/jdx/hk",
            Some("922514152"),
            bundle,
            std::slice::from_ref(&good),
            options,
        )
        .unwrap();
        let wrong = Constraints {
            pins: vec!["ps1_aaaaaaaaaaaaaaaaaaaaaaaaaa".into()],
            ..Constraints::default()
        };
        assert!(
            release(
                "github.com/jdx/hk",
                Some("922514152"),
                bundle,
                &[good.clone(), wrong],
                options
            )
            .is_err()
        );
        assert!(
            release(
                "github.com/jdx/other",
                Some("123"),
                bundle,
                &[good],
                options
            )
            .is_err()
        );
    }
    #[test]
    fn administrator_files_are_independent_and_malformed_pins_fail() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.toml"),
            "[projects.\"github.com/jdx/hk\"]\npins = [\"ps1_snirenkjwr7m5ozgcufameodnm\"]\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.toml"),
            "[projects.\"github.com/jdx/hk\"]\npins = [\"invalid\"]\n",
        )
        .unwrap();
        let constraints = admin_constraints(&[dir.path()], "github.com/jdx/hk").unwrap();
        assert_eq!(constraints.len(), 2);
        let root = crate::sigstore::trusted_root(None).unwrap();
        assert!(
            release(
                "github.com/jdx/hk",
                Some("922514152"),
                include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json"),
                &constraints,
                crate::Options {
                    require_log: true,
                    trusted_root: &root
                }
            )
            .is_err()
        );
    }
    #[test]
    fn provenance_and_attestation_reductions_are_proposals() {
        let old = record("release.yml@refs/tags/v1", true);
        let mut next = old.clone();
        next.provenance_scopes.clear();
        next.attested_by = Attestor::Repackager;
        assert_eq!(changes(&old, &next).len(), 2);
        assert!(
            changes(
                &record("release.yml@refs/tags/v1", false),
                &record("other.yml@refs/tags/v2", true)
            )
            .is_empty()
        );
    }
}
