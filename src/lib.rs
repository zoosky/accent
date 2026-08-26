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

pub mod archive;
pub mod checksums;
pub mod cli;
pub mod commands;
pub mod install;
pub mod net;
pub mod platform;
pub mod release;
pub mod ui;
pub mod verify;
