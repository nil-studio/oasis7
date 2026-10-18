//! World event types that wrap all event kinds.

use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{CapabilityGrantV2, ModuleCallFailure, ModuleEmitEvent, ModuleStateUpdate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::audit::AuditEventKind;
use super::capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityFinalityProof, CapabilityAuthorityRecord,
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord,
    CapabilityBudgetAccount, CapabilityEffectReceiptLink, CapabilityInvocationContext,
};
use super::effect::{EffectIntent, EffectReceipt};
use super::events::{CausedBy, DomainEvent};
use super::governance::{GovernanceEvent, GovernanceFinalityCertificate};
use super::manifest::ManifestUpdate;
use super::modules::ModuleEvent;
use super::policy::PolicyDecisionRecord;
use super::rules::{ActionOverrideRecord, RuleDecisionRecord};
use super::snapshot::{RollbackEvent, SnapshotMeta};
use super::state::ProductValidationDeliveryCursor;
use super::types::{WorldEventId, WorldTime};

/// A world event with full metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEvent {
    pub id: WorldEventId,
    pub time: WorldTime,
    pub caused_by: Option<CausedBy>,
    pub body: WorldEventBody,
}

impl WorldEvent {
    pub fn audit_kind(&self) -> AuditEventKind {
        match self.body {
            WorldEventBody::Domain(_) => AuditEventKind::Domain,
            WorldEventBody::EffectQueued(_) => AuditEventKind::EffectQueued,
            WorldEventBody::ReceiptAppended(_) => AuditEventKind::ReceiptAppended,
            WorldEventBody::CapabilityAuthorization(_) => AuditEventKind::CapabilityAuthorization,
            WorldEventBody::PolicyDecisionRecorded(_) => AuditEventKind::PolicyDecision,
            WorldEventBody::RuleDecisionRecorded(_) => AuditEventKind::RuleDecision,
            WorldEventBody::ActionOverridden(_) => AuditEventKind::ActionOverridden,
            WorldEventBody::Governance(_) => AuditEventKind::Governance,
            WorldEventBody::ModuleEvent(_) => AuditEventKind::ModuleEvent,
            WorldEventBody::ModuleCallFailed(_) => AuditEventKind::ModuleCallFailed,
            WorldEventBody::ModuleEmitted(_) => AuditEventKind::ModuleEmitted,
            WorldEventBody::ModuleStateUpdated(_) => AuditEventKind::ModuleStateUpdated,
            WorldEventBody::ModuleRuntimeCharged(_) => AuditEventKind::ModuleRuntimeCharged,
            WorldEventBody::ProductValidationDeliveryCursorUpdated(_) => {
                AuditEventKind::ProductValidationDeliveryCursorUpdated
            }
            WorldEventBody::SnapshotCreated(_) => AuditEventKind::SnapshotCreated,
            WorldEventBody::ManifestUpdated(_) => AuditEventKind::ManifestUpdated,
            WorldEventBody::RollbackApplied(_) => AuditEventKind::RollbackApplied,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleRuntimeChargeEvent {
    pub module_id: String,
    pub trace_id: String,
    pub payer_agent_id: String,
    pub compute_fee_kind: ResourceKind,
    pub compute_fee_amount: i64,
    pub electricity_fee_kind: ResourceKind,
    pub electricity_fee_amount: i64,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub effect_count: u32,
    pub emit_count: u32,
}

/// Journal evidence for every mutation in the trusted capability lane.
///
/// These events carry the post-transition values instead of relying on a
/// snapshot to capture in-memory authorization maps.  Recovery can therefore
/// replay a journal tail after an older snapshot without re-running a module
/// or accepting a second nonce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "Capability journal events preserve the stable persisted wire shape for replay compatibility."
)]
#[serde(tag = "type", content = "data")]
pub enum CapabilityAuthorizationEvent {
    /// Legacy record-only authority admission. Recovery intentionally rejects
    /// this shape because it has no replayable finality evidence.
    AuthorityInstalled {
        record: CapabilityAuthorityRecord,
    },
    /// Compatibility event for the pre-binding certificate-only admission
    /// shape. Recovery rejects it because the certificate does not bind every
    /// authority-record field.
    AuthorityInstalledWithFinality {
        record: CapabilityAuthorityRecord,
        certificate: GovernanceFinalityCertificate,
    },
    /// Proof-bearing trust-root admission. The proof is retained in the
    /// journal and snapshot so recovery can revalidate the binding.
    AuthorityInstalledWithProof {
        record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    },
    AgentIdentityInstalled {
        agent_id: String,
        identity: CapabilityAgentIdentity,
    },
    AgentSignerDelegationInstalled {
        signed: crate::service_admission::SignedReadRequest<
            crate::service_admission::AgentSignerDelegationChangeV1,
        >,
    },
    WorldServiceIntentRecorded {
        key: String,
        record: serde_json::Value,
    },
    SystemIdentityInstalled {
        system_id: String,
        epoch: u64,
    },
    InvocationContextInstalled {
        key: String,
        context: CapabilityInvocationContext,
    },
    BudgetAccountInstalled {
        key: String,
        account: CapabilityBudgetAccount,
    },
    GrantRegistered {
        grant: CapabilityGrantV2,
    },
    CommandCommitted {
        budget_key: String,
        /// The predecessor budget values are retained beside the post-state
        /// so replay can verify the exact deterministic spend transition even
        /// when the live staged world already contains the post-state.
        #[serde(default)]
        budget_before_remaining_units: i64,
        #[serde(default)]
        budget_before_spent_units: i64,
        #[serde(default)]
        state_hash_before: String,
        #[serde(default)]
        receipt_hash: String,
        budget_account: CapabilityBudgetAccount,
        grant: CapabilityGrantV2,
        nonce_key: String,
        nonce_record: CapabilityAuthorizationNonceRecord,
        receipt: CapabilityAuthorizationAuditReceipt,
        effect_receipt_links: BTreeMap<String, CapabilityEffectReceiptLink>,
    },
    EffectReceiptCommitted {
        intent_id: String,
        authorization_receipt_id: String,
        effect_receipt_id: String,
    },
}

/// The body/payload of a world event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "World event bodies preserve the stable persisted event envelope for replay compatibility."
)]
#[serde(tag = "kind", content = "payload")]
pub enum WorldEventBody {
    Domain(DomainEvent),
    EffectQueued(EffectIntent),
    ReceiptAppended(EffectReceipt),
    CapabilityAuthorization(CapabilityAuthorizationEvent),
    PolicyDecisionRecorded(PolicyDecisionRecord),
    RuleDecisionRecorded(RuleDecisionRecord),
    ActionOverridden(ActionOverrideRecord),
    Governance(GovernanceEvent),
    ModuleEvent(ModuleEvent),
    ModuleCallFailed(ModuleCallFailure),
    ModuleEmitted(ModuleEmitEvent),
    ModuleStateUpdated(ModuleStateUpdate),
    ModuleRuntimeCharged(ModuleRuntimeChargeEvent),
    ProductValidationDeliveryCursorUpdated(ProductValidationDeliveryCursor),
    SnapshotCreated(SnapshotMeta),
    ManifestUpdated(ManifestUpdate),
    RollbackApplied(RollbackEvent),
}
