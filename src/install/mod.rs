//! Placing, inspecting and removing the installed binary.
//!
//! The last step of an install, split by concern: this file places,
//! inspects and removes the binary; `verify_runs.rs` runs a downloaded
//! binary once before it is put in place; `path.rs` warns about PATH and
//! shadowing once it is. Everything is re-exported here, so callers see one
//! `install` module.

mod path;
mod verify_runs;

pub use path::{check_path, check_shadowing, on_path, resolve_on_path};
pub use verify_runs::{highest_glibc_need, runs};

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::platform::{self, BIN_NAME};
use crate::release::Version;
use crate::ui::Ui;

/// Path the `accent` binary occupies inside an install directory.
pub fn binary_in(dir: &Path) -> PathBuf {
    dir.join(BIN_NAME)
}

/// Asks an installed binary what version it is.
///
/// `accent --version` prints `accent <version> …`; the second token is the
/// bare number, the same field `install.sh` takes with awk. Returns `None`
/// for anything unparseable — an old build, a wrapper script, a corrupt file —
/// so callers treat it as "unknown", never as a match.
pub fn installed_version(dir: &Path) -> Option<Version> {
    let output = Command::new(binary_in(dir))
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let token = text.split_whitespace().nth(1)?;
    Version::parse(token).ok()
}

/// Moves the staged binary into place, after `check` has passed it.
///
/// Copies to a sibling temporary file inside the target directory first, then
/// renames: on one filesystem that swap is atomic, so a half-written binary
/// never appears on PATH, and an interrupted install leaves the previous
/// version intact.
///
/// `check` runs against that temporary file before the rename, so it can
/// execute the binary from the directory it will live in (a temporary
/// directory may be mounted `noexec`; the install directory by definition is
/// not). When it fails, the temporary file is removed, the previous version
/// stays in place, and its error is returned unchanged with one line added
/// saying so. The installer passes [`runs`]; tests pass whatever they need.
pub fn place(
    staged: &Path,
    dir: &Path,
    check: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let target = binary_in(dir);
    let pending = dir.join(format!(".{BIN_NAME}.new"));

    std::fs::copy(staged, &pending).with_context(|| format!("writing {}", pending.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("making {} executable", pending.display()))?;
    }

    if let Err(err) = check(&pending) {
        let _ = std::fs::remove_file(&pending);
        return Err(anyhow!(
            "{err}\nNothing was installed; any earlier version at {} is unchanged.",
            target.display()
        ));
    }

    // Windows refuses to rename over a running executable, so move the old
    // one aside first; the rename below then lands on a free name.
    #[cfg(windows)]
    let backup = dir.join(format!(".{BIN_NAME}.old"));
    #[cfg(windows)]
    if target.exists() {
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&target, &backup)
            .context("could not replace the running binary — close Accent CMS and retry")?;
    }

    std::fs::rename(&pending, &target).with_context(|| {
        let _ = std::fs::remove_file(&pending);
        format!("installing {}", target.display())
    })?;

    // The old binary is only needed until the new one is in place. Removing
    // it fails while that binary is still running, which Windows allows for
    // a rename but not a delete; the next install clears it, above.
    #[cfg(windows)]
    let _ = std::fs::remove_file(&backup);

    Ok(target)
}

/// Removes the installed binary. Returns whether there was one to remove.
pub fn remove(dir: &Path) -> Result<bool> {
    let binary = binary_in(dir);
    if !binary.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&binary).with_context(|| format!("removing {}", binary.display()))?;
    Ok(true)
}

/// Deletes the user-level state the product writes: the licence key and the
/// development certificate cache. Never touches the install directory itself,
/// which is shared with other tools.
pub fn purge_data(ui: &Ui) -> Result<()> {
    for dir in platform::data_dirs() {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
            ui.say(format!("Removed {}", dir.display()));
        }
    }
    Ok(())
}

/// A stand-in executable for tests: a shell script with `body`, made
/// executable, at `dir/name`. Unix only, like every test that runs one.
#[cfg(all(test, unix))]
pub(crate) fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_a_binary_and_replaces_it_in_place() {
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let dir = target.path().join("bin");

        let first = staging.path().join("accent");
        std::fs::write(&first, b"version one").unwrap();
        let installed = place(&first, &dir, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"version one");

        std::fs::write(&first, b"version two").unwrap();
        place(&first, &dir, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"version two");

        // No temporary file left behind by either install.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|name| name != BIN_NAME)
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[cfg(unix)]
    #[test]
    fn placed_binaries_are_executable() {
        use std::os::unix::fs::PermissionsExt;
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let staged = staging.path().join("accent");
        std::fs::write(&staged, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600)).unwrap();

        let installed = place(&staged, target.path(), |_| Ok(())).unwrap();
        let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn a_failed_check_installs_nothing_and_keeps_the_old_binary() {
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let installed = binary_in(target.path());
        std::fs::write(&installed, b"old and working").unwrap();

        let staged = staging.path().join("accent");
        std::fs::write(&staged, b"new and broken").unwrap();
        let mut checked = None;
        let err = place(&staged, target.path(), |pending| {
            checked = Some(pending.to_path_buf());
            assert_eq!(std::fs::read(pending).unwrap(), b"new and broken");
            Err(anyhow!("cannot run: GLIBC_2.39 not found"))
        })
        .unwrap_err();

        // The check saw the copy inside the install directory, not the
        // staging file, and the copy is gone afterwards.
        assert_eq!(checked.unwrap().parent().unwrap(), target.path());
        let leftovers: Vec<_> = std::fs::read_dir(target.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|name| name != BIN_NAME)
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        assert_eq!(std::fs::read(&installed).unwrap(), b"old and working");
        let text = err.to_string();
        assert!(
            text.starts_with("cannot run: GLIBC_2.39 not found"),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "Nothing was installed; any earlier version at {} is unchanged.",
                installed.display()
            )),
            "{text}"
        );
    }

    #[test]
    fn remove_reports_whether_anything_was_there() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!remove(dir.path()).unwrap());
        std::fs::write(binary_in(dir.path()), b"binary").unwrap();
        assert!(remove(dir.path()).unwrap());
        assert!(!binary_in(dir.path()).exists());
    }

    #[cfg(unix)]
    #[test]
    fn reads_the_version_an_installed_binary_reports() {
        let dir = tempfile::tempdir().unwrap();
        stub(
            dir.path(),
            BIN_NAME,
            "#!/bin/sh\necho 'accent 0.25.0 (abc1234 2026-08-21)'\n",
        );
        assert_eq!(
            installed_version(dir.path()).unwrap(),
            Version::parse("0.25.0").unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_unparseable_version_is_unknown_not_a_match() {
        let dir = tempfile::tempdir().unwrap();
        stub(dir.path(), BIN_NAME, "#!/bin/sh\necho garbage\n");
        assert_eq!(installed_version(dir.path()), None);

        let empty = tempfile::tempdir().unwrap();
        assert_eq!(installed_version(empty.path()), None);
    }
}
