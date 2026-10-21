//! Bootstrap discovery and HTTPS transport. Fetched documents remain untrusted
//! until the caller verifies the bundle, project, version, and list digest.
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, Write};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid project {0:?}: use github.com/owner/repo, owner/repo, or a host name")]
    Project(String),
    #[error("invalid HTTPS URL or credential origin")]
    Url,
    #[error("HTTPS request failed for {0}")]
    Network(String),
    #[error("HTTP {status} for {url}")]
    Status { status: u16, url: String },
    #[error("download exceeds {limit} bytes: {url}")]
    Limit { limit: u64, url: String },
    #[error("too many HTTPS redirects")]
    Redirects,
    #[error("offline cache is missing {0}")]
    Offline(String),
    #[error("download cache: {0}")]
    Io(#[from] std::io::Error),
    #[error("discovery JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("discovery version is not strict semver: {0:?}")]
    Version(String),
    #[error("no eligible release found")]
    NoRelease,
    #[error("release {0} was withdrawn")]
    Yanked(String),
    #[error("release list is stale, missing, or rolled back")]
    StaleList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project(String);
impl Project {
    /// Parse a project the user asks for into a project name, which follows
    /// the specification's grammar ([`crate::model::valid_project`]).
    /// `owner/repo[/subpath]`, whose first segment has no dot and so is not
    /// a host, is short for a GitHub repository. The host's case is folded,
    /// as DNS ignores it; the path keeps its case.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let name = match input.split_once('/') {
            Some((first, _)) if !first.contains('.') => format!("github.com/{input}"),
            _ => input.to_owned(),
        };
        let name = match name.split_once('/') {
            Some((host, path)) => format!("{}/{path}", host.to_ascii_lowercase()),
            None => name.to_ascii_lowercase(),
        };
        // A GitHub project names a repository, not the forge itself or an
        // owner.
        if !crate::model::valid_project(&name)
            || (crate::model::project_host(&name) == "github.com"
                && crate::model::repository(&name).is_none())
        {
            return Err(Error::Project(input.into()));
        }
        Ok(Self(name))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn github_repo(&self) -> Option<String> {
        crate::model::repository(&self.0).map(|(_, o, r)| format!("{o}/{r}"))
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Auth {
    pub bearer_token_env: String,
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HttpConfig {
    #[serde(default)]
    pub auth: BTreeMap<String, Auth>,
}

/// A command-scoped client. Redirects are followed manually so credentials are
/// reconsidered at every exact HTTPS origin, including scheme and port.
pub struct Client {
    inner: reqwest::Client,
    auth: BTreeMap<String, String>,
    cache: PathBuf,
    offline: bool,
    download_timeout: std::time::Duration,
}
fn origin(url: &Url) -> String {
    url.origin().ascii_serialization()
}
fn parse_url(input: &str) -> Result<Url, Error> {
    let url = Url::parse(input).map_err(|_| Error::Url)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(Error::Url);
    }
    Ok(url)
}
fn display_url(url: &Url) -> String {
    format!("{}{}", origin(url), url.path())
}
impl Client {
    pub fn new(cache: PathBuf, offline: bool, config: HttpConfig) -> Result<Self, Error> {
        let inner = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(60))
            .user_agent(concat!("packslip/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| Error::Network("client initialization".into()))?;
        let mut auth = BTreeMap::new();
        if let Some(token) = std::env::var("GH_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()))
        {
            for host in ["https://github.com", "https://api.github.com"] {
                auth.insert(host.into(), token.clone());
            }
        }
        for (name, value) in config.auth {
            let url = parse_url(&name)?;
            if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
                return Err(Error::Url);
            }
            if let Ok(token) = std::env::var(&value.bearer_token_env) {
                auth.insert(origin(&url), token);
            }
        }
        Ok(Self {
            inner,
            auth,
            cache,
            offline,
            download_timeout: std::time::Duration::from_secs(24 * 60 * 60),
        })
    }
    /// Bound the entire artifact transfer, including redirects, while the
    /// per-read timeout still rejects a stalled connection. Default: 24 hours.
    pub fn with_download_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.download_timeout = timeout;
        self
    }
    fn cache_path(&self, url: &Url) -> PathBuf {
        self.cache
            .join(hex::encode(sha2::Sha256::digest(url.as_str().as_bytes())))
    }
    /// Fetch at most `limit` bytes, with no network at all in offline mode.
    /// Only 404 means an optional document is absent; malformed/failed ones fail.
    pub async fn fetch(&self, input: &str, limit: u64) -> Result<Option<Vec<u8>>, Error> {
        self.fetch_under(input, limit, None).await
    }
    fn host_cache_path(&self, url: &Url) -> PathBuf {
        self.cache.join(format!(
            "host-{}",
            hex::encode(sha2::Sha256::digest(url.as_str().as_bytes()))
        ))
    }
    async fn fetch_under(
        &self,
        input: &str,
        limit: u64,
        host_origin: Option<&str>,
    ) -> Result<Option<Vec<u8>>, Error> {
        let initial = parse_url(input)?;
        // Host first-use authority has its own cache namespace; bytes fetched
        // with the general cross-origin redirect policy cannot gain authority.
        let path = if host_origin.is_some() {
            self.host_cache_path(&initial)
        } else {
            self.cache_path(&initial)
        };
        if self.offline {
            if path.with_extension("missing").is_file() && !path.is_file() {
                return Ok(None);
            }
            let file = std::fs::File::open(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::Offline(display_url(&initial))
                } else {
                    e.into()
                }
            })?;
            let mut bytes = Vec::new();
            file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > limit {
                return Err(Error::Limit {
                    limit,
                    url: display_url(&initial),
                });
            }
            return Ok(Some(bytes));
        }
        let mut current = initial.clone();
        for redirects in 0..=10 {
            let mut request = self
                .inner
                .get(current.clone())
                .timeout(std::time::Duration::from_secs(60));
            if let Some(token) = self.auth.get(&origin(&current)) {
                request = request.bearer_auth(token);
            }
            let mut response = request
                .send()
                .await
                .map_err(|_| Error::Network(display_url(&current)))?;
            if response.status().is_redirection() {
                if redirects == 10 {
                    return Err(Error::Redirects);
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|h| h.to_str().ok())
                    .ok_or(Error::Url)?;
                current = parse_url(current.join(location).map_err(|_| Error::Url)?.as_str())?;
                if host_origin.is_some_and(|allowed| origin(&current) != allowed) {
                    return Err(Error::Url);
                }
                continue;
            }
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                create_cache(&self.cache)?;
                std::fs::write(path.with_extension("missing"), b"404")?;
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
                return Ok(None);
            }
            if !response.status().is_success() {
                return Err(Error::Status {
                    status: response.status().as_u16(),
                    url: display_url(&current),
                });
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| Error::Network(display_url(&current)))?
            {
                if bytes.len() as u64 + chunk.len() as u64 > limit {
                    return Err(Error::Limit {
                        limit,
                        url: display_url(&current),
                    });
                }
                bytes.extend_from_slice(&chunk);
            }
            create_cache(&self.cache)?;
            let mut tmp = tempfile::NamedTempFile::new_in(&self.cache)?;
            tmp.write_all(&bytes)?;
            tmp.as_file().sync_all()?;
            tmp.persist(&path).map_err(|e| e.error)?;
            if path.with_extension("missing").exists() {
                std::fs::remove_file(path.with_extension("missing"))?;
            }
            return Ok(Some(bytes));
        }
        Err(Error::Redirects)
    }
    /// Stream a potentially large artifact to the command cache, enforcing the
    /// signed size while reading. The returned file owns a private snapshot;
    /// replacing the URL-keyed cache cannot change its bytes. Keep it alive
    /// through digest verification and extraction, using its path or file handle.
    pub async fn download(
        &self,
        input: &str,
        expected_size: u64,
    ) -> Result<tempfile::NamedTempFile, Error> {
        let initial = parse_url(input)?;
        let path = self.cache_path(&initial);
        if self.offline {
            let snapshot =
                snapshot(&path, &self.cache, expected_size.saturating_add(1)).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        Error::Offline(display_url(&initial))
                    } else {
                        Error::Io(error)
                    }
                })?;
            let metadata = snapshot.as_file().metadata()?;
            if !metadata.is_file() || metadata.len() != expected_size {
                return Err(Error::Limit {
                    limit: expected_size,
                    url: display_url(&initial),
                });
            }
            return Ok(snapshot);
        }
        let deadline = std::time::Instant::now()
            .checked_add(self.download_timeout)
            .ok_or(Error::Url)?;
        let mut current = initial.clone();
        for redirects in 0..=10 {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(Error::Network(display_url(&current)));
            }
            let mut request = self.inner.get(current.clone()).timeout(remaining);
            if let Some(token) = self.auth.get(&origin(&current)) {
                request = request.bearer_auth(token);
            }
            let mut response = request
                .send()
                .await
                .map_err(|_| Error::Network(display_url(&current)))?;
            if response.status().is_redirection() {
                if redirects == 10 {
                    return Err(Error::Redirects);
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|h| h.to_str().ok())
                    .ok_or(Error::Url)?;
                current = parse_url(current.join(location).map_err(|_| Error::Url)?.as_str())?;
                continue;
            }
            if !response.status().is_success() {
                return Err(Error::Status {
                    status: response.status().as_u16(),
                    url: display_url(&current),
                });
            }
            create_cache(&self.cache)?;
            let mut tmp = tempfile::NamedTempFile::new_in(&self.cache)?;
            let mut size = 0u64;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| Error::Network(display_url(&current)))?
            {
                size = size
                    .checked_add(chunk.len() as u64)
                    .ok_or_else(|| Error::Limit {
                        limit: expected_size,
                        url: display_url(&current),
                    })?;
                if size > expected_size {
                    return Err(Error::Limit {
                        limit: expected_size,
                        url: display_url(&current),
                    });
                }
                tmp.write_all(&chunk)?;
            }
            if size != expected_size {
                return Err(Error::Limit {
                    limit: expected_size,
                    url: display_url(&current),
                });
            }
            return Ok(publish_download(tmp, &path)?);
        }
        Err(Error::Redirects)
    }
}

