//! Placing, inspecting and removing the installed binary.

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

/// Runs a downloaded binary once, before it is installed.
///
/// Everything before this point proves the archive is the one that was
/// published; nothing proves this machine can start what is inside it. A
/// Linux release built against a newer glibc than the system has is the
/// case that made this necessary: the v0.25.0 binaries needed glibc 2.39 and
/// the dynamic loader refused them on Debian 12 and Ubuntu 22.04, while the
/// installers reported success over them. So `accent --version` is run on
/// the staged file. Success returns the line it printed; failure returns an
/// error that quotes the loader and, when the loader named a `GLIBC_x.y` the
/// system lacks, says which glibc the release needs and which one the
/// system has, so the user can tell an unsupported system from a broken
/// release. `report_url` is where the latter should go.
pub fn runs(binary: &Path, report_url: &str) -> Result<String> {
    let output = match Command::new(binary).arg("--version").output() {
        Ok(output) => output,
        Err(err) => {
            let mut msg = format!(
                "the downloaded binary cannot run on this system:\n  {}: {err}",
                binary.display()
            );
            // The loader itself is what is missing when exec fails with "not
            // found" on a file that exists: the ELF interpreter the binary
            // names (`/lib64/ld-linux-x86-64.so.2`) is glibc's, and a musl
            // system does not have it.
            if cfg!(target_os = "linux")
                && err.kind() == std::io::ErrorKind::NotFound
                && binary.exists()
            {
                msg.push_str(
                    "\nThe Linux binaries are built against glibc. Systems without it (Alpine and \
                     other musl-based distributions) are not supported; build Accent CMS from source.",
                );
            }
            return Err(anyhow!(msg));
        }
    };

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut msg = String::from("the downloaded binary cannot run on this system:");
    for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
        msg.push_str("\n  ");
        msg.push_str(line);
    }
    if stderr.trim().is_empty() {
        msg.push_str(&format!(
            "\n  {} --version: {}",
            binary.display(),
            output.status
        ));
    }

    if let Some((major, minor)) = highest_glibc_need(&stderr) {
        let have = system_glibc().unwrap_or_else(|| "unknown".to_string());
        msg.push_str(&format!(
            "\nThis release needs glibc {major}.{minor} or newer; this system has glibc {have}.\n\
             The supported Linux versions are listed at https://accentcms.dev/download.\n\
             If this system is one of them, the release is at fault: please report it via {report_url}"
        ));
    }
    Err(anyhow!(msg))
}

/// The newest `GLIBC_x.y` a dynamic-loader complaint names, as `(x, y)`.
///
/// The loader prints one `version \`GLIBC_2.39' not found` line per missing
/// symbol version; the highest of them is the floor the binary really has.
/// `GLIBC_PRIVATE` and other non-numeric tags are ignored.
pub fn highest_glibc_need(loader_stderr: &str) -> Option<(u32, u32)> {
    loader_stderr
        .match_indices("GLIBC_")
        .filter_map(|(at, _)| {
            let rest = &loader_stderr[at + "GLIBC_".len()..];
            let version: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let (major, minor) = version.split_once('.')?;
            let minor = minor.split('.').next()?;
            Some((major.parse().ok()?, minor.parse().ok()?))
        })
        .max()
}

/// The glibc this system runs, as `getconf` reports it (`2.36`), when it is
/// a glibc system at all.
fn system_glibc() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("getconf")
            .arg("GNU_LIBC_VERSION")
            .output()
            .ok()
            .filter(|out| out.status.success())?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.split_whitespace().nth(1).map(str::to_string)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
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

/// Warns when the install directory is not on PATH, with the incantation for
/// the user's shell.
pub fn check_path(dir: &Path, ui: &Ui) {
    if on_path(dir) {
        return;
    }

    ui.blank();
    ui.warn(format!("Note: {} is not on your PATH.", dir.display()));

    if cfg!(windows) {
        ui.warn("To add it, run:");
        ui.warn("");
        ui.warn("  $path = [Environment]::GetEnvironmentVariable('Path', 'User')");
        ui.warn(format!(
            "  [Environment]::SetEnvironmentVariable('Path', \"$path;{}\", 'User')",
            dir.display()
        ));
        ui.warn("");
        ui.warn("Then restart your terminal.");
        return;
    }

    let shell = std::env::var("SHELL").unwrap_or_default();
    let dir = dir.display();
    if shell.ends_with("fish") {
        ui.warn("Add it by appending this to ~/.config/fish/config.fish:");
        ui.warn(format!("  fish_add_path {dir}"));
    } else {
        let profile = if shell.ends_with("zsh") {
            "~/.zshrc"
        } else {
            "~/.bashrc"
        };
        ui.warn(format!("Add it by appending this to {profile}:"));
        ui.warn(format!("  export PATH=\"{dir}:$PATH\""));
    }
    ui.warn("Then restart your shell.");
}

