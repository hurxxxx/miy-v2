//! Resolve a verified helper to an immutable ID in the local Docker image store.
use crate::{
    app_cli::{self, Session},
    cli_error::{Category, CliError},
    docker,
};
use anyhow::{Context, Result, ensure};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metadata {
    schema_version: u8,
    platform: String,
    version: String,
    commit: String,
    image_tag: String,
    config_sha256: String,
    file: String,
    sha256: String,
    development: bool,
}

pub(crate) fn valid_hash(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase())
}

impl Metadata {
    pub(crate) fn from_row(row: &Value, commit: &str, version: &str) -> Result<Self> {
        let value: Self = serde_json::from_value(row.clone())?;
        let arch = value
            .platform
            .strip_prefix("linux/")
            .filter(|arch| matches!(*arch, "amd64" | "arm64"))
            .context("invalid_helper_platform")?;
        ensure!(
            value.schema_version == 1
                && valid_hash(commit, 40)
                && value.commit == commit
                && value.version == version
                && !value.development
                && value.image_tag == format!("miy-db-tunnel:{commit}-{arch}")
                && value.file == format!("miy-db-tunnel-{version}-{arch}.oci.tar")
                && valid_hash(&value.config_sha256, 64)
                && valid_hash(&value.sha256, 64),
            "invalid_helper_provenance"
        );
        Ok(value)
    }

    fn url(&self) -> String {
        format!("/cli/downloads/{}/{}", self.commit, self.file)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Verified {
    metadata: Metadata,
    local_image_id: String,
}

fn image_id(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hash| valid_hash(hash, 64))
}

fn verify_inspect(meta: &Value, expected: &Metadata) -> Result<String> {
    let id = meta["Id"]
        .as_str()
        .filter(|value| image_id(value))
        .context("invalid_helper_local_id")?;
    ensure!(
        crate::runtime_platform::image(meta).as_deref() == Some(&expected.platform)
            && meta["Config"]["Labels"]["org.miy.component"] == "db-tunnel"
            && meta["Config"]["Labels"]["org.opencontainers.image.revision"] == expected.commit,
        "helper_local_identity_mismatch"
    );
    Ok(id.to_owned())
}

fn member(path: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        !name.starts_with('/')
            && !name.contains('\\')
            && name.split('/').all(|part| !matches!(part, "" | "." | "..")),
        "invalid_helper_archive_member"
    );
    let mut archive = tar::Archive::new(std::fs::File::open(path)?);
    let mut found = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path_bytes().as_ref() == name.as_bytes() {
            ensure!(
                found.is_none() && entry.header().entry_type().is_file() && entry.size() <= limit,
                "invalid_helper_archive_member"
            );
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            found = Some(bytes);
        }
    }
    found.context("missing_helper_archive_member")
}

fn verify_archive(path: &Path, expected: &Metadata, check_tag: bool) -> Result<()> {
    let manifest: Vec<Value> = serde_json::from_slice(&member(path, "manifest.json", 64 * 1024)?)?;
    ensure!(manifest.len() == 1, "helper_archive_image_count_mismatch");
    if check_tag {
        ensure!(
            manifest[0]["RepoTags"] == serde_json::json!([expected.image_tag]),
            "helper_archive_tag_mismatch"
        );
    }
    let name = manifest[0]["Config"]
        .as_str()
        .context("invalid_helper_config_path")?;
    let bytes = member(path, name, 2 * 1024 * 1024)?;
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == expected.config_sha256,
        "helper_config_checksum_mismatch"
    );
    let config: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        format!(
            "{}/{}",
            config["os"].as_str().unwrap_or(""),
            config["architecture"].as_str().unwrap_or("")
        ) == expected.platform
            && config["config"]["Labels"]["org.miy.component"] == "db-tunnel"
            && config["config"]["Labels"]["org.opencontainers.image.revision"] == expected.commit,
        "helper_config_provenance_mismatch"
    );
    Ok(())
}

