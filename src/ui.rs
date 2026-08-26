//! Console output.
//!
//! Everything the installer says about its own progress goes to stderr, so
//! that the one thing a caller might want to capture — the path printed by
//! `accentup which` — is the only thing on stdout.

use std::io::IsTerminal;

pub struct Ui {
    quiet: bool,
}

impl Ui {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }

    /// A step the user asked for: what is being downloaded, verified, written.
    pub fn say(&self, msg: impl AsRef<str>) {
        if !self.quiet {
            eprintln!("{}", msg.as_ref());
        }
    }

    pub fn blank(&self) {
        if !self.quiet {
            eprintln!();
        }
    }

    /// Something the user needs to act on. Printed even under `--quiet`,
    /// because a silent warning is the same as no warning.
    pub fn warn(&self, msg: impl AsRef<str>) {
        eprintln!("{}", msg.as_ref());
    }

    /// Whether a progress bar would be read by a human rather than scrolled
    /// past in a CI log. Mirrors the `[ -t 2 ]` check in `install.sh`.
    pub fn wants_progress(&self) -> bool {
        !self.quiet && std::io::stderr().is_terminal()
    }
}
