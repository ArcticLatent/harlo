//! Signs and checks harlo's update manifest, `linux-release.json`. Release
//! tooling only (`packaging/release-manifests.sh`, run by the release
//! workflow): harlo itself only checks signatures (`harlo-update-signing`),
//! so the code that reads the private key never ships.
//!
//!   manifest-signer keygen
//!       A new key pair, both halves in base64. Once: the private half
//!       becomes the HARLO_UPDATE_SIGNING_KEY secret, the public half goes
//!       in harlo-update-signing's PUBLIC_KEY_B64.
//!   manifest-signer public-key
//!       The public half of HARLO_UPDATE_SIGNING_KEY.
//!   manifest-signer sign <manifest>
//!       Signs the manifest with HARLO_UPDATE_SIGNING_KEY and adds its
//!       `signature`.
//!   manifest-signer verify <manifest> [--public-key <base64>]
//!       Checks its signature against harlo's key (or the one given); fails
//!       otherwise, so a release signed with the wrong key stops there.

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use harlo_update_signing::{Asset, PUBLIC_KEY_B64, signing_payload, verify};
use serde::{Deserialize, Serialize};
use std::path::Path;

const SIGNING_KEY_ENV: &str = "HARLO_UPDATE_SIGNING_KEY";

#[derive(Serialize, Deserialize)]
struct ManifestAsset {
    name: String,
    sha256: String,
    download_url: String,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: String,
    tag: String,
    repository: String,
    assets: Vec<ManifestAsset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
}

impl Manifest {
    fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("{} isn't a release manifest", path.display()))
    }

    fn payload(&self) -> Vec<u8> {
        let assets: Vec<Asset> = self
            .assets
            .iter()
            .map(|a| Asset { name: a.name.clone(), sha256: a.sha256.clone(), download_url: a.download_url.clone() })
            .collect();
        signing_payload(&self.version, &self.tag, &self.repository, &assets)
    }
}

fn signing_key() -> Result<SigningKey> {
    let b64 = std::env::var(SIGNING_KEY_ENV).with_context(|| format!("{SIGNING_KEY_ENV} isn't set"))?;
    let bytes = STANDARD.decode(b64.trim()).with_context(|| format!("{SIGNING_KEY_ENV} isn't base64"))?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("{SIGNING_KEY_ENV} isn't 32 bytes"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen"] => {
            let mut seed = [0u8; 32];
            getrandom::getrandom(&mut seed).map_err(|e| anyhow::anyhow!("no random source: {e}"))?;
            let key = SigningKey::from_bytes(&seed);
            println!("private (the {SIGNING_KEY_ENV} secret): {}", STANDARD.encode(key.to_bytes()));
            println!("public (PUBLIC_KEY_B64): {}", STANDARD.encode(key.verifying_key().to_bytes()));
        }
        ["public-key"] => println!("{}", STANDARD.encode(signing_key()?.verifying_key().to_bytes())),
        ["sign", path] => {
            let path = Path::new(path);
            let mut manifest = Manifest::read(path)?;
            manifest.signature = None;
            let signature = signing_key()?.sign(&manifest.payload());
            manifest.signature = Some(STANDARD.encode(signature.to_bytes()));
            std::fs::write(path, serde_json::to_string_pretty(&manifest)? + "\n").with_context(|| format!("writing {}", path.display()))?;
            println!("signed {}", path.display());
        }
        ["verify", path, rest @ ..] => {
            let public = match rest {
                [] => PUBLIC_KEY_B64.to_owned(),
                ["--public-key", key] => (*key).to_owned(),
                _ => bail!("usage: manifest-signer verify <manifest> [--public-key <base64>]"),
            };
            let manifest = Manifest::read(Path::new(path))?;
            let signature = manifest.signature.as_deref().context("the manifest has no signature")?;
            verify(&manifest.payload(), signature, &public).context("the manifest's signature doesn't match harlo's public key")?;
            println!("{path}: signature checks out ({} files, version {})", manifest.assets.len(), manifest.version);
        }
        _ => bail!("usage: manifest-signer keygen | public-key | sign <manifest> | verify <manifest> [--public-key <base64>]"),
    }
    Ok(())
}
