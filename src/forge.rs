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
//!
//! A consumer can hold several pins for a project, such as its own and a
//! lockfile's ([`Expected::pinned_by`]); every one must hold. A transfer a
//! person accepted is remembered in the pin
//! ([`ForgePin::accepted_owner_ids`]), so releases from before it keep
//! verifying. [`same_workflow`] compares two signers the consumer stored,
//! with the pins recorded alongside them, for a no-downgrade check that has
//! no release in hand.

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
    /// Other owners a person accepted the repository from or to: after an
    /// accepted transfer, the owner it moved from. A release signed by any
    /// of them, or by [`ForgePin::owner_id`], is by an accepted owner, so
    /// releases from before the transfer keep verifying. Pins stored before
    /// this field existed read with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_owner_ids: Vec<String>,
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
            accepted_owner_ids: Vec::new(),
        }
    }

    /// The pin with these owners accepted as well: see
    /// [`ForgePin::accepted_owner_ids`].
    pub fn with_accepted_owner_ids<I, S>(self, owner_ids: I) -> ForgePin
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ForgePin {
            accepted_owner_ids: owner_ids.into_iter().map(Into::into).collect(),
            ..self
        }
    }

    /// Whether `owner_id` is the pinned owner or one accepted for the
    /// repository.
    pub fn accepts_owner(&self, owner_id: &str) -> bool {
        self.owner_id.as_deref() == Some(owner_id)
            || self.accepted_owner_ids.iter().any(|id| id == owner_id)
    }

    /// Whether this pin, recorded later, continues `previous`: the same
    /// repository ID on the same forge, under the same owner or one either
    /// pin accepted. Owners are compared by ID when both pins have one, and
    /// by name otherwise. For two pins a consumer stored, such as two
    /// lockfile entries; [`check`] does the same for a release.
    pub fn continues(&self, previous: &ForgePin) -> bool {
        let (Some(before), Some(now)) = (
            ForgeName::parse(&previous.project),
            ForgeName::parse(&self.project),
        ) else {
            return false;
        };
        if before.host != now.host || previous.repository_id != self.repository_id {
            return false;
        }
        match (&previous.owner_id, &self.owner_id) {
            (Some(was), Some(is)) => self.accepts_owner(was) || previous.accepts_owner(is),
            _ => before.owner.eq_ignore_ascii_case(now.owner),
        }
    }
}

/// Where a consumer's pin came from, so that a refusal can say which one
/// the release disagreed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PinSource {
    /// This machine's record of the releases it accepted.
    Local,
    /// A lockfile's commitment, shared with every machine that reads it.
    Lockfile,
}

