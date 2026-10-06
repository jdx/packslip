//! Fresh Sigstore verification keys authenticated from a pinned TUF root.
//!
//! Cache snapshots are committed only after the entire refresh succeeds. An
//! offline command replays the signatures, root chain, expiry, and target digest;
//! it never trusts a cached `trusted_root.json` by itself.
use sigstore_trust_root::TrustedRoot;
use sigstore_tuf::transport::FetchFuture;
use sigstore_tuf::{MetadataStore, Repository, TrustedMetadataSet, Updater};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const CACHE_LIMIT: u64 = 32 * 1024 * 1024;
const TARGET: &str = "trusted_root.json";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("trust-root cache: {0}")]
    Io(#[from] std::io::Error),
    #[error("trust-root cache JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TUF trust-root refresh: {0}")]
    Tuf(#[from] sigstore_tuf::Error),
    #[error("Sigstore trust root: {0}")]
    Root(#[from] sigstore_trust_root::Error),
    #[error("offline trust-root refresh requires an authenticated cache")]
    MissingCache,
}

fn parse_root(bytes: &[u8]) -> Result<TrustedRoot, Error> {
    let json = std::str::from_utf8(bytes)
        .map_err(|_| sigstore_tuf::Error::Malformed("trust root is not UTF-8".into()))?;
    Ok(TrustedRoot::from_json(json)?)
}

#[derive(Clone, Default)]
struct Snapshot(Arc<Mutex<BTreeMap<String, Vec<u8>>>>);
impl MetadataStore for Snapshot {
    fn load(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(name).cloned()
    }
    fn store(&self, name: &str, bytes: &[u8]) -> sigstore_tuf::Result<()> {
        self.0
            .lock()
            .unwrap()
            .insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }
}

fn cache_read(path: &Path) -> Result<Option<Snapshot>, Error> {
    match std::fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(CACHE_LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > CACHE_LIMIT {
                return Err(std::io::Error::other("cache exceeds 32 MiB").into());
            }
            Ok(Some(Snapshot(Arc::new(Mutex::new(
                serde_json::from_slice(&bytes)?,
            )))))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// Validate even expired metadata before deriving version floors. Expiry is
// checked at the real command time when the cache is used, not at this step.
fn validate_cache(cache: &Snapshot, bootstrap: &[u8]) -> Result<BTreeMap<String, u64>, Error> {
    let mut trusted = TrustedMetadataSet::from_root(bootstrap)?;
    loop {
        let next = trusted.root().version + 1;
        match cache.load(&format!("root_history/{next}.root.json")) {
            Some(bytes) => trusted.update_root(&bytes)?,
            None => break,
        }
    }
    let latest = trusted.root().version;
    for key in cache
        .0
        .lock()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("root_history/"))
    {
        let version = key
            .trim_start_matches("root_history/")
            .trim_end_matches(".root.json")
            .parse::<u64>()
            .map_err(|_| {
                sigstore_tuf::Error::Malformed("invalid cached root history name".into())
            })?;
        if version > latest {
            return Err(
                sigstore_tuf::Error::Malformed("cached root chain has a gap".into()).into(),
            );
        }
    }
    if let Some(root) = cache.load("root.json")
        && root != trusted.root_bytes()
    {
        return Err(sigstore_tuf::Error::Malformed(
            "cached root disagrees with authenticated history".into(),
        )
        .into());
    }
    let epoch = jiff::Timestamp::UNIX_EPOCH;
    let required = |name| {
        cache
            .load(name)
            .ok_or_else(|| sigstore_tuf::Error::Malformed(format!("cache is missing {name}")))
    };
    trusted.update_timestamp(&required("timestamp.json")?, epoch)?;
    trusted.update_snapshot(&required("snapshot.json")?, epoch)?;
    trusted.update_targets(&required("targets.json")?, epoch)?;
    // Sigstore's verification material is a top-level target. Fail explicitly
    // rather than treating an unauthenticated target cache as a trust anchor.
    let target = trusted
        .targets_role("targets")
        .and_then(|t| t.target(TARGET))
        .ok_or_else(|| {
            sigstore_tuf::Error::Malformed("trusted_root.json is not a top-level target".into())
        })?;
    let bytes = required("targets/trusted_root.json")?;
    check_target(&bytes, target)?;
    let mut floors = BTreeMap::new();
    floors.insert("timestamp".into(), trusted.timestamp().unwrap().version);
    floors.insert("snapshot".into(), trusted.snapshot().unwrap().version);
    floors.insert(
        "targets".into(),
        trusted.targets_role("targets").unwrap().version,
    );
    Ok(floors)
}

fn check_target(bytes: &[u8], target: &sigstore_tuf::TargetFile) -> sigstore_tuf::Result<()> {
    use sha2::Digest as _;
    if bytes.len() as u64 != target.length {
        return Err(sigstore_tuf::Error::IntegrityMismatch(
            "cached trust-root size".into(),
        ));
    }
    for (algorithm, expected) in &target.hashes {
        let actual = match algorithm.as_str() {
            "sha256" => hex::encode(sha2::Sha256::digest(bytes)),
            "sha512" => hex::encode(sha2::Sha512::digest(bytes)),
            _ => continue,
        };
        if actual != *expected {
            return Err(sigstore_tuf::Error::IntegrityMismatch(
                "cached trust-root digest".into(),
            ));
        }
    }
    if !target.hashes.contains_key("sha256") && !target.hashes.contains_key("sha512") {
        return Err(sigstore_tuf::Error::IntegrityMismatch(
            "trust root has no supported digest".into(),
        ));
    }
    Ok(())
}

struct FloorRepository<R> {
    inner: R,
    floors: BTreeMap<String, u64>,
}
impl<R: Repository> Repository for FloorRepository<R> {
    fn fetch_metadata<'a>(&'a self, name: &'a str, limit: u64) -> FetchFuture<'a> {
        Box::pin(async move {
            let bytes = self.inner.fetch_metadata(name, limit).await?;
            if let Some(bytes) = &bytes {
                let v: serde_json::Value = serde_json::from_slice(bytes)?;
                if let (Some(role), Some(version)) = (
                    v["signed"]["_type"].as_str(),
                    v["signed"]["version"].as_u64(),
                ) && let Some(floor) = self.floors.get(role)
                    && version < *floor
                {
                    return Err(sigstore_tuf::Error::Rollback {
                        role: role.into(),
                        trusted: *floor,
                        new: version,
                    });
                }
            }
            Ok(bytes)
        })
    }
    fn fetch_target<'a>(&'a self, path: &'a str, limit: u64) -> FetchFuture<'a> {
        self.inner.fetch_target(path, limit)
    }
}

async fn replay(cache: Snapshot, bootstrap: &[u8], now: jiff::Timestamp) -> Result<Vec<u8>, Error> {
    let mut updater = Updater::new(sigstore_tuf::StoreRepository::new(cache.clone()), bootstrap)?
        .with_store(cache);
    updater.refresh(now).await?;
    Ok(updater.get_target(TARGET, now).await?)
}

/// Refresh once for a command, or replay an unexpired cache with no network.
/// `bootstrap` is TUF root.json, not Sigstore trusted_root.json. A caller holds
/// a shared lock on `cache_path`'s parent across this operation.
/// Only a transport marked as a genuine network failure permits fallback.
pub async fn refresh<R: Repository + 'static>(
    repo: R,
    network_failed: Arc<AtomicBool>,
    bootstrap: &[u8],
    cache_path: &Path,
    offline: bool,
    now: jiff::Timestamp,
) -> Result<TrustedRoot, Error> {
    let previous = cache_read(cache_path)?;
    let floors = previous
        .as_ref()
        .map(|c| validate_cache(c, bootstrap))
        .transpose()?
        .unwrap_or_default();
    if offline {
        return parse_root(&replay(previous.ok_or(Error::MissingCache)?, bootstrap, now).await?);
    }
    let cache = match &previous {
        Some(c) => Snapshot(Arc::new(Mutex::new(c.0.lock().unwrap().clone()))),
        None => Snapshot::default(),
    };
    let mut updater = Updater::new(
        FloorRepository {
            inner: repo,
            floors,
        },
        bootstrap,
    )?
    .with_store(cache.clone());
    let result = async {
        updater.refresh(now).await?;
        let bytes = updater.get_target(TARGET, now).await?;
        // Reject cache corruption rather than silently redownloading it.
        Ok::<_, sigstore_tuf::Error>(bytes)
    }
    .await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(e)
            if network_failed.load(Ordering::SeqCst)
                && matches!(e, sigstore_tuf::Error::Transport(_)) =>
        {
            return parse_root(
                &replay(previous.ok_or(Error::MissingCache)?, bootstrap, now).await?,
            );
        }
        Err(e) => return Err(e.into()),
    };
    let root = parse_root(&bytes)?;
    let parent = cache_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut tmp, &*cache.0.lock().unwrap())?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist(cache_path).map_err(|e| e.error)?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(root)
}

