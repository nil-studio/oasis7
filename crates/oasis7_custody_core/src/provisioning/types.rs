use crate::DecimalU64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManagerPrincipal {
    pub identity_namespace: String,
    pub native_account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BootstrapManager {
    pub principal: ManagerPrincipal,
    pub environments: Vec<String>,
}

/// These are configuration-owned route IDs, not caller-selected URLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProvisionPlan {
    pub environment: String,
    pub identity_route: String,
    pub world_route: String,
    pub world_identity: String,
    pub purpose: String,
    pub expires_at: DecimalU64,
    pub transport_limit: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProvisionRoutes {
    pub environment: String,
    pub identity_route: String,
    pub world_route: String,
    pub world_identity: String,
}

/// Deployment-owned routing and bootstrap policy, re-read before admission.
pub struct ProvisionPolicy<'a> {
    pub routes: &'a ProvisionRoutes,
    pub managers: &'a [BootstrapManager],
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionError {
    Invalid,
    Forbidden,
    Frozen,
    Conflict,
    Expired,
    Exhausted,
    Missing,
    InvalidTransition,
    Encoding,
    Overflow,
}

impl ProvisionPlan {
    pub fn validate(&self, now: u64, routes: &ProvisionRoutes) -> Result<(), ProvisionError> {
        self.validate_scope(routes)?;
        if self.expires_at.0 <= now {
            return Err(ProvisionError::Expired);
        }
        Ok(())
    }
    pub(super) fn validate_scope(&self, routes: &ProvisionRoutes) -> Result<(), ProvisionError> {
        if self.environment.is_empty()
            || self.identity_route.is_empty()
            || self.world_route.is_empty()
            || self.world_identity.is_empty()
            || self.purpose.is_empty()
            || self.purpose.len() > 256
            || self.transport_limit.0 == 0
        {
            return Err(ProvisionError::Invalid);
        }
        if self.environment != routes.environment
            || self.identity_route != routes.identity_route
            || self.world_route != routes.world_route
            || self.world_identity != routes.world_identity
        {
            return Err(ProvisionError::Forbidden);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, ProvisionError> {
        digest(&("oasis7.custody.provision-plan.v1", self))
    }
}

pub(super) fn digest(value: &impl Serialize) -> Result<String, ProvisionError> {
    let bytes =
        oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|_| ProvisionError::Encoding)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(super) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}