impl std::fmt::Display for PinSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PinSource::Local => "this machine's pin",
            PinSource::Lockfile => "the lockfile's pin",
        })
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
    /// state or a lockfile. A consumer that holds more than one uses
    /// [`Expected::pins`] instead, or as well.
    pub pin: Option<&'a ForgePin>,
    /// Identities remembered from releases accepted before, each with where
    /// it came from. The release must satisfy every one of them, and
    /// [`pin`](Expected::pin) too when it is set.
    pub pins: &'a [(PinSource, ForgePin)],
    /// With no pin: the repository ID the forge's API gives for the
    /// requested name, following its rename redirect (`GET
    /// /repos/{owner}/{repo}` on GitHub answers for a renamed repository
    /// with the new name and the same `id`). Ignored when there is a pin.
    pub resolved_repository_id: Option<&'a str>,
    /// With no pin, alongside [`resolved_repository_id`]: the ID of the
    /// user, organization, or group that the requested project's owner name
    /// belongs to now (`GET
    /// /users/{owner}` on GitHub, `GET /namespaces/{path}` on GitLab; or
    /// the `owner.id` of `GET /repos/{owner}/{repo}` when the answer is
    /// under the requested owner, not a redirect to another). A release
    /// whose certificate carries that owner ID is by the requested owner
    /// even under an owner name that has changed since, and one with
    /// another owner ID is a transfer even under the requested owner's
    /// name. Without it, the owner is compared by name. Ignored when there
    /// is a pin.
    ///
    /// [`resolved_repository_id`]: Expected::resolved_repository_id
    pub resolved_owner_id: Option<&'a str>,
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
            pins: &[],
            resolved_repository_id: None,
            resolved_owner_id: None,
            accept_transfer: false,
        }
    }

    /// The identity remembered for the project, if any.
    pub fn pinned(self, pin: Option<&'a ForgePin>) -> Expected<'a> {
        Expected { pin, ..self }
    }

    /// Every identity remembered for the project, each labelled with where
    /// it came from: this machine's pin store, a lockfile. Each must hold,
    /// and a refusal names the one that did not ([`Evidence::from`] a
    /// [`PinSource`]).
    pub fn pinned_by(self, pins: &'a [(PinSource, ForgePin)]) -> Expected<'a> {
        Expected { pins, ..self }
    }

    /// The repository ID the forge resolves the requested name to, if the
    /// consumer asked it.
    pub fn resolved(self, repository_id: Option<&'a str>) -> Expected<'a> {
        Expected {
            resolved_repository_id: repository_id,
            ..self
        }
    }

    /// The owner ID the forge gives for the requested project's owner
    /// name, if the consumer asked it: see [`Expected::resolved_owner_id`].
    pub fn resolved_owner(self, owner_id: Option<&'a str>) -> Expected<'a> {
        Expected {
            resolved_owner_id: owner_id,
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

    /// Every pin with the evidence a refusal cites for it: [`pin`] first,
    /// then [`pins`] in order.
    ///
    /// [`pin`]: Expected::pin
    /// [`pins`]: Expected::pins
    fn all_pins(&self) -> Vec<(&'a ForgePin, Evidence)> {
        self.pin
            .map(|pin| (pin, Evidence::Pin))
            .into_iter()
            .chain(
                self.pins
                    .iter()
                    .map(|(source, pin)| (pin, Evidence::from(*source))),
            )
            .collect()
    }
}

/// Where the identity a release was compared with came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Evidence {
    /// The consumer's pin from an earlier release, [`Expected::pin`].
    Pin,
    /// The forge's answer for the requested name.
    Resolved,
    /// This machine's pin, from [`Expected::pins`].
    LocalPin,
    /// A lockfile's pin, from [`Expected::pins`].
    LockfilePin,
}

impl Evidence {
    /// Which pin of [`Expected::pins`] this is, if it is one.
    pub fn pin_source(self) -> Option<PinSource> {
        match self {
            Evidence::LocalPin => Some(PinSource::Local),
            Evidence::LockfilePin => Some(PinSource::Lockfile),
            Evidence::Pin | Evidence::Resolved => None,
        }
    }
}

impl From<PinSource> for Evidence {
    fn from(source: PinSource) -> Evidence {
        match source {
            PinSource::Local => Evidence::LocalPin,
            PinSource::Lockfile => Evidence::LockfilePin,
        }
    }
}

impl std::fmt::Display for Evidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Evidence::Pin => "the pinned repository",
            Evidence::Resolved => "the repository the forge resolves the name to",
            Evidence::LocalPin => "the repository this machine pinned",
            Evidence::LockfilePin => "the repository the lockfile pins",
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
    /// under the name it had then. The owner is unchanged, or one the pin
    /// accepted.
    Renamed { requested: String, signed: String },
    /// The same repository under another owner, accepted because
    /// [`Expected::accept_transfer`] was set. [`Check::transfer`] has both
    /// owners' names and IDs.
    Transferred {
        requested: String,
        signed: String,
        /// The previous owner: its pinned or resolved ID, or its name when
        /// there was no owner ID to compare.
        previous_owner: String,
        /// The owner that signed: its ID, or its name to match.
        owner: String,
    },
}

/// A repository that moved to another owner: who owned it before, as the
/// consumer knew it, and who signed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct Transfer {
    /// The project the consumer asked for.
    pub requested: String,
    /// The project the release was signed under.
    pub signed: String,
    /// The previous owner's name: the owner of the project the pin that
    /// disagreed was recorded under, or of the requested project.
    pub previous_owner: String,
    /// The previous owner's ID, when a pin or the forge gave one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_owner_id: Option<String>,
    /// The name of the owner that signed, from the certificate's Source
    /// Repository Owner URI.
    pub owner: String,
    /// The ID of the owner that signed, when the certificate records one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// What the previous owner was taken from: a pin, or the forge's answer
    /// for the requested owner. None when it is the requested project's
    /// owner by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
}

impl std::fmt::Display for Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let owner = |name: &str, id: &Option<String>| match id {
            Some(id) => format!("{name} (ID {id})"),
            None => name.to_string(),
        };
        write!(
            f,
            "{} moved from owner {} to owner {} as {}",
            self.requested,
            owner(&self.previous_owner, &self.previous_owner_id),
            owner(&self.owner, &self.owner_id),
            self.signed,
        )
    }
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
    /// Not returned since 1.5.0, which reports a transfer as
    /// [`IdentityError::OwnerChanged`] with both owners' names and the pin
    /// it disagreed with.
    #[deprecated(
        since = "1.5.0",
        note = "not returned any more: a transfer is IdentityError::OwnerChanged"
    )]
    #[error(
        "{requested} moved from owner {previous_owner} to owner {owner} as {signed}; accept the new owner explicitly to trust it"
    )]
    Transferred {
        requested: String,
        signed: String,
        previous_owner: String,
        owner: String,
    },
    /// The same repository under an owner the consumer has not accepted.
    #[error("{0}; accept the new owner explicitly to trust it")]
    OwnerChanged(Box<Transfer>),
}

