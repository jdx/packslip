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
    pub role: Role,
    pub scheme: Scheme,
    pub signer: String,
    pub issuer: Option<String>,
    pub repository: Option<ForgePin>,
    pub pin_workflow: bool,
    pub attested_by: Attestor,
    pub provenance: Vec<Provenance>,
}
/// Release and release-list continuity have independent remembered records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Release,
    List,
}

/// Preserve each artifact's linked-provenance count within its stable build
/// scope. Formats carrying the same build remain interchangeable. URLs change with build
/// digests and versions; deduplicating scopes would hide partial reductions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Provenance {
    pub scope: String,
    pub links: usize,
}
fn scopes(statement: &crate::Statement) -> Vec<Provenance> {
    let mut scopes: Vec<_> = statement
        .predicate
        .artifacts
        .iter()
        .filter(|a| !a.provenance.is_empty())
        .map(|a| Provenance {
            scope: format!(
                "{:?}",
                (
                    &a.os,
                    &a.arch,
                    &a.libc,
                    &a.variant,
                    a.bin
                        .iter()
                        .map(|bin| &bin.name)
                        .collect::<std::collections::BTreeSet<_>>()
                )
            ),
            links: a
                .provenance
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
        })
        .collect();
    scopes.sort();
    scopes
}
fn retains_provenance(previous: &[Provenance], next: &[Provenance]) -> bool {
    fn group(entries: &[Provenance]) -> BTreeMap<&str, Vec<usize>> {
        let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for entry in entries {
            groups.entry(&entry.scope).or_default().push(entry.links);
        }
        for links in groups.values_mut() {
            links.sort_by(|a, b| b.cmp(a));
        }
        groups
    }
    let before = group(previous);
    let now = group(next);
    before.iter().all(|(scope, counts)| {
        now.get(scope).is_some_and(|next| {
            next.len() >= counts.len()
                && counts
                    .iter()
                    .zip(next)
                    .all(|(before, after)| after >= before)
        })
    })
}
fn changes(previous: &Record, next: &Record) -> Vec<String> {
    let mut changes = Vec::new();
    if previous.scheme == Scheme::SigstoreOidc && next.scheme == Scheme::SigstoreKey {
        changes.push("OIDC identity replaced by a long-lived key".into());
    }
    let signer_continues = previous.scheme == next.scheme
        && previous.issuer == next.issuer
        && match (&previous.repository, &next.repository) {
            (Some(before), Some(now)) => {
                now.continues(before)
                    && (!next.pin_workflow
                        || crate::forge::same_workflow(
                            &previous.signer,
                            Some(before),
                            &next.signer,
                            Some(now),
                        ))
            }
            (Some(_), None) => false,
            _ if previous.scheme == Scheme::SigstoreOidc => crate::forge::same_workflow(
                &previous.signer,
                None,
                &next.signer,
                next.repository.as_ref(),
            ),
            _ => previous.signer == next.signer,
        };
    if !signer_continues {
        match (&previous.repository, &next.repository) {
            (Some(before), Some(now)) if !now.continues(before) => changes.push(format!(
                "repository identity changed from {} (ID {}) to {} (ID {})",
                before.project, before.repository_id, now.project, now.repository_id
            )),
            (Some(before), None) => changes.push(format!(
                "repository pin lost: {} (ID {})",
                before.project, before.repository_id
            )),
            _ => {}
        }
        if previous.issuer != next.issuer {
            changes.push(format!(
                "OIDC issuer changed from {:?} to {:?}",
                previous.issuer, next.issuer
            ));
        }
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
    if !retains_provenance(&previous.provenance, &next.provenance) {
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
    let expected_role = match role {
        "release" => Role::Release,
        "list" => Role::List,
        _ => return Err(Error::Constraint("unknown trust record role".into())),
    };
    if next.role != expected_role || previous.is_some_and(|record| record.role != expected_role) {
        return Err(Error::Constraint(
            "trust record belongs to a different document role".into(),
        ));
    }
    let prior_project_matches = previous.is_none_or(|before| {
        before.project == project
            || before
                .repository
                .as_ref()
                .zip(next.repository.as_ref())
                .is_some_and(|(before_pin, next_pin)| {
                    next_pin.continues(before_pin)
                        && crate::model::repository_subpath(&before.project)
                            == crate::model::repository_subpath(project)
                })
    });
    if next.project != project || !prior_project_matches {
        return Err(Error::Constraint(
            "trust record belongs to a different project".into(),
        ));
    }
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
        if [&c.issuer, &c.identity, &c.identity_prefix]
            .iter()
            .any(|value| value.as_ref().is_some_and(|text| text.trim().is_empty()))
        {
            return Err(Error::Constraint(
                "identity and issuer constraints must be nonempty".into(),
            ));
        }
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
fn host_policy(
    bundle: &str,
    constraints: &[Constraints],
    host_authority: bool,
) -> Result<Policy, Error> {
    if !host_authority {
        // Requirements may come from separate administrator and caller sources.
        // Select a complete verification policy across them; check_constraints
        // still requires every source to agree after signature verification.
        let policy = Policy {
            issuer: constraints.iter().find_map(|c| c.issuer.clone()),
            identity: constraints.iter().find_map(|c| c.identity.clone()),
            identity_prefix: constraints.iter().find_map(|c| c.identity_prefix.clone()),
        };
        if policy.issuer.is_none()
            || (policy.identity.is_none() && policy.identity_prefix.is_none())
        {
            return Err(Error::Constraint("host bundle requires a pinned identity and issuer, or authenticated named-host discovery".into()));
        }
        return Ok(policy);
    }
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
    release_under(project, resolved_id, bundle, constraints, options, false)
}
/// First-use host trust requires bytes fetched from the named HTTPS host.
/// Arbitrary supplied bundles must instead use independent key/identity pins.
pub fn release_host(
    document: &crate::discovery::HostDocument,
    constraints: &[Constraints],
    options: crate::Options<'_>,
) -> Result<Release, Error> {
    let bundle = std::str::from_utf8(document.bytes())
        .map_err(|_| Error::Constraint("host bundle is not UTF-8".into()))?;
    release_under(document.project(), None, bundle, constraints, options, true)
}

/// A signed list fetched from the named host also authorizes exact bundle
/// bytes on a CDN. Reverify its signature and digest binding before carrying
/// that first-use authority across origins; every independent pin still applies.
pub fn release_listed_host(
    document: &crate::discovery::HostDocument,
    url: &str,
    bundle: &str,
    constraints: &[Constraints],
    options: crate::Options<'_>,
) -> Result<Release, Error> {
    let list = list_host(document, constraints, options)?;
    if list.verified.list.digest_of(url)
        != Some(hex::encode(sha2::Sha256::digest(bundle.as_bytes())).as_str())
    {
        return Err(Error::Constraint(
            "release bundle differs from its named-host signed list digest".into(),
        ));
    }
    release_under(document.project(), None, bundle, constraints, options, true)
}
fn release_under(
    project: &str,
    resolved_id: Option<&str>,
    bundle: &str,
    constraints: &[Constraints],
    options: crate::Options<'_>,
    host_authority: bool,
) -> Result<Release, Error> {
    let key = key(constraints)?;
    if key.is_some() && Policy::for_project(project).is_some() {
        return Err(Error::Constraint("forge projects require the forge's OIDC identity; a public-key constraint cannot replace that requirement".into()));
    }
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
                &Trust::Identity(&host_policy(bundle, constraints, host_authority)?),
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
        role: Role::Release,
        scheme: verified.scheme,
        signer: verified.key_id.clone(),
        issuer: verified.issuer.clone(),
        repository,
        pin_workflow: verified.pin_workflow,
        attested_by: verified.attested_by,
        provenance: scopes(&statement),
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
    list_under(project, resolved_id, bundle, constraints, options, false)
}
/// Verify a first-use host list only with the named transport authority.
pub fn list_host(
    document: &crate::discovery::HostDocument,
    constraints: &[Constraints],
    options: crate::Options<'_>,
) -> Result<List, Error> {
    let bundle = std::str::from_utf8(document.bytes())
        .map_err(|_| Error::Constraint("host bundle is not UTF-8".into()))?;
    list_under(document.project(), None, bundle, constraints, options, true)
}
fn list_under(
    project: &str,
    resolved_id: Option<&str>,
    bundle: &str,
    constraints: &[Constraints],
    options: crate::Options<'_>,
    host_authority: bool,
) -> Result<List, Error> {
    let key = key(constraints)?;
    if key.is_some() && Policy::for_project(project).is_some() {
        return Err(Error::Constraint("forge projects require the forge's OIDC identity; a public-key constraint cannot replace that requirement".into()));
    }
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
            crate::verify_release_list(
                bundle,
                &Trust::Identity(&host_policy(bundle, constraints, host_authority)?),
                options,
            )?,
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
        role: Role::List,
        scheme: verified.scheme,
        signer: verified.key_id.clone(),
        issuer: verified.issuer.clone(),
        repository,
        pin_workflow: verified.pin_workflow,
        attested_by: Attestor::Vendor,
        provenance: vec![],
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
            role: Role::Release,
            scheme: Scheme::SigstoreOidc,
            signer: format!("https://github.com/jdx/tool/.github/workflows/{workflow}"),
            issuer: Some(crate::sigstore::GITHUB_ISSUER.into()),
            repository: Some(ForgePin::of("github.com/jdx/tool", "123")),
            pin_workflow,
            attested_by: Attestor::Vendor,
            provenance: vec![Provenance {
                scope: "linux/x86_64/gnu/default".into(),
                links: 1,
            }],
        }
    }
    #[test]
    fn list_and_release_records_have_independent_continuity() {
        let release = record("release.yml@refs/tags/v1", true);
        let mut list = release.clone();
        list.role = Role::List;
        list.provenance.clear();
        continuity(
            &list.project,
            "list",
            "list bundle",
            Some(&list),
            &list,
            &[],
            &[],
        )
        .unwrap();
        assert!(matches!(
            continuity(
                &list.project,
                "list",
                "list bundle",
                Some(&release),
                &list,
                &[],
                &[]
            ),
            Err(Error::Constraint(_))
        ));
        assert!(matches!(
            continuity(
                &release.project,
                "list",
                "list bundle",
                None,
                &release,
                &[],
                &[]
            ),
            Err(Error::Constraint(_))
        ));
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
    fn repository_ids_and_unpinned_workflow_refs_are_compared() {
        let old = record("release.yml@refs/tags/v1", true);
        let mut recreated = old.clone();
        recreated.repository = Some(ForgePin::of("github.com/jdx/tool", "456"));
        let reasons = changes(&old, &recreated);
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("ID 123") && reason.contains("ID 456"))
        );
        let mut old = old;
        old.repository = None;
        let mut next = record("release.yml@refs/tags/v2", true);
        next.repository = None;
        assert!(changes(&old, &next).is_empty());
        next.signer = record("other.yml@refs/tags/v2", true).signer;
        assert!(!changes(&old, &next).is_empty());
    }
    #[test]
    fn project_aliases_follow_repository_ids_but_not_different_tools() {
        let old = record("release.yml@refs/tags/v1", true);
        let mut renamed = old.clone();
        renamed.project = "github.com/new-owner/renamed".into();
        renamed.repository = Some(ForgePin::of(&renamed.project, "123"));
        renamed.signer =
            "https://github.com/new-owner/renamed/.github/workflows/release.yml@refs/tags/v2"
                .into();
        continuity(
            &renamed.project,
            "release",
            "verified bundle",
            Some(&old),
            &renamed,
            &[],
            &[],
        )
        .unwrap();
        renamed.project.push_str("/different-tool");
        assert!(
            continuity(
                &renamed.project,
                "release",
                "verified bundle",
                Some(&old),
                &renamed,
                &[],
                &[]
            )
            .is_err()
        );
    }
    #[test]
    fn empty_identity_pins_are_rejected_even_with_named_host_authority() {
        let bundle = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        let root = crate::sigstore::trusted_root(None).unwrap();
        for constraint in [
            Constraints {
                issuer: Some("".into()),
                ..Default::default()
            },
            Constraints {
                identity: Some(" ".into()),
                ..Default::default()
            },
            Constraints {
                issuer: Some(crate::sigstore::GITHUB_ISSUER.into()),
                identity_prefix: Some("".into()),
                ..Default::default()
            },
        ] {
            for host_authority in [false, true] {
                assert!(matches!(
                    release_under(
                        "example.test",
                        None,
                        bundle,
                        std::slice::from_ref(&constraint),
                        crate::Options {
                            trusted_root: &root,
                            require_log: true
                        },
                        host_authority
                    ),
                    Err(Error::Constraint(_))
                ));
            }
        }
    }
    #[test]
    fn host_identity_and_issuer_can_come_from_independent_requirements() {
        let bundle = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        let root = crate::sigstore::trusted_root(None).unwrap();
        let payload = crate::sigstore::peek_statement(bundle).unwrap();
        let statement: crate::Statement = serde_json::from_slice(&payload).unwrap();
        let identity = statement.predicate.identity.key_id.clone();
        let mut constraints = vec![
            Constraints {
                issuer: Some(crate::sigstore::GITHUB_ISSUER.into()),
                ..Default::default()
            },
            Constraints {
                identity: Some(identity.clone()),
                ..Default::default()
            },
        ];
        let options = crate::Options {
            trusted_root: &root,
            require_log: true,
        };
        let policy = host_policy(bundle, &constraints, false).unwrap();
        crate::verify(bundle, &Trust::Identity(&policy), options, &[]).unwrap();
        constraints.push(Constraints {
            identity_prefix: Some("https://github.com/jdx/hk/".into()),
            ..Default::default()
        });
        let policy = host_policy(bundle, &constraints, false).unwrap();
        crate::verify(bundle, &Trust::Identity(&policy), options, &[]).unwrap();
        constraints[0].issuer = Some("https://different-issuer.test".into());
        let policy = host_policy(bundle, &constraints, false).unwrap();
        assert!(crate::verify(bundle, &Trust::Identity(&policy), options, &[]).is_err());
    }
    #[test]
    fn arbitrary_host_bundles_cannot_supply_their_own_trust_policy() {
        let bundle = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::Options {
            require_log: true,
            trusted_root: &root,
        };
        assert!(matches!(
            release("example.test", None, bundle, &[], options),
            Err(Error::Constraint(_))
        ));
        assert!(matches!(
            list("example.test", None, bundle, &[], options),
            Err(Error::Constraint(_))
        ));
        let policy = host_policy(
            "invalid",
            &[Constraints {
                issuer: Some("https://issuer.test".into()),
                identity: Some("person@example.test".into()),
                ..Constraints::default()
            }],
            false,
        )
        .unwrap();
        assert_eq!(policy.identity.as_deref(), Some("person@example.test"));
    }
    #[test]
    fn provenance_tracks_artifacts_and_link_counts_without_deduplicating_scopes() {
        let mut statement: crate::Statement = serde_json::from_slice(
            &crate::sigstore::peek_statement(include_str!(
                "../tests/fixtures/hk-v2.3.0.sigstore.json"
            ))
            .unwrap(),
        )
        .unwrap();
        statement.predicate.artifacts.truncate(1);
        statement.predicate.artifacts[0].provenance = vec![
            "https://one.test/build1".into(),
            "https://two.test/build1".into(),
        ];
        statement
            .predicate
            .artifacts
            .push(statement.predicate.artifacts[0].clone());
        let before = scopes(&statement);
        statement.predicate.artifacts[1].provenance.clear();
        assert!(!retains_provenance(&before, &scopes(&statement)));
        statement.predicate.artifacts[1].provenance = vec!["https://one.test/build2".into()];
        assert!(!retains_provenance(&before, &scopes(&statement)));
        statement.predicate.artifacts[1]
            .provenance
            .push("https://two.test/build2".into());
        assert!(retains_provenance(&before, &scopes(&statement)));
        statement.predicate.artifacts[1].format = Some("zip".into());
        assert!(retains_provenance(&before, &scopes(&statement)));
        statement.predicate.artifacts[1].bin = vec![crate::Bin::new("replacement")];
        assert!(!retains_provenance(&before, &scopes(&statement)));
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
    #[cfg(feature = "sign")]
    fn key_authority_requires_exact_host_project_and_cannot_replace_forge_identity() {
        let key = crate::minisign::SecretKey::from_seed([7; 32]);
        let public = key.public_key();
        let mut statement: crate::Statement = serde_json::from_slice(
            &crate::sigstore::peek_statement(include_str!(
                "../tests/fixtures/hk-v2.3.0.sigstore.json"
            ))
            .unwrap(),
        )
        .unwrap();
        statement.predicate.project = "example.test".into();
        statement.predicate.identity.scheme = Scheme::SigstoreKey;
        statement.predicate.identity.key_id = crate::minisign::key_id_hex(&public.key_id);
        statement.predicate.identity.issuer = None;
        statement.predicate.identity.pin_workflow = None;
        let bundle = crate::sigstore::sign(
            crate::sigstore::Signer::Key {
                key: key.clone(),
                log: false,
            },
            &serde_json::to_vec(&statement).unwrap(),
        )
        .unwrap();
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::Options {
            require_log: false,
            trusted_root: &root,
        };
        let pinned = Constraints {
            pubkey: Some(public.to_file()),
            ..Constraints::default()
        };
        let accepted = release(
            "example.test",
            None,
            &bundle,
            std::slice::from_ref(&pinned),
            options,
        )
        .unwrap();
        let mut forge_statement = statement.clone();
        forge_statement.predicate.project = "github.com/jdx/hk".into();
        let forge_bundle = crate::sigstore::sign(
            crate::sigstore::Signer::Key { key, log: false },
            &serde_json::to_vec(&forge_statement).unwrap(),
        )
        .unwrap();
        assert!(
            release(
                "github.com/jdx/hk",
                None,
                &forge_bundle,
                std::slice::from_ref(&pinned),
                options
            )
            .is_err()
        );
        assert_eq!(accepted.record.scheme, Scheme::SigstoreKey);
        assert!(accepted.record.repository.is_none()); // No claim of forge ownership.
        assert!(
            release(
                "other.test",
                None,
                &bundle,
                std::slice::from_ref(&pinned),
                options
            )
            .is_err()
        );
        assert!(release("example.test", None, &bundle, &[], options).is_err());
        let forge_pin = Constraints {
            pins: vec!["ps1_snirenkjwr7m5ozgcufameodnm".into()],
            ..Constraints::default()
        };
        assert!(release("example.test", None, &bundle, &[pinned, forge_pin], options).is_err());
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
        next.provenance.clear();
        next.attested_by = Attestor::Repackager;
        assert_eq!(changes(&old, &next).len(), 2);
        assert!(
            !changes(
                &record("release.yml@refs/tags/v1", false),
                &record("other.yml@refs/tags/v2", true)
            )
            .is_empty()
        );
    }
}
