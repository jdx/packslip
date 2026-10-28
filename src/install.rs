//! Command-scoped bootstrap orchestration. Discovery never supplies policy;
//! authenticated lists and releases are checked under the persistent scope lock.
use crate::{discovery, install_extract, install_fs, install_host, install_policy};
use eyre::{Result, bail};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[cfg(unix)]
fn xdg_home(value: Option<std::ffi::OsString>, home: &Path, fallback: &str) -> PathBuf {
    value
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(fallback))
}

pub struct Request {
    pub project: String,
    pub version: Option<String>,
    pub system: bool,
    pub user: bool,
    pub force: bool,
    pub offline: bool,
    pub install_dir: Option<PathBuf>,
    pub bin_dir: Option<PathBuf>,
    pub variant: Option<String>,
    pub constraints: install_policy::Constraints,
    pub accept_trust_change: Vec<String>,
    pub trusted_root: Option<PathBuf>,
    pub allow_unlogged: bool,
    pub allow_incompatible_host: bool,
    pub limits: install_extract::Limits,
}

#[derive(Debug)]
pub struct Report {
    pub project: String,
    pub receipt: install_fs::Receipt,
    pub pin: Option<String>,
    pub warnings: Vec<String>,
}

/// A scope is independent of destination overrides and project aliases.
#[derive(Debug)]
pub struct Scope {
    pub state: PathBuf,
    pub installs: PathBuf,
    pub bin: PathBuf,
    pub config: Option<PathBuf>,
    pub admin: PathBuf,
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("installation configuration requires absolute paths");
    }
    Ok(path)
}

impl Scope {
    pub fn detect(system: bool, user: bool) -> Result<Self> {
        if system && user {
            bail!("--system and --user are mutually exclusive");
        }
        #[cfg(unix)]
        {
            let root = rustix::process::geteuid().is_root();
            if root && user {
                bail!("run without root privileges to select the user scope");
            }
            if system || root {
                return Ok(Self {
                    state: "/var/lib/packslip".into(),
                    installs: "/opt/packslip".into(),
                    bin: "/usr/local/bin".into(),
                    config: None,
                    admin: "/etc/packslip".into(),
                });
            }
            let home = absolute(
                std::env::var_os("HOME")
                    .ok_or_else(|| eyre::eyre!("HOME is required for user installation"))?
                    .into(),
            )?;
            let xdg = |name: &str, fallback: &str| -> Result<PathBuf> {
                absolute(xdg_home(std::env::var_os(name), &home, fallback))
            };
            Ok(Self {
                state: xdg("XDG_STATE_HOME", ".local/state")?.join("packslip"),
                installs: xdg("XDG_DATA_HOME", ".local/share")?.join("packslip"),
                bin: home.join(".local/bin"),
                config: Some(xdg("XDG_CONFIG_HOME", ".config")?.join("packslip")),
                admin: "/etc/packslip".into(),
            })
        }
        #[cfg(windows)]
        {
            use known_folders::{KnownFolder, get_known_folder_path};
            let folder = |f| {
                get_known_folder_path(f)
                    .ok_or_else(|| eyre::eyre!("Windows known folder is unavailable"))
            };
            let admin = folder(KnownFolder::ProgramData)?.join("packslip");
            if system {
                let installs = folder(KnownFolder::ProgramFiles)?.join("packslip");
                Ok(Self {
                    state: admin.join("state"),
                    bin: installs.join("bin"),
                    installs,
                    config: None,
                    admin,
                })
            } else {
                let base = folder(KnownFolder::LocalAppData)?.join("packslip");
                Ok(Self {
                    state: base.join("state"),
                    installs: base.join("installs"),
                    bin: base.join("bin"),
                    config: Some(base.join("config")),
                    admin,
                })
            }
        }
        #[cfg(not(any(unix, windows)))]
        bail!("unsupported installation platform")
    }
}

/// The name a project's alias history is kept under: the project, with a
/// GitHub owner and repository name in lowercase, since GitHub resolves
/// them in any case. A monorepo subpath keeps its case.
fn alias_key(project: &str) -> String {
    let Some((host, owner, repo)) = crate::model::repository(project) else {
        return project.to_owned();
    };
    let repository = format!(
        "{host}/{}/{}",
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase()
    );
    match crate::model::repository_subpath(project) {
        Some(subpath) => format!("{repository}/{subpath}"),
        None => repository,
    }
}

fn installation_key(project: &str, repository_id: Option<&str>) -> String {
    if let Some(id) = repository_id {
        let subpath = crate::model::repository_subpath(project).unwrap_or_default();
        format!(
            "github-{id}-{}",
            &hex::encode(Sha256::digest(subpath))[..16]
        )
    } else {
        format!("host-{}", hex::encode(Sha256::digest(project)))
    }
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    use std::io::Read as _;
    let mut open = std::fs::OpenOptions::new();
    open.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // A replaced configuration path must not block while opening a FIFO.
        open.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = match open.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !file.metadata()?.is_file() {
        bail!("installation configuration must be a regular file");
    }
    let mut text = String::new();
    file.take(4 * 1024 * 1024 + 1).read_to_string(&mut text)?;
    if text.len() > 4 * 1024 * 1024 {
        bail!("installation configuration exceeds 4 MiB");
    }
    Ok(Some(text))
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    http: discovery::HttpConfig,
}

fn constraints(
    scope: &Scope,
    project: &str,
    caller: &install_policy::Constraints,
) -> Result<Vec<install_policy::Constraints>> {
    let mut dirs = vec![scope.admin.join("pins.d")];
    if let Some(config) = &scope.config {
        dirs.push(config.join("pins.d"));
    }
    let mut constraints = install_policy::admin_constraints(
        &dirs.iter().map(PathBuf::as_path).collect::<Vec<_>>(),
        project,
    )?;
    let mut caller = caller.clone();
    if let Some(text) = &caller.pubkey {
        caller.pubkey = Some(if crate::minisign::PublicKey::parse(text).is_ok() {
            text.clone()
        } else {
            read_optional(Path::new(text))?
                .ok_or_else(|| eyre::eyre!("public-key file is missing"))?
        });
    }
    constraints.push(caller);
    Ok(constraints)
}

fn remember_key(history: &mut install_fs::History, constraints: &[install_policy::Constraints]) {
    if let Some(key) = constraints.iter().find_map(|c| c.pubkey.as_ref()) {
        history.pubkey = Some(key.clone());
    }
}

fn check_list(
    list: &install_policy::List,
    bundle: &str,
    request: &Request,
    history: &mut install_fs::History,
    constraints: &[install_policy::Constraints],
    now: jiff::Timestamp,
) -> Result<()> {
    discovery::list_freshness(Some(&list.verified.list), history.sequence, now)?;
    install_policy::continuity(
        &request.project,
        "list",
        bundle,
        history.list.as_ref(),
        &list.record,
        constraints,
        &request.accept_trust_change,
    )?;
    history.list = Some(list.record.clone());
    history.sequence = Some(list.verified.list.predicate.sequence);
    remember_key(history, constraints);
    Ok(())
}

fn check_release(
    release: &install_policy::Release,
    bundle: &str,
    selected: &discovery::Release,
    list: Option<&crate::ReleaseListStatement>,
    request: &Request,
    history: &install_fs::History,
    constraints: &[install_policy::Constraints],
) -> Result<()> {
    if crate::model::parse_version(&release.verified.version)?
        != crate::model::parse_version(&selected.version)?
    {
        bail!("signed release version differs from the selected version");
    }
    // A list subject binds the full bundle, not only the release's artifact.
    if let Some(list) = list
        && let Some(entry) = list.predicate.releases.iter().find(|r| {
            crate::model::parse_version(&r.version).ok()
                == crate::model::parse_version(&selected.version).ok()
        })
    {
        let digest = hex::encode(Sha256::digest(bundle.as_bytes()));
        if list.digest_of(&entry.packslip) != Some(digest.as_str()) {
            bail!("release bundle differs from its signed list digest");
        }
    }
    install_policy::continuity(
        &request.project,
        "release",
        bundle,
        history.release.as_ref(),
        &release.record,
        constraints,
        &request.accept_trust_change,
    )?;
    Ok(())
}

pub async fn run(mut request: Request) -> Result<Report> {
    let project = discovery::Project::parse(&request.project)?;
    request.project = project.as_str().into();
    if project.as_str().split('/').next() == Some("gitlab.com") {
        bail!(
            "bootstrap forge discovery currently supports github.com; other forges require a host-named project"
        );
    }
    let scope = Scope::detect(request.system, request.user)?;
    run_in(request, scope).await
}

/// A previously installed release that the verified list has since yanked.
struct Withdrawn {
    version: String,
    reason: Option<String>,
}

