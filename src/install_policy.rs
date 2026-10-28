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
///
/// A file's entry matches `project` as [`crate::forge::same_project`]
/// compares names: a GitHub or GitLab repository's name in any case, as the
/// forge resolves it. A file with two entries for the project, spelled in
/// different cases, is refused, since it does not say which one it means.
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
            let file: PinFile = toml::from_str(&std::fs::read_to_string(&path)?)?;
            let mut matching = file
                .projects
                .iter()
                .filter(|(name, _)| crate::forge::same_project(name, project));
            if let Some((name, c)) = matching.next() {
                if let Some((other, _)) = matching.next() {
                    return Err(Error::Constraint(format!(
                        "{} pins {project} twice, as {name} and {other}; keep one entry",
                        path.display()
                    )));
                }
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
    /// For a GitHub or GitLab project, its forge pin, which follows the
    /// repository to a new name. For a keyless domain project, the pin of
    /// the repository that signed it, which holds later signers to that
    /// repository as well as to the workflow URL (see [`continuity`]).
    pub repository: Option<ForgePin>,
    pub pin_workflow: bool,
    pub attested_by: Attestor,
    /// For a key-signed project, the fingerprint of the whole public key
    /// that verified the document, since `signer` is only the id the key
    /// file carries. None for a record from before it was kept, which the
    /// history's remembered key stands in for (see
    /// [`crate::install_fs::History::name_remembered_key`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// One entry per artifact. An entry without links only records that the
    /// release offers its build. It matters only while this is the next
    /// record, so remembered state and approval IDs omit it and keep the form
    /// packslip 1.5 through 1.10 wrote.
    #[serde(serialize_with = "linked_only")]
    pub provenance: Vec<Provenance>,
}
fn linked_only<S: serde::Serializer>(
    entries: &[Provenance],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(entries.iter().filter(|entry| entry.links > 0))
}
/// Release and release-list continuity have independent remembered records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Release,
    List,
}

/// One artifact's build scope and its number of distinct provenance links.
/// URLs change with build digests and versions, so releases compare counts.
/// Each artifact keeps its own entry: deduplicating scopes would hide partial
/// reductions.
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
/// The build a scope names: its `os`, `arch`, `libc`, and `variant`. Scopes
/// still end with the command names that packslip 1.5 through 1.10 compared,
/// so those versions read remembered state as before. Comparing without them
/// keeps a renamed command from hiding a build that lost provenance.
fn build(scope: &str) -> &str {
    scope.split_once(", {").map_or(scope, |(build, _)| build)
}
/// Compare only builds both releases offer, so retiring a build drops
/// nothing while the new release still offers a build that carried links.
/// One that offers none of them, for example because it relabels every
/// build, has nothing to compare and drops them all. For a build in both,
/// pair its artifacts' link counts largest first until the shorter list
/// ends; no count may fall. Retiring a format thus drops nothing unless the
/// formats left carry fewer links.
fn retains_provenance(previous: &[Provenance], next: &[Provenance]) -> bool {
    fn group(entries: &[Provenance]) -> BTreeMap<&str, Vec<usize>> {
        let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for entry in entries {
            groups
                .entry(build(&entry.scope))
                .or_default()
                .push(entry.links);
        }
        for links in groups.values_mut() {
            links.sort_by(|a, b| b.cmp(a));
        }
        groups
    }
    let before = group(previous);
    let now = group(next);
    // Remembered state holds only linked entries, so builds without links
    // never count as shared.
    let mut linked = before
        .iter()
        .filter(|(_, counts)| counts.first().is_some_and(|&links| links > 0))
        .peekable();
    let shares_a_build =
        linked.peek().is_none() || linked.any(|(build, _)| now.contains_key(build));
    shares_a_build
        && before.iter().all(|(build, counts)| {
            now.get(build).is_none_or(|next| {
                counts
                    .iter()
                    .zip(next)
                    .all(|(before, after)| after >= before)
            })
        })
}
/// Whether `project` is a GitHub or GitLab project, whose repository ID
/// follows it across renames. Any other project is named after its host.
fn forge_project(project: &str) -> bool {
    Policy::for_project(project).is_some()
}

