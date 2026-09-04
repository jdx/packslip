//! The `packslip` binary: create, verify, show, releases, keygen, schema.

use std::path::{Path, PathBuf};

use eyre::{Context as _, Result, bail};
use packslip::cli::{BinInfo, Version};
use packslip::create::{ArtifactInput, AssetInput, ListRequest, ListedRelease, Request};
use packslip::minisign::{PublicKey, SecretKey, key_id_hex};
use packslip::model::{
    Attestor, Bin, Evidence, RELEASES_PREDICATE_TYPE, ReleaseListStatement, Resource, Source,
    Statement, VersionOrder,
};
use packslip::sigstore::{self, Policy, Signer, Trust};
use packslip::verify::Options;
use usage_rs::RunWith;

const BIN: BinInfo = BinInfo {
    name: "packslip",
    version: env!("CARGO_PKG_VERSION"),
};

/// The file a release ships: `packslip.sigstore.json`, or, for a tool in
/// a monorepo named `github.com/owner/repo/sub/path`,
/// `packslip.sub-path.sigstore.json`, so several tools can share one
/// release. Consumers match on the statement's `project`, not the name.
pub fn bundle_name(project: &str) -> String {
    match packslip::model::repository_subpath(project) {
        Some(sub) => format!("packslip.{}.sigstore.json", sub.replace('/', "-")),
        None => "packslip.sigstore.json".to_string(),
    }
}

/// A signed release manifest: what shipped, and how to verify it
///
/// A vendor runs `packslip create` in its release job to publish one signed
/// document listing every artifact. Consumers verify it with `packslip
/// verify` against a pinned identity or key. See https://packslip.dev.
#[derive(usage_rs::Cli)]
#[usage(
    name = "packslip",
    bin = "packslip",
    version,
    author = "Jeff Dickey <@jdx>",
    arg_required_else_help
)]
struct Cli {
    #[usage(subcommand)]
    command: Option<Commands>,
}

#[derive(usage_rs::Subcommands)]
#[usage(run_with)]
enum Commands {
    Create(Box<Create>),
    Keygen(Keygen),
    Releases(Box<Releases>),
    Schema(Schema),
    Show(Show),
    Verify(Verify),
    Version(Version),
}

/// Generate an Ed25519 key pair for the sigstore-key scheme
///
/// Only needed outside a CI job: with an OIDC identity, `create` signs
/// keylessly and needs no key. Writes the secret seed to the given path
/// (mode 0600) and a minisign-format public key beside it with a .pub
/// extension.
#[derive(Debug, usage_rs::Args)]
struct Keygen {
    /// Where to write the secret key
    #[usage(short = 'o', long, default = "packslip.key")]
    out: PathBuf,
}

impl RunWith<BinInfo> for Keygen {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        use std::io::Write as _;
        let pubkey = self.out.with_extension("pub");
        if self.out == pubkey {
            bail!(
                "secret and public key paths both resolve to {}",
                self.out.display()
            );
        }
        if self.out.exists() || pubkey.exists() {
            bail!(
                "{} or {} exists; not overwriting a key",
                self.out.display(),
                pubkey.display()
            );
        }
        let key = SecretKey::generate();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&self.out)
            .wrap_err_with(|| format!("creating {}", self.out.display()))?;
        file.write_all(key.to_file().as_bytes())?;
        let public_result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pubkey)
            .and_then(|mut public| public.write_all(key.public_key().to_file().as_bytes()));
        if let Err(err) = public_result {
            let _ = std::fs::remove_file(&self.out);
            return Err(err).wrap_err_with(|| format!("writing {}", pubkey.display()));
        }
        println!(
            "wrote {} and {} (key id {})",
            self.out.display(),
            pubkey.display(),
            key_id_hex(&key.public_key().key_id)
        );
        Ok(())
    }
}

/// Print the JSON schema of the release statement, or of the release list
#[derive(Debug, usage_rs::Args)]
struct Schema {
    /// The releases/v1 list instead of the release/v1 statement
    #[usage(long)]
    releases: bool,
}

impl RunWith<BinInfo> for Schema {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let schema = if self.releases {
            ReleaseListStatement::schema()
        } else {
            Statement::schema()
        };
        println!("{}", serde_json::to_string_pretty(&schema)?);
        Ok(())
    }
}

/// Print the statement inside a bundle, without verifying it
#[derive(Debug, usage_rs::Args)]
struct Show {
    /// The packslip.sigstore.json (or release list) to read
    #[usage(value_hint = usage_rs::ValueHint::FilePath)]
    bundle: PathBuf,
    /// The exact signed bytes instead of pretty JSON
    #[usage(long)]
    raw: bool,
}

