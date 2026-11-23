//! Keys and sealed frames between Harlo and a phone.
//!
//! `NaCl`'s box (X25519 + XSalsa20-Poly1305), as the phone app does it. A frame
//! is base64 of `nonce (24) || ciphertext`. Each connection wraps its
//! messages as `{ c, s, m }`: the connection's ID, an increasing sequence
//! number per direction, and the message, so recorded frames can't be
//! replayed into another connection or out of order.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use crypto_box::aead::rand_core::RngCore as _;
use crypto_box::aead::{Aead as _, AeadCore as _, OsRng};
use crypto_box::{PublicKey, SalsaBox, SecretKey};
use serde_json::{Value, json};

/// Encodes bytes as standard base64.
#[must_use]
pub fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Decodes base64, standard or URL-safe, padded or not.
///
/// # Errors
///
/// When `text` isn't base64.
pub fn unb64(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    STANDARD
        .decode(text)
        .or_else(|_| URL_SAFE.decode(text))
        .or_else(|_| URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')))
        .map_err(|e| format!("not base64: {e}"))
}

/// Reads a 32-byte key from base64.
///
/// # Errors
///
/// When `text` isn't base64 of exactly 32 bytes.
pub fn key_bytes(text: &str) -> Result<[u8; 32], String> {
    <[u8; 32]>::try_from(unb64(text)?).map_err(|_bytes| "a key is 32 bytes".to_owned())
}

/// Returns `bytes` random bytes as unpadded URL-safe base64.
#[must_use]
pub fn random_id(bytes: usize) -> String {
    let mut buf = vec![0_u8; bytes];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Writes `text` to `path`, readable only by the user on Unix.
///
/// # Errors
///
/// When the folder can't be made or the file can't be written.
pub(crate) fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let partial = path.with_extension("partial");
    {
        use std::io::Write as _;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600)
        };
        let mut file = options.open(&partial)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    };
    std::fs::rename(partial, path)
}

/// Harlo's long-lived key pair for talking to paired phones.
pub struct Identity {
    secret: SecretKey,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The public half only: the secret key never goes into a log.
        f.debug_struct("Identity")
            .field("public", &self.public_b64())
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// Loads the saved key pair, or makes and saves a new one.
    ///
    /// # Errors
    ///
    /// When a new key pair can't be saved.
    pub fn load_or_create(path: &Path) -> std::io::Result<Self> {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(v) = serde_json::from_str::<Value>(&text)
            && let Some(secret) = v["secret"].as_str().and_then(|s| key_bytes(s).ok())
        {
            return Ok(Self {
                secret: SecretKey::from(secret),
            });
        }
        let secret = SecretKey::generate(&mut OsRng);
        let saved = json!({ "v": 1, "secret": b64(&secret.to_bytes()) });
        write_private(path, &saved.to_string())?;
        Ok(Self { secret })
    }

    /// Makes a key pair from a secret key's bytes.
    #[must_use]
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self {
            secret: SecretKey::from(secret),
        }
    }

    /// Makes a new random key pair, not saved anywhere.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            secret: SecretKey::generate(&mut OsRng),
        }
    }

    /// Returns the public key as base64.
    #[must_use]
    pub fn public_b64(&self) -> String {
        b64(self.secret.public_key().as_bytes())
    }

    /// Returns the channel with the holder of `other`, a public key.
    #[must_use]
    pub fn channel(&self, other: &[u8; 32]) -> Channel {
        Channel {
            cbox: SalsaBox::new(&PublicKey::from(*other), &self.secret),
        }
    }
}

/// Returns a short fingerprint of a public key, to compare by eye.
#[must_use]
pub fn fingerprint(public_b64: &str) -> String {
    let bytes = unb64(public_b64).unwrap_or_default();
    let hex = bytes.iter().take(8).fold(String::new(), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    });
    hex.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `NaCl`'s box between Harlo and one phone.
pub struct Channel {
    cbox: SalsaBox,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The box holds the shared key: nothing of it is shown.
        f.debug_struct("Channel").finish_non_exhaustive()
    }
}

impl Channel {
    /// Seals `plaintext` with a fresh nonce, as base64.
    #[must_use]
    pub fn seal(&self, plaintext: &[u8]) -> String {
        let nonce = SalsaBox::generate_nonce(&mut OsRng);
        // Encryption fails only past the cipher's 256 GiB limit; a phone
        // message is far smaller, so an empty frame (which won't open) is
        // never sent in practice.
        let sealed = self.cbox.encrypt(&nonce, plaintext).unwrap_or_default();
        let mut out = nonce.to_vec();
        out.extend(sealed);
        b64(&out)
    }

    /// Opens a frame from [`seal`](Self::seal).
    ///
    /// # Errors
    ///
    /// When the frame isn't base64, is too short, or doesn't open: another
    /// key, or tampered with.
    pub fn open(&self, frame: &str) -> Result<Vec<u8>, String> {
        let bytes = unb64(frame)?;
        if bytes.len() < 24 + 16 {
            return Err("a frame too short".to_owned());
        }
        let (nonce, sealed) = bytes.split_at(24);
        // (24 bytes, as split off above: `from_slice` doesn't panic.)
        let nonce = crypto_box::Nonce::from_slice(nonce);
        self.cbox
            .decrypt(nonce, sealed)
            .map_err(|_tampered| "a frame didn't open".to_owned())
    }
}

/// One connection's sealed messages, numbered in each direction.
#[derive(Debug)]
pub struct Sealed {
    channel: Channel,
    id: String,
    sent: u64,
    received: u64,
}

impl Sealed {
    /// Starts a connection `id` over `channel`.
    #[must_use]
    pub fn new(channel: Channel, id: String) -> Self {
        Self {
            channel,
            id,
            sent: 0,
            received: 0,
        }
    }

    /// Returns the connection's ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Seals the next message.
    pub fn seal(&mut self, message: &Value) -> String {
        self.sent += 1;
        let framed = json!({ "c": self.id, "s": self.sent, "m": message });
        self.channel.seal(framed.to_string().as_bytes())
    }

    /// Opens a frame for this connection that's newer than the last one.
    ///
    /// # Errors
    ///
    /// When the frame doesn't open, isn't JSON, is for another connection,
    /// or isn't newer than the last.
    pub fn open(&mut self, frame: &str) -> Result<Value, String> {
        let v: Value =
            serde_json::from_slice(&self.channel.open(frame)?).map_err(|e| e.to_string())?;
        if v["c"] != self.id.as_str() {
            return Err("a frame from another connection".to_owned());
        }
        let seq = v["s"].as_u64().unwrap_or(0);
        if seq <= self.received {
            return Err("a frame out of order".to_owned());
        }
        self.received = seq;
        Ok(v["m"].clone())
    }
}
