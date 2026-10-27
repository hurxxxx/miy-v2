//! Remote command groups, checked runtime submission, and resumable observation.
use crate::{
    app_cli::{self, Session},
    cli_config,
    cli_error::{Category, CliError},
    cli_journal::{self, Entry},
    cli_output::{Outcome, OutputMode},
    runtime_contract::{Environment, Operation, PreflightInput, PreflightResult, RuntimeTarget},
};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Write, time::Duration};
use uuid::Uuid;

#[derive(Args)]
pub struct OperationArgs {
    /// Defaults to dev; prod and app removal require an explicitly supplied target.
    #[arg(long,value_parser=["dev","prod"])]
    pub environment: Option<String>,
    #[arg(long)]
    pub no_wait: bool,
    /// Inspect admission and expected state without uploading or submitting work.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand)]
pub enum RemoteAppCommand {
    /// List apps visible to the authenticated user.
    List,
    /// Show the selected app and visible installations.
    Show,
    /// Show server deployment state for --app, or the current project app.
    Status,
    /// Upload this project's immutable build and submit a checked dev deployment.
    Deploy {
        #[arg(long,value_parser=["dev"])]
        environment: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Promote the exact accepted release to the explicitly selected prod target.
    Promote {
        #[arg(long)]
        release: Uuid,
        #[arg(long,value_parser=["prod"])]
        environment: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        dry_run: bool,
    },
    Start(OperationArgs),
    Stop(OperationArgs),
    Restart(OperationArgs),
    /// Remove runtime containers after confirmation, preserving the application database.
    Remove(OperationArgs),
    /// Retrieve a bounded container log snapshot; rotation may omit older lines.
    Logs {
        /// Resume an existing bounded log request without creating another one.
        #[arg(long,conflicts_with_all=["tail","since"])]
        request: Option<Uuid>,
        #[arg(long,value_parser=["dev","prod"],default_value="dev")]
        environment: String,
        #[arg(long,default_value="100",value_parser=clap::value_parser!(u16).range(1..=1000))]
        tail: u16,
        #[arg(long)]
        since: Option<chrono::DateTime<chrono::Utc>>,
        #[arg(long,default_value="60",value_parser=clap::value_parser!(u64).range(1..=3600))]
        timeout: u64,
    },
}

#[derive(Subcommand)]
pub enum JobCommand {
    List {
        #[arg(long,value_parser=["dev","prod"])]
        environment: Option<String>,
        #[arg(long)]
        request_id: Option<Uuid>,
    },
    Get {
        id: Uuid,
    },
    /// Observe events after the saved/explicit cursor; stopping observation does not cancel work.
    Watch {
        id: Uuid,
        #[arg(long)]
        after: Option<String>,
        #[arg(long,default_value="600",value_parser=clap::value_parser!(u64).range(1..=86400))]
        timeout: u64,
    },
    /// Recover the original request or reconnect to an acknowledged job.
    Resume {
        id: Uuid,
        #[arg(long)]
        no_wait: bool,
        #[arg(long,value_parser=["dev","prod"])]
        environment: Option<String>,
    },
    /// Retry a typed transient failure after fresh preflight and target confirmation.
    Retry {
        id: Uuid,
        #[arg(long,value_parser=["dev","prod"])]
        environment: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum RegistrationCommand {
    List,
    Approve { id: Uuid },
    Reject { id: Uuid },
}
#[derive(Subcommand)]
pub enum UserCommand {
    List,
}
#[derive(Subcommand)]
pub enum GrantCommand {
    List,
    Set(GrantArgs),
    Revoke(GrantArgs),
}
#[derive(Args)]
pub struct GrantArgs {
    #[arg(long, conflicts_with = "input")]
    pub user: Option<String>,
    #[arg(long,value_parser=["use","develop","deploy"],conflicts_with="input")]
    pub permission: Option<String>,
    #[arg(long,value_parser=["dev","prod"],conflicts_with="input")]
    pub environment: Option<String>,
    /// Strict JSON {userId,appId?,environment,permission} from @file or stdin (-).
    #[arg(long)]
    pub input: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantInput {
    pub user_id: String,
    pub app_id: Option<String>,
    pub environment: Environment,
    pub permission: GrantPermission,
}
#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrantPermission {
    Use,
    Develop,
    Deploy,
}

fn data(value: Value) -> Result<Outcome> {
    Ok(Outcome::Data(value))
}

async fn require_features(session: &Session, features: &[&str]) -> Result<Value> {
    let value = app_cli::api(session, "/api/v1/capabilities", None)
        .await
        .map_err(|error| {
            if crate::cli_error::classify(&error).category == Category::NotFound {
                CliError::new(
                    Category::Environment,
                    "unsupported_capability",
                    "Upgrade the server to provide the capabilities required by this CLI.",
                )
                .into()
            } else {
                error
            }
        })?;
    let available = value["features"].as_array();
    let missing: Vec<_> = features
        .iter()
        .filter(|feature| {
            !available
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(**feature)))
        })
        .copied()
        .collect();
    if value["schemaVersion"] != 1 || value["cliContractVersion"] != 1 || !missing.is_empty() {
        return Err(CliError::new(
            Category::Environment,
            "unsupported_server_capability",
            "The server does not advertise the contract required for this command.",
        )
        .with_details(json!({"requiredFeatures":features,"missingFeatures":missing}))
        .into());
    }
    Ok(value)
}

fn selected_app() -> Result<String> {
    let app = if let Some(app) = &cli_config::options().app {
        app.clone()
    } else {
        app_cli::manifest()?.id
    };
    if !crate::contract::valid_id(&app) {
        return Err(CliError::new(
            Category::Usage,
            "invalid_app_id",
            "Use a valid app identifier.",
        )
        .into());
    }
    Ok(app)
}

fn query(path: &str, filters: &[(&str, Option<String>)]) -> String {
    let options = cli_config::options();
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair(
        "limit",
        &if options.limit == 0 {
            50
        } else {
            options.limit.min(200)
        }
        .to_string(),
    );
    if let Some(cursor) = &options.cursor {
        query.append_pair("cursor", cursor);
    }
    for (key, value) in filters {
        if let Some(value) = value {
            query.append_pair(key, value);
        }
    }
    format!("{path}?{}", query.finish())
}

pub async fn execute_user(_command: UserCommand) -> Result<Outcome> {
    let session = app_cli::session()?;
    require_features(&session, &["lists.pagination.v1"]).await?;
    data(app_cli::api(&session, &query("/api/v1/users", &[]), None).await?)
}
pub async fn execute_registration(command: RegistrationCommand) -> Result<Outcome> {
    let session = app_cli::session()?;
    require_features(&session, &["lists.pagination.v1"]).await?;
    let value = match command {
        RegistrationCommand::List => {
            app_cli::api(&session, &query("/api/v1/app-registrations", &[]), None).await?
        }
        RegistrationCommand::Approve { id } => {
            app_cli::api(
                &session,
                &format!("/api/v1/app-registrations/{id}/decision"),
                Some(json!({"approve":true})),
            )
            .await?
        }
        RegistrationCommand::Reject { id } => {
            app_cli::api(
                &session,
                &format!("/api/v1/app-registrations/{id}/decision"),
                Some(json!({"approve":false})),
            )
            .await?
        }
    };
    data(value)
}
pub async fn execute_grant(command: GrantCommand) -> Result<Outcome> {
    if matches!(command, GrantCommand::List) {
        let session = app_cli::session()?;
        require_features(&session, &["lists.pagination.v1"]).await?;
        return data(
            app_cli::api(
                &session,
                &query(
                    "/api/v1/grants",
                    &[("appId", cli_config::options().app.clone())],
                ),
                None,
            )
            .await?,
        );
    }
    let (args, enabled) = match command {
        GrantCommand::Set(args) => (args, true),
        GrantCommand::Revoke(args) => (args, false),
        GrantCommand::List => unreachable!(),
    };
    let input = if let Some(source) = args.input {
        crate::cli_input::read::<GrantInput>(&source)?
    } else {
        GrantInput {
            user_id: args.user.ok_or_else(|| {
                CliError::new(
                    Category::Usage,
                    "grant_user_required",
                    "Provide --user or a complete --input document.",
                )
            })?,
            app_id: None,
            environment: environment(Operation::Start, args.environment.as_deref())?,
            permission: match args.permission.as_deref().unwrap_or("use") {
                "use" => GrantPermission::Use,
                "develop" => GrantPermission::Develop,
                "deploy" => GrantPermission::Deploy,
                _ => {
                    return Err(CliError::new(
                        Category::Usage,
                        "invalid_grant_permission",
                        "Choose use, develop or deploy.",
                    )
                    .into());
                }
            },
        }
    };
    if input.app_id.is_some() && cli_config::options().app.is_some() {
        return Err(CliError::new(
            Category::Usage,
            "duplicate_input_field",
            "appId cannot be supplied by both --app and --input.",
        )
        .into());
    }
    let app = match input.app_id {
        Some(app) => app,
        None => selected_app()?,
    };
    if !crate::contract::valid_id(&app)
        || input.user_id.is_empty()
        || input.user_id.len() > 256
        || input.user_id.chars().any(char::is_control)
    {
        return Err(CliError::new(
            Category::Usage,
            "invalid_grant_input",
            "Grant input must specify a valid app, user, environment and permission.",
        )
        .into());
    }
    data(app_cli::api_method(&app_cli::session()?,"/api/v1/grants",reqwest::Method::PUT,Some(json!({"userId":input.user_id,"appId":app,"environment":input.environment,"permission":input.permission,"enabled":enabled}))).await?)
}

pub async fn execute_app(command: RemoteAppCommand) -> Result<Outcome> {
    match command {
        RemoteAppCommand::List => {
            let session = app_cli::session()?;
            require_features(&session, &["lists.pagination.v1"]).await?;
            data(app_cli::api(&session, &query("/api/v1/apps", &[]), None).await?)
        }
        RemoteAppCommand::Show | RemoteAppCommand::Status => {
            let app = selected_app()?;
            let session = app_cli::session()?;
            require_features(&session, &["lists.pagination.v1"]).await?;
            let mut value = app_cli::api(&session, &format!("/api/v1/apps/{app}"), None).await?;
            let local = app_cli::manifest().ok().filter(|m| m.id == app);
            let build = if local.is_some() {
                std::fs::read("dist/image.json")
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .filter(|v| v["appId"] == app)
            } else {
                None
            };
            if let Some(items) = value["installations"].as_array_mut() {
                for item in items {
                    item["url"] = json!(format!(
                        "{}/environments/{}/apps/{app}",
                        session.origin,
                        item["environment"].as_str().unwrap_or("dev")
                    ));
                    if let Some(ref manifest) = local {
                        item["localManifestName"] = json!(manifest.name);
                        item["displayNameDiffers"] = json!(item["name"] != manifest.name);
                    }
                    if let Some(ref build) = build {
                        item["localBuild"] = json!({"version":build["version"],"sourceCommit":build["sourceCommit"],"imageId":build["imageId"],"matchesRunningImage":build["imageId"]==item["imageId"]&&item["runtimeStatus"]=="running"});
                    }
                }
            }
            data(value)
        }
        RemoteAppCommand::Deploy {
            environment,
            no_wait,
            dry_run,
        } => {
            let (app, image) = local_build()?;
            mutate(
                Operation::Deploy,
                environment,
                Some(image),
                None,
                no_wait,
                dry_run,
                Some(app),
                None,
            )
            .await
        }
        RemoteAppCommand::Promote {
            release,
            environment,
            no_wait,
            dry_run,
        } => {
            mutate(
                Operation::Promote,
                environment,
                None,
                Some(release),
                no_wait,
                dry_run,
                None,
                None,
            )
            .await
        }
        RemoteAppCommand::Start(args) => lifecycle(Operation::Start, args).await,
        RemoteAppCommand::Stop(args) => lifecycle(Operation::Stop, args).await,
        RemoteAppCommand::Restart(args) => lifecycle(Operation::Restart, args).await,
        RemoteAppCommand::Remove(args) => lifecycle(Operation::Remove, args).await,
        RemoteAppCommand::Logs {
            request,
            environment,
            tail,
            since,
            timeout,
        } => logs(environment, tail, since, timeout, request).await,
    }
}

async fn lifecycle(action: Operation, args: OperationArgs) -> Result<Outcome> {
    mutate(
        action,
        args.environment,
        None,
        None,
        args.no_wait,
        args.dry_run,
        None,
        None,
    )
    .await
}

fn local_build() -> Result<(String, String)> {
    let manifest = app_cli::manifest()?;
    if cli_config::options()
        .app
        .as_deref()
        .is_some_and(|app| app != manifest.id)
    {
        return Err(CliError::new(
            Category::Usage,
            "build_app_mismatch",
            "--app must match the local project's immutable build.",
        )
        .into());
    }
    let image: Value = serde_json::from_slice(&std::fs::read("dist/image.json").map_err(|_| {
        CliError::new(
            Category::Environment,
            "build_required",
            "Build and commit the current project before deploying.",
        )
    })?)
    .map_err(|_| {
        CliError::new(
            Category::Environment,
            "invalid_build_metadata",
            "The local build metadata is invalid; build the project again.",
        )
    })?;
    let build: Value = serde_json::from_slice(&std::fs::read("dist/build.json").map_err(|_| {
        CliError::new(
            Category::Environment,
            "build_required",
            "The local build receipt is missing.",
        )
    })?)
    .map_err(|_| {
        CliError::new(
            Category::Environment,
            "invalid_build_metadata",
            "The local build receipt is invalid.",
        )
    })?;
    let git = |args: &[&str]| -> Result<String> {
        let result = crate::owned_process::output(&mut crate::platform::command("git", args)?)?;
        if !result.status.success() {
            return Err(CliError::new(
                Category::Environment,
                "git_status_unavailable",
                "Git could not verify the built source revision.",
            )
            .into());
        }
        String::from_utf8(result.stdout).map_err(|_| {
            CliError::new(
                Category::Environment,
                "invalid_git_output",
                "Git returned invalid revision data.",
            )
            .into()
        })
    };
    let image_id = image["imageId"]
        .as_str()
        .filter(|id| crate::runtime_contract::valid_image_id(id))
        .ok_or_else(|| {
            CliError::new(
                Category::Environment,
                "invalid_image_id",
                "The build must identify an immutable Docker image.",
            )
        })?;
    if image["appId"] != manifest.id
        || image["version"] != manifest.version
        || image["sourceCommit"].as_str() != Some(git(&["rev-parse", "HEAD"])?.trim())
        || !git(&["status", "--porcelain"])?.is_empty()
        || build["sourceCommit"] != image["sourceCommit"]
        || build["appId"] != manifest.id
        || image["bundleVersion"] != crate::bundle::VERSION
        || build["bundleVersion"] != crate::bundle::VERSION
    {
        return Err(CliError::new(
            Category::Conflict,
            "stale_build",
            "Commit changes and build the current project before deployment.",
        )
        .into());
    }
    Ok((manifest.id, image_id.into()))
}

fn environment(action: Operation, value: Option<&str>) -> Result<Environment> {
    let value = value.unwrap_or(if action == Operation::Promote {
        "prod"
    } else {
        "dev"
    });
    value.parse().map_err(|_| {
        CliError::new(
            Category::Usage,
            "invalid_environment",
            "Choose dev or prod.",
        )
        .into()
    })
}

fn explicit_target(
    action: Operation,
    environment: Environment,
    environment_supplied: bool,
    app: &str,
) -> Result<()> {
    if action.requires_confirmation(environment) {
        let options = cli_config::options();
        if options.context.is_none() || options.app.as_deref() != Some(app) || !environment_supplied
        {
            return Err(CliError::new(Category::Usage,"explicit_target_required","Prod operations and app removal require explicit --context, --app and --environment, including dry-run and retry.").into());
        }
    }
    Ok(())
}

async fn preflight(session: &Session, input: &PreflightInput) -> Result<PreflightResult> {
    let value = app_cli::api(
        session,
        &format!("/api/v1/apps/{}/runtime/preflight", input.target.app_id),
        Some(serde_json::to_value(input)?),
    )
    .await?;
    let plan: PreflightResult = serde_json::from_value(value).map_err(|_| {
        CliError::new(
            Category::Network,
            "invalid_preflight_response",
            "The server returned an invalid preflight contract.",
        )
    })?;
    if plan.input != *input
        || plan.expected_state.target != input.target
        || crate::runtime_contract::validate_state(&plan.expected_state).is_err()
        || plan.requires_confirmation
            != input.action.requires_confirmation(input.target.environment)
        || !plan.preserves_database
    {
        return Err(CliError::new(
            Category::Network,
            "preflight_target_mismatch",
            "The preflight result does not match the requested target and operation.",
        )
        .into());
    }
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
async fn mutate(
    action: Operation,
    environment_flag: Option<String>,
    image: Option<String>,
    release: Option<Uuid>,
    no_wait: bool,
    dry_run: bool,
    app: Option<String>,
    retry: Option<Uuid>,
) -> Result<Outcome> {
    let app = app.map(Ok).unwrap_or_else(selected_app)?;
    let environment = environment(action, environment_flag.as_deref())?;
    explicit_target(action, environment, environment_flag.is_some(), &app)?;
    let input = PreflightInput {
        target: RuntimeTarget {
            app_id: app.clone(),
            environment,
        },
        action,
        image_id: image,
        release_id: release,
    };
    crate::runtime_contract::validate_input(&input).map_err(|_| {
        CliError::new(
            Category::Usage,
            "invalid_runtime_input",
            "The operation, environment and release/image arguments are incompatible.",
        )
    })?;
    let session = app_cli::session()?;
    let mut features = vec!["runtime.preflight.v1", "runtime.jobs.v1"];
    if !no_wait {
        features.push("runtime.events.v1");
    }
    if action == Operation::Deploy && retry.is_none() {
        features.push("images.idempotency.v1");
    }
    if retry.is_some() {
        features.push("runtime.retry.v1");
    }
    let capabilities = require_features(&session, &features).await?;
    if action == Operation::Deploy && retry.is_none() {
        let metadata: Value =
            serde_json::from_slice(&std::fs::read("dist/image.json")?).map_err(|_| {
                CliError::new(
                    Category::Environment,
                    "invalid_build_metadata",
                    "The local image metadata is invalid.",
                )
            })?;
        let platform = metadata["platform"]
            .as_str()
            .filter(|platform| matches!(*platform, "linux/amd64" | "linux/arm64"))
            .ok_or_else(|| {
                CliError::new(
                    Category::Environment,
                    "build_platform_required",
                    "Rebuild this app with the current CLI to record its immutable image platform.",
                )
            })?;
        if capabilities["runtimePlatform"] != platform {
            return Err(CliError::new(Category::Environment,"image_platform_mismatch","The built image platform must match the selected server runtime platform.")
                .with_details(json!({"imagePlatform":platform,"serverPlatform":capabilities["runtimePlatform"].as_str().filter(|p|matches!(*p,"linux/amd64"|"linux/arm64"))})).into());
        }
    }
    let hash = cli_journal::operation_hash(&session, &input, retry)?;
    let lock = if dry_run {
        None
    } else {
        Some(cli_journal::operation_lock(&hash).await?)
    };
    if !dry_run && let Some(entry) = cli_journal::latest(&session, &input, retry)? {
        if let Some(id) = entry.job_id {
            let job = get_job(&session, id).await?;
            match job["status"].as_str() {
                    Some("succeeded") if retry.is_some()=>return data(job),
                    Some("succeeded")=>{},
                    Some("failed") if retry.is_some()=>return Err(CliError::new(Category::Conflict,"previous_retry_failed","Inspect the failed retry and target that new job explicitly if its typed failure is retryable.").with_details(json!({"jobId":id})).with_recovery("miy",&["job","get",&id.to_string()]).into()),
                    Some("failed")=>{},
                    _=>{drop(lock);return if no_wait {data(job)}else{watch(&session,id,None,600,OutputMode::Json).await};}
                }
        } else if entry.submission_status != "rejected"
            && !entry
                .upload
                .as_ref()
                .is_some_and(|upload| upload.status == "failed")
        {
            return Err(journal_error(
                &entry,
                Category::Conflict,
                "submission_status_unknown",
                "Recover the saved request before starting another operation.",
            ));
        }
    }
    let plan = preflight(&session, &input).await?;
    if dry_run {
        return data(serde_json::to_value(plan)?);
    }
    if !plan.can_submit {
        return Err(CliError::new(
            Category::Environment,
            "preflight_failed",
            "The target does not satisfy runtime admission requirements.",
        )
        .with_details(serde_json::to_value(plan)?)
        .into());
    }
    if plan.requires_confirmation {
        cli_config::confirm(&format!(
            "{} app {} in {} on context {}? Planned changes: {}. The application database is preserved.",
            action,
            app,
            environment,
            cli_journal::actor(&session)?.0,
            serde_json::to_string(&plan.changes)?
        ))?;
    }
    let mut entry = cli_journal::create(&session, input, retry)?;
    entry.retry_of = retry;
    let payload = if let Some(id) = retry {
        entry.submit_path = Some(format!("/api/v1/runtime/jobs/{id}/retry"));
        json!({"appId":app,"environment":environment,"idempotencyKey":entry.request_id,"expectedState":plan.expected_state,"confirmed":plan.requires_confirmation})
    } else {
        entry.submit_path = Some(format!("/api/v1/apps/{app}/runtime"));
        json!({"action":action,"environment":environment,"imageId":entry.intent.image_id,"releaseId":entry.intent.release_id,
            "idempotencyKey":entry.request_id,"expectedState":plan.expected_state,"confirmed":plan.requires_confirmation})
    };
    entry.payload_hash = Some(crate::contract::hash(serde_json::to_vec(&payload)?));
    entry.payload = Some(payload);
    cli_journal::publish(&entry)?;
    if action == Operation::Deploy && retry.is_none() {
        crate::cli_upload::ensure(&session, &mut entry, false).await?;
    }
    let job = submit(&session, &mut entry).await?;
    let id = entry.job_id.unwrap();
    drop(lock);
    if no_wait {
        data(job)
    } else {
        watch(&session, id, None, 600, OutputMode::Json).await
    }
}

fn journal_error(entry: &Entry, category: Category, code: &str, message: &str) -> anyhow::Error {
    let mut error = CliError::new(category, code, message).with_details(
        json!({"requestId":entry.request_id,"jobId":entry.job_id,"remoteWorkCancelled":false}),
    );
    error.recovery = Some(cli_journal::recovery(entry));
    error.into()
}

fn job_id(value: &Value) -> Result<Uuid> {
    value["id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|id| !id.is_nil())
        .ok_or_else(|| {
            CliError::new(
                Category::Network,
                "invalid_job_response",
                "The server did not return a valid job ID.",
            )
            .into()
        })
}

async fn submit(session: &Session, entry: &mut Entry) -> Result<Value> {
    cli_journal::validate(entry, session)?;
    let path = entry.submit_path.clone().ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "submission_not_prepared",
            "The saved request has no checked submission.",
        )
    })?;
    let payload = entry.payload.clone().ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "submission_not_prepared",
            "The saved request has no checked input.",
        )
    })?;
    entry.submission_status = "sending".into();
    cli_journal::save(entry)?;
    let outcome = tokio::select! {
        value=app_cli::api(session,&path,Some(payload))=>Some(value),
        _=tokio::signal::ctrl_c()=>None,
    };
    let Some(outcome) = outcome else {
        return Err(journal_error(
            entry,
            Category::Interrupted,
            "submission_interrupted",
            "The local request was interrupted; discover the saved request before resubmitting.",
        ));
    };
    let value = match outcome {
        Ok(value) => value,
        Err(error) => {
            let failure = crate::cli_error::classify(&error);
            if matches!(
                failure.category,
                Category::Usage | Category::Forbidden | Category::NotFound | Category::Conflict
            ) {
                entry.submission_status = "rejected".into();
                cli_journal::save(entry)?;
            }
            if failure.category == Category::Network {
                return Err(journal_error(
                    entry,
                    Category::Network,
                    "submission_result_unknown",
                    "The submission acknowledgement was lost; resume the saved request to discover its job.",
                ));
            }
            return Err(error);
        }
    };
    if bind_job(entry, &value).is_err() {
        return Err(journal_error(
            entry,
            Category::Network,
            "submission_response_invalid",
            "The submission response was invalid; discover the saved request before attempting further work.",
        ));
    }
    Ok(value)
}