impl RunWith<BinInfo> for Show {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let text = std::fs::read_to_string(&self.bundle)
            .wrap_err_with(|| format!("reading {}", self.bundle.display()))?;
        let payload = sigstore::peek_statement(&text)?;
        if self.raw {
            use std::io::Write as _;
            std::io::stdout().write_all(&payload)?;
            println!();
        } else {
            let value: serde_json::Value = serde_json::from_slice(&payload)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Ok(())
    }
}

/// How to sign: `oidc` (keyless, with the CI job's identity) or `key` (an
/// Ed25519 key given with --key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignWith {
    Oidc,
    Key,
}

impl std::str::FromStr for SignWith {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "oidc" | "sigstore-oidc" => Ok(SignWith::Oidc),
            "key" | "sigstore-key" => Ok(SignWith::Key),
            other => Err(format!("--sign must be oidc or key, got {other:?}")),
        }
    }
}

/// `vendor` or `repackager`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AttestorArg(Attestor);

impl std::str::FromStr for AttestorArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vendor" => Ok(AttestorArg(Attestor::Vendor)),
            "repackager" => Ok(AttestorArg(Attestor::Repackager)),
            other => Err(format!(
                "--attested-by must be vendor or repackager, got {other:?}"
            )),
        }
    }
}

/// `source` or `semver`, as mise's registry spells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionOrderArg(VersionOrder);

impl std::str::FromStr for VersionOrderArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "source" => Ok(VersionOrderArg(VersionOrder::Source)),
            "semver" => Ok(VersionOrderArg(VersionOrder::Semver)),
            other => Err(format!(
                "--version-order must be source or semver, got {other:?}"
            )),
        }
    }
}

/// Resolve who signs from the shared signing flags.
fn signer(key: &Option<PathBuf>, sign: Option<SignWith>, no_log: bool) -> Result<Signer> {
    let sign_with = sign.unwrap_or(if key.is_some() {
        SignWith::Key
    } else {
        SignWith::Oidc
    });
    match sign_with {
        SignWith::Key => {
            let Some(path) = key else {
                bail!("--sign key needs --key");
            };
            let key_text = std::fs::read_to_string(path)
                .wrap_err_with(|| format!("reading {}", path.display()))?;
            Ok(Signer::Key {
                key: SecretKey::parse(&key_text)?,
                log: !no_log,
            })
        }
        SignWith::Oidc => {
            if key.is_some() {
                bail!("--key is for --sign key");
            }
            if no_log {
                bail!("keyless signatures are always logged; --no-log is for --key");
            }
            Ok(Signer::Oidc(sigstore::ambient_identity()?))
        }
    }
}

