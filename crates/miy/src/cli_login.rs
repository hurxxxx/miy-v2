//! Context-bound CLI authentication. Only public login metadata reaches stdout.
use crate::{
    app_cli::Session,
    cli_config,
    cli_error::{Category, CliError},
    credential_store,
};
use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

const HTTP_SECONDS: u64 = 20;
const REFRESH_MARGIN: i64 = 30;
const BODY_LIMIT: usize = 128 * 1024;

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Start device approval; noninteractive mode returns public instructions for auth wait.
    Login {
        /// Display the device code without automatically opening a browser.
        #[arg(long)]
        device_code: bool,
        /// Return the pending login immediately.
        #[arg(long)]
        no_wait: bool,
        /// Do not open a browser.
        #[arg(long)]
        no_open: bool,
        /// Explicit HTTP loopback development shortcut; approves and waits even without a TTY.
        #[arg(long)]
        account: Option<String>,
    },
    /// Wait for a saved device login without printing its private device code.
    Wait { id: Uuid },
    /// Verify the saved identity and show public session metadata.
    Status,
    /// Revoke the server session, then remove local credentials.
    Logout {
        /// Delete local credentials without attempting server revocation.
        #[arg(long)]
        local_only: bool,
    },
    /// List or explicitly revoke this user's CLI sessions.
    Sessions {
        #[command(subcommand)]
        command: SessionCommand,
    },
}

