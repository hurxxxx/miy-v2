//! Explicit server contexts and project bindings. Credentials live in the OS store.
use crate::{
    cli_error::{Category, CliError},
    platform,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Args, Clone, Debug, Default)]
pub struct Options {
    #[arg(long, global = true)]
    pub context: Option<String>,
    #[arg(long, global = true)]
    pub app: Option<String>,
    #[arg(long, global = true)]
    pub project: Option<PathBuf>,
    #[arg(long, global = true)]
    pub no_input: bool,
    #[arg(long, global = true)]
    pub yes: bool,
    #[arg(long, global = true)]
    pub fields: Option<String>,
    #[arg(long, global=true, default_value="50",value_parser=clap::value_parser!(u32).range(1..=200))]
    pub limit: u32,
    #[arg(long, global = true)]
    pub cursor: Option<String>,
}
static OPTIONS: OnceLock<Options> = OnceLock::new();
pub fn options() -> &'static Options {
    OPTIONS.get_or_init(Options::default)
}
pub fn initialize(options: Options) -> Result<()> {
    if let Some(path) = &options.project {
        std::env::set_current_dir(path).map_err(|_| {
            CliError::new(
                Category::Environment,
                "project_unavailable",
                "The project directory is unavailable.",
            )
        })?;
    }
    OPTIONS
        .set(options)
        .map_err(|_| anyhow::anyhow!("CLI options initialized twice"))
}
pub fn interactive() -> bool {
    !options().no_input
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Context {
    pub origin: String,
    pub ca_file: Option<PathBuf>,
    pub user_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    version: u32,
    client_id: uuid::Uuid,
    active: Option<String>,
    contexts: BTreeMap<String, Context>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            client_id: uuid::Uuid::new_v4(),
            active: None,
            contexts: BTreeMap::new(),
        }
    }
}

pub fn root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("MIY_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        Ok(PathBuf::from(std::env::var_os("APPDATA").ok_or_else(|| {
            CliError::new(
                Category::Environment,
                "config_directory_unavailable",
                "APPDATA is unavailable; set MIY_CONFIG_DIR.",
            )
        })?)
        .join("miy"))
    }
    #[cfg(not(windows))]
    {
        if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(path).join("miy"));
        }
        Ok(PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            CliError::new(
                Category::Environment,
                "config_directory_unavailable",
                "HOME is unavailable; set MIY_CONFIG_DIR.",
            )
        })?)
        .join(".config/miy"))
    }
}
pub fn ensure_root() -> Result<PathBuf> {
    ensure_root_path(root()?)
}

#[cfg(target_os = "macos")]
fn trusted_system_ancestor(path: &Path, root: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    if path == root {
        return false;
    }
    let (relative, absolute) = match path.to_str() {
        Some("/var") => ("private/var", "/private/var"),
        Some("/tmp") => ("private/tmp", "/private/tmp"),
        _ => return false,
    };
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| meta.file_type().is_symlink() && meta.uid() == 0)
        && std::fs::read_link(path)
            .is_ok_and(|target| target == Path::new(relative) || target == Path::new(absolute))
}

