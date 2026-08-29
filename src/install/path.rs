//! PATH and shadowing warnings after an install.
//!
//! Placing the binary is not the same as the shell finding it: the install
//! directory may not be on PATH at all, or an older copy earlier in PATH may
//! win. Both are worth a warning the moment they can be detected, which is
//! right after the install, rather than the next time `accent --version`
//! surprises someone.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::platform::BIN_NAME;
use crate::ui::Ui;

use super::binary_in;

/// Warns when the install directory is not on PATH, with the incantation for
/// the user's shell.
pub fn check_path(dir: &Path, ui: &Ui) {
    if on_path(dir) {
        return;
    }

    ui.blank();
    ui.warn(format!("Note: {} is not on your PATH.", dir.display()));

    if cfg!(windows) {
        ui.warn("To add it, run:");
        ui.warn("");
        ui.warn("  $path = [Environment]::GetEnvironmentVariable('Path', 'User')");
        ui.warn(format!(
            "  [Environment]::SetEnvironmentVariable('Path', \"$path;{}\", 'User')",
            dir.display()
        ));
        ui.warn("");
        ui.warn("Then restart your terminal.");
        return;
    }

    let shell = std::env::var("SHELL").unwrap_or_default();
    let dir = dir.display();
    if shell.ends_with("fish") {
        ui.warn("Add it by appending this to ~/.config/fish/config.fish:");
        ui.warn(format!("  fish_add_path {dir}"));
    } else {
        let profile = if shell.ends_with("zsh") {
            "~/.zshrc"
        } else {
            "~/.bashrc"
        };
        ui.warn(format!("Add it by appending this to {profile}:"));
        ui.warn(format!("  export PATH=\"{dir}:$PATH\""));
    }
    ui.warn("Then restart your shell.");
}

/// Warns when `accent` resolves to a different binary than the one just
/// installed.
///
/// The shell resolves `accent` by PATH order, not by what this installer
/// wrote. A stale copy earlier in PATH — a `cargo install`ed one in
/// `~/.cargo/bin` is the usual case — silently wins, and `accent --version`
/// keeps reporting the old build while the user believes they upgraded.
pub fn check_shadowing(dir: &Path, ui: &Ui) {
    let installed = binary_in(dir);
    let Some(resolved) = resolve_on_path(BIN_NAME) else {
        return;
    };
    if same_file(&resolved, &installed) {
        return;
    }

    let version = Command::new(&resolved)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown version".to_string());

    ui.blank();
    ui.warn("Warning: 'accent' currently resolves to a different binary:");
    ui.warn(format!("  {} ({version})", resolved.display()));
    ui.warn(format!(
        "which shadows the up-to-date binary at {}.",
        installed.display()
    ));
    if cfg!(windows) {
        ui.warn(format!(
            "Remove the shadowing binary or move {} earlier in your PATH,",
            dir.display()
        ));
        ui.warn("then start a new terminal.");
    } else {
        ui.warn(format!(
            "Remove the shadowing binary or move {} earlier in your PATH,",
            dir.display()
        ));
        ui.warn("then run 'hash -r' (bash/zsh) or restart your shell.");
    }
}

/// Whether `dir` is one of the entries in `PATH`.
pub fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

/// First `name` on PATH, the way the shell would pick it.
pub fn resolve_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Compares two paths after resolving symlinks, so `~/.local/bin/accent` and a
/// symlink pointing at it are not reported as shadowing each other.
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