#[derive(Subcommand)]
pub enum SessionCommand {
    List,
    Revoke { id: Uuid },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Credential {
    schema_version: u32,
    origin: String,
    user_id: String,
    session_id: Uuid,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    refresh_expires_at: i64,
    reauth_required: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Flow {
    schema_version: u32,
    id: Uuid,
    context: String,
    origin: String,
    user_code: String,
    expires_at: i64,
    interval: u64,
    next_poll_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FlowSecret {
    schema_version: u32,
    id: Uuid,
    origin: String,
    device_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Started {
    id: Uuid,
    device_code: String,
    user_code: String,
    verification_path: String,
    expires_in: i64,
    interval: u64,
}

#[derive(Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum TokenReply {
    Pending {
        interval: u64,
    },
    SlowDown {
        interval: u64,
    },
    Approved {
        session_id: Uuid,
        user_id: String,
        access_token: String,
        refresh_token: String,
        token_type: String,
        expires_in: i64,
        refresh_expires_at: chrono::DateTime<chrono::Utc>,
    },
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn auth_error(code: &'static str, message: &'static str) -> CliError {
    CliError::new(Category::Auth, code, message).with_recovery("miy", &["auth", "login"])
}

fn invalid_response() -> CliError {
    CliError::new(
        Category::Network,
        "invalid_auth_response",
        "The server returned an invalid authentication response.",
    )
}

fn valid_secret(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|secret| {
        (32..=128).contains(&secret.len())
            && secret
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

fn valid_user(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn credential_key(context: &cli_config::Context, user: &str) -> String {
    cli_config::credential_key(context, user)
}

fn vault_get(root: &Path, key: &str) -> Result<Option<String>> {
    credential_store::get(root, key).map_err(|_| storage_error().into())
}
fn vault_set(root: &Path, key: &str, value: &str) -> Result<()> {
    credential_store::set(root, key, value).map_err(|_| storage_error().into())
}
fn vault_remove(root: &Path, key: &str) -> Result<()> {
    credential_store::remove(root, key).map_err(|_| storage_error().into())
}
fn storage_error() -> CliError {
    CliError::new(
        Category::Environment,
        "credential_storage_unavailable",
        "The credential store is unavailable. Check its OS service, backend and private file permissions.",
    )
}

fn context_for_session(session: &Session) -> Result<cli_config::Context> {
    let name = session.context.as_deref().ok_or_else(|| {
        auth_error(
            "context_required",
            "Select a context before inspecting authentication.",
        )
    })?;
    let context = cli_config::named(name)?;
    if context.origin != session.origin || context.user_id != session.user_id {
        return Err(auth_error(
            "context_identity_changed",
            "The context server or user changed; reload its session before continuing.",
        )
        .into());
    }
    Ok(context)
}

fn read_credential(root: &Path, context: &cli_config::Context, user: &str) -> Result<Credential> {
    let data = vault_get(root, &credential_key(context, user))?.ok_or_else(|| {
        auth_error(
            "login_required",
            "No credential is saved for the selected server and user.",
        )
    })?;
    let credential: Credential = serde_json::from_str(&data).map_err(|_| {
        auth_error(
            "invalid_saved_credential",
            "The saved credential is invalid; sign in again.",
        )
    })?;
    validate_credential(&credential, &context.origin, user)?;
    Ok(credential)
}

fn validate_credential(credential: &Credential, origin: &str, user: &str) -> Result<()> {
    if credential.schema_version != 1
        || credential.origin != origin
        || credential.user_id != user
        || !valid_user(user)
        || credential.session_id.is_nil()
        || !valid_secret(&credential.access_token, "miy_access_")
        || !valid_secret(&credential.refresh_token, "miy_refresh_")
        || credential.expires_at > credential.refresh_expires_at
    {
        return Err(auth_error(
            "credential_binding_invalid",
            "The saved credential does not match this server and user.",
        )
        .into());
    }
    Ok(())
}

fn usable(credential: &Credential) -> Result<()> {
    if credential.reauth_required || credential.refresh_expires_at <= now() {
        return Err(auth_error(
            "reauthentication_required",
            "The saved session cannot be safely renewed; sign in again.",
        )
        .into());
    }
    Ok(())
}

fn as_session(name: &str, context: &cli_config::Context, credential: &Credential) -> Session {
    Session {
        context: Some(name.into()),
        user_id: Some(credential.user_id.clone()),
        expires_at: Some(credential.expires_at),
        origin: context.origin.clone(),
        token: Some(credential.access_token.clone()),
        cookie: String::new(),
        csrf: String::new(),
        ca_file: context.ca_file.clone(),
    }
}

pub fn session() -> Result<Session> {
    let (name, context) = cli_config::selected()?;
    crate::cli_client::origin(&context.origin)?;
    let user = context
        .user_id
        .as_deref()
        .ok_or_else(|| auth_error("login_required", "Sign in to the selected server context."))?;
    let credential = read_credential(&cli_config::root()?, &context, user)?;
    usable(&credential)?;
    Ok(as_session(&name, &context, &credential))
}

async fn lock(root: &Path, key: &str) -> Result<std::fs::File> {
    lock_with_timeout(root, key, Duration::from_secs(5)).await
}

async fn lock_with_timeout(root: &Path, key: &str, timeout: Duration) -> Result<std::fs::File> {
    let directory = root.join("auth-locks");
    crate::platform::reject_reparse(root)?;
    crate::platform::reject_reparse(&directory)?;
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{key}.lock"));
    crate::platform::reject_reparse(&path)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if file.try_lock().is_ok() {
            return Ok(file);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(CliError::new(
                Category::Conflict,
                "authentication_in_use",
                "Another CLI is updating this authentication state; retry after it finishes.",
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn persist(root: &Path, context: &cli_config::Context, credential: &Credential) -> Result<()> {
    vault_set(
        root,
        &credential_key(context, &credential.user_id),
        &serde_json::to_string(credential)?,
    )
}

pub async fn refreshed(saved: &Session) -> Result<Session> {
    let Some(name) = saved.context.as_deref() else {
        return Ok(saved.clone());
    };
    let context = context_for_session(saved)?;
    let user = saved.user_id.as_deref().ok_or_else(|| {
        auth_error(
            "credential_binding_invalid",
            "The session has no bound user.",
        )
    })?;
    if context.origin != saved.origin || context.user_id.as_deref() != Some(user) {
        return Err(auth_error(
            "context_identity_changed",
            "The context server or user changed; reload its session before continuing.",
        )
        .into());
    }
    crate::cli_client::origin(&context.origin)?;
    let root = cli_config::root()?;
    let _lock = lock(&root, &credential_key(&context, user)).await?;
    // Reload after taking the interprocess lock: another process may already have rotated.
    let mut credential = read_credential(&root, &context, user)?;
    usable(&credential)?;
    if credential.expires_at > now() + REFRESH_MARGIN {
        return Ok(as_session(name, &context, &credential));
    }
    // Persist the uncertain state BEFORE sending. A killed process or a lost response
    // must not leave a used refresh token available for another process to replay.
    credential.reauth_required = true;
    persist(&root, &context, &credential)?;
    let reply = public_post(
        &context,
        "/auth/cli/token",
        json!({"grantType":"refresh_token","refreshToken":credential.refresh_token}),
    )
    .await;
    let mut renewed = match reply.and_then(|reply| credential_from_reply(reply,&context.origin)) {
        Ok(renewed) if renewed.user_id == user && renewed.session_id == credential.session_id => renewed,
        _ => return Err(auth_error("refresh_result_unavailable", "The renewal result is unavailable or invalid; sign in again without retrying the old refresh token.").with_details(json!({"sessionId":credential.session_id,"reauthenticationRequired":true})).into()),
    };
    if verify_identity(&context, &renewed).await.is_err() {
        return Err(auth_error(
            "refreshed_identity_invalid",
            "The renewed credential could not be verified; sign in again.",
        )
        .into());
    }
    renewed.reauth_required = false;
    persist(&root,&context,&renewed).map_err(|_| CliError::new(Category::Environment,"refreshed_credential_not_saved","The renewed credential could not be stored. Repair credential storage and sign in again.").with_recovery("miy",&["auth","login"]))?;
    Ok(as_session(name, &context, &renewed))
}

async fn response(mut response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(crate::cli_error::transport)?
    {
        if bytes.len() + chunk.len() > BODY_LIMIT {
            return Err(if status.is_success() {
                invalid_response()
            } else {
                auth_http_error(status, None)
            }
            .into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Option<Value> = serde_json::from_slice(&bytes).ok();
    if !status.is_success() {
        return Err(auth_http_error(status, value.as_ref()).into());
    }
    value.ok_or_else(|| invalid_response().into())
}

fn auth_http_error(status: reqwest::StatusCode, body: Option<&Value>) -> CliError {
    let (category, default) = match status.as_u16() {
        400 | 422 => (Category::Usage, "invalid_auth_request"),
        401 => (Category::Auth, "unauthorized"),
        403 => (Category::Forbidden, "forbidden"),
        404 => (Category::NotFound, "auth_endpoint_unavailable"),
        409 => (Category::Conflict, "auth_state_conflict"),
        429 => (Category::Network, "rate_limited"),
        _ => (Category::Network, "auth_service_unavailable"),
    };
    // A server may reflect submitted credentials into error codes or request IDs.
    // Authentication accepts only known codes and never forwards arbitrary strings.
    let code = match body.and_then(|body| body["error"]["code"].as_str()) {
        Some("access_denied") => "access_denied",
        Some("expired_token") => "expired_token",
        Some("invalid_grant") => "invalid_grant",
        Some("refresh_reuse_detected") => "refresh_reuse_detected",
        Some("session_expired_or_revoked") => "session_expired_or_revoked",
        Some("user_disabled") => "user_disabled",
        _ => default,
    };
    CliError::new(category,code,"The authentication service rejected the request; check the login state, access and server availability.")
        .with_details(json!({"httpStatus":status.as_u16()}))
}

async fn public_post(context: &cli_config::Context, path: &str, value: Value) -> Result<Value> {
    let origin = crate::cli_client::origin(&context.origin)?;
    let client = crate::cli_client::client(context.ca_file.as_deref(), HTTP_SECONDS)?;
    response(
        client
            .post(format!("{origin}{path}"))
            .json(&value)
            .send()
            .await
            .map_err(crate::cli_error::transport)?,
    )
    .await
}

async fn authenticated(
    context: &cli_config::Context,
    token: &str,
    path: &str,
    method: reqwest::Method,
) -> Result<Value> {
    let origin = crate::cli_client::origin(&context.origin)?;
    let client = crate::cli_client::client(context.ca_file.as_deref(), HTTP_SECONDS)?;
    response(
        client
            .request(method, format!("{origin}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(crate::cli_error::transport)?,
    )
    .await
}

async fn verify_identity(context: &cli_config::Context, credential: &Credential) -> Result<()> {
    let me = authenticated(
        context,
        &credential.access_token,
        "/api/v1/me",
        reqwest::Method::GET,
    )
    .await?;
    if me["id"].as_str() != Some(&credential.user_id) {
        return Err(auth_error(
            "authentication_subject_mismatch",
            "The server identity does not match the issued credential.",
        )
        .into());
    }
    Ok(())
}

fn credential_from_reply(value: Value, origin: &str) -> Result<Credential> {
    let reply: TokenReply = serde_json::from_value(value).map_err(|_| invalid_response())?;
    let TokenReply::Approved {
        session_id,
        user_id,
        access_token,
        refresh_token,
        token_type,
        expires_in,
        refresh_expires_at,
    } = reply
    else {
        return Err(invalid_response().into());
    };
    let timestamp = now();
    if token_type != "Bearer"
        || !(1..=900).contains(&expires_in)
        || refresh_expires_at.timestamp() <= timestamp
    {
        return Err(invalid_response().into());
    }
    let credential = Credential {
        schema_version: 1,
        origin: origin.into(),
        user_id,
        session_id,
        access_token,
        refresh_token,
        expires_at: (timestamp + expires_in).min(refresh_expires_at.timestamp()),
        refresh_expires_at: refresh_expires_at.timestamp(),
        reauth_required: false,
    };
    validate_credential(&credential, origin, &credential.user_id)
        .map_err(|_| invalid_response())?;
    Ok(credential)
}

fn flow_path(root: &Path, id: Uuid) -> PathBuf {
    root.join("login-flows").join(format!("{id}.json"))
}
fn flow_key(flow: &Flow) -> String {
    crate::contract::hash(format!("device-flow\0{}\0{}", flow.origin, flow.id))
}

fn save_flow(root: &Path, flow: &Flow) -> Result<()> {
    let directory = root.join("login-flows");
    crate::platform::reject_reparse(&directory)?;
    std::fs::create_dir_all(&directory)?;
    crate::platform::write_private(&flow_path(root, flow.id), &serde_json::to_vec_pretty(flow)?)
}

fn load_flow(root: &Path, id: Uuid) -> Result<Flow> {
    let path = flow_path(root, id);
    crate::platform::reject_reparse(&root.join("login-flows"))?;
    crate::platform::reject_reparse(&path)?;
    let bytes = std::fs::read(path).map_err(|_| {
        auth_error(
            "login_flow_unavailable",
            "The saved login request is unavailable; start a new login.",
        )
    })?;
    if bytes.len() > 16384 {
        return Err(
            auth_error("invalid_login_flow", "The saved login metadata is invalid.").into(),
        );
    }
    let flow: Flow = serde_json::from_slice(&bytes)
        .map_err(|_| auth_error("invalid_login_flow", "The saved login metadata is invalid."))?;
    if flow.schema_version != 1
        || flow.id != id
        || !crate::contract::valid_id(&flow.context)
        || !valid_user_code(&flow.user_code)
        || !(1..=60).contains(&flow.interval)
        || crate::cli_client::origin(&flow.origin).ok().as_deref() != Some(&flow.origin)
    {
        return Err(
            auth_error("invalid_login_flow", "The saved login metadata is invalid.").into(),
        );
    }
    Ok(flow)
}

fn valid_user_code(value: &str) -> bool {
    value.len() == 11
        && value.as_bytes()[5] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 5 || b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(&byte))
}

fn pending(flow: &Flow) -> Value {
    json!({"status":"pending","loginId":flow.id,"context":flow.context,
        "verificationUrl":format!("{}/cli/device/{}",flow.origin,flow.id),"userCode":flow.user_code,
        "expiresAt":flow.expires_at,"expiresIn":(flow.expires_at-now()).max(0),"interval":flow.interval,
        "restart":{"executable":"miy","args":["--context",flow.context,"auth","login"]},
        "recovery":{"executable":"miy","args":["--context",flow.context,"auth","wait",flow.id.to_string()]}})
}

fn clean_flow(root: &Path, flow: &Flow) -> bool {
    let secret = vault_remove(root, &flow_key(flow)).is_ok();
    let file = match std::fs::remove_file(flow_path(root, flow.id)) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    };
    secret && file
}

async fn login(
    device_code: bool,
    no_wait: bool,
    no_open: bool,
    account: Option<String>,
) -> Result<Value> {
    let (name, context) = cli_config::login_context()?;
    crate::cli_client::origin(&context.origin)?;
    let capability = crate::cli_client::client(context.ca_file.as_deref(), HTTP_SECONDS)?
        .get(format!("{}/api/v1/capabilities", context.origin))
        .send()
        .await
        .map_err(crate::cli_error::transport)?;
    if capability.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CliError::new(
            Category::Environment,
            "unsupported_capability",
            "Upgrade the server before using device and refresh-session login.",
        )
        .into());
    }
    let capability = response(capability).await?;
    let features = capability["features"].as_array();
    if capability["schemaVersion"] != 1
        || capability["cliContractVersion"] != 1
        || !["auth.device.v1", "auth.sessions.v1"]
            .iter()
            .all(|required| features.is_some_and(|items| items.iter().any(|item| item == required)))
    {
        return Err(CliError::new(Category::Environment,"unsupported_capability","The server does not advertise the required device and refresh-session authentication contracts.").into());
    }
    if let Some(account) = account.as_deref() {
        validate_development(&context.origin, account)?;
    }
    let started: Started = serde_json::from_value(
        public_post(
            &context,
            "/auth/cli/device",
            json!({"clientId":cli_config::client_id()?,"clientName":"miy CLI"}),
        )
        .await?,
    )
    .map_err(|_| invalid_response())?;
    if started.id.is_nil()
        || !valid_secret(&started.device_code, "miy_device_")
        || !valid_user_code(&started.user_code)
        || started.verification_path != format!("/cli/device/{}", started.id)
        || !(1..=300).contains(&started.expires_in)
        || !(1..=60).contains(&started.interval)
    {
        return Err(invalid_response().into());
    }
    let flow = Flow {
        schema_version: 1,
        id: started.id,
        context: name,
        origin: context.origin.clone(),
        user_code: started.user_code,
        expires_at: now() + started.expires_in,
        interval: started.interval,
        next_poll_at: 0,
    };
    let root = cli_config::ensure_root()?;
    let secret = FlowSecret {
        schema_version: 1,
        id: flow.id,
        origin: flow.origin.clone(),
        device_code: started.device_code,
    };
    vault_set(&root, &flow_key(&flow), &serde_json::to_string(&secret)?)?;
    if let Err(error) = save_flow(&root, &flow) {
        let _ = vault_remove(&root, &flow_key(&flow));
        return Err(error);
    }
    if let Some(account) = account {
        development_approve(&context, &flow, &account).await?;
        return wait(flow.id).await;
    }
    if !cli_config::interactive() || !std::io::stdout().is_terminal() || no_wait {
        return Ok(pending(&flow));
    }
    let url = format!("{}{}", context.origin, started.verification_path);
    eprintln!(
        "Open {url} and approve code {}. Login request: {}. Expires at {} ({} seconds remaining). If it expires, run: miy --context {} auth login",
        flow.user_code,
        flow.id,
        chrono::DateTime::from_timestamp(flow.expires_at, 0)
            .unwrap()
            .to_rfc3339(),
        (flow.expires_at - now()).max(0),
        flow.context
    );
    if !device_code && !no_open {
        let _ = crate::platform::open_browser(&url);
    }
    wait(flow.id).await
}

async fn wait(id: Uuid) -> Result<Value> {
    let root = cli_config::root()?;
    let initial = load_flow(&root, id)?;
    let _lock = lock(&root, &flow_key(&initial)).await?;
    let mut flow = load_flow(&root, id)?;
    let (name, context) = cli_config::selected()?;
    if name != flow.context || context.origin != flow.origin {
        return Err(CliError::new(
            Category::Conflict,
            "login_context_mismatch",
            "Select the context that started this login before waiting.",
        )
        .with_recovery(
            "miy",
            &["--context", &flow.context, "auth", "wait", &id.to_string()],
        )
        .into());
    }
    let secret: FlowSecret =
        serde_json::from_str(&vault_get(&root, &flow_key(&flow))?.ok_or_else(|| {
            auth_error(
                "login_secret_unavailable",
                "The saved login secret is unavailable; start a new login.",
            )
        })?)
        .map_err(|_| {
            auth_error(
                "invalid_login_secret",
                "The saved login secret is invalid; start a new login.",
            )
        })?;
    if secret.schema_version != 1
        || secret.id != id
        || secret.origin != flow.origin
        || !valid_secret(&secret.device_code, "miy_device_")
    {
        return Err(auth_error(
            "login_secret_binding_invalid",
            "The login secret does not match the saved login request.",
        )
        .into());
    }
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        if now() >= flow.expires_at {
            clean_flow(&root, &flow);
            return Err(restart_login(
                &flow,
                "login_expired",
                "The login request expired; start a new login.",
            )
            .into());
        }
        let delay = (flow.next_poll_at - now())
            .max(0)
            .min(flow.expires_at - now());
        tokio::select! {
            _ = &mut interrupt => return Err(interrupted(&flow).into()),
            _ = tokio::time::sleep(Duration::from_secs(delay as u64)) => {}
        }
        if now() >= flow.expires_at {
            continue;
        }
        // Persist throttling before the request so another invocation respects it.
        flow.next_poll_at = now() + flow.interval as i64;
        save_flow(&root, &flow)?;
        let request = public_post(
            &context,
            "/auth/cli/token",
            json!({"grantType":"device_code","id":id,"deviceCode":secret.device_code}),
        );
        let reply = tokio::select! {
            _ = &mut interrupt => return Err(interrupted(&flow).into()),
            reply = request => reply,
        };
        let reply = match reply {
            Ok(reply) => reply,
            Err(error) => {
                let classified = crate::cli_error::classify(&error);
                if matches!(
                    classified.code.as_str(),
                    "access_denied" | "expired_token" | "invalid_grant"
                ) {
                    clean_flow(&root, &flow);
                    return Err(restart_login(
                        &flow,
                        "login_not_approved",
                        "The login request was denied, expired or consumed; start a new login.",
                    )
                    .into());
                }
                return Err(error);
            }
        };
        let parsed: TokenReply =
            serde_json::from_value(reply.clone()).map_err(|_| invalid_response())?;
        match parsed {
            TokenReply::Pending { interval } | TokenReply::SlowDown { interval } => {
                if !(1..=60).contains(&interval) {
                    return Err(invalid_response().into());
                }
                flow.interval = flow.interval.max(interval);
                flow.next_poll_at = now() + flow.interval as i64;
                save_flow(&root, &flow)?;
            }
            TokenReply::Approved { .. } => {
                let credential = credential_from_reply(reply, &flow.origin)?;
                verify_identity(&context, &credential).await?;
                let _credential_lock =
                    lock(&root, &credential_key(&context, &credential.user_id)).await?;
                let current = cli_config::named(&flow.context)?;
                if current.origin != flow.origin {
                    return Err(auth_error(
                        "login_context_changed",
                        "The context server changed during login; start a new login.",
                    )
                    .into());
                }
                let key = credential_key(&context, &credential.user_id);
                let previous = vault_get(&root, &key)?;
                if persist(&root, &context, &credential).is_err() {
                    let revoked = revoke_issued(&context, &credential).await;
                    return Err(CliError::new(Category::Environment,"login_credential_not_saved","The issued session could not be saved. Repair credential storage and start a new login.")
                        .with_details(json!({"credentialStored":false,"remoteSessionRevoked":revoked}))
                        .with_recovery("miy",&["auth","login"]).into());
                }
                if cli_config::set_user(&flow.context, Some(credential.user_id.clone())).is_err() {
                    let restored = match previous {
                        Some(previous) => vault_set(&root, &key, &previous).is_ok(),
                        None => vault_remove(&root, &key).is_ok(),
                    };
                    let revoked = revoke_issued(&context, &credential).await;
                    return Err(CliError::new(Category::Environment,"login_context_update_failed","The context could not be updated. Local rollback and remote revocation were attempted; repair the context and start a new login.")
                        .with_details(json!({"contextUpdated":false,"previousCredentialRestored":restored,"remoteSessionRevoked":revoked}))
                        .with_recovery("miy",&["auth","login"]).into());
                }
                let cleanup = clean_flow(&root, &flow);
                return Ok(
                    json!({"status":"authenticated","context":flow.context,"userId":credential.user_id,
                    "sessionId":credential.session_id,"expiresAt":credential.expires_at,"refreshExpiresAt":credential.refresh_expires_at,"loginFlowRemoved":cleanup}),
                );
            }
        }
    }
}

fn restart_login(flow: &Flow, code: &str, message: &str) -> CliError {
    CliError::new(Category::Auth, code, format!("{message} Run: miy --context {} auth login", flow.context))
        .with_details(json!({"loginId":flow.id,"expiresAt":flow.expires_at,"expiresIn":(flow.expires_at-now()).max(0)}))
        .with_recovery("miy", &["--context", &flow.context, "auth", "login"])
}

fn interrupted(flow: &Flow) -> CliError {
    CliError::new(
        Category::Interrupted,
        "login_wait_interrupted",
        "Stopped waiting locally; the login request remains available until it expires.",
    )
    .with_details(json!({"loginId":flow.id,"expiresAt":flow.expires_at}))
    .with_recovery(
        "miy",
        &[
            "--context",
            &flow.context,
            "auth",
            "wait",
            &flow.id.to_string(),
        ],
    )
}

fn validate_development(origin: &str, account: &str) -> Result<()> {
    let url = url::Url::parse(origin).map_err(|_| invalid_response())?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        || !crate::contract::valid_id(account)
    {
        return Err(CliError::new(
            Category::Usage,
            "development_login_requires_loopback_origin",
            "--account is allowed only for an explicit HTTP loopback development server.",
        )
        .into());
    }
    Ok(())
}

async fn development_approve(
    context: &cli_config::Context,
    flow: &Flow,
    account: &str,
) -> Result<()> {
    validate_development(&context.origin, account)?;
    let client = crate::cli_client::client(context.ca_file.as_deref(), HTTP_SECONDS)?;
    let response = client
        .post(format!("{}/auth/development", context.origin))
        .header("origin", &context.origin)
        .json(&json!({"account":account}))
        .send()
        .await
        .map_err(crate::cli_error::transport)?;
    if !response.status().is_success() {
        self::response(response).await?;
        unreachable!();
    }
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or_else(invalid_response)?
        .to_owned();
    let me = self::response(
        client
            .get(format!("{}/api/v1/me", context.origin))
            .header("cookie", &cookie)
            .send()
            .await
            .map_err(crate::cli_error::transport)?,
    )
    .await?;
    let csrf = me["csrf"]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or_else(invalid_response)?;
    self::response(
        client
            .post(format!(
                "{}/api/v1/cli/devices/{}/decision",
                context.origin, flow.id
            ))
            .header("cookie", &cookie)
            .header("origin", &context.origin)
            .header("x-miy-csrf", csrf)
            .json(&json!({"userCode":flow.user_code,"decision":"approve"}))
            .send()
            .await
            .map_err(crate::cli_error::transport)?,
    )
    .await?;
    Ok(())
}

async fn logout(local_only: bool) -> Result<Value> {
    let (name, context) = cli_config::selected()?;
    let Some(user) = context.user_id.as_deref() else {
        return Ok(json!({"signedOut":true,"alreadySignedOut":true,"remoteRevoked":false}));
    };
    let root = cli_config::root()?;
    let key = credential_key(&context, user);
    let _lock = lock(&root, &key).await?;
    if !local_only {
        let credential = read_credential(&root, &context, user)?;
        let revoked=public_post(&context,"/auth/cli/revoke",json!({"refreshToken":credential.refresh_token})).await
            .map_err(|_|CliError::new(Category::Network,"logout_remote_failed","Server revocation could not be confirmed. Local credentials were retained; retry or explicitly use --local-only.")
                .with_details(json!({"remoteRevoked":null,"localCredentialsRemoved":false}))
                .with_recovery("miy",&["--context",&name,"auth","logout"]))?;
        if revoked["revoked"] != true {
            return Err(CliError::new(
                Category::Network,
                "logout_remote_unconfirmed",
                "The server did not confirm revocation; local credentials were retained.",
            )
            .with_details(json!({"remoteRevoked":null,"localCredentialsRemoved":false}))
            .with_recovery("miy", &["--context", &name, "auth", "logout"])
            .into());
        }
    }
    vault_remove(&root, &key).map_err(|_| {
        CliError::new(
            Category::Environment,
            "logout_local_cleanup_failed",
            "Server revocation and local removal are separate; local credential removal failed.",
        )
        .with_details(json!({"remoteRevoked":!local_only,"localCredentialsRemoved":false}))
        .with_recovery(
            "miy",
            &["--context", &name, "auth", "logout", "--local-only"],
        )
    })?;
    cli_config::set_user(&name,None).map_err(|_|CliError::new(Category::Environment,"logout_context_cleanup_failed","The credential was removed but the context user binding could not be cleared.")
        .with_details(json!({"remoteRevoked":!local_only,"localCredentialsRemoved":true,"contextUpdated":false}))
        .with_recovery("miy",&["--context",&name,"auth","logout","--local-only"]))?;
    Ok(
        json!({"signedOut":true,"context":name,"remoteRevoked":!local_only,"localCredentialsRemoved":true}),
    )
}

async fn revoke_issued(context: &cli_config::Context, credential: &Credential) -> bool {
    public_post(
        context,
        "/auth/cli/revoke",
        json!({"refreshToken":credential.refresh_token}),
    )
    .await
    .is_ok_and(|value| value["revoked"] == true)
}

pub async fn execute(command: AuthCommand) -> Result<Value> {
    match command {
        AuthCommand::Login {
            device_code,
            no_wait,
            no_open,
            account,
        } => login(device_code, no_wait, no_open, account).await,
        AuthCommand::Wait { id } => wait(id).await,
        AuthCommand::Logout { local_only } => logout(local_only).await,
        AuthCommand::Status => {
            let session = refreshed(&session()?).await?;
            let name = session.context.as_deref().ok_or_else(|| {
                auth_error(
                    "context_required",
                    "Select a context before inspecting authentication.",
                )
            })?;
            let context = context_for_session(&session)?;
            let credential = read_credential(
                &cli_config::root()?,
                &context,
                session.user_id.as_deref().unwrap_or_default(),
            )?;
            usable(&credential)?;
            verify_identity(&context, &credential).await?;
            Ok(
                json!({"authenticated":true,"context":name,"userId":credential.user_id,"sessionId":credential.session_id,
                "expiresAt":credential.expires_at,"refreshExpiresAt":credential.refresh_expires_at}),
            )
        }
        AuthCommand::Sessions { command } => {
            let session = refreshed(&session()?).await?;
            let context = context_for_session(&session)?;
            let token = session.token.as_deref().ok_or_else(|| {
                auth_error("login_required", "Sign in before inspecting sessions.")
            })?;
            match command {
                SessionCommand::List => {
                    let mut endpoint =
                        url::Url::parse(&format!("{}/api/v1/cli/sessions", context.origin))
                            .map_err(|_| invalid_response())?;
                    let options = cli_config::options();
                    endpoint.query_pairs_mut().append_pair(
                        "limit",
                        &if options.limit == 0 {
                            50
                        } else {
                            options.limit.min(200)
                        }
                        .to_string(),
                    );
                    if let Some(cursor) = options.cursor.as_deref() {
                        endpoint.query_pairs_mut().append_pair("cursor", cursor);
                    }
                    let path = format!(
                        "{}?{}",
                        endpoint.path(),
                        endpoint.query().unwrap_or_default()
                    );
                    let value = authenticated(&context, token, &path, reqwest::Method::GET).await?;
                    let list: SessionList =
                        serde_json::from_value(value).map_err(|_| invalid_response())?;
                    Ok(serde_json::to_value(list)?)
                }
                SessionCommand::Revoke { id } => {
                    cli_config::confirm(&format!("Revoke CLI session {id}?"))?;
                    let value = authenticated(
                        &context,
                        token,
                        &format!("/api/v1/cli/sessions/{id}"),
                        reqwest::Method::DELETE,
                    )
                    .await?;
                    if value["id"].as_str() != Some(&id.to_string()) || value["revoked"] != true {
                        return Err(invalid_response().into());
                    }
                    Ok(json!({"id":id,"revoked":true}))
                }
            }
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionList {
    sessions: Vec<SessionSummary>,
    next_cursor: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionSummary {
    id: Uuid,
    client_name: String,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    last_used_at: chrono::DateTime<chrono::Utc>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential() -> Credential {
        Credential {
            schema_version: 1,
            origin: "https://platform.example".into(),
            user_id: "alice".into(),
            session_id: Uuid::new_v4(),
            access_token: format!("miy_access_{}", "a".repeat(40)),
            refresh_token: format!("miy_refresh_{}", "b".repeat(40)),
            expires_at: now() + 900,
            refresh_expires_at: now() + 86400,
            reauth_required: false,
        }
    }

    #[test]
    fn credentials_are_bound_to_origin_and_subject_and_uncertain_rotation_fails_closed() {
        let mut credential = credential();
        assert!(validate_credential(&credential, "https://platform.example", "alice").is_ok());
        assert!(validate_credential(&credential, "https://other.example", "alice").is_err());
        assert!(validate_credential(&credential, "https://platform.example", "bob").is_err());
        assert!(usable(&credential).is_ok());
        credential.reauth_required = true;
        assert_eq!(
            usable(&credential)
                .unwrap_err()
                .downcast_ref::<CliError>()
                .unwrap()
                .category,
            Category::Auth
        );
    }

    #[test]
    fn public_flow_metadata_excludes_private_device_and_session_tokens() {
        let flow = Flow {
            schema_version: 1,
            id: Uuid::new_v4(),
            context: "dev".into(),
            origin: "http://localhost:18000".into(),
            user_code: "ABCDE-FGHJK".into(),
            expires_at: now() + 300,
            interval: 5,
            next_poll_at: 0,
        };
        let value = pending(&flow);
        assert_eq!(value["recovery"]["args"][0], "--context");
        assert_eq!(value["userCode"], "ABCDE-FGHJK");
        for forbidden in [
            "deviceCode",
            "accessToken",
            "refreshToken",
            "cookie",
            "csrf",
        ] {
            assert!(value.get(forbidden).is_none());
            assert!(
                serde_json::to_value(&flow)
                    .unwrap()
                    .get(forbidden)
                    .is_none()
            );
        }
    }

    #[test]
    fn wire_validation_rejects_unknown_fields_and_unexpected_secret_formats() {
        let credential = credential();
        let mut value = json!({"status":"approved","sessionId":credential.session_id,"userId":"alice",
            "accessToken":credential.access_token,"refreshToken":credential.refresh_token,"tokenType":"Bearer","expiresIn":900,
            "refreshExpiresAt":chrono::Utc::now()+chrono::Duration::days(1)});
        assert!(credential_from_reply(value.clone(), "https://platform.example").is_ok());
        value["unexpected"] = json!("TOKEN-SENTINEL");
        let error = credential_from_reply(value, "https://platform.example")
            .err()
            .unwrap();
        assert!(!error.to_string().contains("TOKEN-SENTINEL"));
        assert!(!valid_secret("miy_refresh_short", "miy_refresh_"));
    }

    #[test]
    fn development_shortcut_is_explicit_http_loopback_only() {
        assert!(validate_development("http://127.0.0.1:18000", "admin").is_ok());
        assert!(validate_development("http://[::1]:18000", "admin").is_ok());
        assert!(validate_development("https://platform.example", "admin").is_err());
        assert!(validate_development("http://platform.example", "admin").is_err());
        assert!(!valid_user_code("TOKEN-SENTINEL"));
    }

    #[test]
    fn authentication_errors_do_not_reflect_credentials_or_request_ids() {
        let secret = format!("miy_refresh_{}", "a".repeat(40));
        let failure = auth_http_error(
            reqwest::StatusCode::UNAUTHORIZED,
            Some(&json!({"error":{"code":secret,"requestId":secret}})),
        );
        assert_eq!(failure.category, Category::Auth);
        let rendered = serde_json::to_string(&failure).unwrap();
        assert!(!rendered.contains(&secret));
        let failure = auth_http_error(
            reqwest::StatusCode::FORBIDDEN,
            Some(&json!({"error":{"code":"user_disabled"}})),
        );
        assert_eq!(failure.code, "user_disabled");
    }

    #[tokio::test]
    async fn authentication_lock_serializes_writers_and_has_a_bounded_wait() {
        let root = tempfile::tempdir().unwrap();
        let key = "a".repeat(64);
        let first = lock_with_timeout(root.path(), &key, Duration::from_millis(20))
            .await
            .unwrap();
        let error = lock_with_timeout(root.path(), &key, Duration::from_millis(20))
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.downcast_ref::<CliError>().unwrap().category,
            Category::Conflict
        );
        drop(first);
        assert!(
            lock_with_timeout(root.path(), &key, Duration::from_millis(20))
                .await
                .is_ok()
        );
    }
}
