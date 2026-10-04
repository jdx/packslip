//! The `packslip` binary: create, releases, verify, pin, show, keygen,
//! schema, completion, and version.

use std::path::{Path, PathBuf};

use eyre::{Context as _, Result, bail};
use packslip::cli::{BinInfo, Version};
#[cfg(all(feature = "create", feature = "manifest"))]
use packslip::create::{ArtifactInput, AssetInput, ListRequest, ListedRelease, Request};
use packslip::fingerprint::Fingerprint;
#[cfg(all(feature = "create", feature = "manifest"))]
use packslip::manifest::Manifest;
use packslip::minisign::PublicKey;
#[cfg(feature = "sign")]
use packslip::minisign::{SecretKey, key_id_hex};
use packslip::model::{Attestor, RELEASES_PREDICATE_TYPE};
#[cfg(all(feature = "create", feature = "manifest"))]
use packslip::model::{Bin, Evidence, Extensions, RequiredBin, Resource, Source};
#[cfg(feature = "schema")]
use packslip::model::{ReleaseListStatement, Statement};
#[cfg(all(feature = "create", feature = "manifest"))]
use packslip::sigstore::Signer;
use packslip::sigstore::{self, Policy, Trust};
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
#[cfg(all(feature = "create", feature = "manifest"))]
pub fn bundle_name(project: &str) -> String {
    match packslip::model::repository_subpath(project) {
        Some(sub) => format!("packslip.{}.sigstore.json", sub.replace('/', "-")),
        None => "packslip.sigstore.json".to_string(),
    }
}

/// A signed release manifest: what shipped, and how to verify it
///
/// A vendor runs `packslip create` in its release job to sign one document
/// listing every artifact, then uploads it beside them. Consumers check that
/// document, and the files they download, with `packslip verify` against a
/// pinned identity, key, or signer fingerprint. See https://packslip.dev.
#[derive(usage_rs::Cli)]
#[usage(completion = true)]
#[usage(
    name = "packslip",
    bin = "packslip",
    version,
    author = "Jeff Dickey <@jdx>, Shunsuke Suzuki <@suzuki-shunsuke>",
    arg_required_else_help
)]
struct Cli {
    #[usage(subcommand)]
    command: Option<Commands>,
}

#[derive(usage_rs::Subcommands)]
#[usage(run_with)]
enum Commands {
    Completion(Completion),
    #[cfg(all(feature = "create", feature = "manifest"))]
    Create(Box<Create>),
    #[cfg(feature = "install-cli")]
    Install(Box<Install>),
    #[cfg(feature = "sign")]
    Keygen(Keygen),
    Pin(Box<Pin>),
    #[cfg(all(feature = "create", feature = "manifest"))]
    Releases(Box<Releases>),
    #[cfg(feature = "schema")]
    Schema(Schema),
    Show(Show),
    #[usage(hide)]
    Usage(Usage),
    Verify(Verify),
    Version(Version),
}

/// Install an authenticated upstream release and export its declared commands
///
/// Keeps the complete installation tree. Does not run downloaded code, install
/// dependencies, or edit shell configuration. Use --system for a shared install.
#[cfg(feature = "install-cli")]
#[derive(Debug, usage_rs::Args)]
struct Install {
    /// Project name (owner/repo, github.com/owner/repo[/subpath], or host[/path])
    #[usage(arg)]
    project: String,
    /// Version, prefix, tag, or latest (the default)
    #[usage(long)]
    version: Option<String>,
    /// Install for all users; Unix root uses this scope by default
    #[usage(long)]
    system: bool,
    /// Explicitly select the current user's installation scope
    #[usage(long)]
    user: bool,
    /// Replace conflicting command entries and unmarked installation directories
    #[usage(long)]
    force: bool,
    /// Use only locally cached, authenticated metadata and artifacts
    #[usage(long)]
    offline: bool,
    /// Override the installation tree without moving trust state
    #[usage(long, value_hint = usage_rs::ValueHint::DirPath)]
    install_dir: Option<PathBuf>,
    /// Override the command directory without moving trust state
    #[usage(long, value_hint = usage_rs::ValueHint::DirPath)]
    bin_dir: Option<PathBuf>,
    /// Select a publisher's named variant
    #[usage(long)]
    variant: Option<String>,
    /// Allowed signer fingerprint (repeatable, alternatives within this option)
    #[usage(long)]
    pin: Vec<String>,
    /// Ed25519 public key or path to its public-key file
    #[usage(long)]
    pubkey: Option<String>,
    /// Require this OIDC issuer as well as the project's default identity
    #[usage(long)]
    issuer: Option<String>,
    /// Require this exact certificate identity
    #[usage(long)]
    identity: Option<String>,
    /// Require this certificate identity prefix
    #[usage(long)]
    identity_prefix: Option<String>,
    /// Approve one exact displayed trust-change proposal (repeatable)
    #[usage(long)]
    accept_trust_change: Vec<String>,
    /// Administrator-supplied Sigstore trusted_root.json instead of TUF refresh
    #[usage(long, value_hint = usage_rs::ValueHint::FilePath)]
    trusted_root: Option<PathBuf>,
    /// Accept signatures without transparency-log entries for this attempt
    #[usage(long)]
    allow_unlogged: bool,
    /// Accept the selected artifact's incompatible host requirements
    #[usage(long)]
    allow_incompatible_host: bool,
    /// Local maximum expanded archive size in bytes
    #[usage(long, default = "10737418240")]
    max_extracted_size: u64,
    /// Local maximum archive entry count
    #[usage(long, default = "100000")]
    max_archive_entries: u64,
}

#[cfg(feature = "install-cli")]
impl RunWith<BinInfo> for Install {
    type Output = Result<()>;
    fn run_with(self, _: BinInfo) -> Result<()> {
        let request = packslip::install::Request {
            project: self.project,
            version: self.version,
            system: self.system,
            user: self.user,
            force: self.force,
            offline: self.offline,
            install_dir: self.install_dir,
            bin_dir: self.bin_dir,
            variant: self.variant,
            constraints: packslip::install_policy::Constraints {
                pins: self.pin,
                pubkey: self.pubkey,
                issuer: self.issuer,
                identity: self.identity,
                identity_prefix: self.identity_prefix,
            },
            accept_trust_change: self.accept_trust_change,
            trusted_root: self.trusted_root,
            allow_unlogged: self.allow_unlogged,
            allow_incompatible_host: self.allow_incompatible_host,
            limits: packslip::install_extract::Limits {
                bytes: self.max_extracted_size,
                entries: self.max_archive_entries,
            },
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let report = runtime.block_on(packslip::install::run(request))?;
        println!(
            "Installed {} {} at {}",
            report.project,
            report.receipt.version,
            report.receipt.tree.display()
        );
        if let Some(pin) = report.pin {
            println!("Signer pin: {pin}");
        }
        for warning in report.warnings {
            eprintln!("warning: {warning}");
        }
        for path in report.receipt.exports.keys() {
            println!("Command: {}", path.display());
        }
        Ok(())
    }
}

/// Generate a self-contained shell completion script
///
/// Print the script to stdout. Redirect it to a file where your shell loads
/// completions, as in the example. The script asks the installed packslip
/// for candidates, so it needs no other tool and keeps working after
/// upgrades.
#[derive(Debug, usage_rs::Args)]
#[usage(example(
    "mkdir -p ~/.local/share/bash-completion/completions && packslip completion bash > ~/.local/share/bash-completion/completions/packslip",
    header = "Install bash completions for your user"
))]
struct Completion {
    /// Shell to generate completions for
    #[usage(
        arg,
        choices("bash", "elvish", "zsh", "fish", "nu", "nushell", "powershell", "pwsh")
    )]
    shell: String,
}

impl RunWith<BinInfo> for Completion {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let shell = usage_rs::complete::Shell::from_name(&self.shell)
            .ok_or_else(|| eyre::eyre!("unsupported shell: {}", self.shell))?;
        print!("{}", Cli::completion_script(shell));
        Ok(())
    }
}