fn ensure_root_path(root: PathBuf) -> Result<PathBuf> {
    for path in root.ancestors() {
        #[cfg(target_os = "macos")]
        if trusted_system_ancestor(path, &root) {
            continue;
        }
        platform::reject_reparse(path).map_err(|_| CliError::new(
            Category::Environment,
            "config_path_unavailable",
            "Use an accessible, canonical configuration path without symlinks or reparse points.",
        ))?;
    }
    let exists = root.exists();
    std::fs::create_dir_all(&root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if exists && std::fs::metadata(&root)?.permissions().mode() & 0o077 != 0 {
            return Err(CliError::new(
                Category::Environment,
                "config_directory_not_private",
                "Choose a dedicated private MIY_CONFIG_DIR with mode 0700.",
            )
            .into());
        }
        if !exists {
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    #[cfg(not(unix))]
    let _ = exists;
    Ok(root)
}
fn load() -> Result<Config> {
    let path = root()?.join("config.json");
    platform::reject_reparse(&path)?;
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e.into()),
    };
    let value: Config = serde_json::from_slice(&data).map_err(|_| {
        CliError::new(
            Category::Environment,
            "invalid_context_config",
            "The context configuration is invalid.",
        )
    })?;
    if value.version != 1 {
        return Err(CliError::new(
            Category::Environment,
            "unsupported_config_version",
            "The context configuration version is unsupported.",
        )
        .into());
    }
    Ok(value)
}
fn save(config: &Config) -> Result<()> {
    platform::write_private(
        &ensure_root()?.join("config.json"),
        &serde_json::to_vec_pretty(config)?,
    )
}
fn lock() -> Result<std::fs::File> {
    let path = ensure_root()?.join("config.lock");
    platform::reject_reparse(&path)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    lock.try_lock().map_err(|_| {
        CliError::new(
            Category::Conflict,
            "context_config_in_use",
            "Another CLI is updating the context configuration; retry when it finishes.",
        )
    })?;
    Ok(lock)
}
pub fn client_id() -> Result<uuid::Uuid> {
    let _lock = lock()?;
    let config = load()?;
    let id = config.client_id;
    save(&config)?;
    Ok(id)
}
pub fn selected() -> Result<(String, Context)> {
    let config = load()?;
    let explicit = options()
        .context
        .clone()
        .or_else(|| std::env::var("MIY_CONTEXT").ok());
    let binding = if explicit.is_none() {
        match std::fs::read(".miy/context.json") {
            Ok(bytes) => Some(
                serde_json::from_slice::<Value>(&bytes).map_err(|_| {
                    CliError::new(
                        Category::Environment,
                        "invalid_project_context",
                        "The project context binding is invalid.",
                    )
                })?["context"]
                    .as_str()
                    .ok_or_else(|| {
                        CliError::new(
                            Category::Environment,
                            "invalid_project_context",
                            "The project context binding must name a context.",
                        )
                    })?
                    .to_owned(),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    let name = explicit.or(binding).or(config.active).ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "context_required",
            "Add and select a server context before remote operations.",
        )
        .with_recovery("miy", &["context", "init"])
    })?;
    let context = config.contexts.get(&name).cloned().ok_or_else(|| {
        CliError::new(
            Category::NotFound,
            "context_not_found",
            "The selected context does not exist.",
        )
    })?;
    Ok((name, context))
}
pub fn optional_selected() -> Result<Option<(String, Context)>> {
    match selected() {
        Ok(value) => Ok(Some(value)),
        Err(error)
            if error
                .downcast_ref::<CliError>()
                .is_some_and(|e| e.code == "context_required") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
pub fn named(name: &str) -> Result<Context> {
    load()?.contexts.get(name).cloned().ok_or_else(|| {
        CliError::new(
            Category::NotFound,
            "context_not_found",
            "The named context does not exist.",
        )
        .into()
    })
}
pub fn set_user(name: &str, user: Option<String>) -> Result<()> {
    let _lock = lock()?;
    let mut config = load()?;
    config
        .contexts
        .get_mut(name)
        .ok_or_else(|| {
            CliError::new(
                Category::NotFound,
                "context_not_found",
                "The context was removed.",
            )
        })?
        .user_id = user;
    save(&config)
}
pub fn credential_key(context: &Context, user: &str) -> String {
    crate::contract::hash(format!("session\0{}\0{user}", context.origin))
}
pub fn bind_project(path: &Path) -> Result<()> {
    if let Some((name, _)) = optional_selected()? {
        platform::write_private(
            &path.join(".miy/context.json"),
            &serde_json::to_vec_pretty(&json!({"context":name}))?,
        )?;
    }
    Ok(())
}

#[derive(Subcommand)]
pub enum ContextCommand {
    /// Register an explicitly supplied server, or ask for its URL on a terminal.
    Init {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        ca_file: Option<PathBuf>,
    },
    /// Add a named server origin without storing credentials in the project.
    Add {
        name: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        ca_file: Option<PathBuf>,
    },
    List,
    Show {
        name: Option<String>,
    },
    Use {
        name: String,
    },
    /// Remove a local context binding; remote sessions remain until explicitly revoked.
    Remove {
        name: String,
    },
}
pub fn execute(command: ContextCommand) -> Result<Value> {
    match command {
        ContextCommand::Init { url, ca_file } => initialize_context(url, ca_file),
        ContextCommand::List => {
            context_page(&load()?, options().limit, options().cursor.as_deref())
        }
        ContextCommand::Show { name } => {
            let (name, context) = if let Some(name) = name {
                (name.clone(), named(&name)?)
            } else {
                selected()?
            };
            Ok(json!({"name":name,"context":context}))
        }
        ContextCommand::Add { name, url, ca_file } => {
            if !crate::contract::valid_id(&name) {
                return Err(CliError::new(
                    Category::Usage,
                    "invalid_context_name",
                    "Use a lowercase context identifier.",
                )
                .into());
            }
            let origin = crate::cli_client::origin(&url)?;
            let ca_file = ca_file.map(|path| path.canonicalize()).transpose()?;
            crate::cli_client::client(ca_file.as_deref(), 20)?;
            let _lock = lock()?;
            let mut c = load()?;
            if c.contexts.contains_key(&name) {
                return Err(CliError::new(
                    Category::Conflict,
                    "context_exists",
                    "This context already exists; use a different name.",
                )
                .into());
            }
            c.contexts.insert(
                name.clone(),
                Context {
                    origin,
                    ca_file,
                    user_id: None,
                },
            );
            if c.active.is_none() {
                c.active = Some(name.clone());
            }
            save(&c)?;
            Ok(json!({"added":name,"active":c.active}))
        }
        ContextCommand::Use { name } => {
            let _lock = lock()?;
            let mut c = load()?;
            if !c.contexts.contains_key(&name) {
                return Err(CliError::new(
                    Category::NotFound,
                    "context_not_found",
                    "The named context does not exist.",
                )
                .into());
            }
            c.active = Some(name.clone());
            save(&c)?;
            Ok(json!({"active":name}))
        }
        ContextCommand::Remove { name } => {
            named(&name)?;
            confirm(&format!(
                "Remove local context {name}? Unshared local credentials will be removed; server sessions and projects will remain."
            ))?;
            let _lock = lock()?;
            let mut c = load()?;
            let context = c.contexts.remove(&name).ok_or_else(|| {
                CliError::new(
                    Category::NotFound,
                    "context_not_found",
                    "The context was removed by another process.",
                )
            })?;
            let mut local_credentials_removed = false;
            if let Some(user) = &context.user_id {
                let shared = c.contexts.values().any(|other| {
                    other.origin == context.origin && other.user_id.as_ref() == Some(user)
                });
                if !shared {
                    crate::credential_store::remove(&ensure_root()?,&credential_key(&context,user)).map_err(|_|CliError::new(Category::Environment,"credential_storage_unavailable","Local credentials could not be removed; the context configuration was preserved."))?;
                    local_credentials_removed = true;
                }
            }
            if c.active.as_deref() == Some(&name) {
                c.active = None;
            }
            save(&c)?;
            Ok(
                json!({"removed":name,"localCredentialsRemoved":local_credentials_removed,"remoteSessionsRevoked":false,"projectDataPreserved":true}),
            )
        }
    }
}

fn preserved_context(config: &Config, requested: Option<&str>) -> Value {
    let active_origin = config
        .active
        .as_ref()
        .and_then(|name| config.contexts.get(name))
        .map(|context| context.origin.as_str());
    let mismatch = requested.is_some_and(|origin| active_origin != Some(origin));
    let mut next = Vec::new();
    if let Some(origin) = requested.filter(|_| mismatch) {
        // These are argument arrays, not shell source. A URL must never be
        // interpolated into a suggested command without shell-specific quoting.
        if let Some((name, _)) = config.contexts.iter().find(|(_, c)| c.origin == origin) {
            next.push(json!({"program":"miy","args":["context","use",name]}));
        } else {
            let mut name = "work".to_owned();
            let mut suffix = 2;
            while config.contexts.contains_key(&name) {
                name = format!("work-{suffix}");
                suffix += 1;
            }
            next.push(json!({"program":"miy","args":["context","add",name,"--url",origin]}));
            next.push(json!({"program":"miy","args":["context","use",name]}));
        }
    } else if active_origin.is_none() {
        next.push(json!({"program":"miy","args":["context","list"]}));
        next.push(json!({"program":"miy","args":["context","add","work","--url","<server-url>"]}));
    } else {
        next.push(json!({"program":"miy","args":["context","show"]}));
    }
    json!({"status":"preserved","active":config.active,"activeOrigin":active_origin,
        "requestedOrigin":requested,"serverMismatch":mismatch,"setupRequired":active_origin.is_none(),
        "next":next})
}

fn prompt_server_origin() -> Result<String> {
    if !interactive() {
        return Err(CliError::new(
            Category::Usage,
            "server_url_required",
            "No server is configured. Supply --url with an explicit HTTP or HTTPS server URL, or run miy context init in a terminal.",
        )
        .with_recovery("miy", &["context", "init"])
        .into());
    }
    loop {
        eprint!("Target server URL (include http:// or https://; blank cancels): ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if answer.trim().is_empty() {
            return Err(CliError::new(
                Category::Interrupted,
                "server_setup_cancelled",
                "Server registration was cancelled. No default server was configured.",
            )
            .with_recovery("miy", &["context", "init"])
            .into());
        }
        match crate::cli_client::origin(answer.trim()) {
            Ok(origin) => return Ok(origin),
            Err(_) => eprintln!(
                "Enter a complete HTTP or HTTPS origin without credentials, path, query or fragment."
            ),
        }
    }
}

fn initialize_context(url: Option<String>, ca_file: Option<PathBuf>) -> Result<Value> {
    let requested = url.as_deref().map(crate::cli_client::origin).transpose()?;
    let ca_file = ca_file.map(std::fs::canonicalize).transpose()?;
    crate::cli_client::client(ca_file.as_deref(), 20)?;
    let path = root()?.join("config.json");
    platform::reject_reparse(&path)?;
    if path.try_exists()? {
        // An intentionally empty configuration also belongs to the user.
        // Validate its private directory and serialize with other config writers.
        let _lock = lock()?;
        return Ok(preserved_context(&load()?, requested.as_deref()));
    }
    let origin = match requested {
        Some(origin) => origin,
        None => prompt_server_origin()?,
    };
    // Do not hold the configuration lock while waiting for human input.
    let _lock = lock()?;
    platform::reject_reparse(&path)?;
    if path.try_exists()? {
        return Ok(preserved_context(&load()?, Some(&origin)));
    }
    let mut config = Config {
        active: Some("default".into()),
        ..Config::default()
    };
    config.contexts.insert(
        "default".into(),
        Context {
            origin: origin.clone(),
            ca_file,
            user_id: None,
        },
    );
    save(&config)?;
    Ok(
        json!({"status":"initialized","active":"default","context":"default",
        "origin":origin,"setupRequired":false,"serverMismatch":false}),
    )
}

/// Only a fresh, interactive login may request a server. Existing configurations
/// and explicit/project selections retain their normal selection semantics.
pub fn login_context() -> Result<(String, Context)> {
    match selected() {
        Ok(context) => Ok(context),
        Err(error)
            if interactive()
                && error
                    .downcast_ref::<CliError>()
                    .is_some_and(|error| error.code == "context_required") =>
        {
            let setup = initialize_context(None, None)?;
            if setup["setupRequired"] == true {
                return Err(CliError::new(
                    Category::Environment,
                    "context_required",
                    "Existing server configuration was preserved. Add a context with miy context add NAME --url URL, then select it with miy context use NAME.",
                )
                .with_details(setup)
                .with_recovery("miy", &["context", "list"])
                .into());
            }
            selected()
        }
        Err(error) => Err(error),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextCursor {
    version: u8,
    scope: String,
    boundary: String,
}

fn context_page(config: &Config, limit: u32, cursor: Option<&str>) -> Result<Value> {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let limit = if limit == 0 { 50 } else { limit };
    if limit > 200 {
        return Err(CliError::new(
            Category::Usage,
            "invalid_page_limit",
            "Use a page limit from 1 to 200.",
        )
        .into());
    }
    // Bind the complete snapshot, including origin/user changes, to this local client.
    let scope = crate::contract::hash(serde_json::to_vec(&json!([
        "context list",
        config.client_id,
        config.contexts
    ]))?);
    let invalid = || {
        CliError::new(
            Category::Usage,
            "invalid_cursor",
            "The cursor is invalid for this context list; restart from the first page.",
        )
    };
    let boundary = if let Some(raw) = cursor {
        if raw.len() > 4096 {
            return Err(invalid().into());
        }
        let value: ContextCursor = URL_SAFE_NO_PAD
            .decode(raw)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(invalid)?;
        if value.version != 1
            || value.scope != scope
            || !config.contexts.contains_key(&value.boundary)
        {
            return Err(invalid().into());
        }
        Some(value.boundary)
    } else {
        None
    };
    let mut entries = config
        .contexts
        .iter()
        .filter(|(name, _)| boundary.as_ref().is_none_or(|boundary| *name > boundary));
    let page: BTreeMap<_, _> = entries.by_ref().take(limit as usize).collect();
    let next_cursor = if entries.next().is_some() {
        page.last_key_value()
            .map(|(name, _)| {
                serde_json::to_vec(&ContextCursor {
                    version: 1,
                    scope,
                    boundary: (*name).clone(),
                })
                .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
            })
            .transpose()?
    } else {
        None
    };
    Ok(json!({"active":config.active,"contexts":page,"nextCursor":next_cursor}))
}

pub fn confirm(message: &str) -> Result<()> {
    if options().yes {
        return Ok(());
    }
    if !interactive() {
        return Err(CliError::new(
            Category::Usage,
            "confirmation_required",
            "Explicit confirmation is required; inspect the target and use --yes to proceed.",
        )
        .into());
    }
    eprintln!("{message} Type yes to continue:");
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if answer.trim() != "yes" {
        return Err(CliError::new(
            Category::Interrupted,
            "confirmation_declined",
            "The operation was not confirmed.",
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod pagination_tests {
    use super::*;
    #[test]
    fn preservation_reports_missing_setup_and_exact_server_selection_without_defaults() {
        let mut config = Config::default();
        let empty = preserved_context(&config, None);
        assert_eq!(empty["status"], "preserved");
        assert_eq!(empty["setupRequired"], true);
        assert!(empty["activeOrigin"].is_null());
        assert!(empty["requestedOrigin"].is_null());
        let requested = "http://192.168.10.2:8080";
        let empty_request = preserved_context(&config, Some(requested));
        assert_eq!(empty_request["serverMismatch"], true);
        assert_eq!(empty_request["next"][0]["args"][4], requested);
        config.contexts.insert(
            "work".into(),
            Context {
                origin: requested.into(),
                ca_file: None,
                user_id: None,
            },
        );
        let selected = preserved_context(&config, Some(requested));
        assert_eq!(selected["next"].as_array().unwrap().len(), 1);
        assert_eq!(
            selected["next"][0]["args"],
            json!(["context", "use", "work"])
        );
        config.active = Some("work".into());
        let same = preserved_context(&config, Some(requested));
        assert_eq!(same["setupRequired"], false);
        assert_eq!(same["serverMismatch"], false);
        let other = preserved_context(&config, Some("https://another.example"));
        assert_eq!(other["activeOrigin"], requested);
        assert_eq!(other["next"][0]["args"][2], "work-2");
    }
    #[cfg(unix)]
    #[test]
    fn config_owned_and_ancestor_links_are_rejected_before_creation() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let real = base.join("real");
        ensure_root_path(real.clone()).unwrap();
        let linked = base.join("linked");
        symlink(&real, &linked).unwrap();
        for path in [linked.clone(), linked.join("not-created")] {
            let error = ensure_root_path(path).unwrap_err();
            assert_eq!(
                crate::cli_error::classify(&error).code,
                "config_path_unavailable"
            );
        }
        assert!(!real.join("not-created").exists());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_trusted_temporary_ancestor_keeps_owned_links_forbidden() {
        let temp = tempfile::tempdir_in("/private/tmp").unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let alias = Path::new("/tmp").join(canonical.strip_prefix("/private/tmp").unwrap());
        assert!(trusted_system_ancestor(Path::new("/tmp"), &alias));
        assert!(!trusted_system_ancestor(
            Path::new("/tmp"),
            Path::new("/tmp")
        ));
        ensure_root_path(alias.join("private-config")).unwrap();
        assert!(canonical.join("private-config").is_dir());
    }

    fn fixture() -> Config {
        let mut config = Config {
            active: Some("charlie".into()),
            ..Config::default()
        };
        for name in ["charlie", "alpha", "bravo"] {
            config.contexts.insert(
                name.into(),
                Context {
                    origin: "https://platform.example".into(),
                    ca_file: None,
                    user_id: Some("alice".into()),
                },
            );
        }
        config
    }

    #[test]
    fn context_pages_preserve_map_and_active_with_exact_end() {
        let config = fixture();
        let first = context_page(&config, 1, None).unwrap();
        assert_eq!(first["active"], "charlie");
        assert_eq!(
            crate::cli_input::project(&first, "contexts.alpha.origin").unwrap(),
            json!({"contexts":{"alpha":{"origin":"https://platform.example"}}})
        );
        assert_eq!(
            first["contexts"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["alpha"]
        );
        let second = context_page(&config, 2, first["nextCursor"].as_str()).unwrap();
        assert_eq!(
            second["contexts"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["bravo", "charlie"]
        );
        assert!(second["nextCursor"].is_null());
        assert!(context_page(&Config::default(), 50, None).unwrap()["nextCursor"].is_null());
    }

    #[test]
    fn context_cursor_rejects_other_clients_changed_accounts_and_bad_input() {
        let mut config = fixture();
        let first = context_page(&config, 1, None).unwrap();
        let cursor = first["nextCursor"].as_str().unwrap();
        let client = config.client_id;
        config.client_id = uuid::Uuid::new_v4();
        assert_eq!(
            context_page(&config, 1, Some(cursor))
                .unwrap_err()
                .downcast_ref::<CliError>()
                .unwrap()
                .exit(),
            2
        );
        config.client_id = client;
        config.contexts.get_mut("alpha").unwrap().user_id = Some("bob".into());
        for raw in [cursor, "not-a-cursor", "", &"x".repeat(4097)] {
            let error = context_page(&config, 1, Some(raw)).unwrap_err();
            let error = error.downcast_ref::<CliError>().unwrap();
            assert_eq!(error.code, "invalid_cursor");
            assert_eq!(error.exit(), 2);
        }
        assert!(context_page(&config, 201, None).is_err());
    }
}
