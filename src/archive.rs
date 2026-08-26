//! Unpacking a release archive.
//!
//! Both archive kinds hold the `accent` binary at the root. A one-directory
//! nesting is tolerated as well, so a repackaged mirror does not break the
//! install.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::platform::{ArchiveKind, BIN_NAME};

/// Extracts `archive` below `workdir` and returns the path of the binary
/// inside it.
pub fn unpack(archive: &Path, workdir: &Path, kind: ArchiveKind) -> Result<PathBuf> {
    let out = workdir.join("unpacked");
    std::fs::create_dir_all(&out)?;

    match kind {
        ArchiveKind::TarGz => unpack_tar_gz(archive, &out)?,
        ArchiveKind::Zip => unpack_zip(archive, &out)?,
    }

    find_binary(&out).with_context(|| format!("no `{BIN_NAME}` inside {}", archive.display()))
}

fn unpack_tar_gz(archive: &Path, out: &Path) -> Result<()> {
    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    tar.set_preserve_permissions(true);

    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        // An absolute path or a `..` component in a malformed archive would
        // write outside the temporary directory.
        if path.is_absolute() || path.components().any(|c| c.as_os_str() == "..") {
            bail!("archive contains an unsafe path: {}", path.display());
        }
        entry
            .unpack_in(out)
            .with_context(|| format!("extracting {}", path.display()))?;
    }
    Ok(())
}

#[cfg(windows)]
fn unpack_zip(archive: &Path, out: &Path) -> Result<()> {
    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    // `extract` refuses entries that escape the destination directory.
    zip::ZipArchive::new(file)?.extract(out)?;
    Ok(())
}

#[cfg(not(windows))]
fn unpack_zip(_archive: &Path, _out: &Path) -> Result<()> {
    bail!("zip archives are only published for Windows targets")
}

/// Releases ship the binary at the archive root; tolerate one level of
/// nesting for repackaged mirrors.
fn find_binary(root: &Path) -> Result<PathBuf> {
    let direct = root.join(BIN_NAME);
    if direct.is_file() {
        return Ok(direct);
    }
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            let nested = path.join(BIN_NAME);
            if nested.is_file() {
                return Ok(nested);
            }
        }
    }
    bail!("binary not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a .tar.gz. Entry names are written into the header directly, so
    /// that a test can produce the sort of path a well-behaved archiver would
    /// refuse to emit.
    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, body) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Regular);
            let field = &mut header.as_ustar_mut().unwrap().name;
            let bytes = name.as_bytes();
            field[..bytes.len()].copy_from_slice(bytes);
            header.set_cksum();
            builder
                .append(&header, std::io::Cursor::new(*body))
                .unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap()
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn finds_a_binary_at_the_archive_root() {
        let dir = tempfile::tempdir().unwrap();
        let archive = write(
            dir.path(),
            "accent.tar.gz",
            &tar_gz(&[("accent", b"#!/bin/sh\n")]),
        );
        let binary = unpack(&archive, dir.path(), ArchiveKind::TarGz).unwrap();
        assert_eq!(binary.file_name().unwrap(), "accent");
        assert_eq!(std::fs::read(&binary).unwrap(), b"#!/bin/sh\n");
    }

    #[test]
    fn finds_a_binary_one_directory_down() {
        let dir = tempfile::tempdir().unwrap();
        let archive = write(
            dir.path(),
            "accent.tar.gz",
            &tar_gz(&[("accent-v0.25.0/accent", b"binary")]),
        );
        let binary = unpack(&archive, dir.path(), ArchiveKind::TarGz).unwrap();
        assert_eq!(binary.file_name().unwrap(), "accent");
    }

    #[test]
    fn refuses_a_traversing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let archive = write(
            dir.path(),
            "evil.tar.gz",
            &tar_gz(&[("../escaped", b"pwned"), ("accent", b"binary")]),
        );
        let err = unpack(&archive, dir.path(), ArchiveKind::TarGz).unwrap_err();
        assert!(err.to_string().contains("unsafe path"), "{err}");
        assert!(!dir.path().join("escaped").exists());
    }

    #[test]
    fn refuses_an_absolute_entry() {
        let dir = tempfile::tempdir().unwrap();
        let archive = write(
            dir.path(),
            "evil.tar.gz",
            &tar_gz(&[("/tmp/accent-escaped", b"pwned")]),
        );
        let err = unpack(&archive, dir.path(), ArchiveKind::TarGz).unwrap_err();
        assert!(err.to_string().contains("unsafe path"), "{err}");
    }

    #[test]
    fn reports_an_archive_without_the_binary() {
        let dir = tempfile::tempdir().unwrap();
        let archive = write(
            dir.path(),
            "wrong.tar.gz",
            &tar_gz(&[("README.md", b"nothing here")]),
        );
        assert!(unpack(&archive, dir.path(), ArchiveKind::TarGz).is_err());
    }
}
