//! The signature on harlo's update manifest (`linux-release.json`).
//!
//! The manifest lists a release's packages with their SHA-256 sums, and
//! harlo checks each download against it. Without a signature, whoever
//! could change a release (a stolen GitHub token, a compromised workflow)
//! could publish a package and a matching sum, and harlo would install it.
//! So the release workflow signs the manifest with a private key that only
//! it holds (the `HARLO_UPDATE_SIGNING_KEY` repository secret), and harlo
//! checks the signature against the public key built in here before it
//! trusts anything in the manifest.
//!
//! Signing is in `crates/harlo-manifest-signer`, which never ships, so the
//! code that handles the private key isn't in harlo. Both sides build the
//! signed bytes with [`signing_payload`], so they can't drift apart.

// Rust guideline compliant 2026-10-09

use std::backtrace::{Backtrace, BacktraceStatus};
use std::fmt;

use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// The public half of the release-signing key.
///
/// The private half is the `HARLO_UPDATE_SIGNING_KEY` secret, given to the signing step alone and
/// never committed. To change the key: make a new pair
/// (`manifest-signer keygen`), ship a harlo with the new public key here,
/// and only then sign releases with the new private key (harlos still on
/// the old key can't take updates signed with the new one).
pub const PUBLIC_KEY_B64: &str = "88ss/kHgA3VDCaQ9VeOhPpLFf8fYpjtZ0cMo3rEtjow=";

/// What a manifest signature covers, so it can't stand for anything else.
const DOMAIN: &[u8] = b"harlo-linux-release-manifest\x00v1\x00";

/// One package of a release: its file name, SHA-256 (hex) and address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
    pub download_url: String,
}

/// The bytes a manifest's signature covers.
///
/// The domain, then version, tag and repository, then each asset (sorted by name, so the order the
/// files were listed in doesn't matter), every field ending in a NUL. NULs
/// can't occur in these fields, so no two different manifests give the same
/// bytes ("1" + "2x" vs "12" + "x").
pub fn signing_payload(version: &str, tag: &str, repository: &str, assets: &[Asset]) -> Vec<u8> {
    let mut buf = DOMAIN.to_vec();
    let mut field = |s: &str| {
        buf.extend_from_slice(s.as_bytes());
        buf.push(0);
    };
    field(version);
    field(tag);
    field(repository);
    let mut sorted: Vec<&Asset> = assets.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for a in sorted {
        field(&a.name);
        field(&a.sha256);
        field(&a.download_url);
    }
    buf
}

/// Why a manifest signature wasn't accepted.
///
/// A key or signature that can't be read ([`is_malformed`](Self::is_malformed))
/// is told apart from a well-formed signature that doesn't match
/// ([`is_mismatch`](Self::is_mismatch)). The message names which, followed
/// by the upstream cause when there is one; the cause is part of the
/// message, so error chains print it once.
#[derive(Debug)]
pub struct VerifyError {
    kind: Kind,
    cause: Option<String>,
    backtrace: Backtrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    KeyNotBase64,
    KeyLength,
    KeyNotEd25519,
    SignatureNotBase64,
    SignatureLength,
    Mismatch,
}

impl VerifyError {
    fn new(kind: Kind, cause: Option<&dyn fmt::Display>) -> Self {
        Self { kind, cause: cause.map(ToString::to_string), backtrace: Backtrace::capture() }
    }

    /// The signature is well formed but wasn't made by the key over these bytes.
    #[must_use]
    pub fn is_mismatch(&self) -> bool {
        self.kind == Kind::Mismatch
    }

