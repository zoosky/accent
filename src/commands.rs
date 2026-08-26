//! What each subcommand actually does.
//!
//! The install flow follows `install.sh` step for step — detect, resolve,
//! check what is already there, download, verify, extract, place, then warn
//! about PATH — so that switching between the shell installer and this one
//! produces the same result and the same messages.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::net::Http;
use crate::platform::{self, Platform};
use crate::release::{self, Repo, Version, OLDEST_PUBLISHED};
use crate::ui::Ui;
use crate::{archive, checksums, install, verify};

/// Everything a command needs: where to install, what to talk to, and how
/// loudly to say so.
pub struct Session {
    /// Directory the `accent` binary is placed in.
    pub dir: PathBuf,
    /// Where progress and warnings go.
    pub ui: Ui,
    /// Repository releases are downloaded from.
    pub repo: Repo,
    /// HTTP client shared by every step.
    pub http: Http,
}

impl Session {
    /// Builds a session, falling back to the platform's default install
    /// directory when none was given.
    pub fn new(dir: Option<PathBuf>, quiet: bool) -> Result<Self> {
        let dir = match dir {
            Some(dir) => dir,
            None => platform::default_install_dir()?,
        };
        Ok(Self {
            dir,
            ui: Ui::new(quiet),
            repo: Repo::from_env(),
            http: Http::new(),
        })
    }
}

/// Downloads, verifies and installs a release.
///
/// With no `version`, the latest one. Without `force`, an existing
/// installation of the same version is a no-op and of any other version an
/// error. With `dry_run`, everything runs but nothing is written.
pub fn install(session: &Session, version: Option<&str>, force: bool, dry_run: bool) -> Result<()> {
    let ui = &session.ui;
    ui.say("Accent CMS Installer");
    ui.say("===================");
    ui.blank();

    let platform = platform::detect()?;
    ui.say(format!(
        "Detected platform: {} {} ({})",
        platform.os, platform.arch, platform.target
    ));

    let target = resolve_version(session, version)?;

    if !force && already_installed(session, &target)? {
        return Ok(());
    }

    fetch_and_place(session, &platform, &target, dry_run)
}

/// Installs the latest release over an existing installation, or reports
/// that it is already current.
pub fn update(session: &Session, force: bool) -> Result<()> {
    let ui = &session.ui;
    let platform = platform::detect()?;
    let current = install::installed_version(&session.dir);

    let latest = resolve_version(session, None)?;

    match &current {
        Some(current) if current == &latest && !force => {
            ui.say(format!("Already up to date ({latest})."));
            install::check_path(&session.dir, ui);
            install::check_shadowing(&session.dir, ui);
            return Ok(());
        }
        Some(current) => ui.say(format!("Updating {current} -> {latest}")),
        None => ui.say(format!(
            "Accent CMS is not installed in {} — installing {latest}",
            session.dir.display()
        )),
    }

    fetch_and_place(session, &platform, &latest, false)
}

/// Removes the installed binary, and with `purge` the product's user-level
/// state as well.
pub fn uninstall(session: &Session, purge: bool) -> Result<()> {
    let ui = &session.ui;
    let binary = install::binary_in(&session.dir);

    if install::remove(&session.dir)? {
        ui.say(format!("Removed {}", binary.display()));
    } else {
        ui.say(format!("Nothing to remove at {}", binary.display()));
    }

    if purge {
        install::purge_data(ui)?;
        ui.say("Licence key and cached development certificate are gone.");
    }

    // A copy elsewhere on PATH keeps `accent` working after an uninstall,
    // which looks like the uninstall silently failed.
    if let Some(other) = install::resolve_on_path(platform::BIN_NAME) {
        ui.blank();
        ui.warn(format!(
            "Note: 'accent' still resolves to {} — this installer did not put it there.",
            other.display()
        ));
    }
    Ok(())
}

/// Prints the path and version of the installed binary on stdout.
pub fn which(session: &Session) -> Result<()> {
    let binary = install::binary_in(&session.dir);
    if !binary.exists() {
        bail!(
            "Accent CMS is not installed in {}\nRun `accentup install` to install it.",
            session.dir.display()
        );
    }

    match install::installed_version(&session.dir) {
        Some(version) => println!("{}  ({version})", binary.display()),
        None => println!("{}  (unknown version)", binary.display()),
    }

    install::check_shadowing(&session.dir, &session.ui);
    Ok(())
}

// ------------------------------------------------------------------ steps ---

