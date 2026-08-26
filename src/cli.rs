//! Command line surface.
//!
//! The environment variables mirror the ones the shell installers accept, for
//! the same reason they exist there: in `curl … | sh`, a flag written after
//! the URL is eaten by curl and never reaches the installer, so there has to
//! be a way to pass the same intent through the environment. An explicit flag
//! always wins over its variable.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "accentup",
    version,
    about = "Install and update Accent CMS",
    long_about = "Installs Accent CMS from https://github.com/AccentCMS/accent.\n\n\
                  Downloads the release archive for this platform, verifies the published \
                  checksums against the Accent CMS release signing key, checks the archive \
                  against those checksums, and places the `accent` binary atomically. \
                  Verification is not optional.",
    after_help = "Environment:\n  \
        ACCENT_VERSION       Version to install (same as --version)\n  \
        ACCENT_FORCE         Any value but empty or 0 means --force\n  \
        ACCENT_INSTALL_DIR   Where the binary goes (same as --dir)\n  \
        ACCENT_REPO          Repository to download from (default AccentCMS/accent)"
)]
/// Everything `accentup` accepts on the command line.
pub struct Cli {
    /// The subcommand, defaulting to `install` when absent.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Directory the `accent` binary is placed in.
    #[arg(long, global = true, env = "ACCENT_INSTALL_DIR", value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Print only warnings and errors.
    #[arg(long, short, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand, Debug)]
/// The subcommands.
pub enum Command {
    /// Download and install Accent CMS. The default when no command is given.
    Install {
        /// Version to install, with or without the `v` (default: latest).
        #[arg(long, env = "ACCENT_VERSION", value_name = "VERSION")]
        version: Option<String>,

        /// Reinstall over an existing installation.
        #[arg(long)]
        force: bool,

        /// Download and verify, then stop without writing anything.
        #[arg(long)]
        dry_run: bool,
    },

    /// Update an existing installation to the latest release.
    Update {
        /// Reinstall even when the installed version is already current.
        #[arg(long)]
        force: bool,
    },

    /// Remove the installed binary.
    Uninstall {
        /// Also delete the licence key and cached development certificate.
        #[arg(long)]
        purge: bool,
    },

    /// Print the path and version of the installed binary.
    Which,
}

impl Cli {
    /// The command to run, defaulting to `install` so that a bare `accentup`
    /// does the obvious thing.
    pub fn command_or_default(self) -> Command {
        self.command.unwrap_or(Command::Install {
            version: std::env::var("ACCENT_VERSION")
                .ok()
                .filter(|v| !v.is_empty()),
            force: false,
            dry_run: false,
        })
    }
}

/// `ACCENT_FORCE`, read with the same rule as the shell installers: set, and
/// neither empty nor `0`, counts as force.
pub fn force_from_env() -> bool {
    match std::env::var("ACCENT_FORCE") {
        Ok(value) => !matches!(value.trim(), "" | "0"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_the_documented_invocations() {
        let cli =
            Cli::try_parse_from(["accentup", "install", "--version", "0.25.0", "--force"]).unwrap();
        match cli.command_or_default() {
            Command::Install {
                version,
                force,
                dry_run,
            } => {
                assert_eq!(version.as_deref(), Some("0.25.0"));
                assert!(force);
                assert!(!dry_run);
            }
            other => panic!("{other:?}"),
        }

        let cli = Cli::try_parse_from(["accentup", "--dir", "/tmp/bin", "update"]).unwrap();
        assert_eq!(cli.dir.as_deref(), Some(std::path::Path::new("/tmp/bin")));
        assert!(matches!(
            cli.command_or_default(),
            Command::Update { force: false }
        ));
    }

    #[test]
    fn a_bare_invocation_installs() {
        let cli = Cli::try_parse_from(["accentup"]).unwrap();
        assert!(matches!(cli.command_or_default(), Command::Install { .. }));
    }
}
