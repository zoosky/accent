# accent

Installer and updater for [Accent CMS](https://accentcms.dev).

The crate builds one binary, **`accentup`**. It downloads the release archive
for your platform from [`AccentCMS/accent`][dist], verifies it, and places the
`accent` binary where the official install scripts put it. It is the same job
`install.sh` and `install.ps1` do at that repository, as a signed,
cross-platform binary instead of a shell script.

The installer is called `accentup`, not `accent`, so that `cargo install
accent` cannot drop a binary in `~/.cargo/bin` that shadows the product on
your PATH.

[dist]: https://github.com/AccentCMS/accent

## Install

```sh
cargo install accent
accentup install
```

Without a Rust toolchain, use the shell installers from the dist repository:

```sh
curl -fsSL https://raw.githubusercontent.com/AccentCMS/accent/main/install.sh | sh
```

```powershell
irm https://raw.githubusercontent.com/AccentCMS/accent/main/install.ps1 | iex
```

## Usage

```
accentup                                  # same as `accentup install`
accentup install [--version 0.25.0] [--force] [--dry-run]
accentup update  [--force]
accentup uninstall [--purge]
accentup which
```

Global flags: `--dir PATH` to choose the install directory, `--quiet` to print
only warnings and errors.

Versions are accepted with or without the `v` (`0.25.0` and `v0.25.0` are the
same release). Only `v0.22.0` and later are published.

| Variable | Effect |
| --- | --- |
| `ACCENT_VERSION` | Version to install (same as `--version`) |
| `ACCENT_FORCE` | Any value but empty or `0` means `--force` |
| `ACCENT_INSTALL_DIR` | Where the binary goes (same as `--dir`) |
| `ACCENT_REPO` | Repository to download from, default `AccentCMS/accent` |

An explicit flag always wins over its variable.

### Where things go

| | Install directory |
| --- | --- |
| Linux, macOS | `~/.local/bin` |
| Windows | `%LOCALAPPDATA%\accent` |

These are the directories the shell installers use; matching them keeps
`accentup` and `install.sh` managing one installation rather than two.

`uninstall` removes the binary. `uninstall --purge` also deletes the
user-level state the product writes — the licence key in
`~/.config/accent/` and the cached development certificate — but never the
install directory itself, which is shared with other tools.

After installing, `accentup` warns if the install directory is not on your
PATH, and if `accent` resolves to some other binary that shadows the one it
just wrote.

## Verification

Every release publishes `checksums-<version>.txt` covering all archives, and a
detached OpenPGP signature `checksums-<version>.txt.asc` made with the Accent
CMS release signing key. `accentup` checks both, in this order:

1. the signature over the checksums file, against the key compiled into the
   binary from [`release-signing-key.asc`](release-signing-key.asc);
2. the SHA-256 of the downloaded archive, against that checksums file.

Neither step is optional and there is no flag to skip them. Verification runs
in-process, so unlike `install.sh` — which falls back to checksum-only when
`gpg` is not installed — a missing tool cannot weaken it. The trust root is
never fetched over the network.

Key fingerprint, pinned in `src/verify.rs` and asserted against the embedded
key by the test suite:

```
C019 7617 BAE7 5201 9693  A17E 9537 7BC8 B27F F227
```

To verify by hand:

```sh
gpg --import release-signing-key.asc
gpg --verify checksums-v0.25.0.txt.asc checksums-v0.25.0.txt
sha256sum -c --ignore-missing checksums-v0.25.0.txt
```

## Supported targets

`x86_64` and `aarch64`, on Linux (gnu), macOS and Windows (msvc). There is no
musl build; on a musl host, build Accent CMS from source.

The Linux binaries need glibc 2.28 or newer -- Debian 10, Ubuntu 20.04,
RHEL 8 (and AlmaLinux and Rocky Linux 8), Amazon Linux 2023, and anything
after them. `accentup` runs the downloaded binary once (`accent --version`)
before putting it in place; a binary this system cannot start is not
installed, the previous installation is left as it was, and the error names
the glibc the release needs and the one the system has.

## Development

```sh
cargo test              # offline: unit tests and CLI tests
cargo test -- --ignored # also downloads and installs a real release
cargo clippy --all-targets -- -D warnings
```

The layout follows the install flow: `platform` (which asset), `release`
(which version, and the URLs), `net` (fetch it), `checksums` and `verify`
(is it genuine), `archive` (unpack it), `install` (run it once, place it, PATH
warnings),
`commands` (the flow itself), `cli` and `main` (arguments).

To test against a fork or a staging repository with the same asset layout,
set `ACCENT_REPO=owner/name`. Swapping the trust root means replacing
`release-signing-key.asc` *and* the fingerprint in `src/verify.rs`; the tests
fail if the two disagree.

## Licence

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT licence ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this crate by you, as defined in the Apache-2.0
licence, shall be dual-licensed as above, without any additional terms or
conditions.

This covers the installer only. The Accent CMS binaries it downloads are
proprietary and carry [their own licence](https://github.com/AccentCMS/accent/blob/main/LICENSE).