/// Create and sign a packslip for a release
///
/// Digests every artifact, infers os/arch/libc/format from file names
/// (override with path:os/arch[/libc], add @variant to tell apart two
/// builds for one platform), and writes packslip.sigstore.json into --out.
/// Inside a CI job the document is signed keylessly with the job's
/// identity. With --key it is signed with an Ed25519 key from `packslip
/// keygen`. Either way the signature is logged to Rekor.
#[derive(Debug, usage_rs::Args)]
struct Create {
    /// The project's name: a host path such as github.com/owner/repo, or
    /// github.com/owner/repo/tool for one tool of a monorepo
    #[usage(long)]
    project: String,
    /// The release version
    #[usage(long)]
    version: String,
    /// Artifact files, optionally as path[:os/arch[/libc]][@variant]
    #[usage(required = true)]
    artifacts: Vec<String>,
    /// Sign with this secret key instead of a CI identity
    #[usage(short = 'k', long, value_hint = usage_rs::ValueHint::FilePath)]
    key: Option<PathBuf>,
    /// How to sign; defaults to key when --key is given, else oidc
    #[usage(long)]
    sign: Option<SignWith>,
    /// With --key: do not record the signature in Rekor. Consumers must
    /// then opt in with --allow-unlogged
    #[usage(long)]
    no_log: bool,
    /// Directory to write into
    #[usage(short = 'o', long, default = ".")]
    out: PathBuf,
    /// Download URL prefix for the artifacts
    #[usage(long)]
    url_base: Option<String>,
    /// Download URL for one artifact, as FILENAME=URL (repeatable)
    #[usage(long)]
    url: Vec<String>,
    /// Source repository URL
    #[usage(long)]
    source_repo: Option<String>,
    /// Source commit
    #[usage(long)]
    commit: Option<String>,
    /// Source tag
    #[usage(long)]
    tag: Option<String>,
    /// RFC 3339 publish time; defaults to now
    #[usage(long)]
    published_at: Option<String>,
    /// Mark the release as not for general use
    #[usage(long)]
    prerelease: bool,
    /// The release channel: stable, beta, nightly
    #[usage(long)]
    channel: Option<String>,
    /// How consumers order this project's versions: source (the release
    /// list's order, the default) or semver (strict MAJOR.MINOR.PATCH,
    /// calver included)
    #[usage(long)]
    version_order: Option<VersionOrderArg>,
    /// URL of the release notes
    #[usage(long)]
    notes_url: Option<String>,
    /// SBOM URL
    #[usage(long)]
    sbom: Option<String>,
    /// The version this release replaces
    #[usage(long)]
    supersedes: Option<String>,
    /// Executable inside every archive, as PATH or NAME=PATH (repeatable)
    #[usage(long)]
    bin: Vec<String>,
    /// Something else the release ships, as KIND[/QUALIFIER]=SOURCE:VALUE
    /// where SOURCE is archive (a path inside every archive), asset (a
    /// separate release file, by local path), repo (a path at --commit),
    /// or exec (a command whose stdout is the file). Kinds: completion/SHELL
    /// (or completion/SHELL,SHELL with exec and a {shell} placeholder),
    /// man, cli-spec/FORMAT[/BIN], skill/NAME, desktop, icon, app.
    /// Example: 'completion/zsh=archive:share/zsh/site-functions/_tool'
    /// (repeatable)
    #[usage(long)]
    resource: Vec<String>,
    /// Provenance URL for every artifact (repeatable, positional order)
    #[usage(long)]
    provenance: Vec<String>,
    /// Who makes the claim: vendor (default) or repackager
    #[usage(long)]
    attested_by: Option<AttestorArg>,
    /// What a repackager checked, as KIND or KIND=DETAIL (repeatable)
    #[usage(long)]
    evidence: Vec<String>,
    /// Record only sha256, not sha512 as well
    #[usage(long)]
    no_sha512: bool,
}

/// `PATH` or `NAME=PATH`.
fn parse_bin(spec: &str) -> Bin {
    match spec.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => Bin::named(path, name),
        _ => Bin::new(spec),
    }
}

/// A parsed `--resource`: the entry, and the local file behind an `asset`
/// source.
struct ResourceSpec {
    resource: Resource,
    asset_path: Option<PathBuf>,
}

/// `KIND[/QUALIFIER...]=SOURCE:VALUE`. `completion/zsh=archive:PATH`,
/// `completion/bash,zsh,fish=exec:tool completion {shell}`,
/// `skill/NAME=repo:PATH`, `cli-spec/usage[/BIN]=exec:tool usage`,
/// `man=archive:PATH`, `app=archive:Tool.app`. With one `--bin`, a
/// `cli-spec` may omit the executable's name.
fn parse_resource(spec: &str, default_bin: Option<&str>) -> Result<ResourceSpec> {
    let Some((head, value)) = spec.split_once('=') else {
        bail!("--resource wants KIND[/QUALIFIER]=SOURCE:VALUE, got {spec:?}");
    };
    let mut parts = head.split('/');
    let kind = parts.next().unwrap_or_default();
    if kind.is_empty() {
        bail!("--resource {spec:?} has an empty kind");
    }
    let qualifiers: Vec<&str> = parts.collect();
    let mut resource = Resource::new(kind);
    match (kind, qualifiers.as_slice()) {
        ("completion", [shell]) if shell.contains(',') => {
            resource.shells = shell
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
        ("completion", [shell]) => resource.shell = Some(shell.trim().to_string()),
        ("completion", _) => bail!("--resource {spec:?}: completion wants completion/SHELL"),
        ("cli-spec", [format]) => {
            resource.format = Some(format.to_string());
            let Some(bin) = default_bin else {
                bail!("--resource {spec:?}: say which executable, as cli-spec/{format}/BIN");
            };
            resource.bin = Some(bin.to_string());
        }
        ("cli-spec", [format, bin]) => {
            resource.format = Some(format.to_string());
            resource.bin = Some(bin.to_string());
        }
        ("cli-spec", _) => bail!("--resource {spec:?}: cli-spec wants cli-spec/FORMAT[/BIN]"),
        ("skill", [name]) => resource.name = Some(name.to_string()),
        ("skill", _) => bail!("--resource {spec:?}: skill wants skill/NAME"),
        (_, []) => {}
        (_, [name]) => resource.name = Some(name.to_string()),
        (_, _) => bail!("--resource {spec:?}: {kind} takes at most one qualifier"),
    }
    let mut asset_path = None;
    match value.split_once(':') {
        Some(("archive", path)) => resource.archive = Some(path.to_string()),
        Some(("repo", path)) => resource.repo = Some(path.to_string()),
        Some(("exec", argv)) => {
            resource.exec = argv.split_whitespace().map(str::to_string).collect();
        }
        Some(("asset", path)) => {
            let path = PathBuf::from(path);
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                bail!("--resource {spec:?}: asset wants a file path");
            };
            resource.asset = Some(name.to_string());
            asset_path = Some(path);
        }
        _ => bail!(
            "--resource {spec:?}: the value must start with archive:, asset:, repo:, or exec:"
        ),
    }
    Ok(ResourceSpec {
        resource,
        asset_path,
    })
}

