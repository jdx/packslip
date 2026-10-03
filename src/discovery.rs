//! Bootstrap discovery and HTTPS transport. Fetched documents remain untrusted
//! until the caller verifies the bundle, project, version, and list digest.
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::collections::BTreeMap;
use std::io::{Read, Write};
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
            .timeout(std::time::Duration::from_secs(60))
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
        })
    }
    fn cache_path(&self, url: &Url) -> PathBuf {
        self.cache
            .join(hex::encode(sha2::Sha256::digest(url.as_str().as_bytes())))
    }
    /// Fetch at most `limit` bytes, with no network at all in offline mode.
    /// Only 404 means an optional document is absent; malformed/failed ones fail.
    pub async fn fetch(&self, input: &str, limit: u64) -> Result<Option<Vec<u8>>, Error> {
        let initial = parse_url(input)?;
        let path = self.cache_path(&initial);
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
            let mut request = self.inner.get(current.clone());
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
    /// signed size while reading. The caller checks its digest before extraction.
    pub async fn download(&self, input: &str, expected_size: u64) -> Result<PathBuf, Error> {
        let initial = parse_url(input)?;
        let path = self.cache_path(&initial);
        if self.offline {
            let metadata =
                std::fs::metadata(&path).map_err(|_| Error::Offline(display_url(&initial)))?;
            if metadata.len() != expected_size {
                return Err(Error::Limit {
                    limit: expected_size,
                    url: display_url(&initial),
                });
            }
            return Ok(path);
        }
        let mut current = initial.clone();
        for redirects in 0..=10 {
            let mut request = self.inner.get(current.clone());
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
            tmp.as_file().sync_all()?;
            tmp.persist(&path).map_err(|e| e.error)?;
            return Ok(path);
        }
        Err(Error::Redirects)
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
pub async fn github(client: &Client, project: &Project) -> Result<(Vec<Release>, String), Error> {
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
                let bundles = release
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
            return Ok((releases, list_url.to_string()));
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
pub fn choose(
    live: &[Release],
    list: Option<&crate::ReleaseListStatement>,
    requested: Option<&str>,
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
    let mut choices: BTreeMap<String, Release> = live
        .iter()
        .map(|r| (r.version.clone(), r.clone()))
        .collect();
    if let Some(list) = list {
        for entry in &list.predicate.releases {
            if entry.is_yanked() {
                choices.remove(&entry.version);
                if requested.is_some_and(|v| v == entry.version || entry.tag.as_deref() == Some(v))
                {
                    return Err(Error::Yanked(entry.version.clone()));
                }
                continue;
            }
            choices.insert(
                entry.version.clone(),
                Release {
                    version: entry.version.clone(),
                    tag: entry.tag.clone().unwrap_or_else(|| entry.version.clone()),
                    bundles: vec![entry.packslip.clone()],
                    recommended: list.predicate.latest.as_deref() == Some(&entry.version)
                        || (list.predicate.latest.is_none()
                            && live
                                .iter()
                                .any(|r| r.version == entry.version && r.recommended)),
                },
            );
        }
        if list.predicate.latest.is_some() {
            for choice in choices.values_mut() {
                choice.recommended = list.predicate.latest.as_deref() == Some(&choice.version);
            }
        }
    }
    if let Some(request) = requested.filter(|r| *r != "latest") {
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

fn matches_request(release: &Release, request: &str) -> bool {
    let requested = request.strip_prefix('v').unwrap_or(request);
    if release.tag == request
        || release.tag.strip_prefix('v') == Some(requested)
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
            bundles: vec![],
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
    fn recommendation_precedes_highest_semver() {
        let releases = vec![
            Release {
                version: "1.0.0".into(),
                tag: "v1.0.0".into(),
                bundles: vec![],
                recommended: true,
            },
            Release {
                version: "2.0.0".into(),
                tag: "v2.0.0".into(),
                bundles: vec![],
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
                bundles: vec![],
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
