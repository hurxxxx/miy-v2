//! Discovery and shell completion share the executable's Clap command tree.
use crate::cli_error::{Category, CliError};
use anyhow::Result;
use clap::{Arg, ArgAction, Command, CommandFactory};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{any::TypeId, collections::BTreeSet, io::Write, path::PathBuf};

#[derive(clap::Args, Debug, Default)]
pub struct DescribeArgs {
    /// Inspect one command, for example --command "app deploy".
    #[arg(long, conflicts_with_all = ["search", "all"])]
    pub command: Option<String>,
    /// Search command names and descriptions without loading the full reference.
    #[arg(long, conflicts_with_all = ["command", "all"])]
    pub search: Option<String>,
    /// Return the complete command reference explicitly.
    #[arg(long, conflicts_with_all = ["command", "search"])]
    pub all: bool,
}

pub fn inspect(args: DescribeArgs) -> Result<Value> {
    inspect_tree(crate::Args::command(), args)
}

fn inspect_tree(mut root: Command, args: DescribeArgs) -> Result<Value> {
    if usize::from(args.command.is_some())
        + usize::from(args.search.is_some())
        + usize::from(args.all)
        > 1
    {
        return Err(CliError::new(
            Category::Usage,
            "conflicting_discovery_options",
            "Choose one of --command, --search or --all.",
        )
        .into());
    }
    root.build();
    if let Some(query) = args.command {
        let mut words = query.split_whitespace().peekable();
        if words.peek() == Some(&"miy") {
            words.next();
        }
        let mut current = &root;
        let mut path = Vec::new();
        for word in words {
            current = current
                .get_subcommands()
                .find(|command| {
                    !command.is_hide_set()
                        && (command.get_name() == word
                            || command.get_all_aliases().any(|alias| alias == word))
                })
                .ok_or_else(unknown_command)?;
            path.push(current.get_name().to_owned());
        }
        if path.is_empty() {
            return Err(unknown_command().into());
        }
        return Ok(json!({
            "cliVersion":env!("CARGO_PKG_VERSION"),
            "scope":"command",
            "command":detail(current, &path),
        }));
    }
    let mut commands = Vec::new();
    if args.all {
        visit(&root, &[], &mut |command, path| {
            commands.push(detail(command, path));
        });
    } else if let Some(query) = args.search.as_ref() {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Err(CliError::new(
                Category::Usage,
                "empty_discovery_search",
                "Provide a nonempty command search.",
            )
            .into());
        }
        visit(&root, &[], &mut |command, path| {
            if path.join(" ").to_lowercase().contains(&query)
                || description(command).to_lowercase().contains(&query)
            {
                commands.push(summary(command, path));
            }
        });
    } else {
        for command in root.get_subcommands().filter(|command| visible(command)) {
            commands.push(summary(command, &[command.get_name().to_owned()]));
        }
    }
    Ok(json!({
        "cliVersion":env!("CARGO_PKG_VERSION"),
        "scope":if args.all { "all" } else if args.search.is_some() { "search" } else { "summary" },
        "commands":commands,
    }))
}

fn unknown_command() -> CliError {
    // Do not echo arbitrary input: command queries may accidentally contain credentials.
    CliError::new(
        Category::Usage,
        "unknown_command",
        "Select a command present in this CLI build.",
    )
    .with_recovery("miy", &["describe"])
}

fn visible(command: &Command) -> bool {
    !command.is_hide_set() && command.get_name() != "help"
}

fn visit(command: &Command, path: &[String], visitor: &mut impl FnMut(&Command, &[String])) {
    for child in command.get_subcommands().filter(|command| visible(command)) {
        let mut child_path = path.to_vec();
        child_path.push(child.get_name().to_owned());
        visitor(child, &child_path);
        visit(child, &child_path, visitor);
    }
}

fn description(command: &Command) -> String {
    command
        .get_about()
        .or_else(|| command.get_long_about())
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn summary(command: &Command, path: &[String]) -> Value {
    json!({
        "command":path.join(" "),
        "description":description(command),
        "hasSubcommands":command.get_subcommands().any(visible),
    })
}

fn detail(command: &Command, path: &[String]) -> Value {
    let name = path.join(" ");
    let arguments: Vec<_> = command
        .get_arguments()
        .filter(|argument| !argument.is_hide_set())
        .map(|argument| argument_detail(command, argument))
        .collect();
    let mut help = command.clone().bin_name(format!("miy {name}"));
    let policy = policy(&name, command.get_subcommands().any(visible));
    json!({
        "command":name,
        "description":description(command),
        "aliases":command.get_all_aliases().collect::<Vec<_>>(),
        "subcommands":command.get_subcommands().filter(|child| visible(child)).map(|child| {
            let mut child_path = path.to_vec();
            child_path.push(child.get_name().to_owned());
            summary(child, &child_path)
        }).collect::<Vec<_>>(),
        "arguments":arguments,
        "argumentGroups":command.get_groups().map(|group| {
            let mut group = group.clone();
            json!({"id":group.get_id().as_str(),"required":group.is_required_set(),
                "multiple":group.is_multiple(),"arguments":group.get_args().map(ToString::to_string).collect::<Vec<_>>()})
        }).collect::<Vec<_>>(),
        "inputSchema":input_schema(command),
        "outputSchema":output_schema(&name, policy.output),
        "dataSchema":data_schema(&name),
        "contracts":contracts(&name),
        "behavior":behavior(&name),
        "fieldSelectionSupported":crate::cli_input::validate_fields(&name,"id").is_ok(),
        "paginationSupported":matches!(name.as_str(),"app list"|"user list"|"grant list"|"registration list"|"job list"|"auth sessions list"|"context list"|"component list"),
        "pagination":pagination(&name),
        "schemaScope":{
            "input":"Explicit CLI arguments keyed by Clap argument ID, before default expansion. Clap remains the parser for relationships and command-specific validation.",
            "output":if policy.output == Output::Envelope {
                "The JSON envelope is specified; pre-dispatch parser errors use command miy. dataSchema describes unprojected data; --fields may select a subset. Shared request contracts have additional semantic validation."
            } else {
                "This command uses its native protocol or server process output, not a single JSON result envelope."
            }
        },
        "help":help.render_long_help().to_string(),
        "policy":policy,
        "errors":error_contract(),
        "errorExamples":[{"output":"json","exit":2,"response":{"schemaVersion":1,"command":"miy","ok":false,"data":null,
            "error":{"code":"invalid_arguments","message":"Invalid command arguments; inspect command help.","retryable":false,"details":{}},"meta":{}}}],
        "examples":examples(&name,path),
        "authenticationEffects":"Authenticated commands may renew the server session and update protected local credentials. These authentication effects are separate from app/runtime mutations.",
    })
}

fn argument_detail(command: &Command, argument: &Arg) -> Value {
    json!({
        "id":argument.get_id().as_str(),
        "long":argument.get_long(),
        "short":argument.get_short().map(|value| value.to_string()),
        "aliases":argument.get_all_aliases().unwrap_or_default(),
        "shortAliases":argument.get_all_short_aliases().unwrap_or_default(),
        "position":argument.get_index(),
        "global":argument.is_global_set(),
        "action":format!("{:?}",argument.get_action()),
        "required":argument.is_required_set() && argument.get_default_values().is_empty(),
        "description":argument.get_long_help().or_else(|| argument.get_help()).map(ToString::to_string),
        "environmentVariable":argument.get_env().map(|name| name.to_string_lossy().to_string()),
        "valueNames":argument.get_value_names().map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>()).unwrap_or_default(),
        "minValues":argument.get_num_args().map(|range| range.min_values()),
        "maxValues":argument.get_num_args().and_then(|range| (range.max_values() != usize::MAX).then_some(range.max_values())),
        "conflicts":conflicts(command, argument),
        "schema":argument_schema(argument),
    })
}