/// The outcome of a [`check`] that passed.
///
/// A consumer's tests can make one without a real bundle, since [`check`]
/// only compares what it is given:
///
/// ```
/// use packslip::forge::{self, Continuity, Expected, ForgePin};
/// use packslip::sigstore::{GITHUB_ISSUER, SourceRepository};
///
/// let source = SourceRepository::new("https://github.com/jdx/hook")
///     .with_id("922514152")
///     .with_owner("https://github.com/jdx", "216188");
/// let pin = ForgePin::new("github.com/jdx/hk", "922514152", Some("216188".into()));
/// let check = forge::check(
///     &Expected::new("github.com/jdx/hk").pinned(Some(&pin)),
///     "github.com/jdx/hook",
///     "https://github.com/jdx/hook/.github/workflows/release.yml@refs/tags/v3.0.0",
///     Some(GITHUB_ISSUER),
///     Some(&source),
/// )
/// .unwrap();
/// assert!(matches!(check.continuity, Continuity::Renamed { .. }));
/// assert!(check.continues_signer("https://github.com/jdx/hk/.github/workflows/release.yml"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Check {
    pub continuity: Continuity,
    /// The repository the signing certificate records, if it records one.
    pub source: Option<SourceRepository>,
    /// What to remember for the project once the release is accepted in
    /// full; none when the certificate carries no repository ID. It keeps
    /// the owners the consumer's pins accepted, and after an accepted
    /// transfer the owner the repository moved from, in
    /// [`ForgePin::accepted_owner_ids`].
    pub pin: Option<ForgePin>,
    identity: String,
    /// Whether the forge's repository ID, not the name alone, showed the
    /// repository is the one expected.
    by_id: bool,
    transfer: Option<Transfer>,
}

impl Check {
    /// Whether the signer is the same workflow as `previous`, a signer
    /// accepted before for the expected project (a certificate identity,
    /// with or without its ref). When the check matched the repository by
    /// its ID, workflows are compared by their path inside the repository,
    /// whatever it is called now:
    /// `https://github.com/old/tool/.github/workflows/release.yml`
    /// continues as `https://github.com/new/tool/.github/workflows/release.yml`.
    /// When the name was all there was to go on, the identity must be the
    /// same apart from its ref. [`same_workflow`] compares two signers a
    /// consumer stored.
    pub fn continues_signer(&self, previous: &str) -> bool {
        let pin = self.pin.as_ref().filter(|_| self.by_id);
        same_workflow(previous, pin, &self.identity, pin)
    }

    /// The owners of an accepted transfer, when [`Check::continuity`] is
    /// [`Continuity::Transferred`].
    pub fn transfer(&self) -> Option<&Transfer> {
        self.transfer.as_ref()
    }
}

