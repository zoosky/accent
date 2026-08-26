//! The two checks that stand between a download and your PATH.
//!
//! Every release publishes `checksums-<version>.txt` covering all archives,
//! plus a detached OpenPGP signature over that file made with the Accent CMS
//! release signing key. So the chain is: pinned key → signed checksums file →
//! SHA-256 of the archive on disk.
//!
//! The key is compiled in from `release-signing-key.asc`. An installer that
//! fetched its own trust root over the network would verify nothing, and the
//! `--force`-style escape hatch other installers grow ("skip verification")
//! deliberately does not exist here.

use anyhow::{bail, Context, Result};
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::types::KeyDetails;

/// Published at `https://github.com/AccentCMS/accent/blob/main/release-signing-key.asc`.
pub const SIGNING_KEY: &str = include_str!("../release-signing-key.asc");

/// Fingerprint of [`SIGNING_KEY`], repeated here so that swapping the key file
/// alone cannot silently change the trust root: the two have to agree.
pub const SIGNING_KEY_FINGERPRINT: &str = "C0197617BAE752019693A17E95377BC8B27FF227";

/// Parses the embedded key and checks it is the one this build expects.
pub fn signing_key() -> Result<SignedPublicKey> {
    let (key, _headers) = SignedPublicKey::from_string(SIGNING_KEY)
        .context("the embedded release signing key is malformed")?;

    let actual = format!("{:x}", key.fingerprint());
    if !actual.eq_ignore_ascii_case(SIGNING_KEY_FINGERPRINT) {
        bail!(
            "embedded signing key {actual} is not the pinned key {}",
            SIGNING_KEY_FINGERPRINT.to_lowercase()
        );
    }
    Ok(key)
}

/// Verifies a detached, armoured signature over `data`.
pub fn signature(data: &[u8], armoured: &str) -> Result<()> {
    let key = signing_key()?;
    let (signature, _headers) =
        DetachedSignature::from_string(armoured).context("the .asc signature file is malformed")?;

    signature
        .verify(&key.primary_key, data)
        .context("the checksums file does not match the Accent CMS release signing key")
}

/// Compares a computed digest against the one recorded in the checksums file.
pub fn checksum(expected: &str, actual: &str) -> Result<()> {
    if !expected.eq_ignore_ascii_case(actual) {
        bail!("checksum mismatch\n  expected: {expected}\n  actual:   {actual}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKSUMS: &str = include_str!("../tests/fixtures/checksums-v0.25.0.txt");
    const SIGNATURE: &str = include_str!("../tests/fixtures/checksums-v0.25.0.txt.asc");

    #[test]
    fn the_embedded_key_is_the_pinned_one() {
        signing_key().unwrap();
    }

    #[test]
    fn accepts_a_real_release_signature() {
        signature(CHECKSUMS.as_bytes(), SIGNATURE).unwrap();
    }

    #[test]
    fn rejects_a_tampered_checksums_file() {
        // One digit of one digest flipped: the archive it names would still
        // install if only the checksum were checked against this file.
        let tampered = CHECKSUMS.replacen("98e5", "98e6", 1);
        assert_ne!(tampered, CHECKSUMS);
        assert!(signature(tampered.as_bytes(), SIGNATURE).is_err());
    }

    #[test]
    fn rejects_a_signature_that_is_not_a_signature() {
        assert!(signature(CHECKSUMS.as_bytes(), "not a signature").is_err());
    }

    #[test]
    fn checksum_comparison_ignores_case_only() {
        let digest = "98e505528fac8256353fd8cf27f4017dd3da3580424fed9f95d4d0e1e4b4b30d";
        checksum(digest, &digest.to_uppercase()).unwrap();
        assert!(checksum(digest, &digest.replace("98e5", "98e6")).is_err());
    }
}
