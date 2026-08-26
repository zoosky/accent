//! Host detection: which release asset belongs to this machine, and where
//! the binary goes.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};

/// File name of the installed product (not of this installer).
#[cfg(windows)]
pub const BIN_NAME: &str = "accent.exe";
/// File name of the installed product (not of this installer).
#[cfg(not(windows))]
pub const BIN_NAME: &str = "accent";

/// How a release archive for a target is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    /// Linux and macOS releases.
    TarGz,
    /// Windows releases.
    Zip,
}

impl ArchiveKind {
    /// The extension used in asset names, without the leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            ArchiveKind::TarGz => "tar.gz",
            ArchiveKind::Zip => "zip",
        }
    }
}

/// The release build that belongs to this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    /// Rust target triple, as it appears in the release asset names.
    pub target: &'static str,
    /// How that target's archive is packed.
    pub archive: ArchiveKind,
    /// Human-readable OS name, for the "Detected platform" line.
    pub os: &'static str,
    /// Human-readable architecture, for the same line.
    pub arch: &'static str,
}

impl Platform {
    /// e.g. `accent-v0.25.0-aarch64-apple-darwin.tar.gz`
    pub fn asset_name(&self, tag: &str) -> String {
        format!("accent-{tag}-{}.{}", self.target, self.archive.extension())
    }
}

/// The release target for the machine this is running on.
pub fn detect() -> Result<Platform> {
    for_host(std::env::consts::OS, std::env::consts::ARCH)
}

/// Resolves an `(os, arch)` pair — the values of [`std::env::consts::OS`] and
/// [`std::env::consts::ARCH`] — to a published release target.
///
/// Only the six targets listed in the dist repository exist; notably there is
/// no musl build, so a musl host has to build from source.
pub fn for_host(os: &str, arch: &str) -> Result<Platform> {
    let arch_name = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("unsupported architecture: {other} — Accent CMS ships x86_64 and aarch64"),
    };

    let (os_name, triple, archive) = match (os, arch_name) {
        ("linux", "x86_64") => ("linux", "x86_64-unknown-linux-gnu", ArchiveKind::TarGz),
        ("linux", "aarch64") => ("linux", "aarch64-unknown-linux-gnu", ArchiveKind::TarGz),
        ("macos", "x86_64") => ("macos", "x86_64-apple-darwin", ArchiveKind::TarGz),
        ("macos", "aarch64") => ("macos", "aarch64-apple-darwin", ArchiveKind::TarGz),
        ("windows", "x86_64") => ("windows", "x86_64-pc-windows-msvc", ArchiveKind::Zip),
        ("windows", "aarch64") => ("windows", "aarch64-pc-windows-msvc", ArchiveKind::Zip),
        (other, _) => bail!(
            "unsupported operating system: {other} — Accent CMS ships Linux, macOS and Windows builds"
        ),
    };

    Ok(Platform {
        target: triple,
        archive,
        os: os_name,
        arch: arch_name,
    })
}

/// Where the `accent` binary lands, absent `--dir` or `ACCENT_INSTALL_DIR`.
///
/// The same directories the shell installers use: `~/.local/bin` on Unix,
/// `%LOCALAPPDATA%\accent` on Windows. Picking anything else would install a
/// second copy next to the one those scripts manage.
pub fn default_install_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let local = dirs::data_local_dir().context("could not determine %LOCALAPPDATA%")?;
        Ok(local.join("accent"))
    }
    #[cfg(not(windows))]
    {
        let home = dirs::home_dir().context("could not determine the home directory")?;
        Ok(home.join(".local").join("bin"))
    }
}

/// User-level state written by the product itself: the licence key under the
/// config directory, and the development TLS certificate under the cache
/// directory. Only `uninstall --purge` touches these.
pub fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(config) = dirs::config_dir() {
        dirs.push(config.join("accent"));
    }
    if let Some(cache) = dirs::cache_dir() {
        dirs.push(cache.join("accent"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_published_target() {
        let cases = [
            (
                "linux",
                "x86_64",
                "x86_64-unknown-linux-gnu",
                ArchiveKind::TarGz,
            ),
            (
                "linux",
                "aarch64",
                "aarch64-unknown-linux-gnu",
                ArchiveKind::TarGz,
            ),
            ("macos", "x86_64", "x86_64-apple-darwin", ArchiveKind::TarGz),
            (
                "macos",
                "aarch64",
                "aarch64-apple-darwin",
                ArchiveKind::TarGz,
            ),
            (
                "windows",
                "x86_64",
                "x86_64-pc-windows-msvc",
                ArchiveKind::Zip,
            ),
            (
                "windows",
                "aarch64",
                "aarch64-pc-windows-msvc",
                ArchiveKind::Zip,
            ),
        ];
        for (os, arch, triple, archive) in cases {
            let plat = for_host(os, arch).unwrap();
            assert_eq!(plat.target, triple);
            assert_eq!(plat.archive, archive);
        }
    }

    #[test]
    fn rejects_targets_that_are_not_published() {
        assert!(for_host("linux", "riscv64").is_err());
        assert!(for_host("freebsd", "x86_64").is_err());
    }

    #[test]
    fn asset_names_match_the_published_layout() {
        let unix = for_host("macos", "aarch64").unwrap();
        assert_eq!(
            unix.asset_name("v0.25.0"),
            "accent-v0.25.0-aarch64-apple-darwin.tar.gz"
        );
        let win = for_host("windows", "x86_64").unwrap();
        assert_eq!(
            win.asset_name("v0.25.0"),
            "accent-v0.25.0-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn the_host_running_these_tests_is_supported() {
        detect().unwrap();
    }
}
