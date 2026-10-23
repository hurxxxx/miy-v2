//! Immutable, authenticated component distribution; no component source is compiled into the CLI.
use crate::{
    State,
    auth::Principal,
    contract,
    http::{ApiError, ApiResult},
};
use anyhow::{Context, Result, ensure};
use axum::{
    Json,
    extract::{Path, State as S},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path as FsPath};

pub const PROTOCOL: &str = "miy.component/v1";
pub const MAX_PACKAGE: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Package {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub ui_contract: String,
    pub bundle_versions: Vec<String>,
    pub files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub sha256: String,
    pub package: Package,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Source {
    protocol: String,
    name: String,
    version: String,
    description: String,
    ui_contract: String,
    bundle_versions: Vec<String>,
    files: Vec<String>,
}
pub fn version(v: &str) -> Result<[u32; 3]> {
    let parts = v
        .split('.')
        .map(|v| {
            ensure!(
                !v.is_empty()
                    && v.len() <= 9
                    && (v == "0" || !v.starts_with('0'))
                    && v.bytes().all(|b| b.is_ascii_digit()),
                "invalid_component_version"
            );
            Ok(v.parse()?)
        })
        .collect::<Result<Vec<u32>>>()?;
    parts
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid_component_version"))
}
pub fn valid_file(path: &str) -> bool {
    path.len() <= 180
        && path.split('/').all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        && [".ts", ".tsx", ".css", ".svg", ".json", ".md"]
            .iter()
            .any(|s| path.ends_with(s))
        && path.split('/').all(|p| p != "package.json")
}
pub fn validate(p: &Package) -> Result<()> {
    ensure!(p.protocol == PROTOCOL, "unsupported_component_protocol");
    ensure!(contract::valid_id(&p.name), "invalid_component_name");
    version(&p.version)?;
    ensure!(
        !p.description.trim().is_empty() && p.description.len() <= 1024,
        "invalid_component_description"
    );
    ensure!(
        p.ui_contract == "miy.ui/v1alpha1",
        "unsupported_component_ui_contract"
    );
    ensure!(
        !p.bundle_versions.is_empty() && p.bundle_versions.len() <= 32,
        "invalid_component_compatibility"
    );
    for v in &p.bundle_versions {
        version(v)?;
    }
    ensure!(
        p.files.len() <= 128
            && p.files.contains_key("README.md")
            && (p.files.contains_key("index.tsx") || p.files.contains_key("index.ts")),
        "component_entry_and_readme_required"
    );
    for (path, content) in &p.files {
        ensure!(valid_file(path), "invalid_component_path: {path}");
        ensure!(
            !p.files
                .keys()
                .any(|other| other.starts_with(&format!("{path}/"))),
            "component_file_directory_conflict"
        );
        ensure!(content.len() <= 512 * 1024, "component_file_too_large");
    }
    ensure!(
        serde_json::to_vec(p)?.len() <= MAX_PACKAGE,
        "component_package_too_large"
    );
    Ok(())
}
pub fn compatible(p: &Package) -> bool {
    p.bundle_versions
        .iter()
        .any(|v| crate::bundle::component_compatible(v))
        && p.ui_contract == "miy.ui/v1alpha1"
}
pub fn release(p: Package) -> Result<Release> {
    validate(&p)?;
    Ok(Release {
        sha256: contract::hash(serde_json::to_vec(&p)?),
        package: p,
    })
}
pub fn verify_release(r: &Release) -> Result<()> {
    ensure!(
        release(r.package.clone())?.sha256 == r.sha256,
        "component_digest_mismatch"
    );
    Ok(())
}
pub fn read_source(root: &FsPath) -> Result<Package> {
    ensure!(
        std::fs::symlink_metadata(root)?.is_dir(),
        "invalid_component_source"
    );
    let metadata = root.join("component.json");
    ensure!(
        std::fs::symlink_metadata(&metadata)?.is_file(),
        "invalid_component_descriptor"
    );
    ensure!(
        std::fs::metadata(&metadata)?.len() <= MAX_PACKAGE as u64,
        "component_package_too_large"
    );
    let source: Source = serde_json::from_slice(&std::fs::read(metadata)?)?;
    ensure!(source.files.len() <= 128, "too_many_component_files");
    let mut files = BTreeMap::new();
    for name in source.files {
        ensure!(valid_file(&name), "invalid_component_path");
        let mut path = root.to_path_buf();
        for segment in name.split('/') {
            path.push(segment);
            ensure!(
                !std::fs::symlink_metadata(&path)?.is_symlink(),
                "component_symlink_forbidden"
            );
        }
        ensure!(
            std::fs::metadata(&path)?.len() <= 512 * 1024,
            "component_file_too_large"
        );
        ensure!(
            files.insert(name, std::fs::read_to_string(path)?).is_none(),
            "duplicate_component_file"
        );
    }
    let p = Package {
        protocol: source.protocol,
        name: source.name,
        version: source.version,
        description: source.description,
        ui_contract: source.ui_contract,
        bundle_versions: source.bundle_versions,
        files,
    };
    validate(&p)?;
    Ok(p)
}
pub async fn store(pool: &sqlx::PgPool, publisher: &str, p: Package) -> ApiResult<Value> {
    let r = release(p).map_err(|e| ApiError::bad_owned(e.to_string()))?;
    sqlx::query("INSERT INTO component_releases(name,version,digest,package,publisher) VALUES($1,$2,$3,$4,$5) ON CONFLICT(name,version) DO NOTHING")
        .bind(&r.package.name).bind(&r.package.version).bind(&r.sha256).bind(serde_json::to_value(&r.package)?).bind(publisher).execute(pool).await?;
    let existing: String =
        sqlx::query_scalar("SELECT digest FROM component_releases WHERE name=$1 AND version=$2")
            .bind(&r.package.name)
            .bind(&r.package.version)
            .fetch_one(pool)
            .await?;
    if existing != r.sha256 {
        return Err(ApiError::conflict("immutable_component_version"));
    }
    Ok(json!({"name":r.package.name,"version":r.package.version,"sha256":r.sha256}))
}
pub async fn catalog_data(pool: &sqlx::PgPool) -> ApiResult<Value> {
    let entries: Vec<Value> = sqlx::query_scalar("SELECT (package - 'files') || jsonb_build_object('sha256',digest,'publishedAt',published_at) FROM component_releases ORDER BY name, published_at")
        .fetch_all(pool).await?;
    Ok(json!(entries))
}
pub async fn get_data(pool: &sqlx::PgPool, name: &str, v: &str) -> ApiResult<Value> {
    if !contract::valid_id(name) || version(v).is_err() {
        return Err(ApiError::bad("invalid_component_reference"));
    }
    let data: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('sha256',digest,'package',package) FROM component_releases WHERE name=$1 AND version=$2")
        .bind(name).bind(v).fetch_optional(pool).await?;
    data.ok_or_else(ApiError::not_found)
}
pub fn guidance(release: Value) -> Value {
    let p = &release["package"];
    json!({"name":p["name"],"version":p["version"],"sha256":release["sha256"],"description":p["description"],"bundleVersions":p["bundleVersions"],"uiContract":p["uiContract"],"usage":p["files"]["README.md"],"installCommand":format!("miy app components add {} --version {}",p["name"].as_str().unwrap_or(""),p["version"].as_str().unwrap_or(""))})
}
pub async fn catalog(S(s): S<State>) -> ApiResult<Json<Value>> {
    Ok(Json(catalog_data(&s.pool).await?))
}
pub async fn get(
    S(s): S<State>,
    Path((name, v)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    Ok(Json(get_data(&s.pool, &name, &v).await?))
}
pub async fn publish(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Json(package): Json<Package>,
) -> ApiResult<Json<Value>> {
    crate::domain::admin(&s, &p).await?;
    Ok(Json(store(&s.pool, &p.id, package).await?))
}
pub async fn seed(pool: &sqlx::PgPool) -> Result<()> {
    let root = std::env::var("MIY_COMPONENT_SEED")
        .unwrap_or_else(|_| "registry/components/ui/1.0.1".into());
    let p = read_source(FsPath::new(&root)).context("read_component_seed")?;
    store(pool, "bootstrap", p)
        .await
        .map_err(|e| anyhow::anyhow!("component_seed: {}", e.code))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_paths_versions_and_packages_are_rejected() {
        for path in [
            "../escape.tsx",
            "/escape.tsx",
            "a/../b.ts",
            "a\\b.ts",
            "a//b.ts",
            "a/package.json",
            ".hidden.ts",
            "script.sh",
        ] {
            assert!(!valid_file(path), "{path}");
        }
        for v in ["1", "1.0", "1.0.0.1", "01.0.0", "1.0.-1", "1.0.0-beta"] {
            assert!(version(v).is_err());
        }
        assert!(version("1.10.2").unwrap() > version("1.9.9").unwrap());
        let mut p = read_source(
            FsPath::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../registry/components/ui/1.0.0")
                .as_path(),
        )
        .unwrap();
        let mut r = release(p.clone()).unwrap();
        r.package.files.insert("index.tsx".into(), "changed".into());
        assert!(verify_release(&r).is_err());
        p.files.insert("../escape.tsx".into(), "bad".into());
        assert!(validate(&p).is_err());
    }
}
