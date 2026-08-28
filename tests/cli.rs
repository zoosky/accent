//! End-to-end tests against the built `accentup` binary.
//!
//! Everything here except the `network` module runs offline: the flows that
//! stop at an existing installation never reach the network, which is exactly
//! what makes them cheap to assert on.
//!
//! Run the network tests with:
//!
//! ```sh
//! cargo test -- --ignored
//! ```

use std::process::{Command, Output};

/// Runs `accentup` with a clean environment, so that variables set in the
/// developer's shell cannot change what is being tested.
fn accentup(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_accentup"));
    for var in [
        "ACCENT_VERSION",
        "ACCENT_FORCE",
        "ACCENT_INSTALL_DIR",
        "ACCENT_REPO",
    ] {
        cmd.env_remove(var);
    }
    cmd.args(args).output().expect("running accentup")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Writes a stand-in for an installed `accent` that reports `version`.
///
/// Unix only, like the tests that use it: a shell script is the cheapest
/// executable that answers `--version`, and on Windows the binary would need
/// to be a real `.exe`. Everything it needs is imported here so that the
/// Windows build, which compiles the file without this function, has no
/// unused import to reject under `-D warnings`.
#[cfg(unix)]
fn fake_install(dir: &std::path::Path, version: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let path = accent::install::binary_in(dir);
    std::fs::write(
        &path,
        format!("#!/bin/sh\necho 'accent {version} (abc1234)'\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn reports_its_own_version() {
    let out = accentup(&["--version"]);
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("accentup "), "{}", stdout(&out));
}

#[test]
fn which_fails_when_nothing_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    let out = accentup(&["which", "--dir", dir.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("not installed"), "{}", stderr(&out));
}

#[test]
fn uninstalling_nothing_succeeds_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let out = accentup(&["uninstall", "--dir", dir.path().to_str().unwrap()]);
    assert!(out.status.success());
    assert!(
        stderr(&out).contains("Nothing to remove"),
        "{}",
        stderr(&out)
    );
}

#[cfg(unix)]
#[test]
fn which_prints_the_path_and_version_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    fake_install(dir.path(), "0.25.0");

    let out = accentup(&["which", "--dir", dir.path().to_str().unwrap()]);
    assert!(out.status.success());
    let line = stdout(&out);
    assert!(line.contains("accent"), "{line}");
    assert!(line.contains("v0.25.0"), "{line}");
}

#[cfg(unix)]
#[test]
fn installing_the_version_already_present_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    fake_install(dir.path(), "0.25.0");

    let out = accentup(&[
        "install",
        "--dir",
        dir.path().to_str().unwrap(),
        "--version",
        "0.25.0",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("Already up to date (v0.25.0)"),
        "{}",
        stderr(&out)
    );
}

#[cfg(unix)]
#[test]
fn installing_over_a_different_version_needs_force() {
    let dir = tempfile::tempdir().unwrap();
    fake_install(dir.path(), "0.24.0");

    let out = accentup(&[
        "install",
        "--dir",
        dir.path().to_str().unwrap(),
        "--version",
        "0.25.0",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let text = stderr(&out);
    assert!(text.contains("already exists"), "{text}");
    assert!(text.contains("accentup update"), "{text}");
}

#[cfg(unix)]
#[test]
fn accent_force_zero_does_not_mean_force() {
    let dir = tempfile::tempdir().unwrap();
    fake_install(dir.path(), "0.24.0");

    let out = Command::new(env!("CARGO_BIN_EXE_accentup"))
        .env("ACCENT_FORCE", "0")
        .args([
            "install",
            "--dir",
            dir.path().to_str().unwrap(),
            "--version",
            "0.25.0",
        ])
        .output()
        .unwrap();

    // Still refused: only a value other than empty or 0 forces.
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("already exists"), "{}", stderr(&out));
}

#[test]
fn an_unparseable_version_is_rejected_before_any_download() {
    let dir = tempfile::tempdir().unwrap();
    let out = accentup(&[
        "install",
        "--dir",
        dir.path().to_str().unwrap(),
        "--version",
        "../../etc/passwd",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("not a release version"),
        "{}",
        stderr(&out)
    );
}

/// Tests that talk to github.com. Ignored by default so `cargo test` stays
/// offline and deterministic.
mod network {
    use super::*;

    #[test]
    #[ignore = "requires network access to github.com"]
    fn dry_run_verifies_the_latest_release_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let out = accentup(&[
            "install",
            "--dir",
            dir.path().to_str().unwrap(),
            "--dry-run",
        ]);
        assert!(out.status.success(), "{}", stderr(&out));

        let text = stderr(&out);
        assert!(text.contains("Signature verified."), "{text}");
        assert!(text.contains("Checksum verified."), "{text}");
        assert!(text.contains("nothing written"), "{text}");
        assert!(!accent::install::binary_in(dir.path()).exists());
    }

    #[test]
    #[ignore = "requires network access to github.com"]
    fn installs_and_the_installed_binary_runs() {
        let dir = tempfile::tempdir().unwrap();
        let out = accentup(&["install", "--dir", dir.path().to_str().unwrap()]);
        assert!(out.status.success(), "{}", stderr(&out));

        // `accent.exe` on Windows: ask the crate rather than spell the name.
        let binary = accent::install::binary_in(dir.path());
        assert!(binary.is_file(), "{} was not installed", binary.display());

        let reported = Command::new(&binary).arg("--version").output().unwrap();
        assert!(reported.status.success());
        assert!(
            String::from_utf8_lossy(&reported.stdout).starts_with("accent "),
            "{}",
            String::from_utf8_lossy(&reported.stdout)
        );

        // Second run is a no-op rather than a re-download.
        let again = accentup(&["install", "--dir", dir.path().to_str().unwrap()]);
        assert!(again.status.success(), "{}", stderr(&again));
        assert!(
            stderr(&again).contains("Already up to date"),
            "{}",
            stderr(&again)
        );

        let removed = accentup(&["uninstall", "--dir", dir.path().to_str().unwrap()]);
        assert!(removed.status.success());
        assert!(!binary.exists());
    }
}
