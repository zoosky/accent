//! Blocking HTTP, sized for one download and two small metadata fetches.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

use crate::ui::Ui;

const USER_AGENT: &str = concat!("accentup/", env!("CARGO_PKG_VERSION"));

/// Ceiling on a downloaded artifact, so a hostile or misconfigured mirror
/// cannot fill the disk. Release archives are ~30 MB.
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

/// Ceiling on the metadata fetches (checksums, signature), which are ~1 KB.
const MAX_TEXT_BYTES: u64 = 1024 * 1024;

/// The HTTP client, configured once per run.
pub struct Http {
    agent: ureq::Agent,
    /// Same configuration with redirects switched off, for reading the
    /// `releases/latest` Location header.
    no_redirect: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    /// Builds the client, with timeouts suited to a large download.
    pub fn new() -> Self {
        let base = || {
            ureq::Agent::config_builder()
                .user_agent(USER_AGENT)
                .timeout_connect(Some(Duration::from_secs(15)))
                .timeout_recv_body(Some(Duration::from_secs(300)))
        };
        Self {
            agent: base().build().new_agent(),
            no_redirect: base()
                .max_redirects(0)
                .max_redirects_will_error(false)
                .build()
                .new_agent(),
        }
    }

    /// Fetches a small text resource (checksums file, signature).
    pub fn get_text(&self, url: &str) -> Result<String> {
        self.agent
            .get(url)
            .call()
            .map_err(|err| describe(url, err))?
            .body_mut()
            .with_config()
            .limit(MAX_TEXT_BYTES)
            .read_to_string()
            .with_context(|| format!("reading the response from {url}"))
    }

    /// Returns the `Location` header of the first response, without following
    /// it. `None` when the server answered without a redirect.
    pub fn redirect_location(&self, url: &str) -> Result<Option<String>> {
        let response = self
            .no_redirect
            .get(url)
            .call()
            .map_err(|err| describe(url, err))?;
        Ok(response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string))
    }

    /// Streams `url` into `dest`, returning the hex SHA-256 of the bytes
    /// written. The artifact is never held in memory in full.
    pub fn download(&self, url: &str, dest: &Path, ui: &Ui) -> Result<String> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|err| describe(url, err))?;

        let total = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);

        if total > MAX_ARTIFACT_BYTES {
            bail!("{url} is {total} bytes, past the {MAX_ARTIFACT_BYTES} byte limit");
        }

        let bar = (ui.wants_progress() && total > 0).then(|| {
            let bar = ProgressBar::new(total);
            bar.set_style(
                ProgressStyle::with_template(
                    "  {bytes:>10}/{total_bytes:<10} [{bar:28}] {bytes_per_sec:>11} {eta:>4}",
                )
                .expect("progress template is valid")
                .progress_chars("=> "),
            );
            bar
        });

        let file = File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
        let mut writer = BufWriter::new(file);
        let mut reader = response.body_mut().as_reader();
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut written: u64 = 0;

        loop {
            let n = reader.read(&mut buf).context("connection interrupted")?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > MAX_ARTIFACT_BYTES {
                bail!("download from {url} passed the {MAX_ARTIFACT_BYTES} byte limit");
            }
            hasher.update(&buf[..n]);
            writer.write_all(&buf[..n])?;
            if let Some(bar) = &bar {
                bar.set_position(written);
            }
        }
        writer.flush()?;
        drop(writer);
        if let Some(bar) = bar {
            bar.finish_and_clear();
        }

        if total > 0 && written != total {
            bail!("truncated download: expected {total} bytes from {url}, got {written}");
        }

        Ok(hex::encode(hasher.finalize()))
    }
}

/// Turns a transport error into something that names the URL, keeping a 404
/// distinguishable from a network failure — the two need different fixes.
fn describe(url: &str, err: ureq::Error) -> anyhow::Error {
    match err {
        ureq::Error::StatusCode(404) => anyhow::anyhow!("not found (404): {url}"),
        ureq::Error::StatusCode(code) => anyhow::anyhow!("HTTP {code} from {url}"),
        other => anyhow::Error::new(other).context(format!("GET {url}")),
    }
}
