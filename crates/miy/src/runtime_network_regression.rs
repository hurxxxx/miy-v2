//! Disposable network regression. Never targets an unlabeled platform database.
use super::*;
use anyhow::ensure;
use sqlx::Connection;

async fn gateway(database: &str) -> Result<String> {
    let routes = docker(&["exec", database, "cat", "/proc/net/route"]).await?;
    let gateway = routes
        .lines()
        .skip(1)
        .find_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() >= 3 && fields[1] == "00000000").then(|| fields[2].to_owned())
        })
        .context("missing_fixture_default_gateway")?;
    Ok(gateway)
}

async fn connect_after_fixture_change(url: &str) -> Result<sqlx::PgConnection> {
    // Only fixture setup and undoing the intentionally unsafe control may wait
    // for Docker to restore its published-port proxy. The positive assertion
    // below reuses one existing socket and never reconnects.
    for _ in 0..100 {
        if let Ok(connection) = sqlx::PgConnection::connect(url).await {
            return Ok(connection);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("fixture_published_port_not_ready")
}

#[tokio::test]
#[ignore = "requires scripts/check-database-network.py isolated fixture"]
async fn database_attachment_retains_published_connections() -> Result<()> {
    // Bound every positive socket probe too: a gateway regression can silently
    // blackhole packets instead of returning an immediate connection reset.
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        run_database_attachment_retains_published_connections(),
    )
    .await
    .context("database_network_regression_timeout")?
}

async fn run_database_attachment_retains_published_connections() -> Result<()> {
    let root = PathBuf::from(std::env::var("MIY_GATEWAY_TEST_ROOT")?);
    let config: Value = serde_json::from_slice(&std::fs::read(root.join("fixture.json"))?)?;
    let prefix = config["prefix"]
        .as_str()
        .context("missing_fixture_prefix")?;
    let database = config["database"]
        .as_str()
        .context("missing_fixture_database")?;
    let control = config["controlNetwork"]
        .as_str()
        .context("missing_control_network")?;
    let installation: Uuid = serde_json::from_value(config["installation"].clone())?;
    ensure!(
        prefix.starts_with("miy-gateway-test-")
            && std::env::var("MIY_DATABASE_CONTAINER")? == database
    );
    ensure!(
        inspect(database).await?["Config"]["Labels"]["org.miy.gateway-test"] == prefix,
        "refusing_unowned_gateway_fixture"
    );
    for network in [
        control.to_owned(),
        format!("miy-runtime-{}", installation.simple()),
    ] {
        let metadata: Value =
            serde_json::from_str(&docker(&["network", "inspect", &network]).await?)?;
        ensure!(
            metadata[0]["Labels"]["org.miy.gateway-test"] == prefix,
            "refusing_unowned_fixture_network"
        );
    }
    let connection = std::env::var("DATABASE_URL")?;
    let mut legacy = connect_after_fixture_change(&connection).await?;
    let original_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut legacy)
        .await?;
    let original_gateway = gateway(database).await?;
    // Negative control reproduces the old command on this disposable database.
    docker(&[
        "network", "connect", "--alias", "postgres", control, database,
    ])
    .await?;
    let switched_gateway = gateway(database).await?;
    ensure!(
        switched_gateway != original_gateway,
        "negative_control_did_not_change_gateway"
    );
    let legacy_query = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()").fetch_one(&mut legacy),
    )
    .await;
    let lost = !matches!(legacy_query, Ok(Ok(pid)) if pid == original_pid);
    ensure!(lost, "negative_control_did_not_reproduce_connection_loss");
    drop(legacy);
    docker(&["network", "disconnect", control, database]).await?;
    ensure!(
        gateway(database).await? == original_gateway,
        "fixture_primary_gateway_not_restored"
    );
    println!(
        "PASS database-network: default-priority attachment reproduces the published-port connection loss"
    );

    let mut held = connect_after_fixture_change(&connection).await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut held)
        .await?;
    sqlx::query("SELECT pg_advisory_lock(73490999)")
        .execute(&mut held)
        .await?;
    sqlx::query("CREATE TEMP TABLE retained_lock_probe(value text)")
        .execute(&mut held)
        .await?;
    sqlx::query("INSERT INTO retained_lock_probe VALUES('same connection')")
        .execute(&mut held)
        .await?;
    let network = installation_network(installation).await?;
    ensure!(
        gateway(database).await? == original_gateway,
        "database_default_gateway_changed"
    );
    ensure!(inspect(database).await?["NetworkSettings"]["Networks"][&network]["GwPriority"] == -1);
    let after_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut held)
        .await?;
    let retained: String = sqlx::query_scalar("SELECT value FROM retained_lock_probe")
        .fetch_one(&mut held)
        .await?;
    let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock(73490999)")
        .fetch_one(&mut held)
        .await?;
    ensure!(pid == after_pid && retained == "same connection" && unlocked);
    println!(
        "PASS database-network: production attachment preserves gateway, backend PID, temporary data and advisory lock"
    );

    ensure!(installation_network(installation).await? == network);
    ensure!(gateway(database).await? == original_gateway);
    ensure!(
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
            .fetch_one(&mut held)
            .await?
            == pid
    );
    println!(
        "PASS database-network: repeated attachment leaves the existing endpoint and connection unchanged"
    );
    std::fs::write(
        root.join("results.json"),
        serde_json::to_vec_pretty(&json!({
            "negativeControlConnectionLost":lost,"initialGateway":original_gateway,"unsafeGateway":switched_gateway,
            "safeBackendPid":pid,"sameBackendPid":pid==after_pid,"advisoryUnlockSucceeded":unlocked,
            "attachmentPriority":-1,"observations":3
        }))?,
    )?;
    Ok(())
}