/// `KIND` or `KIND=DETAIL`.
fn parse_evidence(spec: &str) -> Evidence {
    match spec.split_once('=') {
        Some((kind, detail)) => Evidence {
            kind: kind.to_string(),
            detail: Some(detail.to_string()),
        },
        None => Evidence {
            kind: spec.to_string(),
            detail: None,
        },
    }
}

impl RunWith<BinInfo> for Create {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        if self.source_repo.is_none() && (self.commit.is_some() || self.tag.is_some()) {
            bail!("--commit and --tag require --source-repo");
        }
        let attested_by = self.attested_by.map(|a| a.0).unwrap_or_default();
        if attested_by == Attestor::Vendor && !self.evidence.is_empty() {
            bail!("--evidence describes what a repackager checked; pass --attested-by repackager");
        }
        let signer = signer(&self.key, self.sign, self.no_log)?;
        let mut urls = std::collections::BTreeMap::new();
        for spec in &self.url {
            let Some((name, url)) = spec.split_once('=') else {
                bail!("--url wants FILENAME=URL, got {spec:?}");
            };
            urls.insert(name.to_string(), url.to_string());
        }
        let bins: Vec<Bin> = self.bin.iter().map(|s| parse_bin(s)).collect();
        let parsed: Vec<ArtifactSpec> = self.artifacts.iter().map(|s| parse_spec(s)).collect();
        let artifacts: Vec<ArtifactInput<'_>> = parsed
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let file_name = spec
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                ArtifactInput {
                    path: &spec.path,
                    os: spec.os.as_deref(),
                    arch: spec.arch.as_deref(),
                    libc: spec.libc.as_deref(),
                    variant: spec.variant.clone(),
                    url: urls.get(file_name).cloned(),
                    bin: bins.clone(),
                    requires: None,
                    provenance: self.provenance.get(i).cloned().into_iter().collect(),
                }
            })
            .collect();
        let default_bin = match bins.as_slice() {
            [only] => Some(only.name.as_str()),
            _ => None,
        };
        let mut resources = Vec::new();
        let mut asset_paths: Vec<PathBuf> = Vec::new();
        for spec in &self.resource {
            let parsed = parse_resource(spec, default_bin)?;
            if let Some(path) = parsed.asset_path
                && !asset_paths.contains(&path)
            {
                asset_paths.push(path);
            }
            resources.push(parsed.resource);
        }
        let assets: Vec<AssetInput<'_>> = asset_paths
            .iter()
            .map(|path| AssetInput {
                path,
                url: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|name| urls.get(name).cloned()),
            })
            .collect();
        for name in urls.keys() {
            let is_file = |path: &Path| path.file_name().and_then(|n| n.to_str()) == Some(name);
            if !parsed.iter().any(|s| is_file(&s.path)) && !asset_paths.iter().any(|p| is_file(p)) {
                bail!("--url names {name:?}, which is not among the artifacts or assets");
            }
        }
        let source = self.source_repo.as_ref().map(|repo| Source {
            repo: repo.clone(),
            commit: self.commit.clone(),
            tag: self.tag.clone(),
        });
        let created = packslip::create::create(&Request {
            published_at: self.published_at.as_deref(),
            prerelease: self.prerelease,
            channel: self.channel.as_deref(),
            version_order: self.version_order.map(|v| v.0).unwrap_or_default(),
            source,
            artifacts,
            resources,
            assets,
            url_base: self.url_base.as_deref(),
            notes_url: self.notes_url.as_deref(),
            sbom: self.sbom.as_deref(),
            supersedes: self.supersedes.as_deref(),
            attested_by,
            evidence: self.evidence.iter().map(|s| parse_evidence(s)).collect(),
            sha512: !self.no_sha512,
            ..Request::new(&self.project, &self.version, signer.identity())
        })?;
        let identity = created.statement.predicate.identity.clone();
        let bundle = sigstore::sign(signer, &created.document)?;
        std::fs::create_dir_all(&self.out)
            .wrap_err_with(|| format!("creating {}", self.out.display()))?;
        let path = self.out.join(bundle_name(&self.project));
        std::fs::write(&path, &bundle)?;
        let resource_count = created.statement.predicate.resources.len();
        println!(
            "wrote {} ({} artifact(s){}, signed by {}{}{})",
            path.display(),
            created.statement.predicate.artifacts.len(),
            if resource_count > 0 {
                format!(", {resource_count} resource(s)")
            } else {
                String::new()
            },
            identity.key_id,
            if attested_by == Attestor::Repackager {
                ", repackager-attested"
            } else {
                ""
            },
            if self.no_log { ", unlogged" } else { "" }
        );
        Ok(())
    }
}

