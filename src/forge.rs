//! Forge identity: a GitHub or GitLab project's name locates it, and the
//! forge's immutable repository ID pins it.
//!
//! A repository can be renamed, transferred to another owner, or deleted
//! and its name taken by someone else. The name alone cannot tell those
//! apart: after a rename the old name redirects and new releases name the
//! new one, and after a deletion anyone can create a repository under the
//! old name with a workflow at the same path. Fulcio records the forge's
//! repository ID and owner ID in every GitHub Actions and GitLab CI
//! certificate ([`SourceRepository`]), and those do not change on a
//! rename. [`check`] classifies a verified release against what the
//! consumer expected, so a rename keeps working, a transfer asks, and a
//! recreated name is refused. [`crate::verify::verify_forge`] verifies a
//! bundle and runs it.

use crate::model;
use crate::sigstore::{GITHUB_ISSUER, GITLAB_ISSUER, Policy, SourceRepository};

/// A forge project name split along its forge's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForgeName<'a> {
    host: &'a str,
    /// `owner/repo` on GitHub; the whole project path on GitLab.
    repository: &'a str,
    /// The GitHub owner, or the GitLab namespace: the path without its
    /// last segment.
    owner: &'a str,
    /// The tool inside a GitHub monorepo.
    subpath: Option<&'a str>,
}

impl ForgeName<'_> {
    fn parse(project: &str) -> Option<ForgeName<'_>> {
        if let Some((host, owner, repo)) = model::repository(project) {
            let end = host.len() + 1 + owner.len() + 1 + repo.len();
            return Some(ForgeName {
                host,
                repository: project.get(host.len() + 1..end)?,
                owner,
                subpath: model::repository_subpath(project),
            });
        }
        let (host, path) = project.split_once('/')?;
        if host != "gitlab.com" || path.split('/').any(str::is_empty) {
            return None;
        }
        let (owner, _) = path.rsplit_once('/')?;
        Some(ForgeName {
            host,
            repository: path,
            owner,
            subpath: None,
        })
    }

    fn issuer(&self) -> &'static str {
        if self.host == "gitlab.com" {
            GITLAB_ISSUER
        } else {
            GITHUB_ISSUER
        }
    }

    /// The repository's URL, as Fulcio's Source Repository URI spells it.
    fn uri(&self) -> String {
        format!("https://{}/{}", self.host, self.repository)
    }

    /// What every signer identity of the repository starts with: a GitHub
    /// workflow under the repository, a GitLab pipeline config after `//`.
    fn signer_prefix(&self) -> String {
        if self.host == "gitlab.com" {
            format!("{}//", self.uri())
        } else {
            format!("{}/", self.uri())
        }
    }
}

/// What a consumer remembers about a forge project's identity once it has
/// accepted a release: the name it was last signed under and the forge's
/// IDs from that release's certificate. [`Check::pin`] gives the value to
/// store.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ForgePin {
    /// The project name the accepted release was signed under.
    pub project: String,
    /// The forge's repository ID.
    pub repository_id: String,
    /// The forge's ID of the repository's owner, when the certificate had
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
}

impl ForgePin {
    pub fn new(
        project: impl Into<String>,
        repository_id: impl Into<String>,
        owner_id: Option<String>,
    ) -> ForgePin {
        ForgePin {
            project: project.into(),
            repository_id: repository_id.into(),
            owner_id,
        }
    }
}

/// What the consumer expects of a forge project's release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Expected<'a> {
    /// The project the user asked for: `github.com/owner/repo[/tool]` or
    /// `gitlab.com/<path>`.
    pub project: &'a str,
    /// The identity remembered from a release accepted before, from local
    /// state or a lockfile.
    pub pin: Option<&'a ForgePin>,
    /// With no pin: the repository ID the forge's API gives for the
    /// requested name, following its rename redirect (`GET
    /// /repos/{owner}/{repo}` on GitHub answers for a renamed repository
    /// with the new name and the same `id`). Ignored when there is a pin.
    pub resolved_repository_id: Option<&'a str>,
    /// Accept a repository that moved to another owner. Set it only when a
    /// person has said to trust the new owner, as for a signer change.
    pub accept_transfer: bool,
}

