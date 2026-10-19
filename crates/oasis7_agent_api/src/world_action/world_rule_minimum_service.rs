//! Authority-certified minimum-service contracts, independent of rescue requests.
use super::{EmergencyServiceRegistrationV1, GovernanceFinalityCertificate};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum MinimumServiceCommandV1 {
    Register {
        registration: EmergencyServiceRegistrationV1,
        region_budget_lineage: String,
        certificate: GovernanceFinalityCertificate,
    },
}
impl MinimumServiceCommandV1 {
    pub fn authority_scope(&self) -> String {
        match self {
            Self::Register { registration, .. } => registration.obligation_id.clone(),
        }
    }
}
/// Domain-separated payload; the certificate manifest is its SHA-256 digest.
pub fn minimum_service_authority_payload(
    actor: &str,
    registration: &EmergencyServiceRegistrationV1,
    region_budget_lineage: &str,
    world_binding: &str,
    now: u64,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&(
        "oasis7.minimum-service.register.v1",
        world_binding,
        now,
        actor,
        registration,
        region_budget_lineage,
    ))
}

pub fn minimum_service_authority_manifest(
    actor: &str,
    registration: &EmergencyServiceRegistrationV1,
    region_budget_lineage: &str,
    world_binding: &str,
    now: u64,
) -> Result<String, serde_json::Error> {
    minimum_service_authority_payload(
        actor,
        registration,
        region_budget_lineage,
        world_binding,
        now,
    )
    .map(|bytes| {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(bytes))
    })
}