/// An artifact argument: a path, optionally with `:os/arch[/libc]` and
/// `@variant`.
struct ArtifactSpec {
    path: PathBuf,
    os: Option<String>,
    arch: Option<String>,
    libc: Option<String>,
    variant: Option<String>,
}

/// Recognize only a well-formed `os/arch[/libc]` suffix and a trailing
/// `@variant`. In particular, colons inside timestamped directory names
/// remain part of the path, and so does an `@` inside a file name that
/// is not followed by a plain word.
fn parse_spec(spec: &str) -> ArtifactSpec {
    let (rest, variant) = match spec.rsplit_once('@') {
        Some((rest, variant)) if valid_word(variant) && !rest.is_empty() => {
            (rest, Some(variant.to_string()))
        }
        _ => (spec, None),
    };
    match rest.rsplit_once(':') {
        Some((path, platform)) if valid_platform(platform) => {
            let mut parts = platform.split('/');
            ArtifactSpec {
                path: PathBuf::from(path),
                os: parts.next().map(str::to_string),
                arch: parts.next().map(str::to_string),
                libc: parts.next().map(str::to_string),
                variant,
            }
        }
        _ => ArtifactSpec {
            path: PathBuf::from(rest),
            os: None,
            arch: None,
            libc: None,
            variant,
        },
    }
}

/// A variant is a word like `fips` or `install_only`, never something
/// with a dot in it, so `scoped@pkg-1.0.tgz` stays a file name.
fn valid_word(part: &str) -> bool {
    part.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_platform(platform: &str) -> bool {
    let parts: Vec<_> = platform.split('/').collect();
    (parts.len() == 2 || parts.len() == 3)
        && parts.iter().all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

/// Create and sign a project's release list
///
/// For a project on its own domain: lists released packslips with their
/// digests so consumers can discover releases, with an expiry and a
/// sequence number so they notice a stale or truncated list. Publish the
/// result at https://<host>/.well-known/packslip/<path>.json.
#[derive(Debug, usage_rs::Args)]
struct Releases {
    /// The project's name, which every listed packslip must carry
    #[usage(long)]
    project: String,
    /// Increases with every list published
    #[usage(long)]
    sequence: u64,
    /// How consumers order the listed versions: source (this list's order,
    /// the default) or semver
    #[usage(long)]
    version_order: Option<VersionOrderArg>,
    /// How long the list stays current: 30d, 12h, 2w
    #[usage(long, default = "30d")]
    valid_for: String,
    /// RFC 3339 generation time; defaults to now
    #[usage(long)]
    generated_at: Option<String>,
    /// A released packslip as URL=PATH: where consumers fetch it, and the
    /// local copy to read (repeatable)
    #[usage(long, required = true)]
    release: Vec<String>,
    /// Mark a listed release withdrawn, as URL=REASON (repeatable)
    #[usage(long)]
    yank: Vec<String>,
    /// Mark a listed release as a security fix, by URL (repeatable)
    #[usage(long)]
    security: Vec<String>,
    /// Sign with this secret key instead of a CI identity
    #[usage(short = 'k', long, value_hint = usage_rs::ValueHint::FilePath)]
    key: Option<PathBuf>,
    /// How to sign; defaults to key when --key is given, else oidc
    #[usage(long)]
    sign: Option<SignWith>,
    /// With --key: do not record the signature in Rekor
    #[usage(long)]
    no_log: bool,
    /// Where to write the list
    #[usage(short = 'o', long, default = "packslip-releases.sigstore.json")]
    out: PathBuf,
}

impl RunWith<BinInfo> for Releases {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let signer = signer(&self.key, self.sign, self.no_log)?;
        let mut pairs = Vec::new();
        for spec in &self.release {
            let Some((url, path)) = spec.split_once('=') else {
                bail!("--release wants URL=PATH, got {spec:?}");
            };
            pairs.push((url.to_string(), PathBuf::from(path)));
        }
        let mut yanked = std::collections::BTreeMap::new();
        for spec in &self.yank {
            let Some((url, reason)) = spec.split_once('=') else {
                bail!("--yank wants URL=REASON, got {spec:?}");
            };
            yanked.insert(url.to_string(), reason.to_string());
        }
        for url in yanked.keys().chain(self.security.iter()) {
            if !pairs.iter().any(|(u, _)| u == url) {
                bail!("{url} is not among the --release entries");
            }
        }
        let releases = pairs
            .iter()
            .map(|(url, path)| ListedRelease {
                url,
                bundle_path: path,
                yanked: yanked.get(url).cloned(),
                security: self.security.contains(url),
            })
            .collect();
        let created = packslip::create::create_release_list(&ListRequest {
            project: &self.project,
            generated_at: self.generated_at.as_deref(),
            valid_for: parse_duration(&self.valid_for)?,
            sequence: self.sequence,
            version_order: self.version_order.map(|v| v.0).unwrap_or_default(),
            releases,
            identity: signer.identity(),
        })?;
        let bundle = sigstore::sign(signer, &created.document)?;
        if let Some(parent) = self.out.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.out, &bundle)?;
        println!(
            "wrote {} ({} release(s), sequence {}, expires {})",
            self.out.display(),
            created.statement.predicate.releases.len(),
            created.statement.predicate.sequence,
            created.statement.predicate.expires_at
        );
        Ok(())
    }
}