/// Generate a usage spec for the CLI
///
/// https://usage.jdx.dev
#[derive(Debug, usage_rs::Args)]
struct Usage;

impl RunWith<BinInfo> for Usage {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        println!(
            "// @generated by `packslip usage`\n{}",
            Cli::to_kdl().trim_end()
        );
        Ok(())
    }
}

/// Generate an Ed25519 key pair for key-signed releases (sigstore-key)
///
/// Write a new secret key to --out (a hex Ed25519 seed, mode 0600 on Unix)
/// and its public key, in minisign format, to the same path with the
/// extension replaced by .pub: --out release.key writes release.key and
/// release.pub. Refuses to overwrite either file.
///
/// Sign with `packslip create --key release.key`, and give consumers
/// release.pub for `packslip verify --pubkey`. Keep the secret key private.
/// A CI job with an OIDC identity can sign keyless and needs no key.
#[derive(Debug, usage_rs::Args)]
#[usage(example(
    "packslip keygen --out release.key",
    header = "Write release.key and release.pub"
))]
#[cfg(feature = "sign")]
struct Keygen {
    /// Where to write the secret key
    #[usage(short = 'o', long, default = "packslip.key")]
    out: PathBuf,
}

#[cfg(feature = "sign")]
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

/// Print the JSON schema for a decoded release statement
///
/// Use --releases for the release-list statement schema. These schemas
/// describe the in-toto payload, not the enclosing sigstore bundle. Both are
/// published at https://packslip.dev/schema/release-v1.json and
/// https://packslip.dev/schema/releases-v1.json.
#[derive(Debug, usage_rs::Args)]
#[cfg(feature = "schema")]
struct Schema {
    /// Print the releases/v1 list schema instead of the release/v1 statement
    /// schema
    #[usage(long)]
    releases: bool,
}

#[cfg(feature = "schema")]
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
    /// Release bundle or release list to read
    #[usage(value_hint = usage_rs::ValueHint::FilePath)]
    bundle: PathBuf,
    /// Print the signed payload followed by a newline, without pretty-printing
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
#[cfg(all(feature = "create", feature = "manifest"))]
enum SignWith {
    Oidc,
    Key,
}

#[cfg(all(feature = "create", feature = "manifest"))]
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
#[cfg(all(feature = "create", feature = "manifest"))]
struct AttestorArg(Attestor);

#[cfg(all(feature = "create", feature = "manifest"))]
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

/// Resolve who signs from the shared signing flags.
#[cfg(all(feature = "create", feature = "manifest"))]
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
/// Hash local artifacts, infer platforms and formats from file names, and
/// write the signed bundle (packslip.sigstore.json, or a per-tool name for a
/// GitHub monorepo tool; see --out) into the --out directory. Use --manifest
/// for per-artifact paths, formats, requirements, and scoped resources. No
/// files are uploaded.
///
/// By default, create signs keyless with an OIDC token: the one in
/// SIGSTORE_ID_TOKEN when it is set, or else the CI job's own identity,
/// which on GitHub Actions needs the `id-token: write` permission. Pass --key
/// to sign with a key from `packslip keygen` instead. Either way, the
/// signature is recorded in the Rekor transparency log. With --key, --no-log
/// skips the log entry, and consumers must then accept the release with
/// --allow-unlogged.
///
/// The TOML manifest and more examples:
/// https://packslip.dev/docs/describing-releases/
#[derive(Debug, usage_rs::Args)]
#[usage(
    example(
        "packslip create \\\n        --project github.com/owner/mytool \\\n        --version 1.2.3 \\\n        --bin mytool \\\n        --url-base https://github.com/owner/mytool/releases/download/v1.2.3 \\\n        --out dist \\\n        dist/mytool-1.2.3-linux-x64.tar.gz \\\n        dist/mytool-1.2.3-darwin-arm64.tar.gz",
        header = "Sign keyless in a CI job"
    ),
    example(
        "packslip create --manifest release.toml --out dist",
        header = "Describe the artifacts in a TOML manifest"
    ),
    example(
        "packslip create \\\n        --project mytool.example.com \\\n        --version 1.2.3 \\\n        --bin mytool \\\n        --url-base https://mytool.example.com/v1.2.3 \\\n        --key release.key \\\n        --out dist \\\n        dist/mytool-1.2.3-linux-x64.tar.gz",
        header = "Sign with a key from packslip keygen"
    )
)]
#[cfg(all(feature = "create", feature = "manifest"))]
struct Create {
    /// The project's name: a host path such as github.com/owner/repo, or
    /// github.com/owner/repo/tool for one tool of a monorepo. Required
    /// unless the manifest sets it
    #[usage(long, help_heading = "Input")]
    project: Option<String>,
    /// Semver release version, such as 1.2.3. Required unless the manifest
    /// sets it
    #[usage(long, help_heading = "Input")]
    version: Option<String>,
    /// Files to describe, each as PATH, PATH:OS/ARCH[/LIBC], or PATH:any,
    /// with an optional @VARIANT
    ///
    /// Without a platform suffix, create infers the platform from the file
    /// name. Give any as ARCH or LIBC to leave that field out, for a build
    /// that runs on every architecture or C library; PATH:any leaves out OS,
    /// ARCH, and LIBC. These files join the manifest's artifacts; a file the
    /// manifest lists under the same file name keeps its manifest entry, and
    /// its platform suffix and @VARIANT here are ignored.
    artifacts: Vec<String>,
    /// A TOML manifest giving per-artifact executables, formats,
    /// requirements, platforms, and the release's resources; its artifact and
    /// asset paths are relative to the working directory, not to the
    /// manifest. See
    /// https://packslip.dev/docs/describing-releases/#use-a-toml-manifest
    #[usage(short = 'm', long, value_hint = usage_rs::ValueHint::FilePath, help_heading = "Input")]
    manifest: Option<PathBuf>,
    /// Sign with this secret key instead of a CI identity
    #[usage(short = 'k', long, value_hint = usage_rs::ValueHint::FilePath, help_heading = "Signing")]
    key: Option<PathBuf>,
    /// How to sign: oidc (keyless) or key (needs --key). Optional; inferred
    /// from whether --key is given
    #[usage(long, help_heading = "Signing")]
    sign: Option<SignWith>,
    /// With --key: do not record the signature in Rekor. Consumers must
    /// then opt in with --allow-unlogged
    #[usage(long, help_heading = "Signing")]
    no_log: bool,
    /// Directory to write the bundle into, created if missing
    ///
    /// The bundle is packslip.sigstore.json, or packslip.TOOL.sigstore.json
    /// for a tool in a GitHub monorepo, such as github.com/owner/repo/TOOL (a
    /// deeper path has each / replaced by -). An existing bundle is
    /// replaced. Artifacts are not copied.
    #[usage(short = 'o', long, default = ".", help_heading = "Output")]
    out: PathBuf,
    /// Download URL prefix: each artifact and resource asset gets
    /// PREFIX/FILENAME unless --url or the manifest gives its URL
    #[usage(long, help_heading = "Download URLs")]
    url_base: Option<String>,
    /// Download URL for one artifact or resource asset, as FILENAME=URL (repeatable)
    #[usage(long, help_heading = "Download URLs")]
    url: Vec<String>,
    /// Format of one artifact whose name does not say, as FILENAME=FORMAT:
    /// an archive (tar.xz, tar.gz, tar.zst, tar.bz2, tgz, tar, zip, 7z), a
    /// single compressed executable (gz, xz, zst, bz2), an installer (deb,
    /// rpm, dmg, pkg, msi, msix, exe, appimage), raw for a bare
    /// executable, or a type of your own (repeatable)
    #[usage(long, help_heading = "Artifacts")]
    format: Vec<String>,
    /// Source repository URL, such as https://github.com/owner/repo
    #[usage(long, help_heading = "Source")]
    source_repo: Option<String>,
    /// Commit the release was built from; needs --source-repo or the
    /// manifest's source.repo. A resource with a repo: source requires it,
    /// and consumers read that file at this commit
    #[usage(long, help_heading = "Source")]
    commit: Option<String>,
    /// Git tag of the release, such as v1.2.3; needs --source-repo or the
    /// manifest's source.repo
    ///
    /// Consumers that list a GitHub project's versions from its tags do not
    /// see the release when the tag does not name the release's version,
    /// unless a signed release list names the release. create warns when that
    /// happens.
    #[usage(long, help_heading = "Source")]
    tag: Option<String>,
    /// RFC 3339 publish time; defaults to now
    #[usage(long, help_heading = "Release metadata")]
    published_at: Option<String>,
    /// URL of the release notes
    #[usage(long, help_heading = "Release metadata")]
    notes_url: Option<String>,
    /// Release-level extension as NAME=JSON, where NAME is who defines it
    /// (a consumer such as mise, or a domain the vendor controls) and JSON
    /// is its value. Example: 'example.com={"build_id":"20260901.3"}'
    /// (repeatable)
    #[usage(long, help_heading = "Release metadata")]
    extension: Vec<String>,
    /// Executable in every artifact: a NAME to find inside each archive, a
    /// PATH from the archive root, or NAME=PATH when the command's name
    /// differs from the file's (repeatable)
    ///
    /// A PATH includes any top-level directory of the archive. For a bare
    /// executable, give the command name it installs as.
    #[usage(long, help_heading = "Artifacts")]
    bin: Vec<String>,
    /// Another file the release ships, such as a completion script, man
    /// page, or SBOM, as KIND[/QUALIFIER][@OS[/ARCH[/LIBC]]]=SOURCE:VALUE
    /// (repeatable)
    ///
    /// SOURCE is archive (a path inside the artifact, from its root), asset
    /// (a separate release file, by local path), repo (a path in the source
    /// repository at --commit), or exec (a command whose stdout is the file;
    /// leading NAME=value words set its environment).
    ///
    /// Known kinds: completion/SHELL[/BIN] (completion/SHELL,SHELL[/BIN]
    /// with exec and a {shell} placeholder), man[/BIN],
    /// cli-spec/FORMAT[/BIN], skill/NAME, sbom/FORMAT, desktop, icon, app.
    /// Any other kind takes at most one qualifier, its name.
    ///
    /// An @ scope limits the entry to the artifacts of that platform, for a
    /// layout that differs across them; release.toml can also limit an
    /// entry to one artifact. Examples:
    /// 'completion/zsh=archive:share/zsh/site-functions/_tool',
    /// 'man@linux=archive:share/man/man1/tool.1'. See
    /// https://packslip.dev/docs/resources/#write-a-resource-entry
    #[usage(long, help_heading = "Resources")]
    resource: Vec<String>,
    /// Provenance URL for an artifact, as FILENAME=URL, or bare URLs in
    /// the order of the ARTIFACTS arguments (repeatable)
    #[usage(long, help_heading = "Artifacts")]
    provenance: Vec<String>,
    /// Who makes the claim: vendor (default) or repackager
    #[usage(long, help_heading = "Repackagers")]
    attested_by: Option<AttestorArg>,
    /// What a repackager checked, as KIND or KIND=DETAIL (repeatable)
    #[usage(long, help_heading = "Repackagers")]
    evidence: Vec<String>,
    /// Record only sha256, not sha512 as well
    #[usage(long, help_heading = "Artifacts")]
    no_sha512: bool,
    /// A command the executables need on PATH, as bin:NAME or bin:NAME@MIN
    /// where MIN is the lowest version that works. Example: bin:java@17
    /// (repeatable)
    #[usage(long, help_heading = "Artifacts")]
    require: Vec<String>,
    /// Do not read the executables for the shared libraries and C library
    /// they load from the host
    ///
    /// With this flag, create still finds each --bin inside the archives but
    /// does not derive requires.libs; a libs list in release.toml is recorded
    /// as written, unchecked. A Linux artifact whose platform and file name
    /// give no C library is recorded as gnu, even a static build.
    #[usage(long, help_heading = "Artifacts")]
    no_libs: bool,
    /// Keyless only: write identity.pin_workflow: false, so consumers hold
    /// later releases to the signing repository instead of to the workflow
    /// file that signs this one
    ///
    /// For a vendor whose releases are signed by more than one workflow of
    /// its repository. The signer must still be a workflow of that
    /// repository. A consumer that last accepted a release without the flag
    /// refuses the first one with it until a person approves it. See
    /// https://packslip.dev/release/v1/#workflow-pinning
    #[usage(long, help_heading = "Signing")]
    no_pin_workflow: bool,
}

