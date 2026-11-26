use crate::{
    app_cli, cli_client,
    cli_error::{self, Category, CliError},
    cli_output::Outcome,
    source_archive::{MAX_SOURCE_BYTES, valid_digest},
};
use anyhow::{Result, ensure};
use clap::Subcommand;
use futures_util::StreamExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Subcommand)]
pub enum SourceCommand {
    /// Save the verified build archive of a release to a new local file.
    Download {
        #[arg(long)]
        release: Uuid,
        /// Path to a new tar file. The parent directory must exist.
        #[arg(long)]
        out: PathBuf,
    },
    /// Restore a release from its original committed project or exact build tar.
    /// Use the global --project to select a different local project directory.
    Upload {
        #[arg(long)]
        release: Uuid,
        /// Use an existing build tar. Select the app explicitly with --app.
        #[arg(long)]
        archive: Option<PathBuf>,
    },
}

fn invalid_response() -> CliError {
    CliError::new(
        Category::Network,
        "invalid_release_source_response",
        "The server returned an invalid release source response.",
    )
}

async fn request(
    method: reqwest::Method,
    app: &str,
    release: Uuid,
) -> Result<reqwest::RequestBuilder> {
    ensure!(
        crate::contract::valid_id(app),
        CliError::new(
            Category::Usage,
            "invalid_app_id",
            "Use a valid app identifier."
        )
    );
    let session = app_cli::session()?;
    crate::cli_remote::require_features(&session, &["app.release-source.v1"]).await?;
    let session = crate::cli_login::refreshed(&session).await?;
    let client = cli_client::client(session.ca_file.as_deref(), 300)?;
    let url = format!(
        "{}/api/v1/apps/{app}/releases/{release}/source",
        session.origin
    );
    Ok(cli_client::authenticate(
        &session,
        client.request(method, url),
    ))
}

fn missing_app() -> CliError {
    CliError::new(
        Category::Usage,
        "app_required",
        "Select the app with the global --app option.",
    )
}

fn existing_output() -> CliError {
    CliError::new(
        Category::Conflict,
        "release_source_output_exists",
        "Select a new destination file to avoid overwriting local data.",
    )
}

async fn download(app: &str, release: Uuid, out: &Path) -> Result<Outcome> {
    match std::fs::symlink_metadata(out) {
        Ok(_) => return Err(existing_output().into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let response = request(reqwest::Method::GET, app, release)
        .await?
        .send()
        .await?;
    if !response.status().is_success() {
        return cli_error::response(response).await.map(Outcome::Data);
    }
    let size = response
        .content_length()
        .filter(|size| (1..=MAX_SOURCE_BYTES).contains(size))
        .ok_or_else(invalid_response)?;
    let hash = response
        .headers()
        .get("x-miy-archive-sha256")
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_digest(value))
        .ok_or_else(invalid_response)?
        .to_owned();
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::from_std(temp.reopen()?);
    let mut digest = Sha256::new();
    let mut bytes = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        bytes += chunk.len() as u64;
        ensure!(bytes <= size, invalid_response());
        digest.update(&chunk);
        file.write_all(&chunk).await?;
    }
    ensure!(
        bytes == size && hex::encode(digest.finalize()) == hash,
        invalid_response()
    );
    file.sync_all().await?;
    drop(file);
    temp.persist_noclobber(out)
        .map_err(|error| -> anyhow::Error {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                existing_output().into()
            } else {
                error.error.into()
            }
        })?;
    Ok(Outcome::Data(
        json!({"appId":app,"releaseId":release,"archiveSha256":hash,"sizeBytes":size,"path":out}),
    ))
}

async fn upload(app: &str, release: Uuid, archive: &Path) -> Result<Outcome> {
    crate::platform::reject_reparse(archive)?;
    let metadata = std::fs::symlink_metadata(archive)?;
    let size = metadata.len();
    ensure!(
        metadata.is_file() && (1..=MAX_SOURCE_BYTES).contains(&size),
        CliError::new(
            Category::Usage,
            "invalid_release_source_file",
            "Select a regular build tar file up to 64 MiB."
        )
    );
    let archive_hash = crate::source_archive::digest(archive)?;
    let response = request(reqwest::Method::PUT, app, release)
        .await?
        .header(reqwest::header::CONTENT_TYPE, "application/x-tar")
        .header(reqwest::header::CONTENT_LENGTH, size)
        .body(tokio::fs::File::open(archive).await?)
        .send()
        .await?;
    let result = cli_error::response(response).await?;
    ensure!(
        result["appId"] == app
            && result["releaseId"] == release.to_string()
            && result["archiveSha256"] == archive_hash
            && result["sizeBytes"] == size,
        invalid_response()
    );
    Ok(Outcome::Data(result))
}

pub async fn execute(command: SourceCommand) -> Result<Outcome> {
    let selected = crate::cli_config::options().app.clone();
    match command {
        SourceCommand::Download { release, out } => {
            let app = match selected {
                Some(app) => app,
                None => app_cli::manifest()?.id,
            };
            download(&app, release, &out).await
        }
        SourceCommand::Upload {
            release,
            archive: Some(archive),
        } => upload(&selected.ok_or_else(missing_app)?, release, &archive).await,
        SourceCommand::Upload {
            release,
            archive: None,
        } => {
            let temp = tempfile::tempdir_in(crate::cli_config::ensure_root()?)?;
            let archive = temp.path().join("source.tar");
            let source = crate::cli_build::pack_source(Path::new("."), &archive)?;
            let app = source.manifest.id;
            ensure!(
                selected.is_none_or(|selected| selected == app),
                CliError::new(
                    Category::Usage,
                    "app_target_mismatch",
                    "Select the app declared by the local project manifest."
                )
            );
            match upload(&app, release, &archive).await {
                Err(err)
                    if Path::new("README.md").is_file()
                        && err.downcast_ref::<CliError>().is_some_and(|e| {
                            matches!(
                                e.code.as_str(),
                                "release_source_digest_mismatch" | "release_source_size_mismatch"
                            )
                        }) =>
                {
                    let legacy = temp.path().join("legacy.tar");
                    let original =
                        crate::source_archive::pack_committed(Path::new("."), &legacy, false)?;
                    ensure!(
                        original.source_commit == source.source_commit,
                        "source_changed_during_archive"
                    );
                    upload(&app, release, &legacy).await
                }
                result => result,
            }
        }
    }
}