fn bind_job(entry: &mut Entry, value: &Value) -> Result<()> {
    let id = job_id(value)?;
    if value["appId"] != entry.app_id
        || value["environment"] != entry.intent.target.environment.as_str()
        || value["action"] != entry.intent.action.as_str()
        || value["requestId"] != entry.request_id.to_string()
        || entry.job_id.is_some_and(|prior| prior != id)
    {
        return Err(journal_error(
            entry,
            Category::Network,
            "job_request_mismatch",
            "The returned job does not match the saved request identity and target.",
        ));
    }
    entry.job_id = Some(id);
    entry.submission_status = "accepted".into();
    cli_journal::save(entry)
}

async fn get_job(session: &Session, id: Uuid) -> Result<Value> {
    let value = app_cli::api(session, &format!("/api/v1/runtime/jobs/{id}"), None).await?;
    if job_id(&value)? != id {
        return Err(CliError::new(
            Category::Network,
            "job_id_mismatch",
            "The returned job does not match the requested ID.",
        )
        .into());
    }
    Ok(value)
}

pub async fn execute_job(command: JobCommand, mode: OutputMode) -> Result<Outcome> {
    if mode == OutputMode::Jsonl && !matches!(command, JobCommand::Watch { .. }) {
        return Err(CliError::new(
            Category::Usage,
            "unsupported_output",
            "Only job watch supports JSONL event output.",
        )
        .into());
    }
    let session = app_cli::session()?;
    require_features(&session, &["runtime.jobs.v1"]).await?;
    match command {
        JobCommand::List {
            environment,
            request_id,
        } => data(
            app_cli::api(
                &session,
                &query(
                    "/api/v1/runtime/jobs",
                    &[
                        ("appId", cli_config::options().app.clone()),
                        ("environment", environment),
                        ("requestId", request_id.map(|id| id.to_string())),
                    ],
                ),
                None,
            )
            .await?,
        ),
        JobCommand::Get { id } => data(get_job(&session, id).await?),
        JobCommand::Watch { id, after, timeout } => {
            watch(
                &session,
                id,
                after.or_else(|| cli_config::options().cursor.clone()),
                timeout,
                mode,
            )
            .await
        }
        JobCommand::Resume {
            id,
            no_wait,
            environment,
        } => {
            if !no_wait {
                require_features(&session, &["runtime.events.v1"]).await?;
            }
            resume(&session, id, no_wait, environment).await
        }
        JobCommand::Retry {
            id,
            environment,
            no_wait,
            dry_run,
        } => {
            let job = get_job(&session, id).await?;
            let status = job["status"]
                .as_str()
                .and_then(|value| value.parse::<crate::runtime_contract::JobStatus>().ok());
            let failure = crate::runtime_contract::RuntimeFailure::new(
                crate::runtime_contract::FailureCode::from_code(
                    job["errorCode"].as_str().unwrap_or(""),
                ),
            );
            if !status.is_some_and(|status| crate::runtime_contract::can_retry(status, &failure))
                || job["retryable"] != true
                || job["failureClass"] != "after_preflight"
            {
                return Err(CliError::new(Category::Conflict,"retry_not_allowed","Only terminal failures explicitly classified as retryable can create a retry job.").with_details(json!({"jobId":id})).into());
            }
            let action: Operation = job["action"]
                .as_str()
                .and_then(|value| value.parse().ok())
                .ok_or_else(|| {
                    CliError::new(
                        Category::Network,
                        "invalid_job_action",
                        "The original job has an unsupported action.",
                    )
                })?;
            let app = job["appId"]
                .as_str()
                .filter(|id| crate::contract::valid_id(id))
                .ok_or_else(|| {
                    CliError::new(
                        Category::Network,
                        "invalid_job_target",
                        "The original job has an invalid target.",
                    )
                })?
                .to_owned();
            if cli_config::options()
                .app
                .as_deref()
                .is_some_and(|selected| selected != app)
                || environment
                    .as_deref()
                    .is_some_and(|selected| Some(selected) != job["environment"].as_str())
            {
                return Err(CliError::new(
                    Category::Conflict,
                    "retry_target_mismatch",
                    "The explicitly selected target does not match the original job.",
                )
                .into());
            }
            let inferred = job["environment"]
                .as_str()
                .ok_or_else(|| {
                    CliError::new(
                        Category::Network,
                        "invalid_job_target",
                        "The original job has an invalid environment.",
                    )
                })?
                .to_owned();
            let env: Environment = inferred.parse().map_err(|_| {
                CliError::new(
                    Category::Network,
                    "invalid_job_target",
                    "The original job has an invalid environment.",
                )
            })?;
            explicit_target(action, env, environment.is_some(), &app)?;
            let image = if action == Operation::Deploy {
                job["imageId"].as_str().map(str::to_owned)
            } else {
                None
            };
            let release = if action == Operation::Promote {
                job["releaseId"]
                    .as_str()
                    .and_then(|id| Uuid::parse_str(id).ok())
            } else {
                None
            };
            mutate(
                action,
                Some(inferred),
                image,
                release,
                no_wait,
                dry_run,
                Some(app),
                Some(id),
            )
            .await
        }
    }
}

