//! Harlo's updates against a stand-in release server: the signed manifest,
//! the download checked against it, and an AppImage replaced.

// Rust guideline compliant 2026-10-09

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use futures_util::StreamExt as _;
use harlo_core::download::{Download, Fetch};
use harlo_core::updates::{self, Asset, InstallKind, Release};
use sha2::{Digest as _, Sha256};

/// A throwaway key, never the release key.
const KEY: [u8; 32] = [5; 32];

/// Its public half, as the app would hold it.
fn public() -> String {
    STANDARD.encode(SigningKey::from_bytes(&KEY).verifying_key().to_bytes())
}

/// A manifest as the release workflow writes it, signed with `key`.
fn manifest(version: &str, repository: &str, key: &[u8; 32], assets: &[Asset]) -> String {
    let tag = format!("v{version}");
    let payload = harlo_update_signing::signing_payload(version, &tag, repository, assets);
    let signature = STANDARD.encode(SigningKey::from_bytes(key).sign(&payload).to_bytes());
    serde_json::json!({
        "version": version, "tag": tag, "repository": repository, "signature": signature,
        "assets": assets.iter().map(|a| serde_json::json!({
            "name": a.name, "sha256": a.sha256, "download_url": a.download_url,
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

/// The four packages of a release, at `base`.
fn assets(version: &str, base: &str, appimage_sum: &str) -> Vec<Asset> {
    [
        format!("harlo_{version}-1_amd64.deb"),
        format!("harlo-{version}-1.x86_64.rpm"),
        format!("harlo-{version}-1-x86_64.pkg.tar.zst"),
        format!("harlo-{version}-x86_64.AppImage"),
    ]
    .into_iter()
    .map(|name| Asset {
        sha256: if name.ends_with(".AppImage") {
            appimage_sum.to_owned()
        } else {
            format!("AB{}", name.len())
        },
        download_url: format!("{base}/{name}"),
        name,
    })
    .collect()
}

#[test]
fn only_harlos_signed_manifests_are_trusted() {
    let list = assets("2.1.0", "https://x", "CD12");
    let good = manifest("2.1.0", "ArcticLatent/harlo", &KEY, &list);
    let release = Release::verified_by(&good, &public()).expect("trusted");
    assert_eq!(release.version, "2.1.0");
    let pick = |kind| release.asset(&kind).map(|a| a.name.clone());
    assert_eq!(
        pick(InstallKind::Deb).as_deref(),
        Some("harlo_2.1.0-1_amd64.deb")
    );
    assert_eq!(
        pick(InstallKind::Rpm).as_deref(),
        Some("harlo-2.1.0-1.x86_64.rpm")
    );
    assert_eq!(
        pick(InstallKind::Arch).as_deref(),
        Some("harlo-2.1.0-1-x86_64.pkg.tar.zst")
    );
    let appimage = release
        .asset(&InstallKind::AppImage("/a".into()))
        .expect("an AppImage");
    assert_eq!(appimage.sha256, "cd12", "sums are lowercased");
    // Another key, a change after signing, no signature, another project:
    // all refused.
    let other = manifest("2.1.0", "ArcticLatent/harlo", &[6; 32], &list);
    Release::verified_by(&other, &public()).expect_err("another key");
    let changed = good.replace(
        "https://x/harlo-2.1.0-x86_64.AppImage",
        "https://evil/harlo",
    );
    Release::verified_by(&changed, &public()).expect_err("changed");
    let mut unsigned: serde_json::Value = serde_json::from_str(&good).expect("JSON");
    if let Some(fields) = unsigned.as_object_mut() {
        fields.remove("signature");
    }
    Release::verified_by(&unsigned.to_string(), &public()).expect_err("unsigned");
    let elsewhere = manifest("2.1.0", "someone/else", &KEY, &list);
    Release::verified_by(&elsewhere, &public()).expect_err("someone else's");
    // Harlo's own key trusts none of the test's.
    Release::verified(&good).expect_err("Harlo's key");
}

/// Serves `files` (path, body) on `listener`.
fn serve_on(listener: tokio::net::TcpListener, files: Vec<(String, Vec<u8>)>) {
    let mut app = axum::Router::new();
    for (path, body) in files {
        app = app.route(
            &path,
            axum::routing::get(move || {
                let body = body.clone();
                async move { body }
            }),
        );
    }
    tokio::spawn(async move { axum::serve(listener, app).await });
}

#[tokio::test(flavor = "multi_thread")]
async fn an_appimage_updates_from_a_signed_release() {
    // The next AppImage, and its release, on a stand-in server.
    let next = b"#!/bin/sh\necho harlo 2.1.0\n".to_vec();
    let sum = hex::encode(Sha256::digest(&next));
    let changelog = "# Changelog\n\n## 2.1.0\n\n- Faster.\n\n## 2.0.0\n\n- Rebuilt.\n";
    // The manifest names the stand-in's own address, so it's served last.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let manifest_text = manifest(
        "2.1.0",
        "ArcticLatent/harlo",
        &KEY,
        &assets("2.1.0", &at, &sum),
    );
    serve_on(
        listener,
        vec![
            ("/linux-release.json".into(), manifest_text.into_bytes()),
            ("/CHANGELOG.md".into(), changelog.as_bytes().to_vec()),
            ("/harlo-2.1.0-x86_64.AppImage".into(), next.clone()),
        ],
    );

    let release = updates::latest_at(&format!("{at}/linux-release.json"), &public())
        .await
        .expect("the release");
    assert!(updates::newer(&release.version, "2.0.0"));
    let notes = updates::changes_at(&format!("{at}/CHANGELOG.md"), "2.0.0", &release.version)
        .await
        .expect("the notes");
    assert_eq!(notes, "• Faster.");
    // A wrong address for the manifest says so.
    updates::latest_at(&format!("{at}/missing.json"), &public())
        .await
        .expect_err("no manifest there");

    // The running AppImage, downloaded over and replaced.
    let home = tempfile::tempdir().expect("a folder");
    let running = home.path().join("Harlo.AppImage");
    std::fs::write(&running, b"old").expect("the old one");
    let kind = InstallKind::AppImage(running.clone());
    let asset = release.asset(&kind).expect("an AppImage").clone();
    let file = home.path().join("updates").join(&asset.name);
    let mut steps = Box::pin(
        Fetch {
            url: asset.download_url.clone(),
            path: file.clone(),
            size: 0,
            sha256: Some(asset.sha256.clone()),
        }
        .download(),
    );
    let mut done = None;
    while let Some(step) = steps.next().await {
        if let Download::Done(result) = step {
            done = Some(result);
        }
    }
    assert_eq!(done, Some(Ok(())));
    updates::install(&kind, &file).await.expect("installed");
    assert_eq!(std::fs::read(&running).expect("the new one"), next);
    assert!(!file.exists(), "the download is cleaned up");
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(&running).expect("metadata").permissions(),
    );
    assert_eq!(mode & 0o111, 0o111, "runnable");

    // A download that doesn't match its sum is thrown away.
    let mut steps = Box::pin(
        Fetch {
            url: asset.download_url,
            path: home.path().join("updates/bad.AppImage"),
            size: 0,
            sha256: Some("00".repeat(32)),
        }
        .download(),
    );
    let mut done = None;
    while let Some(step) = steps.next().await {
        if let Download::Done(result) = step {
            done = Some(result);
        }
    }
    assert!(matches!(done, Some(Err(_))), "{done:?}");
    assert!(!home.path().join("updates/bad.AppImage").exists());
}

#[test]
fn an_appimages_server_runs_from_a_copy_of_its_own() {
    let home = tempfile::tempdir().expect("a folder");
    let bundled = home.path().join("mount/usr/bin/harlo-server");
    std::fs::create_dir_all(bundled.parent().expect("its folder")).expect("folders");
    std::fs::write(&bundled, b"server 2.0.0").expect("the bundled server");
    let copies = home.path().join("data/harlo/server");
    let copy = harlo_core::server::server_copy(&bundled, &copies, "2.0.0").expect("copied");
    assert_eq!(copy, copies.join("2.0.0/harlo-server"));
    assert_eq!(std::fs::read(&copy).expect("the copy"), b"server 2.0.0");
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(&copy).expect("metadata").permissions(),
    );
    assert_eq!(mode & 0o111, 0o111, "runnable");
    // Made once per version: a later start keeps the copy as it is.
    std::fs::write(&bundled, b"changed").expect("the mount changed");
    let again = harlo_core::server::server_copy(&bundled, &copies, "2.0.0").expect("found");
    assert_eq!(std::fs::read(&again).expect("the copy"), b"server 2.0.0");
    // A new version gets a copy of its own beside it.
    let next = harlo_core::server::server_copy(&bundled, &copies, "2.1.0").expect("copied");
    assert_eq!(std::fs::read(&next).expect("the copy"), b"changed");
    assert_eq!(
        std::fs::read_dir(copies.join("2.1.0"))
            .expect("its folder")
            .count(),
        1,
        "no part file left"
    );
}
