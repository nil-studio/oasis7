//! Public, non-secret HTTP contracts shared by the real Worker handler.
use serde::{Deserialize, Serialize};
#[cfg(test)]
#[path = "http_schema.rs"]
mod schema_export;

pub const SCHEMA_REVISION: &str = "custody-http-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Identity,
    Owner,
    Executor,
    RecoveryFence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ServiceMetadata {
    pub schema_revision: String,
    pub role: Role,
    pub api_profile: String,
    pub execution_enabled: bool,
}

impl ServiceMetadata {
    pub fn disabled(role: Role) -> Self {
        Self {
            schema_revision: SCHEMA_REVISION.into(),
            role,
            api_profile: "custody-foundation-v1".into(),
            execution_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    NotReady,
    MethodNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
    pub schema_revision: String,
    pub code: ErrorCode,
}

impl ErrorResponse {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            schema_revision: SCHEMA_REVISION.into(),
            code,
        }
    }
}

/// Exactly the public routing policy used by all four fetch handlers.
/// Business routes remain disabled until the native identity/PoP adapters exist.
pub fn route(method: &str, path: &str, has_query: bool) -> Result<(), (u16, ErrorCode)> {
    if path != "/v1/metadata" || has_query {
        return Err((404, ErrorCode::NotFound));
    }
    if method != "GET" {
        return Err((405, ErrorCode::MethodNotAllowed));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_routing_never_exposes_business_or_secret_routes() {
        assert!(route("GET", "/v1/metadata", false).is_ok());
        for path in [
            "/v1/accounts",
            "/v1/actions",
            "/v1/owner-intents",
            "/get_secret",
            "/v1/metadata/",
        ] {
            assert_eq!(route("POST", path, false), Err((404, ErrorCode::NotFound)));
        }
        assert_eq!(
            route("GET", "/v1/metadata", true),
            Err((404, ErrorCode::NotFound))
        );
        assert_eq!(
            route("POST", "/v1/metadata", false),
            Err((405, ErrorCode::MethodNotAllowed))
        );
    }
    #[test]
    fn emit_http_contract() {
        // Deliberate developer export, never executed by a build script or handler.
        if let Ok(directory) = std::env::var("OASIS7_CUSTODY_SCHEMA_OUTPUT") {
            schema_export::export(std::path::Path::new(&directory));
        }
    }
}