fn resolve_version(session: &Session, requested: Option<&str>) -> Result<Version> {
    let ui = &session.ui;
    match requested {
        Some(raw) => {
            let version = Version::parse(raw)?;
            ui.say(format!("Installing version: {version}"));
            Ok(version)
        }
        None => {
            ui.say("Fetching latest version...");
            let version = release::resolve_latest(&session.http, &session.repo)?;
            ui.say(format!("Latest version: {version}"));
            Ok(version)
        }
    }
}

/// Handles the "there is already a binary here" case, mirroring `install.sh`:
/// the same version is a success and stops, a different one asks for --force.
/// Returns `true` when the caller should stop.
fn already_installed(session: &Session, target: &Version) -> Result<bool> {
    let ui = &session.ui;
    let binary = install::binary_in(&session.dir);
    if !binary.exists() {
        return Ok(false);
    }

    let current = install::installed_version(&session.dir);
    match &current {
        Some(current) => ui.say(format!(
            "Accent CMS is already installed: accent {}",
            current.number()
        )),
        None => ui.say(format!(
            "Accent CMS is already installed at: {}",
            binary.display()
        )),
    }

    if current.as_ref() == Some(target) {
        ui.say(format!("Already up to date ({target})."));
        // A correct file in the install directory is not the whole story: a
        // stale binary earlier in PATH would still be the one that runs, and
        // a cheerful "up to date" here would hide exactly that.
        install::check_path(&session.dir, ui);
        install::check_shadowing(&session.dir, ui);
        return Ok(true);
    }

    bail!(
        "{} already exists.\n\
         To reinstall or update in place: accentup update  (or accentup install --force)\n\
         Or remove it first: rm {}",
        binary.display(),
        binary.display()
    )
}

/// Download, verify, extract, place.
fn fetch_and_place(
    session: &Session,
    platform: &Platform,
    version: &Version,
    dry_run: bool,
) -> Result<()> {
    let ui = &session.ui;
    let asset = platform.asset_name(&version.tag());
    let workdir = tempfile::Builder::new()
        .prefix("accentup-")
        .tempdir()
        .context("could not create a temporary directory")?;
    let archive_path = workdir.path().join(&asset);

    ui.say(format!("Downloading {asset}..."));
    let digest = session
        .http
        .download(
            &session.repo.download_url(version, &asset),
            &archive_path,
            ui,
        )
        .with_context(|| {
            format!(
                "downloading {asset} ({version} may not be published — see {}, which carries \
                 {OLDEST_PUBLISHED} and later)",
                session.repo.releases_page()
            )
        })?;

    verify_download(session, version, &asset, &digest)?;

    if dry_run {
        ui.blank();
        ui.say(format!(
            "Dry run: accent {version} for {} verified, nothing written.",
            platform.target
        ));
        return Ok(());
    }

    ui.say("Extracting...");
    let staged = archive::unpack(&archive_path, workdir.path(), platform.archive)?;
    let installed = install::place(&staged, &session.dir)?;
    ui.say(format!("Installed accent to {}", installed.display()));

    install::check_path(&session.dir, ui);
    install::check_shadowing(&session.dir, ui);

    ui.blank();
    ui.say("Installation complete! Run 'accent --version' to verify.");
    Ok(())
}

/// Signature over the checksums file, then the archive against the checksums.
///
/// Both are required. `install.sh` degrades to checksum-only when `gpg` is
/// missing and to no verification at all when the checksums file 404s; here
/// the key is compiled in and the verifier is in-process, so a failure at
/// either step means something is wrong with the download, not with the
/// machine.
fn verify_download(session: &Session, version: &Version, asset: &str, digest: &str) -> Result<()> {
    let ui = &session.ui;

    ui.say("Downloading checksums...");
    let checksums = session
        .http
        .get_text(&session.repo.checksums_url(version))
        .with_context(|| format!("downloading {}", release::checksums_name(version)))?;
    let signature = session
        .http
        .get_text(&session.repo.signature_url(version))
        .with_context(|| format!("downloading {}.asc", release::checksums_name(version)))?;

    verify::signature(checksums.as_bytes(), &signature).with_context(|| {
        format!(
            "signature verification failed, nothing was installed — do not use this download; \
             report it via https://github.com/{}/discussions",
            session.repo.slug()
        )
    })?;
    ui.say("Signature verified.");

    let expected = checksums::lookup(&checksums, asset)?;
    verify::checksum(expected, digest)
        .context("the download does not match its published checksum — nothing was installed")?;
    ui.say("Checksum verified.");
    Ok(())
}