impl Withdrawn {
    /// `list` is verified, and `installed` is the version this scope last
    /// installed for the project. Warns only; it never affects selection.
    fn find(list: &crate::ReleaseListStatement, installed: &str) -> Option<Self> {
        let installed = crate::model::parse_version(installed).ok()?;
        let entry = list.predicate.releases.iter().find(|r| {
            r.is_yanked()
                && crate::model::parse_version(&r.version).ok().as_ref() == Some(&installed)
        })?;
        // The reason is the publisher's text; keep it to one printable line.
        let reason = entry
            .status_reason
            .as_deref()
            .map(|r| {
                r.chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .filter(|r| !r.is_empty());
        Some(Self {
            version: entry.version.clone(),
            reason,
        })
    }

    /// `installed` is the release this run installed, or `None` when the run
    /// ended without installing one.
    fn describe(&self, project: &str, installed: Option<&str>) -> String {
        let reason = self
            .reason
            .as_ref()
            .map(|r| format!(": {r}"))
            .unwrap_or_default();
        let outcome = match installed {
            Some(version) => format!("this run installed {version} instead"),
            None => "this run did not install a different release".into(),
        };
        format!(
            "the {project} {} you installed was withdrawn in its signed release list{reason}; {outcome}",
            self.version
        )
    }
}

// Explicit scope injection keeps integration fixtures isolated from real user
// and admin state. The CLI always obtains it from Scope::detect.
async fn run_in(request: Request, scope: Scope) -> Result<Report> {
    let project = request.project.clone();
    let mut withdrawn = None;
    match install_in(request, scope, &mut withdrawn).await {
        Ok(mut report) => {
            if let Some(withdrawn) = withdrawn {
                let notice = withdrawn.describe(&report.project, Some(&report.receipt.version));
                report.warnings.insert(0, notice);
            }
            Ok(report)
        }
        Err(error) => {
            // Without a report, a failure is the only place the user can learn
            // that the installed release was withdrawn.
            if let Some(withdrawn) = withdrawn {
                eprintln!("warning: {}", withdrawn.describe(&project, None));
            }
            Err(error)
        }
    }
}

async fn install_in(
    request: Request,
    scope: Scope,
    withdrawn: &mut Option<Withdrawn>,
) -> Result<Report> {
    let project = discovery::Project::parse(&request.project)?;
    let bin = absolute(request.bin_dir.clone().unwrap_or_else(|| scope.bin.clone()))?;
    let lookup = scope.installs.join(".lookup");
    let lookup_session = install_fs::Session::open(&scope.state, &lookup, &bin)?;
    let config: Config = scope
        .config
        .as_ref()
        .map(|p| read_optional(&p.join("config.toml")))
        .transpose()?
        .flatten()
        .map(|text| toml::from_str(&text))
        .transpose()?
        .unwrap_or_default();
    let client =
        discovery::Client::new(scope.state.join("downloads"), request.offline, config.http)?;
    let root = if let Some(path) = &request.trusted_root {
        let json =
            read_optional(path)?.ok_or_else(|| eyre::eyre!("trusted root file is missing"))?;
        crate::sigstore::trusted_root(Some(&json))?
    } else {
        let (repository, failed) = crate::trust_root::HttpRepository::production()?;
        crate::trust_root::refresh(
            repository,
            failed,
            sigstore_trust_root::SigstoreInstance::PublicGood.tuf_root(),
            &scope.state.join("tuf.json"),
            request.offline,
            jiff::Timestamp::now(),
        )
        .await?
    };
    let github = if project.as_str().starts_with("github.com/") {
        Some(discovery::github(&client, &project).await?)
    } else {
        None
    };
    let repository_id = github.as_ref().map(|g| g.repository_id.as_str());
    let key = installation_key(project.as_str(), repository_id);
    let tree = absolute(
        request
            .install_dir
            .clone()
            .unwrap_or_else(|| scope.installs.join(&key)),
    )?;
    drop(lookup_session);
    let session = install_fs::Session::open_for(&scope.state, &tree, &bin, &key)?;
    let mut history = session.history(&key)?;
    history.name_remembered_key();
    // Only a warning needs this, and the commit reads the receipt again and
    // refuses one it cannot use, so an unreadable receipt is not an error here.
    let installed = session.receipt(&key).ok().flatten().map(|r| r.version);
    // ID-based ownership follows renames, but a remembered name must never
    // silently become first use when that name is deleted and recreated, in
    // whatever case the request spells it. A history written before names
    // folded case is under the spelling of the request that wrote it, which
    // its records name. A domain project's name resolves to no repository:
    // its records' pins name the repository that signed it, which the
    // continuity checks below compare.
    let alias = alias_key(project.as_str());
    let mut names: BTreeSet<String> = session
        .remembered_projects()?
        .into_iter()
        .filter(|name| crate::forge::same_project(name, project.as_str()))
        .collect();
    names.extend([alias.clone(), project.as_str().to_owned()]);
    for name in &names {
        let prior = session.history(name)?;
        if let Some(previous) = prior.release.as_ref().or(prior.list.as_ref())
            && let Some(pin) = &previous.repository
            && github.is_some()
            && repository_id != Some(pin.repository_id.as_str())
        {
            bail!("repository name now resolves to a different remembered repository ID");
        }
    }
    let mut constraints = constraints(&scope, project.as_str(), &request.constraints)?;
    if !constraints.iter().any(|c| c.pubkey.is_some())
        && let Some(pubkey) = &history.pubkey
    {
        constraints.push(install_policy::Constraints {
            pubkey: Some(pubkey.clone()),
            ..Default::default()
        });
    }
    let options = crate::Options {
        require_log: !request.allow_unlogged,
        trusted_root: &root,
    };
    let mut host_list_document = None;
    let list = if let Some(github) = &github {
        if let Some(bytes) = discovery::github_list(&client, &github.list_url).await? {
            let bundle = std::str::from_utf8(&bytes)?;
            let list = install_policy::list(
                project.as_str(),
                repository_id,
                bundle,
                &constraints,
                options,
            )?;
            check_list(
                &list,
                bundle,
                &request,
                &mut history,
                &constraints,
                jiff::Timestamp::now(),
            )?;
            Some(list)
        } else {
            None
        }
    } else {
        let url = crate::list_url(
            project.as_str().split('/').next().unwrap(),
            project.as_str(),
        );
        let document = client
            .fetch_host(&project, &url, 16 * 1024 * 1024)
            .await?
            .ok_or_else(|| eyre::eyre!("host project has no signed release list"))?;
        let bundle = std::str::from_utf8(document.bytes())?;
        let list = install_policy::list_host(&document, &constraints, options)?;
        check_list(
            &list,
            bundle,
            &request,
            &mut history,
            &constraints,
            jiff::Timestamp::now(),
        )?;
        host_list_document = Some(document);
        Some(list)
    };
    discovery::list_freshness(
        list.as_ref().map(|l| &l.verified.list),
        history.sequence,
        jiff::Timestamp::now(),
    )?;
    if list.is_some() {
        session.remember(&key, &history)?;
        session.remember(&alias, &history)?;
    }
    let signed_list = list.as_ref().map(|l| &l.verified.list);
    *withdrawn = signed_list
        .zip(installed.as_deref())
        .and_then(|(list, installed)| Withdrawn::find(list, installed));
    let candidates = match &github {
        Some(github) => {
            github.candidates(project.as_str(), signed_list, request.version.as_deref())?
        }
        None => discovery::candidates_with_latest(
            project.as_str(),
            &[],
            signed_list,
            request.version.as_deref(),
            None,
        )?,
    };
    let lookup = Lookup {
        client: &client,
        project: &project,
        repository_id,
        forge: github.is_some(),
        list: signed_list,
        host_list: host_list_document.as_ref(),
        constraints: &constraints,
        options,
    };
    let mut host = install_host::Snapshot::detect()?;
    let mut skipped = candidates.skipped.clone();
    let mut fetched = 0;
    let mut chosen = None;
    for (index, selected) in candidates.releases.iter().enumerate() {
        // The first candidate is always examined in full, as before fallback
        // existed. A later one is examined only while its bundles fit.
        if index > 0 && fetched + selected.bundles.len() > MAX_BUNDLES {
            bail!(
                "stopped looking for an installable release of {} after fetching {} from {}; {}; name an older release with --version",
                project.as_str(),
                count(fetched, "bundle"),
                count(index, "release"),
                report_skips(&skipped)
            );
        }
        fetched += selected.bundles.len();
        let step = async {
            let (release, bundle, url) = match lookup.bundles(selected).await? {
                Packslip::Accepted(found) => *found,
                Packslip::Missing(reason) => return Ok(Step::Skip(reason)),
            };
            check_release(
                &release,
                &bundle,
                selected,
                signed_list,
                &request,
                &history,
                &constraints,
            )?;
            let mut remembered = history.clone();
            remembered.release = Some(release.record.clone());
            session.remember(&alias, &remembered)?;
            match crate::select_artifact(
                &release.statement.predicate.artifacts,
                &host.host(),
                request.variant.as_deref(),
                FORMATS,
            ) {
                Ok(artifact) => {
                    let artifact = artifact.clone();
                    Ok(Step::Install(Box::new((release, url, artifact))))
                }
                // Only a latest request counts a release without an artifact
                // for this host as ineligible. Ties and host requirements
                // never redirect installation to another release.
                Err(crate::model::Selection::NoMatch) if candidates.latest => {
                    Ok(Step::Skip(no_artifact(&host, request.variant.as_deref())))
                }
                Err(error) => Err(eyre::Report::from(error)),
            }
        }
        .await
        .map_err(|error| after_skips(error, &candidates.describe(selected), &skipped))?;
        match step {
            Step::Install(found) => {
                chosen = Some((selected, *found));
                break;
            }
            Step::Skip(reason) => skipped.push(candidates.skip(selected, reason)),
        }
    }
    let Some((selected, (release, url, artifact))) = chosen else {
        bail!(
            "no installable release of {}; {}",
            project.as_str(),
            report_skips(&skipped)
        );
    };
    let label = candidates.describe(selected);
    let notes = skip_notes(&skipped);
    let installed = async {
        host.observe(&artifact);
        let report = host.check(artifact.requires.as_ref());
        report.require_compatible(request.allow_incompatible_host)?;
        let artifact_url = match &artifact.url {
            Some(url) => url.clone(),
            None => {
                let mut base = reqwest::Url::parse(&url)?;
                base.set_query(None);
                base.set_fragment(None);
                base.path_segments_mut()
                    .map_err(|_| eyre::eyre!("bundle URL cannot locate artifacts"))?
                    .pop()
                    .push(&artifact.name);
                base.to_string()
            }
        };
        let input = client.download(&artifact_url, artifact.size).await?;
        let (digest, size) = crate::digest_file(input.path())?;
        if size != artifact.size
            || release.statement.digest_of(&artifact.name) != Some(digest.as_str())
        {
            bail!("downloaded artifact differs from its signed size or digest");
        }
        let parent = std::fs::canonicalize(
            tree.parent()
                .ok_or_else(|| eyre::eyre!("installation tree has no parent"))?,
        )?;
        let extracted =
            install_extract::extract(input.path(), &artifact, &parent, request.limits)?;
        history.release = Some(release.record.clone());
        remember_key(&mut history, &constraints);
        if request.force {
            eprintln!(
                "Replacement allowed at installation destination: {}",
                tree.display()
            );
            for (name, _) in &extracted.bins {
                eprintln!(
                    "Replacement allowed at command destination: {}",
                    bin.join(if cfg!(windows) {
                        format!("{name}.exe")
                    } else {
                        name.clone()
                    })
                    .display()
                );
            }
        }
        // The fingerprint of a forge project's repository. A keyless domain
        // project's record names the repository that signed it too, but
        // install reports a fingerprint only for a forge project.
        let pin = release
            .record
            .repository
            .as_ref()
            .filter(|_| lookup.forge)
            .and_then(|r| {
                crate::Fingerprint::of(release.record.issuer.as_deref()?, &r.repository_id)
            })
            .map(|p| p.to_string());
        let receipt = session.commit(
            extracted,
            &key,
            &release.verified.version,
            request.force,
            history,
            crate::install_launcher::write_export,
        )?;
        let mut warnings = notes;
        warnings.extend(report.warnings);
        for resource in crate::select_resources(&release.statement, &artifact) {
            warnings.push(format!("resource {} is retained in the release metadata; bootstrap installs only declared commands", resource.kind));
        }
        if !std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|p| p == bin))
        {
            warnings.push(format!("{} is absent from PATH; invoke the reported command paths or add this directory yourself", bin.display()));
        }
        Ok::<_, eyre::Report>(Report {
            project: project.as_str().into(),
            receipt,
            pin,
            warnings,
        })
    }
    .await;
    installed.map_err(|error| after_skips(error, &label, &skipped))
}

