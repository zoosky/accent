# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-26

First release.

### Added

- `accentup install`, `update`, `uninstall`, `which`. A bare `accentup`
  installs.
- Downloads the release archive for the host from `AccentCMS/accent`,
  resolving `latest` through the `releases/latest` redirect rather than the
  GitHub API, so runs in CI are not subject to the unauthenticated API rate
  limit.
- Mandatory verification: the detached OpenPGP signature over
  `checksums-<version>.txt` is checked against the release signing key
  compiled into the binary, then the archive is checked against that file.
  Neither step can be skipped, and the key is never fetched over the network.
- Installs to the same locations as the shell installers — `~/.local/bin` on
  Linux and macOS, `%LOCALAPPDATA%\accent` on Windows — via a copy-then-rename
  that never leaves a half-written binary on PATH.
- Warns when the install directory is not on `PATH`, and when `accent`
  resolves to a different binary that shadows the one just installed.
- `ACCENT_VERSION`, `ACCENT_FORCE`, `ACCENT_INSTALL_DIR` and `ACCENT_REPO`,
  with the same semantics as the shell installers; an explicit flag always
  wins over its variable.

[Unreleased]: https://github.com/zoosky/accent/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/zoosky/accent/releases/tag/v0.1.0
