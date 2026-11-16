//! Shared dev-test-accounts-v1 contract; callers own authentication, current native
//! authority verification, storage transactions, clocks and cryptographic entropy.
mod operations;
mod sessions;
pub use operations::{Arguments, Call, Ledger, Original, Scope, usage_key, valid_id};
pub use sessions::{Binding, Grant, Sessions};

use serde::{Deserialize, Serialize};
/// Trusted transport envelope. Model-facing adapters must reject credential injection.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRequest {
    pub credential: String,
    pub method: String,
    pub grant_ref: Option<String>,
    pub session_ref: Option<String>,
    pub expires_at_unix: Option<u64>,
    pub operation: Option<Call>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<Call>,
}

/// Closed business methods, shared by adapters and the trusted service.
pub const METHODS: [&str; 9] = [
    "accounts",
    "open",
    "close",
    "describe",
    "task",
    "capabilities",
    "context",
    "move",
    "lookup",
];

#[cfg(test)]
mod tests;
