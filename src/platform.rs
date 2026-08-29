//! Host detection: which release asset belongs to this machine, and where
//! the binary goes.

use std::path::{Path, PathBuf};

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

/// Which C library a Linux host runs, and therefore which of the two Linux
/// builds it can start.
///
/// The glibc build is dynamically linked against glibc (2.28 floor) and
/// cannot start on a musl system, whose loader is a different file. The
/// musl build is fully static and starts anywhere, but swaps the allocator,
/// so it is not the default on glibc systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Libc {
    /// glibc: Debian, Ubuntu, RHEL and relatives, Amazon Linux, ...
    Gnu,
    /// musl: Alpine above all.
    Musl,
}

impl Libc {
    /// The libc component of the target triple (`gnu` / `musl`).
    pub fn as_str(self) -> &'static str {
        match self {
            Libc::Gnu => "gnu",
            Libc::Musl => "musl",
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
    /// The libc the target is built for; `None` outside Linux, where the
    /// question does not arise.
    pub libc: Option<Libc>,
}

impl Platform {
    /// e.g. `accent-v0.25.0-aarch64-apple-darwin.tar.gz`
    pub fn asset_name(&self, tag: &str) -> String {
        format!("accent-{tag}-{}.{}", self.target, self.archive.extension())
    }
}

/// The release target for the machine this is running on.
pub fn detect() -> Result<Platform> {
    let arch = std::env::consts::ARCH;
    for_host_with_libc(std::env::consts::OS, arch, detect_libc(arch))
}

/// Resolves an `(os, arch)` pair — the values of [`std::env::consts::OS`] and
/// [`std::env::consts::ARCH`] — to a published release target, taking the
/// glibc build on Linux.
///
/// [`detect`] is what the installer calls; it adds libc detection. This
/// form exists for callers and tests that only have the pair.
pub fn for_host(os: &str, arch: &str) -> Result<Platform> {
    for_host_with_libc(os, arch, Libc::Gnu)
}

/// Resolves an `(os, arch, libc)` triple to a published release target.
///
/// Eight targets exist in the dist repository: two Linux glibc, two Linux
/// musl (fully static, published from [`crate::release::OLDEST_MUSL`]
/// onward), two macOS, two Windows. `libc` only matters on Linux and is
/// ignored elsewhere.
pub fn for_host_with_libc(os: &str, arch: &str, libc: Libc) -> Result<Platform> {
    let arch_name = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("unsupported architecture: {other} — Accent CMS ships x86_64 and aarch64"),
    };

    let (os_name, triple, archive, libc) = match (os, arch_name, libc) {
        ("linux", "x86_64", Libc::Gnu) => {
            ("linux", "x86_64-unknown-linux-gnu", ArchiveKind::TarGz, Some(Libc::Gnu))
        }
        ("linux", "aarch64", Libc::Gnu) => {
            ("linux", "aarch64-unknown-linux-gnu", ArchiveKind::TarGz, Some(Libc::Gnu))
        }
        ("linux", "x86_64", Libc::Musl) => {
            ("linux", "x86_64-unknown-linux-musl", ArchiveKind::TarGz, Some(Libc::Musl))
        }
        ("linux", "aarch64", Libc::Musl) => {
            ("linux", "aarch64-unknown-linux-musl", ArchiveKind::TarGz, Some(Libc::Musl))
        }
        ("macos", "x86_64", _) => ("macos", "x86_64-apple-darwin", ArchiveKind::TarGz, None),
        ("macos", "aarch64", _) => ("macos", "aarch64-apple-darwin", ArchiveKind::TarGz, None),
        ("windows", "x86_64", _) => ("windows", "x86_64-pc-windows-msvc", ArchiveKind::Zip, None),
        ("windows", "aarch64", _) => ("windows", "aarch64-pc-windows-msvc", ArchiveKind::Zip, None),
        (other, _, _) => bail!(
            "unsupported operating system: {other} — Accent CMS ships Linux, macOS and Windows builds"
        ),
    };