/// `30d`, `12h`, `2w`, `90m`, `45s`.
fn parse_duration(s: &str) -> Result<std::time::Duration> {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (number, unit) = s.split_at(split);
    let number: u64 = number.parse().wrap_err_with(|| format!("duration {s:?}"))?;
    let seconds = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        _ => bail!("duration {s:?}: expected a unit of s, m, h, d or w"),
    };
    Ok(std::time::Duration::from_secs(number * seconds))
}

/// Verify a packslip, or a release list, against a pinned identity or key
///
/// Checks the bundle's signature and log entry, the statement, and the
/// digest and size of every artifact file given. Exits 1 on any failure.
/// A keyless document is checked against --identity, --identity-prefix,
/// and --issuer, or, for a project on github.com or gitlab.com, against
/// the repository the project name says signed it. A key-signed document
/// is checked against --pubkey.
#[derive(Debug, usage_rs::Args)]
struct Verify {
    /// The packslip.sigstore.json to verify
    #[usage(value_hint = usage_rs::ValueHint::FilePath)]
    bundle: PathBuf,
    /// The pinned public key file, or its base64 line
    #[usage(short = 'p', long)]
    pubkey: Option<String>,
    /// The exact certificate identity a keyless signer must have
    #[usage(long)]
    identity: Option<String>,
    /// A prefix the certificate identity must start with, such as
    /// https://github.com/owner/repo/
    #[usage(long)]
    identity_prefix: Option<String>,
    /// The OIDC issuer a keyless signer must have
    #[usage(long)]
    issuer: Option<String>,
    /// Accept a bundle without a transparency log entry
    #[usage(long)]
    allow_unlogged: bool,
    /// A sigstore trusted_root.json to use instead of the embedded one
    #[usage(long, value_hint = usage_rs::ValueHint::FilePath)]
    trusted_root: Option<PathBuf>,
    /// Artifact files to check against the document
    #[usage(short = 'a', long)]
    artifact: Vec<PathBuf>,
    /// Print the result as JSON
    #[usage(short = 'J', long)]
    json: bool,
}

/// An owned pin, so a `Trust` can borrow it.
enum Pin {
    Key(PublicKey),
    Identity(Policy),
}

impl Pin {
    fn as_trust(&self) -> Trust<'_> {
        match self {
            Pin::Key(key) => Trust::Key(key),
            Pin::Identity(policy) => Trust::Identity(policy),
        }
    }
}