/// Publish a separate cached snapshot while retaining the owned file, rewound
/// for reading. Atomic replacement leaves existing callers' snapshots intact.
fn publish_download(
    mut file: tempfile::NamedTempFile,
    cache: &std::path::Path,
) -> Result<tempfile::NamedTempFile, std::io::Error> {
    file.as_file().sync_all()?;
    snapshot(
        file.path(),
        cache
            .parent()
            .ok_or_else(|| std::io::Error::other("cache has no parent"))?,
        file.as_file().metadata()?.len().saturating_add(1),
    )?
    .persist(cache)
    .map_err(|error| error.error)?;
    file.as_file_mut().rewind()?;
    Ok(file)
}

/// Open a regular cache file without following a Unix symlink or Windows
/// reparse point, then validate the opened object instead of its pathname.
fn cache_file(input: &std::path::Path) -> Result<std::fs::File, std::io::Error> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags};
        std::fs::File::from(rustix::fs::open(
            input,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?)
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
            .open(input)?
    };
    #[cfg(not(any(unix, windows)))]
    let file = std::fs::File::open(input)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "artifact cache is not a regular file",
        ));
    }
    Ok(file)
}

/// Copy at most `limit` bytes through open handles into an exclusively created
/// snapshot. Keeping the destination open avoids a pathname replacement race.
/// The caller checks signed size on the completed, synchronized, rewound file.
fn snapshot(
    input: &std::path::Path,
    parent: &std::path::Path,
    limit: u64,
) -> Result<tempfile::NamedTempFile, std::io::Error> {
    let source = cache_file(input)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut source.take(limit), file.as_file_mut())?;
    file.as_file().sync_all()?;
    file.as_file_mut().rewind()?;
    Ok(file)
}

/// Bytes obtained through the named host's authenticated HTTPS origin (or its
/// local offline cache). The private fields prevent arbitrary supplied bundles
/// from claiming that transport authority. Redirects stay within that origin;
/// general HTTPS downloads may redirect when independent pins establish trust.
#[derive(Debug)]
pub struct HostDocument {
    project: Project,
    bytes: Vec<u8>,
}
impl HostDocument {
    pub fn project(&self) -> &str {
        self.project.as_str()
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Client {
    pub async fn fetch_host(
        &self,
        project: &Project,
        input: &str,
        limit: u64,
    ) -> Result<Option<HostDocument>, Error> {
        let url = parse_url(input)?;
        let host = project.as_str().split('/').next().ok_or(Error::Url)?;
        if url.host_str() != Some(host)
            || url.port_or_known_default() != Some(443)
            || crate::sigstore::Policy::for_project(project.as_str()).is_some()
        {
            return Err(Error::Url);
        }
        Ok(self
            .fetch_under(input, limit, Some(&origin(&url)))
            .await?
            .map(|bytes| HostDocument {
                project: project.clone(),
                bytes,
            }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub bundles: Vec<String>,
    /// True for the release the recommendation names: GitHub's latest
    /// release, or the signed list's entry for the recommended version.
    /// Another release whose tag names the same version, such as a signed
    /// list entry that gives the version another tag, is not marked.
    pub recommended: bool,
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// GitHub's live release view and the default branch's supplementary-list URL.
/// Pages are bounded; a larger repository fails rather than exposing a silently
/// truncated version history. Assets still need verified project matching.
pub struct Github {
    pub releases: Vec<Release>,
    pub list_url: String,
    /// Keep the forge hint even when only the signed list maps its tag.
    pub latest_tag: Option<String>,
    pub repository_id: String,
}

impl Github {
    pub fn choose(
        &self,
        list: Option<&crate::ReleaseListStatement>,
        requested: Option<&str>,
    ) -> Result<Release, Error> {
        choose_with_latest(&self.releases, list, requested, self.latest_tag.as_deref())
    }
    /// Every release the request may install for `project`, in the order to
    /// try them. See [`candidates_with_latest`].
    pub fn candidates(
        &self,
        project: &str,
        list: Option<&crate::ReleaseListStatement>,
        requested: Option<&str>,
    ) -> Result<Candidates, Error> {
        candidates_with_latest(
            project,
            &self.releases,
            list,
            requested,
            self.latest_tag.as_deref(),
        )
    }
}

/// Why a release with no packslip for `project` was skipped, whether
/// discovery saw no packslip asset at all or the consumer fetched every
/// bundle and none named the project.
pub fn no_packslip(project: &str) -> String {
    format!("its release carries no packslip for {project}")
}

/// A release that selection passed over, and why. The specification's
/// Latest rules require a consumer to report a skipped recommendation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Skipped {
    /// The release as a user knows it, such as
    /// `2.0.0 (tag oxc_v2.0.0), GitHub's latest release`.
    pub release: String,
    /// Why it was passed over, such as `it is withdrawn`.
    pub reason: String,
    /// True when the release is a latest request's recommendation, which a
    /// report names on its own rather than in a run of other releases.
    pub recommendation: bool,
}

impl Skipped {
    /// A skipped release that is not the recommendation.
    pub fn new(release: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            release: release.into(),
            reason: reason.into(),
            recommendation: false,
        }
    }
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.release, self.reason)
    }
}

/// The releases a request may install, in the order a consumer tries them.
///
/// For an unconstrained `latest` request, the eligible recommendation comes
/// first, then every other eligible release by descending semver precedence,
/// including another release that shares the recommendation's version.
/// For a version, prefix, or range request, the matching releases come by
/// descending precedence. An exact tag names its release alone. Otherwise,
/// releases that share a version follow one another in the forge's order.
///
/// Whether a forge release carries a packslip for the requested project is
/// known only once its bundles are fetched, so a consumer skips such a
/// release and tries the next one, whatever the request.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Candidates {
    pub releases: Vec<Release>,
    /// True for an unconstrained `latest` request. Only then does a release
    /// with no artifact for the host count as ineligible, so the consumer
    /// tries the next release instead of failing.
    pub latest: bool,
    /// The recommendation, when selection set it aside before any bundle
    /// was fetched: it is withdrawn, it is a prerelease, or its forge release
    /// carries no packslip.
    pub skipped: Vec<Skipped>,
    /// Who made the recommendation, for a latest request only: no other
    /// request gives the recommendation a place, so no report names it.
    recommended_by: Option<&'static str>,
    /// The tag of the one candidate that is the recommendation itself.
    recommendation: Option<String>,
    /// Versions that more than one tag names, among forge releases and
    /// signed list entries.
    shared: BTreeSet<String>,
}

impl Candidates {
    /// Name a release for a report: its version, its tag when that spells
    /// the version differently or another release's tag names the same
    /// version, and whether it is a latest request's recommendation.
    pub fn describe(&self, release: &Release) -> String {
        describe(
            release,
            self.role(release),
            self.shared.contains(&release.version),
        )
    }

    /// Report `release` as skipped for `reason`.
    pub fn skip(&self, release: &Release, reason: impl Into<String>) -> Skipped {
        Skipped {
            recommendation: self.role(release).is_some(),
            ..Skipped::new(self.describe(release), reason)
        }
    }

    fn role(&self, release: &Release) -> Option<&'static str> {
        self.recommended_by
            .filter(|_| self.recommendation.as_deref() == Some(release.tag.as_str()))
    }
}

/// The tag of the candidate a report calls the recommendation: the one
/// release the recommendation names, whatever else shares its version.
fn recommendation(releases: &[Release]) -> Option<String> {
    releases
        .iter()
        .find(|r| r.recommended)
        .map(|r| r.tag.clone())
}

/// Versions that more than one tag names, among forge releases and signed
/// list entries, such as 2.0.0 for a release-please root release `v2.0.0`
/// and the tool's own `tool-v2.0.0`. A report names those releases by tag
/// as well.
fn shared_versions(
    live: &[Release],
    list: Option<&crate::ReleaseListStatement>,
) -> BTreeSet<String> {
    let mut tags: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for release in live {
        tags.entry(&release.version)
            .or_default()
            .insert(&release.tag);
    }
    for entry in list.iter().flat_map(|list| &list.predicate.releases) {
        if let Some(tag) = &entry.tag {
            tags.entry(&entry.version).or_default().insert(tag);
        }
    }
    tags.into_iter()
        .filter(|(_, tags)| tags.len() > 1)
        .map(|(version, _)| version.to_owned())
        .collect()
}

fn describe(release: &Release, role: Option<&str>, shared: bool) -> String {
    let mut text = release.version.clone();
    if shared
        || (release.tag != release.version
            && release.tag.strip_prefix('v') != Some(&release.version))
    {
        text = format!("{text} (tag {})", release.tag);
    }
    if let Some(role) = role {
        text = format!("{text}, {role}");
    }
    text
}