pub(crate) async fn resolve(session: &Session) -> Result<String> {
    let info: Value =
        serde_json::from_str(&docker::docker(&["info", "--format", "{{json .}}"]).await?)?;
    let platform = crate::runtime_platform::normalize(
        info["OSType"].as_str().unwrap_or(""),
        info["Architecture"].as_str().unwrap_or(""),
    )
    .ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "linux_docker_required",
            "Use Linux containers in Docker Desktop.",
        )
    })?;
    let guide = app_cli::api(session, "/api/v1/cli/install", None).await?;
    let row = guide["tunnelImages"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["platform"] == platform))
        .ok_or_else(|| {
            CliError::new(Category::Environment, "development_helper_unavailable",
                "The server has not published the development connection helper for this Docker platform.")
        })?;
    ensure!(
        guide["cliVersion"] == env!("CARGO_PKG_VERSION"),
        "helper_cli_version_mismatch"
    );
    let expected = Metadata::from_row(
        row,
        guide["sourceCommit"]
            .as_str()
            .context("missing_helper_commit")?,
        env!("CARGO_PKG_VERSION"),
    )?;
    ensure!(
        row["url"].as_str() == Some(&expected.url()),
        "invalid_helper_url"
    );
    let cache = crate::cli_config::ensure_root()?.join("cache");
    crate::platform::reject_reparse(&cache)?;
    std::fs::create_dir_all(&cache)?;
    let record = cache.join(format!("tunnel-{}.json", expected.sha256));
    crate::platform::reject_reparse(&record)?;
    if let Ok(bytes) = std::fs::read(&record)
        && let Ok(saved) = serde_json::from_slice::<Verified>(&bytes)
        && saved.metadata == expected
        && image_id(&saved.local_image_id)
        && let Ok(meta) = docker::inspect(&saved.local_image_id).await
        && verify_inspect(&meta, &expected).ok().as_deref() == Some(&saved.local_image_id)
    {
        return Ok(saved.local_image_id);
    }
    let download = cache.join(format!("tunnel-{}.partial", Uuid::new_v4()));
    let exported = cache.join(format!("tunnel-{}.verify.tar", Uuid::new_v4()));
    let result: Result<String> = async {
        let response = crate::cli_client::client(session.ca_file.as_deref(), 300)?
            .get(format!("{}{}", session.origin, expected.url()))
            .send()
            .await?
            .error_for_status()?;
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&download)
            .await?;
        let mut hash = Sha256::new();
        let mut size: usize = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            size += chunk.len();
            ensure!(size <= 512 * 1024 * 1024, "helper_archive_too_large");
            hash.update(&chunk);
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        drop(file);
        ensure!(
            hex::encode(hash.finalize()) == expected.sha256,
            "helper_checksum_mismatch"
        );
        verify_archive(&download, &expected, true)?;
        docker::docker(&[
            "load",
            "--input",
            download.to_str().context("invalid_path")?,
        ])
        .await?;
        // The same archive has a config ID on classic Docker and a manifest/index
        // ID on containerd. Verify the captured local ID, then use only that ID.
        let id = verify_inspect(&docker::inspect(&expected.image_tag).await?, &expected)?;
        docker::docker(&[
            "image",
            "save",
            "--output",
            exported.to_str().context("invalid_path")?,
            &id,
        ])
        .await?;
        verify_archive(&exported, &expected, false)?;
        crate::platform::write_private(
            &record,
            &serde_json::to_vec(&Verified {
                metadata: expected.clone(),
                local_image_id: id.clone(),
            })?,
        )?;
        Ok(id)
    }
    .await;
    let _ = std::fs::remove_file(download);
    let _ = std::fs::remove_file(exported);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (Metadata, Vec<u8>) {
        let commit = "a".repeat(40);
        let config = serde_json::to_vec(&json!({"os":"linux","architecture":"amd64",
            "config":{"Labels":{"org.miy.component":"db-tunnel","org.opencontainers.image.revision":commit}}})).unwrap();
        let row = json!({"schemaVersion":1,"platform":"linux/amd64","version":"0.6.0","commit":commit,
            "imageTag":format!("miy-db-tunnel:{commit}-amd64"),"configSha256":hex::encode(Sha256::digest(&config)),
            "file":"miy-db-tunnel-0.6.0-amd64.oci.tar","sha256":"b".repeat(64),"development":false});
        (Metadata::from_row(&row, &commit, "0.6.0").unwrap(), config)
    }

    fn archive(path: &Path, name: &str, config: &[u8], tag: &str) {
        let mut writer = tar::Builder::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in [
            (
                "manifest.json",
                serde_json::to_vec(&json!([{"Config":name,"RepoTags":[tag],"Layers":[]}])).unwrap(),
            ),
            (name, config.to_vec()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            writer
                .append_data(&mut header, name, bytes.as_slice())
                .unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn classic_and_containerd_exports_share_portable_config_identity() {
        let (expected, config) = fixture();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.tar");
        for name in [
            format!("{}.json", expected.config_sha256),
            format!("blobs/sha256/{}", expected.config_sha256),
        ] {
            archive(&path, &name, &config, &expected.image_tag);
            verify_archive(&path, &expected, true).unwrap();
            let mut changed = config.clone();
            changed.push(b' ');
            archive(&path, &name, &changed, &expected.image_tag);
            assert!(verify_archive(&path, &expected, true).is_err());
            archive(&path, &name, &config, "miy-db-tunnel:other");
            assert!(verify_archive(&path, &expected, true).is_err());
            verify_archive(&path, &expected, false).unwrap();
        }
    }

    #[test]
    fn helper_provenance_is_bound_to_the_candidate_and_platform() {
        let (expected, _) = fixture();
        for (field, value) in [
            ("development", json!(true)),
            ("commit", json!("c".repeat(40))),
            ("imageTag", json!("miy-db-tunnel:latest")),
            ("platform", json!("windows/amd64")),
            ("configSha256", json!("Z".repeat(64))),
            ("version", json!("0.5.0")),
        ] {
            let mut row = serde_json::to_value(&expected).unwrap();
            row[field] = value;
            assert!(Metadata::from_row(&row, &expected.commit, &expected.version).is_err());
        }
        assert!(!image_id("miy-db-tunnel:latest"));
        assert!(image_id(&format!("sha256:{}", "c".repeat(64))));
    }
}