fn conflicts(command: &Command, argument: &Arg) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for other in command.get_arg_conflicts_with(argument) {
        ids.insert(other.get_id().to_string());
    }
    // Clap accepts conflicts declared on either argument; report the symmetric relation.
    for other in command.get_arguments() {
        if command
            .get_arg_conflicts_with(other)
            .iter()
            .any(|candidate| candidate.get_id() == argument.get_id())
        {
            ids.insert(other.get_id().to_string());
        }
    }
    ids.into_iter().collect()
}

fn argument_schema(argument: &Arg) -> Value {
    let id = argument.get_value_parser().type_id();
    let mut scalar = if matches!(
        argument.get_action(),
        ArgAction::SetTrue
            | ArgAction::SetFalse
            | ArgAction::Help
            | ArgAction::HelpShort
            | ArgAction::HelpLong
            | ArgAction::Version
    ) {
        json!({"type":"boolean"})
    } else if matches!(argument.get_action(), ArgAction::Count) {
        json!({"type":"integer","minimum":0,"maximum":u8::MAX})
    } else if id == TypeId::of::<u16>() {
        json!({"type":"integer","minimum":0,"maximum":u16::MAX})
    } else if id == TypeId::of::<u32>() {
        json!({"type":"integer","minimum":0,"maximum":u32::MAX})
    } else if id == TypeId::of::<u64>() || id == TypeId::of::<usize>() {
        json!({"type":"integer","minimum":0})
    } else if id == TypeId::of::<u8>() {
        json!({"type":"integer","minimum":0,"maximum":u8::MAX})
    } else if id == TypeId::of::<i8>()
        || id == TypeId::of::<i16>()
        || id == TypeId::of::<i32>()
        || id == TypeId::of::<i64>()
        || id == TypeId::of::<isize>()
    {
        json!({"type":"integer"})
    } else if id == TypeId::of::<f32>() || id == TypeId::of::<f64>() {
        json!({"type":"number"})
    } else if id == TypeId::of::<bool>() {
        json!({"type":"boolean"})
    } else if id == TypeId::of::<uuid::Uuid>() {
        json!({"type":"string","format":"uuid"})
    } else if id == TypeId::of::<PathBuf>() {
        json!({"type":"string","x-valueKind":"path"})
    } else {
        json!({"type":"string"})
    };
    let possible = argument.get_possible_values();
    if !possible.is_empty() {
        scalar["enum"] = json!(
            possible
                .iter()
                .map(|value| value.get_name())
                .collect::<Vec<_>>()
        );
    }
    let repeated = matches!(argument.get_action(), ArgAction::Append)
        || argument
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1);
    let mut schema = if repeated {
        json!({"type":"array","items":scalar})
    } else {
        scalar
    };
    let defaults = argument.get_default_values();
    if !defaults.is_empty() {
        let item = if repeated { &schema["items"] } else { &schema };
        let values: Vec<_> = defaults
            .iter()
            .map(|value| {
                let value = value.to_string_lossy();
                match item["type"].as_str() {
                    Some("boolean" | "integer" | "number") => {
                        serde_json::from_str(&value).unwrap_or_else(|_| json!(value))
                    }
                    _ => json!(value),
                }
            })
            .collect();
        schema["default"] = if repeated {
            json!(values)
        } else {
            values.into_iter().next().unwrap()
        };
    }
    schema
}

fn input_schema(command: &Command) -> Value {
    let arguments: Vec<_> = command
        .get_arguments()
        .filter(|argument| {
            !argument.is_hide_set()
                && !matches!(
                    argument.get_action(),
                    ArgAction::Help
                        | ArgAction::HelpShort
                        | ArgAction::HelpLong
                        | ArgAction::Version
                )
        })
        .collect();
    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut pairs = BTreeSet::new();
    for argument in &arguments {
        let id = argument.get_id().as_str();
        properties.insert(id.into(), argument_schema(argument));
        if argument.is_required_set() && argument.get_default_values().is_empty() {
            required.push(id);
        }
        for other in conflicts(command, argument) {
            if arguments
                .iter()
                .any(|argument| argument.get_id().as_str() == other)
            {
                let mut pair = [id.to_owned(), other];
                pair.sort();
                pairs.insert(pair);
            }
        }
    }
    let mut schema = json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object","properties":properties,"required":required,"additionalProperties":false,
    });
    let mut constraints: Vec<_> = pairs
        .into_iter()
        .map(|pair| json!({"not":{"required":pair}}))
        .collect();
    for group in command.get_groups() {
        let mut group = group.clone();
        let members: Vec<_> = group.get_args().map(|id| id.as_str().to_owned()).collect();
        // Nested groups still appear in argumentGroups; only encode constraints whose
        // complete membership is represented in this argument-value schema.
        if members.is_empty()
            || members
                .iter()
                .any(|id| !schema["properties"].as_object().unwrap().contains_key(id))
        {
            continue;
        }
        if group.is_required_set() {
            constraints.push(json!({"anyOf":members.iter().map(|id|json!({"required":[id]})).collect::<Vec<_>>()}));
        }
        if !group.is_multiple() {
            for (index, left) in members.iter().enumerate() {
                for right in members.iter().skip(index + 1) {
                    constraints.push(json!({"not":{"required":[left,right]}}));
                }
            }
        }
    }
    if !constraints.is_empty() {
        schema["allOf"] = json!(constraints);
    }
    schema
}