/// Whether `current` is the same signer as `previous`, two keyless signers
/// a consumer recorded for a project (certificate identities, with or
/// without their ref), each with the forge pin recorded alongside it, if
/// any. Two lockfile entries, or a lockfile entry and a pin store's, are
/// what this compares; [`Check::continues_signer`] compares a verified
/// release's signer.
///
/// With both pins, the signers are the same when `current_pin`
/// [continues](ForgePin::continues) `previous_pin` (the same repository ID
/// and an accepted owner) and the workflow has the same path inside the
/// repository, so a rename keeps the signer. A changed repository ID is a
/// different signer even under the same identity: that is what a recreated
/// name looks like. Without both pins, the identities must be the same
/// apart from a workflow's ref.
pub fn same_workflow(
    previous: &str,
    previous_pin: Option<&ForgePin>,
    current: &str,
    current_pin: Option<&ForgePin>,
) -> bool {
    match (previous_pin, current_pin) {
        (Some(before), Some(now)) => {
            now.continues(before)
                && match (workflow_path(previous), workflow_path(current)) {
                    (Some(before), Some(now)) => before == now,
                    _ => without_ref(previous) == without_ref(current),
                }
        }
        _ => without_ref(previous) == without_ref(current),
    }
}

/// A workflow identity without the ref after its last `@`. Only a URL
/// identity has a ref; any other (an email) can contain `@` itself.
fn without_ref(identity: &str) -> &str {
    if !identity.starts_with("https://") {
        return identity;
    }
    identity.rsplit_once('@').map_or(identity, |(path, _)| path)
}

