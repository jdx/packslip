//! The `packslip` binary: keygen, create, verify, schema.

use std::path::{Path, PathBuf};

use eyre::{Context as _, Result, bail};
use packslip::cli::{BinInfo, Version};
use packslip::create::{ArtifactInput, Request};
use packslip::minisign::{PublicKey, SecretKey, key_id_hex};
use packslip::model::{Identity, Scheme, Source, Statement};
use packslip::sigstore;
use packslip::verify::{Signature, Trust};
use usage_rs::RunWith;

const BIN: BinInfo = BinInfo {
    name: "packslip",
    version: env!("CARGO_PKG_VERSION"),
};

/// A signed release manifest: what shipped, and how to verify it
///
/// A vendor runs `packslip create` in its release job to publish one signed
/// document listing every artifact. Consumers verify it with `packslip
/// verify` against a pinned identity. See https://packslip.dev.
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
    Schema(Schema),
    Verify(Verify),
    Version(Version),
}

/// Generate a minisign key pair
///
/// Only needed for the minisign scheme. A CI job with an OIDC identity signs
/// keylessly and needs no key at all. Writes the secret seed to the given
/// path (mode 0600) and a minisign-format public key beside it with a .pub
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

/// Print the JSON schema of the document
#[derive(Debug, usage_rs::Args)]
struct Schema {}

impl RunWith<BinInfo> for Schema {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        println!("{}", serde_json::to_string_pretty(&Statement::schema())?);
        Ok(())
    }
}

/// How `create` signs: `oidc` (keyless, with the CI job's identity through
/// sigstore) or `minisign` (a long-lived key given with --key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignWith {
    Oidc,
    Minisign,
}

impl std::str::FromStr for SignWith {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "oidc" | "sigstore" | "sigstore-oidc" => Ok(SignWith::Oidc),
            "minisign" => Ok(SignWith::Minisign),
            other => Err(format!("--sign must be oidc or minisign, got {other:?}")),
        }
    }
}

/// Create and sign a packslip for a release
///
/// Digests every artifact, infers os/arch/libc/format from file names
/// (override with name:os/arch[/libc]), and writes packslip.json plus its
/// signature into --out. Inside a CI job the document is signed keylessly
/// with the job's identity and the signature is packslip.sigstore.json.
/// With --key it is signed with a minisign key and the signature is
/// packslip.json.minisig.
#[derive(Debug, usage_rs::Args)]
struct Create {
    /// The project's name: a host path such as github.com/owner/repo
    #[usage(long)]
    project: String,
    /// The release version
    #[usage(long)]
    version: String,
    /// Artifact files, optionally as path:os/arch[/libc]
    #[usage(required = true)]
    artifacts: Vec<String>,
    /// Sign with a minisign secret key from `packslip keygen`
    #[usage(short = 'k', long, value_hint = usage_rs::ValueHint::FilePath)]
    key: Option<PathBuf>,
    /// How to sign; defaults to minisign when --key is given, else oidc
    #[usage(long)]
    sign: Option<SignWith>,
    /// Directory to write into
    #[usage(short = 'o', long, default = ".")]
    out: PathBuf,
    /// Download URL prefix for the artifacts
    #[usage(long)]
    url_base: Option<String>,
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
    /// SBOM URL
    #[usage(long)]
    sbom: Option<String>,
    /// The version this release replaces
    #[usage(long)]
    supersedes: Option<String>,
    /// Executable inside every archive, relative to its root (repeatable)
    #[usage(long)]
    bin: Vec<String>,
    /// Provenance URL for every artifact (repeatable, positional order)
    #[usage(long)]
    provenance: Vec<String>,
}

