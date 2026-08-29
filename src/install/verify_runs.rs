//! Running a downloaded binary once before it is installed.
//!
//! Signature and checksum verification prove the archive is the one that
//! was published; nothing before this point proves the machine can start
//! what is inside it. This module answers that question with the one call
//! that matters, `accent --version`, and turns a refusal into a message
//! that tells an unsupported system apart from a broken release.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Result};

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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::install::stub;

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
}