async fn resume(
    session: &Session,
    id: Uuid,
    no_wait: bool,
    environment: Option<String>,
) -> Result<Outcome> {
    let mut entry = match cli_journal::load(session, id) {
        Ok(entry) => entry,
        Err(error) if crate::cli_error::classify(&error).category == Category::NotFound => {
            return if no_wait {
                data(get_job(session, id).await?)
            } else {
                watch(session, id, None, 600, OutputMode::Json).await
            };
        }
        Err(error) => return Err(error),
    };
    let lock = cli_journal::operation_lock(&entry.input_hash).await?;
    entry = cli_journal::load(session, entry.request_id)?;
    if cli_config::options()
        .app
        .as_ref()
        .is_some_and(|app| app != &entry.app_id)
        || environment
            .as_deref()
            .is_some_and(|value| value != entry.intent.target.environment.as_str())
    {
        return Err(CliError::new(
            Category::Conflict,
            "resume_target_mismatch",
            "Explicit app and environment must match the saved runtime request.",
        )
        .into());
    }
    if let Some(id) = entry.job_id {
        drop(lock);
        return if no_wait {
            data(get_job(session, id).await?)
        } else {
            watch(session, id, None, 600, OutputMode::Json).await
        };
    }
    let found = app_cli::api(
        session,
        &format!(
            "/api/v1/runtime/jobs?limit=2&requestId={}&appId={}",
            entry.request_id, entry.app_id
        ),
        None,
    )
    .await?;
    let jobs = found["jobs"].as_array().ok_or_else(|| {
        CliError::new(
            Category::Network,
            "invalid_jobs_response",
            "The server returned an invalid request lookup.",
        )
    })?;
    if jobs.len() > 1 {
        return Err(journal_error(
            &entry,
            Category::Network,
            "ambiguous_request_result",
            "The request lookup returned multiple jobs.",
        ));
    }
    let job = if let Some(job) = jobs.first() {
        bind_job(&mut entry, job)?;
        job.clone()
    } else {
        let mut features = vec!["runtime.preflight.v1"];
        if entry.intent.action == Operation::Deploy && entry.retry_of.is_none() {
            features.push("images.idempotency.v1");
        }
        if entry.retry_of.is_some() {
            features.push("runtime.retry.v1");
        }
        require_features(session, &features).await?;
        if entry.submission_status == "rejected" {
            return Err(journal_error(
                &entry,
                Category::Conflict,
                "submission_rejected",
                "The original request was rejected; inspect current state and start a new explicitly checked operation.",
            ));
        }
        if entry
            .intent
            .action
            .requires_confirmation(entry.intent.target.environment)
        {
            explicit_target(
                entry.intent.action,
                entry.intent.target.environment,
                environment.is_some(),
                &entry.app_id,
            )?;
            if environment.as_deref() != Some(entry.intent.target.environment.as_str()) {
                return Err(CliError::new(
                    Category::Conflict,
                    "resume_target_mismatch",
                    "--environment must match the saved runtime request.",
                )
                .into());
            }
            cli_config::confirm(&format!(
                "Resubmit the original {} request for {} in {}? The saved expected state and original idempotency key will be retained; the database is preserved.",
                entry.intent.action, entry.app_id, entry.intent.target.environment
            ))?;
        }
        if entry.intent.action == Operation::Deploy && entry.retry_of.is_none() {
            crate::cli_upload::ensure(session, &mut entry, true).await?;
        }
        submit(session, &mut entry).await?
    };
    let id = entry.job_id.unwrap();
    drop(lock);
    if no_wait {
        data(job)
    } else {
        watch(session, id, None, 600, OutputMode::Json).await
    }
}