/// The forge and the path of a workflow or pipeline config inside its
/// repository, without the ref.
fn workflow_path(identity: &str) -> Option<(&str, &str)> {
    let rest = without_ref(identity).strip_prefix("https://")?;
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
/// another repository. Otherwise, with pins or a resolved repository ID
/// to compare the certificate's with:
///
/// - a different repository ID from any of them is a different
///   repository, refused even under the requested name, since that is what
///   a recreated name looks like;
/// - the same repository ID under an owner a pin has not accepted is a
///   transfer, refused unless [`Expected::accept_transfer`] is set. Each
///   pin's owner is compared by ID when it has one, and by name otherwise;
///   with no pin, the owner is compared with [`Expected::resolved_owner_id`]
///   when it is given, and with the requested project's owner by name
///   otherwise;
/// - the same repository ID and an accepted owner is [`Continuity::Same`]
///   under the requested name and [`Continuity::Renamed`] under another. A
///   monorepo tool's subpath must be the same under both names.
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
    let pins = expected.all_pins();
    let same_name = signed_project == requested;
    let by_id =
        actual_id.is_some() && (!pins.is_empty() || expected.resolved_repository_id.is_some());
    let mut transfer = None;
    let continuity = match actual_id {
        Some(actual) if by_id => {
            let known: Vec<(&str, Evidence)> = if pins.is_empty() {
                expected
                    .resolved_repository_id
                    .map(|id| (id, Evidence::Resolved))
                    .into_iter()
                    .collect()
            } else {
                pins.iter()
                    .map(|(pin, evidence)| (pin.repository_id.as_str(), *evidence))
                    .collect()
            };
            if let Some((expected_id, evidence)) = known.iter().find(|(id, _)| *id != actual) {
                return Err(IdentityError::DifferentRepository {
                    project: signed_project.to_string(),
                    expected: expected_id.to_string(),
                    actual: actual.to_string(),
                    evidence: *evidence,
                });
            }
            let owner = source
                .and_then(|s| s.owner_uri.as_deref())
                .and_then(|uri| uri.strip_prefix(&format!("https://{}/", got.host)))
                .filter(|name| !name.is_empty())
                .unwrap_or(got.owner);
            let moved = |previous_owner: &str,
                         previous_owner_id: Option<&str>,
                         evidence: Option<Evidence>| Transfer {
                requested: requested.to_string(),
                signed: signed_project.to_string(),
                previous_owner: previous_owner.to_string(),
                previous_owner_id: previous_owner_id.map(str::to_string),
                owner: owner.to_string(),
                owner_id: actual_owner_id.map(str::to_string),
                evidence,
            };
            // GitHub and GitLab owner names are case-insensitive.
            let another_owner_name = |before: &str| !before.eq_ignore_ascii_case(got.owner);
            let found = if pins.is_empty() {
                match (expected.resolved_owner_id, actual_owner_id) {
                    (Some(before), Some(now)) => (before != now)
                        .then(|| moved(want.owner, Some(before), Some(Evidence::Resolved))),
                    _ => another_owner_name(want.owner).then(|| moved(want.owner, None, None)),
                }
            } else {
                pins.iter().find_map(|(pin, evidence)| {
                    // The owner the consumer last saw under this pin, by name.
                    let before = ForgeName::parse(&pin.project)
                        .filter(|p| p.host == want.host)
                        .map_or(want.owner, |p| p.owner);
                    let has_owner_ids =
                        pin.owner_id.is_some() || !pin.accepted_owner_ids.is_empty();
                    let differs = match actual_owner_id {
                        Some(now) if has_owner_ids => !pin.accepts_owner(now),
                        _ => another_owner_name(before),
                    };
                    differs.then(|| moved(before, pin.owner_id.as_deref(), Some(*evidence)))
                })
            };
            match found {
                Some(found) if expected.accept_transfer => {
                    // As 1.4 reported it: both IDs, or else both names.
                    let (previous_owner, owner) = match (&found.previous_owner_id, &found.owner_id)
                    {
                        (Some(before), Some(now)) => (before.clone(), now.clone()),
                        _ => (found.previous_owner.clone(), found.owner.clone()),
                    };
                    transfer = Some(found);
                    Continuity::Transferred {
                        requested: requested.to_string(),
                        signed: signed_project.to_string(),
                        previous_owner,
                        owner,
                    }
                }
                Some(found) => return Err(IdentityError::OwnerChanged(Box::new(found))),
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
    let pin = actual_id.map(|id| {
        // Every owner accepted so far stays accepted: the pins' owners and
        // the ones they accepted, and the owner an accepted transfer moved
        // the repository from.
        let mut accepted: Vec<String> = Vec::new();
        let previous = pins
            .iter()
            .filter(|_| by_id)
            .flat_map(|(pin, _)| pin.owner_id.iter().chain(&pin.accepted_owner_ids))
            .chain(transfer.as_ref().and_then(|t| t.previous_owner_id.as_ref()));
        for owner in previous {
            if Some(owner.as_str()) != actual_owner_id && !accepted.contains(owner) {
                accepted.push(owner.clone());
            }
        }
        ForgePin::new(signed_project, id, actual_owner_id.map(str::to_string))
            .with_accepted_owner_ids(accepted)
    });
    Ok(Check {
        continuity,
        source: source.cloned(),
        pin,
        identity: identity.to_string(),
        by_id,
        transfer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HK_WORKFLOW: &str =
        "https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0";

    fn source(uri: &str, id: &str, owner_uri: &str, owner_id: &str) -> SourceRepository {
        SourceRepository::new(uri)
            .with_id(id)
            .with_owner(owner_uri, owner_id)
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
        let transfer = Transfer {
            requested: "github.com/jdx/hk".into(),
            signed: "github.com/acme/hk".into(),
            previous_owner: "jdx".into(),
            previous_owner_id: Some("216188".into()),
            owner: "acme".into(),
            owner_id: Some("999".into()),
            evidence: Some(Evidence::Pin),
        };
        assert_eq!(err, IdentityError::OwnerChanged(Box::new(transfer.clone())));
        assert_eq!(
            err.to_string(),
            "github.com/jdx/hk moved from owner jdx (ID 216188) to owner acme (ID 999) as \
             github.com/acme/hk; accept the new owner explicitly to trust it"
        );
        let ok = run(
            &expected.accepting_transfer(true),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap();
        assert_eq!(
            ok.continuity,
            Continuity::Transferred {
                requested: "github.com/jdx/hk".into(),
                signed: "github.com/acme/hk".into(),
                previous_owner: "216188".into(),
                owner: "999".into(),
            }
        );
        assert_eq!(ok.transfer(), Some(&transfer));
        assert_eq!(
            ok.pin,
            Some(
                ForgePin::new("github.com/acme/hk", "922514152", Some("999".into()))
                    .with_accepted_owner_ids(["216188"])
            ),
            "the pin moves to the new owner and keeps the old one accepted"
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
            matches!(&err, IdentityError::OwnerChanged(t)
                if t.previous_owner == "jdx" && t.previous_owner_id.is_none()
                    && t.owner == "acme" && t.evidence.is_none()),
            "{err}"
        );
        let old_pin = ForgePin::new("github.com/jdx/hk", "922514152", None);
        let err = run(
            &Expected::new("github.com/jdx/hk").pinned(Some(&old_pin)),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, IdentityError::OwnerChanged(t) if t.evidence == Some(Evidence::Pin)),
            "{err}"
        );
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

    #[test]
    fn every_pin_must_hold_and_a_refusal_names_the_one_that_did_not() {
        let other = ForgePin::new("github.com/jdx/hk", "555", Some("216188".into()));
        let pins = [
            (PinSource::Local, pin()),
            (PinSource::Lockfile, other.clone()),
        ];
        let err = run(
            &Expected::new("github.com/jdx/hk").pinned_by(&pins),
            "github.com/jdx/hk",
            Some(&hk()),
        )
        .unwrap_err();
        assert_eq!(
            err,
            IdentityError::DifferentRepository {
                project: "github.com/jdx/hk".into(),
                expected: "555".into(),
                actual: "922514152".into(),
                evidence: Evidence::LockfilePin,
            }
        );
        assert!(
            err.to_string()
                .contains("the repository the lockfile pins is ID 555"),
            "{err}"
        );
        // The single pin of 1.4 is checked too, first.
        let good = [(PinSource::Lockfile, pin())];
        let err = run(
            &Expected::new("github.com/jdx/hk")
                .pinned(Some(&other))
                .pinned_by(&good),
            "github.com/jdx/hk",
            Some(&hk()),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                IdentityError::DifferentRepository {
                    evidence: Evidence::Pin,
                    ..
                }
            ),
            "{err}"
        );

        // Pins that agree, recorded under the names before and after a
        // rename, follow it; the forge's answer is not needed.
        let pins = [
            (PinSource::Local, pin()),
            (
                PinSource::Lockfile,
                ForgePin::new("github.com/jdx/hook", "922514152", Some("216188".into())),
            ),
        ];
        let expected = Expected::new("github.com/jdx/hk")
            .pinned_by(&pins)
            .resolved(Some("555"));
        let ok = run(&expected, "github.com/jdx/hook", Some(&renamed())).unwrap();
        assert!(matches!(ok.continuity, Continuity::Renamed { .. }));
        assert!(ok.continues_signer("https://github.com/jdx/hk/.github/workflows/release.yml"));

        // A transfer only this machine accepted is still one to the lockfile.
        let pins = [
            (PinSource::Local, pin().with_accepted_owner_ids(["999"])),
            (PinSource::Lockfile, pin()),
        ];
        let err = run(
            &Expected::new("github.com/jdx/hk").pinned_by(&pins),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, IdentityError::OwnerChanged(t)
                if t.evidence == Some(Evidence::LockfilePin)),
            "{err}"
        );
    }

    #[test]
    fn an_accepted_transfer_keeps_old_releases_verifying() {
        // jdx/hk moved to acme/hk and a person accepted it: the pin records
        // acme and remembers jdx.
        let accepted = ForgePin::new("github.com/acme/hk", "922514152", Some("999".into()))
            .with_accepted_owner_ids(["216188"]);
        let expected = Expected::new("github.com/acme/hk").pinned(Some(&accepted));
        let ok = run(&expected, "github.com/acme/hk", Some(&transferred())).unwrap();
        assert_eq!(ok.continuity, Continuity::Same);
        assert_eq!(ok.pin.as_ref(), Some(&accepted));
        assert_eq!(ok.transfer(), None);

        // A release from before the transfer, signed by jdx, is by an
        // accepted owner, and the pin it gives still accepts acme.
        let ok = run(&expected, "github.com/jdx/hk", Some(&hk())).unwrap();
        assert_eq!(
            ok.continuity,
            Continuity::Renamed {
                requested: "github.com/acme/hk".into(),
                signed: "github.com/jdx/hk".into(),
            }
        );
        let older = ok.pin.unwrap();
        assert_eq!(older, pin().with_accepted_owner_ids(["999"]));
        let ok = run(
            &Expected::new("github.com/acme/hk").pinned(Some(&older)),
            "github.com/acme/hk",
            Some(&transferred()),
        )
        .unwrap();
        assert_eq!(ok.continuity, Continuity::Same);

        // An owner nobody accepted is still a transfer.
        let evil = source(
            "https://github.com/evil/hk",
            "922514152",
            "https://github.com/evil",
            "1",
        );
        let err = run(&expected, "github.com/evil/hk", Some(&evil)).unwrap_err();
        assert!(matches!(err, IdentityError::OwnerChanged(_)), "{err}");

        // A pin stored before the field existed still reads, and a pin with
        // no accepted owners is stored as before.
        let old: ForgePin = serde_json::from_str(
            r#"{"project":"github.com/jdx/hk","repository_id":"922514152","owner_id":"216188"}"#,
        )
        .unwrap();
        assert_eq!(old, pin());
        assert!(
            !serde_json::to_string(&pin())
                .unwrap()
                .contains("accepted_owner_ids")
        );
        let text = serde_json::to_string(&accepted).unwrap();
        assert_eq!(serde_json::from_str::<ForgePin>(&text).unwrap(), accepted);
    }

    #[test]
    fn a_resolved_owner_id_tells_an_owner_rename_from_a_transfer() {
        // The owner is called jdx now and was jdx2 when this release was
        // signed; a user asks for the current name.
        let before_rename = source(
            "https://github.com/jdx2/hk",
            "922514152",
            "https://github.com/jdx2",
            "216188",
        );
        let expected = Expected::new("github.com/jdx/hk").resolved(Some("922514152"));
        // By name alone, another owner name is a transfer.
        let err = run(&expected, "github.com/jdx2/hk", Some(&before_rename)).unwrap_err();
        assert!(
            matches!(&err, IdentityError::OwnerChanged(t) if t.evidence.is_none()),
            "{err}"
        );
        // The forge says jdx is owner 216188, the one that signed.
        let ok = run(
            &expected.resolved_owner(Some("216188")),
            "github.com/jdx2/hk",
            Some(&before_rename),
        )
        .unwrap();
        assert!(matches!(ok.continuity, Continuity::Renamed { .. }));

        // Another owner ID is a transfer even under the requested owner's
        // name: the name was taken by another account.
        let retaken = source(
            "https://github.com/jdx/hk",
            "922514152",
            "https://github.com/jdx",
            "31337",
        );
        let err = run(
            &expected.resolved_owner(Some("216188")),
            "github.com/jdx/hk",
            Some(&retaken),
        )
        .unwrap_err();
        assert_eq!(
            err,
            IdentityError::OwnerChanged(Box::new(Transfer {
                requested: "github.com/jdx/hk".into(),
                signed: "github.com/jdx/hk".into(),
                previous_owner: "jdx".into(),
                previous_owner_id: Some("216188".into()),
                owner: "jdx".into(),
                owner_id: Some("31337".into()),
                evidence: Some(Evidence::Resolved),
            }))
        );
        // Accepted, the resolved owner is remembered as accepted.
        let ok = run(
            &expected
                .resolved_owner(Some("216188"))
                .accepting_transfer(true),
            "github.com/jdx/hk",
            Some(&retaken),
        )
        .unwrap();
        assert_eq!(
            ok.pin.unwrap().accepted_owner_ids,
            vec!["216188".to_string()]
        );
        // With a pin, the pin decides.
        let pin = pin();
        let err = run(
            &expected.pinned(Some(&pin)).resolved_owner(Some("31337")),
            "github.com/jdx/hk",
            Some(&retaken),
        )
        .unwrap_err();
        assert!(
            matches!(&err, IdentityError::OwnerChanged(t) if t.evidence == Some(Evidence::Pin)),
            "{err}"
        );
    }

    #[test]
    fn a_gitlab_project_moved_to_another_group_is_a_transfer() {
        // A GitLab project's owner is its immediate group, so a move
        // between subgroups of one top-level group changes the owner ID.
        let moved = source(
            "https://gitlab.com/group/other/tool",
            "42",
            "https://gitlab.com/group/other",
            "8",
        );
        let pin = ForgePin::new("gitlab.com/group/sub/tool", "42", Some("7".into()));
        let err = check(
            &Expected::new("gitlab.com/group/sub/tool").pinned(Some(&pin)),
            "gitlab.com/group/other/tool",
            "https://gitlab.com/group/other/tool//.gitlab-ci.yml@refs/tags/v1",
            Some(GITLAB_ISSUER),
            Some(&moved),
        )
        .unwrap_err();
        assert!(
            matches!(&err, IdentityError::OwnerChanged(t)
                if t.previous_owner == "group/sub" && t.owner == "group/other"),
            "{err}"
        );
    }

    #[test]
    fn stored_signers_are_compared_within_their_repository() {
        let release =
            |repo: &str| format!("https://github.com/{repo}/.github/workflows/release.yml");
        let hk = pin();
        let hook = ForgePin::new("github.com/jdx/hook", "922514152", Some("216188".into()));
        // A rename keeps the workflow, whatever the ref.
        assert!(same_workflow(
            &release("jdx/hk"),
            Some(&hk),
            &format!("{}@refs/tags/v3.0.0", release("jdx/hook")),
            Some(&hook),
        ));
        assert!(!same_workflow(
            &release("jdx/hk"),
            Some(&hk),
            "https://github.com/jdx/hook/.github/workflows/other.yml",
            Some(&hook),
        ));
        // A recreated name: the same identity, another repository.
        let squatter = ForgePin::new("github.com/jdx/hk", "555", Some("216188".into()));
        assert!(!same_workflow(
            &release("jdx/hk"),
            Some(&hk),
            &release("jdx/hk"),
            Some(&squatter),
        ));
        // Another owner, unless it was accepted.
        let acme = ForgePin::new("github.com/acme/hk", "922514152", Some("999".into()));
        assert!(!same_workflow(
            &release("jdx/hk"),
            Some(&hk),
            &release("acme/hk"),
            Some(&acme),
        ));
        assert!(same_workflow(
            &release("jdx/hk"),
            Some(&hk),
            &release("acme/hk"),
            Some(&acme.clone().with_accepted_owner_ids(["216188"])),
        ));
        // Without a pin on both sides, only the identity itself, less a
        // workflow's ref.
        assert!(same_workflow(
            &release("jdx/hk"),
            None,
            &format!("{}@refs/tags/v1", release("jdx/hk")),
            Some(&hk),
        ));
        assert!(!same_workflow(
            &release("jdx/hk"),
            None,
            &release("jdx/hook"),
            Some(&hook),
        ));
        assert!(same_workflow(
            "alice@example.com",
            None,
            "alice@example.com",
            None
        ));
        assert!(!same_workflow(
            "alice@example.com",
            None,
            "alice@example.org",
            None
        ));

        // Pins compare IDs on one forge, and owners by name without IDs.
        let gitlab = ForgePin::new("gitlab.com/jdx/hk", "922514152", Some("216188".into()));
        assert!(!gitlab.continues(&hk));
        let by_name = |project: &str| ForgePin::new(project, "922514152", None);
        assert!(by_name("github.com/JDX/hook").continues(&by_name("github.com/jdx/hk")));
        assert!(!by_name("github.com/acme/hk").continues(&by_name("github.com/jdx/hk")));
    }

    #[test]
    fn signer_continuity_by_name_needs_the_same_identity() {
        // No pin and no forge answer: the name was all there was, so the
        // workflow's path alone does not make another repository's the same.
        let ok = run(
            &Expected::new("github.com/jdx/hk"),
            "github.com/jdx/hk",
            Some(&hk()),
        )
        .unwrap();
        assert!(ok.continues_signer("https://github.com/jdx/hk/.github/workflows/release.yml"));
        assert!(!ok.continues_signer("https://github.com/other/hk/.github/workflows/release.yml"));
    }
}
