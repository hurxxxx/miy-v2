//! Generic registry client. New component releases do not require a new executable.
use crate::{
    app_cli::{self, Session},
    cli_error::{self, Category, CliError},
    component_contract as registry, contract,
};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const DIRECTORY: &str = "platform-components";
const LOCK: &str = "miy.components.lock.json";
#[derive(Subcommand)]
pub enum ComponentCommand {
    /// Read live component releases from the configured platform.
    List,
    /// Read usage, source and compatibility of a release (latest compatible by default).
    Show {
        name: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Install a new component; does not execute package hooks or change business code.
    Add {
        name: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Explicitly update an installed component (latest compatible by default).
    Update {
        name: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Fetch template defaults initially; otherwise restore exact locked releases and missing files.
    Sync,
    /// Verify installed files and compatibility without contacting the platform.
    Check,
    /// Publish a component source directory; requires platform administrator authority.
    Publish { path: PathBuf },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    version: String,
    sha256: String,
    description: String,
    ui_contract: String,
    bundle_versions: Vec<String>,
    files: BTreeMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lock {
    protocol: String,
    registry: String,
    components: BTreeMap<String, Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    protocol: String,
    defaults: BTreeMap<String, String>,
}

fn regular(path: &Path) -> Result<()> {
    crate::platform::reject_reparse(path)?;
    ensure!(
        std::fs::symlink_metadata(path)?.is_file(),
        "component_file_must_be_regular: {}",
        path.display()
    );
    Ok(())
}
fn defaults(root: &Path) -> Result<Defaults> {
    let file = root.join("miy.components.json");
    regular(&file)?;
    let v: Defaults = serde_json::from_slice(&std::fs::read(file)?)?;
    ensure!(
        v.protocol == registry::PROTOCOL,
        "unsupported_component_protocol"
    );
    for (name, v) in &v.defaults {
        ensure!(contract::valid_id(name), "invalid_component_name");
        registry::version(v)?;
    }
    Ok(v)
}
fn load(root: &Path) -> Result<Option<Lock>> {
    let dir = root.join(DIRECTORY);
    crate::platform::reject_reparse(&dir)?;
    if !dir.try_exists()? {
        return Ok(None);
    }
    ensure!(
        std::fs::symlink_metadata(&dir)?.is_dir(),
        "invalid_component_directory"
    );
    let path = dir.join(LOCK);
    regular(&path)
        .context("missing_component_lock: do not mix unmanaged files into platform-components")?;
    ensure!(
        std::fs::metadata(&path)?.len() <= 4 * 1024 * 1024,
        "component_lock_too_large"
    );
    let lock: Lock = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        lock.protocol == registry::PROTOCOL,
        "unsupported_component_protocol"
    );
    crate::cli_client::origin(&lock.registry).map_err(|_| {
        CliError::new(
            Category::Environment,
            "invalid_component_registry",
            "The locked component registry origin is invalid.",
        )
    })?;
    ensure!(lock.components.len() <= 128, "too_many_components");
    for (name, entry) in &lock.components {
        ensure!(contract::valid_id(name), "invalid_component_name");
        registry::version(&entry.version)?;
        ensure!(
            entry.files.len() <= 128
                && entry
                    .bundle_versions
                    .iter()
                    .any(|v| crate::bundle::component_compatible(v)),
            "incompatible_component: {name}"
        );
        for (file, hash) in &entry.files {
            ensure!(
                registry::valid_file(file)
                    && hash.len() == 64
                    && hash.bytes().all(|v| v.is_ascii_hexdigit()),
                "invalid_component_lock_file"
            );
        }
    }
    Ok(Some(lock))
}
fn inventory(root: &Path, path: &Path, files: &mut BTreeSet<String>) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        crate::platform::reject_reparse(&entry.path())?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            inventory(root, &entry.path(), files)?;
        } else {
            ensure!(kind.is_file(), "component_symlink_or_special_file");
            files.insert(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .context("invalid_component_path")?
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}
fn inspect(root: &Path, lock: &Lock, allow_missing: bool) -> Result<()> {
    let dir = root.join(DIRECTORY);
    let mut actual = BTreeSet::new();
    inventory(&dir, &dir, &mut actual)?;
    let mut expected = BTreeSet::from([LOCK.into()]);
    for (name, entry) in &lock.components {
        for (file, digest) in &entry.files {
            let relative = format!("{name}/{file}");
            expected.insert(relative.clone());
            let path = dir.join(relative);
            if allow_missing && !path.try_exists()? {
                continue;
            }
            regular(&path).context("component_file_missing: run app components sync")?;
            ensure!(
                std::fs::metadata(&path)?.len() <= 512 * 1024,
                "component_file_too_large"
            );
            ensure!(
                contract::hash(std::fs::read(path)?) == *digest,
                "component_modified: {name}/{file}; preserve changes outside platform-components before updating"
            );
        }
    }
    ensure!(
        actual.is_subset(&expected),
        "unmanaged_component_files: move custom code into web/src"
    );
    Ok(())
}
fn materialize(root: &Path, lock: &Lock) -> Result<BTreeMap<String, registry::Release>> {
    inspect(root, lock, false)?;
    let mut releases = BTreeMap::new();
    for (name, entry) in &lock.components {
        let files = entry
            .files
            .keys()
            .map(|file| {
                Ok((
                    file.clone(),
                    std::fs::read_to_string(root.join(DIRECTORY).join(name).join(file))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let r = registry::release(registry::Package {
            protocol: registry::PROTOCOL.into(),
            name: name.clone(),
            version: entry.version.clone(),
            description: entry.description.clone(),
            ui_contract: entry.ui_contract.clone(),
            bundle_versions: entry.bundle_versions.clone(),
            files,
        })?;
        ensure!(
            r.sha256 == entry.sha256,
            "component_lock_digest_mismatch: {name}"
        );
        releases.insert(name.clone(), r);
    }
    Ok(releases)
}
pub fn verify(root: &Path) -> Result<Value> {
    cli_error::validation(verify_local(root), "component_validation_failed")
}
fn verify_local(root: &Path) -> Result<Value> {
    ensure!(
        !root.join(".miy/components-backup").exists(),
        "component_update_interrupted: run app components sync"
    );
    let lock =
        load(root)?.context("components_not_installed: log in and run app components sync")?;
    for name in defaults(root)?.defaults.keys() {
        ensure!(
            lock.components.contains_key(name),
            "missing_required_component: {name}"
        );
    }
    materialize(root, &lock)?;
    Ok(
        json!({"valid":true,"registry":lock.registry,"components":lock.components.iter().map(|(name,e)|json!({"name":name,"version":e.version,"sha256":e.sha256})).collect::<Vec<_>>()}),
    )
}
fn operation(root: &Path) -> Result<std::fs::File> {
    let local = root.join(".miy");
    crate::platform::reject_reparse(&local)?;
    std::fs::create_dir_all(&local)?;
    ensure!(
        std::fs::symlink_metadata(&local)?.is_dir(),
        "invalid_miy_directory"
    );
    let lock_path = local.join("components.operation.lock");
    if lock_path.exists() {
        regular(&lock_path)?;
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock.try_lock()
        .context("component_operation_already_running")?;
    let backup = local.join("components-backup");
    crate::platform::reject_reparse(&backup)?;
    if backup.try_exists()? {
        ensure!(
            std::fs::symlink_metadata(&backup)?.is_dir(),
            "invalid_component_backup"
        );
        if root.join(DIRECTORY).try_exists()? {
            std::fs::remove_dir_all(backup)?;
        } else {
            std::fs::rename(backup, root.join(DIRECTORY))?;
        }
    }
    Ok(lock)
}
fn commit(root: &Path, origin: &str, releases: BTreeMap<String, registry::Release>) -> Result<()> {
    ensure!(releases.len() <= 128, "too_many_components");
    let staging = root
        .join(".miy")
        .join(format!("components-stage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging)?;
    let result = (|| -> Result<()> {
        let mut entries = BTreeMap::new();
        for (name, r) in releases {
            registry::verify_release(&r)?;
            ensure!(
                registry::compatible(&r.package),
                "incompatible_component: {name}"
            );
            let p = r.package;
            let mut hashes = BTreeMap::new();
            for (file, content) in p.files {
                let path = staging.join(&name).join(&file);
                std::fs::create_dir_all(path.parent().unwrap())?;
                std::fs::write(path, &content)?;
                hashes.insert(file, contract::hash(content));
            }
            entries.insert(
                name,
                Entry {
                    version: p.version,
                    sha256: r.sha256,
                    description: p.description,
                    ui_contract: p.ui_contract,
                    bundle_versions: p.bundle_versions,
                    files: hashes,
                },
            );
        }
        let lock = Lock {
            protocol: registry::PROTOCOL.into(),
            registry: origin.into(),
            components: entries,
        };
        std::fs::write(staging.join(LOCK), serde_json::to_vec_pretty(&lock)?)?;
        let destination = root.join(DIRECTORY);
        let backup = root.join(".miy/components-backup");
        let previous = destination.try_exists()?;
        if previous {
            std::fs::rename(&destination, &backup)?;
        }
        if let Err(e) = std::fs::rename(&staging, &destination) {
            if previous {
                std::fs::rename(backup, destination)?;
            }
            return Err(e.into());
        }
        if previous {
            std::fs::remove_dir_all(backup)?;
        }
        Ok(())
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(staging);
    }
    result
}
pub async fn fetch(s: &Session, name: &str, version: &str) -> Result<registry::Release> {
    if !contract::valid_id(name) {
        return Err(CliError::new(
            Category::Usage,
            "invalid_component_name",
            "Use a valid component identifier.",
        )
        .into());
    }
    registry::version(version).map_err(|_| {
        CliError::new(
            Category::Usage,
            "invalid_component_version",
            "Use a component version in major.minor.patch format.",
        )
    })?;
    let url = format!("{}/api/v1/components/{name}/{version}", s.origin);
    let client = crate::cli_client::client(s.ca_file.as_deref(), 30)?;
    let mut response = crate::cli_client::authenticate(s, client.get(url))
        .send()
        .await
        .map_err(cli_error::transport)?;
    if !response.status().is_success() {
        return match cli_error::response(response).await {
            Err(error) => Err(error),
            Ok(_) => Err(CliError::new(
                Category::Network,
                "component_download_failed",
                "The component registry returned an unsuccessful response.",
            )
            .into()),
        };
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(cli_error::transport)? {
        if bytes.len() + chunk.len() > registry::MAX_PACKAGE + 1024 {
            return Err(CliError::new(
                Category::Network,
                "component_response_too_large",
                "The component registry response exceeds the package size limit.",
            )
            .into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let invalid_response = || {
        CliError::new(
            Category::Network,
            "invalid_component_response",
            "The registry did not return a valid immutable component release.",
        )
    };
    let release: registry::Release =
        serde_json::from_slice(&bytes).map_err(|_| invalid_response())?;
    registry::verify_release(&release).map_err(|_| invalid_response())?;
    if release.package.name != name || release.package.version != version {
        return Err(CliError::new(
            Category::Network,
            "component_identity_mismatch",
            "The returned component does not match the requested name and version.",
        )
        .into());
    }
    if !registry::compatible(&release.package) {
        return Err(CliError::new(
            Category::Environment,
            "incompatible_component",
            "The requested component is not compatible with this project's bundle.",
        )
        .with_details(json!({"name":name,"version":version}))
        .into());
    }
    Ok(release)
}
pub async fn select(s: &Session, name: &str, v: Option<String>) -> Result<registry::Release> {
    if !contract::valid_id(name) {
        return Err(CliError::new(
            Category::Usage,
            "invalid_component_name",
            "Use a valid component identifier.",
        )
        .into());
    }
    let version = if let Some(v) = v {
        v
    } else {
        let catalog = app_cli::api(s, "/api/v1/components", None).await?;
        catalog
            .as_array()
            .ok_or_else(|| {
                CliError::new(
                    Category::Network,
                    "invalid_component_catalog",
                    "The server returned an invalid component catalog.",
                )
            })?
            .iter()
            .filter(|v| {
                v["name"] == name
                    && v["uiContract"] == "miy.ui/v1alpha1"
                    && v["bundleVersions"].as_array().is_some_and(|a| {
                        a.iter()
                            .any(|v| v.as_str().is_some_and(crate::bundle::component_compatible))
                    })
            })
            .filter_map(|v| {
                let version = v["version"].as_str()?;
                Some((registry::version(version).ok()?, version.to_string()))
            })
            .max_by_key(|(key, _)| *key)
            .map(|(_, v)| v)
            .ok_or_else(|| {
                CliError::new(
                    Category::Environment,
                    "no_compatible_component_release",
                    "No component release is compatible with the current bundle.",
                )
                .with_details(json!({"name":name}))
            })?
    };
    fetch(s, name, &version).await
}
pub async fn sync(root: &Path, s: &Session) -> Result<Value> {
    let _operation = cli_error::validation(operation(root), "component_validation_failed")?;
    let lock = cli_error::validation(load(root), "component_validation_failed")?;
    let mut packages = BTreeMap::new();
    if let Some(lock) = lock {
        if lock.registry != s.origin {
            return Err(CliError::new(Category::Environment, "component_registry_changed", "The locked component registry does not match this project's server. Do not mix registries.").into());
        }
        cli_error::validation(inspect(root, &lock, true), "component_validation_failed")?;
        for (name, entry) in lock.components {
            let r = fetch(s, &name, &entry.version).await?;
            if r.sha256 != entry.sha256 {
                return Err(CliError::new(
                    Category::Environment,
                    "locked_component_digest_changed",
                    "The component release no longer matches its locked digest.",
                )
                .with_details(json!({"name":name}))
                .into());
            }
            packages.insert(name, r);
        }
    }
    for (name, v) in cli_error::validation(defaults(root), "component_validation_failed")?.defaults
    {
        if !packages.contains_key(&name) {
            packages.insert(name.clone(), fetch(s, &name, &v).await?);
        }
    }
    cli_error::validation(
        commit(root, &s.origin, packages),
        "component_validation_failed",
    )?;
    verify(root)
}
pub fn initialize(root: &Path, s: &Session, r: registry::Release) -> Result<Value> {
    cli_error::validation(initialize_local(root, s, r), "component_validation_failed")
}
fn initialize_local(root: &Path, s: &Session, r: registry::Release) -> Result<Value> {
    let _operation = operation(root)?;
    ensure!(load(root)?.is_none(), "component_directory_already_exists");
    commit(
        root,
        &s.origin,
        BTreeMap::from([(r.package.name.clone(), r)]),
    )?;
    verify(root)
}
pub async fn execute(command: ComponentCommand) -> Result<Value> {
    let root = Path::new(".");
    if matches!(command, ComponentCommand::Check) {
        return verify(root);
    }
    let s = app_cli::session()?;
    match command {
        ComponentCommand::List => app_cli::api(&s, "/api/v1/components", None).await,
        ComponentCommand::Show { name, version } => {
            Ok(serde_json::to_value(select(&s, &name, version).await?)?)
        }
        ComponentCommand::Publish { path } => {
            app_cli::api(
                &s,
                "/api/v1/components",
                Some(serde_json::to_value(cli_error::validation(
                    registry::read_source(&path),
                    "component_validation_failed",
                )?)?),
            )
            .await
        }
        ComponentCommand::Sync => {
            cli_error::validation(crate::bundle::verify(root), "component_validation_failed")?;
            sync(root, &s).await
        }
        ComponentCommand::Add { name, version } => change(root, &s, &name, version, false).await,
        ComponentCommand::Update { name, version } => change(root, &s, &name, version, true).await,
        ComponentCommand::Check => unreachable!(),
    }
}
async fn change(
    root: &Path,
    s: &Session,
    name: &str,
    version: Option<String>,
    update: bool,
) -> Result<Value> {
    cli_error::validation(crate::bundle::verify(root), "component_validation_failed")?;
    let _operation = cli_error::validation(operation(root), "component_validation_failed")?;
    let lock =
        cli_error::validation(load(root), "component_validation_failed")?.ok_or_else(|| {
            CliError::new(
                Category::Environment,
                "components_not_installed",
                "Run app components sync before modifying installed components.",
            )
            .with_recovery("miy", &["app", "components", "sync"])
        })?;
    if lock.registry != s.origin {
        return Err(CliError::new(
            Category::Environment,
            "component_registry_changed",
            "The locked component registry does not match this project's server.",
        )
        .into());
    }
    if lock.components.contains_key(name) != update {
        let (code, message) = if update {
            (
                "component_not_installed",
                "The requested component is not installed.",
            )
        } else {
            (
                "component_already_installed",
                "The component is already installed; use app components update.",
            )
        };
        return Err(CliError::new(Category::Environment, code, message).into());
    }
    let mut packages =
        cli_error::validation(materialize(root, &lock), "component_validation_failed")?;
    packages.insert(name.into(), select(s, name, version).await?);
    cli_error::validation(
        commit(root, &s.origin, packages),
        "component_validation_failed",
    )?;
    verify(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn registry_failures_preserve_status_without_exposing_response_content() {
        use axum::{Router, http::StatusCode, routing::get};
        for (status, body, category) in [
            (
                StatusCode::FORBIDDEN,
                r#"{"error":{"code":"component_denied","message":"https://example.invalid/?token=test-secret"}}"#,
                Category::Forbidden,
            ),
            (
                StatusCode::OK,
                "invalid component JSON test-secret",
                Category::Network,
            ),
        ] {
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let app = Router::new().route(
                "/api/v1/components/ui/1.0.1",
                get(move || async move { (status, body) }),
            );
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let session = Session {
                origin,
                cookie: String::new(),
                csrf: String::new(),
                token: None,
                ca_file: None,
            };
            let result = fetch(&session, "ui", "1.0.1").await;
            server.abort();
            let _ = server.await;
            let error = result.unwrap_err();
            let error = error.downcast_ref::<CliError>().unwrap();
            assert_eq!(error.category, category);
            let serialized = serde_json::to_string(error).unwrap();
            assert!(!serialized.contains("test-secret"));
            assert!(!serialized.contains("example.invalid"));
        }
    }
    fn fixture() -> (tempfile::TempDir, Session, registry::Release) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("miy.components.json"),
            r#"{"protocol":"miy.component/v1","defaults":{"ui":"1.0.0"}}"#,
        )
        .unwrap();
        let package = registry::read_source(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../registry/components/ui/1.0.1"),
        )
        .unwrap();
        let session = Session {
            origin: "http://localhost:18000".into(),
            cookie: String::new(),
            csrf: String::new(),
            token: None,
            ca_file: None,
        };
        (root, session, registry::release(package).unwrap())
    }
    #[test]
    fn component_edits_extras_and_symlinks_cannot_be_silently_overwritten() {
        let (root, s, r) = fixture();
        initialize(root.path(), &s, r).unwrap();
        let lock = load(root.path()).unwrap().unwrap();
        let file = root.path().join(DIRECTORY).join("ui/index.tsx");
        let original = std::fs::read(&file).unwrap();
        std::fs::write(&file, "user changes").unwrap();
        assert!(inspect(root.path(), &lock, true).is_err());
        let error = verify(root.path()).unwrap_err();
        let error = error.downcast_ref::<CliError>().unwrap();
        assert_eq!(error.category, Category::Environment);
        assert_eq!(error.code, "component_validation_failed");
        assert!(error.message.contains("component_modified"));
        std::fs::write(&file, original).unwrap();
        let extra = root.path().join(DIRECTORY).join("custom.ts");
        std::fs::write(&extra, "custom code").unwrap();
        assert!(verify(root.path()).is_err());
        std::fs::remove_file(extra).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(verify(root.path()).is_err());
        assert!(inspect(root.path(), &lock, true).is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path().join("miy.components.json"), file).unwrap();
            assert!(inspect(root.path(), &lock, true).is_err());
        }
    }
    #[test]
    fn interrupted_directory_swap_recovers_and_rejects_concurrent_installers() {
        let (root, s, r) = fixture();
        initialize(root.path(), &s, r).unwrap();
        std::fs::rename(
            root.path().join(DIRECTORY),
            root.path().join(".miy/components-backup"),
        )
        .unwrap();
        assert!(verify(root.path()).is_err());
        let active = operation(root.path()).unwrap();
        assert!(verify(root.path()).is_ok());
        assert!(operation(root.path()).is_err());
        drop(active);
        assert!(operation(root.path()).is_ok());
    }
}