impl RunWith<BinInfo> for Verify {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let text = std::fs::read_to_string(&self.bundle)
            .wrap_err_with(|| format!("reading {}", self.bundle.display()))?;
        let peeked: serde_json::Value = serde_json::from_slice(&sigstore::peek_statement(&text)?)
            .wrap_err("the bundle's payload is not JSON")?;
        let project = peeked["predicate"]["project"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let is_list = peeked["predicateType"] == RELEASES_PREDICATE_TYPE;

        let pin = match &self.pubkey {
            Some(text) => {
                if self.identity.is_some()
                    || self.identity_prefix.is_some()
                    || self.issuer.is_some()
                {
                    bail!(
                        "--pubkey pins a key; the identity flags pin a certificate. Pass one kind"
                    );
                }
                let pubkey_text = if Path::new(text).is_file() {
                    std::fs::read_to_string(text)?
                } else {
                    text.clone()
                };
                Pin::Key(PublicKey::parse(&pubkey_text)?)
            }
            None => {
                let explicit = Policy {
                    issuer: self.issuer.clone(),
                    identity: self.identity.clone(),
                    identity_prefix: self.identity_prefix.clone(),
                };
                Pin::Identity(if explicit.is_empty() {
                    Policy::for_project(&project).ok_or(sigstore::Error::NoPolicy(project))?
                } else {
                    explicit
                })
            }
        };
        let root_json = match &self.trusted_root {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .wrap_err_with(|| format!("reading {}", path.display()))?,
            ),
            None => None,
        };
        let trusted_root = sigstore::trusted_root(root_json.as_deref())?;
        let options = Options {
            require_log: !self.allow_unlogged,
            trusted_root: &trusted_root,
        };
        let artifacts: Vec<&Path> = self.artifact.iter().map(PathBuf::as_path).collect();

