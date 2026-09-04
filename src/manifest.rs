//! A release manifest for `packslip create --manifest`: what the command
//! line cannot say per artifact. Executables that live at different paths
//! in different archives, a format the file name gets wrong, host
//! requirements, an artifact that runs anywhere, and the resources the
//! release ships, all in one TOML file a vendor keeps in its repository.
//!
//! ```toml
//! # release.toml
//! bin = ["tool"]                       # inside every artifact, unless an entry says otherwise
//! requires = { glibc_min = "2.31" }    # likewise
//!
//! [source]
//! repo = "https://github.com/owner/tool"
//!
//! [[artifact]]
//! path = "dist/tool-1.2.3-linux-x64.tar.gz"
//! bin = ["tool-1.2.3-linux-x64/tool"]
//!
//! [[artifact]]
//! path = "dist/tool-1.2.3-windows-x64.exe"
//! format = "raw"
//!
//! [[artifact]]
//! path = "dist/tool.jar"
//! portable = true
//! bin = []
//!
//! [[resource]]
//! kind = "completion"
//! shell = "zsh"
//! archive = "share/zsh/site-functions/_tool"
//!
//! [[resource]]
//! kind = "sbom"
//! format = "cyclonedx"
//! asset = "dist/tool.cdx.json"
//! ```
//!
//! Anything the command line also sets wins over the manifest, and
//! artifacts given on the command line join those the manifest lists,
//! with the manifest's entry taken for a file both name.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{Bin, Extensions, Requires, Resource, Source};

/// The manifest file, as written.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub project: Option<String>,
    pub version: Option<String>,
    /// Download URL prefix for artifacts and assets without a `url`.
    pub url_base: Option<String>,
    pub notes_url: Option<String>,
    /// RFC 3339 publish time; defaults to now.
    pub published_at: Option<String>,
    pub source: Option<SourceSpec>,
    /// Executables inside every artifact that does not list its own.
    #[serde(default)]
    pub bin: Vec<Bin>,
    /// Host requirements for every artifact that does not state its own.
    pub requires: Option<Requires>,
    /// Release-level extensions, keyed by who defines them.
    #[serde(default)]
    pub extensions: Extensions,
    #[serde(default, rename = "artifact")]
    pub artifacts: Vec<ArtifactSpec>,
    #[serde(default, rename = "resource")]
    pub resources: Vec<ResourceSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    pub repo: String,
    pub commit: Option<String>,
    pub tag: Option<String>,
}

impl From<SourceSpec> for Source {
    fn from(spec: SourceSpec) -> Source {
        Source {
            repo: spec.repo,
            commit: spec.commit,
            tag: spec.tag,
        }
    }
}

/// One artifact, by local path, with what the file name cannot say.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSpec {
    pub path: PathBuf,
    pub os: Option<String>,
    pub arch: Option<String>,
    pub libc: Option<String>,
    /// Runs on any host: no `os`, `arch`, or `libc`, whatever the name
    /// says.
    #[serde(default)]
    pub portable: bool,
    pub variant: Option<String>,
    /// `raw` for an `.exe` that is the program rather than an installer,
    /// or whatever the file name gets wrong.
    pub format: Option<String>,
    pub url: Option<String>,
    /// Executables inside this artifact; absent means the manifest's
    /// `bin`, and an empty list means none.
    pub bin: Option<Vec<Bin>>,
    /// Host requirements; absent means the manifest's `requires`.
    pub requires: Option<Requires>,
    #[serde(default)]
    pub provenance: Vec<String>,
    /// Artifact-level extensions, keyed by who defines them.
    #[serde(default)]
    pub extensions: Extensions,
}

impl ArtifactSpec {
    /// The executables this artifact holds, falling back to the
    /// manifest's.
    pub fn bins<'a>(&'a self, defaults: &'a [Bin]) -> &'a [Bin] {
        self.bin.as_deref().unwrap_or(defaults)
    }

    /// The host requirements, falling back to the manifest's.
    pub fn requirements(&self, defaults: Option<&Requires>) -> Option<Requires> {
        self.requires.clone().or_else(|| defaults.cloned())
    }
}

/// A resource entry. The same fields as the statement's, except that
/// `asset` is the local path of the file, which is digested into the
/// subject under its file name.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSpec {
    pub kind: String,
    /// Limit the entry to artifacts of this platform.
    pub os: Option<String>,
    pub arch: Option<String>,
    pub libc: Option<String>,
    pub shell: Option<String>,
    #[serde(default)]
    pub shells: Vec<String>,
    pub name: Option<String>,
    pub bin: Option<String>,
    pub format: Option<String>,
    pub archive: Option<String>,
    pub asset: Option<PathBuf>,
    pub url: Option<String>,
    pub repo: Option<String>,
    #[serde(default)]
    pub exec: Vec<String>,
    /// Resource-level extensions, keyed by who defines them.
    #[serde(default)]
    pub extensions: Extensions,
}

