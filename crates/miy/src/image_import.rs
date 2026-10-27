//! Worker-owned verification across Docker classic and containerd image stores.
//! Mutable archive tags are discovery hints only. Every accepted mapping is
//! verified by exporting the captured immutable local ID and checking its config.
use crate::{contract, runtime};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{collections::BTreeMap, io::Read, path::Path};
use uuid::Uuid;

struct Member {
    hash: String,
    size: u64,
    json: Option<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub config_sha256: String,
    pub platform: String,
    pub source_commit: String,
    tags: Vec<String>,
}

fn json<'a>(members: &'a BTreeMap<String, Member>, name: &str) -> Result<&'a Value> {
    members
        .get(name)
        .and_then(|member| member.json.as_ref())
        .context("missing_image_json")
}

fn descriptor<'a>(members: &'a BTreeMap<String, Member>, value: &Value) -> Result<&'a Member> {
    let digest = value["digest"]
        .as_str()
        .filter(|v| runtime::image_id(v))
        .context("invalid_image_descriptor")?;
    let member = members
        .get(&format!("blobs/sha256/{}", &digest[7..]))
        .context("missing_image_blob")?;
    ensure!(
        format!("sha256:{}", member.hash) == digest && value["size"].as_u64() == Some(member.size),
        "image_blob_digest_mismatch"
    );
    Ok(member)
}

fn linked_config(
    members: &BTreeMap<String, Member>,
    description: &Value,
    config_hash: &str,
    layers: &[String],
    depth: usize,
) -> Result<usize> {
    ensure!(depth <= 4, "image_index_too_deep");
    let blob = descriptor(members, description)?;
    let value = blob.json.as_ref().context("invalid_image_manifest")?;
    if let Some(children) = value["manifests"].as_array() {
        ensure!(
            !children.is_empty() && children.len() <= 16,
            "invalid_image_index"
        );
        let mut matches = 0;
        for child in children {
            // BuildKit provenance is not an executable platform image. Its
            // descriptor remains bound by the parent's verified digest.
            if child["annotations"]["vnd.docker.reference.type"] == "attestation-manifest"
                && child["platform"]["os"] == "unknown"
                && child["platform"]["architecture"] == "unknown"
            {
                continue;
            }
            matches += linked_config(members, child, config_hash, layers, depth + 1)?;
            ensure!(matches <= 1, "upload_must_contain_one_image");
        }
        ensure!(matches == 1, "missing_executable_image");
        return Ok(matches);
    }
    let config = descriptor(members, &value["config"])?;
    ensure!(config.hash == config_hash, "image_index_config_mismatch");
    let descriptors = value["layers"].as_array().context("missing_image_layers")?;
    ensure!(
        descriptors.len() == layers.len(),
        "image_index_layers_mismatch"
    );
    for (description, path) in descriptors.iter().zip(layers) {
        let linked = descriptor(members, description)?;
        let exported = members.get(path).context("missing_image_layer")?;
        ensure!(
            linked.hash == exported.hash && linked.size == exported.size,
            "image_index_layers_mismatch"
        );
    }
    Ok(1)
}