/// Production TUF transport. It uses no ambient credentials. Only connection,
/// timeout, and interrupted-body errors qualify for authenticated fallback.
pub struct HttpRepository {
    client: reqwest::Client,
    network_failed: Arc<AtomicBool>,
}
impl HttpRepository {
    pub fn production() -> Result<(Self, Arc<AtomicBool>), sigstore_tuf::Error> {
        let network_failed = Arc::new(AtomicBool::new(false));
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|_| {
                sigstore_tuf::Error::Transport("cannot initialize TUF HTTPS client".into())
            })?;
        Ok((
            Self {
                client,
                network_failed: network_failed.clone(),
            },
            network_failed,
        ))
    }
    fn error(&self, e: reqwest::Error) -> sigstore_tuf::Error {
        if e.is_connect() || e.is_timeout() || e.is_body() {
            self.network_failed.store(true, Ordering::SeqCst);
        }
        sigstore_tuf::Error::Transport(format!("TUF HTTPS request failed: {}", e.without_url()))
    }
    fn fetch<'a>(&'a self, path: String, limit: u64) -> FetchFuture<'a> {
        Box::pin(async move {
            let url = format!("https://tuf-repo-cdn.sigstore.dev/{path}");
            let mut response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| self.error(e))?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                return Err(sigstore_tuf::Error::Malformed(format!(
                    "TUF HTTP status {}",
                    response.status()
                )));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|e| self.error(e))? {
                if bytes.len() as u64 + chunk.len() as u64 > limit {
                    return Err(sigstore_tuf::Error::Malformed(
                        "TUF download exceeds limit".into(),
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(Some(bytes))
        })
    }
}
impl Repository for HttpRepository {
    fn fetch_metadata<'a>(&'a self, name: &'a str, limit: u64) -> FetchFuture<'a> {
        self.fetch(name.into(), limit)
    }
    fn fetch_target<'a>(&'a self, path: &'a str, limit: u64) -> FetchFuture<'a> {
        self.fetch(format!("targets/{path}"), limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer as _;
    use serde_json::{Value, json};
    use sha2::Digest as _;

    #[derive(Clone)]
    struct Repo {
        metadata: BTreeMap<String, Vec<u8>>,
        target: Vec<u8>,
        failure: Option<Arc<AtomicBool>>,
    }
    impl Repository for Repo {
        fn fetch_metadata<'a>(&'a self, name: &'a str, _limit: u64) -> FetchFuture<'a> {
            Box::pin(async move {
                if let Some(failure) = &self.failure {
                    failure.store(true, Ordering::SeqCst);
                    return Err(sigstore_tuf::Error::Transport("disconnected".into()));
                }
                Ok(self.metadata.get(name).cloned())
            })
        }
        fn fetch_target<'a>(&'a self, _path: &'a str, _limit: u64) -> FetchFuture<'a> {
            Box::pin(async move { Ok(Some(self.target.clone())) })
        }
    }
    fn sign(value: Value, keys: &[(&str, &ed25519_dalek::SigningKey)]) -> Vec<u8> {
        let bytes = sigstore_tuf::canonical_json::to_canonical_bytes(&value).unwrap();
        let signatures: Vec<_> = keys
            .iter()
            .map(|(id, k)| json!({"keyid":id,"sig":hex::encode(k.sign(&bytes).to_bytes())}))
            .collect();
        serde_json::to_vec(&json!({"signed":value,"signatures":signatures})).unwrap()
    }
    fn root(
        version: u64,
        expires: &str,
        key: &ed25519_dalek::SigningKey,
        old: Option<&ed25519_dalek::SigningKey>,
    ) -> Vec<u8> {
        let id = format!("key{version}");
        let mut roles = serde_json::Map::new();
        for role in ["root", "timestamp", "snapshot", "targets"] {
            roles.insert(role.into(), json!({"keyids":[id],"threshold":1}));
        }
        let value = json!({"_type":"root","spec_version":"1.0.0","version":version,"expires":expires,"consistent_snapshot":false,"roles":roles,"keys":{id.clone():{"keytype":"ed25519","scheme":"ed25519","keyval":{"public":hex::encode(key.verifying_key().to_bytes())}}}});
        let mut keys = vec![(id.as_str(), key)];
        let old_id = format!("key{}", version - 1);
        if let Some(old) = old {
            keys.push((old_id.as_str(), old));
        }
        sign(value, &keys)
    }
    fn fixture(timestamp_version: u64, expiry: &str) -> (Repo, Vec<u8>) {
        let keys = [1, 2, 3].map(|n| ed25519_dalek::SigningKey::from_bytes(&[n; 32]));
        let bootstrap = root(1, "2020-01-01T00:00:00Z", &keys[0], None);
        let target = sigstore_trust_root::SIGSTORE_PRODUCTION_TRUSTED_ROOT
            .as_bytes()
            .to_vec();
        let hash = hex::encode(sha2::Sha256::digest(&target));
        let targets = sign(
            json!({"_type":"targets","spec_version":"1.0.0","version":1,"expires":"2099-01-01T00:00:00Z","targets":{TARGET:{"length":target.len(),"hashes":{"sha256":hash}}}}),
            &[("key3", &keys[2])],
        );
        let meta = |bytes: &[u8], version| json!({"version":version,"length":bytes.len(),"hashes":{"sha256":hex::encode(sha2::Sha256::digest(bytes))}});
        let snapshot = sign(
            json!({"_type":"snapshot","spec_version":"1.0.0","version":1,"expires":"2099-01-01T00:00:00Z","meta":{"targets.json":meta(&targets,1)}}),
            &[("key3", &keys[2])],
        );
        let timestamp = sign(
            json!({"_type":"timestamp","spec_version":"1.0.0","version":timestamp_version,"expires":expiry,"meta":{"snapshot.json":meta(&snapshot,1)}}),
            &[("key3", &keys[2])],
        );
        let metadata = BTreeMap::from([
            (
                "2.root.json".into(),
                root(2, "2021-01-01T00:00:00Z", &keys[1], Some(&keys[0])),
            ),
            (
                "3.root.json".into(),
                root(3, "2099-01-01T00:00:00Z", &keys[2], Some(&keys[1])),
            ),
            ("timestamp.json".into(), timestamp),
            ("snapshot.json".into(), snapshot),
            ("targets.json".into(), targets),
        ]);
        (
            Repo {
                metadata,
                target,
                failure: None,
            },
            bootstrap,
        )
    }
    fn run(
        repo: Repo,
        root: &[u8],
        path: &Path,
        offline: bool,
        now: &str,
    ) -> Result<TrustedRoot, Error> {
        let failure = repo
            .failure
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(refresh(
                repo,
                failure,
                root,
                path,
                offline,
                now.parse().unwrap(),
            ))
    }
    #[test]
    fn expired_bootstrap_rotates_twice_and_offline_never_fetches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let (mut repo, root) = fixture(2, "2099-01-01T00:00:00Z");
        run(repo.clone(), &root, &path, false, "2026-10-03T00:00:00Z").unwrap();
        let failure = Arc::new(AtomicBool::new(false));
        repo.failure = Some(failure.clone());
        run(repo, &root, &path, true, "2036-10-03T00:00:00Z").unwrap();
        assert!(!failure.load(Ordering::SeqCst));
    }
    #[test]
    fn network_fallback_needs_fresh_authenticated_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let (mut repo, root) = fixture(2, "2027-01-01T00:00:00Z");
        run(repo.clone(), &root, &path, false, "2026-10-03T00:00:00Z").unwrap();
        repo.failure = Some(Arc::new(AtomicBool::new(false)));
        run(repo.clone(), &root, &path, false, "2026-10-03T00:00:00Z").unwrap();
        assert!(run(repo.clone(), &root, &path, false, "2028-10-03T00:00:00Z").is_err());
        assert!(run(repo, &root, &path, true, "2028-10-03T00:00:00Z").is_err());
    }
    #[test]
    fn rollback_and_corruption_never_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let (repo, root) = fixture(2, "2027-01-01T00:00:00Z");
        run(repo, &root, &path, false, "2026-10-03T00:00:00Z").unwrap();
        let (rollback, _) = fixture(1, "2099-01-01T00:00:00Z");
        assert!(matches!(
            run(
                rollback.clone(),
                &root,
                &path,
                false,
                "2028-10-03T00:00:00Z"
            ),
            Err(Error::Tuf(sigstore_tuf::Error::Rollback { .. }))
        ));
        let cache = cache_read(&path).unwrap().unwrap();
        cache
            .store("targets/trusted_root.json", b"corrupt")
            .unwrap();
        std::fs::write(
            &path,
            serde_json::to_vec(&*cache.0.lock().unwrap()).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            run(rollback, &root, &path, false, "2026-10-03T00:00:00Z"),
            Err(Error::Tuf(sigstore_tuf::Error::IntegrityMismatch(_)))
        ));
    }
    #[test]
    fn invalid_online_signature_cannot_use_good_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let (mut repo, root) = fixture(2, "2099-01-01T00:00:00Z");
        run(repo.clone(), &root, &path, false, "2026-10-03T00:00:00Z").unwrap();
        let bytes = repo.metadata.get_mut("timestamp.json").unwrap();
        let mut value: Value = serde_json::from_slice(bytes).unwrap();
        value["signed"]["version"] = json!(3);
        *bytes = serde_json::to_vec(&value).unwrap();
        assert!(run(repo, &root, &path, false, "2026-10-03T00:00:00Z").is_err());
    }
    #[test]
    fn new_packaged_bootstrap_accepts_earlier_cached_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let (repo, root) = fixture(2, "2099-01-01T00:00:00Z");
        run(repo.clone(), &root, &path, false, "2026-10-03T00:00:00Z").unwrap();
        let next_bootstrap = repo.metadata["3.root.json"].clone();
        run(
            repo.clone(),
            &next_bootstrap,
            &path,
            false,
            "2026-10-03T00:00:00Z",
        )
        .unwrap();
        run(repo, &next_bootstrap, &path, true, "2026-10-03T00:00:00Z").unwrap();
    }
}