impl<'a> Expected<'a> {
    /// Expect `project`, with nothing remembered about it.
    pub fn new(project: &'a str) -> Expected<'a> {
        Expected {
            project,
            pin: None,
            resolved_repository_id: None,
            accept_transfer: false,
        }
    }

    /// The identity remembered for the project, if any.
    pub fn pinned(self, pin: Option<&'a ForgePin>) -> Expected<'a> {
        Expected { pin, ..self }
    }

    /// The repository ID the forge resolves the requested name to, if the
    /// consumer asked it.
    pub fn resolved(self, repository_id: Option<&'a str>) -> Expected<'a> {
        Expected {
            resolved_repository_id: repository_id,
            ..self
        }
    }

    /// Whether a person has said to accept a transfer to another owner.
    pub fn accepting_transfer(self, accept: bool) -> Expected<'a> {
        Expected {
            accept_transfer: accept,
            ..self
        }
    }
}

/// Where the repository ID a release was compared with came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Evidence {
    /// The consumer's pin from an earlier release.
    Pin,
    /// The forge's answer for the requested name.
    Resolved,
}

impl std::fmt::Display for Evidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Evidence::Pin => "the pinned repository",
            Evidence::Resolved => "the repository the forge resolves the name to",
        })
    }
}

/// How an accepted release's repository relates to the one expected.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Continuity {
    /// Signed under the requested name, by the pinned repository when
    /// there was a pin.
    Same,
    /// The same repository under another name: renamed since the request's
    /// name was current, or, for a release older than the rename, signed
    /// under the name it had then. The owner is unchanged.
    Renamed { requested: String, signed: String },
    /// The same repository under another owner, accepted because
    /// [`Expected::accept_transfer`] was set.
    Transferred {
        requested: String,
        signed: String,
        /// The previous owner: its pinned ID, or its name when no owner ID
        /// was pinned.
        previous_owner: String,
        /// The owner that signed: its ID, or its name to match.
        owner: String,
    },
}

/// Why a verified release is not the project the consumer expected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    #[error("{0} is not a GitHub or GitLab project, so no forge identity pins it")]
    NotForge(String),
    #[error(
        "the release is for {signed}, not {requested}, and nothing shows they are the same repository"
    )]
    ProjectMismatch { requested: String, signed: String },
    #[error("{project} is signed through issuer {actual:?}, expected {expected}")]
    Issuer {
        project: String,
        expected: String,
        actual: Option<String>,
    },
    #[error("signed by {identity}, which is not a workflow of {repository}")]
    SignerOutsideRepository {
        identity: String,
        repository: String,
    },
    #[error("the certificate's source repository is {certificate}, not {repository}")]
    SourceMismatch {
        certificate: String,
        repository: String,
    },
    #[error(
        "{project} is signed from repository ID {actual}, but {evidence} is ID {expected}: the name belongs to a different repository now"
    )]
    DifferentRepository {
        project: String,
        expected: String,
        actual: String,
        evidence: Evidence,
    },
    #[error(
        "{requested} moved from owner {previous_owner} to owner {owner} as {signed}; accept the new owner explicitly to trust it"
    )]
    Transferred {
        requested: String,
        signed: String,
        previous_owner: String,
        owner: String,
    },
}

/// The outcome of a [`check`] that passed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Check {
    pub continuity: Continuity,
    /// The repository the signing certificate records, if it records one.
    pub source: Option<SourceRepository>,
    /// What to remember for the project once the release is accepted in
    /// full; none when the certificate carries no repository ID.
    pub pin: Option<ForgePin>,
    identity: String,
}

impl Check {
    /// Whether the signer is the same workflow as `previous`, a signer
    /// accepted before (a certificate identity, with or without its ref).
    /// Workflows are compared by their path inside the repository, since
    /// the check established that the repository is the same one whatever
    /// it is called now: `https://github.com/old/tool/.github/workflows/release.yml`
    /// continues as `https://github.com/new/tool/.github/workflows/release.yml`.
    pub fn continues_signer(&self, previous: &str) -> bool {
        match (workflow_path(previous), workflow_path(&self.identity)) {
            (Some(before), Some(now)) => before == now,
            _ => without_ref(previous) == without_ref(&self.identity),
        }
    }
}

