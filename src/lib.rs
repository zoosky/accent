//! The moving parts behind `accentup`, the installer for [Accent CMS].
//!
//! Everything here mirrors what the shell installers published at
//! `AccentCMS/accent` (`install.sh`, `install.ps1`) do, with one deliberate
//! difference: signature verification is mandatory. The shell scripts fall
//! back to checksum-only verification when `gpg` is missing; this crate
//! embeds the release signing key and verifies in-process, so there is
//! nothing to fall back to.
//!
//! The release layout the whole crate is written against:
//!
//! ```text
//! https://github.com/AccentCMS/accent/releases/download/v0.25.0/
//!     accent-v0.25.0-aarch64-apple-darwin.tar.gz   # binary at archive root
//!     checksums-v0.25.0.txt                        # sha256sum format, all targets
//!     checksums-v0.25.0.txt.asc                    # detached OpenPGP signature
//! ```
//!
//! [Accent CMS]: https://accentcms.dev

//! This library exists so the installer's parts can be tested in isolation.
//! It is an implementation detail of the `accentup` binary, and its API is
//! not stable across releases.

#![warn(missing_docs)]

/// Unpacking release archives.
pub mod archive;
/// The published `checksums-<version>.txt`.
pub mod checksums;
/// Command line arguments.
pub mod cli;
/// The install, update, uninstall and which flows.
pub mod commands;
/// Placing, inspecting and removing the installed binary.
pub mod install;
/// HTTP downloads.
pub mod net;
/// Host detection and install locations.
pub mod platform;
/// Versions, repositories and download URLs.
pub mod release;
/// Console output.
pub mod ui;
/// Signature and checksum verification.
pub mod verify;
