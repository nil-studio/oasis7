//! Explicit framing for a paid cleanup HTTP. The attestation is excluded from
//! its own digest preimage; every other decoded body field is canonical-bound.
//! This codec does not issue payment or enable a Native/Node endpoint.
use crate::{paid_cleanup_transport_v1::*, world_service::task_cleanup_v2::*};
use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CleanupTransportBodyV1 {
    Submit(Box<TaskCleanupSubmitV2>),
    Lookup(Box<TaskCleanupLookupV2>),
}
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct PaidCleanupHttpEnvelopeV1 {
    pub api_revision: u8,
    pub body: CleanupTransportBodyV1,
    pub paid: PaidCleanupTransportProofV1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEnvelopeV1 {
    api_revision: u8,
    body: CleanupTransportBodyV1,
    paid: PaidCleanupTransportProofV1,
}
impl<'de> Deserialize<'de> for PaidCleanupHttpEnvelopeV1 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let raw: RawEnvelopeV1 =
            serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
        let result = Self {
            api_revision: raw.api_revision,
            body: raw.body,
            paid: raw.paid,
        };
        if serde_json::to_value(&result).map_err(serde::de::Error::custom)? != value {
            return Err(serde::de::Error::custom(
                "unsupported cleanup HTTP field or representation",
            ));
        }
        Ok(result)
    }
}
impl core::fmt::Debug for PaidCleanupHttpEnvelopeV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PaidCleanupHttpEnvelopeV1(<private>)")
    }
}
impl CleanupTransportBodyV1 {
    pub fn operation(&self) -> CleanupHttpOperationV1 {
        match self {
            Self::Submit(_) => CleanupHttpOperationV1::Submit,
            Self::Lookup(_) => CleanupHttpOperationV1::Lookup,
        }
    }
    pub fn submitted(&self) -> &TaskCleanupSubmitV2 {
        match self {
            Self::Submit(q) => q,
            Self::Lookup(q) => &q.submitted,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Submit(q) => q.validate()?,
            Self::Lookup(q) => q.validate()?,
        }
        self.submitted().verify_owner_signature()
    }
    /// Role separation prevents moving the same typed digest between the two
    /// physical requests. No signature/proof bytes occur in their own preimage.
    pub fn body_digest(&self, role: CleanupConsumerRoleV1) -> Result<String, String> {
        self.validate()?;
        billing_digest("oasis7.custody.cleanup-http-body.v1", &(1u8, role, self))
    }
    pub fn scope_digest(&self) -> Result<String, String> {
        self.validate()?;
        let p = &self.submitted().original.request;
        billing_digest(
            "oasis7.custody.cleanup-scope.v1",
            &(
                &self.submitted().expected_world,
                &p.custody_account_id,
                &p.termination_plan_digest,
                &p.permitted_branches,
                &p.signer_public_key,
                p.signer_generation,
                p.signer_nonce,
                p.revoke_generation,
                p.revoke_nonce,
            ),
        )
    }
}
impl PaidCleanupHttpEnvelopeV1 {
    /// Recompute from the actual closed decoded body, never accept the packet's
    /// asserted digest as evidence of the HTTP body passed to an adapter.
    pub fn bound_body_digest(&self, role: CleanupConsumerRoleV1) -> Result<String, String> {
        if self.api_revision != 1 {
            return Err("cleanup HTTP envelope revision mismatch".into());
        }
        self.body.validate()?;
        self.paid.request.validate()?;
        let submitted = self.body.submitted();
        let original = &submitted.original.request;
        let p = &self.paid.request;
        let digest = self.body.body_digest(role)?;
        let expected = match role {
            CleanupConsumerRoleV1::Native => &p.native_body_digest,
            CleanupConsumerRoleV1::World => &p.world_body_digest,
        };
        if p.operation != self.body.operation()
            || p.world != submitted.expected_world
            || p.original_digest != submitted.original_digest()?
            || p.scope_digest != self.body.scope_digest()?
            || p.canonical_operation_id != original.client_operation_id
            || p.custody_account_id != original.custody_account_id
            || p.termination_plan_digest != original.termination_plan_digest
            || p.branch_id != original.permitted_branches.exact_initial_active.branch_id
            || p.reorg_epoch != original.permitted_branches.exact_initial_active.reorg_epoch
            || (p.operation == CleanupHttpOperationV1::Submit
                && p.expires_at.0 > original.cleanup_until_ms.0)
            || expected != &digest
        {
            return Err("cleanup HTTP body/paid binding mismatch".into());
        }
        Ok(digest)
    }
}

#[cfg(test)]
#[path = "cleanup_http_envelope_v1_tests.rs"]
mod tests;