/// The `identity` block the signer declares, with `pin_workflow: false`
/// when the publisher asked for it. Only a keyless signer has a workflow.
#[cfg(all(feature = "create", feature = "manifest"))]
fn declared_identity(signer: &Signer, no_pin_workflow: bool) -> Result<packslip::model::Identity> {
    let mut identity = signer.identity();
    if no_pin_workflow {
        if identity.scheme != packslip::model::Scheme::SigstoreOidc {
            bail!("--no-pin-workflow applies to keyless signing; a key has no workflow");
        }
        identity.pin_workflow = Some(false);
    }
    Ok(identity)
}

/// `PATH` or `NAME=PATH`.
#[cfg(all(feature = "create", feature = "manifest"))]
fn parse_bin(spec: &str) -> Bin {
    match spec.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => Bin::named(path, name),
        _ => Bin::new(spec),
    }
}

/// A parsed `--resource`: the entry, and the local file behind an `asset`
/// source.
#[cfg(all(feature = "create", feature = "manifest"))]
struct ResourceSpec {
    resource: Resource,
    asset_path: Option<PathBuf>,
}

/// `KIND[/QUALIFIER...][@os[/arch[/libc]]]=SOURCE:VALUE`.
/// `completion/zsh=archive:PATH`,
/// `completion/bash,zsh,fish=exec:tool completion {shell}`,
/// `completion/bash,zsh=exec:COMPLETE={shell} tool` (leading `NAME=value`
/// words become `env`, as at a shell prompt),
/// `skill/NAME=repo:PATH`, `cli-spec/usage[/BIN]=exec:tool usage`,
/// `man=archive:PATH`, `man@linux=archive:PATH`, `app=archive:Tool.app`.
/// With one `--bin`, a `cli-spec` may omit the executable's name.
///
/// The scope is read from the head, which is a small closed grammar of
/// words, so an `@` there is always the scope and never part of a name.
/// The value is left alone, and an `@` inside an exec argv or a path
/// stays where it is.
#[cfg(all(feature = "create", feature = "manifest"))]
fn parse_resource(spec: &str, default_bin: Option<&str>) -> Result<ResourceSpec> {
    let Some((head, value)) = spec.split_once('=') else {
        bail!("--resource wants KIND[/QUALIFIER]=SOURCE:VALUE, got {spec:?}");
    };
    let (head, scope) = match head.split_once('@') {
        Some((head, scope)) => (head, Some(scope)),
        None => (head, None),
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
        ("completion", [shell, bin]) => {
            if shell.contains(',') {
                resource.shells = shell
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            } else {
                resource.shell = Some(shell.trim().to_string());
            }
            resource.bin = Some(bin.to_string());
        }
        ("completion", _) => {
            bail!("--resource {spec:?}: completion wants completion/SHELL[/BIN]")
        }
        ("man", []) => {}
        ("man", [bin]) => resource.bin = Some(bin.to_string()),
        ("man", _) => bail!("--resource {spec:?}: man wants man[/BIN]"),
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
        ("sbom", [format]) => resource.format = Some(format.to_string()),
        ("sbom", _) => bail!("--resource {spec:?}: sbom wants sbom/FORMAT (cyclonedx, spdx)"),
        (_, []) => {}
        (_, [name]) => resource.name = Some(name.to_string()),
        (_, _) => bail!("--resource {spec:?}: {kind} takes at most one qualifier"),
    }
    if let Some(scope) = scope {
        if !valid_scope(scope) {
            bail!("--resource {spec:?}: the @ scope wants os[/arch[/libc]], got {scope:?}");
        }
        let mut fields = scope.split('/');
        resource.os = fields.next().map(str::to_string);
        resource.arch = fields.next().map(str::to_string);
        resource.libc = fields.next().map(str::to_string);
    }
    let mut asset_path = None;
    match value.split_once(':') {
        Some(("archive", path)) => resource.archive = Some(path.to_string()),
        Some(("repo", path)) => resource.repo = Some(path.to_string()),
        Some(("exec", argv)) => {
            let mut words = argv.split_whitespace().peekable();
            while let Some((name, value)) = words.peek().and_then(|w| w.split_once('=')) {
                if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    break;
                }
                resource.env.insert(name.to_string(), value.to_string());
                words.next();
            }
            resource.exec = words.map(str::to_string).collect();
            if resource.exec.is_empty() {
                bail!("--resource {spec:?}: exec wants a command after any NAME=value words");
            }
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

/// `bin:NAME` or `bin:NAME@MIN` for `--require`.
#[cfg(all(feature = "create", feature = "manifest"))]
fn parse_require(spec: &str) -> Result<RequiredBin> {
    let Some(rest) = spec.strip_prefix("bin:") else {
        bail!("--require wants bin:NAME or bin:NAME@MIN, got {spec:?}");
    };
    let (name, min) = match rest.split_once('@') {
        Some((name, min)) => (name, Some(min)),
        None => (rest, None),
    };
    if name.is_empty() {
        bail!("--require {spec:?} has an empty command name");
    }
    if min == Some("") {
        bail!("--require {spec:?} has an empty minimum version");
    }
    Ok(match min {
        Some(min) => RequiredBin::at_least(name, min),
        None => RequiredBin::new(name),
    })
}

/// `KIND` or `KIND=DETAIL`.
#[cfg(all(feature = "create", feature = "manifest"))]
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

/// `NAME=JSON` for `--extension`.
#[cfg(all(feature = "create", feature = "manifest"))]
fn parse_extension(spec: &str) -> Result<(String, serde_json::Value)> {
    let Some((name, json)) = spec.split_once('=') else {
        bail!("--extension wants NAME=JSON, got {spec:?}");
    };
    if name.is_empty() {
        bail!("--extension wants a NAME before the =, got {spec:?}");
    }
    let value = serde_json::from_str(json)
        .wrap_err_with(|| format!("--extension {name}: the value is not JSON: {json:?}"))?;
    Ok((name.to_string(), value))
}

/// The file name of a local path, for matching `--url`, `--provenance`,
/// and manifest entries against command-line artifacts.
#[cfg(all(feature = "create", feature = "manifest"))]
fn file_name_of(path: &Path) -> &str {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
}

#[cfg(all(feature = "create", feature = "manifest"))]
impl RunWith<BinInfo> for Create {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let manifest = match &self.manifest {
            Some(path) => Manifest::read(path)?,
            None => Manifest::default(),
        };
        // The command line wins over the manifest wherever both speak.
        let Some(project) = self.project.clone().or_else(|| manifest.project.clone()) else {
            bail!("--project is required unless the manifest names the project");
        };
        let Some(version) = self.version.clone().or_else(|| manifest.version.clone()) else {
            bail!("--version is required unless the manifest names the version");
        };
        let manifest_source = manifest.source.as_ref();
        let source_repo = self
            .source_repo
            .clone()
            .or_else(|| manifest_source.map(|s| s.repo.clone()));
        let commit = self
            .commit
            .clone()
            .or_else(|| manifest_source.and_then(|s| s.commit.clone()));
        let tag = self
            .tag
            .clone()
            .or_else(|| manifest_source.and_then(|s| s.tag.clone()));
        if source_repo.is_none() && (commit.is_some() || tag.is_some()) {
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
        let mut formats = std::collections::BTreeMap::new();
        for spec in &self.format {
            let Some((name, format)) = spec.split_once('=') else {
                bail!("--format wants FILENAME=FORMAT, got {spec:?}");
            };
            if format.is_empty() {
                bail!("--format {spec:?} has an empty format");
            }
            formats.insert(name.to_string(), format.to_string());
        }
        // The manifest's extensions first; a flag naming the same key wins.
        let mut extensions = manifest.extensions.clone();
        let mut from_flags = Extensions::new();
        for spec in &self.extension {
            let (name, value) = parse_extension(spec)?;
            if from_flags.insert(name.clone(), value.clone()).is_some() {
                bail!("--extension {name:?} is given twice");
            }
            extensions.insert(name, value);
        }
        let mut default_bins: Vec<Bin> = manifest.bin.clone();
        default_bins.extend(self.bin.iter().map(|s| parse_bin(s)));
        let mut requires_bin: Vec<RequiredBin> = Vec::new();
        for spec in &self.require {
            let required = parse_require(spec)?;
            if requires_bin.iter().any(|r| r.name == required.name) {
                bail!("--require names {:?} twice", required.name);
            }
            requires_bin.push(required);
        }
        let any_bin = !default_bins.is_empty()
            || manifest
                .artifacts
                .iter()
                .any(|a| !a.bins(&default_bins).is_empty());
        if !requires_bin.is_empty() && !any_bin {
            bail!("--require says what the executables need; name them with --bin");
        }
        let parsed: Vec<ArtifactArg> = self.artifacts.iter().map(|s| parse_spec(s)).collect();

        // Provenance: `FILENAME=URL`, or a bare URL for the artifact at the
        // same position on the command line.
        let mut provenance: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let mut positional = Vec::new();
        for spec in &self.provenance {
            match spec.split_once('=') {
                Some((name, url))
                    if !name.is_empty() && !name.contains('/') && !name.contains(':') =>
                {
                    provenance.insert(name.to_string(), url.to_string());
                }
                _ => positional.push(spec.clone()),
            }
        }
        for (arg, url) in parsed.iter().zip(positional) {
            provenance
                .entry(file_name_of(&arg.path).to_string())
                .or_insert(url);
        }
        let provenance_of = |name: &str, own: &[String]| -> Vec<String> {
            let mut all = own.to_vec();
            if let Some(url) = provenance.get(name)
                && !all.contains(url)
            {
                all.push(url.clone());
            }
            all
        };

        // The manifest's artifacts, then those on the command line that it
        // does not already describe, so `dist/*` and a manifest coexist.
        let mut artifacts: Vec<ArtifactInput<'_>> = Vec::new();
        for entry in &manifest.artifacts {
            let name = file_name_of(&entry.path);
            artifacts.push(ArtifactInput {
                path: &entry.path,
                os: entry.os.as_deref(),
                arch: entry.arch.as_deref(),
                libc: entry.libc.as_deref(),
                portable: entry.portable,
                variant: entry.variant.clone(),
                url: entry.url.clone().or_else(|| urls.get(name).cloned()),
                format: entry.format.clone().or_else(|| formats.get(name).cloned()),
                bin: entry.bins(&default_bins).to_vec(),
                requires: entry.requirements(manifest.requires.as_ref()),
                provenance: provenance_of(name, &entry.provenance),
                extensions: entry.extensions.clone(),
            });
        }
        for arg in &parsed {
            let name = file_name_of(&arg.path);
            if manifest
                .artifacts
                .iter()
                .any(|entry| file_name_of(&entry.path) == name)
            {
                continue;
            }
            artifacts.push(ArtifactInput {
                path: &arg.path,
                os: arg.os.as_deref(),
                arch: arg.arch.as_deref(),
                libc: arg.libc.as_deref(),
                portable: arg.portable,
                variant: arg.variant.clone(),
                url: urls.get(name).cloned(),
                format: formats.get(name).cloned(),
                bin: default_bins.clone(),
                requires: manifest.requires.clone(),
                provenance: provenance_of(name, &[]),
                extensions: Extensions::new(),
            });
        }
        if artifacts.is_empty() {
            bail!("no artifacts: give files on the command line or list them in the manifest");
        }
        for name in provenance.keys() {
            if !artifacts.iter().any(|a| file_name_of(a.path) == name) {
                bail!("--provenance names {name:?}, which is not among the artifacts");
            }
        }

        let default_bin = match default_bins.as_slice() {
            [only] => Some(only.name.as_str()),
            _ => None,
        };
        let mut resources = Vec::new();
        let mut asset_paths: Vec<PathBuf> = Vec::new();
        let mut add_asset = |path: Option<PathBuf>| {
            if let Some(path) = path
                && !asset_paths.contains(&path)
            {
                asset_paths.push(path);
            }
        };
        for entry in &manifest.resources {
            let (resource, asset_path) = entry.resolve()?;
            add_asset(asset_path);
            resources.push(resource);
        }
        for spec in &self.resource {
            let parsed = parse_resource(spec, default_bin)?;
            add_asset(parsed.asset_path);
            resources.push(parsed.resource);
        }
        let assets: Vec<AssetInput<'_>> = asset_paths
            .iter()
            .map(|path| AssetInput {
                path,
                url: urls.get(file_name_of(path)).cloned(),
            })
            .collect();
        for name in urls.keys() {
            if !artifacts.iter().any(|a| file_name_of(a.path) == name)
                && !asset_paths.iter().any(|p| file_name_of(p) == name)
            {
                bail!("--url names {name:?}, which is not among the artifacts or assets");
            }
        }
        for name in formats.keys() {
            if !artifacts.iter().any(|a| file_name_of(a.path) == name) {
                bail!("--format names {name:?}, which is not among the artifacts");
            }
        }
        let source = source_repo.map(|repo| Source {
            repo,
            commit,
            tag: tag.clone(),
        });
        // On a forge, consumers list versions from tags and only then read
        // the packslip; a tag that names no version, or another one, makes
        // this release invisible to them unless a release list names it.
        if let Some(tag) = &tag
            && packslip::model::repository(&project).is_some()
            && packslip::model::tag_version(tag, &project).as_deref() != Some(version.as_str())
        {
            eprintln!(
                "warning: tag {tag:?} does not name version {version} (a tag is the version, optionally after a v and the tool's or repository's name); consumers listing {project} from its tags will not see this release unless a signed release list names it"
            );
        }
        let url_base = self.url_base.as_deref().or(manifest.url_base.as_deref());
        let notes_url = self.notes_url.as_deref().or(manifest.notes_url.as_deref());
        let published_at = self
            .published_at
            .as_deref()
            .or(manifest.published_at.as_deref());
        let created = packslip::create::create(&Request {
            published_at,
            source,
            artifacts,
            resources,
            assets,
            url_base,
            notes_url,
            extensions,
            attested_by,
            evidence: self.evidence.iter().map(|s| parse_evidence(s)).collect(),
            sha512: !self.no_sha512,
            requires_bin,
            read_executables: !self.no_libs,
            ..Request::new(
                &project,
                &version,
                declared_identity(&signer, self.no_pin_workflow)?,
            )
        })?;
        let identity = created.statement.predicate.identity.clone();
        let bundle = sigstore::sign(signer, &created.document)?;
        std::fs::create_dir_all(&self.out)
            .wrap_err_with(|| format!("creating {}", self.out.display()))?;
        let path = self.out.join(bundle_name(&project));
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
        for artifact in &created.statement.predicate.artifacts {
            if let Some(requires) = artifact.requires.as_ref().filter(|r| !r.is_empty()) {
                println!("  requires {}: {}", artifact.name, requires.summary());
            }
        }
        Ok(())
    }
}

/// An artifact argument: a path, optionally with `:os/arch[/libc]` or
/// `:any`, and `@variant`.
#[cfg(all(feature = "create", feature = "manifest"))]
struct ArtifactArg {
    path: PathBuf,
    os: Option<String>,
    arch: Option<String>,
    libc: Option<String>,
    portable: bool,
    variant: Option<String>,
}

/// Recognize only a well-formed `os/arch[/libc]` or `any` suffix and a
/// trailing `@variant`. In particular, colons inside timestamped
/// directory names remain part of the path, and so does an `@` inside a
/// file name that is not followed by a plain word.
#[cfg(all(feature = "create", feature = "manifest"))]
fn parse_spec(spec: &str) -> ArtifactArg {
    let (rest, variant) = match spec.rsplit_once('@') {
        Some((rest, variant)) if valid_word(variant) && !rest.is_empty() => {
            (rest, Some(variant.to_string()))
        }
        _ => (spec, None),
    };
    let plain = |path: &str, portable: bool| ArtifactArg {
        path: PathBuf::from(path),
        os: None,
        arch: None,
        libc: None,
        portable,
        variant: variant.clone(),
    };
    match rest.rsplit_once(':') {
        Some((path, "any")) if !path.is_empty() => plain(path, true),
        Some((path, platform)) if valid_platform(platform) => {
            let mut parts = platform.split('/');
            ArtifactArg {
                path: PathBuf::from(path),
                os: parts.next().map(str::to_string),
                arch: parts.next().map(str::to_string),
                libc: parts.next().map(str::to_string),
                portable: false,
                variant,
            }
        }
        _ => plain(rest, false),
    }
}

/// A variant is a word like `fips` or `install_only`, never something
/// with a dot in it, so `scoped@pkg-1.0.tgz` stays a file name.
#[cfg(all(feature = "create", feature = "manifest"))]
fn valid_word(part: &str) -> bool {
    part.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// A `--resource` scope: `os`, `os/arch`, or `os/arch/libc`. Unlike an
/// artifact's, the os alone is enough, which is how a resource says it is
/// in every Linux build without naming their architectures.
#[cfg(all(feature = "create", feature = "manifest"))]
fn valid_scope(scope: &str) -> bool {
    let parts: Vec<_> = scope.split('/').collect();
    (1..=3).contains(&parts.len()) && parts.iter().all(|part| valid_word(part))
}

#[cfg(all(feature = "create", feature = "manifest"))]
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
/// Read local copies of released bundles and write a signed release list:
/// each bundle's URL, digest, version, tag, and publish time, plus the
/// list's sequence and expiry. The bundles are read, not verified, so give
/// copies you trust. Repeat --release for every entry to keep; the command
/// does not append to an earlier list or upload anything.
///
/// Publish the list at https://HOST/.well-known/packslip/PATH.json for a
/// project named HOST/PATH, or at https://HOST/.well-known/packslip.json for
/// a project named after a bare host. github.com does not serve that path,
/// so a github.com project commits the list to its default branch instead,
/// as its supplementary list: .well-known/packslip.json, or
/// .well-known/packslip/TOOL.json for a monorepo tool. See
/// https://packslip.dev/docs/release-lists/.
#[derive(Debug, usage_rs::Args)]
#[usage(example(
    "packslip releases \\\n        --project mytool.example.com \\\n        --sequence 1 --valid-for 30d \\\n        --latest 1.2.3 \\\n        --release https://mytool.example.com/v1.2.3/packslip.sigstore.json=releases/1.2.3/packslip.sigstore.json \\\n        --key release.key \\\n        --out site/.well-known/packslip.json",
    header = "List one key-signed release of a self-hosted project"
))]
#[cfg(all(feature = "create", feature = "manifest"))]
struct Releases {
    /// The project's name, which every --release bundle must name
    #[usage(long)]
    project: String,
    /// This list's sequence number, which increases with every list you
    /// publish; consumers refuse a list whose sequence is lower than one
    /// they have accepted
    #[usage(long)]
    sequence: u64,
    /// Recommend this version for an unconstrained latest request, without
    /// reordering versions; it must exactly match the version of a --release
    /// entry
    #[usage(long)]
    latest: Option<String>,
    /// How long until the list expires, as a number and a unit (s, m, h, d,
    /// or w), such as 30d
    #[usage(long, default = "30d")]
    valid_for: String,
    /// RFC 3339 generation time; defaults to now
    #[usage(long)]
    generated_at: Option<String>,
    /// A released packslip as URL=PATH: where consumers fetch it, and the
    /// local copy to read (repeatable)
    #[usage(long, required = true)]
    release: Vec<String>,
    /// Withdraw a listed release, as URL=REASON, with URL exactly as given
    /// to --release (repeatable)
    #[usage(long)]
    yank: Vec<String>,
    /// Mark a listed release as a security fix, by its --release URL
    /// (repeatable)
    #[usage(long)]
    security: Vec<String>,
    /// What a publisher other than the vendor checked about a listed
    /// release, as URL=KIND or URL=KIND=DETAIL (repeatable)
    #[usage(long)]
    evidence: Vec<String>,
    /// Sign with this secret key instead of a CI identity
    #[usage(short = 'k', long, value_hint = usage_rs::ValueHint::FilePath, help_heading = "Signing")]
    key: Option<PathBuf>,
    /// How to sign: oidc (keyless) or key (needs --key). Optional; inferred
    /// from whether --key is given
    #[usage(long, help_heading = "Signing")]
    sign: Option<SignWith>,
    /// With --key: do not record the signature in Rekor. Consumers must
    /// then opt in with --allow-unlogged
    #[usage(long, help_heading = "Signing")]
    no_log: bool,
    /// Keyless only: write identity.pin_workflow: false into the list, as
    /// `packslip create --no-pin-workflow` does for a release. See
    /// https://packslip.dev/release/v1/#workflow-pinning
    #[usage(long, help_heading = "Signing")]
    no_pin_workflow: bool,
    /// Where to write the list; consumers fetch it only from its .well-known
    /// path, never under this default name
    #[usage(short = 'o', long, default = "packslip-releases.sigstore.json")]
    out: PathBuf,
}

