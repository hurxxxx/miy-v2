//! Fixed-listener routing for the two browser trust domains.
//! Non-browser CLI credentials keep the published portal API address.
use crate::auth::BrowserPurpose;
use axum::http::Method;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Public,
    Shared,
    Portal,
    Console,
    NonBrowser,
    // Let MCP issue its published 401 + resource_metadata challenge before the
    // client has a bearer. The MCP guard never authenticates browser cookies.
    BearerChallenge,
}

/// Matched route templates, not user-supplied app IDs or forwarded headers.
/// Unknown/new routes are denied until deliberately classified.
pub fn access(route: &str, method: &Method) -> Option<Access> {
    let read = matches!(*method, Method::GET | Method::HEAD);
    let route = route.strip_prefix("/api/v1").unwrap_or(route);
    Some(match route {
        "/api/health"
        | "/capabilities"
        | "/cli/install"
        | "/auth/login"
        | "/auth/options"
        | "/auth/development"
        | "/auth/callback"
        | "/auth/logout"
        | "/auth/cli/device"
        | "/auth/cli/token"
        | "/auth/cli/revoke"
        | "/auth/cli/start"
        | "/auth/cli/exchange"
        | "/.well-known/oauth-protected-resource"
        | "/.well-known/oauth-protected-resource/mcp"
        | "/cli/authorize/{id}"
        | "/cli/device/{id}" => Access::Public,
        "/me" => Access::Shared,
        "/installations" if read => Access::Portal,
        "/artifacts/{hash}/ui/{*path}"
        | "/installations/{id}/api/{*path}"
        | "/installations/{id}/platform/directory"
        | "/installations/{id}/connections/{target}/api/{*path}"
        | "/preview/apps/{app}/platform/directory"
        | "/preview/apps/{app}/connections/{target}/api/{*path}" => Access::Portal,
        "/dev-sessions/{id}/database" => Access::NonBrowser,
        "/apps/{app}/dev-sessions"
        | "/dev-sessions/{id}"
        | "/dev-sessions/{id}/heartbeat"
        | "/apps/{app}/dev-sessions/by-request/{request}"
        | "/components"
        | "/components/{name}/{version}"
        | "/delegations"
        | "/delegations/{id}"
        | "/installations/{id}/permissions"
        | "/cli-authorizations/{id}/approve"
        | "/cli/logout"
        | "/cli/devices/{id}"
        | "/cli/devices/{id}/decision"
        | "/cli/sessions"
        | "/cli/sessions/{id}"
        | "/cli/legacy-sessions"
        | "/cli/legacy-sessions/{id}"
        | "/apps/{app}/test-acceptance"
        | "/apps/{app}/images"
        | "/apps/{app}/builds"
        | "/builds/{id}"
        | "/builds/{id}/logs"
        | "/apps/{app}/build-requests/{request}"
        | "/image-uploads/{id}"
        | "/apps/{app}/image-uploads/by-request/{request}"
        | "/apps"
        | "/apps/{app}"
        | "/apps/{app}/name"
        | "/app-registrations"
        | "/app-registrations/{id}/decision"
        | "/users"
        | "/users/{id}/state"
        | "/apps/{app}/releases"
        | "/installations"
        | "/grants"
        | "/audit"
        | "/groups"
        | "/groups/{id}"
        | "/groups/{id}/parent"
        | "/groups/{id}/members"
        | "/users/{id}/primary-group"
        | "/group-grants"
        | "/apps/{app}/access"
        | "/apps/{app}/admission"
        | "/apps/{app}/control"
        | "/contracts"
        | "/contracts/validate"
        | "/catalog"
        | "/openapi.json"
        | "/database/prepare"
        | "/jobs/{id}"
        | "/apps/{app}/runtime"
        | "/apps/{app}/runtime/preflight"
        | "/apps/{app}/logs"
        | "/runtime/logs/{id}"
        | "/runtime/jobs"
        | "/runtime/installations"
        | "/runtime/jobs/{id}"
        | "/runtime/jobs/{id}/events"
        | "/runtime/jobs/{id}/retry" => Access::Console,
        value if value == "/mcp" || value.starts_with("/mcp/") => Access::BearerChallenge,
        value
            if value == "/cli/downloads"
                || value.starts_with("/cli/downloads/")
                || value == "/cli/installers"
                || value.starts_with("/cli/installers/") =>
        {
            Access::Public
        }
        _ => return None,
    })
}

pub fn allowed(
    access: Access,
    purpose: BrowserPurpose,
    bearer: bool,
    origin_present: bool,
) -> bool {
    match access {
        Access::Public | Access::Shared => true,
        // The console never serves application-controlled active responses,
        // even when a client sends a bearer instead of browser credentials.
        Access::Portal => purpose == BrowserPurpose::Portal,
        Access::Console => purpose == BrowserPurpose::Console || (bearer && !origin_present),
        Access::NonBrowser => bearer && !origin_present,
        Access::BearerChallenge => !origin_present,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_declared_core_route_has_a_policy_and_unknown_routes_are_denied() {
        for source in [include_str!("http.rs"), include_str!("cli_install.rs")] {
            for tail in source.split(".route(").skip(1) {
                if let Some(literal) = tail.trim_start().strip_prefix('"') {
                    let route = literal.split('"').next().unwrap();
                    assert!(
                        access(route, &Method::GET).is_some(),
                        "unclassified {route}"
                    );
                }
            }
        }
        assert_eq!(access("/api/v1/new-management-route", &Method::GET), None);
    }

    #[test]
    fn browser_purpose_cannot_be_replaced_by_origin_or_a_bearer_header() {
        for route in [
            "/api/v1/users",
            "/api/v1/cli/devices/{id}/decision",
            "/api/v1/cli-authorizations/{id}/approve",
            "/api/v1/delegations",
        ] {
            let rule = access(route, &Method::POST).unwrap();
            assert!(!allowed(rule, BrowserPurpose::Portal, false, true));
            assert!(!allowed(rule, BrowserPurpose::Portal, true, true));
            assert!(allowed(rule, BrowserPurpose::Console, false, true));
            assert!(allowed(rule, BrowserPurpose::Portal, true, false));
        }
        for bearer in [true, false] {
            assert!(!allowed(
                Access::Portal,
                BrowserPurpose::Console,
                bearer,
                false
            ));
        }
        assert_eq!(
            access("/api/v1/installations", &Method::GET),
            Some(Access::Portal)
        );
        assert_eq!(
            access("/api/v1/installations", &Method::POST),
            Some(Access::Console)
        );
    }
}
