//! The `checksums-<version>.txt` published with every release.
//!
//! Plain `sha256sum` output: a hex digest, whitespace, then the file name,
//! optionally prefixed with `*` for binary mode.

use anyhow::{bail, Result};

/// Looks up the digest recorded for `asset`.
pub fn lookup<'a>(text: &'a str, asset: &str) -> Result<&'a str> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((digest, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let name = name.trim().trim_start_matches('*');
        if name == asset {
            let digest = digest.trim();
            if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("{asset} has a malformed SHA-256 entry in the checksums file");
            }
            return Ok(digest);
        }
    }
    bail!("the checksums file has no entry for {asset}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from the v0.25.0 release, trimmed to three lines.
    const SAMPLE: &str = "\
98e505528fac8256353fd8cf27f4017dd3da3580424fed9f95d4d0e1e4b4b30d  accent-v0.25.0-aarch64-apple-darwin.tar.gz
0b88021d08464ac996893132ce20c4aa0b2da5ed26caa5c3fb57b9a512379ae2  accent-v0.25.0-aarch64-pc-windows-msvc.zip
cb4a909a0a1a1cd1b4636964cfc4954b308c232f0827e50c6ae99ef479ca7128  accent-v0.25.0-aarch64-unknown-linux-gnu.tar.gz
";

    #[test]
    fn finds_the_entry_for_an_asset() {
        assert_eq!(
            lookup(SAMPLE, "accent-v0.25.0-aarch64-apple-darwin.tar.gz").unwrap(),
            "98e505528fac8256353fd8cf27f4017dd3da3580424fed9f95d4d0e1e4b4b30d"
        );
    }

    #[test]
    fn accepts_binary_mode_and_ragged_whitespace() {
        let text = "  aa11 *nope\n\
            98e505528fac8256353fd8cf27f4017dd3da3580424fed9f95d4d0e1e4b4b30d *accent-v0.25.0-x86_64-apple-darwin.tar.gz  \n";
        assert_eq!(
            lookup(text, "accent-v0.25.0-x86_64-apple-darwin.tar.gz").unwrap(),
            "98e505528fac8256353fd8cf27f4017dd3da3580424fed9f95d4d0e1e4b4b30d"
        );
    }

    #[test]
    fn a_missing_asset_is_an_error_not_a_skipped_check() {
        let err = lookup(SAMPLE, "accent-v0.25.0-x86_64-unknown-linux-gnu.tar.gz").unwrap_err();
        assert!(err.to_string().contains("no entry"));
    }

    #[test]
    fn rejects_a_digest_that_is_not_a_sha256() {
        let text = "deadbeef  accent-v0.25.0-x86_64-apple-darwin.tar.gz\n";
        assert!(lookup(text, "accent-v0.25.0-x86_64-apple-darwin.tar.gz").is_err());
    }

    /// A name that merely contains the asset name must not match it.
    #[test]
    fn matches_the_whole_name() {
        let text = "98e505528fac8256353fd8cf27f4017dd3da3580424fed9f95d4d0e1e4b4b30d  old-accent-v0.25.0-x86_64-apple-darwin.tar.gz\n";
        assert!(lookup(text, "accent-v0.25.0-x86_64-apple-darwin.tar.gz").is_err());
    }
}