/// A certificate identity without the ref after its last `@`.
fn without_ref(identity: &str) -> &str {
    identity.rsplit_once('@').map_or(identity, |(path, _)| path)
}

/// The forge and the path of a workflow or pipeline config inside its
/// repository, without the ref.
fn workflow_path(identity: &str) -> Option<(&str, &str)> {
    let rest = without_ref(identity.strip_prefix("https://")?);
    if let Some(path) = rest.strip_prefix("github.com/") {
        let mut parts = path.splitn(3, '/');
        let (_owner, _repo, file) = (parts.next()?, parts.next()?, parts.next()?);
        return Some(("github.com", file));
    }
    if let Some(path) = rest.strip_prefix("gitlab.com/") {
        let (_project, file) = path.split_once("//")?;
        return Some(("gitlab.com", file));
    }
    None
}

/// The signer policy to verify a forge project's bundle with, given the
/// project the (not yet verified) statement names. A statement for another
/// repository on the same forge, with the same monorepo subpath, is
/// verified under its own repository's policy so that [`check`] can then
/// decide whether it is the expected repository renamed; anything else
/// under the requested project's. None when the requested project is not
/// on a forge. The policy alone accepts any repository that signs for the
/// name it claims: never use it without [`check`].
pub fn policy(requested: &str, signed_project: &str) -> Option<Policy> {
    let want = ForgeName::parse(requested)?;
    let target = match ForgeName::parse(signed_project) {
        Some(got) if got.host == want.host && got.subpath == want.subpath => signed_project,
        _ => requested,
    };
    Policy::for_project(target)
}