/// Warns when `accent` resolves to a different binary than the one just
/// installed.
///
/// The shell resolves `accent` by PATH order, not by what this installer
/// wrote. A stale copy earlier in PATH — a `cargo install`ed one in
/// `~/.cargo/bin` is the usual case — silently wins, and `accent --version`
/// keeps reporting the old build while the user believes they upgraded.
pub fn check_shadowing(dir: &Path, ui: &Ui) {
    let installed = binary_in(dir);
    let Some(resolved) = resolve_on_path(BIN_NAME) else {
        return;
    };
    if same_file(&resolved, &installed) {
        return;
    }

    let version = Command::new(&resolved)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown version".to_string());

    ui.blank();
    ui.warn("Warning: 'accent' currently resolves to a different binary:");
    ui.warn(format!("  {} ({version})", resolved.display()));
    ui.warn(format!(
        "which shadows the up-to-date binary at {}.",
        installed.display()
    ));
    if cfg!(windows) {
        ui.warn(format!(
            "Remove the shadowing binary or move {} earlier in your PATH,",
            dir.display()
        ));
        ui.warn("then start a new terminal.");
    } else {
        ui.warn(format!(
            "Remove the shadowing binary or move {} earlier in your PATH,",
            dir.display()
        ));
        ui.warn("then run 'hash -r' (bash/zsh) or restart your shell.");
    }
}

/// Whether `dir` is one of the entries in `PATH`.
pub fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

/// First `name` on PATH, the way the shell would pick it.
pub fn resolve_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Compares two paths after resolving symlinks, so `~/.local/bin/accent` and a
/// symlink pointing at it are not reported as shadowing each other.
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

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
    fn the_highest_glibc_the_loader_names_is_the_floor() {
        let stderr = "accent: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by accent)\n\
                      accent: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found (required by accent)\n\
                      accent: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.4' not found (required by accent)\n";
        assert_eq!(highest_glibc_need(stderr), Some((2, 39)));
        assert_eq!(
            highest_glibc_need("version `GLIBC_PRIVATE' not found"),
            None
        );
        assert_eq!(highest_glibc_need("Segmentation fault"), None);
        assert_eq!(highest_glibc_need(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_runs_reports_its_version_line() {
        let dir = tempfile::tempdir().unwrap();
        let bin = stub(
            dir.path(),
            "ok",
            "#!/bin/sh\necho 'accent 0.26.0 (abc1234)'\n",
        );
        assert_eq!(
            runs(&bin, "https://example.invalid").unwrap(),
            "accent 0.26.0 (abc1234)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_loader_refusal_names_the_glibc_the_release_needs() {
        let dir = tempfile::tempdir().unwrap();
        let bin = stub(
            dir.path(),
            "refused",
            "#!/bin/sh\n\
             echo \"$0: /lib/x86_64-linux-gnu/libc.so.6: version \\`GLIBC_2.38' not found (required by $0)\" >&2\n\
             echo \"$0: /lib/x86_64-linux-gnu/libc.so.6: version \\`GLIBC_2.39' not found (required by $0)\" >&2\n\
             exit 127\n",
        );
        let text = runs(&bin, "https://example.invalid/discussions")
            .unwrap_err()
            .to_string();
        assert!(
            text.starts_with("the downloaded binary cannot run on this system:"),
            "{text}"
        );
        assert!(text.contains("GLIBC_2.39' not found"), "{text}");
        assert!(
            text.contains("This release needs glibc 2.39 or newer; this system has glibc "),
            "{text}"
        );
        assert!(text.contains("https://accentcms.dev/download"), "{text}");
        assert!(
            text.contains("report it via https://example.invalid/discussions"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failure_without_glibc_in_it_is_quoted_without_a_diagnosis() {
        let dir = tempfile::tempdir().unwrap();
        let bin = stub(
            dir.path(),
            "crash",
            "#!/bin/sh\necho 'Illegal instruction' >&2\nexit 132\n",
        );
        let text = runs(&bin, "https://example.invalid")
            .unwrap_err()
            .to_string();
        assert!(text.contains("  Illegal instruction"), "{text}");
        assert!(!text.contains("needs glibc"), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_executed_at_all_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("missing");
        let text = runs(&bin, "https://example.invalid")
            .unwrap_err()
            .to_string();
        assert!(
            text.starts_with("the downloaded binary cannot run on this system:"),
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
