//! Bootstrap discovery and HTTPS transport. Fetched documents remain untrusted
//! until the caller verifies the bundle, project, version, and list digest.
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::collections::BTreeMap;
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
    pub fn parse(input: &str) -> Result<Self, Error> {
        if input.is_empty()
            || input.contains([':', '?', '#', '%', '\\'])
            || input.starts_with('/')
            || input.ends_with('/')
        {
            return Err(Error::Project(input.into()));
        }
        if input.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        }) {
            return Err(Error::Project(input.into()));
        }
        let name = if input.contains('/') && !input.split('/').next().unwrap().contains('.') {
            format!("github.com/{input}")
        } else {
            input.to_owned()
        };
        // DNS host spelling is case-insensitive and URL parsing normalizes it.
        // Preserve path case while canonicalizing the project authority.
        let (host, suffix) = name
            .split_once('/')
            .map_or((name.as_str(), ""), |(host, suffix)| (host, suffix));
        let name = if suffix.is_empty() {
            host.to_ascii_lowercase()
        } else {
            format!("{}/{suffix}", host.to_ascii_lowercase())
        };
        let parts: Vec<_> = name.split('/').collect();
        if parts[0] == "github.com" && parts.len() < 3 {
            return Err(Error::Project(input.into()));
        }
        if !parts[0].contains('.') {
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
                std::fs::create_dir_all(&self.cache)?;
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
            std::fs::create_dir_all(&self.cache)?;
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
            let snapshot = snapshot(&path, &self.cache).map_err(|error| {
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
            std::fs::create_dir_all(&self.cache)?;
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

// Publish another name while retaining the owned file, positioned for reading.
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
    )?
    .persist(cache)
    .map_err(|error| error.error)?;
    file.as_file_mut().rewind()?;
    Ok(file)
}

// A private name pins the selected cache inode across atomic cache replacement.
// Copy only on filesystems that cannot make hard links. Recheck signed size on
// this snapshot, rather than on a pathname that another command can replace.
fn snapshot(
    input: &std::path::Path,
    parent: &std::path::Path,
) -> Result<tempfile::NamedTempFile, std::io::Error> {
    if !std::fs::symlink_metadata(input)?.is_file() {
        return Err(std::io::Error::other(
            "artifact cache is not a regular file",
        ));
    }
    let slot = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    std::fs::remove_file(&slot)?;
    if let Err(link_error) = std::fs::hard_link(input, &slot) {
        // Linking is an optimization, not an access/trust requirement. A read
        // plus a private copy is equally valid, including when link permission
        // differs from read permission. Retain both errors if neither works.
        std::fs::copy(input, &slot).map_err(|copy_error| {
            std::io::Error::new(
                copy_error.kind(),
                format!(
                    "cache snapshot: hard link failed ({link_error}); copy failed ({copy_error})"
                ),
            )
        })?;
    }
    if !std::fs::symlink_metadata(&slot)?.is_file() {
        return Err(std::io::Error::other(
            "artifact snapshot is not a regular file",
        ));
    }
    let file = std::fs::File::open(&slot)?;
    Ok(tempfile::NamedTempFile::from_parts(file, slot))
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
    for release in live {
        match choices.entry(release.version.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(release.clone());
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let recommended = entry.get().recommended || release.recommended;
                if entry.get().bundles.is_empty() && !release.bundles.is_empty() {
                    entry.insert(release.clone());
                }
                entry.get_mut().recommended = recommended;
            }
        }
    }
    if let Some(list) = list {
        for entry in &list.predicate.releases {
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
                continue;
            }
            let tag = entry
                .tag
                .clone()
                .or_else(|| choices.get(&entry.version).map(|r| r.tag.clone()))
                .unwrap_or_else(|| entry.version.clone());
            choices.insert(
                entry.version.clone(),
                Release {
                    version: entry.version.clone(),
                    recommended: list.predicate.latest.as_deref() == Some(&entry.version)
                        || (list.predicate.latest.is_none()
                            && (latest_tag == Some(tag.as_str())
                                || live
                                    .iter()
                                    .any(|r| r.version == entry.version && r.recommended))),
                    tag,
                    bundles: vec![entry.packslip.clone()],
                },
            );
        }
        if list.predicate.latest.is_some() {
            for choice in choices.values_mut() {
                choice.recommended = list.predicate.latest.as_deref() == Some(&choice.version);
            }
        }
    }
    choices.retain(|_, release| !release.bundles.is_empty());
    if let Some(request) = requested.filter(|r| *r != "latest") {
        if let Some(exact) = exact_tag {
            // Signed entries remain authoritative over every forge tag alias.
            // Otherwise an exact tag retains that release's own bundle URLs.
            if list.is_some_and(|list| {
                list.predicate
                    .releases
                    .iter()
                    .any(|entry| entry.version == exact.version)
            }) {
                return choices.get(&exact.version).cloned().ok_or(Error::NoRelease);
            }
            if !exact.bundles.is_empty() {
                return Ok(exact.clone());
            }
            // A numeric version may select any installable tag for that
            // version; a named tag without its own bundle cannot substitute.
            if numeric.is_none() {
                return Err(Error::NoRelease);
            }
        }
        let mut matches: Vec<_> = choices
            .into_values()
            .filter(|r| matches_request(r, request))
            .collect();
        matches.sort_by(|a, b| {
            semver::Version::parse(&b.version)
                .unwrap()
                .cmp(&semver::Version::parse(&a.version).unwrap())
        });
        return matches.into_iter().next().ok_or(Error::NoRelease);
    }
    let mut choices: Vec<_> = choices
        .into_values()
        .filter(|r| semver::Version::parse(&r.version).is_ok_and(|v| v.pre.is_empty()))
        .collect();
    choices.sort_by(|a, b| {
        semver::Version::parse(&b.version)
            .unwrap()
            .cmp(&semver::Version::parse(&a.version).unwrap())
    });
    choices
        .iter()
        .find(|r| r.recommended)
        .cloned()
        .or_else(|| choices.into_iter().next())
        .ok_or(Error::NoRelease)
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
        return release.version.starts_with(requested)
            && release.version.as_bytes().get(requested.len()) == Some(&b'.');
    }
    semver::VersionReq::parse(requested).is_ok_and(|range| range.matches(&version))
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
        ] {
            assert!(Project::parse(input).is_err(), "{input}");
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
            assert!(release.recommended);
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
}