/// Formats a bootstrap install can unpack, best first.
const FORMATS: &[&str] = &[
    "tar.xz", "tar.zst", "tar.gz", "tgz", "tar.bz2", "tar", "zip", "xz", "zst", "gz", "bz2", "raw",
];

/// The bundle budget for one install's search for a release it can use. A
/// latest request skips a release whose forge release carries no packslip
/// for the project or whose packslip has no artifact for this host, and any
/// request skips the first kind; each skip costs that release's bundles.
/// The first candidate is always examined in full, however many bundles it
/// has. A later one is examined only if the total, its bundles included,
/// stays within this budget, so a host that no release supports does not
/// walk a long release history. The install command's `--version` help
/// states this number.
const MAX_BUNDLES: usize = 32;

/// What one candidate release holds for the project.
enum Packslip {
    /// The one authenticated packslip, with its text and URL.
    Accepted(Box<(install_policy::Release, String, String)>),
    /// No packslip this consumer reads, and why, for the skip report.
    Missing(String),
}

/// One candidate release's outcome: install it, or skip it for a reason.
enum Step {
    Install(Box<(install_policy::Release, String, crate::model::Artifact)>),
    Skip(String),
}

/// One line for a skipped recommendation, then one per run of the other
/// skipped releases that share a reason. Install tries those others in
/// descending version order, so a run reads as a range, such as
/// `skipped 5.0.0 to 2.0.0 (4 releases): no artifact fits this host (...)`.
fn skip_notes(skipped: &[discovery::Skipped]) -> Vec<String> {
    skipped
        .chunk_by(|a, b| a.reason == b.reason && !a.recommendation && !b.recommendation)
        .map(|run| match run {
            [one] => format!("skipped {one}"),
            [first, second] => format!(
                "skipped {} and {}: {}",
                first.release, second.release, first.reason
            ),
            [first, .., last] => format!(
                "skipped {} to {} ({} releases): {}",
                first.release,
                last.release,
                run.len(),
                first.reason
            ),
            [] => unreachable!("chunks are never empty"),
        })
        .collect()
}

fn report_skips(skipped: &[discovery::Skipped]) -> String {
    skip_notes(skipped).join("; ")
}

/// `1 bundle`, `2 bundles`.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A failure after a skip names what was skipped, so the error explains
/// why install tried this release.
fn after_skips(error: eyre::Report, release: &str, skipped: &[discovery::Skipped]) -> eyre::Report {
    if skipped.is_empty() {
        return error;
    }
    error.wrap_err(format!(
        "failed to install {release}; {}",
        report_skips(skipped)
    ))
}

fn no_artifact(host: &install_host::Snapshot, variant: Option<&str>) -> String {
    let host = host.host();
    let mut place = format!("{} {}", host.os, host.arch);
    if let Some(libc) = host.libc {
        place = format!("{place} {libc}");
    }
    match variant {
        Some(variant) => format!("no artifact with variant {variant} fits this host ({place})"),
        None => format!("no artifact fits this host ({place})"),
    }
}

/// What install needs to authenticate one candidate release's bundles.
struct Lookup<'a> {
    client: &'a discovery::Client,
    project: &'a discovery::Project,
    repository_id: Option<&'a str>,
    forge: bool,
    list: Option<&'a crate::ReleaseListStatement>,
    host_list: Option<&'a discovery::HostDocument>,
    constraints: &'a [install_policy::Constraints],
    options: crate::Options<'a>,
}

