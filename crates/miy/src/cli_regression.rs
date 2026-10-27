//! Real executable against the disposable HTTP/PostgreSQL regression fixture.
use crate::State;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
struct Cli {
    binary: PathBuf,
    root: PathBuf,
    config: PathBuf,
    context_env: Option<String>,
}
impl Cli {
    async fn output(&self, args: &[&str]) -> Result<std::process::Output> {
        let mut command = tokio::process::Command::new(&self.binary);
        if !args.contains(&"--output") {
            command.args(["--output", "json"]);
        }
        command
            .arg("--no-input")
            .args(args)
            .current_dir(&self.root)
            .env("MIY_CONFIG_DIR", &self.config)
            .env("MIY_CREDENTIAL_BACKEND", "file")
            .env("RUST_LOG", "off")
            .env_remove("MIY_CONTEXT")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(context) = &self.context_env {
            command.env("MIY_CONTEXT", context);
        }
        let output = tokio::time::timeout(Duration::from_secs(40), command.output()).await??;
        for bytes in [&output.stdout, &output.stderr] {
            let text = String::from_utf8_lossy(bytes);
            ensure!(
                !text.contains("miy_access_")
                    && !text.contains("miy_refresh_")
                    && !text.contains("miy_device_"),
                "CLI exposed an authentication secret"
            );
        }
        Ok(output)
    }
    async fn call(&self, args: &[&str], expected: i32) -> Result<Value> {
        let output = self.output(args).await?;
        let value: Value = serde_json::from_slice(&output.stdout)
            .context("CLI did not produce one JSON envelope")?;
        ensure!(
            output.status.code() == Some(expected),
            "CLI {:?} exit mismatch expected {expected}, received {:?}, code={}",
            args,
            output.status.code(),
            value["error"]["code"]
        );
        ensure!(
            value["schemaVersion"] == 1 && value["ok"] == (expected == 0),
            "CLI envelope contract mismatch"
        );
        ensure!(
            output.stderr.is_empty(),
            "Unexpected diagnostic output for fixture CLI command"
        );
        Ok(value)
    }
}
async fn count(s: &State, table: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&s.pool)
        .await?)
}
fn expire_saved_access(root: &Path) -> Result<()> {
    for entry in std::fs::read_dir(root.join("credentials"))? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let mut record: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let Some(text) = record["value"].as_str() else {
            continue;
        };
        let Ok(mut credential) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if credential.get("accessToken").is_some() {
            credential["expiresAt"] = json!(chrono::Utc::now().timestamp() - 1);
            record["value"] = json!(serde_json::to_string(&credential)?);
            std::fs::write(&path, serde_json::to_vec(&record)?)?;
            return Ok(());
        }
    }
    anyhow::bail!("Synthetic file credential was not found")
}
pub async fn run(s: &State, base: &str) -> Result<Vec<String>> {
    ensure!(
        sqlx::query_scalar::<_, String>("SELECT current_database()")
            .fetch_one(&s.pool)
            .await?
            == "miy_audit_core",
        "Disposable regression database required"
    );
    let binary = PathBuf::from(
        std::env::var("MIY_CLI_TEST_BINARY").context("runner must build the actual CLI")?,
    );
    ensure!(binary.is_file(), "CLI binary missing");
    let temp = tempfile::tempdir()?;
    let cli = Cli {
        binary,
        root: temp.path().into(),
        config: temp.path().join("private-config"),
        context_env: None,
    };
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..12];
    let user = format!("cli-{suffix}");
    let guest = format!("guest-{suffix}");
    let app = format!("cli-app-{suffix}");
    sqlx::query(
        "INSERT INTO users(id,name,admin,can_develop) VALUES($1,$1,true,true),($2,$2,false,false)",
    )
    .bind(&user)
    .bind(&guest)
    .execute(&s.pool)
    .await?;
    sqlx::query("UPDATE users SET name='admin' WHERE id=$1")
        .bind(&user)
        .execute(&s.pool)
        .await?;
    let mut passed = vec![];
    cli.call(&["context", "add", "audit", "--url", base], 0)
        .await?;
    cli.call(&["auth", "login", "--account", "admin"], 0)
        .await?;
    let status = cli.call(&["auth", "status"], 0).await?;
    ensure!(
        status["data"].to_string().contains(&user),
        "CLI identity does not match explicit fixture account"
    );
    ensure!(
        !cli.root.join(".miy/session.json").exists(),
        "CLI saved a project credential"
    );
    let before:i64=sqlx::query_scalar("SELECT count(*) FROM cli_refresh_tokens r JOIN cli_sessions s ON s.id=r.session_id WHERE s.user_id=$1 AND r.used_at IS NOT NULL").bind(&user).fetch_one(&s.pool).await?;
    expire_saved_access(&cli.config)?;
    let (first, second) = tokio::join!(
        cli.call(&["auth", "status"], 0),
        cli.call(&["auth", "status"], 0)
    );
    first?;
    second?;
    let rotated:i64=sqlx::query_scalar("SELECT count(*) FROM cli_refresh_tokens r JOIN cli_sessions s ON s.id=r.session_id WHERE s.user_id=$1 AND r.used_at IS NOT NULL").bind(&user).fetch_one(&s.pool).await?;
    ensure!(
        rotated == before + 1,
        "Concurrent CLI calls did not serialize refresh rotation"
    );
    passed.push("real CLI context login, private global credentials and two concurrent processes rotate once".into());

    let users = cli
        .call(
            &[
                "user",
                "list",
                "--limit",
                "1",
                "--fields",
                "users.id,nextCursor",
            ],
            0,
        )
        .await?;
    ensure!(
        users["data"]["users"]
            .as_array()
            .is_some_and(|v| v.len() == 1),
        "CLI pagination did not bound results"
    );
    ensure!(
        users["data"]["users"][0]
            .as_object()
            .is_some_and(|v| v.len() == 1),
        "Field projection leaked unselected columns"
    );
    if let Some(cursor) = users["data"]["nextCursor"].as_str() {
        cli.call(&["user", "list", "--limit", "1", "--cursor", cursor], 0)
            .await?;
    }
    cli.call(&["--app", "audit-runtime", "app", "show"], 0)
        .await?;
    cli.call(&["doctor", "--for", "operate"], 0).await?;
    let project = cli.root.join("한글 앱 with spaces");
    cli.call(
        &[
            "app",
            "init",
            project.to_str().unwrap(),
            "--id",
            &app,
            "--name",
            "회귀 앱",
        ],
        0,
    )
    .await?;
    cli.call(&["context", "add", "alternate", "--url", base], 0)
        .await?;
    cli.call(&["context", "use", "alternate"], 0).await?;
    cli.call(
        &["--project", project.to_str().unwrap(), "auth", "status"],
        0,
    )
    .await?;
    cli.call(
        &[
            "--context",
            "alternate",
            "--project",
            project.to_str().unwrap(),
            "auth",
            "status",
        ],
        3,
    )
    .await?;
    let environment_cli = Cli {
        binary: cli.binary.clone(),
        root: cli.root.clone(),
        config: cli.config.clone(),
        context_env: Some("alternate".into()),
    };
    environment_cli
        .call(
            &["--project", project.to_str().unwrap(), "auth", "status"],
            3,
        )
        .await?;
    environment_cli
        .call(
            &[
                "--context",
                "audit",
                "--project",
                project.to_str().unwrap(),
                "auth",
                "status",
            ],
            0,
        )
        .await?;
    cli.call(&["context", "use", "audit"], 0).await?;
    passed.push("real CLI context selection follows flag, environment, project binding and active context in order".into());
    let registration = cli
        .call(
            &["--project", project.to_str().unwrap(), "app", "register"],
            0,
        )
        .await?;
    let id = registration["data"]["id"]
        .as_str()
        .context("registration ID missing")?;
    cli.call(&["registration", "approve", id], 0).await?;
    let input = cli.root.join("grant.json");
    std::fs::write(
        &input,
        serde_json::to_vec(
            &json!({"userId":guest,"appId":app,"environment":"dev","permission":"use"}),
        )?,
    )?;
    let source = format!("@{}", input.display());
    cli.call(&["grant", "set", "--input", &source], 0).await?;
    cli.call(&["--app", &app, "grant", "list", "--limit", "1"], 0)
        .await?;
    cli.call(&["grant", "revoke", "--input", &source], 0)
        .await?;
    std::fs::write(
        &input,
        serde_json::to_vec(
            &json!({"userId":guest,"appId":app,"environment":"dev","permission":"use","admin":true}),
        )?,
    )?;
    cli.call(&["grant", "set", "--input", &source], 2).await?;
    cli.call(&["grant", "set", "--user", &guest, "--input", &source], 2)
        .await?;
    cli.call(&["--app", &app, "app", "accept", "--input", &source], 2)
        .await?;
    passed.push("real CLI project-free pagination/projection, project binding, registration approval and strict typed grant input".into());

    let jobs = count(s, "runtime_jobs").await?;
    let uploads = count(s, "image_uploads").await?;
    let plan = cli
        .call(
            &[
                "--context",
                "audit",
                "--app",
                "audit-runtime",
                "app",
                "remove",
                "--environment",
                "dev",
                "--dry-run",
            ],
            0,
        )
        .await?;
    ensure!(
        plan["data"]["preservesDatabase"] == true,
        "Dry-run omitted preservation contract"
    );
    cli.call(
        &[
            "--app",
            "audit-runtime",
            "app",
            "remove",
            "--environment",
            "dev",
            "--yes",
        ],
        2,
    )
    .await?;
    cli.call(
        &[
            "--context",
            "audit",
            "--app",
            "audit-runtime",
            "app",
            "stop",
            "--environment",
            "prod",
        ],
        2,
    )
    .await?;
    cli.call(
        &[
            "--context",
            "audit",
            "--app",
            "audit-runtime",
            "app",
            "remove",
            "--environment",
            "dev",
        ],
        2,
    )
    .await?;
    ensure!(
        count(s, "runtime_jobs").await? == jobs && count(s, "image_uploads").await? == uploads,
        "Dry-run or missing target/confirmation created remote work"
    );
    passed.push("real CLI dry-run and missing explicit targets/confirmation create no upload or runtime job".into());

    let submitted = cli
        .call(&["--app", "audit-runtime", "app", "stop", "--no-wait"], 0)
        .await?;
    let id = submitted["data"]["id"]
        .as_str()
        .context("submitted job ID missing")?;
    let request = submitted["data"]["requestId"]
        .as_str()
        .context("submitted request ID missing")?;
    let timeout = cli.call(&["job", "watch", id, "--timeout", "1"], 8).await?;
    ensure!(
        timeout["error"]["details"]["jobId"] == id,
        "Observation timeout lost job ID"
    );
    let before = count(s, "runtime_jobs").await?;
    // Simulate losing the accepted response after the durable pre-send journal.
    let journal = cli.config.join("journal").join(format!("{request}.json"));
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&journal)?)?;
    saved["jobId"] = Value::Null;
    saved["submissionStatus"] = json!("unknown");
    std::fs::write(&journal, serde_json::to_vec(&saved)?)?;
    cli.call(&["--app", &app, "job", "resume", request, "--no-wait"], 6)
        .await?;
    cli.call(
        &[
            "job",
            "resume",
            request,
            "--environment",
            "prod",
            "--no-wait",
        ],
        6,
    )
    .await?;
    ensure!(
        count(s, "runtime_jobs").await? == before,
        "Mismatched resume target created a job"
    );
    cli.call(&["job", "resume", request, "--no-wait"], 0)
        .await?;
    ensure!(
        count(s, "runtime_jobs").await? == before,
        "Resume created a duplicate job"
    );
    crate::runtime::tick(&s.pool).await?;
    let watched = cli
        .output(&["job", "watch", id, "--output", "jsonl", "--timeout", "10"])
        .await?;
    ensure!(watched.status.success(), "Completed job JSONL watch failed");
    let lines: Vec<Value> = String::from_utf8(watched.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        lines.len() >= 2
            && lines
                .iter()
                .all(|v| v["schemaVersion"] == 1 && v["ok"] == true),
        "JSONL event envelopes invalid"
    );
    let start = cli
        .call(&["--app", "audit-runtime", "app", "start", "--no-wait"], 0)
        .await?;
    crate::runtime::tick(&s.pool).await?;
    cli.call(
        &[
            "job",
            "watch",
            start["data"]["id"].as_str().unwrap(),
            "--timeout",
            "10",
        ],
        0,
    )
    .await?;
    passed.push("real CLI no-wait journal, timeout without cancellation, lost-ack request-ID resume without duplication and terminal JSONL events".into());
    cli.call(&["auth", "logout"], 0).await?;
    cli.call(&["auth", "status"], 3).await?;
    cli.call(&["context", "remove", "audit"], 2).await?;
    cli.call(&["context", "remove", "audit", "--yes"], 0)
        .await?;
    passed.push(
        "real CLI remote logout, local credential deletion and explicit context removal".into(),
    );
    Ok(passed)
}