fn error_contract() -> Value {
    json!({
        "scope":"Shared exit categories; applicability and specific error codes depend on the command.",
        "exitCodes":[
            {"exit":0,"category":"success","meaning":"Completed or accepted with --no-wait"},
            {"exit":1,"category":"internal"}, {"exit":2,"category":"usage"},
            {"exit":3,"category":"auth"}, {"exit":4,"category":"forbidden"},
            {"exit":5,"category":"notFound"}, {"exit":6,"category":"conflict"},
            {"exit":7,"category":"environment"}, {"exit":8,"category":"network"},
            {"exit":9,"category":"remoteFailure"}, {"exit":130,"category":"interrupted"}
        ],
        "recovery":"Use error.recovery.executable and error.recovery.args as an argument vector; recovery is never run automatically.",
    })
}

fn output_schema(command: &str, output: Output) -> Value {
    if output != Output::Envelope {
        return json!({"x-outputProtocol":output,"description":"Native output; no single-response JSON Schema applies."});
    }
    json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object",
        "required":["schemaVersion","command","ok","data","error","meta"],
        "additionalProperties":false,
        "properties":{
            "schemaVersion":{"const":1},"command":{"enum":[command,"miy"]},
            "ok":{"type":"boolean"},"data":{},"meta":{"type":"object"},
            "error":{"oneOf":[{"type":"null"},{
                "type":"object","required":["code","message","retryable","details"],
                "additionalProperties":false,
                "properties":{
                    "code":{"type":"string"},"message":{"type":"string"},
                    "retryable":{"type":"boolean"},"details":{},
                    "recovery":{"oneOf":[{"type":"null"},{
                        "type":"object","required":["executable","args"],"additionalProperties":false,
                        "properties":{"executable":{"type":"string"},"args":{"type":"array","items":{"type":"string"}}}
                    }]}
                }
            }]}
        },
        "oneOf":[
            {"properties":{"ok":{"const":true},"error":{"type":"null"}}},
            {"properties":{"ok":{"const":false},"data":{"type":"null"},"error":{"type":"object"}}}
        ]
    })
}

fn shared_schema<T: schemars::JsonSchema>() -> Value {
    let schema = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<T>();
    serde_json::to_value(schema).expect("JSON Schema serialization is infallible")
}

fn contracts(command: &str) -> Value {
    match command {
        "app accept" => {
            json!({"structuredInput":shared_schema::<crate::acceptance_contract::ReleaseCheck>(),
            "inputSource":"--input @file or --input -; mutually exclusive with note/release/report flags",
            "semanticValidation":{"source":"acceptance_contract::validate","noteMaxBytes":2000,"maxChecks":64,
                "checkScopeMaxBytes":120,"checkActorMaxBytes":120,"checkEvidenceMaxBytes":2000,
                "resultEnum":["passed","not_run"],"nonemptyChecksRequirePassedResult":true}})
        }
        "grant set" | "grant revoke" => {
            json!({"structuredInput":shared_schema::<crate::cli_remote::GrantInput>(),
            "inputSource":"--input @file or --input -; mutually exclusive with user/permission/environment flags; appId also conflicts with --app",
            "semanticValidation":{"source":"cli_remote::execute_grant","appId":"valid platform app identifier","userIdMaxBytes":256}})
        }
        "app deploy" | "app promote" | "app start" | "app stop" | "app restart" | "app remove"
        | "job retry" | "job resume" => json!({
            "preflightInput":shared_schema::<crate::runtime_contract::PreflightInput>(),
            "preflightResult":shared_schema::<crate::runtime_contract::PreflightResult>(),
            "expectedState":shared_schema::<crate::runtime_contract::ExpectedState>(),
            "runtimeFailure":shared_schema::<crate::runtime_contract::RuntimeFailure>(),
            "schemaUse":"Wire contracts for CLI/server exchange, not an --input argument on this command.",
            "semanticValidation":"runtime_contract::validate_input/validate_state, server preflight/admission and worker checks; structs alone do not grant permission or retry eligibility."}),
        _ => json!({}),
    }
}

fn data_schema(command: &str) -> Value {
    let collection = match command {
        "app list" => Some("apps"),
        "user list" => Some("users"),
        "grant list" => Some("grants"),
        "registration list" => Some("registrations"),
        "job list" => Some("jobs"),
        "auth sessions list" => Some("sessions"),
        "component list" => Some("components"),
        _ => None,
    };
    if let Some(collection) = collection {
        return json!({"type":"object","required":[collection,"nextCursor"],"properties":{
            collection:{"type":"array","items":{"type":"object","additionalProperties":true}},"nextCursor":{"type":["string","null"]}},"additionalProperties":true});
    }
    match command {
        "context init" => {
            json!({"oneOf":[
                {"type":"object","required":["status","active","context","origin","setupRequired","serverMismatch"],"properties":{
                    "status":{"const":"initialized"},"active":{"const":"default"},"context":{"const":"default"},"origin":{"type":"string"},"setupRequired":{"const":false},"serverMismatch":{"const":false}},"additionalProperties":false},
                {"type":"object","required":["status","active","activeOrigin","requestedOrigin","setupRequired","serverMismatch","next"],"properties":{
                    "status":{"const":"preserved"},"active":{"type":["string","null"]},"activeOrigin":{"type":["string","null"]},"requestedOrigin":{"type":["string","null"]},"setupRequired":{"type":"boolean"},"serverMismatch":{"type":"boolean"},"next":{"type":"array","items":{"type":"object","required":["program","args"],"properties":{"program":{"const":"miy"},"args":{"type":"array","items":{"type":"string"}}},"additionalProperties":false}}},"additionalProperties":false}
            ]})
        }
        "context list" => {
            json!({"type":"object","required":["active","contexts","nextCursor"],"properties":{
            "active":{"type":["string","null"]},"contexts":{"type":"object","additionalProperties":{"type":"object"}},"nextCursor":{"type":["string","null"]}},"additionalProperties":false})
        }
        "app show" | "app status" => {
            json!({"type":"object","required":["app","installations"],"properties":{
            "app":{"type":"object","additionalProperties":true},"installations":{"type":"array","items":{"type":"object","additionalProperties":true}}},"additionalProperties":true})
        }
        "job get" => job_schema(),
        "job watch" => {
            json!({"oneOf":[job_schema(),{"type":"object","required":["cursor","sequence","createdAt","job"],
            "properties":{"cursor":{"type":"string"},"sequence":{"type":"integer","minimum":1},"createdAt":{"type":"string","format":"date-time"},"job":job_schema()},"additionalProperties":true}],
            "description":"Ordinary output is the terminal job; JSONL contains runtime events, or build log entries followed by the completed build receipt."})
        }
        "app deploy" | "app promote" | "app start" | "app stop" | "app restart" | "app remove"
        | "job retry" => {
            json!({"oneOf":[job_schema(),shared_schema::<crate::runtime_contract::PreflightResult>()],
            "description":"Dry-run returns preflight data; execution returns the accepted or terminal job."})
        }
        "job resume" => job_schema(),
        "job logs" => {
            json!({"type":"object","required":["id","jobId","logs","nextSequence","terminal"],
            "properties":{"id":{"type":"string","format":"uuid"},"jobId":{"type":"string","format":"uuid"},
            "logs":{"type":"array","items":{"type":"object","required":["sequence","text"],"properties":{"sequence":{"type":"integer","minimum":1},"text":{"type":"string"}},"additionalProperties":true}},
            "nextSequence":{"type":"integer","minimum":0},"terminal":{"type":"boolean"}},"additionalProperties":true})
        }
        "doctor" => {
            json!({"type":"object","required":["for","ready","cliVersion","bundleVersion","checks"],"properties":{
            "for":{"enum":["operate","develop"]},"ready":{"type":"boolean"},"cliVersion":{"type":"string"},"bundleVersion":{"type":"string"},"checks":{"type":"array","items":{"type":"object"}}},"additionalProperties":true,
            "description":"A failed required diagnostic uses exit 7 and preserves this report in error.details."})
        }
        "auth login" | "auth wait" => {
            json!({"type":"object","required":["status","context"],"properties":{"status":{"enum":["pending","authenticated"]},"context":{"type":"string"}},"additionalProperties":true})
        }
        "auth status" => {
            json!({"type":"object","required":["authenticated","context","userId","sessionId","expiresAt","refreshExpiresAt"],"properties":{"authenticated":{"const":true},"context":{"type":"string"},"userId":{"type":"string"},"sessionId":{"type":"string","format":"uuid"},"expiresAt":{"type":"integer"},"refreshExpiresAt":{"type":"integer"}},"additionalProperties":true})
        }
        "app logs" => {
            json!({"type":"object","required":["id","status","appId","environment","text","truncated","rotationMayOmit"],"properties":{
            "id":{"type":"string","format":"uuid"},"status":{"const":"ready"},"appId":{"type":"string"},"environment":{"enum":["dev","prod"]},"text":{"type":["string","null"]},"truncated":{"type":"boolean"},"rotationMayOmit":{"const":true}},"additionalProperties":true})
        }
        _ => {
            json!({"description":"Command data is an extensible JSON value; this command has no separately constrained data model in discovery."})
        }
    }
}

