use crate::{State, auth::Principal, contract, domain, http::ApiError};
use axum::{
    Router,
    extract::{Request, State as S},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone)]
struct Mcp {
    state: State,
    tool_router: ToolRouter<Self>,
}
fn principal(ctx: &RequestContext<RoleServer>) -> Result<Principal, McpError> {
    ctx.extensions
        .get::<axum::http::request::Parts>()
        .and_then(|p| p.extensions.get::<Principal>())
        .cloned()
        .ok_or_else(|| McpError::internal_error("missing_authenticated_context", None))
}
fn result(v: Result<Value, ApiError>) -> Result<CallToolResult, McpError> {
    Ok(match v {
        Ok(v) => CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(
            json!({"error":{"code":e.code,"requestId":Uuid::new_v4()}}).to_string(),
        )]),
    })
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Search {
    query: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ApiId {
    id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct JobId {
    id: Uuid,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ComponentId {
    name: String,
    version: String,
}
#[tool_router]
impl Mcp {
    #[tool(
        description = "Search live component releases and compatibility; this catalog is independent of CLI releases.",
        annotations(read_only_hint = true)
    )]
    async fn list_components(
        &self,
        Parameters(v): Parameters<Search>,
    ) -> Result<CallToolResult, McpError> {
        result(
            async {
                let entries = crate::component_registry::catalog_data(&self.state.pool).await?;
                Ok(json!(
                    entries
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| e
                            .to_string()
                            .to_lowercase()
                            .contains(&v.query.to_lowercase()))
                        .collect::<Vec<_>>()
                ))
            }
            .await,
        )
    }
    #[tool(
        description = "Read an exact component release's usage and install command without installing code.",
        annotations(read_only_hint = true)
    )]
    async fn get_component(
        &self,
        Parameters(v): Parameters<ComponentId>,
    ) -> Result<CallToolResult, McpError> {
        result(
            crate::component_registry::get_data(&self.state.pool, &v.name, &v.version)
                .await
                .map(crate::component_registry::guidance),
        )
    }
    fn new(state: State) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
    #[tool(
        description = "Get miy app contract, manifest schema, UI and SDK guidance. Read this before development.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_contract(&self) -> Result<CallToolResult, McpError> {
        result(Ok(domain::get_contract()))
    }
    #[tool(
        description = "Search the supported V0-V1 public APIs by ID or description",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn search_api(
        &self,
        Parameters(v): Parameters<Search>,
    ) -> Result<CallToolResult, McpError> {
        let entries = domain::catalog();
        let q = v.query.to_lowercase();
        result(Ok(json!(
            entries
                .as_array()
                .unwrap()
                .iter()
                .filter(|x| x.to_string().to_lowercase().contains(&q))
                .collect::<Vec<_>>()
        )))
    }
    #[tool(
        description = "Get a public API's input schema, permissions and usage",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_api(&self, Parameters(v): Parameters<ApiId>) -> Result<CallToolResult, McpError> {
        result(
            domain::catalog()
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == v.id)
                .cloned()
                .ok_or_else(ApiError::not_found),
        )
    }
    #[tool(
        description = "Validate a manifest. Declared permissions do not grant actual access.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn validate_manifest(
        &self,
        Parameters(v): Parameters<contract::Manifest>,
    ) -> Result<CallToolResult, McpError> {
        result(
            contract::validate(&v)
                .map(|_| json!({"valid":true}))
                .map_err(ApiError::bad_owned),
        )
    }
    #[tool(
        description = "Prepare a registered installation's DB with a registered release. Requires develop permission on that app and environment. No arbitrary SQL or paths accepted."
    )]
    async fn prepare_database(
        &self,
        Parameters(v): Parameters<domain::Prepare>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        result(domain::prepare(&self.state, &principal(&ctx)?, v).await)
    }
    #[tool(
        description = "Read a persistent DB preparation job within your authorized installation",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_job(
        &self,
        Parameters(v): Parameters<JobId>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        result(domain::get_job(&self.state, &principal(&ctx)?, v.id).await)
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        info.with_instructions("Use get_contract first, then search_api/get_api for supported public APIs. Work only within your assigned app and environment. Permissions in a manifest are requests, not grants. Never connect to core or another app's DB. Use registered releases and inspect job results; do not claim a DB job is an app deployment.")
    }
    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![Resource::new(
                "miy://contracts/v1alpha1",
                "miy application development contract",
            )],
            ..Default::default()
        })
    }
    async fn read_resource(
        &self,
        v: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        if v.uri != "miy://contracts/v1alpha1" {
            return Err(McpError::resource_not_found("unknown_resource", None));
        }
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            domain::get_contract().to_string(),
            v.uri,
        )])
        .into())
    }
}
pub fn router(state: State) -> Router<State> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    let cloned = state.clone();
    let origin = url::Url::parse(&state.auth.origin).expect("validated platform origin");
    let authority = origin[url::Position::BeforeHost..url::Position::AfterPort].to_string();
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts([
            authority,
            "localhost:18080".into(),
            "127.0.0.1:18080".into(),
        ])
        .with_allowed_origins([state.auth.origin.clone()]);
    let service = StreamableHttpService::new(
        move || Ok(Mcp::new(cloned.clone())),
        LocalSessionManager::default().into(),
        config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .route_layer(middleware::from_fn_with_state(state, guard))
}
async fn guard(S(state): S<State>, mut req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let auth = match token {
        Some(t) => state.auth.bearer(t).await,
        None => Err(ApiError::unauthorized()),
    };
    match auth {
        Ok(p) => {
            if let Err(e) = domain::me(&state, &p).await {
                return e.into_response();
            }
            req.extensions_mut().insert(p);
            next.run(req).await
        }
        Err(e) => {
            let mut r = e.into_response();
            r.headers_mut().insert("www-authenticate",format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"miy.read\"",state.auth.origin).parse().unwrap());
            r
        }
    }
}