#[cfg(all(feature = "create", feature = "manifest"))]
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
        let mut evidence: std::collections::BTreeMap<String, Vec<Evidence>> =
            std::collections::BTreeMap::new();
        for spec in &self.evidence {
            let Some((url, rest)) = spec.split_once('=') else {
                bail!("--evidence wants URL=KIND or URL=KIND=DETAIL, got {spec:?}");
            };
            evidence
                .entry(url.to_string())
                .or_default()
                .push(parse_evidence(rest));
        }
        for url in yanked
            .keys()
            .chain(self.security.iter())
            .chain(evidence.keys())
        {
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
                evidence: evidence.get(url).cloned().unwrap_or_default(),
            })
            .collect();
        let created = packslip::create::create_release_list(&ListRequest {
            project: &self.project,
            generated_at: self.generated_at.as_deref(),
            valid_for: parse_duration(&self.valid_for)?,
            sequence: self.sequence,
            latest: self.latest.as_deref(),
            releases,
            identity: declared_identity(&signer, self.no_pin_workflow)?,
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
#[cfg(all(feature = "create", feature = "manifest"))]
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
/// Check the signature, log evidence, and statement structure. With
/// --artifact, also check local files against signed digests and artifact
/// sizes. Without it, only the bundle is checked. Nothing is downloaded:
/// remote artifacts and linked provenance are not fetched.
///
/// Choose what to trust. By default, the identity policy comes from the
/// project the document claims, if it is on github.com or gitlab.com: that
/// forge's issuer and any workflow of that repository. Any other project
/// needs --pubkey or an identity flag. For a keyless release, --issuer with
/// --identity-prefix (any workflow of a repository, such as
/// https://github.com/owner/repo/) or --identity (one exact certificate
/// identity, including its ref) replaces the derived policy, and only the
/// flags you pass are checked: --issuer alone accepts any signer from that
/// issuer. For a key-signed release, pass --pubkey. --pin adds a check on
/// top of the identity policy: the signing repository must have that signer
/// fingerprint.
///
/// Then match the verified project and version to what you meant to
/// install. For a release list, the command does not check the expiry or
/// compare the sequence with an earlier list. See
/// https://packslip.dev/docs/verifying/.
#[derive(Debug, usage_rs::Args)]
#[usage(
    exit_code(0, "The bundle verified, and every --artifact matched it"),
    exit_code(
        1,
        "Verification failed, or an input was unusable: a file could not be read, a key or fingerprint is malformed, or the trust flags conflict or are missing"
    ),
    exit_code(2, "The command line is invalid"),
    example(
        "packslip verify packslip.sigstore.json \\\n        --identity-prefix https://github.com/owner/repo/ \\\n        --issuer https://token.actions.githubusercontent.com \\\n        --artifact mytool-1.2.3-linux-x64.tar.gz",
        header = "Verify a GitHub release against its repository"
    ),
    example(
        "packslip verify packslip.sigstore.json \\\n        --pin ps1_snirenkjwr7m5ozgcufameodnm \\\n        --artifact hk-x86_64-unknown-linux-gnu.tar.gz",
        header = "Verify hk 2.3.0 against its signer fingerprint"
    ),
    example(
        "packslip verify packslip.sigstore.json \\\n        --pubkey release.pub \\\n        --artifact mytool-1.2.3-linux-x64.tar.gz",
        header = "Verify a key-signed release"
    )
)]
struct Verify {
    /// Local release bundle or signed release list to verify
    #[usage(value_hint = usage_rs::ValueHint::FilePath)]
    bundle: PathBuf,
    /// The pinned public key file, or its base64 line
    #[usage(short = 'p', long, help_heading = "Trust")]
    pubkey: Option<String>,
    /// The exact certificate identity a keyless signer must have, including
    /// its ref, such as
    /// https://github.com/owner/repo/.github/workflows/release.yml@refs/tags/v1.2.3
    #[usage(long, help_heading = "Trust")]
    identity: Option<String>,
    /// A prefix the certificate identity must start with, such as
    /// https://github.com/owner/repo/
    #[usage(long, help_heading = "Trust")]
    identity_prefix: Option<String>,
    /// The OIDC issuer a keyless signer must have; pass it with --identity
    /// or --identity-prefix, since alone it accepts any signer from that
    /// issuer
    #[usage(long, help_heading = "Trust")]
    issuer: Option<String>,
    /// The signer fingerprint (ps1_...) the certificate's repository must
    /// have, as `packslip pin` prints it
    #[usage(long, help_heading = "Trust")]
    pin: Option<String>,
    /// Accept a bundle without a transparency log entry
    #[usage(long, help_heading = "Trust")]
    allow_unlogged: bool,
    /// A sigstore trusted_root.json to use instead of the embedded one
    #[usage(long, value_hint = usage_rs::ValueHint::FilePath, help_heading = "Trust")]
    trusted_root: Option<PathBuf>,
    /// A downloaded artifact or resource asset to check against the bundle,
    /// matched by file name, so keep the name it was published under
    /// (repeatable)
    #[usage(short = 'a', long)]
    artifact: Vec<PathBuf>,
    /// Print the verified report as JSON, with source_repository when the
    /// signing certificate records one; for a release list, print the
    /// verified list statement
    #[usage(short = 'J', long, help_heading = "Output")]
    json: bool,
}