fn pagination(command: &str) -> Value {
    let (execution, ordering, scope) = match command {
        "context list" => (
            "local",
            "context name, ascending",
            "Local client and complete context snapshot; context changes require restarting pagination.",
        ),
        "component list" => (
            "client",
            "component name then version, ascending lexical order",
            "Collection, context, server origin and authenticated user; every page authenticates again. The server catalog is fetched in full before the CLI selects the page.",
        ),
        "app list" | "user list" | "grant list" | "registration list" | "job list"
        | "auth sessions list" => (
            "server",
            "server-defined stable key",
            "Collection, authenticated user and applicable filters; every page checks permission.",
        ),
        _ => return Value::Null,
    };
    json!({"limit":{"default":50,"minimum":1,"maximum":200},"cursor":"Opaque --cursor from data.nextCursor; null means the final page. JSON envelope meta.nextCursor is preserved when --fields omits it.","invalidCursorExit":2,"execution":execution,"ordering":ordering,"scope":scope,
        "fieldSelectionExample":match command {"context list"=>Some("contexts,active"),"component list"=>Some("components.name,components.version"),_=>None}})
}

fn job_schema() -> Value {
    json!({"type":"object","required":["id","appId","environment","action","status"],"properties":{
        "id":{"type":"string","format":"uuid"},"appId":{"type":"string"},"environment":{"enum":["dev","prod"]},
        "action":{"anyOf":[shared_schema::<crate::runtime_contract::Operation>(),{"const":"build"}]},
        "status":{"anyOf":[shared_schema::<crate::runtime_contract::JobStatus>(),{"const":"receiving"}]},
        "requestId":{"type":"string"},"retryable":{"type":"boolean"},"errorCode":{"type":["string","null"]}},"additionalProperties":true})
}