/// Validate without extracting any host paths. Both the original OCI digest
/// chain and Docker's manifest.json must select exactly the same config/layers.
pub(crate) fn archive(path: &Path, app: &str, original: Option<&str>) -> Result<Identity> {
    let mut archive = tar::Archive::new(std::fs::File::open(path)?);
    let mut members = BTreeMap::new();
    let mut json_bytes = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry
            .path()?
            .to_str()
            .context("invalid_archive_path")?
            .to_owned();
        ensure!(
            members.len() < 4096 && !members.contains_key(&name),
            "invalid_archive_entries"
        );
        ensure!(
            contract::safe_relative(name.trim_end_matches('/')),
            "invalid_archive_path"
        );
        ensure!(
            entry.header().entry_type().is_file() || entry.header().entry_type().is_dir(),
            "invalid_archive_entry_type"
        );
        let size = entry.size();
        ensure!(
            size <= crate::image_contract::MAX_ARCHIVE,
            "invalid_archive_member_size"
        );
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut small = (size <= 1024 * 1024).then(Vec::new);
        let mut read = 0u64;
        loop {
            let count = entry.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            read += count as u64;
            hash.update(&buffer[..count]);
            if let Some(data) = &mut small {
                data.extend_from_slice(&buffer[..count]);
            }
        }
        ensure!(read == size, "truncated_image_member");
        let parsed = small.and_then(|bytes| serde_json::from_slice(&bytes).ok());
        if parsed.is_some() {
            json_bytes += size;
            ensure!(
                json_bytes <= 16 * 1024 * 1024,
                "image_json_metadata_too_large"
            );
        }
        members.insert(
            name,
            Member {
                hash: hex::encode(hash.finalize()),
                size,
                json: parsed,
            },
        );
    }
    let manifest = json(&members, "manifest.json")?;
    ensure!(
        manifest.as_array().is_some_and(|v| v.len() == 1),
        "upload_must_contain_one_image"
    );
    let config_path = manifest[0]["Config"]
        .as_str()
        .context("missing_image_config")?;
    let config = json(&members, config_path)?;
    let config_hash = &members
        .get(config_path)
        .context("missing_image_config")?
        .hash;
    ensure!(
        config["config"]["Labels"]["org.miy.app.id"] == app,
        "image_app_mismatch"
    );
    let platform = crate::runtime_platform::normalize(
        config["os"].as_str().unwrap_or(""),
        config["architecture"].as_str().unwrap_or(""),
    )
    .filter(|v| matches!(v.as_str(), "linux/amd64" | "linux/arm64"))
    .context("invalid_image_platform")?;
    let commit = config["config"]["Labels"]["org.opencontainers.image.revision"]
        .as_str()
        .filter(|value| {
            value.len() == 40
                && value
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        })
        .context("invalid_image_revision")?;
    let layers: Vec<String> = manifest[0]["Layers"]
        .as_array()
        .context("missing_image_layers")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("invalid_image_layer")
        })
        .collect::<Result<_>>()?;
    ensure!(
        layers.iter().all(|name| members.contains_key(name)),
        "missing_image_layer"
    );
    if let Some(index) = members.get("index.json") {
        let index = index.json.as_ref().context("invalid_image_index")?;
        let descriptions = index["manifests"]
            .as_array()
            .filter(|v| v.len() == 1)
            .context("upload_must_contain_one_image")?;
        if let Some(original) = original {
            ensure!(
                descriptions[0]["digest"] == original,
                "image_digest_mismatch"
            );
        }
        ensure!(
            linked_config(&members, &descriptions[0], config_hash, &layers, 0)? == 1,
            "image_index_config_mismatch"
        );
    } else if let Some(original) = original {
        ensure!(
            format!("sha256:{config_hash}") == original,
            "image_digest_mismatch"
        );
    }
    let tags = manifest[0]["RepoTags"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|value| {
            let tag = value.as_str().context("invalid_image_tag")?;
            ensure!(
                !tag.is_empty()
                    && tag.len() <= 255
                    && !tag.starts_with('-')
                    && tag
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"/._:@-".contains(&b)),
                "invalid_image_tag"
            );
            Ok(tag.to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(tags.len() <= 32, "too_many_image_tags");
    Ok(Identity {
        config_sha256: config_hash.clone(),
        platform,
        source_commit: commit.to_owned(),
        tags,
    })
}

fn inspect_identity(meta: &Value, app: &str, identity: &Identity) -> Result<String> {
    let id = meta["Id"]
        .as_str()
        .filter(|v| runtime::image_id(v))
        .context("invalid_local_image_id")?;
    ensure!(
        meta["Config"]["Labels"]["org.miy.app.id"] == app
            && meta["Config"]["Labels"]["org.opencontainers.image.revision"]
                == identity.source_commit
            && crate::runtime_platform::image(meta).as_deref() == Some(&identity.platform),
        "imported_image_identity_mismatch"
    );
    Ok(id.to_owned())
}

pub(crate) async fn bind(
    pool: &sqlx::PgPool,
    app: &str,
    original: &str,
    identity: &Identity,
    local: &str,
    archive_hash: &str,
) -> Result<()> {
    let changed = sqlx::query("INSERT INTO image_imports(app_id,original_image_id,config_sha256,platform,source_commit,local_image_id,archive_hash) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(app_id,original_image_id) DO UPDATE SET local_image_id=excluded.local_image_id,archive_hash=excluded.archive_hash,verified_at=now() WHERE image_imports.config_sha256=excluded.config_sha256 AND image_imports.platform=excluded.platform AND image_imports.source_commit=excluded.source_commit")
        .bind(app).bind(original).bind(&identity.config_sha256).bind(&identity.platform)
        .bind(&identity.source_commit).bind(local).bind(archive_hash).execute(pool).await?.rows_affected();
    ensure!(changed == 1, "immutable_image_import_conflict");
    Ok(())
}

pub(crate) async fn load(
    pool: &sqlx::PgPool,
    app: &str,
    original: &str,
    path: &Path,
    identity: &Identity,
    archive_hash: &str,
) -> Result<String> {
    // Loading the authenticated, hashed archive also handles recovery after a
    // worker crash. No mutable tag is accepted as the durable import identity.
    let output =
        runtime::docker(&["load", "--input", path.to_str().context("invalid_path")?]).await?;
    let mut candidates = vec![
        original.to_owned(),
        format!("sha256:{}", identity.config_sha256),
    ];
    for line in output.lines() {
        if let Some(id) = line
            .strip_prefix("Loaded image ID: ")
            .filter(|id| runtime::image_id(id))
        {
            candidates.push(id.to_owned());
        }
    }
    candidates.extend(identity.tags.clone());
    let verified_export = path.with_file_name(format!("verify-{}.tar", Uuid::new_v4()));
    let result = async {
        for candidate in candidates {
            let Ok(meta) = runtime::inspect(&candidate).await else {
                continue;
            };
            let Ok(local) = inspect_identity(&meta, app, identity) else {
                continue;
            };
            // Capture the immutable ID before export; a concurrent retag can no
            // longer change which image is verified or later executed.
            runtime::docker(&[
                "image",
                "save",
                "--output",
                verified_export.to_str().context("invalid_path")?,
                &local,
            ])
            .await?;
            let exported = verified_export.clone();
            let checked_app = app.to_owned();
            let actual =
                tokio::task::spawn_blocking(move || archive(&exported, &checked_app, None))
                    .await??;
            if actual.config_sha256 != identity.config_sha256
                || actual.platform != identity.platform
                || actual.source_commit != identity.source_commit
            {
                continue;
            }
            bind(pool, app, original, identity, &local, archive_hash).await?;
            return Ok(local);
        }
        anyhow::bail!("imported_image_unavailable")
    }
    .await;
    let _ = tokio::fs::remove_file(verified_export).await;
    result
}

/// Runtime uses the public original ID for release identity and this local ID
/// exclusively for Docker execution. Old server-built images need no upload.
pub(crate) async fn resolve(pool: &sqlx::PgPool, app: &str, original: &str) -> Result<String> {
    let Some(row) =
        sqlx::query("SELECT * FROM image_imports WHERE app_id=$1 AND original_image_id=$2")
            .bind(app)
            .bind(original)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(original.to_owned());
    };
    let local: String = row.get("local_image_id");
    let identity = Identity {
        config_sha256: row.get("config_sha256"),
        platform: row.get("platform"),
        source_commit: row.get("source_commit"),
        tags: vec![],
    };
    let meta = runtime::inspect(&local).await?;
    ensure!(
        inspect_identity(&meta, app, &identity)? == local,
        "imported_image_identity_mismatch"
    );
    Ok(local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(extra: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"os":"linux","architecture":"amd64",
            "rootfs":{"type":"layers","diff_ids":[]},
            "config":{"Env":[extra],"Labels":{"org.miy.app.id":"sample",
                "org.opencontainers.image.revision":"b".repeat(40)}}}))
        .unwrap()
    }
    fn append(writer: &mut tar::Builder<std::fs::File>, name: &str, bytes: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        writer.append_data(&mut header, name, bytes).unwrap();
    }
    fn fixture(path: &Path, oci: bool, replaced_docker_config: bool, bad_size: bool) -> String {
        let original_config = config("original");
        let original_hash = contract::hash(&original_config);
        let selected_config = if replaced_docker_config {
            config("replaced")
        } else {
            original_config.clone()
        };
        let selected_hash = contract::hash(&selected_config);
        let selected_name = format!("blobs/sha256/{selected_hash}");
        let mut writer = tar::Builder::new(std::fs::File::create(path).unwrap());
        append(
            &mut writer,
            "manifest.json",
            &serde_json::to_vec(&json!([{
                "Config":selected_name,"RepoTags":["sample:immutable"],"Layers":[]
            }]))
            .unwrap(),
        );
        append(&mut writer, &selected_name, &selected_config);
        if replaced_docker_config {
            append(
                &mut writer,
                &format!("blobs/sha256/{original_hash}"),
                &original_config,
            );
        }
        let image = if oci {
            let manifest = serde_json::to_vec(&json!({"schemaVersion":2,
                "config":{"digest":format!("sha256:{original_hash}"),
                    "size":original_config.len()+usize::from(bad_size)},"layers":[]}))
            .unwrap();
            let manifest_hash = contract::hash(&manifest);
            append(
                &mut writer,
                &format!("blobs/sha256/{manifest_hash}"),
                &manifest,
            );
            append(
                &mut writer,
                "index.json",
                &serde_json::to_vec(&json!({"schemaVersion":2,
                "manifests":[{"digest":format!("sha256:{manifest_hash}"),"size":manifest.len()}]}))
                .unwrap(),
            );
            format!("sha256:{manifest_hash}")
        } else {
            format!("sha256:{original_hash}")
        };
        writer.finish().unwrap();
        image
    }

    #[test]
    fn original_identity_is_bound_to_the_docker_selected_config() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("image.tar");
        for oci in [false, true] {
            let image = fixture(&path, oci, false, false);
            assert_eq!(
                archive(&path, "sample", Some(&image))
                    .unwrap()
                    .config_sha256,
                contract::hash(config("original"))
            );
            assert!(archive(&path, "different-app", Some(&image)).is_err());
            assert!(archive(&path, "sample", Some(&format!("sha256:{}", "0".repeat(64)))).is_err());
            let image = fixture(&path, oci, true, false);
            assert!(
                archive(&path, "sample", Some(&image)).is_err(),
                "a separate Docker manifest must not replace the original config"
            );
        }
        let image = fixture(&path, true, false, true);
        assert!(archive(&path, "sample", Some(&image)).is_err());
    }
}