    /// The key or the signature isn't base64, has the wrong length, or isn't an Ed25519 key.
    #[must_use]
    pub fn is_malformed(&self) -> bool {
        !self.is_mismatch()
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind {
            Kind::KeyNotBase64 => "the public key isn't base64",
            Kind::KeyLength => "the public key isn't 32 bytes",
            Kind::KeyNotEd25519 => "the public key isn't an Ed25519 key",
            Kind::SignatureNotBase64 => "the signature isn't base64",
            Kind::SignatureLength => "the signature isn't 64 bytes",
            Kind::Mismatch => "the signature doesn't match",
        })?;
        if let Some(cause) = &self.cause {
            write!(f, ": {cause}")?;
        }
        if self.backtrace.status() == BacktraceStatus::Captured {
            write!(f, "\n{}", self.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for VerifyError {}

fn public_key(b64: &str) -> Result<VerifyingKey, VerifyError> {
    let bytes = STANDARD.decode(b64.trim()).map_err(|e| VerifyError::new(Kind::KeyNotBase64, Some(&e)))?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|b: Vec<u8>| VerifyError::new(Kind::KeyLength, Some(&format_args!("{} bytes", b.len()))))?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| VerifyError::new(Kind::KeyNotEd25519, Some(&e)))
}

/// Checks `signature_b64` (an Ed25519 signature, base64) over `payload`
/// with `public_key_b64`.
///
/// # Errors
///
/// A [`VerifyError`] when the key or the signature can't be read, or the
/// signature doesn't match `payload`.
pub fn verify(payload: &[u8], signature_b64: &str, public_key_b64: &str) -> Result<(), VerifyError> {
    let key = public_key(public_key_b64)?;
    let bytes = STANDARD.decode(signature_b64.trim()).map_err(|e| VerifyError::new(Kind::SignatureNotBase64, Some(&e)))?;
    let bytes: [u8; 64] = bytes.try_into().map_err(|b: Vec<u8>| VerifyError::new(Kind::SignatureLength, Some(&format_args!("{} bytes", b.len()))))?;
    key.verify(payload, &Signature::from_bytes(&bytes)).map_err(|e| VerifyError::new(Kind::Mismatch, Some(&e)))
}

/// Harlo's public key (base64), or `HARLO_UPDATE_PUBLIC_KEY` when set.
///
/// Tests sign with a throwaway key; whoever can set harlo's environment
/// can already point it at another manifest.
pub fn harlo_public_key() -> String {
    std::env::var("HARLO_UPDATE_PUBLIC_KEY").ok().filter(|k| !k.trim().is_empty()).unwrap_or_else(|| PUBLIC_KEY_B64.to_owned())
}

/// [`verify`] with [`harlo_public_key`].
///
/// # Errors
///
/// As [`verify`]: when the key or signature can't be read, or the
/// signature doesn't match `payload`.
pub fn verify_with_harlo_key(payload: &[u8], signature_b64: &str) -> Result<(), VerifyError> {
    verify(payload, signature_b64, &harlo_public_key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn asset(name: &str) -> Asset {
        Asset { name: name.into(), sha256: format!("sha-{name}"), download_url: format!("https://x/{name}") }
    }

    /// A throwaway key, never the release key.
    fn signed(payload: &[u8]) -> (String, String) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        (STANDARD.encode(key.sign(payload).to_bytes()), STANDARD.encode(key.verifying_key().to_bytes()))
    }

    #[test]
    fn a_signature_checks_out_and_nothing_else_does() {
        let assets = [asset("harlo_1.2.0-1_amd64.deb"), asset("harlo-1.2.0-x86_64.AppImage")];
        let payload = signing_payload("1.2.0", "v1.2.0", "ArcticLatent/harlo", &assets);
        let (sig, public) = signed(&payload);
        verify(&payload, &sig, &public).unwrap();
        // Another file behind the same name: refused.
        let mut tampered = assets.clone();
        tampered[0].sha256 = "evil".into();
        let e = verify(&signing_payload("1.2.0", "v1.2.0", "ArcticLatent/harlo", &tampered), &sig, &public).unwrap_err();
        assert!(e.is_mismatch(), "{e}");
        // Another key: refused.
        let other = STANDARD.encode(SigningKey::from_bytes(&[9u8; 32]).verifying_key().to_bytes());
        assert!(verify(&payload, &sig, &other).unwrap_err().is_mismatch());
        // Not base64, the wrong length: refused, not a panic, and said so.
        let e = verify(&payload, "not base64!", &public).unwrap_err();
        assert!(e.is_malformed() && e.to_string().starts_with("the signature isn't base64: "), "{e}");
        let e = verify(&payload, &STANDARD.encode([1u8; 10]), &public).unwrap_err();
        assert!(e.to_string().starts_with("the signature isn't 64 bytes: 10 bytes"), "{e}");
    }

    #[test]
    fn the_order_of_files_does_not_matter_but_field_boundaries_do() {
        let (a, b) = (asset("a.deb"), asset("b.rpm"));
        assert_eq!(signing_payload("1", "v1", "r", &[a.clone(), b.clone()]), signing_payload("1", "v1", "r", &[b, a]));
        assert_ne!(signing_payload("1", "2x", "r", &[]), signing_payload("12", "x", "r", &[]));
    }

    #[test]
    fn the_built_in_key_is_a_key() {
        public_key(PUBLIC_KEY_B64).expect("PUBLIC_KEY_B64 is a valid Ed25519 public key");
    }
}
