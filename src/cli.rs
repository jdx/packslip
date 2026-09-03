//! Glue for the binary: turn a usage-rs parse result into either the parsed
//! command or the help, version, or failure text a user expects, with
//! clap's exit codes.

use std::ffi::{OsStr, OsString};

use eyre::Result;
use usage_rs::RunWith;

/// Borrow `argv` as the slice of `OsStr` that usage-rs parses.
pub fn argv(args: &[OsString]) -> Vec<&OsStr> {
    args.iter().map(OsString::as_os_str).collect()
}

/// Unwrap a parse result, or print help, version, or a usage failure and exit.
///
/// `argv` is the full command line including the program name at index 0.
/// Exit codes follow clap: 0 for help and version, 2 for a usage error.
pub fn unwrap_or_exit<T>(
    spec: &usage_rs::spec::Spec<'_>,
    argv: &[&OsStr],
    result: Result<T, usage_rs::Error<'_, '_>>,
) -> T {
    match result {
        Ok(cli) => cli,
        Err(err) => exit_with_usage_error(spec, argv.get(1..).unwrap_or_default(), err),
    }
}

fn exit_with_usage_error(
    spec: &usage_rs::spec::Spec<'_>,
    args: &[&OsStr],
    err: usage_rs::Error<'_, '_>,
) -> ! {
    match err {
        usage_rs::Error::Help { cmd, long } => {
            if let Some(page) = usage_rs::help::render(spec, cmd, long) {
                print!("{page}");
            }
            std::process::exit(0)
        }
        usage_rs::Error::HelpAll { cmd } => {
            if let Some(page) = usage_rs::help::render_all(spec, cmd) {
                print!("{page}");
            }
            std::process::exit(0)
        }
        usage_rs::Error::MissingArgsHelp { cmd } => {
            if let Some(page) = usage_rs::help::render(spec, cmd, false) {
                eprint!("{page}");
            }
            std::process::exit(2)
        }
        usage_rs::Error::Version { long } => {
            let version = if long {
                spec.long_version.or(spec.version)
            } else {
                spec.version
            }
            .unwrap_or_default();
            println!("{} {version}", spec.name);
            std::process::exit(0)
        }
        err => {
            eprint!("{}", usage_rs::render_failure(spec, args, &err));
            std::process::exit(2)
        }
    }
}

/// Show the version
#[derive(Debug, usage_rs::Args)]
#[usage(visible_alias = "v")]
pub struct Version {
    /// Print as JSON
    #[usage(short = 'J', long)]
    json: bool,
}

/// The binary's identity, handed to commands that print it.
#[derive(Debug, Clone, Copy)]
pub struct BinInfo {
    pub name: &'static str,
    pub version: &'static str,
}

impl AsRef<BinInfo> for BinInfo {
    fn as_ref(&self) -> &BinInfo {
        self
    }
}

impl<Ctx: AsRef<BinInfo>> RunWith<Ctx> for Version {
    type Output = Result<()>;

    fn run_with(self, ctx: Ctx) -> Self::Output {
        let bin = ctx.as_ref();
        if self.json {
            let json = serde_json::json!({
                "name": bin.name,
                "version": bin.version,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        } else {
            println!("{} {}", bin.name, bin.version);
        }
        Ok(())
    }
}