/// An owned pin, so a `Trust` can borrow it.
enum TrustPin {
    Key(PublicKey),
    Identity(Policy),
}

impl TrustPin {
    fn as_trust(&self) -> Trust<'_> {
        match self {
            TrustPin::Key(key) => Trust::Key(key),
            TrustPin::Identity(policy) => Trust::Identity(policy),
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
        let project = peeked["predicate"]["project"].as_str().unwrap_or_default();
        let is_list = peeked["predicateType"] == RELEASES_PREDICATE_TYPE;

        let want_fingerprint: Option<Fingerprint> = match &self.pin {
            Some(pin) => Some(pin.parse().wrap_err("--pin")?),
            None => None,
        };
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
                if want_fingerprint.is_some() {
                    bail!(
                        "--pin is the fingerprint of a certificate's repository; a key has none. Use --pubkey alone"
                    );
                }
                let pubkey_text = if Path::new(text).is_file() {
                    std::fs::read_to_string(text)?
                } else {
                    text.clone()
                };
                TrustPin::Key(PublicKey::parse(&pubkey_text)?)
            }
            None => TrustPin::Identity(identity_policy(
                &self.identity,
                &self.identity_prefix,
                &self.issuer,
                project,
                sigstore::Error::NoPolicy,
            )?),
        };
        let trusted_root = load_trusted_root(self.trusted_root.as_deref())?;
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
                    if let Some(want) = &want_fingerprint {
                        let source = sigstore::source_repository(&text)?;
                        if let Err(err) = match_fingerprint(
                            want,
                            &list.project,
                            &verified.key_id,
                            verified.issuer.as_deref(),
                            source.as_ref(),
                        ) {
                            eprintln!("verification failed: {err}");
                            std::process::exit(1)
                        }
                    }
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
                        if let Some(source) = sigstore::source_repository(&text)? {
                            println!("  repository {}", describe_source(&source));
                        }
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
                // The bundle verified, so its certificate's record of the
                // repository is the one Fulcio issued.
                let source = sigstore::source_repository(&text)?;
                if let Some(want) = &want_fingerprint
                    && let Err(err) = match_fingerprint(
                        want,
                        &verified.project,
                        &verified.key_id,
                        verified.issuer.as_deref(),
                        source.as_ref(),
                    )
                {
                    eprintln!("verification failed: {err}");
                    std::process::exit(1)
                }
                if self.json {
                    #[derive(serde::Serialize)]
                    struct Report<'a> {
                        #[serde(flatten)]
                        verified: &'a packslip::Verified,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        source_repository: Option<&'a sigstore::SourceRepository>,
                    }
                    let report = Report {
                        verified: &verified,
                        source_repository: source.as_ref(),
                    };
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "ok: {} {}{} published {} signed by {} ({}){}{} ({}{} checked{}{})",
                        verified.project,
                        verified.version,
                        match (&verified.channel, verified.prerelease) {
                            (Some(channel), _) => format!(" ({channel} prerelease)"),
                            (None, true) => " (prerelease)".to_string(),
                            (None, false) => String::new(),
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
                        format_args!(
                            "{} of {} artifact(s)",
                            verified.checked_artifacts.len(),
                            verified.artifact_count
                        ),
                        // Only a release that lists assets counts them, so
                        // the line for one that lists none is unchanged.
                        if verified.asset_count == 0 {
                            String::new()
                        } else {
                            format!(
                                " and {} of {} asset(s)",
                                verified.checked_assets.len(),
                                verified.asset_count
                            )
                        },
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
                    if let Some(source) = &source {
                        println!("  repository {}", describe_source(source));
                    }
                    for line in &verified.requires {
                        println!("  requires {line}");
                    }
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

/// The certificate policy for `project`: the identity flags, or else the
/// one its GitHub or GitLab name implies. `missing` builds the error for a
/// project that is on neither, so each command can name the flags it has.
fn identity_policy(
    identity: &Option<String>,
    identity_prefix: &Option<String>,
    issuer: &Option<String>,
    project: &str,
    missing: fn(String) -> sigstore::Error,
) -> Result<Policy> {
    let explicit = Policy {
        issuer: issuer.clone(),
        identity: identity.clone(),
        identity_prefix: identity_prefix.clone(),
    };
    if explicit.is_empty() {
        Ok(Policy::for_project(project).ok_or_else(|| missing(project.into()))?)
    } else {
        Ok(explicit)
    }
}

fn load_trusted_root(path: Option<&Path>) -> Result<sigstore_trust_root::TrustedRoot> {
    let json = match path {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .wrap_err_with(|| format!("reading {}", path.display()))?,
        ),
        None => None,
    };
    Ok(sigstore::trusted_root(json.as_deref())?)
}