fn behavior(command: &str) -> Value {
    match command {
        "context init" => json!({
            "freshConfiguration":"Creates and selects the default context from the explicitly supplied URL; no built-in server address.",
            "missingUrl":"Prompts for an explicit HTTP/HTTPS URL only on a terminal. Noninteractive or --no-input returns server_url_required; blank input cancels with exit 130 without configuration writes.",
            "existingConfiguration":"Validates and preserves an existing configuration byte-for-byte, including empty configurations. Invalid configurations fail without replacement.",
            "differentServer":"Reports activeOrigin, requestedOrigin and serverMismatch plus explicit context add/use argument vectors; never silently changes the selected server.",
            "selection":"Does not use --context, MIY_CONTEXT or project bindings to select a server.",
            "network":"Offline: does not contact the server or log in."}),
        "auth login" => {
            json!({"serverSetup":"Fresh interactive login requests a server URL if none is configured. Noninteractive login requires an explicitly registered context; existing configuration is preserved.","noninteractive":"Returns pending public instructions after context validation; auth wait LOGIN_ID completes approval.","noWait":"Returns pending immediately.","deviceCode":"Suppresses automatic browser opening.","developmentAccount":"Explicit enabled HTTP loopback shortcut auto-approves and waits even without a TTY.","deviceApprovalSeconds":300,"accessTokenMaxSeconds":900,"refreshSessionMaxDays":30})
        }
        "auth wait" => {
            json!({"interruption":"Exit 130 preserves the flow until expiry; no token or private device code is printed."})
        }
        "auth logout" => {
            json!({"localOnly":"Skips remote revocation explicitly; local deletion failures and uncertain remote revocation are reported separately."})
        }
        "doctor" => {
            json!({"operate":"Default: context, identity, connection and server features; no local source or development tools required.","develop":"Adds project integrity, exact Codex/Claude MCP configuration, bounded local MCP initialize/tools/list, Rust linker/rustfmt, versions and server architecture checks; Docker engine checks require --runtime docker. Requires development.sessions.v1.","mcpLimit":"The local stdio probe does not verify a real agent session or live API tool calls."})
        }
        "app init" => {
            json!({"serverAddress":"Uses an explicitly selected context or --url; no built-in IP/domain or localhost fallback. Without either, scaffolds an offline unbound project requiring server registration before remote work."})
        }
        "app dev" => {
            json!({"runtimeDefault":"native","native":"Compiles the native Rust backend, serves the built React preview on loopback and runs the authenticated DB tunnel in-process; no Docker, WSL or local PostgreSQL required.","databaseDefault":"server","localDatabase":"Only --runtime docker --database local uses a separate local DB; connection failure never selects it automatically.","serverDatabase":"Requires registration approval and development.sessions.v1. The server prepares/applies SQL and the local app connects through an authenticated WS/WSS tunnel.","concurrency":"One local session per app; stop it with app dev-stop before dev deployment.","migration":"Schema changes stop the deployed dev app; applied SQL cannot be edited or deleted. Dev deployment reuses the same database and rows; prod uses a separate database."})
        }
        "app deploy" | "app start" | "app stop" | "app restart" | "app remove" | "app promote"
        | "job retry" => json!({
            "runtimeEnvironmentDefault":if command=="app promote" {Value::Null}else{json!("dev (job retry inherits its original target)")},
            "explicitTarget":"Prod operations and every app removal require --context, --app and --environment, including dry-run and retry.",
            "confirmation":"Dry-run skips confirmation. Execution requires interactive confirmation or --yes; permission checks remain mandatory.",
            "dryRun":"No image upload or runtime job; returns expected state, intended changes and worker checks that remain deferred.",
            "recovery":"Persisted request keys and payloads are recovered with job resume before creating another operation; image upload recovery is whole-archive, not byte-range continuation."}),
        "job resume" => {
            json!({"knownJob":"Observes an accepted runtime or build job. A completed build restores image metadata for its original project when its committed revision still matches.","unacknowledgedRequest":"Look up the original request before explicit resubmission with its original key and input. Build recovery uses the saved source archive and never local Docker; runtime prod/remove requires explicit context/app/environment and fresh confirmation."})
        }
        "job logs" => {
            json!({"scope":"Server builds only; use app logs for application runtime logs.","cursor":"Pass the returned nonnegative nextSequence to --after.","bounded":true})
        }
        "job watch" => {
            json!({"jsonl":true,"cursor":"Use --after or --cursor, or the saved journal cursor. Server builds use numeric log sequences; runtime jobs use opaque event cursors.","interruption":"Exit 130 stops observation, never cancels the remote job.","timeout":"Exit 8 preserves job ID and cursor.","terminalFailureExit":9})
        }
        "app logs" => {
            json!({"bounded":true,"retentionService":false,"rotationMayOmit":true,"resume":"Use --request ID to observe the same accepted log request; tail/since cannot be combined with --request."})
        }
        "grant set" | "grant revoke" => {
            json!({"flagDefaults":{"environment":"dev","permission":"use"},"structuredInput":"Complete typed document; defaults above apply only to the flag form."})
        }
        "app build" => {
            json!({"default":"Local Docker build with local checks; --remote uploads a filtered, clean committed source snapshot for server-side build.",
                "remoteRequirements":"Named context, login, approved app develop permission, Git and app.builds.v1; remote mode never invokes local Docker, Rust or pnpm.",
                "platform":"Remote mode uses the selected server runtime platform; an explicit --platform must match it. Local engine/server CPU differences require explicit platform and a working builder.",
                "recovery":"Before upload, records actor/origin/context, source hash and original request ID. Use job resume ID after interruption; accepted work continues. Re-running app build after terminal failure starts a new attempt.",
                "result":"Successful remote builds write server-bound dist/build.json and dist/image.json; app deploy verifies the receipt and uses the existing server image directly.",
                "observation":"--no-wait returns the accepted build; --timeout defaults to 1800 seconds. job get/watch/resume and job logs accept build IDs."})
        }
        _ => json!({}),
    }
}

fn examples(command: &str, path: &[String]) -> Value {
    let mut help = path.to_vec();
    help.push("--help".into());
    let mut examples = vec![
        json!({"executable":"miy","args":help,"purpose":"Read help without executing the operation."}),
    ];
    let template: &[&str] = match command {
        "context init" => &["context", "init", "--url", "https://platform.example"],
        "context add" => &[
            "context",
            "add",
            "work",
            "--url",
            "https://platform.example",
        ],
        "auth login" => &["auth", "login", "--device-code", "--no-wait"],
        "auth wait" => &["auth", "wait", "<LOGIN_UUID>"],
        "app init" => &[
            "--context",
            "work",
            "app",
            "init",
            "./my-app",
            "--id",
            "my-app",
            "--name",
            "My app",
        ],
        "app build" => &["app", "build", "--remote", "--no-wait"],
        "job logs" => &["job", "logs", "<BUILD_UUID>", "--after", "0"],
        "app deploy" => &["app", "deploy", "--environment", "dev", "--dry-run"],
        "app promote" => &[
            "--context",
            "work",
            "--app",
            "my-app",
            "app",
            "promote",
            "--environment",
            "prod",
            "--release",
            "<RELEASE_UUID>",
            "--dry-run",
        ],
        "app remove" => &[
            "--context",
            "work",
            "--app",
            "my-app",
            "app",
            "remove",
            "--environment",
            "dev",
            "--dry-run",
        ],
        "job watch" => &[
            "--output",
            "jsonl",
            "job",
            "watch",
            "<JOB_UUID>",
            "--after",
            "<EVENT_CURSOR>",
        ],
        "job retry" => &[
            "--context",
            "work",
            "--app",
            "my-app",
            "job",
            "retry",
            "<JOB_UUID>",
            "--environment",
            "prod",
            "--dry-run",
        ],
        "job resume" => &[
            "--context",
            "work",
            "--app",
            "my-app",
            "job",
            "resume",
            "<REQUEST_OR_JOB_UUID>",
            "--environment",
            "dev",
        ],
        "app logs" => &[
            "--app",
            "my-app",
            "app",
            "logs",
            "--environment",
            "dev",
            "--request",
            "<LOG_REQUEST_UUID>",
        ],
        "app accept" => &["app", "accept", "--input", "@acceptance.json"],
        "grant set" => &["--app", "my-app", "grant", "set", "--input", "@grant.json"],
        "app list" => &[
            "--output",
            "json",
            "--fields",
            "apps.id,apps.name,nextCursor",
            "app",
            "list",
            "--limit",
            "20",
        ],
        _ => &[],
    };
    if !template.is_empty() {
        examples.push(json!({"executable":"miy","args":template,"template":true,"purpose":"Replace example targets, paths and placeholders with reviewed values; inspect permissions and preconditions before executing."}));
    }
    json!(examples)
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Output {
    Envelope,
    JsonRpc,
    ShellScript,
    ExternalProcess,
    ServerProcess,
    Group,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Policy {
    coverage: &'static str,
    permissions: &'static str,
    local_effects: &'static [&'static str],
    remote_effects: &'static [&'static str],
    source: &'static str,
    output: Output,
}

