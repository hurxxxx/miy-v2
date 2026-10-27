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