impl ResourceSpec {
    /// The statement entry, and the local file behind an `asset` source.
    pub fn resolve(&self) -> Result<(Resource, Option<PathBuf>), Error> {
        let asset_name = match &self.asset {
            Some(path) => Some(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                    .ok_or_else(|| Error::AssetPath(path.display().to_string()))?,
            ),
            None => None,
        };
        let resource = Resource {
            kind: self.kind.clone(),
            os: self.os.clone(),
            arch: self.arch.clone(),
            libc: self.libc.clone(),
            shell: self.shell.clone(),
            shells: self.shells.clone(),
            name: self.name.clone(),
            bin: self.bin.clone(),
            format: self.format.clone(),
            archive: self.archive.clone(),
            asset: asset_name,
            url: self.url.clone(),
            repo: self.repo.clone(),
            exec: self.exec.clone(),
            extensions: self.extensions.clone(),
        };
        Ok((resource, self.asset.clone()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("resource asset {0:?} is not a file path")]
    AssetPath(String),
    #[error("artifact {0:?} is listed twice")]
    DuplicateArtifact(String),
}

impl Manifest {
    /// Read and check a manifest file. Relative artifact and asset paths
    /// are left as written, to be resolved from the working directory.
    pub fn read(path: &Path) -> Result<Manifest, Error> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        let manifest = Manifest::parse(&text).map_err(|source| Error::Parse {
            path: path.display().to_string(),
            source: Box::new(source),
        })?;
        let mut seen = std::collections::BTreeSet::new();
        for artifact in &manifest.artifacts {
            if !seen.insert(&artifact.path) {
                return Err(Error::DuplicateArtifact(
                    artifact.path.display().to_string(),
                ));
            }
        }
        Ok(manifest)
    }

    /// Parse manifest text.
    pub fn parse(text: &str) -> Result<Manifest, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_manifest_parses() {
        let m = Manifest::parse(
            r#"
            project = "github.com/owner/tool"
            version = "1.2.3"
            url_base = "https://github.com/owner/tool/releases/download/v1.2.3"
            bin = ["tool", { path = "bin/helper-x86_64", name = "helper" }]
            requires = { glibc_min = "2.31" }

            [source]
            repo = "https://github.com/owner/tool"
            tag = "v1.2.3"

            [[artifact]]
            path = "dist/tool-1.2.3-linux-x64.tar.gz"
            bin = ["tool-1.2.3-linux-x64/tool"]
            requires = { glibc_min = "2.17" }

            [[artifact]]
            path = "dist/tool-1.2.3-windows-x64.exe"
            format = "raw"

            [[artifact]]
            path = "dist/tool.jar"
            portable = true
            bin = []

            [[resource]]
            kind = "completion"
            shell = "zsh"
            archive = "share/zsh/site-functions/_tool"

            [[resource]]
            kind = "sbom"
            format = "cyclonedx"
            asset = "dist/tool.cdx.json"
            "#,
        )
        .unwrap();
        assert_eq!(m.project.as_deref(), Some("github.com/owner/tool"));
        assert_eq!(m.bin[1], Bin::named("bin/helper-x86_64", "helper"));
        assert_eq!(m.artifacts.len(), 3);
        let linux = &m.artifacts[0];
        assert_eq!(linux.bins(&m.bin), [Bin::new("tool-1.2.3-linux-x64/tool")]);
        assert_eq!(
            linux
                .requirements(m.requires.as_ref())
                .unwrap()
                .glibc_min
                .as_deref(),
            Some("2.17")
        );
        let windows = &m.artifacts[1];
        assert_eq!(windows.bins(&m.bin).len(), 2);
        assert_eq!(windows.format.as_deref(), Some("raw"));
        assert_eq!(
            windows
                .requirements(m.requires.as_ref())
                .unwrap()
                .glibc_min
                .as_deref(),
            Some("2.31")
        );
        let jar = &m.artifacts[2];
        assert!(jar.portable);
        assert!(jar.bins(&m.bin).is_empty());
        let (sbom, path) = m.resources[1].resolve().unwrap();
        assert_eq!(sbom.asset.as_deref(), Some("tool.cdx.json"));
        assert_eq!(sbom.format.as_deref(), Some("cyclonedx"));
        assert_eq!(path, Some(PathBuf::from("dist/tool.cdx.json")));
        assert_eq!(m.source.unwrap().tag.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn unknown_keys_and_duplicates_are_refused() {
        assert!(Manifest::parse("bins = [\"tool\"]").is_err());
        assert!(Manifest::parse("[[artifact]]\npath = \"a\"\nplatform = \"linux\"").is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("release.toml");
        std::fs::write(
            &path,
            "[[artifact]]\npath = \"dist/a.tar.gz\"\n[[artifact]]\npath = \"dist/a.tar.gz\"\n",
        )
        .unwrap();
        let err = Manifest::read(&path).unwrap_err();
        assert!(matches!(err, Error::DuplicateArtifact(_)), "{err}");
        std::fs::write(&path, "version = 1").unwrap();
        let err = Manifest::read(&path).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "{err}");
        let err = Manifest::read(&dir.path().join("missing.toml")).unwrap_err();
        assert!(matches!(err, Error::Io { .. }), "{err}");
    }
}
