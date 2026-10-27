//! Real classic Docker archive -> current server store import. Uses only the
//! runtime suite's labeled app image and its disposable database/artifact root.
use crate::{State, auth::Principal, delivery, image_import, runtime};
use anyhow::{Context, Result, ensure};
use axum::{Extension, body::Body, extract::Path, extract::State as S, http::Request};
use serde_json::{Value, json};
use sqlx::Row;
use std::{collections::BTreeSet, io::Read, path::Path as FilePath};
use uuid::Uuid;

fn classic(source: &FilePath, target: &FilePath, tag: &str) -> Result<String> {
    let mut archive = tar::Archive::new(std::fs::File::open(source)?);
    let mut manifest = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path_bytes().as_ref() == b"manifest.json" {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            manifest = Some(serde_json::from_slice::<Value>(&bytes)?);
            break;
        }
    }
    let mut manifest = manifest.context("missing_fixture_manifest")?;
    ensure!(manifest.as_array().is_some_and(|rows| rows.len() == 1));
    let config = manifest[0]["Config"]
        .as_str()
        .context("missing_fixture_config")?
        .to_owned();
    let mut wanted = BTreeSet::from([config.clone()]);
    for layer in manifest[0]["Layers"]
        .as_array()
        .context("missing_fixture_layers")?
    {
        wanted.insert(layer.as_str().context("invalid_fixture_layer")?.to_owned());
    }
    manifest[0]["RepoTags"] = json!([tag]);
    let mut writer = tar::Builder::new(std::fs::File::create(target)?);
    let bytes = serde_json::to_vec(&manifest)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    writer.append_data(&mut header, "manifest.json", bytes.as_slice())?;
    let mut original = None;
    let mut archive = tar::Archive::new(std::fs::File::open(source)?);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry
            .path()?
            .to_str()
            .context("invalid_fixture_path")?
            .to_owned();
        if !wanted.remove(&name) {
            continue;
        }
        let mut header = entry.header().clone();
        if name == config {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            original = Some(format!("sha256:{}", crate::contract::hash(&bytes)));
            writer.append_data(&mut header, &name, bytes.as_slice())?;
        } else {
            writer.append_data(&mut header, &name, &mut entry)?;
        }
    }
    ensure!(wanted.is_empty(), "missing_classic_archive_members");
    writer.finish()?;
    original.context("missing_classic_config_identity")
}

pub(crate) async fn run(
    s: &State,
    actor: &Principal,
    native: &str,
    tag: &str,
) -> Result<(String, String, Value)> {
    let root = std::path::PathBuf::from(std::env::var("MIY_AUDIT_ROOT")?);
    let native_archive = root.join("evidence/native-image.tar");
    let legacy_archive = root.join("evidence/classic-image.tar");
    runtime::docker(&[
        "image",
        "save",
        "--output",
        native_archive.to_str().unwrap(),
        native,
    ])
    .await?;
    // Prove the same parser also accepts the real OCI index/config/layer chain.
    let identity = image_import::archive(&native_archive, "audit-runtime", Some(native))?;
    let original = classic(&native_archive, &legacy_archive, tag)?;
    let classic_identity =
        image_import::archive(&legacy_archive, "audit-runtime", Some(&original))?;
    ensure!(identity.config_sha256 == classic_identity.config_sha256);
    let bytes = std::fs::read(&legacy_archive)?;
    let archive_hash = crate::contract::hash(&bytes);
    let request_id = Uuid::new_v4();
    let request = Request::builder()
        .method("POST")
        .header("x-miy-image", &original)
        .header("x-miy-archive-sha256", &archive_hash)
        .header("x-miy-request-id", request_id.to_string())
        .header("content-length", bytes.len().to_string())
        .body(Body::from(bytes))?;
    let receipt = delivery::upload(
        S(s.clone()),
        Extension(actor.clone()),
        Path("audit-runtime".into()),
        request,
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?
    .0;
    ensure!(receipt["imageId"] == original && receipt["status"] == "pending");
    ensure!(delivery::tick(&s.pool).await?);
    let row = sqlx::query("SELECT status,image_id FROM image_uploads WHERE id=$1")
        .bind(serde_json::from_value::<Uuid>(receipt["id"].clone())?)
        .fetch_one(&s.pool)
        .await?;
    ensure!(
        row.get::<String, _>("status") == "ready",
        "classic image import did not finish"
    );
    ensure!(
        row.get::<String, _>("image_id") == original,
        "public upload identity changed"
    );
    let local = image_import::resolve(&s.pool, "audit-runtime", &original).await?;
    let meta = runtime::inspect(&local).await?;
    ensure!(meta["Id"] == local && meta["Config"]["Labels"]["org.miy.app.id"] == "audit-runtime");
    // A second verified identity cannot replace the original config binding.
    let mut collision = classic_identity.clone();
    collision.config_sha256 = "0".repeat(64);
    ensure!(
        image_import::bind(
            &s.pool,
            "audit-runtime",
            &original,
            &collision,
            &local,
            &archive_hash
        )
        .await
        .is_err()
    );
    ensure!(image_import::resolve(&s.pool, "audit-runtime", &original).await? == local);
    let evidence = json!({"originalImageId":original,"serverLocalImageId":local,"sourceNativeImageId":native,
        "differentStoreIdentity":original!=local,"configSha256":identity.config_sha256,
        "archiveSha256":archive_hash,"receiptIdentityPreserved":true,"configCollisionRejected":true});
    std::fs::write(
        root.join("evidence/portable-image-import.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    Ok((original, local, evidence))
}