        if is_list {
            if !artifacts.is_empty() {
                bail!("a release list has no artifacts to check");
            }
            return match packslip::verify_release_list(&text, &pin.as_trust(), options) {
                Ok(verified) => {
                    let list = &verified.list.predicate;
                    if self.json {
                        println!("{}", serde_json::to_string_pretty(&verified.list)?);
                    } else {
                        let yanked = list.releases.iter().filter(|r| r.is_yanked()).count();
                        println!(
                            "ok: release list for {} sequence {} expires {} signed by {} ({}) listing {} release(s){}",
                            list.project,
                            list.sequence,
                            list.expires_at,
                            verified.key_id,
                            verified.scheme,
                            list.releases.len(),
                            if yanked > 0 {
                                format!(", {yanked} yanked")
                            } else {
                                String::new()
                            }
                        );
                    }
                    Ok(())
                }
                Err(err) => {
                    eprintln!("verification failed: {err}");
                    std::process::exit(1)
                }
            };
        }
        match packslip::verify(&text, &pin.as_trust(), options, &artifacts) {
            Ok(verified) => {
                if self.json {
                    println!("{}", serde_json::to_string_pretty(&verified)?);
                } else {
                    println!(
                        "ok: {} {}{} published {} signed by {} ({}){}{} ({} of {} artifact(s) checked{}{})",
                        verified.project,
                        verified.version,
                        if verified.prerelease {
                            " (prerelease)"
                        } else {
                            ""
                        },
                        verified.published_at,
                        verified.key_id,
                        verified.scheme,
                        if verified.attested_by == Attestor::Repackager {
                            " repackager-attested"
                        } else {
                            ""
                        },
                        match &verified.logged_at {
                            Some(at) => format!(" logged {at}"),
                            None => " unlogged".to_string(),
                        },
                        verified.checked_artifacts.len(),
                        verified.artifact_count,
                        if verified.provenance_linked {
                            ", provenance linked"
                        } else {
                            ""
                        },
                        if verified.resources.is_empty() {
                            String::new()
                        } else {
                            format!(", {} resource(s)", verified.resources.len())
                        }
                    );
                }
                Ok(())
            }
            Err(err) => {
                eprintln!("verification failed: {err}");
                std::process::exit(1)
            }
        }
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args: Vec<_> = std::env::args_os().collect();
    let argv = packslip::cli::argv(&args);
    let cli = packslip::cli::unwrap_or_exit(Cli::spec(), &argv, Cli::parse_from_argv(&argv));
    match cli.command {
        Some(command) => command.run_with(BIN),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_specs() {
        let spec = parse_spec("build/2026-09-01T12:00:00Z/tool.tar.gz");
        assert_eq!(
            spec.path,
            PathBuf::from("build/2026-09-01T12:00:00Z/tool.tar.gz")
        );
        assert!(spec.os.is_none());
        assert!(spec.variant.is_none());
        let spec = parse_spec("weird.bin:freebsd/riscv64");
        assert_eq!(spec.os.as_deref(), Some("freebsd"));
        assert_eq!(spec.arch.as_deref(), Some("riscv64"));
        let spec = parse_spec("tool.tar.gz:linux/x86_64/musl@fips");
        assert_eq!(spec.libc.as_deref(), Some("musl"));
        assert_eq!(spec.variant.as_deref(), Some("fips"));
        let spec = parse_spec("tool-fips.tar.gz@fips");
        assert_eq!(spec.path, PathBuf::from("tool-fips.tar.gz"));
        assert_eq!(spec.variant.as_deref(), Some("fips"));
        let spec = parse_spec("scoped@pkg-1.0.tgz");
        assert_eq!(spec.path, PathBuf::from("scoped@pkg-1.0.tgz"));
        assert!(spec.variant.is_none(), "a variant is a plain word");
    }

    #[test]
    fn bins_and_evidence_parse() {
        assert_eq!(parse_bin("bin/tool"), Bin::new("bin/tool"));
        assert_eq!(
            parse_bin("oxlint=bin/oxlint-x86_64"),
            Bin::named("bin/oxlint-x86_64", "oxlint")
        );
        assert_eq!(parse_evidence("pkgbuild-checksums").detail, None);
        assert_eq!(
            parse_evidence("apt-release-gpg=3FEF9748").detail.as_deref(),
            Some("3FEF9748")
        );
    }

    #[test]
    fn resources_parse() {
        let r = parse_resource(
            "completion/zsh=archive:share/zsh/site-functions/_tool",
            None,
        )
        .unwrap();
        assert_eq!(r.resource.kind, "completion");
        assert_eq!(r.resource.shell.as_deref(), Some("zsh"));
        assert_eq!(
            r.resource.archive.as_deref(),
            Some("share/zsh/site-functions/_tool")
        );
        assert!(r.asset_path.is_none());
        let r = parse_resource(
            "completion/bash,zsh,fish=exec:tool completion {shell}",
            None,
        )
        .unwrap();
        assert_eq!(r.resource.shells, ["bash", "zsh", "fish"]);
        assert_eq!(r.resource.exec, ["tool", "completion", "{shell}"]);
        let r = parse_resource(
            "completion/bash, zsh,,fish =exec:tool completion {shell}",
            None,
        )
        .unwrap();
        assert_eq!(r.resource.shells, ["bash", "zsh", "fish"]);
        let r = parse_resource("cli-spec/usage=exec:tool usage", Some("tool")).unwrap();
        assert_eq!(r.resource.format.as_deref(), Some("usage"));
        assert_eq!(r.resource.bin.as_deref(), Some("tool"));
        let r = parse_resource("cli-spec/usage/other=repo:specs/other.kdl", Some("tool")).unwrap();
        assert_eq!(r.resource.bin.as_deref(), Some("other"));
        assert_eq!(r.resource.repo.as_deref(), Some("specs/other.kdl"));
        assert!(parse_resource("cli-spec/usage=repo:x", None).is_err());
        let r = parse_resource("skill/tool=asset:dist/tool-skill.tar.gz", None).unwrap();
        assert_eq!(r.resource.name.as_deref(), Some("tool"));
        assert_eq!(r.resource.asset.as_deref(), Some("tool-skill.tar.gz"));
        assert_eq!(r.asset_path, Some(PathBuf::from("dist/tool-skill.tar.gz")));
        let r = parse_resource("man=archive:man/man1/tool.1", None).unwrap();
        assert!(r.resource.name.is_none());
        let r = parse_resource("font/Tool=archive:fonts/Tool.ttf", None).unwrap();
        assert_eq!(r.resource.name.as_deref(), Some("Tool"));
        for bad in [
            "man",
            "=archive:x",
            "man=x",
            "man=ftp:x",
            "completion=archive:x",
            "skill=archive:x",
            "man/a/b=archive:x",
        ] {
            assert!(parse_resource(bad, None).is_err(), "{bad}");
        }
    }

    #[test]
    fn bundle_names() {
        assert_eq!(bundle_name("github.com/jdx/mise"), "packslip.sigstore.json");
        assert_eq!(bundle_name("mise.jdx.dev"), "packslip.sigstore.json");
        assert_eq!(bundle_name("example.com/tool"), "packslip.sigstore.json");
        assert_eq!(
            bundle_name("github.com/oxc-project/oxc/oxlint"),
            "packslip.oxlint.sigstore.json"
        );
        assert_eq!(
            bundle_name("github.com/biomejs/biome/crates/cli"),
            "packslip.crates-cli.sigstore.json"
        );
    }

    #[test]
    fn durations() {
        assert_eq!(
            parse_duration("30d").unwrap(),
            std::time::Duration::from_secs(30 * 86_400)
        );
        assert_eq!(
            parse_duration("2w").unwrap(),
            std::time::Duration::from_secs(14 * 86_400)
        );
        assert!(parse_duration("3x").is_err());
        assert!(parse_duration("").is_err());
    }
}