pub async fn github(client: &Client, project: &Project) -> Result<Github, Error> {
    let repo = project
        .github_repo()
        .ok_or_else(|| Error::Project(project.0.clone()))?;
    let url = format!("https://api.github.com/repos/{repo}");
    let info: serde_json::Value = serde_json::from_slice(
        &client
            .fetch(&url, 2 * 1024 * 1024)
            .await?
            .ok_or(Error::NoRelease)?,
    )?;
    let branch = info["default_branch"].as_str().ok_or(Error::NoRelease)?;
    let repository_id = info["id"].as_u64().ok_or(Error::NoRelease)?.to_string();
    let mut list_url = Url::parse("https://api.github.com").unwrap();
    {
        let mut segments = list_url.path_segments_mut().map_err(|_| Error::Url)?;
        segments.extend(["repos"]);
        segments.extend(repo.split('/'));
        segments.push("contents");
        segments.extend(
            crate::github_list_path(project.as_str())
                .ok_or(Error::NoRelease)?
                .split('/'),
        );
    }
    list_url.query_pairs_mut().append_pair("ref", branch);
    let mut releases = Vec::new();
    let latest = client
        .fetch(&format!("{url}/releases/latest"), 2 * 1024 * 1024)
        .await?;
    let latest: Option<GithubRelease> = latest.map(|b| serde_json::from_slice(&b)).transpose()?;
    for page in 1..=100 {
        let bytes = client
            .fetch(
                &format!("{url}/releases?per_page=100&page={page}"),
                8 * 1024 * 1024,
            )
            .await?
            .ok_or(Error::NoRelease)?;
        let page_releases: Vec<GithubRelease> = serde_json::from_slice(&bytes)?;
        let done = page_releases.len() < 100;
        for release in page_releases.into_iter().filter(|r| !r.draft) {
            if let Some(version) = crate::tag_version(&release.tag_name, project.as_str()) {
                let bundles: Vec<_> = release
                    .assets
                    .into_iter()
                    .filter(|a| {
                        a.name.starts_with("packslip") && a.name.ends_with(".sigstore.json")
                    })
                    .map(|a| a.browser_download_url)
                    .collect();
                releases.push(Release {
                    version: version.to_string(),
                    recommended: latest
                        .as_ref()
                        .is_some_and(|l| l.tag_name == release.tag_name),
                    tag: release.tag_name,
                    bundles,
                });
            }
        }
        if done {
            return Ok(Github {
                releases,
                list_url: list_url.to_string(),
                latest_tag: latest.map(|r| r.tag_name),
                repository_id,
            });
        }
    }
    Err(Error::Limit {
        limit: 10_000,
        url: format!("{url}/releases"),
    })
}

/// Decode the GitHub contents API response; errors are never absence.
pub async fn github_list(client: &Client, url: &str) -> Result<Option<Vec<u8>>, Error> {
    use base64::Engine as _;
    let Some(bytes) = client.fetch(url, 16 * 1024 * 1024).await? else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if value["encoding"] != "base64" {
        return Err(Error::NoRelease);
    }
    let encoded = value["content"]
        .as_str()
        .ok_or(Error::NoRelease)?
        .replace(['\n', '\r'], "");
    Ok(Some(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| Error::NoRelease)?,
    ))
}

/// Check an already verified list before selecting even an explicit version.
pub fn list_freshness(
    list: Option<&crate::ReleaseListStatement>,
    previous_sequence: Option<u64>,
    now: jiff::Timestamp,
) -> Result<(), Error> {
    match list {
        None if previous_sequence.is_some() => Err(Error::StaleList),
        Some(list)
            if !list.is_current(now)
                || previous_sequence.is_some_and(|n| list.predicate.sequence < n) =>
        {
            Err(Error::StaleList)
        }
        _ => Ok(()),
    }
}
/// Merge list entries over GitHub tags; signed entries always decide withdrawal,
/// tag, URL, and vendor recommendation. An explicit version never revives a yank.
/// Numeric one/two-component selectors retain prefix semantics even if a forge
/// also has a tag with that spelling. Other exact tag aliases retain their URLs.
pub fn choose(
    live: &[Release],
    list: Option<&crate::ReleaseListStatement>,
    requested: Option<&str>,
) -> Result<Release, Error> {
    choose_with_latest(live, list, requested, None)
}

/// Like [`choose`], preserving a GitHub latest tag that the signed list maps
/// to a version even when the forge tag itself is not semver.
pub fn choose_with_latest(
    live: &[Release],
    list: Option<&crate::ReleaseListStatement>,
    requested: Option<&str>,
    latest_tag: Option<&str>,
) -> Result<Release, Error> {
    candidates_with_latest("the requested project", live, list, requested, latest_tag)?
        .releases
        .into_iter()
        .next()
        .ok_or(Error::NoRelease)
}

/// Every release the request may install for `project`, in the order to
/// try them, with the same merging, withdrawal, and matching rules as
/// [`choose_with_latest`], whose choice is the first candidate. A consumer
/// moves to the next candidate only when one is absent or ineligible, never
/// after a fetch or verification failure. `project` names the tool in the
/// reason for a skipped recommendation.
pub fn candidates_with_latest(
    project: &str,
    live: &[Release],
    list: Option<&crate::ReleaseListStatement>,
    requested: Option<&str>,
    latest_tag: Option<&str>,
) -> Result<Candidates, Error> {
    for release in live {
        semver::Version::parse(&release.version)
            .map_err(|_| Error::Version(release.version.clone()))?;
    }
    if let Some(list) = list {
        for entry in &list.predicate.releases {
            semver::Version::parse(&entry.version)
                .map_err(|_| Error::Version(entry.version.clone()))?;
        }
    }
    // Keep assetless forge tags as metadata for list tag mapping and yanks;
    // filter them from installable choices after merging the signed list.
    let numeric = requested.and_then(numeric_request);
    let numeric_prefix = numeric
        .as_ref()
        .is_some_and(|version| !version.contains(['-', '+']) && version.split('.').count() < 3);
    let exact_tag = requested
        .filter(|_| !numeric_prefix)
        .and_then(|tag| live.iter().find(|release| release.tag == tag));
    let mut choices: BTreeMap<String, Release> = BTreeMap::new();
    // Another forge release with the same version, such as a monorepo
    // sibling whose tag carries the repository-name prefix, stays a
    // candidate after the one chosen above: only its bundles say which
    // project it describes.
    let mut alternates: BTreeMap<String, Vec<Release>> = BTreeMap::new();
    // A withdrawn build that a request names only by leaving out its build
    // metadata, reported if no release still matches the request.
    let mut yanked_build = None;
    for release in live {
        match choices.entry(release.version.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(release.clone());
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                // Each release keeps its own mark. Another release that
                // shares the recommended version is not the recommendation,
                // and it does not stand in when that release is dropped.
                if entry.get().bundles.is_empty() && !release.bundles.is_empty() {
                    entry.insert(release.clone());
                } else if !release.bundles.is_empty() {
                    alternates
                        .entry(release.version.clone())
                        .or_default()
                        .push(release.clone());
                }
            }
        }
    }
    if let Some(list) = list {
        for entry in &list.predicate.releases {
            // A signed entry pins one bundle for its version.
            alternates.remove(&entry.version);
            if entry.is_yanked() {
                let removed = choices.remove(&entry.version);
                if requested.is_some_and(|v| {
                    numeric.as_deref().unwrap_or(v) == entry.version
                        || exact_tag.is_some_and(|release| release.version == entry.version)
                        || (!numeric_prefix && entry.tag.as_deref() == Some(v))
                        || (!numeric_prefix && removed.as_ref().is_some_and(|r| r.tag == v))
                }) {
                    return Err(Error::Yanked(entry.version.clone()));
                }
                if requested
                    .is_some_and(|v| same_version(numeric.as_deref().unwrap_or(v), &entry.version))
                {
                    yanked_build.get_or_insert_with(|| entry.version.clone());
                }
                continue;
            }
            // GitHub's latest release, when it has the entry's version, is
            // the forge release of an entry that names no tag of its own.
            let github_latest = || {
                live.iter()
                    .find(|r| {
                        r.version == entry.version
                            && (r.recommended || latest_tag == Some(r.tag.as_str()))
                    })
                    .map(|r| r.tag.clone())
            };
            let tag = entry
                .tag
                .clone()
                .or_else(|| github_latest().filter(|_| list.predicate.latest.is_none()))
                .or_else(|| choices.get(&entry.version).map(|r| r.tag.clone()))
                .unwrap_or_else(|| entry.version.clone());
            choices.insert(
                entry.version.clone(),
                Release {
                    version: entry.version.clone(),
                    // GitHub's pointer marks only the release it names. An
                    // entry that gives the version another tag is not that
                    // release, so it does not inherit the mark.
                    recommended: list.predicate.latest.as_deref() == Some(&entry.version)
                        || (list.predicate.latest.is_none()
                            && (latest_tag == Some(tag.as_str())
                                || live.iter().any(|r| r.tag == tag && r.recommended))),
                    tag,
                    bundles: vec![entry.packslip.clone()],
                },
            );
        }
        // A signed entry that maps a forge tag decides which version the
        // tag names, so the version derived from that tag is no candidate
        // of its own: a vendor who tags `v1.2.3` and signs `1.2.3+build.5`
        // offers only the build, which install would refuse to take as
        // `1.2.3`. A withdrawn entry withdraws the tag that names it.
        let mapped: BTreeMap<&str, &str> = list
            .predicate
            .releases
            .iter()
            .filter_map(|entry| Some((entry.tag.as_deref()?, entry.version.as_str())))
            .collect();
        let unmapped = |version: &str, release: &Release| {
            mapped
                .get(release.tag.as_str())
                .is_none_or(|mapped| *mapped == version)
        };
        choices.retain(|version, release| unmapped(version, release));
        for (version, others) in &mut alternates {
            others.retain(|release| unmapped(version, release));
            // A mapped tag that dropped the version's own candidate leaves
            // its first remaining alternate as the candidate, so a sibling
            // release with its own bundles stays reachable.
            if !others.is_empty() && !choices.contains_key(version) {
                choices.insert(version.clone(), others.remove(0));
            }
        }
        if list.predicate.latest.is_some() {
            for choice in choices.values_mut() {
                choice.recommended = list.predicate.latest.as_deref() == Some(&choice.version);
            }
            for alternate in alternates.values_mut().flatten() {
                alternate.recommended = false;
            }
        }
    }
    choices.retain(|_, release| !release.bundles.is_empty());
    let signed_latest = list.is_some_and(|l| l.predicate.latest.is_some());
    let recommended_by = if signed_latest {
        Some("the signed list's recommendation")
    } else {
        latest_tag.map(|_| "GitHub's latest release")
    };
    let shared = shared_versions(live, list);
    let single = |release: Release| Candidates {
        recommendation: None,
        releases: vec![release],
        latest: false,
        skipped: Vec::new(),
        recommended_by: None,
        shared: shared.clone(),
    };
    // Order by descending precedence, each release followed by any others
    // that share its version.
    let ordered = |mut releases: Vec<Release>, alternates: &mut BTreeMap<String, Vec<Release>>| {
        releases.sort_by(|a, b| {
            semver::Version::parse(&b.version)
                .unwrap()
                .cmp(&semver::Version::parse(&a.version).unwrap())
        });
        let mut all = Vec::with_capacity(releases.len());
        for release in releases {
            let others = alternates.remove(&release.version).unwrap_or_default();
            all.push(release);
            all.extend(others);
        }
        all
    };
    if let Some(request) = requested.filter(|r| *r != "latest") {
        if let Some(exact) = exact_tag {
            // Signed entries remain authoritative over every forge tag alias,
            // and an entry that maps this tag decides which release it names:
            // a vendor who tags `1.2.3` and signs `1.2.3+build.5` maps the
            // tag to that build. Otherwise an exact tag retains that
            // release's own bundle URLs.
            let listed = list.and_then(|list| {
                let releases = &list.predicate.releases;
                releases
                    .iter()
                    .find(|entry| entry.version == exact.version)
                    .or_else(|| {
                        releases
                            .iter()
                            .find(|entry| entry.tag.as_deref() == Some(exact.tag.as_str()))
                    })
            });
            if let Some(entry) = listed {
                return choices
                    .get(&entry.version)
                    .cloned()
                    .map(single)
                    .ok_or(Error::NoRelease);
            }
            if !exact.bundles.is_empty() {
                return Ok(single(exact.clone()));
            }
            // A numeric version may select any installable tag for that
            // version; a named tag without its own bundle cannot substitute.
            if numeric.is_none() {
                return Err(Error::NoRelease);
            }
        }
        let matches: Vec<_> = choices
            .into_values()
            .filter(|r| matches_request(r, request))
            .collect();
        let releases: Vec<_> = ordered(matches, &mut alternates)
            .into_iter()
            .filter(|r| matches_request(r, request))
            .collect();
        if releases.is_empty() {
            return Err(yanked_build.map_or(Error::NoRelease, Error::Yanked));
        }
        return Ok(Candidates {
            recommendation: None,
            releases,
            latest: false,
            skipped: Vec::new(),
            recommended_by: None,
            shared,
        });
    }
    // The recommendation as discovery names it, before eligibility applies.
    let target = match list.and_then(|l| Some((l, l.predicate.latest.as_deref()?))) {
        Some((list, latest)) => list
            .predicate
            .releases
            .iter()
            .find(|entry| entry.version == latest)
            .map(|entry| Release {
                version: entry.version.clone(),
                tag: entry.tag.clone().unwrap_or_else(|| entry.version.clone()),
                bundles: Vec::new(),
                recommended: true,
            }),
        None => latest_tag.and_then(|tag| {
            list.and_then(|list| {
                list.predicate
                    .releases
                    .iter()
                    .find(|entry| entry.tag.as_deref() == Some(tag))
            })
            .map(|entry| Release {
                version: entry.version.clone(),
                tag: tag.to_owned(),
                bundles: Vec::new(),
                recommended: true,
            })
            .or_else(|| live.iter().find(|release| release.tag == tag).cloned())
        }),
    };
    let stable = |release: &Release| {
        semver::Version::parse(&release.version).is_ok_and(|v| v.pre.is_empty())
    };
    let mut eligible = ordered(
        choices.into_values().filter(stable).collect(),
        &mut alternates,
    );
    let mut skipped = Vec::new();
    if let Some(index) = eligible.iter().position(|r| r.recommended) {
        // Only the recommendation itself goes first. Another release that
        // shares its version keeps its place by precedence, so a higher
        // release is tried before it.
        let recommendation = eligible.remove(index);
        eligible.insert(0, recommendation);
    } else if let Some(target) = target {
        let reason = if list.is_some_and(|list| {
            list.predicate
                .releases
                .iter()
                .any(|entry| entry.version == target.version && entry.is_yanked())
        }) {
            "it is withdrawn".to_owned()
        } else if !stable(&target) {
            "it is a prerelease".to_owned()
        } else if let Some(tag) = list
            .and_then(|list| {
                list.predicate
                    .releases
                    .iter()
                    .find(|entry| entry.version == target.version)
            })
            .and_then(|entry| entry.tag.as_deref())
            .filter(|tag| *tag != target.tag)
        {
            // GitHub's latest release names a listed version, but the
            // signed entry gives that version another release.
            format!("the signed list maps {} to tag {tag}", target.version)
        } else {
            no_packslip(project)
        };
        skipped.push(Skipped {
            recommendation: true,
            ..Skipped::new(
                describe(&target, recommended_by, shared.contains(&target.version)),
                reason,
            )
        });
    }
    if eligible.is_empty() {
        return Err(Error::NoRelease);
    }
    Ok(Candidates {
        recommendation: recommendation(&eligible),
        releases: eligible,
        latest: true,
        skipped,
        recommended_by,
        shared,
    })
}