/// Classify a verified release of a forge project against what the
/// consumer expected. `signed_project` is the verified statement's
/// `project`, `identity` and `issuer` the certificate's, and `source`
/// what the certificate records about the repository
/// ([`crate::sigstore::source_repository`]).
///
/// The release is refused when its signer is not a workflow of the
/// repository its statement names, or when the certificate's IDs describe
/// another repository. Otherwise, with a pin or a resolved repository ID
/// to compare the certificate's with:
///
/// - a different repository ID is a different repository, refused even
///   under the requested name, since that is what a recreated name looks
///   like;
/// - the same repository ID under another owner is a transfer, refused
///   unless [`Expected::accept_transfer`] is set. The owner is compared by
///   its ID when one was pinned, and by name otherwise;
/// - the same repository ID and owner is [`Continuity::Same`] under the
///   requested name and [`Continuity::Renamed`] under another. A monorepo
///   tool's subpath must be the same under both names.
///
/// Without IDs to compare, on either side, the name is all there is: the
/// release must be for the requested project.
pub fn check(
    expected: &Expected<'_>,
    signed_project: &str,
    identity: &str,
    issuer: Option<&str>,
    source: Option<&SourceRepository>,
) -> Result<Check, IdentityError> {
    let requested = expected.project;
    let want = ForgeName::parse(requested)
        .ok_or_else(|| IdentityError::NotForge(requested.to_string()))?;
    let mismatch = || IdentityError::ProjectMismatch {
        requested: requested.to_string(),
        signed: signed_project.to_string(),
    };
    let got = ForgeName::parse(signed_project).ok_or_else(mismatch)?;
    if got.host != want.host || got.subpath != want.subpath {
        return Err(mismatch());
    }
    if issuer != Some(want.issuer()) {
        return Err(IdentityError::Issuer {
            project: signed_project.to_string(),
            expected: want.issuer().to_string(),
            actual: issuer.map(str::to_string),
        });
    }
    let repository = got.uri();
    if !identity.starts_with(&got.signer_prefix()) {
        return Err(IdentityError::SignerOutsideRepository {
            identity: identity.to_string(),
            repository,
        });
    }
    // The IDs describe the repository the run was based on; they say
    // something about this project only if that is the one it names.
    if let Some(source) = source
        && source.uri != repository
    {
        return Err(IdentityError::SourceMismatch {
            certificate: source.uri.clone(),
            repository,
        });
    }

    let actual_id = source.and_then(|s| s.id.as_deref());
    let actual_owner_id = source.and_then(|s| s.owner_id.as_deref());
    let known = match (expected.pin, expected.resolved_repository_id) {
        (Some(pin), _) => Some((pin.repository_id.as_str(), Evidence::Pin)),
        (None, Some(id)) => Some((id, Evidence::Resolved)),
        (None, None) => None,
    };
    let same_name = signed_project == requested;
    let continuity = match (known, actual_id) {
        (Some((expected_id, evidence)), Some(actual)) if expected_id != actual => {
            return Err(IdentityError::DifferentRepository {
                project: signed_project.to_string(),
                expected: expected_id.to_string(),
                actual: actual.to_string(),
                evidence,
            });
        }
        (Some(_), Some(_)) => {
            let pinned_owner_id = expected.pin.and_then(|p| p.owner_id.as_deref());
            let moved = match (pinned_owner_id, actual_owner_id) {
                (Some(before), Some(now)) => {
                    (before != now).then(|| (before.to_string(), now.to_string()))
                }
                // No owner ID to compare: the owner the consumer last saw,
                // by name. GitHub and GitLab owner names are
                // case-insensitive.
                _ => {
                    let before = expected
                        .pin
                        .and_then(|p| ForgeName::parse(&p.project))
                        .filter(|p| p.host == want.host)
                        .map_or(want.owner, |p| p.owner);
                    (!before.eq_ignore_ascii_case(got.owner))
                        .then(|| (before.to_string(), got.owner.to_string()))
                }
            };
            match moved {
                Some((previous_owner, owner)) if expected.accept_transfer => {
                    Continuity::Transferred {
                        requested: requested.to_string(),
                        signed: signed_project.to_string(),
                        previous_owner,
                        owner,
                    }
                }
                Some((previous_owner, owner)) => {
                    return Err(IdentityError::Transferred {
                        requested: requested.to_string(),
                        signed: signed_project.to_string(),
                        previous_owner,
                        owner,
                    });
                }
                None if same_name => Continuity::Same,
                None => Continuity::Renamed {
                    requested: requested.to_string(),
                    signed: signed_project.to_string(),
                },
            }
        }
        _ if same_name => Continuity::Same,
        _ => return Err(mismatch()),
    };
    Ok(Check {
        continuity,
        source: source.cloned(),
        pin: actual_id
            .map(|id| ForgePin::new(signed_project, id, actual_owner_id.map(str::to_string))),
        identity: identity.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HK_WORKFLOW: &str =
        "https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0";

    fn source(uri: &str, id: &str, owner_uri: &str, owner_id: &str) -> SourceRepository {
        SourceRepository {
            uri: uri.into(),
            id: Some(id.into()),
            owner_uri: Some(owner_uri.into()),
            owner_id: Some(owner_id.into()),
        }
    }

    fn hk() -> SourceRepository {
        source(
            "https://github.com/jdx/hk",
            "922514152",
            "https://github.com/jdx",
            "216188",
        )
    }

    /// `jdx/hk` renamed to `jdx/hook`, same owner.
    fn renamed() -> SourceRepository {
        source(
            "https://github.com/jdx/hook",
            "922514152",
            "https://github.com/jdx",
            "216188",
        )
    }

    /// `jdx/hk` transferred to `acme/hk`.
    fn transferred() -> SourceRepository {
        source(
            "https://github.com/acme/hk",
            "922514152",
            "https://github.com/acme",
            "999",
        )
    }

    /// A new repository under the old name.
    fn squatted() -> SourceRepository {
        source(
            "https://github.com/jdx/hk",
            "555",
            "https://github.com/jdx",
            "216188",
        )
    }

    fn pin() -> ForgePin {
        ForgePin::new("github.com/jdx/hk", "922514152", Some("216188".into()))
    }

    fn run(
        expected: &Expected<'_>,
        project: &str,
        source: Option<&SourceRepository>,
    ) -> Result<Check, IdentityError> {
        let identity = match source {
            Some(s) => format!("{}/.github/workflows/release.yml@refs/tags/v1", s.uri),
            None => format!(
                "https://{}/.github/workflows/release.yml@refs/tags/v1",
                project
            ),
        };
        check(expected, project, &identity, Some(GITHUB_ISSUER), source)
    }

    #[test]
    fn same_name_and_repository_is_the_same() {
        let pin = pin();
        for expected in [
            Expected::new("github.com/jdx/hk"),
            Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            Expected::new("github.com/jdx/hk").resolved(Some("922514152")),
        ] {
            let ok = run(&expected, "github.com/jdx/hk", Some(&hk())).unwrap();
            assert_eq!(ok.continuity, Continuity::Same);
            assert_eq!(ok.pin, Some(pin.clone()));
            assert_eq!(ok.source, Some(hk()));
        }
        // A certificate without the extensions: the name is all there is.
        let ok = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            "github.com/jdx/hk",
            None,
        )
        .unwrap();
        assert_eq!(ok.continuity, Continuity::Same);
        assert_eq!(ok.pin, None);
    }

    #[test]
    fn a_rename_is_followed_by_repository_id() {
        let pin = pin();
        for expected in [
            Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            Expected::new("github.com/jdx/hk").resolved(Some("922514152")),
        ] {
            let ok = run(&expected, "github.com/jdx/hook", Some(&renamed())).unwrap();
            assert_eq!(
                ok.continuity,
                Continuity::Renamed {
                    requested: "github.com/jdx/hk".into(),
                    signed: "github.com/jdx/hook".into(),
                }
            );
            assert_eq!(
                ok.pin,
                Some(ForgePin::new(
                    "github.com/jdx/hook",
                    "922514152",
                    Some("216188".into())
                ))
            );
        }
        // An owner renamed along with it keeps its ID, so a pin follows it.
        let moved_owner = source(
            "https://github.com/jdx2/hook",
            "922514152",
            "https://github.com/jdx2",
            "216188",
        );
        let ok = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            "github.com/jdx2/hook",
            Some(&moved_owner),
        )
        .unwrap();
        assert!(matches!(ok.continuity, Continuity::Renamed { .. }));
    }

    #[test]
    fn old_releases_under_the_old_name_still_verify() {
        // The user now asks for the new name; a release from before the
        // rename names the old one and was signed there.
        let pin = ForgePin::new("github.com/jdx/hook", "922514152", Some("216188".into()));
        let ok = run(
            &Expected::new("github.com/jdx/hook").pinned(Some(&pin)),
            "github.com/jdx/hk",
            Some(&hk()),
        )
        .unwrap();
        assert_eq!(
            ok.continuity,
            Continuity::Renamed {
                requested: "github.com/jdx/hook".into(),
                signed: "github.com/jdx/hk".into(),
            }
        );
    }

    #[test]
    fn a_transfer_needs_explicit_trust() {
        let pin = pin();
        let expected = Expected::new("github.com/jdx/hk").pinned(Some(&pin));
        let err = run(&expected, "github.com/acme/hk", Some(&transferred())).unwrap_err();
        assert_eq!(
            err,
            IdentityError::Transferred {
                requested: "github.com/jdx/hk".into(),
                signed: "github.com/acme/hk".into(),
                previous_owner: "216188".into(),
                owner: "999".into(),
            }
        );
        let ok = run(
            &expected.accepting_transfer(true),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap();
        assert!(matches!(ok.continuity, Continuity::Transferred { .. }));
        assert_eq!(
            ok.pin.unwrap().owner_id.as_deref(),
            Some("999"),
            "the pin moves to the new owner"
        );

        // With no pinned owner ID, the owner is compared by name: the
        // forge's redirect alone does not show the new owner is the old.
        let err = run(
            &Expected::new("github.com/jdx/hk").resolved(Some("922514152")),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, IdentityError::Transferred { previous_owner, owner, .. }
                if previous_owner == "jdx" && owner == "acme"),
            "{err}"
        );
        let old_pin = ForgePin::new("github.com/jdx/hk", "922514152", None);
        let err = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&old_pin)),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap_err();
        assert!(matches!(err, IdentityError::Transferred { .. }), "{err}");
        // A repository renamed within an owner whose name only differs in
        // case is not a transfer.
        let ok = run(
            &Expected::new("github.com/JDX/hk").resolved(Some("922514152")),
            "github.com/jdx/hook",
            Some(&renamed()),
        )
        .unwrap();
        assert!(matches!(ok.continuity, Continuity::Renamed { .. }));
    }

    #[test]
    fn a_recreated_name_is_a_different_repository() {
        let pin = pin();
        let err = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            "github.com/jdx/hk",
            Some(&squatted()),
        )
        .unwrap_err();
        assert_eq!(
            err,
            IdentityError::DifferentRepository {
                project: "github.com/jdx/hk".into(),
                expected: "922514152".into(),
                actual: "555".into(),
                evidence: Evidence::Pin,
            }
        );
        // The pin wins over whatever the forge now resolves the name to.
        let err = run(
            &Expected::new("github.com/jdx/hk")
                .pinned(Some(&pin))
                .resolved(Some("555")),
            "github.com/jdx/hk",
            Some(&squatted()),
        )
        .unwrap_err();
        assert!(
            matches!(err, IdentityError::DifferentRepository { .. }),
            "{err}"
        );
        let err = run(
            &Expected::new("github.com/jdx/hk").resolved(Some("922514152")),
            "github.com/other/hk",
            Some(&source(
                "https://github.com/other/hk",
                "777",
                "https://github.com/other",
                "1",
            )),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                IdentityError::DifferentRepository {
                    evidence: Evidence::Resolved,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn another_name_without_id_evidence_is_a_mismatch() {
        let err = run(
            &Expected::new("github.com/jdx/hk"),
            "github.com/jdx/hook",
            Some(&renamed()),
        )
        .unwrap_err();
        assert_eq!(
            err,
            IdentityError::ProjectMismatch {
                requested: "github.com/jdx/hk".into(),
                signed: "github.com/jdx/hook".into(),
            }
        );
        // A pin, but a certificate that carries no IDs to compare.
        let pin = pin();
        let err = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            "github.com/jdx/hook",
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, IdentityError::ProjectMismatch { .. }),
            "{err}"
        );
    }

    #[test]
    fn a_monorepo_subpath_must_survive_the_rename() {
        let pin = pin();
        let expected = Expected::new("github.com/jdx/hk/tool").pinned(Some(&pin));
        let ok = run(&expected, "github.com/jdx/hook/tool", Some(&renamed())).unwrap();
        assert!(matches!(ok.continuity, Continuity::Renamed { .. }));
        for other in ["github.com/jdx/hook/other", "github.com/jdx/hook"] {
            let err = run(&expected, other, Some(&renamed())).unwrap_err();
            assert!(
                matches!(err, IdentityError::ProjectMismatch { .. }),
                "{err}"
            );
        }
    }

    #[test]
    fn the_signer_and_certificate_must_belong_to_the_named_repository() {
        let pin = pin();
        let expected = Expected::new("github.com/jdx/hk").pinned(Some(&pin));
        // A workflow of another repository.
        let err = check(
            &expected,
            "github.com/jdx/hk",
            "https://github.com/evil/x/.github/workflows/r.yml@refs/tags/v1",
            Some(GITHUB_ISSUER),
            Some(&hk()),
        )
        .unwrap_err();
        assert!(
            matches!(err, IdentityError::SignerOutsideRepository { .. }),
            "{err}"
        );
        // IDs from a run based on another repository.
        let err = check(
            &expected,
            "github.com/jdx/hk",
            HK_WORKFLOW,
            Some(GITHUB_ISSUER),
            Some(&renamed()),
        )
        .unwrap_err();
        assert!(matches!(err, IdentityError::SourceMismatch { .. }), "{err}");
        // The forge's own issuer only.
        let err = check(
            &expected,
            "github.com/jdx/hk",
            HK_WORKFLOW,
            Some(GITLAB_ISSUER),
            Some(&hk()),
        )
        .unwrap_err();
        assert!(matches!(err, IdentityError::Issuer { .. }), "{err}");
        let err = check(&expected, "github.com/jdx/hk", HK_WORKFLOW, None, None).unwrap_err();
        assert!(matches!(err, IdentityError::Issuer { .. }), "{err}");
        // Another forge, or no forge.
        assert!(matches!(
            run(&expected, "gitlab.com/jdx/hk", None).unwrap_err(),
            IdentityError::ProjectMismatch { .. }
        ));
        assert_eq!(
            run(&Expected::new("mise.jdx.dev"), "mise.jdx.dev", None).unwrap_err(),
            IdentityError::NotForge("mise.jdx.dev".into())
        );
    }

    #[test]
    fn gitlab_projects_are_pinned_by_project_id() {
        let old = source(
            "https://gitlab.com/group/sub/tool",
            "42",
            "https://gitlab.com/group/sub",
            "7",
        );
        let new = source(
            "https://gitlab.com/group/sub/tool2",
            "42",
            "https://gitlab.com/group/sub",
            "7",
        );
        let pin = ForgePin::new("gitlab.com/group/sub/tool", "42", Some("7".into()));
        let expected = Expected::new("gitlab.com/group/sub/tool").pinned(Some(&pin));
        let signer = |uri: &str| format!("{uri}//.gitlab-ci.yml@refs/tags/v1");
        let ok = check(
            &expected,
            "gitlab.com/group/sub/tool",
            &signer(&old.uri),
            Some(GITLAB_ISSUER),
            Some(&old),
        )
        .unwrap();
        assert_eq!(ok.continuity, Continuity::Same);
        let ok = check(
            &expected,
            "gitlab.com/group/sub/tool2",
            &signer(&new.uri),
            Some(GITLAB_ISSUER),
            Some(&new),
        )
        .unwrap();
        assert!(matches!(ok.continuity, Continuity::Renamed { .. }));
        assert!(ok.continues_signer("https://gitlab.com/group/sub/tool//.gitlab-ci.yml"));
        // A deeper project is another project, not a pipeline of this one.
        let err = check(
            &expected,
            "gitlab.com/group/sub/tool",
            "https://gitlab.com/group/sub/tool/x//.gitlab-ci.yml@refs/tags/v1",
            Some(GITLAB_ISSUER),
            Some(&old),
        )
        .unwrap_err();
        assert!(
            matches!(err, IdentityError::SignerOutsideRepository { .. }),
            "{err}"
        );
    }

    #[test]
    fn signer_continuity_follows_the_repository() {
        let pin = pin();
        let ok = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
            "github.com/jdx/hook",
            Some(&renamed()),
        )
        .unwrap();
        assert!(ok.continues_signer("https://github.com/jdx/hk/.github/workflows/release.yml"));
        assert!(ok.continues_signer(
            "https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v0.1.0"
        ));
        assert!(!ok.continues_signer("https://github.com/jdx/hk/.github/workflows/other.yml"));
        assert!(!ok.continues_signer("https://gitlab.com/jdx/hk//.github/workflows/release.yml"));
        assert!(!ok.continues_signer("me@example.com"));
    }

    #[test]
    fn the_verification_policy_follows_the_signed_name_on_the_same_forge() {
        let renamed = policy("github.com/jdx/hk", "github.com/jdx/hook").unwrap();
        assert_eq!(
            renamed.identity_prefix.as_deref(),
            Some("https://github.com/jdx/hook/")
        );
        assert_eq!(renamed.issuer.as_deref(), Some(GITHUB_ISSUER));
        for other in [
            "github.com/jdx/hook/tool",
            "gitlab.com/jdx/hook",
            "mise.jdx.dev",
            "",
        ] {
            let p = policy("github.com/jdx/hk", other).unwrap();
            assert_eq!(
                p.identity_prefix.as_deref(),
                Some("https://github.com/jdx/hk/"),
                "{other}"
            );
        }
        assert!(policy("mise.jdx.dev", "github.com/jdx/hk").is_none());
    }
}
