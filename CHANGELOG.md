# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Alpine Linux support. On a musl-based system `accentup` installs the fully
  static `*-unknown-linux-musl` build (published from Accent CMS v0.26.0
  onward) instead of the glibc build that cannot start there; glibc systems
  keep the gnu build. Detection probes the musl loader at
  `/lib/ld-musl-<arch>.so.1`, then `ldd`, and defaults to glibc. A failed
  musl download names the first release that ships one.

## [0.1.2] - 2026-08-29

### Fixed

- The downloaded binary is run once (`accent --version`) before it is put in
  place. A release that verifies but cannot start on this system -- the
  v0.25.0 Linux binaries needed glibc 2.39 and the loader refused them on
  Debian 12 and Ubuntu 22.04 -- used to install under "Installation
  complete!"; it now fails with the loader's message, the glibc the release
  needs and the one the system has, and leaves any earlier installation
  untouched. The README states the Linux floor (glibc 2.28).

## [0.1.1] - 2026-08-28

### Fixed

- The archive unsafe-path guard used `is_absolute()`, which on Windows is
  false for a rooted entry like `/tmp/x` (no drive) and a prefixed one like
  `C:x` (no root). It now refuses any prefix, root, or `..` component on
  every platform.
- On Windows, updating in place left the previous binary behind as
  `.accent.exe.old`. It is removed once the new binary is in place; if the
  old one is still running, the next install clears it.
- The test suite is Windows-correct: it no longer hardcodes `accent` where
  the binary is `accent.exe`, and the signed fixtures are protected from
  line-ending conversion on checkout. CI on `windows-latest` had been red
  since the first release and now runs the live install there.

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

[Unreleased]: https://github.com/zoosky/accent/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/zoosky/accent/releases/tag/v0.1.1
[0.1.0]: https://github.com/zoosky/accent/releases/tag/v0.1.0