fn policy(command: &str, group: bool) -> Policy {
    let mut policy = Policy {
        coverage: "reviewed",
        permissions: "No platform authentication required.",
        local_effects: &[],
        remote_effects: &[],
        source: "crates/miy/src/app_cli.rs",
        output: Output::Envelope,
    };
    if group {
        return Policy {
            coverage: "group",
            permissions: "Select a subcommand for its permissions and effects.",
            output: Output::Group,
            ..policy
        };
    }
    match command {
        "describe" | "app describe" => policy.source = "crates/miy/src/cli_discovery.rs",
        "completion" => {
            policy.source = "crates/miy/src/cli_discovery.rs";
            policy.output = Output::ShellScript;
        }
        "doctor" | "app doctor" => {
            policy.source = "crates/miy/src/cli_doctor.rs";
            policy.permissions = "A selected server context and valid user session are required for readiness; develop also requires a local project and tools.";
            policy.local_effects = &[
                "Operate reads configuration; develop also verifies project/MCP files, starts a bounded local MCP probe, links a temporary Rust executable and invokes tool/Docker probes.",
            ];
            policy.remote_effects =
                &["Reads server capabilities and verifies the authenticated identity."];
        }
        "context init" => {
            policy.source = "crates/miy/src/cli_config.rs";
            policy.local_effects = &[
                "Validates an explicitly supplied or terminal-entered URL and optional CA; creates a context only when config.json is absent. Existing configuration and credentials are preserved; no address is assumed.",
            ];
        }
        "context add" | "context use" => {
            policy.source = "crates/miy/src/cli_config.rs";
            policy.local_effects = &[
                "Updates named server configuration or active-context selection; add validates the origin and optional CA.",
            ];
        }
        "context list" | "context show" => {
            policy.source = "crates/miy/src/cli_config.rs";
            policy.local_effects = &[
                "Reads public context configuration and user bindings; never returns credentials.",
            ];
        }
        "context remove" => {
            policy.source = "crates/miy/src/cli_config.rs";
            policy.permissions = "Local OS user; explicit confirmation or --yes.";
            policy.local_effects = &[
                "Removes the named local context and clears it if active; project data is preserved.",
            ];
            policy.remote_effects = &[
                "None: removing a context does not revoke server sessions or remove an application.",
            ];
        }
        "app init" => {
            policy.local_effects = &[
                "Creates a project, bundled CLI, SDKs, instructions and MCP configuration at the requested path. No server URL is invented when scaffolding an unbound offline project.",
            ]
        }
        "app setup" | "app setup-codex" => {
            policy.local_effects = &[
                "Writes project-local CLI, agent instructions, MCP configuration and trusted CA files as applicable.",
            ]
        }
        "app upgrade" => {
            policy.local_effects = &[
                "Updates supported project SDK and configuration files while preserving business code and SQL.",
            ];
            policy.remote_effects = &["May read registry/dependency metadata."];
        }
        "app check" => {
            policy.local_effects = &[
                "Runs project Rust tests and frontend toolchains; installs dependencies and writes build artifacts. Project code executes with the current OS user's authority.",
            ]
        }
        "app fmt" => {
            policy.local_effects = &["Formats and rewrites the project's Rust source files."]
        }
        "app build" => {
            policy.source = "crates/miy/src/app_cli.rs; crates/miy/src/cli_build.rs; crates/miy/src/source_archive.rs";
            policy.permissions = "Remote mode requires a named context, login and develop permission for an approved app; local mode can build offline.";
            policy.local_effects = &[
                "Local mode runs project checks and Docker. Remote mode reads committed Git blobs, excludes secrets and local state, saves a private source archive/recovery journal, and writes completed build metadata without invoking local Docker or language tools.",
            ];
            policy.remote_effects = &[
                "Remote mode uploads source and starts a durable server Docker build; local mode only reads the selected server platform. Timeout or interruption does not cancel accepted builds.",
            ];
        }
        "app dev" => {
            policy.permissions = "Authenticated developer for an administrator-approved app; platform/connected APIs enforce their permissions.";
            policy.local_effects = &[
                "Builds and starts the local app, authenticated DB tunnel and loopback gateway; writes preview state and may open a browser. Native is the default; --runtime docker --database local explicitly starts a separate local DB.",
            ];
            policy.remote_effects = &[
                "By default creates a single-app development session, prepares the server dev DB and applies migrations; schema changes stop a deployed dev app. Application code uses that dev DB and authorized connected APIs.",
            ];
        }
        "app dev-stop" => {
            policy.local_effects = &[
                "Stops the project's owned gateway, native backend and database tunnel (or explicit Docker containers); preserves database data.",
            ];
            policy.remote_effects =
                &["Releases the owned server development session and its temporary DB access."];
        }
        "app login" | "auth login" | "auth wait" => {
            policy.source = "crates/miy/src/cli_login.rs";
            policy.local_effects = &[
                "Stores private flow/session credentials and public recovery metadata; updates the context's user binding. Only interactive auth login may open a browser.",
            ];
            policy.remote_effects = &[
                "Creates or completes device approval and issues a user session. Explicit --account uses an enabled HTTP loopback development session to approve the same flow.",
            ];
        }
        "app logout" | "auth logout" => {
            policy.source = "crates/miy/src/cli_login.rs";
            policy.permissions = "The saved credential identifies the session to revoke.";
            policy.local_effects = &["Deletes the selected local credentials."];
            policy.remote_effects = &[
                "Revokes the saved remote CLI session before local removal, unless --local-only is explicitly requested.",
            ];
        }
        "auth status" | "auth sessions list" => {
            policy.source = "crates/miy/src/cli_login.rs";
            policy.permissions = "Authenticated user of the selected context; sessions list returns that user's sessions.";
            policy.local_effects =
                &["Reads protected credentials and may save a serialized session renewal."];
            policy.remote_effects = &[
                "Verifies identity or reads this user's sessions; expiring credentials may rotate through the refresh endpoint.",
            ];
        }
        "auth sessions revoke" => {
            policy.source = "crates/miy/src/cli_login.rs";
            policy.permissions =
                "The session's authenticated owner; explicit confirmation or --yes.";
            policy.remote_effects = &["Revokes the selected owned CLI session on the server."];
        }
        "app register" => {
            policy.permissions = "Administrator or developer account with miy.dev scope.";
            policy.remote_effects = &[
                "Creates an app registration request; approval is required before access is granted.",
            ];
        }
        "app registrations" | "registration list" => {
            policy.permissions =
                "Authenticated user; users see their own requests, administrators may see all.";
            policy.remote_effects = &["Reads registration requests."];
        }
        "app approve" | "registration approve" => {
            policy.permissions = "Administrator.";
            policy.remote_effects = &[
                "Approves a pending registration, creates the app and grants the requester develop access to dev.",
            ];
        }
        "app reject" | "registration reject" => {
            policy.permissions = "Administrator.";
            policy.remote_effects = &["Rejects a pending registration request."];
        }
        "app users" | "user list" => {
            policy.permissions = "Administrator.";
            policy.remote_effects = &["Reads platform user IDs."];
        }
        "app grant" | "grant set" | "grant revoke" => {
            policy.permissions = "Administrator.";
            policy.remote_effects =
                &["Grants or revokes the specified app/environment permission."];
        }
        "grant list" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/read_api.rs";
            policy.permissions = "Authenticated user; non-administrators see their own grants, administrators may see all.";
            policy.remote_effects =
                &["Reads grants with server-side app filtering and actor/filter-bound pagination."];
        }
        "app rename" => {
            policy.permissions = "Administrator.";
            policy.remote_effects = &[
                "Changes the platform display name; app identity and source manifest are unchanged.",
            ];
        }
        "app accept" => {
            policy.permissions = "miy.dev scope plus administrator or develop permission for this app's dev environment.";
            policy.remote_effects =
                &["Records acceptance evidence for the current healthy dev release."];
        }
        "app deploy" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/cli_upload.rs";
            policy.permissions = "miy.dev scope plus administrator or develop permission for this app's dev environment.";
            policy.local_effects = &[
                "Saves deployment recovery state. Exports a local image only for local builds; server build receipts require no Docker export.",
            ];
            policy.remote_effects = &[
                "Verifies server build receipts directly, or uploads local image data, then submits a checked dev deployment. Dry-run submits neither upload nor runtime work. The worker may migrate the app database and replace containers.",
            ];
        }
        "app promote" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/runtime_contract.rs";
            policy.permissions = "miy.dev scope plus administrator or deploy permission for this app's prod environment; requires acceptance of the exact release.";
            policy.local_effects = &["Saves job recovery state."];
            policy.remote_effects = &[
                "Except in dry-run, promotes the accepted release to prod; runs migrations and replaces containers without copying dev data.",
            ];
        }
        "app control" | "app start" | "app stop" | "app restart" | "app remove" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/runtime_contract.rs";
            policy.permissions = "miy.dev scope plus administrator or develop permission for dev / deploy permission for prod.";
            policy.local_effects = &["Saves job recovery state."];
            policy.remote_effects = &[
                "Dry-run only reads preflight. Execution submits the selected runtime operation. App removal deletes runtime containers and preserves the app database.",
            ];
        }
        "app resume" | "job resume" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/cli_journal.rs; crates/miy/src/cli_upload.rs";
            policy.permissions = "Saved requests require their original context and actor; known jobs require app/environment visibility. Resubmission rechecks management permission; prod/remove requires explicit target and confirmation.";
            policy.local_effects = &[
                "Reads and updates recovery state. Build recovery reuses the saved committed source archive; server-image deployment recovery never exports Docker images. Local-image recovery may export the original image.",
            ];
            policy.remote_effects = &[
                "Observes an existing job, or looks up the saved upload/runtime request and explicitly resubmits unacknowledged input with its original idempotency key and expected state.",
            ];
        }
        "app status" | "app show" | "app list" => {
            policy.permissions =
                "Authenticated user with access to the app's returned installations.";
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/read_api.rs";
            policy.remote_effects = &[
                "Reads authorized app/installation metadata; manager-only acceptance details remain subject to server authorization.",
            ];
        }
        "job list" | "job get" | "job watch" | "job logs" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/job_api.rs";
            policy.permissions =
                "Authenticated user authorized for the returned job's app/environment.";
            policy.local_effects = &[
                "Watch may persist an event/log cursor; a completed build restores its original project build receipt when the committed revision still matches.",
            ];
            policy.remote_effects = &[
                "List returns runtime jobs; get/watch also accept server builds. Logs reads bounded build log entries. Timeout/Ctrl-C stops local observation without cancelling accepted work.",
            ];
        }
        "job retry" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/runtime_contract.rs";
            policy.permissions = "Current management permission for the original app/environment, a typed retryable terminal failure, and fresh preflight. Prod/remove requires explicit context/app/environment and confirmation.";
            policy.local_effects = &[
                "Persists a separate retry request linked to the failed job; repeated unknown submission is recovered with its original key.",
            ];
            policy.remote_effects = &[
                "Dry-run reads preflight only; execution submits a new runtime job linked to the original, with a fresh expected state.",
            ];
        }
        "app logs" => {
            policy.source = "crates/miy/src/cli_remote.rs; crates/miy/src/runtime_logs.rs";
            policy.permissions = "miy.dev scope and administrator or develop/dev or deploy/prod management permission; only the requesting actor may read its log request.";
            policy.remote_effects = &[
                "Queues a bounded, expiring container-log snapshot, or observes an existing --request ID. Container generation/ownership is checked; rotation/truncation may omit data.",
            ];
        }
        "app components list" | "app components show" | "component list" | "component show" => {
            policy.permissions = "Authenticated platform user.";
            policy.remote_effects = &["Reads the component registry."];
            policy.source = "crates/miy/src/component_cli.rs";
        }
        "app components add"
        | "app components update"
        | "app components sync"
        | "component add"
        | "component update"
        | "component sync" => {
            policy.permissions = "Authenticated platform user.";
            policy.local_effects = &["Writes local component files and lock state."];
            policy.remote_effects =
                &["Reads component releases; does not publish registry changes."];
            policy.source = "crates/miy/src/component_cli.rs";
        }
        "app components check" | "component check" => {
            policy.source = "crates/miy/src/component_cli.rs"
        }
        "app components publish" | "component publish" => {
            policy.permissions = "Administrator.";
            policy.remote_effects = &["Publishes an immutable component release to the registry."];
            policy.source = "crates/miy/src/component_cli.rs";
        }
        "app mcp" | "mcp" => {
            policy.permissions =
                "Guidance is local; connected read tools use the saved user's permissions.";
            policy.remote_effects =
                &["Reads authorized platform development guidance and catalogs."];
            policy.output = Output::JsonRpc;
            policy.source = "crates/miy/src/agent_tools.rs";
        }
        "app codex" => {
            policy.local_effects = &[
                "Launches an external agent; its tool actions can modify files or invoke services according to its own permissions.",
            ];
            policy.output = Output::ExternalProcess;
        }
        "serve" | "worker" | "migrate" | "seed" => {
            policy.permissions =
                "Server operator with configured database and service credentials.";
            policy.local_effects = &[
                "Runs server processes, opens listeners or executes configured infrastructure operations as applicable.",
            ];
            policy.remote_effects = &[
                "May modify platform/application databases, sessions, jobs and runtime resources.",
            ];
            policy.output = Output::ServerProcess;
            policy.source = "crates/miy/src/main.rs";
        }
        _ => {
            policy.coverage = "unspecified";
            policy.permissions = "Permissions and effects have not been declared in discovery metadata; consult the command help and implementation.";
            policy.source = "";
        }
    }
    policy
}