impl RunWith<BinInfo> for Create {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        if self.source_repo.is_none() && (self.commit.is_some() || self.tag.is_some()) {
            bail!("--commit and --tag require --source-repo");
        }
        let sign_with = self.sign.unwrap_or(if self.key.is_some() {
            SignWith::Minisign
        } else {
            SignWith::Oidc
        });
        enum Signer {
            Minisign(SecretKey),
            Oidc(sigstore::OidcIdentity),
        }
        let (signer, identity) = match sign_with {
            SignWith::Minisign => {
                let Some(path) = &self.key else {
                    bail!("--sign minisign needs --key");
                };
                let key_text = std::fs::read_to_string(path)
                    .wrap_err_with(|| format!("reading {}", path.display()))?;
                let key = SecretKey::parse(&key_text)?;
                let identity = Identity {
                    scheme: Scheme::Minisign,
                    key_id: key_id_hex(&key.public_key().key_id),
                    issuer: None,
                };
                (Signer::Minisign(key), identity)
            }
            SignWith::Oidc => {
                if self.key.is_some() {
                    bail!("--key is for --sign minisign");
                }
                let oidc = sigstore::ambient_identity()?;
                let identity = Identity {
                    scheme: Scheme::SigstoreOidc,
                    key_id: oidc.identity.clone(),
                    issuer: Some(oidc.issuer.clone()),
                };
                (Signer::Oidc(oidc), identity)
            }
        };
        let parsed: Vec<ArtifactSpec> = self.artifacts.iter().map(|s| parse_spec(s)).collect();
        let artifacts: Vec<ArtifactInput<'_>> = parsed
            .iter()
            .enumerate()
            .map(|(i, spec)| ArtifactInput {
                path: &spec.path,
                os: spec.os.as_deref(),
                arch: spec.arch.as_deref(),
                libc: spec.libc.as_deref(),
                bin: self.bin.clone(),
                provenance: self.provenance.get(i).cloned().into_iter().collect(),
            })
            .collect();
        let source = self.source_repo.as_ref().map(|repo| Source {
            repo: repo.clone(),
            commit: self.commit.clone(),
            tag: self.tag.clone(),
        });
        let created = packslip::create::create(&Request {
            project: &self.project,
            version: &self.version,
            published_at: self.published_at.as_deref(),
            source,
            artifacts,
            url_base: self.url_base.as_deref(),
            sbom: self.sbom.as_deref(),
            supersedes: self.supersedes.as_deref(),
            identity,
        })?;
        let (signature_path, signature) = match signer {
            Signer::Minisign(key) => (
                self.out.join("packslip.json.minisig"),
                packslip::create::sign_minisign(&created, &key),
            ),
            Signer::Oidc(oidc) => (
                self.out.join("packslip.sigstore.json"),
                sigstore::sign(oidc, &created.document)?,
            ),
        };
        std::fs::create_dir_all(&self.out)
            .wrap_err_with(|| format!("creating {}", self.out.display()))?;
        let document = self.out.join("packslip.json");
        std::fs::write(&document, &created.document)?;
        std::fs::write(&signature_path, &signature)?;
        println!(
            "wrote {} and {} ({} artifact(s), signed by {}, level {})",
            document.display(),
            signature_path.display(),
            created.statement.predicate.artifacts.len(),
            created.statement.predicate.identity.key_id,
            created.statement.declared_level()
        );
        Ok(())
    }
}

/// An artifact argument: a path, optionally with `:os/arch[/libc]`.
struct ArtifactSpec {
    path: PathBuf,
    os: Option<String>,
    arch: Option<String>,
    libc: Option<String>,
}

