//! Publisher-controlled app identities and immutable source pins. Only the
//! local host command imports a catalog; app manifests are not catalog input.
use crate::{component_contract, contract, source_archive};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};
use std::{collections::BTreeSet, path::Path, sync::LazyLock};

const BUNDLED: &[u8] = include_bytes!("../../../registry/apps/catalog.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    apps: Vec<Product>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Product {
    app_id: String,
    name: String,
    default_install: bool,
    releases: Vec<Version>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Version {
    version: String,
    source_commit: String,
    archive_sha256: String,
}

impl Catalog {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let mut catalog: Self = serde_json::from_slice(bytes).context("invalid_catalog_json")?;
        ensure!(catalog.schema_version == 1, "unsupported_catalog_schema");
        // All imports acquire app locks in the same order, including overlapping catalogs.
        catalog
            .apps
            .sort_unstable_by(|a, b| a.app_id.cmp(&b.app_id));
        ensure!(
            catalog
                .apps
                .windows(2)
                .all(|apps| apps[0].app_id != apps[1].app_id),
            "duplicate_catalog_app"
        );
        for app in &catalog.apps {
            ensure!(
                contract::valid_id(&app.app_id)
                    && !app.name.trim().is_empty()
                    && app.name.len() <= 120
                    && !app.name.chars().any(char::is_control),
                "invalid_catalog_app"
            );
            let mut versions = BTreeSet::new();
            for release in &app.releases {
                ensure!(
                    component_contract::version(&release.version).is_ok()
                        && source_archive::valid_commit(&release.source_commit)
                        && crate::helper_image::valid_hash(&release.archive_sha256, 64),
                    "invalid_catalog_release: {}",
                    app.app_id
                );
                ensure!(
                    versions.insert(&release.version),
                    "duplicate_catalog_version: {} {}",
                    app.app_id,
                    release.version
                );
            }
        }
        Ok(catalog)
    }
}

/// Reserve shipped product IDs from the first request on a new server. The
/// import also accepts later publisher additions independently of Core releases.
pub(crate) fn bundled_id(app: &str) -> bool {
    static CATALOG: LazyLock<Catalog> =
        LazyLock::new(|| Catalog::parse(BUNDLED).expect("invalid bundled catalog"));
    CATALOG
        .apps
        .binary_search_by(|product| product.app_id.as_str().cmp(app))
        .is_ok()
}

/// Registration, direct creation and import must agree on a single owner even
/// while there is no apps row to lock yet. The caller holds a transaction.
pub(crate) async fn lock_app(conn: &mut PgConnection, app: &str) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('app-id:' || $1,0))")
        .bind(app)
        .execute(conn)
        .await?;
    Ok(())
}

/// Resolve an already published release on this site. A successor can reuse its
/// image without obtaining the original developer's private build receipt.
pub(crate) async fn release_image(
    pool: &PgPool,
    app: &str,
    release: uuid::Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT image_id FROM releases WHERE id=$1 AND app_id=$2 AND official_version=version AND image_id IS NOT NULL")
        .bind(release)
        .bind(app)
        .fetch_optional(pool)
        .await
}

pub(crate) async fn sync_file(pool: &PgPool, path: &Path) -> Result<()> {
    let catalog = Catalog::parse(&std::fs::read(path).context("read_official_catalog")?)?;
    sync(pool, &catalog).await?;
    println!(
        "{}",
        serde_json::json!({
            "apps": catalog.apps.len(),
            "releasePins": catalog.apps.iter().map(|app| app.releases.len()).sum::<usize>(),
        })
    );
    Ok(())
}