/// Completion is a native shell script and intentionally bypasses the JSON renderer.
pub fn completion(shell: clap_complete::Shell) -> Result<()> {
    let mut command = crate::Args::command();
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut command, "miy", &mut script);
    std::io::stdout().lock().write_all(&script)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_list_pagination_declares_actual_shapes_and_limits() {
        for (name, collection, kind, execution) in [
            ("context list", "contexts", "object", "local"),
            ("component list", "components", "array", "client"),
        ] {
            let value = inspect(DescribeArgs {
                command: Some(name.into()),
                ..Default::default()
            })
            .unwrap();
            let command = &value["command"];
            assert_eq!(command["paginationSupported"], true);
            assert_eq!(command["pagination"]["execution"], execution);
            assert_eq!(command["pagination"]["limit"]["default"], 50);
            assert_eq!(
                command["dataSchema"]["properties"][collection]["type"],
                kind
            );
            assert!(
                command["dataSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("nextCursor"))
            );
            assert!(command["help"].as_str().unwrap().contains("--cursor"));
            let mut argv = vec!["miy"];
            argv.extend(name.split_whitespace());
            argv.extend(["--limit", "201"]);
            assert!(crate::Args::command().try_get_matches_from(argv).is_err());
        }
    }

    #[test]
    fn scoped_arguments_match_clap_and_help() {
        let value = inspect(DescribeArgs {
            command: Some("app deploy".into()),
            ..Default::default()
        })
        .unwrap();
        let command = &value["command"];
        let environment = command["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|argument| argument["id"] == "environment")
            .unwrap();
        let mut root = crate::Args::command();
        root.build();
        let deploy = root
            .find_subcommand("app")
            .unwrap()
            .find_subcommand("deploy")
            .unwrap();
        let arg = deploy
            .get_arguments()
            .find(|argument| argument.get_id() == "environment")
            .unwrap();
        assert_eq!(
            environment["schema"]["enum"],
            json!(
                arg.get_possible_values()
                    .iter()
                    .map(|value| value.get_name())
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(environment["schema"], argument_schema(arg));
        assert_eq!(
            command["inputSchema"]["properties"]["environment"],
            environment["schema"]
        );
        assert!(command["help"].as_str().unwrap().contains("--environment"));
        assert_eq!(
            command["outputSchema"]["properties"]["command"]["enum"],
            json!(["app deploy", "miy"])
        );
        assert!(
            root.try_get_matches_from(["miy", "app", "deploy", "--environment", "prod"])
                .is_err()
        );
    }

    #[test]
    fn default_is_compact_and_only_exposes_this_builds_commands() {
        let value = inspect(DescribeArgs::default()).unwrap();
        let mut root = crate::Args::command();
        root.build();
        let expected: Vec<_> = root
            .get_subcommands()
            .filter(|command| visible(command))
            .map(|command| command.get_name())
            .collect();
        let summaries = value["commands"].as_array().unwrap();
        assert_eq!(
            summaries
                .iter()
                .map(|command| command["command"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            summaries
                .iter()
                .all(|command| command.get("help").is_none() && command.get("arguments").is_none())
        );
    }

    #[test]
    fn unknown_command_is_a_safe_usage_error() {
        let error = inspect(DescribeArgs {
            command: Some("never-a-command SECRET-SENTINEL".into()),
            ..Default::default()
        })
        .unwrap_err();
        let failure = error.downcast_ref::<CliError>().unwrap();
        assert_eq!(failure.category, Category::Usage);
        assert_eq!(failure.code, "unknown_command");
        assert!(!error.to_string().contains("SECRET-SENTINEL"));
    }

    #[test]
    fn visible_leaf_policies_are_reviewed_and_removed_commands_are_not_advertised() {
        let value = inspect(DescribeArgs {
            all: true,
            ..Default::default()
        })
        .unwrap();
        for command in value["commands"].as_array().unwrap() {
            assert_ne!(
                command["policy"]["coverage"], "unspecified",
                "{}",
                command["command"]
            );
            assert!(!matches!(
                command["command"].as_str(),
                Some("app login" | "app resume" | "app control" | "app components")
            ));
        }
    }

    #[test]
    fn structured_schemas_share_runtime_types_without_dangling_references() {
        let grant = contracts("grant set");
        assert_eq!(
            grant["structuredInput"]["properties"]["permission"]["enum"],
            json!(["use", "develop", "deploy"])
        );
        assert_eq!(
            grant["structuredInput"]["properties"]["environment"]["enum"],
            json!(["dev", "prod"])
        );
        let acceptance = contracts("app accept");
        assert_eq!(
            acceptance["structuredInput"]["properties"]["releaseId"]["format"],
            "uuid"
        );
        let runtime = contracts("app deploy");
        assert!(runtime["expectedState"]["properties"]["target"].is_object());
        assert!(
            !serde_json::to_string(&runtime)
                .unwrap()
                .contains("\"$ref\"")
        );
        assert!(contracts("app list").as_object().unwrap().is_empty());
    }

    #[test]
    fn types_required_defaults_and_symmetric_conflicts_are_derived() {
        let root = Command::new("miy").subcommand(
            Command::new("sample")
                .arg(
                    Arg::new("id")
                        .required(true)
                        .value_parser(clap::value_parser!(uuid::Uuid)),
                )
                .arg(
                    Arg::new("port")
                        .long("port")
                        .default_value("18100")
                        .value_parser(clap::value_parser!(u16)),
                )
                .arg(
                    Arg::new("yes")
                        .long("yes")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("no"),
                )
                .arg(Arg::new("no").long("no").action(ArgAction::SetTrue)),
        );
        let value = inspect_tree(
            root,
            DescribeArgs {
                command: Some("sample".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let schema = &value["command"]["inputSchema"];
        assert_eq!(schema["required"], json!(["id"]));
        assert_eq!(schema["properties"]["id"]["format"], "uuid");
        assert_eq!(schema["properties"]["port"]["default"], 18100);
        assert_eq!(schema["properties"]["port"]["maximum"], 65535);
        assert_eq!(schema["properties"]["yes"]["type"], "boolean");
        assert_eq!(schema["allOf"], json!([{"not":{"required":["no","yes"]}}]));
        let no = value["command"]["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|arg| arg["id"] == "no")
            .unwrap();
        assert_eq!(no["conflicts"], json!(["yes"]));
    }
}