/// Whether what a verified certificate says about its repository is about
/// the project the statement names. A signer can be a reusable workflow of
/// another repository, so the certificate's own source repository must be
/// the project's, whichever workflow signed. A project that is not on a forge
/// has nothing to compare.
fn bound_to_project(
    project: &str,
    signer: &str,
    issuer: Option<&str>,
    source: Option<&sigstore::SourceRepository>,
) -> std::result::Result<(), String> {
    match packslip::forge::check_source(
        &packslip::forge::Expected::new(project),
        project,
        signer,
        issuer,
        source,
    ) {
        Ok(_) | Err(packslip::forge::IdentityError::NotForge(_)) => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

/// The signer fingerprint of a verified keyless release.
fn signer_fingerprint(
    project: &str,
    signer: &str,
    issuer: Option<&str>,
    source: Option<&sigstore::SourceRepository>,
) -> std::result::Result<Fingerprint, String> {
    if issuer.is_none() {
        return Err(format!(
            "{project} is signed with a key, which has no signer fingerprint; pin it with --pubkey"
        ));
    }
    bound_to_project(project, signer, issuer, source)?;
    Fingerprint::of_signer(issuer, source).ok_or_else(|| {
        format!(
            "{project}'s certificate records no repository ID, so it has no signer fingerprint; pin it with --identity-prefix and --issuer instead"
        )
    })
}

/// Check a verified release's signer against a fingerprint.
fn match_fingerprint(
    want: &Fingerprint,
    project: &str,
    signer: &str,
    issuer: Option<&str>,
    source: Option<&sigstore::SourceRepository>,
) -> std::result::Result<(), String> {
    bound_to_project(project, signer, issuer, source)?;
    want.verify(issuer, source).map_err(|e| e.to_string())
}

/// Print the signer fingerprint of a project's keyless releases
///
/// Verify a release bundle and print the signer fingerprint of the
/// repository that signed it: `ps1_` and 26 characters. Run it on a release
/// you already trust, because it fingerprints whichever repository signed
/// the bundle it is given. A vendor publishes the fingerprint where
/// consumers can read it without trusting a release, such as its README,
/// and a consumer records it in its own configuration, such as a Dockerfile
/// or CI workflow; `packslip verify --pin` then checks a release against it.
///
/// The fingerprint is derived from the forge's issuer and repository ID
/// only. It stays the same when the repository is renamed, moves to another
/// owner, or signs from another workflow, and a repository that later takes
/// over the old name gets a different one. Only keyless releases whose
/// certificate records a repository ID have one; for a key-signed release,
/// pin the key with `packslip verify --pubkey`.
///
/// Verification works as in `packslip verify`: the policy is the one the
/// project's name implies unless --identity, --identity-prefix, or --issuer
/// replace it. See https://packslip.dev/docs/verifying/.
#[derive(Debug, usage_rs::Args)]
#[usage(
    exit_code(0, "Printed the signer fingerprint"),
    exit_code(
        1,
        "Verification failed, the release has no signer fingerprint, or an input was unusable"
    ),
    exit_code(2, "The command line is invalid"),
    example(
        "packslip pin packslip.sigstore.json",
        header = "Print the fingerprint of a release you already trust"
    )
)]
struct Pin {
    /// Local release bundle to verify and fingerprint
    #[usage(value_hint = usage_rs::ValueHint::FilePath)]
    bundle: PathBuf,
    /// The exact certificate identity a keyless signer must have, including
    /// its ref, such as
    /// https://github.com/owner/repo/.github/workflows/release.yml@refs/tags/v1.2.3
    #[usage(long, help_heading = "Trust")]
    identity: Option<String>,
    /// A prefix the certificate identity must start with, such as
    /// https://github.com/owner/repo/
    #[usage(long, help_heading = "Trust")]
    identity_prefix: Option<String>,
    /// The OIDC issuer a keyless signer must have; pass it with --identity
    /// or --identity-prefix, since alone it accepts any signer from that
    /// issuer
    #[usage(long, help_heading = "Trust")]
    issuer: Option<String>,
    /// Accept a bundle without a transparency log entry
    #[usage(long, help_heading = "Trust")]
    allow_unlogged: bool,
    /// A sigstore trusted_root.json to use instead of the embedded one
    #[usage(long, value_hint = usage_rs::ValueHint::FilePath, help_heading = "Trust")]
    trusted_root: Option<PathBuf>,
}