async fn sync(pool: &PgPool, catalog: &Catalog) -> Result<()> {
    let mut tx = pool.begin().await?;
    for app in &catalog.apps {
        lock_app(&mut tx, &app.app_id).await?;
        let official: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM official_apps WHERE app_id=$1)")
                .bind(&app.app_id)
                .fetch_one(&mut *tx)
                .await?;
        if !official {
            let claimed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM apps WHERE id=$1)
                     OR EXISTS(SELECT 1 FROM app_registrations WHERE app_id=$1)
                     OR EXISTS(SELECT 1 FROM grants WHERE app_id=$1)",
            )
            .bind(&app.app_id)
            .fetch_one(&mut *tx)
            .await?;
            ensure!(!claimed, "catalog_app_id_conflict: {}", app.app_id);
            // A site's administrator can later customize the display name.
            sqlx::query("INSERT INTO apps(id,name) VALUES ($1,$2)")
                .bind(&app.app_id)
                .bind(app.name.trim())
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO official_apps(app_id,default_install) VALUES ($1,$2)
             ON CONFLICT (app_id) DO UPDATE SET default_install=EXCLUDED.default_install
             WHERE official_apps.default_install<>EXCLUDED.default_install",
        )
        .bind(&app.app_id)
        .bind(app.default_install)
        .execute(&mut *tx)
        .await?;
        for release in &app.releases {
            let inserted = sqlx::query(
                "INSERT INTO official_app_versions(app_id,version,source_commit,archive_sha256)
                 VALUES ($1,$2,$3,$4) ON CONFLICT (app_id,version) DO NOTHING",
            )
            .bind(&app.app_id)
            .bind(&release.version)
            .bind(&release.source_commit)
            .bind(&release.archive_sha256)
            .execute(&mut *tx)
            .await?;
            if inserted.rows_affected() == 0 {
                let identical: bool = sqlx::query_scalar(
                    "SELECT source_commit=$3 AND archive_sha256=$4
                     FROM official_app_versions WHERE app_id=$1 AND version=$2",
                )
                .bind(&app.app_id)
                .bind(&release.version)
                .bind(&release.source_commit)
                .bind(&release.archive_sha256)
                .fetch_one(&mut *tx)
                .await?;
                ensure!(
                    identical,
                    "catalog_version_conflict: {} {}",
                    app.app_id,
                    release.version
                );
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
pub(crate) use tests::regression;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn product() -> Value {
        json!({
            "appId":"planner", "name":"플래너", "defaultInstall":true,
            "releases":[{"version":"1.2.3","sourceCommit":"a".repeat(40),"archiveSha256":"b".repeat(64)}],
        })
    }

    fn parse(apps: Value) -> Result<Catalog> {
        Catalog::parse(&serde_json::to_vec(
            &json!({"schemaVersion":1,"apps":apps}),
        )?)
    }

    /// Called only by the disposable server-build PostgreSQL fixture.
    pub(crate) async fn regression(pool: &PgPool) -> Result<()> {
        let mut app = product();
        app["appId"] = json!("catalog-check-planner");
        let first = parse(json!([app]))?;
        let (a, b) = tokio::join!(sync(pool, &first), sync(pool, &first));
        a?;
        b?;
        sqlx::raw_sql(
            "INSERT INTO users(id,name) VALUES ('catalog-check-user','Employee');
             UPDATE apps SET name='Local label' WHERE id='catalog-check-planner';
             INSERT INTO app_controls(app_id,disabled) VALUES ('catalog-check-planner',true);
             INSERT INTO grants(user_id,app_id,environment,permission)
                 VALUES ('catalog-check-user','catalog-check-planner','prod','use');",
        )
        .execute(pool)
        .await?;
        app["defaultInstall"] = json!(false);
        app["releases"][0]["version"] = json!("1.2.4");
        app["releases"][0]["sourceCommit"] = json!("c".repeat(40));
        sync(pool, &parse(json!([app]))?).await?;
        let state: (String, bool, bool, i64, i64) = sqlx::query_as(
            "SELECT a.name, o.default_install, c.disabled,
                    (SELECT count(*) FROM grants WHERE app_id=a.id),
                    (SELECT count(*) FROM official_app_versions WHERE app_id=a.id)
             FROM apps a JOIN official_apps o ON o.app_id=a.id
             JOIN app_controls c ON c.app_id=a.id WHERE a.id='catalog-check-planner'",
        )
        .fetch_one(pool)
        .await?;
        ensure!(
            state == ("Local label".into(), false, true, 1, 2),
            "import changed site state or lost a pin"
        );

        let mut new = product();
        new["appId"] = json!("catalog-check-first"); // sorted and processed before the conflict
        app["defaultInstall"] = json!(true);
        app["releases"][0]["archiveSha256"] = json!("d".repeat(64));
        let error = sync(pool, &parse(json!([new, app]))?).await.unwrap_err();
        ensure!(error.to_string().starts_with("catalog_version_conflict"));
        let rollback: (bool, bool) = sqlx::query_as(
            "SELECT default_install, EXISTS(SELECT 1 FROM apps WHERE id='catalog-check-first')
             FROM official_apps WHERE app_id='catalog-check-planner'",
        )
        .fetch_one(pool)
        .await?;
        ensure!(
            rollback == (false, false),
            "partial catalog import committed"
        );

        sqlx::raw_sql(
            "INSERT INTO apps(id,name) VALUES ('catalog-check-org','Organization app');
             INSERT INTO app_registrations(id,app_id,name,actor)
                 VALUES (gen_random_uuid(),'catalog-check-request','Requested','catalog-check-user');
             INSERT INTO grants(user_id,app_id,environment,permission)
                 VALUES ('catalog-check-user','catalog-check-pregrant','prod','use');"
        ).execute(pool).await?;
        for claimed in [
            "catalog-check-org",
            "catalog-check-request",
            "catalog-check-pregrant",
        ] {
            new["appId"] = json!(claimed);
            let error = sync(pool, &parse(json!([new]))?).await.unwrap_err();
            ensure!(error.to_string().starts_with("catalog_app_id_conflict"));
        }
        release_constraints(pool).await?;
        Ok(())
    }

    async fn release_constraints(pool: &PgPool) -> Result<()> {
        const INSERT_RELEASE: &str =
            "INSERT INTO releases(id,app_id,version,artifact_hash,manifest,image_id,source_commit,official_version)
             VALUES (gen_random_uuid(),'catalog-check-planner','1.2.3','artifact','{}',
                     'sha256:'||repeat('e',64),repeat('a',40),$1)";
        let build = sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO app_builds(id,app_id,actor,request_id,source_commit,archive_hash,
                                    size_bytes,platform,status,image_id,version)
             VALUES (gen_random_uuid(),'catalog-check-planner','catalog-check-user',gen_random_uuid(),
                     repeat('a',40),repeat('c',64),42,'linux/amd64','succeeded','sha256:'||repeat('e',64),'1.2.3')
             RETURNING id"
        ).fetch_one(pool).await?;
        let wrong_hash = sqlx::query(INSERT_RELEASE)
            .bind("1.2.3")
            .execute(pool)
            .await
            .unwrap_err();
        ensure!(
            wrong_hash
                .as_database_error()
                .is_some_and(|e| e.message() == "official_release_required")
        );

        sqlx::query("UPDATE app_builds SET archive_hash=repeat('b',64) WHERE id=$1")
            .bind(build)
            .execute(pool)
            .await?;
        let legacy = sqlx::query(INSERT_RELEASE)
            .bind(None::<&str>)
            .execute(pool)
            .await
            .unwrap_err();
        ensure!(
            legacy
                .as_database_error()
                .is_some_and(|e| e.message() == "official_release_required")
        );
        sqlx::query(INSERT_RELEASE)
            .bind("1.2.3")
            .execute(pool)
            .await?;

        let changed = sqlx::query(
            "UPDATE releases SET official_version=NULL WHERE app_id='catalog-check-planner'",
        )
        .execute(pool)
        .await
        .unwrap_err();
        ensure!(
            changed
                .as_database_error()
                .is_some_and(|e| e.message() == "official_release_immutable")
        );
        let changed = sqlx::query(
            "UPDATE official_app_versions SET source_commit=repeat('e',40) WHERE app_id='catalog-check-planner'"
        ).execute(pool).await.unwrap_err();
        ensure!(
            changed
                .as_database_error()
                .is_some_and(|e| e.message() == "official_version_immutable")
        );
        Ok(())
    }

    #[test]
    fn bundled_catalog_reserves_named_products() {
        let catalog = Catalog::parse(BUNDLED).unwrap();
        assert!(catalog.apps.iter().all(|app| bundled_id(&app.app_id)));
        assert!(bundled_id("planner") && bundled_id("pms"));
        assert!(!bundled_id("my-pms"));
    }

    #[test]
    fn rejects_ambiguous_products_and_invalid_source_pins() {
        let app = product();
        assert!(parse(json!([app])).is_ok());
        assert!(parse(json!([app, app])).is_err());
        let mut changed = app.clone();
        changed["releases"]
            .as_array_mut()
            .unwrap()
            .push(app["releases"][0].clone());
        assert!(parse(json!([changed])).is_err());
        for (key, value) in [
            ("version", json!("latest")),
            ("sourceCommit", json!("A".repeat(40))),
            ("archiveSha256", json!("0".repeat(63))),
            ("archiveSha256", json!("F".repeat(64))),
        ] {
            let mut changed = app.clone();
            changed["releases"][0][key] = value;
            assert!(parse(json!([changed])).is_err());
        }
        let mut changed = app;
        changed["publisher"] = json!("other");
        assert!(parse(json!([changed])).is_err());
        assert!(Catalog::parse(br#"{"schemaVersion":2,"apps":[]}"#).is_err());
    }
}
