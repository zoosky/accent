//! Release naming and version resolution.

use std::fmt;

use anyhow::{bail, Context, Result};

use crate::net::Http;

/// Repository the binaries are published from.
pub const DEFAULT_REPO: &str = "AccentCMS/accent";

/// Oldest release published to the dist repository. Older tags exist in the
/// private build repository but have no downloadable assets here.
pub const OLDEST_PUBLISHED: &str = "v0.22.0";

/// First release that ships the fully static Linux musl archives
/// (`*-unknown-linux-musl`). Earlier releases are glibc-only on Linux.
pub const OLDEST_MUSL: &str = "v0.26.0";

/// A release version, stored without its `v`.
///
/// Tags always carry the prefix, users type it either way, and
/// `accent --version` prints it without. Normalising once, here, keeps the
/// download URL and the up-to-date comparison from drifting apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(String);

impl Version {
    /// Accepts `0.25.0` and `v0.25.0` alike; rejects anything that is not
    /// a plausible tag.
    pub fn parse(raw: &str) -> Result<Self> {
        let bare = raw.trim().trim_start_matches('v');
        if bare.is_empty() {
            bail!("empty version");
        }
        if bare.contains('/') || bare.contains(char::is_whitespace) {
            bail!("not a release version: {raw}");
        }
        Ok(Self(bare.to_string()))
    }

    /// The git tag, e.g. `v0.25.0`.
    pub fn tag(&self) -> String {
        format!("v{}", self.0)
    }

    /// The bare number, e.g. `0.25.0`, as `accent --version` reports it.
    pub fn number(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// The `owner/name` the installer downloads from. Override with `ACCENT_REPO`
/// to point at a fork or a staging repository with the same asset layout.
#[derive(Debug, Clone)]
pub struct Repo(String);

impl Repo {
    /// Reads `ACCENT_REPO`, falling back to [`DEFAULT_REPO`].
    pub fn from_env() -> Self {
        match std::env::var("ACCENT_REPO") {
            Ok(slug) if !slug.trim().is_empty() => Self(slug.trim().to_string()),
            _ => Self(DEFAULT_REPO.to_string()),
        }
    }

    /// The `owner/name` this downloads from.
    pub fn slug(&self) -> &str {
        &self.0
    }

    /// Human-facing releases page, for error messages.
    pub fn releases_page(&self) -> String {
        format!("https://github.com/{}/releases", self.0)
    }

    /// The URL that redirects to the newest release.
    pub fn latest_release_url(&self) -> String {
        format!("https://github.com/{}/releases/latest", self.0)
    }

    /// Download URL for one asset of one release.
    pub fn download_url(&self, version: &Version, asset: &str) -> String {
        format!(
            "https://github.com/{}/releases/download/{}/{asset}",
            self.0,
            version.tag()
        )
    }

    /// Download URL of the checksums file.
    pub fn checksums_url(&self, version: &Version) -> String {
        self.download_url(version, &checksums_name(version))
    }

    /// Download URL of the detached signature over the checksums file.
    pub fn signature_url(&self, version: &Version) -> String {
        self.download_url(version, &format!("{}.asc", checksums_name(version)))
    }
}

/// Asset name of the checksums file, e.g. `checksums-v0.25.0.txt`.
pub fn checksums_name(version: &Version) -> String {
    format!("checksums-{}.txt", version.tag())
}

/// Asks GitHub which tag `releases/latest` points at.
///
/// Reads the redirect rather than the REST API, exactly as `install.sh` does:
/// the redirect is not subject to the 60-per-hour unauthenticated API rate
/// limit, which an installer run from CI will otherwise hit.
pub fn resolve_latest(http: &Http, repo: &Repo) -> Result<Version> {
    let url = repo.latest_release_url();
    let location = http
        .redirect_location(&url)
        .with_context(|| format!("asking {url} for the latest release"))?;

    let version = location
        .as_deref()
        .and_then(tag_from_location)
        .map(|tag| Version::parse(&tag))
        .transpose()?;

    version.with_context(|| {
        format!(
            "could not determine the latest version — there may be no published release yet.\n\
             Check {} or pass --version.",
            repo.releases_page()
        )
    })
}

/// Pulls `v0.25.0` out of `https://github.com/owner/repo/releases/tag/v0.25.0`.
pub fn tag_from_location(location: &str) -> Option<String> {
    let tag = location.rsplit_once("/tag/")?.1;
    let tag = tag.split(['?', '#']).next().unwrap_or_default().trim();
    if tag.is_empty() || tag.contains('/') {
        return None;
    }
    Some(tag.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_version_with_or_without_the_v() {
        assert_eq!(Version::parse("0.25.0").unwrap().tag(), "v0.25.0");
        assert_eq!(Version::parse("v0.25.0").unwrap().tag(), "v0.25.0");
        assert_eq!(Version::parse(" v0.25.0 ").unwrap().number(), "0.25.0");
        assert_eq!(
            Version::parse("0.25.0").unwrap(),
            Version::parse("v0.25.0").unwrap()
        );
    }

    #[test]
    fn rejects_junk_versions() {
        assert!(Version::parse("").is_err());
        assert!(Version::parse("v").is_err());
        assert!(Version::parse("0.25.0/../etc").is_err());
        assert!(Version::parse("0.25.0 rc1").is_err());
    }

    #[test]
    fn builds_the_published_urls() {
        let repo = Repo(DEFAULT_REPO.to_string());
        let v = Version::parse("0.25.0").unwrap();
        assert_eq!(
            repo.download_url(&v, "accent-v0.25.0-aarch64-apple-darwin.tar.gz"),
            "https://github.com/AccentCMS/accent/releases/download/v0.25.0/accent-v0.25.0-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(
            repo.checksums_url(&v),
            "https://github.com/AccentCMS/accent/releases/download/v0.25.0/checksums-v0.25.0.txt"
        );
        assert_eq!(
            repo.signature_url(&v),
            "https://github.com/AccentCMS/accent/releases/download/v0.25.0/checksums-v0.25.0.txt.asc"
        );
    }

    #[test]
    fn reads_the_tag_out_of_a_redirect() {
        assert_eq!(
            tag_from_location("https://github.com/AccentCMS/accent/releases/tag/v0.25.0")
                .as_deref(),
            Some("v0.25.0")
        );
        // No release yet: GitHub redirects to the releases index instead.
        assert_eq!(
            tag_from_location("https://github.com/AccentCMS/accent/releases"),
            None
        );
        assert_eq!(tag_from_location("").as_deref(), None);
    }
}