impl Lookup<'_> {
    /// The one authenticated packslip for the project among a release's
    /// bundles, or why there is none once every bundle was read: none
    /// describes the project, or the only one that does is a release
    /// statement of a format version this consumer does not read, which
    /// the specification has it ignore. A forge asset that cannot be
    /// fetched or read might be the project's packslip, which fails rather
    /// than skipping the release: fallback never follows a fetch or
    /// verification failure.
    async fn bundles(&self, selected: &discovery::Release) -> Result<Packslip> {
        let project = self.project.as_str();
        let mut accepted = None;
        let mut unknown: Option<eyre::Report> = None;
        let mut unsupported: Option<Option<String>> = None;
        for url in &selected.bundles {
            let listed_digest = self.list.and_then(|list| list.digest_of(url));
            let may_skip = self.forge && listed_digest.is_none();
            let host_authority = !self.forge
                && reqwest::Url::parse(url).ok().is_some_and(|u| {
                    u.host_str() == Some(project.split('/').next().unwrap())
                        && u.port_or_known_default() == Some(443)
                });
            let document = if host_authority {
                Some(
                    self.client
                        .fetch_host(self.project, url, 16 * 1024 * 1024)
                        .await?
                        .ok_or_else(|| eyre::eyre!("release bundle is missing"))?,
                )
            } else {
                None
            };
            let fetched: Result<String> = async {
                Ok(match &document {
                    Some(document) => std::str::from_utf8(document.bytes())?.to_owned(),
                    None => String::from_utf8(
                        self.client
                            .fetch(url, 16 * 1024 * 1024)
                            .await?
                            .ok_or_else(|| eyre::eyre!("release bundle is missing"))?,
                    )?,
                })
            }
            .await;
            let bundle = match fetched {
                Ok(bundle) => bundle,
                Err(error) if may_skip => {
                    unknown.get_or_insert(error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let Some(expected) = listed_digest
                && hex::encode(Sha256::digest(bundle.as_bytes())) != expected
            {
                bail!("release bundle differs from its signed list digest");
            }
            // A forge release can describe several monorepo tools. The peek only
            // filters candidates; it never establishes identity or authorizes use.
            if may_skip {
                match candidate(&bundle, project, self.repository_id) {
                    Candidate::Requested => {}
                    Candidate::Other => continue,
                    Candidate::Unsupported(predicate_type) => {
                        unsupported.get_or_insert(predicate_type);
                        continue;
                    }
                    Candidate::Unreadable => {
                        unknown.get_or_insert_with(|| {
                            eyre::eyre!("a packslip asset is not a readable bundle")
                        });
                        continue;
                    }
                }
            }
            let release = match &document {
                Some(document) => {
                    install_policy::release_host(document, self.constraints, self.options)?
                }
                None if self.host_list.is_some() && listed_digest.is_some() => {
                    install_policy::release_listed_host(
                        self.host_list.unwrap(),
                        url,
                        &bundle,
                        self.constraints,
                        self.options,
                    )?
                }
                None => install_policy::release(
                    project,
                    self.repository_id,
                    &bundle,
                    self.constraints,
                    self.options,
                )?,
            };
            if accepted.is_some() {
                bail!("multiple authenticated packslips describe the requested project");
            }
            accepted = Some((release, bundle, url.clone()));
        }
        match (accepted, unknown, unsupported) {
            (Some(found), _, _) => Ok(Packslip::Accepted(Box::new(found))),
            (None, Some(error), _) => Err(error.wrap_err(format!(
                "cannot tell whether release {} carries a packslip for {project}",
                selected.tag
            ))),
            (None, None, Some(Some(predicate_type))) => Ok(Packslip::Missing(format!(
                "its packslip for {project} has predicate type {predicate_type}, which this version of packslip cannot read"
            ))),
            (None, None, Some(None)) => Ok(Packslip::Missing(format!(
                "its packslip for {project} uses a release format this version of packslip cannot read"
            ))),
            (None, None, None) => Ok(Packslip::Missing(discovery::no_packslip(project))),
        }
    }
}

/// What an unverified peek says a forge asset describes.
#[derive(Debug, PartialEq, Eq)]
enum Candidate {
    /// The requested project, or the same repository under an older name.
    Requested,
    /// The requested project in another version of the release format,
    /// such as a future `release/v2`, which this consumer cannot read and
    /// skips like a release without the project's packslip. The predicate
    /// type comes from an unverified peek, so it is kept for the report
    /// only when it is a short run of URL characters.
    Unsupported(Option<String>),
    /// Another project, or a document that is not a release packslip, such
    /// as a signed release list attached to the release.
    Other,
    /// Nothing: the asset is not a bundle with a readable statement.
    Unreadable,
}

// Unlisted forge assets are discovery candidates. Invalid siblings do not
// establish policy and must not prevent finding the requested monorepo tool.
fn candidate(bundle: &str, project: &str, repository_id: Option<&str>) -> Candidate {
    let Ok(payload) = crate::sigstore::peek_statement(bundle) else {
        return Candidate::Unreadable;
    };
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&payload) else {
        return Candidate::Unreadable;
    };
    let name = payload["predicate"]["project"].as_str().unwrap_or_default();
    let predicate_type = payload["predicateType"].as_str().unwrap_or_default();
    // Release statements of every format version share this prefix; the
    // signed list's `releases/v1` does not.
    let release_types = crate::model::PREDICATE_TYPE.trim_end_matches("v1");
    let found = if predicate_type == crate::model::PREDICATE_TYPE {
        Candidate::Requested
    } else if predicate_type.starts_with(release_types) {
        let printable = predicate_type.len() <= 64
            && predicate_type
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/:.-_".contains(&b));
        Candidate::Unsupported(printable.then(|| predicate_type.to_owned()))
    } else {
        return Candidate::Other;
    };
    if crate::model::repository(name).is_none()
        || crate::model::repository_subpath(name) != crate::model::repository_subpath(project)
    {
        return Candidate::Other;
    }
    // The subpaths are the same, so this compares the repositories, whose
    // names GitHub resolves in any case.
    if crate::forge::same_project(name, project) {
        return found;
    }
    // A renamed or transferred repository can keep publishing its old name.
    // This unverified certificate peek only keeps a candidate for subsequent
    // authentication; the resolved ID and every trust constraint are checked
    // again by release verification before anything can be installed.
    let renamed = crate::sigstore::source_repository(bundle)
        .ok()
        .flatten()
        .is_some_and(|source| {
            source.id.as_deref().is_some()
                && source.id.as_deref() == repository_id
                && crate::model::repository(name).is_some_and(|(host, owner, repo)| {
                    source.uri.strip_prefix("https://").is_some_and(|uri| {
                        crate::forge::same_project(uri, &format!("{host}/{owner}/{repo}"))
                    })
                })
        });
    if renamed { found } else { Candidate::Other }
}

#[cfg(test)]
mod tests {
    /// Create a fixture directory and its parents as `rwxr-xr-x`, so the
    /// tests pass under any umask, as the installer's own directories do.
    fn test_dirs(path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o755);
        }
        builder.create(path)
    }
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn relative_xdg_values_use_home_defaults() {
        let home = Path::new("/home/test");
        for value in [None, Some("".into()), Some("relative".into())] {
            assert_eq!(
                xdg_home(value, home, ".local/state"),
                home.join(".local/state")
            );
        }
        assert_eq!(
            xdg_home(Some("/custom/state".into()), home, ".local/state"),
            Path::new("/custom/state")
        );
    }

    #[test]
    fn malformed_and_unrelated_forge_candidates_are_skipped() {
        let f = Fixture::new();
        let requested = "github.com/owner/repo/tool";
        assert_eq!(
            candidate("not a bundle", requested, Some("42")),
            Candidate::Unreadable
        );
        assert_eq!(
            candidate(&f.signed(&f.statement), requested, Some("42")),
            Candidate::Other
        );
        let mut statement = f.statement.clone();
        statement["predicate"]["project"] = json!(requested);
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Requested
        );
        // GitHub resolves names in any case; a subpath names a tool exactly.
        assert_eq!(
            candidate(&f.signed(&statement), "github.com/Owner/Repo/tool", None),
            Candidate::Requested
        );
        assert_eq!(
            candidate(&f.signed(&statement), "github.com/owner/repo/Tool", None),
            Candidate::Other
        );
        statement["predicate"]["project"] = json!("github.com/unrelated/repo/tool");
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Other
        );
        statement["predicate"]["project"] = json!("github.com/owner/repo/other");
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Other
        );
        // A release statement of another format version for this project
        // is recognized, so install can say why it skipped the release; one
        // for another tool, or a signed release list, is not this
        // project's packslip.
        statement["predicateType"] = json!("https://packslip.dev/release/v2");
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Other
        );
        statement["predicate"]["project"] = json!(requested);
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Unsupported(Some("https://packslip.dev/release/v2".into()))
        );
        // The type is unverified, so one a terminal could misread is not
        // repeated in a report.
        statement["predicateType"] = json!("https://packslip.dev/release/v2\u{1b}[2J");
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Unsupported(None)
        );
        statement["predicateType"] = json!(crate::model::RELEASES_PREDICATE_TYPE);
        assert_eq!(
            candidate(&f.signed(&statement), requested, Some("42")),
            Candidate::Other
        );
    }

    #[test]
    fn forge_candidate_renames_require_the_resolved_repository_id() {
        let bundle = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");
        for (project, id, expected) in [
            // The same name in another case needs no repository ID.
            ("github.com/JDX/HK", None, Candidate::Requested),
            (
                "github.com/jdx/renamed-hk",
                Some("922514152"),
                Candidate::Requested,
            ),
            (
                "github.com/new-owner/hk",
                Some("922514152"),
                Candidate::Requested,
            ),
            ("github.com/unrelated/hk", Some("43"), Candidate::Other),
            ("github.com/new-owner/hk", None, Candidate::Other),
            (
                "github.com/new-owner/hk/other",
                Some("922514152"),
                Candidate::Other,
            ),
        ] {
            assert_eq!(candidate(bundle, project, id), expected, "{project}");
        }
    }

    #[test]
    fn named_host_list_binds_off_host_bundles_without_weakening_pins() {
        let mut f = Fixture::new();
        f.bundle_url = "https://cdn.example.test/packslip.sigstore.json".into();
        f.publish(7, false);
        let bundle = f.signed(&f.statement);
        f.cache(&f.bundle_url, false, bundle.as_bytes());
        let client =
            discovery::Client::new(f.scope().state.join("downloads"), true, Default::default())
                .unwrap();
        let project = discovery::Project::parse("example.test").unwrap();
        let document = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(client.fetch_host(
                &project,
                "https://example.test/.well-known/packslip.json",
                16 * 1024 * 1024,
            ))
            .unwrap()
            .unwrap();
        let root = crate::sigstore::trusted_root(None).unwrap();
        let options = crate::Options {
            trusted_root: &root,
            require_log: false,
        };
        let constraints = [f.request(true).constraints];
        install_policy::release_listed_host(
            &document,
            &f.bundle_url,
            &bundle,
            &constraints,
            options,
        )
        .unwrap();
        assert!(
            install_policy::release_listed_host(
                &document,
                "https://other.test/bundle",
                &bundle,
                &constraints,
                options
            )
            .is_err()
        );
        assert!(
            install_policy::release_listed_host(
                &document,
                &f.bundle_url,
                &(bundle.clone() + " "),
                &constraints,
                options
            )
            .is_err()
        );
        let wrong = [install_policy::Constraints {
            pubkey: Some(
                crate::minisign::SecretKey::from_seed([30; 32])
                    .public_key()
                    .to_file(),
            ),
            ..Default::default()
        }];
        assert!(
            install_policy::release_listed_host(&document, &f.bundle_url, &bundle, &wrong, options)
                .is_err()
        );
        f.run(f.request(true)).unwrap();
    }

    struct Fixture {
        root: tempfile::TempDir,
        key: crate::minisign::SecretKey,
        statement: serde_json::Value,
        bundle_url: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = crate::install_extract::private_builder(".tmp")
                .tempdir()
                .unwrap();
            let tree = root.path().join("publisher/dist");
            test_dirs(tree.join("bin")).unwrap();
            test_dirs(tree.join("runtime")).unwrap();
            std::fs::write(tree.join("runtime/data"), "complete runtime").unwrap();
            let name = if cfg!(windows) { "tool.exe" } else { "tool" };
            let source = root.path().join("probe.rs");
            std::fs::write(
                &source,
                r#"
                fn main() {
                    let exe=std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
                    let runtime=exe.parent().unwrap().parent().unwrap().join("runtime/data");
                    println!("{}",std::fs::read_to_string(runtime).unwrap());
                    println!("{}",std::env::args().skip(1).collect::<Vec<_>>().join("|"));
                    std::process::exit(37);
                }
            "#,
            )
            .unwrap();
            let output = std::process::Command::new("rustc")
                .arg(&source)
                .arg("-o")
                .arg(tree.join("bin").join(name))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let archive = root.path().join("tool.tar");
            let mut tar = tar::Builder::new(std::fs::File::create(&archive).unwrap());
            // Rust/linker outputs may be sparse on the runner filesystem. This
            // fixture publishes ordinary tar files; GNU sparse extensions are
            // intentionally outside the bootstrap extractor's accepted entries.
            tar.sparse(false);
            tar.append_dir_all("dist", &tree).unwrap();
            tar.finish().unwrap();
            drop(tar);
            let (digest, size) = crate::digest_file(&archive).unwrap();
            let key = crate::minisign::SecretKey::from_seed([19; 32]);
            let identity = json!({"scheme":"sigstore-key","key_id":crate::minisign::key_id_hex(&key.public_key().key_id)});
            let statement = json!({"_type":crate::model::STATEMENT_TYPE,"predicateType":crate::model::PREDICATE_TYPE,
                "subject":[{"name":"tool.tar","digest":{"sha256":digest}}],
                "predicate":{"project":"example.test","version":"1.0.0","published_at":"2020-01-01T00:00:00Z",
                "identity":identity,"artifacts":[{"name":"tool.tar","url":"https://example.test/tool.tar",
                    "size":size,"format":"tar","bin":[{"name":"tool","path":format!("dist/bin/{name}")}],"requires":{"libs":[]}}]}});
            let fixture = Self {
                root,
                key,
                statement,
                bundle_url: "https://example.test/releases/packslip.sigstore.json".into(),
            };
            test_dirs(fixture.scope().state.join("downloads")).unwrap();
            fixture.cache(
                "https://example.test/tool.tar",
                false,
                &std::fs::read(archive).unwrap(),
            );
            std::fs::write(
                fixture.root.path().join("root.json"),
                sigstore_trust_root::SIGSTORE_PRODUCTION_TRUSTED_ROOT,
            )
            .unwrap();
            fixture.publish(7, false);
            fixture
        }
        fn scope(&self) -> Scope {
            Scope {
                state: self.root.path().join("state"),
                installs: self.root.path().join("installs"),
                bin: self.root.path().join("bin"),
                config: None,
                admin: self.root.path().join("admin"),
            }
        }
        fn cache(&self, url: &str, host: bool, bytes: &[u8]) {
            let digest = hex::encode(Sha256::digest(url));
            std::fs::write(
                self.scope().state.join("downloads").join(if host {
                    format!("host-{digest}")
                } else {
                    digest
                }),
                bytes,
            )
            .unwrap();
        }
        fn signed(&self, statement: &serde_json::Value) -> String {
            let envelope = crate::dsse::Envelope::sign(
                crate::dsse::IN_TOTO_PAYLOAD_TYPE,
                &serde_json::to_vec(statement).unwrap(),
                &self.key,
            );
            json!({"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json",
                "verificationMaterial":{"publicKey":{"hint":crate::sigstore::key_hint(&self.key.public_key())}},
                "dsseEnvelope":envelope}).to_string()
        }
        fn publish(&self, sequence: u64, yanked: bool) {
            let bundle = self.signed(&self.statement);
            self.cache(&self.bundle_url, true, bundle.as_bytes());
            let mut entry = json!({"version":"1.0.0","published_at":"2020-01-01T00:00:00Z","packslip":self.bundle_url});
            if yanked {
                entry["status"] = json!("yanked");
            }
            let now = jiff::Timestamp::now();
            let list = json!({"_type":crate::model::STATEMENT_TYPE,"predicateType":crate::model::RELEASES_PREDICATE_TYPE,
                "subject":[{"name":self.bundle_url,"digest":{"sha256":hex::encode(Sha256::digest(bundle.as_bytes()))}}],
                "predicate":{"project":"example.test","identity":self.statement["predicate"]["identity"],
                    "generated_at":now.to_string(),"expires_at":(now+std::time::Duration::from_secs(86400)).to_string(),
                    "sequence":sequence,"releases":[entry]}});
            self.cache(
                "https://example.test/.well-known/packslip.json",
                true,
                self.signed(&list).as_bytes(),
            );
        }
        /// Publish a list of the fixture's release at the given versions,
        /// each with its own signed bundle; `Some(reason)` yanks that version.
        fn publish_versions(&self, sequence: u64, releases: &[(&str, Option<&str>)]) {
            let releases: Vec<_> = releases
                .iter()
                .map(|(version, yanked)| {
                    let mut statement = self.statement.clone();
                    statement["predicate"]["version"] = json!(version);
                    (statement, *yanked)
                })
                .collect();
            self.publish_list(sequence, &releases);
        }
        /// `publish_list` with no release yanked.
        fn publish_releases(&self, sequence: u64, statements: &[serde_json::Value]) {
            let releases: Vec<_> = statements.iter().map(|s| (s.clone(), None)).collect();
            self.publish_list(sequence, &releases);
        }
        /// Publish a signed list naming one release per statement, each
        /// bundle under its version's directory on the host; `Some(reason)`
        /// yanks that release.
        fn publish_list(&self, sequence: u64, releases: &[(serde_json::Value, Option<&str>)]) {
            let mut subjects = Vec::new();
            let mut entries = Vec::new();
            for (statement, yanked) in releases {
                let version = statement["predicate"]["version"].as_str().unwrap();
                let url = format!("https://example.test/releases/{version}/packslip.sigstore.json");
                let bundle = self.signed(statement);
                self.cache(&url, true, bundle.as_bytes());
                subjects.push(json!({"name":url,"digest":{"sha256":hex::encode(Sha256::digest(bundle.as_bytes()))}}));
                let mut entry =
                    json!({"version":version,"published_at":"2020-01-01T00:00:00Z","packslip":url});
                if let Some(reason) = yanked {
                    entry["status"] = json!("yanked");
                    entry["status_reason"] = json!(reason);
                }
                entries.push(entry);
            }
            let now = jiff::Timestamp::now();
            let list = json!({"_type":crate::model::STATEMENT_TYPE,"predicateType":crate::model::RELEASES_PREDICATE_TYPE,
                "subject":subjects,
                "predicate":{"project":"example.test","identity":self.statement["predicate"]["identity"],
                    "generated_at":now.to_string(),"expires_at":(now+std::time::Duration::from_secs(86400)).to_string(),
                    "sequence":sequence,"releases":entries}});
            self.cache(
                "https://example.test/.well-known/packslip.json",
                true,
                self.signed(&list).as_bytes(),
            );
        }
        /// The fixture's release at another version, with its one artifact
        /// limited to a platform no test host runs.
        fn foreign(&self, version: &str) -> serde_json::Value {
            let mut statement = self.statement.clone();
            statement["predicate"]["version"] = json!(version);
            statement["predicate"]["artifacts"][0]["os"] = json!("illumos");
            statement["predicate"]["artifacts"][0]["arch"] = json!("x86_64");
            statement
        }
        /// Serve github.com/jdx/hk from the cache: GitHub's latest release is
        /// v3.0.0, whose only packslip asset holds `decoy`, and v2.3.0 carries
        /// the real hk packslip.
        fn github_hk(&self, decoy: &[u8]) {
            self.github_hk_releases("v3.0.0", &[("v3.0.0", decoy), ("v2.3.0", HK.as_bytes())]);
        }
        /// Serve github.com/jdx/hk from the cache with `releases` in the
        /// releases endpoint's order, each with one packslip asset holding
        /// the given bytes, and `latest` as GitHub's latest release. The
        /// repository has no supplementary list.
        fn github_hk_releases(&self, latest: &str, releases: &[(&str, &[u8])]) {
            let base = "https://api.github.com/repos/jdx/hk";
            let download = "https://github.com/jdx/hk/releases/download";
            let listed: Vec<_> = releases
                .iter()
                .map(|(tag, _)| {
                    json!({"tag_name":tag,"draft":false,"assets":[{"name":"packslip.sigstore.json",
                        "browser_download_url":format!("{download}/{tag}/packslip.sigstore.json")}]})
                })
                .collect();
            for (url, value) in [
                (
                    base.to_owned(),
                    json!({"id":922514152,"default_branch":"main"}),
                ),
                (
                    format!("{base}/releases/latest"),
                    json!({"tag_name":latest,"draft":false}),
                ),
                (
                    format!("{base}/releases?per_page=100&page=1"),
                    json!(listed),
                ),
            ] {
                self.cache(&url, false, &serde_json::to_vec(&value).unwrap());
            }
            let list = format!("{base}/contents/.well-known/packslip.json?ref=main");
            std::fs::write(
                self.scope()
                    .state
                    .join("downloads")
                    .join(format!("{}.missing", hex::encode(Sha256::digest(&list)))),
                b"404",
            )
            .unwrap();
            for (tag, bytes) in releases {
                self.cache(
                    &format!("{download}/{tag}/packslip.sigstore.json"),
                    false,
                    bytes,
                );
            }
        }
        /// A key-signed packslip for another tool in jdx/hk, which install
        /// passes over when it looks for hk's own packslip.
        fn sibling(&self, version: &str) -> Vec<u8> {
            let mut sibling = self.statement.clone();
            sibling["predicate"]["project"] = json!("github.com/jdx/hk/other");
            sibling["predicate"]["version"] = json!(version);
            self.signed(&sibling).into_bytes()
        }
        fn hk_request(&self, version: Option<&str>) -> Request {
            let mut request = self.request(false);
            request.project = "github.com/jdx/hk".into();
            request.version = version.map(Into::into);
            request.allow_unlogged = false;
            request
        }
        fn request(&self, first: bool) -> Request {
            Request {
                project: "example.test".into(),
                version: None,
                system: false,
                user: false,
                force: false,
                offline: true,
                install_dir: None,
                bin_dir: None,
                variant: None,
                constraints: install_policy::Constraints {
                    pubkey: first.then(|| self.key.public_key().to_file()),
                    ..Default::default()
                },
                accept_trust_change: vec![],
                trusted_root: Some(self.root.path().join("root.json")),
                allow_unlogged: true,
                allow_incompatible_host: false,
                limits: install_extract::Limits::default(),
            }
        }
        fn run(&self, request: Request) -> Result<Report> {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(run_in(request, self.scope()))
        }
        /// The run's outcome with the withdrawal it found, which `run_in`
        /// prints to stderr when the run fails.
        fn run_withdrawn(&self, request: Request) -> (Result<Report>, Option<Withdrawn>) {
            let mut withdrawn = None;
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(install_in(request, self.scope(), &mut withdrawn));
            (result, withdrawn)
        }
    }

    #[test]
    fn offline_install_handoff_relocation_and_failed_freshness_preserve_history() {
        let f = Fixture::new();
        let report = f.run(f.request(true)).unwrap();
        assert!(report.receipt.tree.join("runtime/data").is_file());
        assert_eq!(report.receipt.exports.len(), 1);
        assert!(!f.scope().bin.join("runtime").exists());
        let command = report.receipt.exports.keys().next().unwrap();
        let output = std::process::Command::new(command)
            .args(["two words", "quote\"inside"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(37),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .replace("\r\n", "\n"),
            "complete runtime\ntwo words|quote\"inside\n"
        );
        let mut request = f.request(false); // Remembered key, no caller key.
        request.install_dir = Some(f.root.path().join("relocated/tree"));
        request.bin_dir = Some(f.root.path().join("relocated/bin"));
        let report = f.run(request).unwrap();
        assert!(!command.exists());
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
        f.publish(6, false);
        let mut request = f.request(false);
        request.force = true;
        assert!(f.run(request).unwrap_err().to_string().contains("stale"));
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
        f.publish(8, true);
        let mut request = f.request(false);
        request.version = Some("1.0.0".into());
        assert!(
            f.run(request)
                .unwrap_err()
                .to_string()
                .contains("withdrawn")
        );
        let session = install_fs::Session::open_for(
            &f.scope().state,
            &report.receipt.tree,
            &f.root.path().join("relocated/bin"),
            &report.receipt.project,
        )
        .unwrap();
        assert_eq!(
            session.history(&report.receipt.project).unwrap().sequence,
            Some(8)
        );
        assert_eq!(
            session
                .receipt(&report.receipt.project)
                .unwrap()
                .unwrap()
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn upgrade_warns_when_the_installed_release_was_withdrawn() {
        let f = Fixture::new();
        let first = f.run(f.request(true)).unwrap();
        assert_eq!(first.receipt.version, "1.0.0");
        assert!(first.warnings.iter().all(|w| !w.contains("withdrawn")));
        // Yanking some other version says nothing about the installed one.
        f.publish_versions(8, &[("1.1.0", Some("bad build")), ("1.0.0", None)]);
        let unaffected = f.run(f.request(false)).unwrap();
        assert_eq!(unaffected.receipt.version, "1.0.0");
        assert!(unaffected.warnings.iter().all(|w| !w.contains("withdrawn")));
        f.publish_versions(9, &[("1.1.0", None), ("1.0.0", Some("CVE-2026-1234"))]);
        let report = f.run(f.request(false)).unwrap();
        assert_eq!(report.receipt.version, "1.1.0");
        assert_eq!(
            report.warnings[0],
            "the example.test 1.0.0 you installed was withdrawn in its signed release list: \
             CVE-2026-1234; this run installed 1.1.0 instead"
        );
        // The replacement is what the scope has now, so the next run is quiet.
        let again = f.run(f.request(false)).unwrap();
        assert!(again.warnings.iter().all(|w| !w.contains("withdrawn")));
    }

    #[test]
    fn first_install_of_a_list_that_yanks_other_versions_does_not_warn() {
        let f = Fixture::new();
        f.publish_versions(8, &[("1.1.0", None), ("1.0.0", Some("CVE-2026-1234"))]);
        let report = f.run(f.request(true)).unwrap();
        assert_eq!(report.receipt.version, "1.1.0");
        assert!(report.warnings.iter().all(|w| !w.contains("withdrawn")));
    }

    #[test]
    fn warning_survives_a_run_that_cannot_install_a_replacement() {
        let f = Fixture::new();
        let first = f.run(f.request(true)).unwrap();
        f.publish_versions(8, &[("1.0.0", Some("CVE-2026-1234"))]);
        let (result, withdrawn) = f.run_withdrawn(f.request(false));
        // Nothing is refused for the warning; there is simply nothing eligible.
        assert!(result.is_err());
        assert_eq!(
            withdrawn.unwrap().describe("example.test", None),
            "the example.test 1.0.0 you installed was withdrawn in its signed release list: \
             CVE-2026-1234; this run did not install a different release"
        );
        {
            // The session holds the scope lock, so it must end before the next run.
            let session = install_fs::Session::open_for(
                &f.scope().state,
                &first.receipt.tree,
                &f.scope().bin,
                &first.receipt.project,
            )
            .unwrap();
            assert_eq!(
                session
                    .receipt(&first.receipt.project)
                    .unwrap()
                    .unwrap()
                    .version,
                "1.0.0"
            );
        }
        // An explicit request for the yanked release is still refused.
        let mut request = f.request(false);
        request.version = Some("1.0.0".into());
        let (result, withdrawn) = f.run_withdrawn(request);
        assert!(result.unwrap_err().to_string().contains("withdrawn"));
        assert!(withdrawn.is_some());
    }

    #[test]
    fn withdrawal_reason_is_one_printable_line() {
        let list = |reason: &str| -> crate::ReleaseListStatement {
            serde_json::from_value(json!({"_type":crate::model::STATEMENT_TYPE,
                "predicateType":crate::model::RELEASES_PREDICATE_TYPE,
                "subject":[{"name":"https://example.test/1.0.0/packslip.sigstore.json","digest":{"sha256":"0".repeat(64)}}],
                "predicate":{"project":"example.test","identity":{"scheme":"sigstore-key","key_id":"5A0A0B8B9C6D7E1F"},
                    "generated_at":"2026-09-01T12:00:00Z","expires_at":"2026-10-01T12:00:00Z","sequence":1,
                    "releases":[{"version":"1.0.0","published_at":"2020-01-01T00:00:00Z",
                        "packslip":"https://example.test/1.0.0/packslip.sigstore.json",
                        "status":"yanked","status_reason":reason}]}}))
            .unwrap()
        };
        for (reason, shown) in [
            ("\u{1b}[2Jbad\nbuild\u{7}", ": [2Jbad build"),
            ("  \t ", ""),
            ("", ""),
        ] {
            let withdrawn = Withdrawn::find(&list(reason), "1.0.0").unwrap();
            assert_eq!(
                withdrawn.describe("example.test", Some("1.1.0")),
                format!(
                    "the example.test 1.0.0 you installed was withdrawn in its signed release list{shown}; \
                     this run installed 1.1.0 instead"
                ),
                "{reason:?}"
            );
        }
        // Only the installed version matters; build metadata is part of it.
        assert!(Withdrawn::find(&list("x"), "1.0.1").is_none());
        assert!(Withdrawn::find(&list("x"), "1.0.0+build.5").is_none());
        assert!(Withdrawn::find(&list("x"), "not a version").is_none());
    }

    #[test]
    fn latest_falls_back_past_a_release_without_an_artifact_for_this_host() {
        let f = Fixture::new();
        f.publish_releases(8, &[f.foreign("2.0.0"), f.statement.clone()]);
        let report = f.run(f.request(true)).unwrap();
        assert_eq!(report.receipt.version, "1.0.0");
        assert!(
            report.warnings[0].starts_with("skipped 2.0.0: no artifact fits this host ("),
            "{:?}",
            report.warnings
        );
        // A version request names what it wants; a missing artifact for this
        // host fails instead of selecting an older release.
        for version in ["2.0.0", ">=1"] {
            let mut request = f.request(true);
            request.version = Some(version.into());
            let error = f.run(request).unwrap_err();
            assert!(
                error.to_string().contains("no artifact fits this host"),
                "{version}: {error:?}"
            );
        }
    }

    #[test]
    fn a_withdrawal_warning_comes_before_the_releases_a_fallback_skipped() {
        let f = Fixture::new();
        assert_eq!(f.run(f.request(true)).unwrap().receipt.version, "1.0.0");
        let mut replacement = f.statement.clone();
        replacement["predicate"]["version"] = json!("1.1.0");
        f.publish_list(
            8,
            &[
                (f.foreign("2.0.0"), None),
                (replacement, None),
                (f.statement.clone(), Some("CVE-2026-1234")),
            ],
        );
        let report = f.run(f.request(false)).unwrap();
        assert_eq!(report.receipt.version, "1.1.0");
        assert_eq!(
            report.warnings[0],
            "the example.test 1.0.0 you installed was withdrawn in its signed release list: \
             CVE-2026-1234; this run installed 1.1.0 instead"
        );
        assert!(
            report.warnings[1].starts_with("skipped 2.0.0: no artifact fits this host ("),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn host_requirements_never_redirect_installation_to_another_release() {
        let f = Fixture::new();
        let mut newer = f.statement.clone();
        newer["predicate"]["version"] = json!("2.0.0");
        newer["predicate"]["artifacts"][0]["requires"] = json!({
            "libs": ["libpackslip-test-missing.so.1"],
            "glibc_min": "999999",
            "os_min": "999999"
        });
        f.publish_releases(8, &[newer, f.statement.clone()]);
        match f.run(f.request(true)) {
            Ok(report) => assert_eq!(report.receipt.version, "2.0.0"),
            Err(error) => assert!(
                error.to_string().contains("incompatible with this host"),
                "{error:?}"
            ),
        }
    }

    #[test]
    fn an_artifact_tie_fails_instead_of_falling_back() {
        // One artifact names this host's os and the other its arch. Both
        // fit, equally specific and in the same format, so neither wins.
        let f = Fixture::new();
        let snapshot = install_host::Snapshot::detect().unwrap();
        let host = snapshot.host();
        let mut tied = f.statement.clone();
        tied["predicate"]["version"] = json!("2.0.0");
        let mut copy = tied["predicate"]["artifacts"][0].clone();
        tied["predicate"]["artifacts"][0]["os"] = json!(host.os);
        copy["name"] = json!("tool-copy.tar");
        copy["arch"] = json!(host.arch);
        tied["predicate"]["artifacts"]
            .as_array_mut()
            .unwrap()
            .push(copy);
        let mut subject = tied["subject"][0].clone();
        subject["name"] = json!("tool-copy.tar");
        tied["subject"].as_array_mut().unwrap().push(subject);
        f.publish_releases(8, &[tied, f.statement.clone()]);
        let error = f.run(f.request(true)).unwrap_err().to_string();
        assert_eq!(
            error,
            r#"artifacts "tool.tar" and "tool-copy.tar" both fit this host and nothing tells them apart"#
        );
    }

    #[test]
    fn fallback_stops_after_a_bounded_number_of_bundles() {
        let f = Fixture::new();
        let mut statements: Vec<_> = (2..42).map(|n| f.foreign(&format!("{n}.0.0"))).collect();
        statements.push(f.statement.clone());
        f.publish_releases(8, &statements);
        let error = f.run(f.request(true)).unwrap_err().to_string();
        assert!(
            error.starts_with(&format!(
                "stopped looking for an installable release of example.test after fetching {MAX_BUNDLES} bundles from {MAX_BUNDLES} releases; skipped 41.0.0 to 10.0.0 ({MAX_BUNDLES} releases): no artifact fits this host ("
            )),
            "{error}"
        );
        assert!(
            error.ends_with("); name an older release with --version"),
            "{error}"
        );
    }

    #[test]
    fn counts_agree_with_their_number() {
        assert_eq!(count(1, "bundle"), "1 bundle");
        assert_eq!(count(2, "bundle"), "2 bundles");
        assert_eq!(count(1, "release"), "1 release");
    }

    #[test]
    fn skips_sharing_a_reason_are_reported_together() {
        let skip = |release: &str, reason: &str| discovery::Skipped::new(release, reason);
        // The recommendation, tried first, gets its own line even when the
        // releases after it share its reason; they follow by version.
        let recommendation = |release: &str, reason: &str| discovery::Skipped {
            recommendation: true,
            ..skip(release, reason)
        };
        assert_eq!(
            skip_notes(&[
                recommendation("2.0.0, GitHub's latest release", "no artifact fits"),
                skip("4.0.0", "no artifact fits"),
                skip("3.0.0", "no artifact fits"),
                skip("1.9.0", "no artifact fits"),
                skip("1.8.0", "its release carries no packslip for p"),
                skip("1.7.0", "no artifact fits"),
                skip("1.6.0", "no artifact fits"),
            ]),
            [
                "skipped 2.0.0, GitHub's latest release: no artifact fits",
                "skipped 4.0.0 to 1.9.0 (3 releases): no artifact fits",
                "skipped 1.8.0: its release carries no packslip for p",
                "skipped 1.7.0 and 1.6.0: no artifact fits",
            ]
        );
        assert_eq!(
            skip_notes(&[
                recommendation("2.3.0, GitHub's latest release", "no packslip"),
                skip("2.4.0", "no packslip"),
            ]),
            [
                "skipped 2.3.0, GitHub's latest release: no packslip",
                "skipped 2.4.0: no packslip",
            ]
        );
    }

    /// hk's real v2.3.0 packslip, signed keylessly by its release workflow.
    const HK: &str = include_str!("../tests/fixtures/hk-v2.3.0.sigstore.json");

    /// The error a latest install of the GitHub hk fixture reports once it
    /// reaches hk 2.3.0, named `release`, after the skips `notes`. A forge
    /// project accepts only a packslip its forge signed, and the fixture
    /// cache holds none of hk's multi-megabyte archives, so the install
    /// cannot finish offline. On a host that an hk 2.3.0 artifact fits, it
    /// fails after selecting 2.3.0, and the error names what it skipped
    /// with the text a successful install would print as warnings. On
    /// any other host, 2.3.0 is skipped too.
    fn hk_outcome(release: &str, notes: &str) -> String {
        let host = install_host::Snapshot::detect().unwrap();
        let statement: crate::Statement =
            serde_json::from_slice(&crate::sigstore::peek_statement(HK).unwrap()).unwrap();
        match crate::select_artifact(&statement.predicate.artifacts, &host.host(), None, FORMATS) {
            Err(crate::model::Selection::NoMatch) => format!(
                "no installable release of github.com/jdx/hk; {notes}; skipped {release}: {}",
                no_artifact(&host, None)
            ),
            _ => format!("failed to install {release}; {notes}"),
        }
    }

    #[test]
    fn forge_release_without_this_projects_packslip_is_skipped_and_reported() {
        let f = Fixture::new();
        // A monorepo sibling's packslip under a tag that names a version
        // of github.com/jdx/hk.
        f.github_hk(&f.sibling("3.0.0"));
        let error = f.run(f.hk_request(None)).unwrap_err().to_string();
        assert_eq!(
            error,
            hk_outcome(
                "2.3.0",
                "skipped 3.0.0, GitHub's latest release: its release carries no packslip for github.com/jdx/hk"
            )
        );
        // An exact tag names one release and does not fall back. The
        // recommendation has no place in that request, so the report does
        // not name it.
        let error = f.run(f.hk_request(Some("v3.0.0"))).unwrap_err().to_string();
        assert_eq!(
            error,
            "no installable release of github.com/jdx/hk; skipped 3.0.0: its release carries no packslip for github.com/jdx/hk"
        );
    }

    #[test]
    fn a_forge_release_sharing_the_recommended_version_keeps_its_place() {
        let f = Fixture::new();
        // GitHub's latest release, v2.3.0, carries only another tool's
        // packslip, and GitHub lists hk-v2.3.0, which carries hk's, before
        // it. Install tries GitHub's latest release first and then the
        // other releases from the highest down, so 2.4.0 comes before
        // hk-v2.3.0, though hk-v2.3.0 shares the recommended version. The
        // report names the recommendation on its own line and by tag.
        f.github_hk_releases(
            "v2.3.0",
            &[
                ("hk-v2.3.0", HK.as_bytes()),
                ("v2.3.0", &f.sibling("2.3.0")),
                ("v2.4.0", &f.sibling("2.4.0")),
            ],
        );
        let error = f.run(f.hk_request(None)).unwrap_err().to_string();
        assert_eq!(
            error,
            hk_outcome(
                "2.3.0 (tag hk-v2.3.0)",
                "skipped 2.3.0 (tag v2.3.0), GitHub's latest release: its release carries no packslip for github.com/jdx/hk; skipped 2.4.0: its release carries no packslip for github.com/jdx/hk"
            )
        );
    }

    #[test]
    fn an_unreadable_forge_asset_fails_instead_of_falling_back() {
        let f = Fixture::new();
        f.github_hk(b"not a bundle");
        let error = format!("{:?}", f.run(f.hk_request(None)).unwrap_err());
        assert!(
            error.contains(
                "cannot tell whether release v3.0.0 carries a packslip for github.com/jdx/hk"
            ),
            "{error}"
        );
        assert!(!error.contains("2.3.0"), "{error}");
    }

    #[test]
    fn a_packslip_in_an_unread_format_is_skipped_and_reported() {
        let f = Fixture::new();
        // The specification has a consumer keep only release/v1 packslips,
        // so a release whose only hk packslip is a future format is one it
        // does not discover, and the report says why.
        let mut newer = f.statement.clone();
        newer["predicateType"] = json!("https://packslip.dev/release/v2");
        newer["predicate"]["project"] = json!("github.com/jdx/hk");
        newer["predicate"]["version"] = json!("3.0.0");
        f.github_hk(f.signed(&newer).as_bytes());
        let reason = "its packslip for github.com/jdx/hk has predicate type https://packslip.dev/release/v2, which this version of packslip cannot read";
        let error = f.run(f.hk_request(None)).unwrap_err().to_string();
        assert_eq!(
            error,
            hk_outcome(
                "2.3.0",
                &format!("skipped 3.0.0, GitHub's latest release: {reason}")
            )
        );
        let error = f.run(f.hk_request(Some("v3.0.0"))).unwrap_err().to_string();
        assert_eq!(
            error,
            format!("no installable release of github.com/jdx/hk; skipped 3.0.0: {reason}")
        );
    }

    #[test]
    fn host_list_with_no_releases_offers_nothing_to_install() {
        let f = Fixture::new();
        // A list with no releases carries only the placeholder subject.
        let now = jiff::Timestamp::now();
        let list = json!({"_type":crate::model::STATEMENT_TYPE,"predicateType":crate::model::RELEASES_PREDICATE_TYPE,
            "subject":[{"name":"_","digest":{"sha256":hex::encode(Sha256::digest(b""))}}],
            "predicate":{"project":"example.test","identity":f.statement["predicate"]["identity"],
                "generated_at":now.to_string(),"expires_at":(now+std::time::Duration::from_secs(86400)).to_string(),
                "sequence":8,"releases":[]}});
        f.cache(
            "https://example.test/.well-known/packslip.json",
            true,
            f.signed(&list).as_bytes(),
        );
        for version in [None, Some("1.0.0")] {
            let mut request = f.request(true);
            request.version = version.map(Into::into);
            let err = f.run(request).unwrap_err().to_string();
            assert!(err.contains("no eligible release found"), "{err}");
        }
    }

    #[test]
    fn force_cannot_bypass_release_signature_digest_or_version_binding() {
        let mut f = Fixture::new();
        let report = f.run(f.request(true)).unwrap();
        f.statement["predicate"]["version"] = json!("2.0.0");
        f.publish(8, false);
        let mut request = f.request(false);
        request.force = true;
        assert!(f.run(request).unwrap_err().to_string().contains("version"));
        f.statement["predicate"]["version"] = json!("1.0.0");
        f.statement["subject"][0]["digest"]["sha256"] = json!("a".repeat(64));
        f.publish(9, false);
        let mut request = f.request(false);
        request.force = true;
        assert!(f.run(request).unwrap_err().to_string().contains("digest"));
        let mut bundle: serde_json::Value = serde_json::from_str(&f.signed(&f.statement)).unwrap();
        bundle["dsseEnvelope"]["signatures"][0]["sig"] = json!("AAAA");
        f.cache(&f.bundle_url, true, bundle.to_string().as_bytes());
        let mut request = f.request(false);
        request.force = true;
        assert!(f.run(request).is_err());
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
    }

    #[test]
    fn key_rotation_requires_exact_list_and_release_approvals() {
        let mut f = Fixture::new();
        f.run(f.request(true)).unwrap();
        f.key = crate::minisign::SecretKey::from_seed([20; 32]);
        f.statement["predicate"]["identity"]["key_id"] =
            json!(crate::minisign::key_id_hex(&f.key.public_key().key_id));
        f.publish(8, false);
        let mut request = f.request(true);
        request.force = true;
        let error = f.run(request).unwrap_err();
        let install_policy::Error::Change { id: list_id, .. } =
            error.downcast_ref::<install_policy::Error>().unwrap()
        else {
            panic!("{error:?}")
        };
        let list_id = list_id.clone();
        let mut request = f.request(true);
        request.accept_trust_change = vec![list_id.clone()];
        let error = f.run(request).unwrap_err();
        let install_policy::Error::Change { id: release_id, .. } =
            error.downcast_ref::<install_policy::Error>().unwrap()
        else {
            panic!("{error:?}")
        };
        let release_id = release_id.clone();
        let mut request = f.request(true);
        request.accept_trust_change = vec![list_id, release_id];
        f.run(request).unwrap();
        f.run(f.request(false)).unwrap();
    }

    #[test]
    fn caller_and_administrator_constraints_are_independent_of_force() {
        let f = Fixture::new();
        let report = f.run(f.request(true)).unwrap();
        let pins = f.scope().admin.join("pins.d");
        test_dirs(&pins).unwrap();
        let other = crate::minisign::SecretKey::from_seed([99; 32])
            .public_key()
            .to_file();
        std::fs::write(
            pins.join("policy.toml"),
            format!(
                "[projects.\"example.test\"]\npubkey = {}\n",
                toml::Value::String(other)
            ),
        )
        .unwrap();
        let mut request = f.request(true);
        request.force = true;
        assert!(f.run(request).unwrap_err().to_string().contains("disagree"));
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
    }
    #[test]
    fn recreated_repository_name_cannot_become_first_use_even_with_force() {
        let f = Fixture::new();
        let report = f.run(f.request(true)).unwrap();
        // Remember `name` as the repository with ID 42, under the key `key`.
        let remember = |name: &str, key: &str| {
            let scope = f.scope();
            let session = install_fs::Session::open_for(
                &scope.state,
                &report.receipt.tree,
                &scope.bin,
                &report.receipt.project,
            )
            .unwrap();
            let mut prior = session.history(&report.receipt.project).unwrap();
            let record = prior.release.as_mut().unwrap();
            record.project = name.into();
            record.repository = Some(
                serde_json::from_value(json!({
                    "project": name, "repository_id": "42"
                }))
                .unwrap(),
            );
            session.remember(key, &prior).unwrap();
        };
        // Ask for `name`, which GitHub now resolves to ID 43.
        let refused = |name: &str| {
            let base = format!(
                "https://api.github.com/repos/{}",
                name.strip_prefix("github.com/").unwrap()
            );
            for (url, value) in [
                (base.clone(), json!({"id":43,"default_branch":"main"})),
                (
                    format!("{base}/releases/latest"),
                    json!({"tag_name":"v1.0.0","draft":false}),
                ),
                (format!("{base}/releases?per_page=100&page=1"), json!([])),
            ] {
                f.cache(&url, false, &serde_json::to_vec(&value).unwrap());
            }
            let mut request = f.request(false);
            request.project = name.into();
            request.force = true;
            f.run(request)
                .unwrap_err()
                .to_string()
                .contains("different remembered repository ID")
        };
        remember("github.com/owner/tool", "github.com/owner/tool");
        assert!(refused("github.com/owner/tool"));
        // GitHub resolves a name in any case, so the remembered name is
        // remembered in every case.
        assert!(refused("github.com/Owner/Tool"));
        // A history written before names folded case is under the exact
        // spelling, and still counts.
        remember("github.com/Legacy/Tool", "github.com/Legacy/Tool");
        assert!(refused("github.com/Legacy/Tool"));
        // It counts for the name in any other case too: its record names the
        // spelling it is kept under.
        remember("github.com/Older/Tool", "github.com/Older/Tool");
        assert!(refused("github.com/older/TOOL"));
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
    }

    #[test]
    fn a_domain_projects_repository_pin_is_held_by_continuity_not_by_name() {
        let f = Fixture::new();
        let report = f.run(f.request(true)).unwrap();
        {
            let scope = f.scope();
            let session = install_fs::Session::open_for(
                &scope.state,
                &report.receipt.tree,
                &scope.bin,
                &report.receipt.project,
            )
            .unwrap();
            // The fixture signs with a key, which no certificate ties to a
            // repository.
            let mut prior = session.history(&report.receipt.project).unwrap();
            assert!(prior.list.as_ref().unwrap().repository.is_none());
            assert!(prior.release.as_ref().unwrap().repository.is_none());
            // A keyless domain project's records name the repository that
            // signed it. The project's name resolves to no repository, so
            // install holds the project to that pin, and does not refuse the
            // name as one that now resolves to another repository.
            let pin = crate::forge::ForgePin::of("github.com/jdx/tool", "42");
            prior.list.as_mut().unwrap().repository = Some(pin.clone());
            prior.release.as_mut().unwrap().repository = Some(pin);
            session.remember(&report.receipt.project, &prior).unwrap();
            session.remember("example.test", &prior).unwrap();
        }
        let error = f.run(f.request(false)).unwrap_err();
        let Some(install_policy::Error::Change { reasons, .. }) = error.downcast_ref() else {
            panic!("{error:?}")
        };
        assert!(
            reasons.contains(&"repository pin lost: github.com/jdx/tool (ID 42)".into()),
            "{reasons:?}"
        );
    }

    #[test]
    fn alias_history_folds_the_case_of_a_github_repository_name() {
        assert_eq!(alias_key("github.com/JDX/Mise"), "github.com/jdx/mise");
        assert_eq!(
            alias_key("github.com/Oxc-Project/OXC/OxLint"),
            "github.com/oxc-project/oxc/OxLint"
        );
        assert_eq!(alias_key("tool.example.com/Tool"), "tool.example.com/Tool");
    }

    #[test]
    fn public_key_files_are_read_once_and_the_key_is_remembered() {
        let f = Fixture::new();
        let path = f.root.path().join("release.pub");
        std::fs::write(&path, f.key.public_key().to_file()).unwrap();
        let mut request = f.request(false);
        request.constraints.pubkey = Some(path.to_str().unwrap().into());
        let report = f.run(request).unwrap();
        std::fs::remove_file(&path).unwrap();
        f.run(f.request(false)).unwrap();
        let mut request = f.request(false);
        request.constraints.pubkey = Some(path.to_str().unwrap().into());
        request.force = true;
        assert!(
            f.run(request)
                .unwrap_err()
                .to_string()
                .contains("public-key file is missing")
        );
        test_dirs(&path).unwrap();
        let mut request = f.request(false);
        request.constraints.pubkey = Some(path.to_str().unwrap().into());
        assert!(f.run(request).is_err());
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
    }

    #[cfg(unix)]
    #[test]
    fn configuration_fifo_is_rejected_without_waiting_for_a_writer() {
        let root = crate::install_extract::private_builder(".tmp")
            .tempdir()
            .unwrap();
        let path = root.path().join("key.pub");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            read_optional(&path)
                .unwrap_err()
                .to_string()
                .contains("regular file")
        );
    }

    #[test]
    fn unsupported_forge_is_reported_before_scope_or_network_access() {
        let f = Fixture::new();
        let mut request = f.request(true);
        request.project = "gitlab.com/group/subgroup/tool".into();
        request.system = true;
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(request))
            .unwrap_err();
        assert!(error.to_string().contains("supports github.com"));
    }

    #[test]
    fn immutable_repo_identity_and_subpath_define_ownership() {
        assert_eq!(
            installation_key("github.com/old/name/tool", Some("42")),
            installation_key("github.com/new/renamed/tool", Some("42"))
        );
        assert_ne!(
            installation_key("github.com/old/name/tool", Some("42")),
            installation_key("github.com/old/name/tool", Some("43"))
        );
        assert_ne!(
            installation_key("github.com/old/name/tool", Some("42")),
            installation_key("github.com/old/name/other", Some("42"))
        );
    }
    #[test]
    fn scopes_are_exclusive() {
        assert!(Scope::detect(true, true).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn unix_system_scope_uses_fixed_state_and_ignores_user_configuration() {
        let scope = Scope::detect(true, false).unwrap();
        assert_eq!(scope.state, PathBuf::from("/var/lib/packslip"));
        assert_eq!(scope.installs, PathBuf::from("/opt/packslip"));
        assert_eq!(scope.bin, PathBuf::from("/usr/local/bin"));
        assert!(scope.config.is_none());
    }
    #[cfg(windows)]
    #[test]
    fn windows_user_and_system_scopes_have_separate_persistent_state() {
        let user = Scope::detect(false, true).unwrap();
        let system = Scope::detect(true, false).unwrap();
        assert_ne!(user.state, system.state);
        assert_ne!(user.installs, system.installs);
        assert!(user.config.is_some());
        assert!(system.config.is_none());
        assert_eq!(system.state, system.admin.join("state"));
    }
}