    Ok(Platform {
        target: triple,
        archive,
        os: os_name,
        arch: arch_name,
        libc,
    })
}

/// The libc of this host, for the Linux build choice. Always [`Libc::Gnu`]
/// outside Linux, where the value is not used.
///
/// The musl dynamic loader lives at a fixed, architecture-named path, so
/// its presence is the most reliable marker; `ldd` identifying itself as
/// musl covers a layout that moved it. When neither says musl, glibc is
/// the answer -- the correct one on every glibc system, and the same
/// default the shell installer takes.
pub fn detect_libc(arch: &str) -> Libc {
    if std::env::consts::OS != "linux" {
        return Libc::Gnu;
    }
    let loader = Path::new("/lib")
        .join(format!("ld-musl-{arch}.so.1"))
        .exists();
    let ldd = std::process::Command::new("ldd")
        .arg("--version")
        .output()
        .ok()
        .map(|out| {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            text
        });
    libc_from_markers(loader, ldd.as_deref())
}

/// The decision behind [`detect_libc`], separated from the probes so it
/// can be tested on any host.
pub fn libc_from_markers(musl_loader_present: bool, ldd_version_output: Option<&str>) -> Libc {
    if musl_loader_present {
        return Libc::Musl;
    }
    match ldd_version_output {
        Some(text) if text.to_ascii_lowercase().contains("musl") => Libc::Musl,
        _ => Libc::Gnu,
    }
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
    fn linux_takes_the_musl_build_only_when_asked() {
        let musl = for_host_with_libc("linux", "x86_64", Libc::Musl).unwrap();
        assert_eq!(musl.target, "x86_64-unknown-linux-musl");
        assert_eq!(musl.libc, Some(Libc::Musl));
        let musl = for_host_with_libc("linux", "aarch64", Libc::Musl).unwrap();
        assert_eq!(musl.target, "aarch64-unknown-linux-musl");

        let gnu = for_host_with_libc("linux", "x86_64", Libc::Gnu).unwrap();
        assert_eq!(gnu.target, "x86_64-unknown-linux-gnu");
        assert_eq!(gnu.libc, Some(Libc::Gnu));
        assert_eq!(for_host("linux", "aarch64").unwrap().libc, Some(Libc::Gnu));

        // Outside Linux the libc is meaningless and changes nothing.
        let mac = for_host_with_libc("macos", "aarch64", Libc::Musl).unwrap();
        assert_eq!(mac.target, "aarch64-apple-darwin");
        assert_eq!(mac.libc, None);
        let win = for_host_with_libc("windows", "x86_64", Libc::Musl).unwrap();
        assert_eq!(win.target, "x86_64-pc-windows-msvc");
    }

    #[test]
    fn the_musl_loader_decides_and_ldd_is_the_fallback() {
        assert_eq!(libc_from_markers(true, None), Libc::Musl);
        assert_eq!(
            libc_from_markers(true, Some("ldd (GNU libc) 2.36")),
            Libc::Musl
        );
        assert_eq!(
            libc_from_markers(false, Some("musl libc (x86_64)\nVersion 1.2.5")),
            Libc::Musl
        );
        assert_eq!(
            libc_from_markers(false, Some("ldd (Debian GLIBC 2.36-9+deb12u10) 2.36")),
            Libc::Gnu
        );
        assert_eq!(
            libc_from_markers(false, Some("sh: ldd: not found")),
            Libc::Gnu
        );
        assert_eq!(libc_from_markers(false, None), Libc::Gnu);
    }

    #[test]
    fn musl_asset_names_follow_the_same_layout() {
        let musl = for_host_with_libc("linux", "aarch64", Libc::Musl).unwrap();
        assert_eq!(
            musl.asset_name("v0.26.0"),
            "accent-v0.26.0-aarch64-unknown-linux-musl.tar.gz"
        );
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