impl RunWith<BinInfo> for Pin {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let text = std::fs::read_to_string(&self.bundle)
            .wrap_err_with(|| format!("reading {}", self.bundle.display()))?;
        let peeked: serde_json::Value = serde_json::from_slice(&sigstore::peek_statement(&text)?)
            .wrap_err("the bundle's payload is not JSON")?;
        if peeked["predicateType"] == RELEASES_PREDICATE_TYPE {
            bail!("a release list is not a release; give a release's packslip.sigstore.json");
        }
        let project = peeked["predicate"]["project"].as_str().unwrap_or_default();
        let policy = identity_policy(
            &self.identity,
            &self.identity_prefix,
            &self.issuer,
            project,
            sigstore::Error::NoIdentityPolicy,
        )?;
        let trusted_root = load_trusted_root(self.trusted_root.as_deref())?;
        let options = Options {
            require_log: !self.allow_unlogged,
            trusted_root: &trusted_root,
        };
        let verified = match packslip::verify(&text, &Trust::Identity(&policy), options, &[]) {
            Ok(verified) => verified,
            Err(err) => {
                eprintln!("verification failed: {err}");
                std::process::exit(1)
            }
        };
        // The bundle verified, so its certificate's record of the
        // repository is the one Fulcio issued.
        let source = sigstore::source_repository(&text)?;
        match signer_fingerprint(
            &verified.project,
            &verified.key_id,
            verified.issuer.as_deref(),
            source.as_ref(),
        ) {
            Ok(fingerprint) => {
                println!("{fingerprint}");
                Ok(())
            }
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1)
            }
        }
    }
}