fn observation_error(
    id: Uuid,
    cursor: Option<&str>,
    category: Category,
    code: &str,
    message: &str,
) -> anyhow::Error {
    CliError::new(category, code, message)
        .with_details(json!({"jobId":id,"cursor":cursor,"remoteWorkCancelled":false}))
        .with_recovery("miy", &["job", "watch", &id.to_string()])
        .into()
}

async fn watch(
    session: &Session,
    id: Uuid,
    after: Option<String>,
    timeout: u64,
    mode: OutputMode,
) -> Result<Outcome> {
    require_features(session, &["runtime.events.v1"]).await?;
    let mut entry = match cli_journal::load(session, id) {
        Ok(entry) => Some(entry),
        Err(error) if crate::cli_error::classify(&error).category == Category::NotFound => None,
        Err(error) => return Err(error),
    };
    let explicit_cursor = after.is_some();
    let mut cursor = after.or_else(|| entry.as_ref().and_then(|entry| entry.cursor.clone()));
    let mut sequence = if explicit_cursor {
        0
    } else {
        entry.as_ref().and_then(|entry| entry.sequence).unwrap_or(0)
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("limit", "200");
        if let Some(cursor) = &cursor {
            query.append_pair("after", cursor);
        }
        let path = format!("/api/v1/runtime/jobs/{id}/events?{}", query.finish());
        let page = tokio::select! {
            _=tokio::time::sleep_until(deadline)=>return Err(observation_error(id,cursor.as_deref(),Category::Network,"job_watch_timeout","Observation timed out; the remote job continues and can be watched again.")),
            _=&mut interrupt=>return Err(observation_error(id,cursor.as_deref(),Category::Interrupted,"job_watch_interrupted","Stopped local observation; the remote job continues.")),
            value=app_cli::api(session,&path,None)=>value?,
        };
        if page["jobId"] != id.to_string() {
            return Err(observation_error(
                id,
                cursor.as_deref(),
                Category::Network,
                "job_events_mismatch",
                "The event stream does not match the requested job.",
            ));
        }
        let events = page["events"].as_array().ok_or_else(|| {
            observation_error(
                id,
                cursor.as_deref(),
                Category::Network,
                "invalid_job_events",
                "The server returned invalid job events.",
            )
        })?;
        for event in events {
            let next = event["cursor"]
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 2048)
                .ok_or_else(|| {
                    observation_error(
                        id,
                        cursor.as_deref(),
                        Category::Network,
                        "invalid_event_cursor",
                        "The event cursor is invalid.",
                    )
                })?;
            let current = event["sequence"].as_u64().ok_or_else(|| {
                observation_error(
                    id,
                    cursor.as_deref(),
                    Category::Network,
                    "invalid_event_sequence",
                    "The event sequence is invalid.",
                )
            })?;
            if job_id(&event["job"])? != id || current <= sequence {
                return Err(observation_error(
                    id,
                    cursor.as_deref(),
                    Category::Network,
                    "invalid_event_order",
                    "Job events must advance in sequence for the requested job.",
                ));
            }
            if mode == OutputMode::Jsonl {
                let line = json!({"schemaVersion":1,"command":"job watch","ok":true,"data":event,"error":null,"meta":{"cursor":next}});
                let mut stdout = std::io::stdout().lock();
                serde_json::to_writer(&mut stdout, &line)?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
            cursor = Some(next.into());
            sequence = current;
            if let Some(entry) = &mut entry {
                entry.cursor = cursor.clone();
                entry.sequence = Some(sequence);
                cli_journal::save(entry)?;
            }
        }
        if page["terminal"] == true {
            let job = get_job(session, id).await?;
            if job["status"] == "failed" {
                return Err(observation_error(
                    id,
                    cursor.as_deref(),
                    Category::RemoteFailure,
                    "remote_job_failed",
                    "The accepted remote job reached a terminal failure; inspect job get before considering a typed retry.",
                ));
            }
            if job["status"] != "succeeded" {
                return Err(observation_error(
                    id,
                    cursor.as_deref(),
                    Category::Network,
                    "inconsistent_job_terminal_state",
                    "The job terminal state is inconsistent.",
                ));
            }
            return if mode == OutputMode::Jsonl {
                Ok(Outcome::Raw)
            } else {
                data(job)
            };
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(observation_error(
                id,
                cursor.as_deref(),
                Category::Network,
                "job_watch_timeout",
                "Observation timed out; the remote job continues and can be watched again.",
            ));
        }
        tokio::select! {_=&mut interrupt=>return Err(observation_error(id,cursor.as_deref(),Category::Interrupted,"job_watch_interrupted","Stopped local observation; the remote job continues.")),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
    }
}

async fn logs(
    environment: String,
    tail: u16,
    since: Option<chrono::DateTime<chrono::Utc>>,
    timeout: u64,
    existing: Option<Uuid>,
) -> Result<Outcome> {
    let session = app_cli::session()?;
    require_features(&session, &["runtime.logs.v1"]).await?;
    let app = selected_app()?;
    let id = if let Some(id) = existing {
        id
    } else {
        let request = app_cli::api(
            &session,
            &format!("/api/v1/apps/{app}/logs"),
            Some(json!({"environment":environment,"tail":tail,"since":since})),
        )
        .await?;
        job_id(&request)?
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let interrupted = || {
        CliError::new(
            Category::Interrupted,
            "log_observation_interrupted",
            "Stopped local log observation; the accepted remote request remains available.",
        )
        .with_details(json!({"logRequestId":id,"remoteWorkCancelled":false}))
        .with_recovery(
            "miy",
            &[
                "--context",
                session.context.as_deref().unwrap_or_default(),
                "--app",
                &app,
                "app",
                "logs",
                "--environment",
                &environment,
                "--request",
                &id.to_string(),
            ],
        )
    };
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        let log_path = format!("/api/v1/runtime/logs/{id}");
        let value = tokio::select! {_=&mut interrupt=>return Err(interrupted().into()),
        _=tokio::time::sleep_until(deadline)=>return Err(CliError::new(Category::Network,"log_observation_timeout","The accepted log request is still pending; resume it by its saved ID.")
            .with_details(json!({"logRequestId":id,"remoteWorkCancelled":false}))
            .with_recovery("miy",&["--context",session.context.as_deref().unwrap_or_default(),"--app",&app,"app","logs","--environment",&environment,"--request",&id.to_string()]).into()),
        value=app_cli::api(&session,&log_path,None)=>value?};
        if job_id(&value)? != id || value["appId"] != app || value["environment"] != environment {
            return Err(CliError::new(
                Category::Network,
                "log_target_mismatch",
                "The log response does not match the requested app and environment.",
            )
            .into());
        }
        match value["status"].as_str() {
            Some("ready") => return data(value),
            Some("failed") => {
                return Err(CliError::new(
                    Category::RemoteFailure,
                    "remote_logs_failed",
                    "The accepted log request failed.",
                )
                .with_details(json!({"logRequestId":id}))
                .into());
            }
            Some("queued" | "running") => {}
            _ => {
                return Err(CliError::new(
                    Category::Network,
                    "invalid_log_status",
                    "The log request returned an invalid status.",
                )
                .into());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(CliError::new(
                Category::Network,
                "log_observation_timeout",
                "The log request remains pending; its request ID is preserved for inspection.",
            )
            .with_details(json!({"logRequestId":id,"remoteWorkCancelled":false}))
            .with_recovery(
                "miy",
                &[
                    "--context",
                    session.context.as_deref().unwrap_or_default(),
                    "--app",
                    &app,
                    "app",
                    "logs",
                    "--environment",
                    &environment,
                    "--request",
                    &id.to_string(),
                ],
            )
            .into());
        }
        tokio::select! {_=&mut interrupt=>return Err(interrupted().into()),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_grant_input_rejects_unknown_fields_and_missing_required_values() {
        assert!(
            serde_json::from_value::<GrantInput>(
                json!({"userId":"alice","environment":"dev","permission":"use"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<GrantInput>(
                json!({"userId":"alice","environment":"dev","permission":"use","enabled":true})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<GrantInput>(json!({"userId":"alice"})).is_err());
    }
    #[test]
    fn environment_defaults_preserve_dev_only_deploy_and_prod_promotion() {
        assert_eq!(
            environment(Operation::Deploy, None).unwrap(),
            Environment::Dev
        );
        assert_eq!(
            environment(Operation::Promote, None).unwrap(),
            Environment::Prod
        );
        assert!(environment(Operation::Start, Some("invalid")).is_err());
    }
}