/// The pin a keyless domain project's record keeps: the repository its
/// signing certificate records, when the certificate records that
/// repository's ID on GitHub or GitLab. A later list or release must then
/// come from the same repository as well as the same workflow URL, so
/// neither a repository recreated under the old name nor another
/// repository's run that calls the signing workflow as a reusable workflow
/// continues the signer. None for a forge project, whose pin
/// [`crate::verify_forge`] gives, for a key-signed bundle, and for a
/// certificate that records no repository ID on GitHub or GitLab.
fn domain_repository(project: &str, bundle: &str) -> Result<Option<ForgePin>, Error> {
    if forge_project(project) {
        return Ok(None);
    }
    Ok(crate::sigstore::source_repository(bundle)?
        .as_ref()
        .and_then(crate::forge::source_pin))
}

fn changes(previous: &Record, next: &Record) -> Vec<String> {
    let mut changes = Vec::new();
    if previous.scheme == Scheme::SigstoreOidc && next.scheme == Scheme::SigstoreKey {
        changes.push("OIDC identity replaced by a long-lived key".into());
    }
    if previous.scheme == Scheme::SigstoreKey && next.scheme == Scheme::SigstoreKey {
        match (&previous.key, &next.key) {
            (Some(before), Some(now)) if before != now => {
                changes.push(format!("public key changed from {before} to {now}"));
            }
            (Some(before), None) => {
                changes.push(format!("public key {before} no longer identified"));
            }
            _ => {}
        }
    }
    let signer_continues = previous.scheme == next.scheme
        && previous.issuer == next.issuer
        && match (&previous.repository, &next.repository) {
            // A keyless domain project is held to the repository that signed
            // it as well as to the workflow URL: the signer continues only
            // from the same repository ID and the same workflow URL apart
            // from its ref, whatever `pin_workflow` declares, as without an
            // ID. Its name is the host's, so a renamed repository's new URL
            // still needs approval.
            (Some(before), Some(now)) if !forge_project(&next.project) => {
                now.continues(before)
                    && crate::forge::same_workflow(&previous.signer, None, &next.signer, None)
            }
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
            // A record without a repository ID has only its workflow URL to
            // compare, such as one packslip 1.10.1 or earlier kept for a
            // keyless domain project. The next list or release from that
            // workflow continues it, and the record that replaces it adopts
            // that document's repository ID without asking: trust on first
            // use of the ID, as the URL alone was trusted before.
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
    // A forge project's repository ID follows it to a new name. A domain
    // project's names the repository that signed it, which can sign other
    // projects too, so it never makes another project's record this one's.
    let prior_project_matches = previous.is_none_or(|before| {
        crate::forge::same_project(&before.project, project)
            || before
                .repository
                .as_ref()
                .zip(next.repository.as_ref())
                .filter(|_| forge_project(&before.project) && forge_project(project))
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
                        Error::Constraint(format!(
                            "invalid signer fingerprint in constraint {index}"
                        ))
                    })
                })
                .collect::<Result<_, _>>()?;
            if !pins
                .iter()
                .any(|p| p.verify(record.issuer.as_deref(), source.as_ref()).is_ok())
            {
                return Err(Error::Constraint(format!(
                    "signer does not match constraint {index}'s signer fingerprints"
                )));
            }
        }
        if c.issuer
            .as_ref()
            .is_some_and(|s| record.issuer.as_ref() != Some(s))
            || c.identity
                .as_ref()
                .is_some_and(|s| !crate::forge::same_url(&record.signer, s))
            || c.identity_prefix
                .as_ref()
                .is_some_and(|s| !crate::forge::identity_has_prefix(&record.signer, s))
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
    if !crate::forge::same_project(&verified.project, project) && repository.is_none() {
        return Err(Error::Constraint(
            "signed project differs from requested project".into(),
        ));
    }
    let repository = match repository {
        None => domain_repository(project, bundle)?,
        pin => pin,
    };
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
        key: key.as_ref().map(crate::minisign::PublicKey::fingerprint),
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
    if !crate::forge::same_project(&verified.list.predicate.project, project)
        && repository.is_none()
    {
        return Err(Error::Constraint(
            "signed list project differs from requested project".into(),
        ));
    }
    let repository = match repository {
        None => domain_repository(project, bundle)?,
        pin => pin,
    };
    let record = Record {
        project: project.into(),
        role: Role::List,
        scheme: verified.scheme,
        signer: verified.key_id.clone(),
        issuer: verified.issuer.clone(),
        repository,
        pin_workflow: verified.pin_workflow,
        attested_by: Attestor::Vendor,
        key: key.as_ref().map(crate::minisign::PublicKey::fingerprint),
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
            key: None,
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
    fn keyed(public: &crate::minisign::PublicKey) -> Record {
        Record {
            project: "example.test".into(),
            role: Role::Release,
            scheme: Scheme::SigstoreKey,
            signer: crate::minisign::key_id_hex(&public.key_id),
            issuer: None,
            repository: None,
            pin_workflow: false,
            attested_by: Attestor::Vendor,
            key: Some(public.fingerprint()),
            provenance: vec![],
        }
    }
    #[test]
    fn a_replacement_key_with_the_same_id_is_a_trust_change() {
        let old = crate::minisign::SecretKey::from_seed([1u8; 32]).public_key();
        let mut replacement = crate::minisign::SecretKey::from_seed([2u8; 32]).public_key();
        replacement.key_id = old.key_id;
        let (before, after) = (keyed(&old), keyed(&replacement));
        assert_eq!(before.signer, after.signer);
        assert!(changes(&before, &before).is_empty());
        let reasons = changes(&before, &after);
        assert_eq!(reasons.len(), 1, "{reasons:?}");
        assert!(
            reasons[0].starts_with("public key changed from "),
            "{reasons:?}"
        );
        let Err(Error::Change { id, .. }) = continuity(
            "example.test",
            "release",
            "bundle",
            Some(&before),
            &after,
            &[],
            &[],
        ) else {
            panic!("a different key with the old id continued the signer");
        };
        continuity(
            "example.test",
            "release",
            "bundle",
            Some(&before),
            &after,
            &[],
            &[id],
        )
        .unwrap();
    }
    #[test]
    fn a_record_without_a_key_adopts_the_key_it_meets() {
        let public = crate::minisign::SecretKey::from_seed([1u8; 32]).public_key();
        let mut legacy = keyed(&public);
        legacy.key = None;
        assert!(changes(&legacy, &keyed(&public)).is_empty());
        let reasons = changes(&keyed(&public), &legacy);
        assert_eq!(reasons.len(), 1, "{reasons:?}");
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
    const PACKSLIP_WORKFLOW: &str = "https://github.com/jdx/packslip/.github/workflows/release.yml";
    const PACKSLIP_REPOSITORY: Option<(&str, &str)> = Some(("github.com/jdx/packslip", "123"));
    /// `PACKSLIP_WORKFLOW` at `tag`.
    fn at(tag: &str) -> String {
        format!("{PACKSLIP_WORKFLOW}@refs/tags/{tag}")
    }
    /// A record of packslip.dev, a keyless domain project, for `role`,
    /// signed by `signer` in a run of `source`, the repository name and ID
    /// its certificate records, if any.
    fn domain(role: Role, signer: &str, source: Option<(&str, &str)>) -> Record {
        Record {
            project: "packslip.dev".into(),
            role,
            scheme: Scheme::SigstoreOidc,
            signer: signer.into(),
            issuer: Some(crate::sigstore::GITHUB_ISSUER.into()),
            repository: source.map(|(name, id)| ForgePin::of(name, id)),
            pin_workflow: true,
            attested_by: Attestor::Vendor,
            key: None,
            provenance: vec![],
        }
    }
    #[test]
    fn a_domain_project_remembers_the_repository_its_certificate_records() {
        let hk = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        assert_eq!(
            domain_repository("example.test", hk).unwrap(),
            Some(ForgePin::of("github.com/jdx/hk", "922514152"))
        );
        // mkghtag signed through another repository's reusable workflow, so
        // its certificate names that workflow and records mkghtag's
        // repository, which ran it.
        let mkghtag = include_str!("../tests/fixtures/mkghtag-v0.1.14-10.sigstore.json");
        assert_eq!(
            domain_repository("tools.example.test/mkghtag", mkghtag).unwrap(),
            Some(ForgePin::of(
                "github.com/suzuki-shunsuke/mkghtag",
                "701940692"
            ))
        );
        // A forge project's pin is the one verify_forge gives.
        assert_eq!(domain_repository("github.com/jdx/hk", hk).unwrap(), None);
    }
    #[test]
    fn a_domain_project_is_held_to_the_repository_that_signed_it() {
        for role in [Role::List, Role::Release] {
            let first = domain(role, &at("v1"), PACKSLIP_REPOSITORY);
            // The same workflow of the same repository continues at any ref.
            assert!(changes(&first, &domain(role, &at("v2"), PACKSLIP_REPOSITORY)).is_empty());
            // A repository recreated under the old name has another ID,
            // though its workflow URL is the same.
            let recreated = domain(role, &at("v2"), Some(("github.com/jdx/packslip", "456")));
            let reasons = changes(&first, &recreated);
            assert!(
                reasons.contains(&"repository identity changed from github.com/jdx/packslip (ID 123) to github.com/jdx/packslip (ID 456)".into()),
                "{reasons:?}"
            );
            // Another repository's run that calls the signing workflow as a
            // reusable workflow: the certificate names the same workflow and
            // records the caller as its source repository.
            let caller = domain(role, &at("v1"), Some(("github.com/someone/app", "789")));
            let reasons = changes(&first, &caller);
            assert!(
                reasons.contains(&"repository identity changed from github.com/jdx/packslip (ID 123) to github.com/someone/app (ID 789)".into()),
                "{reasons:?}"
            );
            // A renamed repository keeps its ID, but its workflow URL
            // changes. As without an ID, a domain project needs approval for
            // that; a GitHub project follows the ID to the new name.
            let moved = "https://github.com/jdx/renamed/.github/workflows/release.yml@refs/tags/v2";
            let renamed = domain(role, moved, Some(("github.com/jdx/renamed", "123")));
            assert_eq!(
                changes(&first, &renamed),
                [format!("signer changed from {:?} to {moved:?}", at("v1"))]
            );
            // Another workflow of the same repository is a changed signer,
            // whatever `pin_workflow` declares, as without an ID.
            let mut unpinned = first.clone();
            unpinned.pin_workflow = false;
            let other = "https://github.com/jdx/packslip/.github/workflows/other.yml@refs/tags/v2";
            let mut next = domain(role, other, PACKSLIP_REPOSITORY);
            next.pin_workflow = false;
            assert!(!changes(&unpinned, &next).is_empty());
            next.signer = at("v2");
            assert!(changes(&unpinned, &next).is_empty());
            // Once an ID is remembered, a certificate without one cannot
            // show the repository is the same.
            assert!(
                changes(&first, &domain(role, &at("v2"), None))
                    .contains(&"repository pin lost: github.com/jdx/packslip (ID 123)".into())
            );
            // Each change is a proposal a person can accept.
            let role_name = if role == Role::List {
                "list"
            } else {
                "release"
            };
            for next in [&recreated, &caller, &renamed] {
                let Err(Error::Change { id, .. }) = continuity(
                    "packslip.dev",
                    role_name,
                    "bundle",
                    Some(&first),
                    next,
                    &[],
                    &[],
                ) else {
                    panic!("{next:?} continued {first:?}")
                };
                continuity(
                    "packslip.dev",
                    role_name,
                    "bundle",
                    Some(&first),
                    next,
                    &[],
                    &[id],
                )
                .unwrap();
            }
        }
        // The ID names the repository that signed, which can sign the
        // host's other projects too, so another project's record never
        // stands for this one's.
        let first = domain(Role::List, &at("v1"), PACKSLIP_REPOSITORY);
        let mut other = first.clone();
        other.project = "packslip.dev/other".into();
        assert!(matches!(
            continuity(
                "packslip.dev/other",
                "list",
                "bundle",
                Some(&first),
                &other,
                &[],
                &[]
            ),
            Err(Error::Constraint(_))
        ));
    }
    #[test]
    fn a_domain_record_without_a_repository_id_adopts_the_next_one() {
        // A record packslip 1.10.1 kept for a keyless domain project.
        let legacy: Record = serde_json::from_value(serde_json::json!({
            "project": "packslip.dev",
            "role": "list",
            "scheme": "sigstore-oidc",
            "signer": at("v1"),
            "issuer": crate::sigstore::GITHUB_ISSUER,
            "repository": null,
            "pin_workflow": true,
            "attested_by": "vendor",
            "provenance": []
        }))
        .unwrap();
        // The next list from the same workflow URL continues it without a
        // proposal, and install remembers that list's record, ID included.
        let adopted = domain(Role::List, &at("v2"), PACKSLIP_REPOSITORY);
        continuity(
            "packslip.dev",
            "list",
            "bundle",
            Some(&legacy),
            &adopted,
            &[],
            &[],
        )
        .unwrap();
        // From then on the ID holds.
        let recreated = domain(
            Role::List,
            &at("v3"),
            Some(("github.com/jdx/packslip", "456")),
        );
        assert!(!changes(&adopted, &recreated).is_empty());
        // A different workflow URL still needs approval, as before.
        let other = "https://github.com/jdx/packslip/.github/workflows/other.yml@refs/tags/v2";
        assert!(!changes(&legacy, &domain(Role::List, other, PACKSLIP_REPOSITORY)).is_empty());
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
    fn github_repository_names_match_in_any_case() {
        // A prior record under another spelling of the repository's name
        // continues, with no repository ID to go by.
        let mut old = record("release.yml@refs/tags/v1", true);
        old.repository = None;
        let mut next = record("release.yml@refs/tags/v2", true);
        next.repository = None;
        next.project = "github.com/JDX/Tool".into();
        continuity(
            &next.project,
            "release",
            "verified bundle",
            Some(&old),
            &next,
            &[],
            &[],
        )
        .unwrap();
        // A monorepo subpath names a tool exactly.
        old.project = "github.com/jdx/tool/cli".into();
        next.project = "github.com/JDX/Tool/CLI".into();
        assert!(
            continuity(
                &next.project,
                "release",
                "verified bundle",
                Some(&old),
                &next,
                &[],
                &[]
            )
            .is_err()
        );

        // So does an identity-prefix constraint, apart from the workflow's
        // path inside the repository.
        let bundle = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::Options {
            require_log: true,
            trusted_root: &root,
        };
        let prefix = |prefix: &str| Constraints {
            identity_prefix: Some(prefix.into()),
            ..Constraints::default()
        };
        let accepted = release(
            "github.com/JDX/HK",
            Some("922514152"),
            bundle,
            &[prefix("https://github.com/JDX/HK/")],
            options,
        )
        .unwrap();
        assert_eq!(accepted.record.project, "github.com/JDX/HK");
        assert!(
            release(
                "github.com/jdx/hk",
                Some("922514152"),
                bundle,
                &[prefix("https://github.com/jdx/hk/.GITHUB/")],
                options
            )
            .is_err()
        );
        // So does an identity constraint, apart from the workflow's path
        // and ref.
        let exact = |identity: &str| Constraints {
            identity: Some(identity.into()),
            ..Constraints::default()
        };
        release(
            "github.com/jdx/hk",
            Some("922514152"),
            bundle,
            &[exact(
                "https://github.com/JDX/HK/.github/workflows/release.yml@refs/tags/v2.3.0",
            )],
            options,
        )
        .unwrap();
        for identity in [
            "https://github.com/jdx/hk/.GITHUB/workflows/release.yml@refs/tags/v2.3.0",
            "https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/V2.3.0",
        ] {
            assert!(
                release(
                    "github.com/jdx/hk",
                    Some("922514152"),
                    bundle,
                    &[exact(identity)],
                    options
                )
                .is_err(),
                "{identity}"
            );
        }
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
    fn hk() -> crate::Statement {
        serde_json::from_slice(
            &crate::sigstore::peek_statement(include_str!(
                "../tests/fixtures/hk-v2.3.0.sigstore.json"
            ))
            .unwrap(),
        )
        .unwrap()
    }
    /// hk's linux/x86_64/gnu build in each listed format with that many
    /// provenance links, and its darwin/aarch64 build when `darwin` is set.
    fn provenance_release(
        version: u32,
        linux: &[(&str, usize)],
        darwin: Option<usize>,
    ) -> Vec<Provenance> {
        let mut statement = hk();
        let template = std::mem::take(&mut statement.predicate.artifacts);
        let builds = linux
            .iter()
            .map(|&(format, links)| (&template[3], format, links))
            .chain(darwin.map(|links| (&template[0], "tar.gz", links)));
        for (artifact, format, links) in builds {
            let mut artifact = artifact.clone();
            artifact.format = Some(format.into());
            artifact.provenance = (0..links)
                .map(|n| format!("https://provenance.test/{version}/{format}/{n}"))
                .collect();
            statement.predicate.artifacts.push(artifact);
        }
        scopes(&statement)
    }
    #[test]
    fn provenance_compares_only_builds_both_releases_offer() {
        let before = provenance_release(1, &[("tar.gz", 2), ("zip", 2)], Some(1));
        let retains = |linux: &[(&str, usize)], darwin| {
            retains_provenance(&before, &provenance_release(2, linux, darwin))
        };
        // Links are counted, not compared: each release has its own URLs.
        assert!(retains(&[("tar.gz", 2), ("zip", 2)], Some(1)));
        // Retiring a build, or one of two equally linked formats, drops
        // nothing.
        assert!(retains(&[("tar.gz", 2), ("zip", 2)], None));
        assert!(retains(&[("tar.gz", 2)], Some(1)));
        // Formats stay interchangeable, and an added format may lack links.
        assert!(retains(&[("tar.xz", 2), ("zip", 2)], Some(1)));
        assert!(retains(
            &[("tar.gz", 2), ("zip", 2), ("tar.xz", 0)],
            Some(1)
        ));
        // In a build both releases offer, match its artifacts' link counts
        // largest first; no count may fall.
        assert!(!retains(&[("tar.gz", 2), ("zip", 0)], Some(1)));
        assert!(!retains(&[("tar.gz", 2), ("zip", 1)], Some(1)));
        assert!(!retains(&[("tar.gz", 3), ("zip", 1)], Some(1)));
        assert!(!retains(&[("tar.gz", 1)], Some(1)));
        assert!(!retains(&[("tar.gz", 2), ("zip", 2)], Some(0)));
        // Keeping only the format that never carried links loses the build's.
        let partial = provenance_release(1, &[("tar.gz", 2), ("zip", 0)], None);
        assert!(!retains_provenance(
            &partial,
            &provenance_release(2, &[("zip", 0)], None)
        ));
        assert!(retains_provenance(
            &partial,
            &provenance_release(2, &[("tar.gz", 2)], None)
        ));
        // As in v1.10.1, counts are matched without regard to format, so
        // links may move from one format to another.
        assert!(retains_provenance(
            &partial,
            &provenance_release(2, &[("tar.gz", 0), ("zip", 2)], None)
        ));
    }
    #[test]
    fn provenance_requires_a_build_that_carried_links() {
        let before = provenance_release(1, &[("tar.gz", 2), ("zip", 2)], None);
        // hk's linux/x86_64 build relabeled from libc gnu to unset, in both
        // formats with that many provenance links.
        let relabeled = |links: usize| {
            let mut statement = hk();
            let linux = statement.predicate.artifacts[3].clone();
            statement.predicate.artifacts = ["tar.gz", "zip"]
                .map(|format| {
                    let mut artifact = linux.clone();
                    artifact.libc = None;
                    artifact.format = Some(format.into());
                    artifact.provenance = (0..links)
                        .map(|n| format!("https://provenance.test/2/{format}/{n}"))
                        .collect();
                    artifact
                })
                .into();
            scopes(&statement)
        };
        // A release that relabels every build shares none with the last
        // release, so nothing is compared: that drops the links, whether or
        // not the relabeled builds carry links of their own.
        assert!(!retains_provenance(&before, &relabeled(0)));
        assert!(!retains_provenance(&before, &relabeled(2)));
        // Retiring builds or formats drops nothing while one build that
        // carried links remains.
        let both = provenance_release(1, &[("tar.gz", 2), ("zip", 2)], Some(1));
        assert!(retains_provenance(
            &both,
            &provenance_release(2, &[], Some(1))
        ));
        assert!(retains_provenance(
            &both,
            &provenance_release(2, &[("zip", 2)], None)
        ));
        // A build that carried no links is not remembered, so sharing only it
        // does not count.
        let darwin_unlinked = provenance_release(1, &[("tar.gz", 2)], Some(0));
        assert!(!retains_provenance(
            &darwin_unlinked,
            &provenance_release(2, &[], Some(0))
        ));
        // Without earlier links there is nothing to drop.
        let unlinked = provenance_release(1, &[("tar.gz", 0)], Some(0));
        assert!(retains_provenance(&unlinked, &relabeled(0)));
        assert!(retains_provenance(&[], &relabeled(0)));
        assert!(retains_provenance(&[], &[]));
    }
    #[test]
    fn remembered_provenance_keeps_its_earlier_form() {
        // packslip 1.5 through 1.10 remembered scopes with command names.
        let remembered = scopes(&hk());
        let legacy = r#"(Some("linux"), Some("x86_64"), Some("gnu"), None, {"hk"})"#;
        assert!(
            remembered
                .iter()
                .any(|entry| entry.scope == legacy && entry.links == 1)
        );
        // A renamed command neither retires a build nor hides lost links.
        let mut renamed = hk();
        for artifact in &mut renamed.predicate.artifacts {
            artifact.bin = vec![crate::Bin::new("hk2")];
        }
        assert!(retains_provenance(&remembered, &scopes(&renamed)));
        renamed.predicate.artifacts[3].provenance.clear();
        assert!(!retains_provenance(&remembered, &scopes(&renamed)));
        // Entries without links mark offered builds but are not remembered.
        let mut next = record("release.yml@refs/tags/v1", true);
        next.provenance = scopes(&renamed);
        let stored: Record = serde_json::from_value(serde_json::to_value(&next).unwrap()).unwrap();
        assert_eq!(stored.provenance.len(), next.provenance.len() - 1);
        assert!(stored.provenance.iter().all(|entry| entry.links == 1));
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
        let err = release(
            "github.com/jdx/hk",
            Some("922514152"),
            bundle,
            &[good.clone(), wrong],
            options,
        )
        .err()
        .expect("a mismatched signer fingerprint must fail");
        assert_eq!(
            err.to_string(),
            "signer does not match constraint 1's signer fingerprints"
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
        let err = release(
            "github.com/jdx/hk",
            Some("922514152"),
            include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json"),
            &constraints,
            crate::Options {
                require_log: true,
                trusted_root: &root,
            },
        )
        .err()
        .expect("a malformed signer fingerprint must fail");
        assert_eq!(
            err.to_string(),
            "invalid signer fingerprint in constraint 1"
        );
    }
    #[test]
    fn administrator_entries_match_a_github_repository_name_in_any_case() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.toml"),
            "[projects.\"github.com/JDX/HK\"]\npins = [\"ps1_snirenkjwr7m5ozgcufameodnm\"]\n",
        )
        .unwrap();
        for project in [
            "github.com/jdx/hk",
            "github.com/JDX/HK",
            "github.com/Jdx/Hk",
        ] {
            assert_eq!(
                admin_constraints(&[dir.path()], project).unwrap().len(),
                1,
                "{project}"
            );
        }
        // A monorepo subpath, and a name on another host, match exactly.
        std::fs::write(
            dir.path().join("b.toml"),
            "[projects.\"github.com/jdx/hk/Tool\"]\npins = [\"ps1_snirenkjwr7m5ozgcufameodnm\"]\n\
             [projects.\"tool.example.com/Tool\"]\npins = [\"ps1_snirenkjwr7m5ozgcufameodnm\"]\n",
        )
        .unwrap();
        assert!(
            admin_constraints(&[dir.path()], "github.com/jdx/hk/tool")
                .unwrap()
                .is_empty()
        );
        assert!(
            admin_constraints(&[dir.path()], "tool.example.com/tool")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            admin_constraints(&[dir.path()], "github.com/JDX/HK/Tool")
                .unwrap()
                .len(),
            1
        );
        // Two entries for one repository in one file are refused, not
        // picked between.
        std::fs::write(
            dir.path().join("c.toml"),
            "[projects.\"github.com/jdx/hk\"]\npins = [\"ps1_snirenkjwr7m5ozgcufameodnm\"]\n\
             [projects.\"github.com/JDX/hk\"]\npins = []\n",
        )
        .unwrap();
        let err = admin_constraints(&[dir.path()], "github.com/jdx/HK")
            .unwrap_err()
            .to_string();
        assert!(err.contains("c.toml pins github.com/jdx/HK twice"), "{err}");
        // A project the duplicates are not for is unaffected.
        assert!(
            admin_constraints(&[dir.path()], "github.com/jdx/mise")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn provenance_and_attestation_reductions_are_proposals() {
        let mut old = record("release.yml@refs/tags/v1", true);
        let mut next = old.clone();
        next.provenance[0].links = 0;
        next.attested_by = Attestor::Repackager;
        assert_eq!(changes(&old, &next).len(), 2);
        // A release that offers no build that carried links drops them.
        next.provenance.clear();
        assert_eq!(changes(&old, &next).len(), 2);
        // Retiring one build while another keeps its links drops nothing.
        old.provenance.push(Provenance {
            scope: "darwin/aarch64/none/default".into(),
            links: 1,
        });
        next.provenance = vec![old.provenance[1].clone()];
        assert_eq!(changes(&old, &next).len(), 1);
        assert!(
            !changes(
                &record("release.yml@refs/tags/v1", false),
                &record("other.yml@refs/tags/v2", true)
            )
            .is_empty()
        );
    }
}