/// The signing repository and the forge's IDs for it and its owner:
/// `https://github.com/jdx/hk (id 922514152), owner https://github.com/jdx (id 216188)`.
fn describe_source(source: &sigstore::SourceRepository) -> String {
    let with_id = |uri: &str, id: &Option<String>| match id {
        Some(id) => format!("{uri} (id {id})"),
        None => uri.to_string(),
    };
    let mut line = with_id(&source.uri, &source.id);
    if let Some(owner) = &source.owner_uri {
        line.push_str(&format!(", owner {}", with_id(owner, &source.owner_id)));
    }
    line
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(answer) = Cli::completion_request(&args[1..]) {
        print!("{answer}");
        return Ok(());
    }
    let argv = packslip::cli::argv(&args);
    let cli = packslip::cli::unwrap_or_exit(Cli::spec(), &argv, Cli::parse_from_argv(&argv));
    match cli.command {
        Some(command) => command.run_with(BIN),
        None => Ok(()),
    }
}

#[cfg(all(test, feature = "create", feature = "manifest"))]
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
        let spec = parse_spec("tool.jar:any");
        assert_eq!(spec.path, PathBuf::from("tool.jar"));
        assert!(spec.portable);
        let spec = parse_spec("tool-linux.jar:any@slim");
        assert!(spec.portable);
        assert_eq!(spec.variant.as_deref(), Some("slim"));
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
            "completion/bash,zsh=exec:COMPLETE={shell} _TOOL_X=1 tool",
            None,
        )
        .unwrap();
        assert_eq!(r.resource.exec, ["tool"]);
        assert_eq!(r.resource.env["COMPLETE"], "{shell}");
        assert_eq!(r.resource.env["_TOOL_X"], "1");
        assert!(parse_resource("man=exec:ONLY=env", None).is_err());
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
        let r = parse_resource("completion/zsh/other=archive:share/_other", None).unwrap();
        assert_eq!(r.resource.shell.as_deref(), Some("zsh"));
        assert_eq!(r.resource.bin.as_deref(), Some("other"));
        let r = parse_resource(
            "completion/bash,zsh/other=exec:other completion {shell}",
            None,
        )
        .unwrap();
        assert_eq!(r.resource.shells, ["bash", "zsh"]);
        assert_eq!(r.resource.bin.as_deref(), Some("other"));
        let r = parse_resource("man/other=archive:share/man/other.1", None).unwrap();
        assert_eq!(r.resource.bin.as_deref(), Some("other"));
        assert!(r.resource.name.is_none());
        assert!(parse_resource("cli-spec/usage=repo:x", None).is_err());
        let r = parse_resource("skill/tool=asset:dist/tool-skill.tar.gz", None).unwrap();
        assert_eq!(r.resource.name.as_deref(), Some("tool"));
        assert_eq!(r.resource.asset.as_deref(), Some("tool-skill.tar.gz"));
        assert_eq!(r.asset_path, Some(PathBuf::from("dist/tool-skill.tar.gz")));
        let r = parse_resource("man=archive:man/man1/tool.1", None).unwrap();
        assert!(r.resource.name.is_none());
        let r = parse_resource("font/Tool=archive:fonts/Tool.ttf", None).unwrap();
        assert_eq!(r.resource.name.as_deref(), Some("Tool"));
        let r = parse_resource("sbom/cyclonedx=asset:dist/tool.cdx.json", None).unwrap();
        assert_eq!(r.resource.format.as_deref(), Some("cyclonedx"));
        assert_eq!(r.resource.name, None);
        assert_eq!(r.resource.asset.as_deref(), Some("tool.cdx.json"));
        assert!(parse_resource("sbom=asset:dist/tool.cdx.json", None).is_err());

        // An @ scope limits the entry to one platform, from the os alone
        // down to os/arch/libc, and rides alongside any qualifier.
        let r = parse_resource("man@linux=archive:share/man/man1/tool.1", None).unwrap();
        assert_eq!(r.resource.os.as_deref(), Some("linux"));
        assert_eq!(r.resource.arch, None);
        assert_eq!(r.resource.libc, None);
        assert_eq!(r.resource.archive.as_deref(), Some("share/man/man1/tool.1"));
        let r = parse_resource("completion/zsh@linux/x86_64/musl=archive:_tool", None).unwrap();
        assert_eq!(r.resource.shell.as_deref(), Some("zsh"));
        assert_eq!(
            (
                r.resource.os.as_deref(),
                r.resource.arch.as_deref(),
                r.resource.libc.as_deref()
            ),
            (Some("linux"), Some("x86_64"), Some("musl"))
        );
        // Only the head is read for a scope, so an @ in an exec argv or a
        // path is left where it is.
        let r = parse_resource("cli-spec/usage/tool=exec:tool usage @all", None).unwrap();
        assert_eq!(r.resource.os, None);
        assert_eq!(r.resource.exec, ["tool", "usage", "@all"]);
        let r = parse_resource("skill/tool=repo:skills/@tool", None).unwrap();
        assert_eq!(r.resource.repo.as_deref(), Some("skills/@tool"));
        assert_eq!(r.resource.os, None);

        for bad in [
            "man",
            "=archive:x",
            "man=x",
            "man=ftp:x",
            "completion=archive:x",
            "skill=archive:x",
            "man/a/b=archive:x",
            "man@=archive:x",
            "man@linux/x86_64/gnu/extra=archive:x",
            "man@linux//gnu=archive:x",
            "man@1inux=archive:x",
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
