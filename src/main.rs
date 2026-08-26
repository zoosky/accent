//! `accentup` — the installer and updater for Accent CMS.

use anyhow::Result;
use clap::Parser;

use accent::cli::{self, Cli, Command};
use accent::commands::{self, Session};

fn main() {
    if let Err(err) = run() {
        // One line per link in the chain: the top line says what failed, the
        // rest say why, without running into a single unreadable sentence.
        eprintln!("Error: {err}");
        for cause in err.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let session = Session::new(cli.dir.clone(), cli.quiet)?;
    let forced_by_env = cli::force_from_env();

    match cli.command_or_default() {
        Command::Install {
            version,
            force,
            dry_run,
        } => commands::install(
            &session,
            version.as_deref(),
            force || forced_by_env,
            dry_run,
        ),
        Command::Update { force } => commands::update(&session, force || forced_by_env),
        Command::Uninstall { purge } => commands::uninstall(&session, purge),
        Command::Which => commands::which(&session),
    }
}
