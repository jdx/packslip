//! Command-scoped bootstrap orchestration. Discovery never supplies policy;
//! authenticated lists and releases are checked under the persistent scope lock.
use crate::{discovery, install_extract, install_fs, install_host, install_policy};
use eyre::{Result, bail};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

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
                absolute(
                    std::env::var_os(name)
                        .filter(|v| !v.is_empty())
                        .map(PathBuf::from)
                        .unwrap_or_else(|| home.join(fallback)),
                )
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
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
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
        caller.pubkey = Some(if Path::new(text).is_file() {
            read_optional(Path::new(text))?.unwrap()
        } else {
            text.clone()
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
    if crate::model::repository(project.as_str()).is_some()
        && !project.as_str().starts_with("github.com/")
    {
        bail!(
            "bootstrap forge discovery currently supports github.com; other forges require a host-named project"
        );
    }
    let scope = Scope::detect(request.system, request.user)?;
    run_in(request, scope).await
}

// Explicit scope injection keeps integration fixtures isolated from real user
// and admin state. The CLI always obtains it from Scope::detect.
async fn run_in(request: Request, scope: Scope) -> Result<Report> {
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
            sigstore_trust_root::PRODUCTION_TUF_ROOT,
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
    // ID-based ownership follows renames, but a remembered name must never
    // silently become first use when that name is deleted and recreated.
    let alias = session.history(project.as_str())?;
    if let Some(previous) = alias.release.as_ref().or(alias.list.as_ref())
        && let Some(pin) = &previous.repository
        && repository_id != Some(pin.repository_id.as_str())
    {
        bail!("repository name now resolves to a different remembered repository ID");
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
        Some(list)
    };
    discovery::list_freshness(
        list.as_ref().map(|l| &l.verified.list),
        history.sequence,
        jiff::Timestamp::now(),
    )?;
    if list.is_some() {
        session.remember(&key, &history)?;
        session.remember(project.as_str(), &history)?;
    }
    let signed_list = list.as_ref().map(|l| &l.verified.list);
    let selected = match &github {
        Some(github) => github.choose(signed_list, request.version.as_deref())?,
        None => discovery::choose(&[], signed_list, request.version.as_deref())?,
    };
    let mut accepted = None;
    for url in &selected.bundles {
        let host_authority = github.is_none()
            && reqwest::Url::parse(url).ok().is_some_and(|u| {
                u.host_str() == Some(project.as_str().split('/').next().unwrap())
                    && u.port_or_known_default() == Some(443)
            });
        let document = if host_authority {
            Some(
                client
                    .fetch_host(&project, url, 16 * 1024 * 1024)
                    .await?
                    .ok_or_else(|| eyre::eyre!("release bundle is missing"))?,
            )
        } else {
            None
        };
        let bundle = match &document {
            Some(document) => std::str::from_utf8(document.bytes())?.to_owned(),
            None => String::from_utf8(
                client
                    .fetch(url, 16 * 1024 * 1024)
                    .await?
                    .ok_or_else(|| eyre::eyre!("release bundle is missing"))?,
            )?,
        };
        let listed_digest = signed_list.and_then(|list| list.digest_of(url));
        if let Some(expected) = listed_digest
            && hex::encode(Sha256::digest(bundle.as_bytes())) != expected
        {
            bail!("release bundle differs from its signed list digest");
        }
        // A forge release can describe several monorepo tools. The peek only
        // filters candidates; it never establishes identity or authorizes use.
        if github.is_some() && listed_digest.is_none() {
            let payload: serde_json::Value =
                serde_json::from_slice(&crate::sigstore::peek_statement(&bundle)?)?;
            let name = payload["predicate"]["project"].as_str().unwrap_or_default();
            if payload["predicateType"] != crate::model::PREDICATE_TYPE
                || crate::model::repository(name).is_none()
                || crate::model::repository_subpath(name)
                    != crate::model::repository_subpath(project.as_str())
            {
                continue;
            }
        }
        let release = match &document {
            Some(document) => install_policy::release_host(document, &constraints, options)?,
            None => install_policy::release(
                project.as_str(),
                repository_id,
                &bundle,
                &constraints,
                options,
            )?,
        };
        if accepted.is_some() {
            bail!("multiple authenticated packslips describe the requested project");
        }
        accepted = Some((release, bundle, url.clone()));
    }
    let (release, bundle, url) = accepted.ok_or_else(|| {
        eyre::eyre!("release has no authenticated packslip for the requested project")
    })?;
    check_release(
        &release,
        &bundle,
        &selected,
        signed_list,
        &request,
        &history,
        &constraints,
    )?;
    let mut alias = history.clone();
    alias.release = Some(release.record.clone());
    session.remember(project.as_str(), &alias)?;
    let mut host = install_host::Snapshot::detect()?;
    let artifact = crate::select_artifact(
        &release.statement.predicate.artifacts,
        &host.host(),
        request.variant.as_deref(),
        &[
            "tar.xz", "tar.zst", "tar.gz", "tgz", "tar.bz2", "tar", "zip", "xz", "zst", "gz",
            "bz2", "raw",
        ],
    )?;
    host.observe(artifact);
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
    if size != artifact.size || release.statement.digest_of(&artifact.name) != Some(digest.as_str())
    {
        bail!("downloaded artifact differs from its signed size or digest");
    }
    let parent = std::fs::canonicalize(
        tree.parent()
            .ok_or_else(|| eyre::eyre!("installation tree has no parent"))?,
    )?;
    let extracted = install_extract::extract(input.path(), artifact, &parent, request.limits)?;
    history.release = Some(release.record.clone());
    remember_key(&mut history, &constraints);
    if request.force {
        eprintln!("Replacing installation destination: {}", tree.display());
        for (name, _) in &extracted.bins {
            eprintln!(
                "Replacing command destination: {}",
                bin.join(if cfg!(windows) {
                    format!("{name}.exe")
                } else {
                    name.clone()
                })
                .display()
            );
        }
    }
    let pin = release
        .record
        .repository
        .as_ref()
        .and_then(|r| crate::Fingerprint::of(release.record.issuer.as_deref()?, &r.repository_id))
        .map(|p| p.to_string());
    let receipt = session.commit(
        extracted,
        &key,
        &release.verified.version,
        request.force,
        history,
        crate::install_launcher::write_export,
    )?;
    let mut warnings = report.warnings;
    for resource in crate::select_resources(&release.statement, artifact) {
        warnings.push(format!("resource {} is retained in the release metadata; bootstrap installs only declared commands", resource.kind));
    }
    if !std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|p| p == bin)) {
        warnings.push(format!("{} is absent from PATH; invoke the reported command paths or add this directory yourself", bin.display()));
    }
    Ok(Report {
        project: project.as_str().into(),
        receipt,
        pin,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture {
        root: tempfile::TempDir,
        key: crate::minisign::SecretKey,
        statement: serde_json::Value,
        bundle_url: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let tree = root.path().join("publisher/dist");
            std::fs::create_dir_all(tree.join("bin")).unwrap();
            std::fs::create_dir_all(tree.join("runtime")).unwrap();
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
            std::fs::create_dir_all(fixture.scope().state.join("downloads")).unwrap();
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
        std::fs::create_dir_all(&pins).unwrap();
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
        let name = "github.com/owner/tool";
        {
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
            session.remember(name, &prior).unwrap();
        }
        let base = "https://api.github.com/repos/owner/tool";
        for (url, value) in [
            (base.to_owned(), json!({"id":43,"default_branch":"main"})),
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
        assert!(
            f.run(request)
                .unwrap_err()
                .to_string()
                .contains("different remembered repository ID")
        );
        assert!(report.receipt.exports.keys().all(|p| p.exists()));
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