/// Recognize only a well-formed `os/arch[/libc]` suffix. In particular,
/// colons inside timestamped directory names remain part of the path.
fn parse_spec(spec: &str) -> ArtifactSpec {
    match spec.rsplit_once(':') {
        Some((path, platform)) if valid_platform(platform) => {
            let mut parts = platform.split('/');
            ArtifactSpec {
                path: PathBuf::from(path),
                os: parts.next().map(str::to_string),
                arch: parts.next().map(str::to_string),
                libc: parts.next().map(str::to_string),
            }
        }
        _ => ArtifactSpec {
            path: PathBuf::from(spec),
            os: None,
            arch: None,
            libc: None,
        },
    }
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

/// Verify a packslip against a pinned identity
///
/// Checks the signature, the document, and the digest and size of every
/// artifact file given. Exits 1 on any failure. A minisign document is
/// checked against --pubkey. A sigstore document is checked against
/// --identity and --issuer, or, for a project on github.com or gitlab.com,
/// against the repository the project name says signed it.
#[derive(Debug, usage_rs::Args)]
struct Verify {
    /// The packslip.json to verify
    #[usage(value_hint = usage_rs::ValueHint::FilePath)]
    document: PathBuf,
    /// The minisign public key file, or its base64 line
    #[usage(short = 'p', long)]
    pubkey: Option<String>,
    /// The signature file; defaults to <document>.minisig or
    /// packslip.sigstore.json beside the document
    #[usage(short = 's', long)]
    signature: Option<PathBuf>,
    /// The exact certificate identity a sigstore signer must have
    #[usage(long)]
    identity: Option<String>,
    /// A prefix the certificate identity must start with, such as
    /// https://github.com/owner/repo/
    #[usage(long)]
    identity_prefix: Option<String>,
    /// The OIDC issuer a sigstore signer must have
    #[usage(long)]
    issuer: Option<String>,
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

impl RunWith<BinInfo> for Verify {
    type Output = Result<()>;

    fn run_with(self, _: BinInfo) -> Self::Output {
        let document = std::fs::read(&self.document)
            .wrap_err_with(|| format!("reading {}", self.document.display()))?;
        let statement: Statement = serde_json::from_slice(&document)
            .wrap_err_with(|| format!("parsing {}", self.document.display()))?;
        let scheme = statement.predicate.identity.scheme;
        let signature_path = self
            .signature
            .clone()
            .unwrap_or_else(|| default_signature_path(&self.document, scheme));
        let signature_text = std::fs::read_to_string(&signature_path)
            .wrap_err_with(|| format!("reading {}", signature_path.display()))?;
        let artifacts: Vec<&Path> = self.artifact.iter().map(PathBuf::as_path).collect();

        let pubkey;
        let policy;
        let trusted_root;
        let (signature, trust) = match scheme {
            Scheme::Minisign => {
                let Some(text) = &self.pubkey else {
                    bail!("the document is minisign-signed; pass --pubkey");
                };
                let pubkey_text = if Path::new(text).is_file() {
                    std::fs::read_to_string(text)?
                } else {
                    text.clone()
                };
                pubkey = PublicKey::parse(&pubkey_text)?;
                (
                    Signature::Minisign(&signature_text),
                    Trust::Minisign(&pubkey),
                )
            }
            Scheme::SigstoreOidc | Scheme::SigstoreKey => {
                if self.pubkey.is_some() {
                    bail!("the document is sigstore-signed; --pubkey does not apply");
                }
                let explicit = sigstore::Policy {
                    issuer: self.issuer.clone(),
                    identity: self.identity.clone(),
                    identity_prefix: self.identity_prefix.clone(),
                };
                policy = if explicit.is_empty() {
                    sigstore::Policy::for_project(&statement.predicate.project).ok_or(
                        sigstore::Error::NoPolicy(statement.predicate.project.clone()),
                    )?
                } else {
                    explicit
                };
                let root_json = match &self.trusted_root {
                    Some(path) => Some(
                        std::fs::read_to_string(path)
                            .wrap_err_with(|| format!("reading {}", path.display()))?,
                    ),
                    None => None,
                };
                trusted_root = sigstore::trusted_root(root_json.as_deref())?;
                (
                    Signature::Sigstore(&signature_text),
                    Trust::Sigstore {
                        policy: &policy,
                        trusted_root: &trusted_root,
                    },
                )
            }
        };
        match packslip::verify(&document, signature, &trust, &artifacts) {
            Ok(verified) => {
                if self.json {
                    println!("{}", serde_json::to_string_pretty(&verified)?);
                } else {
                    println!(
                        "ok: {} {} published {} signed by {} ({}) level {} ({} of {} artifact(s) checked)",
                        verified.project,
                        verified.version,
                        verified.published_at,
                        verified.key_id,
                        verified.scheme,
                        verified.level,
                        verified.checked_artifacts.len(),
                        verified.artifact_count
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

/// `packslip.json.minisig`, or `packslip.sigstore.json` beside the document.
fn default_signature_path(document: &Path, scheme: Scheme) -> PathBuf {
    match scheme {
        Scheme::Minisign => {
            let mut name = document.as_os_str().to_owned();
            name.push(".minisig");
            PathBuf::from(name)
        }
        Scheme::SigstoreOidc | Scheme::SigstoreKey => {
            let stem = document
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.strip_suffix(".json").unwrap_or(n))
                .unwrap_or("packslip");
            document.with_file_name(format!("{stem}.sigstore.json"))
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
    fn artifact_platform_suffix_does_not_consume_timestamped_paths() {
        let spec = parse_spec("build/2026-09-01T12:00:00Z/tool.tar.gz");
        assert_eq!(
            spec.path,
            PathBuf::from("build/2026-09-01T12:00:00Z/tool.tar.gz")
        );
        assert!(spec.os.is_none());
        let spec = parse_spec("weird.bin:freebsd/riscv64");
        assert_eq!(spec.os.as_deref(), Some("freebsd"));
        assert_eq!(spec.arch.as_deref(), Some("riscv64"));
        let spec = parse_spec("tool.tar.gz:linux/x86_64/musl");
        assert_eq!(spec.libc.as_deref(), Some("musl"));
    }

    #[test]
    fn signature_defaults() {
        assert_eq!(
            default_signature_path(Path::new("dist/packslip.json"), Scheme::Minisign),
            PathBuf::from("dist/packslip.json.minisig")
        );
        assert_eq!(
            default_signature_path(Path::new("dist/packslip.json"), Scheme::SigstoreOidc),
            PathBuf::from("dist/packslip.sigstore.json")
        );
    }
}
