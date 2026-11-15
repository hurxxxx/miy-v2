//! Immutable, authenticated component distribution; no component source is compiled into the CLI.
pub use crate::component_contract::{MAX_PACKAGE, Package, guidance};
use crate::component_contract::{read_source, release, version};
use crate::{
    State,
    auth::Principal,
    contract,
    http::{ApiError, ApiResult},
};
use anyhow::{Context, Result};
use axum::{
    Json,
    extract::{Path, State as S},
};
use serde_json::{Value, json};
use std::path::Path as FsPath;

pub async fn store(
    connection: &mut sqlx::PgConnection,
    publisher: &str,
    p: Package,
) -> ApiResult<Value> {
    let r = release(p).map_err(|e| ApiError::bad_owned(e.to_string()))?;
    sqlx::query("INSERT INTO component_releases(name,version,digest,package,publisher) VALUES($1,$2,$3,$4,$5) ON CONFLICT(name,version) DO NOTHING")
        .bind(&r.package.name).bind(&r.package.version).bind(&r.sha256).bind(serde_json::to_value(&r.package)?).bind(publisher).execute(&mut *connection).await?;
    let existing: String =
        sqlx::query_scalar("SELECT digest FROM component_releases WHERE name=$1 AND version=$2")
            .bind(&r.package.name)
            .bind(&r.package.version)
            .fetch_one(&mut *connection)
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
    let mut tx = crate::policy::Write::begin(&s.pool).await?;
    let stored = store(&mut tx, &p.id, package).await?;
    // The insert may wait for a concurrent publication of the same version.
    tx.decide_platform(&p, crate::policy::PlatformAction::Administer)
        .await?
        .commit()
        .await?;
    Ok(Json(stored))
}
/// Publish the shared components a new server starts with: every version under
/// `registry/components/ui`, oldest first, so projects created from any bundle
/// (each pins its own default version) can synchronize. `MIY_COMPONENT_SEED`
/// names one package directory instead.
pub async fn seed(pool: &sqlx::PgPool) -> Result<()> {
    let roots = match std::env::var_os("MIY_COMPONENT_SEED") {
        Some(root) => vec![std::path::PathBuf::from(root)],
        None => seed_versions(FsPath::new("registry/components/ui"))?,
    };
    let mut connection = pool.acquire().await?;
    for root in roots {
        let package = read_source(&root).context("read_component_seed")?;
        store(&mut connection, "bootstrap", package)
            .await
            .map_err(|e| anyhow::anyhow!("component_seed: {}", e.code))?;
    }
    Ok(())
}

fn seed_versions(root: &FsPath) -> Result<Vec<std::path::PathBuf>> {
    let mut versions = Vec::new();
    for entry in std::fs::read_dir(root).context("read_component_seed")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let parts: Option<Vec<u64>> = name
            .to_str()
            .map(|name| name.split('.').map(|part| part.parse().ok()).collect())
            .unwrap_or_default();
        match parts {
            Some(parts) if parts.len() == 3 => versions.push((parts, entry.path())),
            _ => anyhow::bail!("component_seed_version_directory"),
        }
    }
    anyhow::ensure!(!versions.is_empty(), "component_seed_empty");
    versions.sort();
    Ok(versions.into_iter().map(|(_, path)| path).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_holds_every_version_in_order_including_the_template_default() {
        let root = tempfile::tempdir().unwrap();
        for version in ["1.10.0", "1.9.0", "1.0.1"] {
            std::fs::create_dir(root.path().join(version)).unwrap();
        }
        let names: Vec<_> = seed_versions(root.path())
            .unwrap()
            .into_iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, ["1.0.1", "1.9.0", "1.10.0"]);
        std::fs::create_dir(root.path().join("draft")).unwrap();
        assert!(seed_versions(root.path()).is_err());

        // A project created from this source must find its default UI version
        // on a server seeded from the same source.
        let repository = FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let defaults: Value = serde_json::from_slice(
            &std::fs::read(repository.join("templates/rust-react/miy.components.json")).unwrap(),
        )
        .unwrap();
        let wanted = defaults["defaults"]["ui"].as_str().unwrap();
        let seeded = seed_versions(&repository.join("registry/components/ui")).unwrap();
        assert!(
            seeded
                .iter()
                .any(|path| path.file_name().unwrap() == wanted),
            "template default ui {wanted} is not seeded"
        );
        for path in seeded {
            let package = read_source(&path).unwrap();
            assert_eq!(package.name, "ui");
        }
    }
}