// Numeric selectors use the same leading-zero normalization as forge tags,
// while retaining missing components so `3.12` remains a prefix selector.
fn numeric_request(request: &str) -> Option<String> {
    let text = request.strip_prefix('v').unwrap_or(request);
    let (core, tail) = text
        .find(['-', '+'])
        .map_or((text, ""), |i| (&text[..i], &text[i..]));
    let parts: Vec<_> = core.split('.').collect();
    if !(1..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut numbers = parts
        .into_iter()
        .map(|part| {
            let value = part.trim_start_matches('0');
            if value.is_empty() { "0" } else { value }
        })
        .collect::<Vec<_>>();
    // A prerelease/build suffix applies to the fully spelled version, as
    // normalize_version specifies; it is not a major/minor-only prefix.
    if !tail.is_empty() && numbers.len() == 2 {
        numbers.push("0");
    }
    Some(format!("{}{tail}", numbers.join(".")))
}

// An exact request names a release whatever build metadata it carries, since
// build metadata is not a component: `1.2.3` is `1.2.3+build.5`.
fn same_version(requested: &str, version: &str) -> bool {
    version == requested
        || (!requested.contains('+')
            && version
                .split_once('+')
                .is_some_and(|(core, _)| core == requested))
}

fn matches_request(release: &Release, request: &str) -> bool {
    let numeric = numeric_request(request);
    let requested = numeric
        .as_deref()
        .unwrap_or_else(|| request.strip_prefix('v').unwrap_or(request));
    if (numeric.is_none()
        && (release.tag == request || release.tag.strip_prefix('v') == Some(requested)))
        || release.version == requested
    {
        return true;
    }
    let Ok(version) = semver::Version::parse(&release.version) else {
        return false;
    };
    if !version.pre.is_empty() && !requested.contains('-') {
        return false;
    }
    if requested.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && !requested.contains([' ', ',', '^', '~', '*', '<', '>', '='])
    {
        // A prefix ends at a component boundary: `3.12` matches `3.12.1` but
        // not `3.120.0`. Build metadata is not a component, so `1.2.3`
        // matches `1.2.3+build.5`, as `1.2` and `=1.2.3` do.
        return release.version.starts_with(requested)
            && matches!(
                release.version.as_bytes().get(requested.len()),
                Some(b'.' | b'+')
            );
    }
    semver::VersionReq::parse(requested).is_ok_and(|range| range.matches(&version))
}

/// Create the download cache and its missing parents as `rwxr-xr-x`, whatever
/// the umask. The cache usually sits in the installer's state directory, which
/// must not become group-writable under a user-private-group umask such as 002.
fn create_cache(path: &std::path::Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o755);
    }
    builder.create(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_intent_is_not_a_download_url() {
        assert_eq!(
            Project::parse("jdx/mise").unwrap().as_str(),
            "github.com/jdx/mise"
        );
        assert!(
            Project::parse("github.com/jdx/mise/tool")
                .unwrap()
                .github_repo()
                .is_some()
        );
        for input in [
            "https://github.com/jdx/mise",
            "http://evil.test/bundle",
            "github.com/jdx/../mise",
            "github.com/jdx/mise?x=1",
            "localhost",
            "/jdx/mise",
            "evil.test\\file",
            "github.com",
            "github.com/jdx",
            "jdx/mise/",
            // The specification's grammar: LDH host labels, no port, and
            // path segments of letters, digits, `.`, `_`, `~` and `-`.
            "exa_mple.com/tool",
            "-example.com/tool",
            "example.com:8443/tool",
            "example.com/a+b",
            "example.com/@scope/pkg",
        ] {
            assert!(Project::parse(input).is_err(), "{input}");
        }
        // Every name a statement may carry can be asked for.
        for name in [
            "example.org/tool_1.2~x",
            "github.com/jdx/mi~se",
            "gitlab.com/group/sub/proj",
            "mise.jdx.dev",
        ] {
            assert!(crate::model::valid_project(name), "{name}");
            assert_eq!(Project::parse(name).unwrap().as_str(), name);
        }
    }
    #[test]
    fn url_policy_and_redaction() {
        for input in [
            "http://example.com/file",
            "https://user:password@example.com/file",
        ] {
            assert!(parse_url(input).is_err());
        }
        assert_eq!(
            display_url(&parse_url("https://example.com/file?token=SECRET#SECRET").unwrap()),
            "https://example.com/file"
        );
        assert_ne!(
            origin(&parse_url("https://example.com:8443").unwrap()),
            origin(&parse_url("https://example.com").unwrap())
        );
    }
    fn signed_list() -> crate::ReleaseListStatement {
        serde_json::from_value(serde_json::json!({
            "_type": crate::model::STATEMENT_TYPE, "predicateType": crate::model::RELEASES_PREDICATE_TYPE,
            "subject": [], "predicate": { "project": "example.test", "generated_at": "2026-01-01T00:00:00Z", "expires_at": "2027-01-01T00:00:00Z", "sequence": 5,
                "identity": { "scheme": "sigstore-key", "key_id": "0011223344556677" },
                "releases": [{ "version": "2.0.0", "tag": "v2.0.0", "published_at": "2026-01-01T00:00:00Z", "packslip": "https://example.test/v2.sigstore.json", "status": "yanked" }] }
        })).unwrap()
    }
    #[test]
    fn withdrawal_and_freshness_apply_to_explicit_versions() {
        let list = signed_list();
        let releases = [Release {
            version: "2.0.0".into(),
            tag: "v2.0.0".into(),
            bundles: vec!["https://example.test/bundle".into()],
            recommended: true,
        }];
        assert!(matches!(
            choose(&releases, Some(&list), Some("v2.0.0")),
            Err(Error::Yanked(_))
        ));
        assert!(choose(&releases, Some(&list), None).is_err());
        assert!(
            list_freshness(
                Some(&list),
                Some(6),
                "2026-10-03T00:00:00Z".parse().unwrap()
            )
            .is_err()
        );
        assert!(
            list_freshness(
                Some(&list),
                Some(5),
                "2028-10-03T00:00:00Z".parse().unwrap()
            )
            .is_err()
        );
        assert!(
            list_freshness(
                Some(&list),
                Some(5),
                "2026-10-03T00:00:00Z".parse().unwrap()
            )
            .is_ok()
        );
        // A later list with no releases retires the withdrawal.
        let mut retired = signed_list();
        retired.predicate.releases.clear();
        retired.predicate.sequence = 6;
        assert!(
            list_freshness(
                Some(&retired),
                Some(5),
                "2026-10-03T00:00:00Z".parse().unwrap()
            )
            .is_ok()
        );
        assert_eq!(
            choose(&releases, Some(&retired), Some("v2.0.0"))
                .unwrap()
                .version,
            "2.0.0"
        );
    }
    #[test]
    fn offline_fetch_never_uses_transport_and_enforces_limits() {
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new(dir.path().into(), true, HttpConfig::default()).unwrap();
        let url = parse_url("https://example.invalid/file?secret=VALUE").unwrap();
        std::fs::write(client.cache_path(&url), b"cached").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            rt.block_on(client.fetch(url.as_str(), 6)).unwrap().unwrap(),
            b"cached"
        );
        assert!(rt.block_on(client.fetch(url.as_str(), 5)).is_err());
        let error = rt
            .block_on(client.fetch("https://example.invalid/missing?secret=VALUE", 6))
            .unwrap_err()
            .to_string();
        assert!(!error.contains("VALUE"));
    }

    #[test]
    fn owned_downloads_survive_concurrent_cache_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new(dir.path().into(), true, HttpConfig::default()).unwrap();
        let url = parse_url("https://example.invalid/artifact").unwrap();
        let cache = client.cache_path(&url);
        std::fs::write(&cache, b"first").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let first = rt.block_on(client.download(url.as_str(), 5)).unwrap();
        let first_path = first.path().to_owned();
        std::fs::write(&cache, b"alter").unwrap();
        assert_eq!(std::fs::read(first.path()).unwrap(), b"first");
        let mut next = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        next.write_all(b"later").unwrap();
        let mut next = publish_download(next, &cache).unwrap();
        let mut handle_bytes = Vec::new();
        next.read_to_end(&mut handle_bytes).unwrap();
        assert_eq!(handle_bytes, b"later");
        let second = rt.block_on(client.download(url.as_str(), 5)).unwrap();
        assert_eq!(std::fs::read(first.path()).unwrap(), b"first");
        assert_eq!(std::fs::read(second.path()).unwrap(), b"later");
        assert!(rt.block_on(client.download(url.as_str(), 4)).is_err());
        drop(first);
        assert!(!first_path.exists());
        assert_eq!(std::fs::read(cache).unwrap(), b"later");
        let directory = parse_url("https://example.invalid/directory").unwrap();
        std::fs::create_dir(client.cache_path(&directory)).unwrap();
        assert!(matches!(
            rt.block_on(client.download(directory.as_str(), 0)),
            Err(Error::Io(_))
        ));
        assert!(matches!(
            rt.block_on(client.download("https://example.invalid/missing", 0)),
            Err(Error::Offline(_))
        ));
        #[cfg(unix)]
        {
            let linked = parse_url("https://example.invalid/symlink").unwrap();
            std::os::unix::fs::symlink(next.path(), client.cache_path(&linked)).unwrap();
            assert!(matches!(
                rt.block_on(client.download(linked.as_str(), 5)),
                Err(Error::Io(_))
            ));
        }
    }
    #[test]
    fn recommendation_precedes_highest_semver() {
        let releases = vec![
            Release {
                version: "1.0.0".into(),
                tag: "v1.0.0".into(),
                bundles: vec!["https://example.test/bundle".into()],
                recommended: true,
            },
            Release {
                version: "2.0.0".into(),
                tag: "v2.0.0".into(),
                bundles: vec!["https://example.test/bundle".into()],
                recommended: false,
            },
        ];
        assert_eq!(choose(&releases, None, None).unwrap().version, "1.0.0");
        assert_eq!(
            choose(&releases, None, Some("v2.0.0")).unwrap().version,
            "2.0.0"
        );
        assert!(list_freshness(None, Some(5), jiff::Timestamp::now()).is_err());
    }
    #[test]
    fn requests_match_component_prefixes_and_prerelease_prefixes() {
        let releases: Vec<_> = ["3.12.1", "3.12.9", "3.13.0", "3.12.10-beta.1"]
            .into_iter()
            .map(|version| Release {
                version: version.into(),
                tag: format!("v{version}"),
                bundles: vec!["https://example.test/bundle".into()],
                recommended: false,
            })
            .collect();
        assert_eq!(
            choose(&releases, None, Some("3.12")).unwrap().version,
            "3.12.9"
        );
        assert_eq!(
            choose(&releases, None, Some("3.12.10-beta"))
                .unwrap()
                .version,
            "3.12.10-beta.1"
        );
        assert_eq!(
            choose(&releases, None, Some(">=3.12, <3.13"))
                .unwrap()
                .version,
            "3.12.9"
        );
    }
    #[test]
    fn exact_requests_match_build_metadata() {
        let releases: Vec<_> = ["1.2.3+build.5", "1.2.2", "1.2.30", "1.3.0-rc.1+build.2"]
            .into_iter()
            .map(|version| Release {
                version: version.into(),
                tag: format!("release-{version}"),
                bundles: vec!["https://example.test/bundle".into()],
                recommended: false,
            })
            .collect();
        for request in ["1.2.3", "v1.2.3", "1.2.03", "1.2.3+build.5", "=1.2.3"] {
            assert_eq!(
                choose(&releases, None, Some(request)).unwrap().version,
                "1.2.3+build.5",
                "{request}"
            );
        }
        assert_eq!(
            choose(&releases, None, Some("1.2.2")).unwrap().version,
            "1.2.2"
        );
        assert_eq!(
            choose(&releases, None, Some("1.3.0-rc.1")).unwrap().version,
            "1.3.0-rc.1+build.2"
        );
        assert!(matches!(
            choose(&releases, None, Some("1.2.4")),
            Err(Error::NoRelease)
        ));
        // An explicit request for a withdrawn build reports the withdrawal.
        let mut list = signed_list();
        list.predicate.releases[0].version = "1.2.3+build.5".into();
        list.predicate.releases[0].tag = None;
        assert!(matches!(
            choose(&releases, Some(&list), Some("1.2.3")),
            Err(Error::Yanked(version)) if version == "1.2.3+build.5"
        ));
        assert_eq!(
            choose(&releases, Some(&list), Some("1.2.2"))
                .unwrap()
                .version,
            "1.2.2"
        );
    }
    #[test]
    fn exact_requests_select_the_build_a_signed_list_maps_a_tag_to() {
        // A vendor tags `v1.2.3` or `1.2.3`, signs `1.2.3+build.5`, and maps
        // the tag in the supplementary list. GitHub listing still derives a
        // `1.2.3` candidate from the tag; selecting it would fail at install,
        // where the signed version differs.
        for tag in ["v1.2.3", "1.2.3"] {
            let live = vec![Release {
                version: "1.2.3".into(),
                tag: tag.into(),
                bundles: vec!["https://example.test/live".into()],
                recommended: false,
            }];
            let mut list = signed_list();
            list.predicate.releases[0].version = "1.2.3+build.5".into();
            list.predicate.releases[0].tag = Some(tag.into());
            list.predicate.releases[0].packslip = "https://example.test/listed".into();
            list.predicate.releases[0].status = None;
            for request in [
                Some("1.2.3"),
                Some("v1.2.3"),
                Some("1.2.03"),
                Some("=1.2.3"),
                Some("1.2"),
                Some("1.2.3+build.5"),
                Some("latest"),
                None,
            ] {
                let release = choose(&live, Some(&list), request).unwrap();
                assert_eq!(release.version, "1.2.3+build.5", "{tag} {request:?}");
                assert_eq!(release.bundles, vec!["https://example.test/listed"]);
                assert_eq!(release.tag, tag);
            }
            // Withdrawing the build withdraws the tag that names it.
            list.predicate.releases[0].status = Some(crate::model::ReleaseStatus::Yanked);
            assert!(matches!(
                choose(&live, Some(&list), Some(tag)),
                Err(Error::Yanked(version)) if version == "1.2.3+build.5"
            ));
        }
    }
    #[test]
    fn exact_requests_order_colliding_builds_like_equals() {
        // The specification forbids two releases that differ only in build
        // metadata. For a vendor who publishes both, an exact request picks
        // as `=1.2.3` does; the plain release stays reachable by its tag.
        let release = |version: &str| Release {
            version: version.into(),
            tag: format!("v{version}"),
            bundles: vec!["https://example.test/bundle".into()],
            recommended: false,
        };
        let releases = vec![release("1.2.3"), release("1.2.3+build.5")];
        for request in ["1.2.3", "1.2.03", "=1.2.3", "1.2", "1.2.3+build.5"] {
            assert_eq!(
                choose(&releases, None, Some(request)).unwrap().version,
                "1.2.3+build.5",
                "{request}"
            );
        }
        assert_eq!(
            choose(&releases, None, Some("v1.2.3")).unwrap().version,
            "1.2.3"
        );
        // Withdrawing the build leaves the release the request still matches.
        let mut list = signed_list();
        list.predicate.releases[0].version = "1.2.3+build.5".into();
        list.predicate.releases[0].tag = None;
        assert_eq!(
            choose(&releases, Some(&list), Some("1.2.3"))
                .unwrap()
                .version,
            "1.2.3"
        );
        assert!(matches!(
            choose(&releases, Some(&list), Some("1.2.3+build.5")),
            Err(Error::Yanked(_))
        ));
    }
    #[test]
    fn list_preserves_forge_tags_and_maps_latest_hint() {
        let live = vec![Release {
            version: "2.0.0".into(),
            tag: "release-2".into(),
            bundles: vec!["https://example.test/live".into()],
            recommended: false,
        }];
        let mut list = signed_list();
        list.predicate.releases[0].status = None;
        list.predicate.releases[0].tag = None;
        assert_eq!(
            choose(&live, Some(&list), Some("release-2")).unwrap().tag,
            "release-2"
        );
        list.predicate.releases[0].status = Some(crate::model::ReleaseStatus::Yanked);
        assert!(matches!(
            choose(&live, Some(&list), Some("release-2")),
            Err(Error::Yanked(_))
        ));
        list.predicate.releases[0].status = None;
        list.predicate.releases[0].tag = Some("stable".into());
        let newer = vec![Release {
            version: "3.0.0".into(),
            tag: "v3.0.0".into(),
            bundles: vec!["https://example.test/new".into()],
            recommended: false,
        }];
        assert_eq!(
            choose_with_latest(&newer, Some(&list), None, Some("stable"))
                .unwrap()
                .version,
            "2.0.0"
        );
    }
    #[test]
    fn duplicate_normalized_versions_retain_an_installable_candidate() {
        let installable = Release {
            version: "2.0.0".into(),
            tag: "v2.0.0".into(),
            bundles: vec!["https://example.test/bundle".into()],
            recommended: false,
        };
        let assetless = Release {
            tag: "2.0.0".into(),
            bundles: vec![],
            recommended: true,
            ..installable.clone()
        };
        for live in [
            vec![installable.clone(), assetless.clone()],
            vec![assetless, installable],
        ] {
            let release = choose(&live, None, Some("2.0.0")).unwrap();
            assert_eq!(release.bundles, vec!["https://example.test/bundle"]);
            // Only the release the pointer names is the recommendation;
            // another tag for its version does not inherit the mark.
            assert!(!release.recommended);
        }
    }
    #[test]
    fn exact_tag_aliases_retain_their_bundle_and_cannot_bypass_a_yank() {
        let first = Release {
            version: "2.0.0".into(),
            tag: "v2.0.0".into(),
            bundles: vec!["https://example.test/first".into()],
            recommended: false,
        };
        let alias = Release {
            tag: "tool-v2.0.0".into(),
            bundles: vec!["https://example.test/alias".into()],
            ..first.clone()
        };
        let live = vec![first, alias];
        assert_eq!(
            choose(&live, None, Some("tool-v2.0.0")).unwrap().bundles,
            vec!["https://example.test/alias"]
        );
        let list = signed_list();
        assert!(matches!(
            choose(&live, Some(&list), Some("tool-v2.0.0")),
            Err(Error::Yanked(_))
        ));
        let mut list = list;
        list.predicate.releases[0].status = None;
        assert_eq!(
            choose(&live, Some(&list), Some("tool-v2.0.0"))
                .unwrap()
                .bundles,
            vec![list.predicate.releases[0].packslip.clone()]
        );
    }
    #[test]
    fn padded_prerelease_and_build_tags_match_with_or_without_v() {
        for (tag, version) in [
            ("v3.12-beta", "3.12.0-beta"),
            ("v25.07+build.1", "25.7.0+build.1"),
        ] {
            let releases = vec![Release {
                version: version.into(),
                tag: tag.into(),
                bundles: vec!["https://example.test/bundle".into()],
                recommended: false,
            }];
            for request in [tag, tag.strip_prefix('v').unwrap(), version] {
                assert_eq!(
                    choose(&releases, None, Some(request)).unwrap().version,
                    version
                );
            }
        }
    }
    #[test]
    fn numeric_tags_do_not_override_prefixes_and_calver_normalization() {
        let releases: Vec<_> = [
            ("3.12.0", "v3.12", false),
            ("3.12.9", "v3.12.9", true),
            ("25.7.1", "25.07.1", false),
            ("25.7.1", "v25.7.1", true),
        ]
        .into_iter()
        .map(|(version, tag, installable)| Release {
            version: version.into(),
            tag: tag.into(),
            bundles: if installable {
                vec!["https://example.test/bundle".into()]
            } else {
                vec![]
            },
            recommended: false,
        })
        .collect();
        for request in ["3.12", "v3.12", "03.012", "3"] {
            assert_eq!(
                choose(&releases, None, Some(request)).unwrap().version,
                "3.12.9"
            );
        }
        for request in ["25.07.1", "v25.07.1", "25.07"] {
            assert_eq!(
                choose(&releases, None, Some(request)).unwrap().version,
                "25.7.1"
            );
        }
        let mut list = signed_list();
        list.predicate.releases[0].version = "3.12.0".into();
        list.predicate.releases[0].tag = Some("v3.12".into());
        assert_eq!(
            choose(&releases, Some(&list), Some("v3.12"))
                .unwrap()
                .version,
            "3.12.9"
        );
        assert!(matches!(
            choose(&releases, Some(&list), Some("3.12.0")),
            Err(Error::Yanked(_))
        ));
        let mut releases = releases;
        releases[0].bundles = vec!["https://example.test/old".into()];
        assert_eq!(
            choose(&releases, None, Some("v3.12")).unwrap().version,
            "3.12.9"
        );
    }
    #[test]
    fn assetless_named_tags_cannot_substitute_another_tags_bundle() {
        let live = vec![
            Release {
                version: "2.0.0".into(),
                tag: "v2.0.0".into(),
                bundles: vec!["https://example.test/bundle".into()],
                recommended: false,
            },
            Release {
                version: "2.0.0".into(),
                tag: "tool-v2.0.0".into(),
                bundles: vec![],
                recommended: false,
            },
        ];
        assert!(matches!(
            choose(&live, None, Some("tool-v2.0.0")),
            Err(Error::NoRelease)
        ));
        assert!(choose(&live, None, Some("2.0.0")).is_ok());
    }
    #[test]
    fn signed_list_maps_assetless_forge_tags_and_withdrawals() {
        let live = vec![Release {
            version: "2.0.0".into(),
            tag: "v2.0.0".into(),
            bundles: vec![],
            recommended: true,
        }];
        let mut list = signed_list();
        list.predicate.releases[0].tag = None;
        list.predicate.releases[0].status = None;
        let release = choose_with_latest(&live, Some(&list), None, Some("v2.0.0")).unwrap();
        assert_eq!(release.tag, "v2.0.0");
        assert!(release.recommended);
        assert!(choose(&live, None, None).is_err());
        list.predicate.releases[0].status = Some(crate::model::ReleaseStatus::Yanked);
        assert!(matches!(
            choose(&live, Some(&list), Some("v2.0.0")),
            Err(Error::Yanked(_))
        ));
    }
    #[test]
    fn github_filters_uninstallable_choices_and_retains_unmapped_latest() {
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new(dir.path().into(), true, HttpConfig::default()).unwrap();
        let base = "https://api.github.com/repos/jdx/tool";
        for (url, value) in [
            (
                base.to_string(),
                serde_json::json!({"id": 123, "default_branch": "main"}),
            ),
            (
                format!("{base}/releases/latest"),
                serde_json::json!({"tag_name": "stable", "draft": false}),
            ),
            (
                format!("{base}/releases?per_page=100&page=1"),
                serde_json::json!([
                    {"tag_name":"v9.0.0","draft":false,"assets":[]},
                    {"tag_name":"v1.0.0","draft":false,"prerelease":true,"assets":[
                        {"name":"packslip.sigstore.json","browser_download_url":"https://github.com/jdx/tool/bundle"}]},
                    {"tag_name":"stable","draft":false,"assets":[]}
                ]),
            ),
        ] {
            std::fs::write(
                client.cache_path(&parse_url(&url).unwrap()),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let view = rt
            .block_on(github(&client, &Project::parse("jdx/tool").unwrap()))
            .unwrap();
        assert_eq!(view.repository_id, "123");
        assert_eq!(view.latest_tag.as_deref(), Some("stable"));
        assert_eq!(view.releases.len(), 2);
        assert_eq!(choose(&view.releases, None, None).unwrap().version, "1.0.0");
    }
    #[test]
    fn host_transport_authority_cannot_be_claimed_by_another_origin() {
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new(dir.path().into(), true, HttpConfig::default()).unwrap();
        let project = Project::parse("Example.Test").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(
            rt.block_on(client.fetch_host(&project, "https://other.test/list", 10))
                .is_err()
        );
        assert!(
            rt.block_on(client.fetch_host(&project, "https://example.test:8443/list", 10))
                .is_err()
        );
        let url = parse_url("https://example.test/list").unwrap();
        std::fs::write(client.cache_path(&url), b"untrusted referral").unwrap();
        assert!(
            rt.block_on(client.fetch_host(&project, url.as_str(), 30))
                .is_err()
        );
        std::fs::write(client.host_cache_path(&url), b"document").unwrap();
        let document = rt
            .block_on(client.fetch_host(&project, url.as_str(), 10))
            .unwrap()
            .unwrap();
        assert_eq!(document.project(), "example.test");
        assert_eq!(
            Project::parse("Example.Test/Owner/Tool").unwrap().as_str(),
            "example.test/Owner/Tool"
        );
        assert_eq!(
            Project::parse("GitHub.Com/Jdx/Tool").unwrap().as_str(),
            "github.com/Jdx/Tool"
        );
        assert_eq!(document.bytes(), b"document");
    }
    #[test]
    fn malformed_versions_fail_without_panicking_even_when_requested() {
        let mut list = signed_list();
        list.predicate.releases[0].version = "broken".into();
        list.predicate.releases[0].status = None;
        assert!(matches!(
            choose(&[], Some(&list), Some("broken")),
            Err(Error::Version(_))
        ));
        assert!(matches!(
            choose(&[], Some(&list), Some("v2.0.0")),
            Err(Error::Version(_))
        ));
        assert!(matches!(
            choose(&[], Some(&list), None),
            Err(Error::Version(_))
        ));
    }

    const P: &str = "github.com/owner/repo/tool";

    fn release(version: &str, tag: &str, bundles: &[&str], recommended: bool) -> Release {
        Release {
            version: version.into(),
            tag: tag.into(),
            bundles: bundles.iter().map(|b| (*b).to_owned()).collect(),
            recommended,
        }
    }
    fn tags(candidates: &Candidates) -> Vec<&str> {
        candidates.releases.iter().map(|r| r.tag.as_str()).collect()
    }

    #[test]
    fn latest_candidates_try_the_recommendation_then_descending_precedence() {
        let live = vec![
            release("1.5.0", "v1.5.0", &["https://example.test/1.5"], false),
            release("1.0.0", "v1.0.0", &["https://example.test/1.0"], true),
            release(
                "3.0.0-rc.1",
                "v3.0.0-rc.1",
                &["https://example.test/rc"],
                false,
            ),
            release("2.0.0", "v2.0.0", &["https://example.test/2.0"], false),
        ];
        let candidates = candidates_with_latest(P, &live, None, None, Some("v1.0.0")).unwrap();
        assert!(candidates.latest);
        assert!(candidates.skipped.is_empty());
        assert_eq!(tags(&candidates), ["v1.0.0", "v2.0.0", "v1.5.0"]);
        assert_eq!(
            candidates.describe(&candidates.releases[0]),
            "1.0.0, GitHub's latest release"
        );
        assert_eq!(candidates.describe(&candidates.releases[1]), "2.0.0");
        // A version request keeps its matches by precedence and never treats
        // a missing host artifact as a reason to move on.
        let candidates = candidates_with_latest(P, &live, None, Some("1"), Some("v1.0.0")).unwrap();
        assert!(!candidates.latest);
        assert_eq!(tags(&candidates), ["v1.5.0", "v1.0.0"]);
        assert_eq!(
            choose_with_latest(&live, None, None, Some("v1.0.0"))
                .unwrap()
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn a_skipped_recommendation_is_reported_with_its_reason() {
        let live = vec![
            release("2.0.0", "v2.0.0", &["https://example.test/2.0"], false),
            release("1.0.0", "v1.0.0", &["https://example.test/1.0"], false),
        ];
        let mut list = signed_list();
        list.predicate.latest = Some("2.0.0".into());
        let candidates = candidates_with_latest(P, &live, Some(&list), None, None).unwrap();
        assert_eq!(tags(&candidates), ["v1.0.0"]);
        assert_eq!(
            candidates.skipped,
            [Skipped {
                recommendation: true,
                ..Skipped::new("2.0.0, the signed list's recommendation", "it is withdrawn")
            }]
        );
        list.predicate.releases[0].version = "3.0.0-rc.1".into();
        list.predicate.releases[0].tag = Some("release-candidate".into());
        list.predicate.releases[0].status = None;
        list.predicate.latest = Some("3.0.0-rc.1".into());
        let candidates = candidates_with_latest(P, &live, Some(&list), None, None).unwrap();
        assert_eq!(tags(&candidates), ["v2.0.0", "v1.0.0"]);
        assert_eq!(
            candidates.skipped[0].to_string(),
            "3.0.0-rc.1 (tag release-candidate), the signed list's recommendation: it is a prerelease"
        );
        // GitHub's pointer at a release with no packslip asset at all.
        let mut assetless = live.clone();
        assetless.insert(0, release("3.0.0", "v3.0.0", &[], true));
        let candidates = candidates_with_latest(P, &assetless, None, None, Some("v3.0.0")).unwrap();
        assert_eq!(tags(&candidates), ["v2.0.0", "v1.0.0"]);
        assert_eq!(
            candidates.skipped[0].to_string(),
            "3.0.0, GitHub's latest release: its release carries no packslip for github.com/owner/repo/tool"
        );
        // Another tag for the same version does not stand in for an
        // assetless latest release: the recommendation is still reported
        // as skipped, by tag so that it cannot be mistaken for the other
        // release, which is tried at its version's place.
        assetless.insert(
            1,
            release("3.0.0", "tool-v3.0.0", &["https://example.test/3.0"], false),
        );
        let candidates = candidates_with_latest(P, &assetless, None, None, Some("v3.0.0")).unwrap();
        assert_eq!(
            candidates.skipped[0].to_string(),
            "3.0.0 (tag v3.0.0), GitHub's latest release: its release carries no packslip for github.com/owner/repo/tool"
        );
        assert_eq!(tags(&candidates), ["tool-v3.0.0", "v2.0.0", "v1.0.0"]);
        assert_eq!(
            candidates.describe(&candidates.releases[0]),
            "3.0.0 (tag tool-v3.0.0)"
        );
        // A repository-wide pointer whose tag names no version of this
        // project supplies no recommendation, so nothing was skipped.
        let candidates =
            candidates_with_latest(P, &live, None, None, Some("other_v9.0.0")).unwrap();
        assert!(candidates.skipped.is_empty());
        assert_eq!(tags(&candidates), ["v2.0.0", "v1.0.0"]);
    }

    #[test]
    fn an_alternate_stays_reachable_when_a_mapped_tag_drops_its_version() {
        // Two tags derive 1.2.3; the signed list maps the first to a build.
        // The second tag's release, with its own bundles, is still tried.
        let live = vec![
            release("1.2.3", "v1.2.3", &["https://example.test/v"], false),
            release("1.2.3", "tool-1.2.3", &["https://example.test/tool"], false),
            release("1.2.2", "v1.2.2", &["https://example.test/1.2.2"], false),
        ];
        let mut list = signed_list();
        list.predicate.releases[0].version = "1.2.3+build.5".into();
        list.predicate.releases[0].tag = Some("v1.2.3".into());
        list.predicate.releases[0].packslip = "https://example.test/listed".into();
        list.predicate.releases[0].status = None;
        let candidates = candidates_with_latest(P, &live, Some(&list), None, None).unwrap();
        assert_eq!(tags(&candidates), ["v1.2.3", "tool-1.2.3", "v1.2.2"]);
        let candidates =
            candidates_with_latest(P, &live, Some(&list), Some("1.2.3"), None).unwrap();
        assert_eq!(tags(&candidates), ["v1.2.3", "tool-1.2.3"]);
    }

    #[test]
    fn a_tag_a_signed_list_maps_names_no_derived_candidate() {
        // The vendor tags `v1.2.3` and signs `1.2.3+build.5`. The `1.2.3`
        // derived from the tag would fail at install, so it is not tried
        // after the build; the next candidate is the older release.
        let live = vec![
            release("1.2.3", "v1.2.3", &["https://example.test/live"], false),
            release("1.2.2", "v1.2.2", &["https://example.test/1.2.2"], false),
        ];
        let mut list = signed_list();
        list.predicate.releases[0].version = "1.2.3+build.5".into();
        list.predicate.releases[0].tag = Some("v1.2.3".into());
        list.predicate.releases[0].packslip = "https://example.test/listed".into();
        list.predicate.releases[0].status = None;
        for request in [None, Some("1.2"), Some("1")] {
            let candidates = candidates_with_latest(P, &live, Some(&list), request, None).unwrap();
            let versions: Vec<_> = candidates
                .releases
                .iter()
                .map(|r| r.version.as_str())
                .collect();
            assert_eq!(versions, ["1.2.3+build.5", "1.2.2"], "{request:?}");
        }
        // Withdrawing the build withdraws its tag's derived version too.
        list.predicate.releases[0].status = Some(crate::model::ReleaseStatus::Yanked);
        let candidates = candidates_with_latest(P, &live, Some(&list), None, None).unwrap();
        assert_eq!(tags(&candidates), ["v1.2.2"]);
        assert!(matches!(
            candidates_with_latest(P, &live, Some(&list), Some("1.2.3"), None),
            Err(Error::Yanked(version)) if version == "1.2.3+build.5"
        ));
        // A tag that a signed entry maps to an older version offers only
        // that version, not the newer one derived from the tag.
        let live = vec![
            release("3.0.0", "v3.0.0", &["https://example.test/live"], false),
            release("1.3.0", "v1.3.0", &["https://example.test/1.3.0"], false),
        ];
        list.predicate.releases[0].version = "1.4.0".into();
        list.predicate.releases[0].tag = Some("v3.0.0".into());
        list.predicate.releases[0].status = None;
        let candidates = candidates_with_latest(P, &live, Some(&list), None, None).unwrap();
        let versions: Vec<_> = candidates
            .releases
            .iter()
            .map(|r| r.version.as_str())
            .collect();
        assert_eq!(versions, ["1.4.0", "1.3.0"]);
        assert!(matches!(
            candidates_with_latest(P, &live, Some(&list), Some("3.0.0"), None),
            Err(Error::NoRelease)
        ));
    }

    #[test]
    fn forge_releases_sharing_a_version_remain_candidates() {
        // oxc_v2.0.0 names version 2.0.0 for .../oxc/oxlint through the
        // repository-name prefix, but only its bundles say which tool it
        // describes.
        let live = vec![
            release("2.0.0", "oxc_v2.0.0", &["https://example.test/oxc"], true),
            release(
                "2.0.0",
                "oxlint_v2.0.0",
                &["https://example.test/oxlint"],
                false,
            ),
            release(
                "1.9.0",
                "oxlint_v1.9.0",
                &["https://example.test/1.9"],
                false,
            ),
        ];
        let candidates = candidates_with_latest(P, &live, None, None, Some("oxc_v2.0.0")).unwrap();
        assert_eq!(
            tags(&candidates),
            ["oxc_v2.0.0", "oxlint_v2.0.0", "oxlint_v1.9.0"]
        );
        assert_eq!(
            candidates.describe(&candidates.releases[0]),
            "2.0.0 (tag oxc_v2.0.0), GitHub's latest release"
        );
        assert_eq!(
            candidates.describe(&candidates.releases[1]),
            "2.0.0 (tag oxlint_v2.0.0)"
        );
        // GitHub's listing order does not decide which release is the
        // recommendation: GitHub's latest release goes first either way.
        let reversed = vec![live[1].clone(), live[0].clone(), live[2].clone()];
        let candidates =
            candidates_with_latest(P, &reversed, None, None, Some("oxc_v2.0.0")).unwrap();
        assert_eq!(
            tags(&candidates),
            ["oxc_v2.0.0", "oxlint_v2.0.0", "oxlint_v1.9.0"]
        );
        let described: Vec<_> = candidates
            .releases
            .iter()
            .map(|r| candidates.describe(r))
            .collect();
        assert_eq!(
            described,
            [
                "2.0.0 (tag oxc_v2.0.0), GitHub's latest release",
                "2.0.0 (tag oxlint_v2.0.0)",
                "1.9.0 (tag oxlint_v1.9.0)"
            ]
        );
        assert_eq!(
            choose_with_latest(&reversed, None, None, Some("oxc_v2.0.0"))
                .unwrap()
                .tag,
            "oxc_v2.0.0"
        );
        // A version request ignores the recommendation, so its reports do
        // not name it, and keeps releases that share a version in GitHub's
        // order.
        let candidates =
            candidates_with_latest(P, &reversed, None, Some("2"), Some("oxc_v2.0.0")).unwrap();
        assert_eq!(tags(&candidates), ["oxlint_v2.0.0", "oxc_v2.0.0"]);
        assert_eq!(
            candidates.describe(&candidates.releases[0]),
            "2.0.0 (tag oxlint_v2.0.0)"
        );
        assert_eq!(
            candidates.skip(&candidates.releases[1], "no packslip"),
            Skipped::new("2.0.0 (tag oxc_v2.0.0)", "no packslip")
        );
        let candidates = candidates_with_latest(P, &live, None, Some("2.0.0"), None).unwrap();
        assert_eq!(tags(&candidates), ["oxc_v2.0.0", "oxlint_v2.0.0"]);
        let candidates =
            candidates_with_latest(P, &live, None, Some("oxlint_v2.0.0"), None).unwrap();
        assert_eq!(tags(&candidates), ["oxlint_v2.0.0"]);
        // A signed entry pins the one bundle for its version.
        let mut list = signed_list();
        list.predicate.releases[0].status = None;
        list.predicate.releases[0].tag = Some("oxlint_v2.0.0".into());
        let candidates = candidates_with_latest(P, &live, Some(&list), None, None).unwrap();
        assert_eq!(tags(&candidates), ["oxlint_v2.0.0", "oxlint_v1.9.0"]);
        assert_eq!(
            candidates.releases[0].bundles,
            [list.predicate.releases[0].packslip.clone()]
        );
    }

    #[test]
    fn a_list_entry_under_another_tag_is_not_githubs_latest_release() {
        // GitHub marks the release-please root release v2.0.0 latest. It
        // carries no packslip asset, and the signed list maps 2.0.0 to the
        // tool's own tag. That entry is not GitHub's latest release, so the
        // recommendation is reported as skipped, and the entry is tried at
        // its version's place.
        let root = release("2.0.0", "v2.0.0", &[], true);
        let tool = release(
            "2.0.0",
            "tool-v2.0.0",
            &["https://example.test/2.0.0"],
            false,
        );
        let older = release(
            "1.0.0",
            "tool-v1.0.0",
            &["https://example.test/1.0.0"],
            false,
        );
        let patch = release(
            "2.0.1",
            "tool-v2.0.1",
            &["https://example.test/2.0.1"],
            false,
        );
        let mut list = signed_list();
        list.predicate.releases[0].status = None;
        list.predicate.releases[0].tag = Some("tool-v2.0.0".into());
        let skipped = "2.0.0 (tag v2.0.0), GitHub's latest release: the signed list maps 2.0.0 to tag tool-v2.0.0";
        let live = vec![root.clone(), tool.clone(), older.clone()];
        let candidates =
            candidates_with_latest(P, &live, Some(&list), None, Some("v2.0.0")).unwrap();
        assert_eq!(tags(&candidates), ["tool-v2.0.0", "tool-v1.0.0"]);
        assert_eq!(candidates.skipped[0].to_string(), skipped);
        assert_eq!(
            candidates.describe(&candidates.releases[0]),
            "2.0.0 (tag tool-v2.0.0)"
        );
        assert!(!candidates.releases[0].recommended);
        // A higher release is tried before the entry.
        let live = vec![root.clone(), patch.clone(), tool.clone(), older.clone()];
        let candidates =
            candidates_with_latest(P, &live, Some(&list), None, Some("v2.0.0")).unwrap();
        assert_eq!(
            tags(&candidates),
            ["tool-v2.0.1", "tool-v2.0.0", "tool-v1.0.0"]
        );
        assert_eq!(candidates.skipped[0].to_string(), skipped);
        // Without GitHub's tag, a caller's mark on v2.0.0 does not pass to
        // the entry either.
        assert_eq!(choose(&live, Some(&list), None).unwrap().tag, "tool-v2.0.1");
        // An entry with no tag of its own is GitHub's latest release when
        // that release has its version, whatever GitHub's listing order.
        list.predicate.releases[0].tag = None;
        let root = release("2.0.0", "v2.0.0", &["https://example.test/root"], true);
        for live in [
            vec![root.clone(), tool.clone(), patch.clone()],
            vec![tool.clone(), root.clone(), patch.clone()],
        ] {
            let candidates =
                candidates_with_latest(P, &live, Some(&list), None, Some("v2.0.0")).unwrap();
            assert!(candidates.skipped.is_empty());
            assert_eq!(tags(&candidates), ["v2.0.0", "tool-v2.0.1"]);
            assert_eq!(
                candidates.describe(&candidates.releases[0]),
                "2.0.0 (tag v2.0.0), GitHub's latest release"
            );
            assert_eq!(
                candidates.releases[0].bundles,
                [list.predicate.releases[0].packslip.clone()]
            );
        }
    }

    #[test]
    fn a_release_sharing_the_recommended_version_keeps_its_place() {
        // A release-please monorepo tags the root release v2.0.0 and marks
        // it latest; the tool's own releases are unmarked. If v2.0.0 turns
        // out to carry only the root's packslip, the next release to try
        // is the highest one, tool-v2.0.1, not tool-v2.0.0.
        let root = release("2.0.0", "v2.0.0", &["https://example.test/root"], true);
        let patch = release(
            "2.0.1",
            "tool-v2.0.1",
            &["https://example.test/2.0.1"],
            false,
        );
        let sibling = release(
            "2.0.0",
            "tool-v2.0.0",
            &["https://example.test/2.0.0"],
            false,
        );
        for live in [
            vec![patch.clone(), root.clone(), sibling.clone()],
            vec![sibling.clone(), root.clone(), patch.clone()],
        ] {
            let candidates = candidates_with_latest(P, &live, None, None, Some("v2.0.0")).unwrap();
            assert_eq!(tags(&candidates), ["v2.0.0", "tool-v2.0.1", "tool-v2.0.0"]);
            assert!(candidates.skipped.is_empty());
            let described: Vec<_> = candidates
                .releases
                .iter()
                .map(|r| candidates.describe(r))
                .collect();
            // Both releases of 2.0.0 are named by tag, so a report that
            // skipped v2.0.0 cannot be read as skipping tool-v2.0.0.
            assert_eq!(
                described,
                [
                    "2.0.0 (tag v2.0.0), GitHub's latest release",
                    "2.0.1 (tag tool-v2.0.1)",
                    "2.0.0 (tag tool-v2.0.0)"
                ]
            );
            assert!(candidates.skip(&candidates.releases[0], "r").recommendation);
            assert!(!candidates.skip(&candidates.releases[2], "r").recommendation);
            assert_eq!(
                choose_with_latest(&live, None, None, Some("v2.0.0"))
                    .unwrap()
                    .tag,
                "v2.0.0"
            );
        }
    }
}
