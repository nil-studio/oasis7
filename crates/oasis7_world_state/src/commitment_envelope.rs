//! Versioned local `WorldCommitmentEnvelope` for snapshot and `C -> T` recovery integrity.
//!
//! Per `doc/world-runtime/design.md §6.5.2 (2)`:
//! - `WorldCommitmentEnvelope v2` binds `business_state_root`, `authorization_root`,
//!   `governance_sidecar_root`, and `control_continuation_root` (J1/J2 + sequence/queue
//!   continuation) for local checkpoint persistence and `C -> T` event-suffix recovery.
//! - Existing consensus block header fields (`TickBlockHeader.state_root` /
//!   `TickExecutionDigest.state_projection_hash`) remain on their canonical projection.
//! - Legacy `business_state_root` (`SHA256(JSON(StateRootProjection))`) and IWS
//!   `RootSetV1` namespace Merkle roots have distinct semantics and are bound via
//!   explicit recovery profiles rather than conflated by field name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_wasm_abi::canonical_hash;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::capability_authorization::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord,
    CapabilityBudgetAccount, CapabilityEffectReceiptLink, CapabilityInvocationContext,
    CapabilityRevocationState,
};
use crate::codec::{StateRootProjection, state_root_hash};
use crate::effect::EffectIntent;
use crate::error::WorldError;
use crate::events::ActionEnvelope;
use crate::governance::{
    GovernanceExecutionPolicy, GovernanceFinalityEpochSnapshot, GovernanceIdentityPenaltyRecord,
    Proposal,
};
use crate::manifest::Manifest;
use crate::policy::PolicySet;
use crate::snapshot::{RollbackAuthorityRegistry, RollbackNonceOutcome, Snapshot};
use crate::state::{WorldState, WorldStateProjection};
use crate::types::{ActionId, IntentSeq, ProposalId, WorldEventId, WorldTime};
use crate::util::{atomic_write_json_to_path, hash_json, read_json_from_path, to_canonical_cbor};

/// 32-byte deterministic commitment digest.
pub type Digest32 = [u8; 32];

/// Legacy compatibility view schema version (`business_state_root` + `authorization_root` only).
pub const WORLD_COMMITMENT_ENVELOPE_SCHEMA_V1: u16 = 1;

/// Full local recovery commitment envelope schema version (includes governance & J1/J2 control roots).
pub const WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2: u16 = 2;

/// Default file name for versioned local `WorldCommitmentEnvelope` recovery metadata inside a world directory.
pub const WORLD_COMMITMENT_ENVELOPE_FILE: &str = "world-commitment-envelope.json";

/// Domain framing for `WorldCommitmentEnvelope v2` self-digest.
pub const WORLD_COMMITMENT_ENVELOPE_V2_DOMAIN: &str = "oasis7.world_commitment_envelope.v2";

/// Domain framing for `governance_sidecar_root` in `WorldCommitmentEnvelope v2`.
pub const GOVERNANCE_SIDECAR_COMMITMENT_V2_DOMAIN: &str =
    "oasis7.world_commitment.governance_sidecar.v2";

/// Domain framing for `control_continuation_root` (J1/J2) in `WorldCommitmentEnvelope v2`.
pub const CONTROL_CONTINUATION_COMMITMENT_V2_DOMAIN: &str =
    "oasis7.world_commitment.control_continuation.v2";

/// Domain framing for `IncrementalWorldCommitmentBindingV2` (IWS `RootSetV1` local recovery binding).
pub const INCREMENTAL_COMMITMENT_BINDING_V2_DOMAIN: &str =
    "oasis7.world_commitment.incremental_binding.v2";

/// Fixed world format & recovery profile for local commitment verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCommitmentRecoveryProfile {
    /// Legacy ordinary Runtime checkpoint (`v1` compatibility view: business + authorization roots).
    OrdinaryRuntimeSnapshotV1,
    /// Versioned ordinary Runtime checkpoint (`v2` envelope: business + authorization + governance + J1/J2 control).
    OrdinaryRuntimeSnapshotV2,
    /// IWS incremental world state (`#5209`) `RootSetV1` local recovery binding without full `World` materialization.
    IncrementalRootSetBindingV2,
}

/// Decode a 64-character hex SHA-256 digest into its exact 32-byte representation.
pub fn digest32_from_hex(hex_str: &str) -> Result<Digest32, WorldError> {
    let trimmed = hex_str.trim();
    if trimmed.len() != 64 {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "invalid Digest32 hex length: expected 64 hex chars, found {}",
                trimmed.len()
            ),
        });
    }
    let bytes = hex::decode(trimmed).map_err(|err| WorldError::DistributedValidationFailed {
        reason: format!("invalid Digest32 hex encoding: {err}"),
    })?;
    bytes
        .try_into()
        .map_err(|_| WorldError::DistributedValidationFailed {
            reason: "invalid Digest32 byte length".to_string(),
        })
}

/// Encode a 32-byte commitment digest as lowercase 64-character hex.
pub fn digest32_to_hex(digest: &Digest32) -> String {
    hex::encode(digest)
}

fn domain_framed_canonical_cbor_sha256<T: Serialize>(
    domain: &str,
    payload: &T,
) -> Result<Digest32, WorldError> {
    let cbor = to_canonical_cbor(payload)?;
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0u8]);
    hasher.update(&cbor);
    Ok(hasher.finalize().into())
}

/// Borrowed view of durable capability authorization state matching `CapabilityAuthorizationRootBody`.
#[derive(Serialize)]
pub struct AuthorizationRootCommitmentView<'a> {
    pub grants: &'a BTreeMap<String, serde_json::Value>,
    pub revocation: &'a CapabilityRevocationState,
    pub invocation_contexts: &'a BTreeMap<String, CapabilityInvocationContext>,
    pub budget_accounts: &'a BTreeMap<String, CapabilityBudgetAccount>,
    pub nonce_records: &'a BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    pub receipts: &'a BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    pub effect_receipt_links: &'a BTreeMap<String, CapabilityEffectReceiptLink>,
}

/// Compute the canonical authorization root hex string, excluding `revocation.world_service_results`.
pub fn compute_authorization_root_hex(
    view: &AuthorizationRootCommitmentView<'_>,
) -> Result<String, WorldError> {
    if let Some(index) = &view.revocation.task_terminals_v2 {
        index
            .validate()
            .map_err(|reason| WorldError::CapabilityAuthorizationDenied { reason })?;
    }
    let mut authority = view.revocation.clone();
    authority.world_service_results.clear();
    let stripped = AuthorizationRootCommitmentView {
        grants: view.grants,
        revocation: &authority,
        invocation_contexts: view.invocation_contexts,
        budget_accounts: view.budget_accounts,
        nonce_records: view.nonce_records,
        receipts: view.receipts,
        effect_receipt_links: view.effect_receipt_links,
    };
    canonical_hash(&stripped).map_err(|error| WorldError::CapabilityAuthorizationDenied {
        reason: format!("authorization root: {error}"),
    })
}

/// Compute the 32-byte `authorization_root` directly from a borrowed `AuthorizationRootCommitmentView`.
pub fn compute_authorization_root_digest32(
    view: &AuthorizationRootCommitmentView<'_>,
) -> Result<Digest32, WorldError> {
    let hex_root = compute_authorization_root_hex(view)?;
    digest32_from_hex(&hex_root)
}

/// Compute the 32-byte `business_state_root` (`SHA256(JSON(StateRootProjection))`) from borrowed inputs.
pub fn compute_business_state_root_digest32(
    state: &WorldState,
    manifest: &Manifest,
    policies: &PolicySet,
) -> Result<Digest32, WorldError> {
    let manifest_hash = hash_json(manifest)?;
    let policy_hash = hash_json(policies)?;
    let projection = WorldStateProjection::borrowed(state);
    let root_hex = state_root_hash(&StateRootProjection {
        state: &projection,
        manifest_hash: &manifest_hash,
        policy_hash: &policy_hash,
    })?;
    digest32_from_hex(&root_hex)
}

/// Borrowed view of governance sidecar state outside canonical `WorldState`.
#[derive(Debug, Clone, Serialize)]
pub struct GovernanceSidecarCommitmentView<'a> {
    pub proposals: &'a BTreeMap<ProposalId, Proposal>,
    pub next_proposal_id: ProposalId,
    pub proposal_id_era: u64,
    pub governance_execution_policy: &'a GovernanceExecutionPolicy,
    pub governance_finality_epoch_snapshots: &'a BTreeMap<u64, GovernanceFinalityEpochSnapshot>,
    pub governance_emergency_brake_until_tick: Option<WorldTime>,
    pub governance_identity_penalties: &'a BTreeMap<u64, GovernanceIdentityPenaltyRecord>,
    pub next_governance_identity_penalty_id: u64,
    pub rollback_authority_registry: &'a RollbackAuthorityRegistry,
    pub consumed_rollback_nonces: &'a BTreeSet<String>,
    pub rollback_nonce_outcomes: &'a BTreeMap<String, RollbackNonceOutcome>,
}

impl<'a> GovernanceSidecarCommitmentView<'a> {
    pub fn from_snapshot(snapshot: &'a Snapshot) -> Self {
        Self {
            proposals: &snapshot.proposals,
            next_proposal_id: snapshot.next_proposal_id,
            proposal_id_era: snapshot.proposal_id_era,
            governance_execution_policy: &snapshot.governance_execution_policy,
            governance_finality_epoch_snapshots: &snapshot.governance_finality_epoch_snapshots,
            governance_emergency_brake_until_tick: snapshot.governance_emergency_brake_until_tick,
            governance_identity_penalties: &snapshot.governance_identity_penalties,
            next_governance_identity_penalty_id: snapshot.next_governance_identity_penalty_id,
            rollback_authority_registry: &snapshot.rollback_authority_registry,
            consumed_rollback_nonces: &snapshot.consumed_rollback_nonces,
            rollback_nonce_outcomes: &snapshot.rollback_nonce_outcomes,
        }
    }

    pub fn digest32(&self) -> Result<Digest32, WorldError> {
        domain_framed_canonical_cbor_sha256(GOVERNANCE_SIDECAR_COMMITMENT_V2_DOMAIN, self)
    }
}

/// Borrowed view of control continuation state (J1 action allocator/queue, J2 module tick wake
/// schedule, and rolling event/intent/effect sequence state) outside canonical `WorldState`.
#[derive(Debug, Clone, Serialize)]
pub struct ControlContinuationCommitmentView<'a> {
    pub journal_len: usize,
    pub last_event_id: WorldEventId,
    pub event_id_era: u64,
    pub next_action_id: ActionId,
    pub action_id_era: u64,
    pub next_intent_id: IntentSeq,
    pub intent_id_era: u64,
    pub pending_actions: &'a [ActionEnvelope],
    pub pending_effects: &'a [EffectIntent],
    pub inflight_effects: &'a BTreeMap<String, EffectIntent>,
    pub module_tick_schedule: &'a BTreeMap<String, WorldTime>,
    pub scheduler_cursor: Option<&'a str>,
}

impl<'a> ControlContinuationCommitmentView<'a> {
    pub fn from_snapshot(snapshot: &'a Snapshot) -> Self {
        Self {
            journal_len: snapshot.journal_len,
            last_event_id: snapshot.last_event_id,
            event_id_era: snapshot.event_id_era,
            next_action_id: snapshot.next_action_id,
            action_id_era: snapshot.action_id_era,
            next_intent_id: snapshot.next_intent_id,
            intent_id_era: snapshot.intent_id_era,
            pending_actions: &snapshot.pending_actions,
            pending_effects: &snapshot.pending_effects,
            inflight_effects: &snapshot.inflight_effects,
            module_tick_schedule: &snapshot.module_tick_schedule,
            scheduler_cursor: snapshot.scheduler_cursor.as_deref(),
        }
    }

    pub fn digest32(&self) -> Result<Digest32, WorldError> {
        domain_framed_canonical_cbor_sha256(CONTROL_CONTINUATION_COMMITMENT_V2_DOMAIN, self)
    }
}

/// Versioned local commitment envelope binding business, authorization, governance sidecar,
/// and control continuation (J1/J2) digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCommitmentEnvelope {
    pub schema_version: u16,
    pub business_state_root: Digest32,
    pub authorization_root: Digest32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance_sidecar_root: Option<Digest32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_continuation_root: Option<Digest32>,
}

impl WorldCommitmentEnvelope {
    /// Construct a `v1` compatibility view containing only the two legacy roots.
    pub fn new_v1(business_state_root: Digest32, authorization_root: Digest32) -> Self {
        Self {
            schema_version: WORLD_COMMITMENT_ENVELOPE_SCHEMA_V1,
            business_state_root,
            authorization_root,
            governance_sidecar_root: None,
            control_continuation_root: None,
        }
    }

    /// Construct a `v2` commitment envelope binding all four roots.
    pub fn new_v2(
        business_state_root: Digest32,
        authorization_root: Digest32,
        governance_sidecar_root: Digest32,
        control_continuation_root: Digest32,
    ) -> Self {
        Self {
            schema_version: WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2,
            business_state_root,
            authorization_root,
            governance_sidecar_root: Some(governance_sidecar_root),
            control_continuation_root: Some(control_continuation_root),
        }
    }

    /// Return the recovery profile corresponding to this validated envelope version.
    pub fn recovery_profile(&self) -> Result<WorldCommitmentRecoveryProfile, WorldError> {
        self.validate_structure()?;
        match self.schema_version {
            WORLD_COMMITMENT_ENVELOPE_SCHEMA_V1 => {
                Ok(WorldCommitmentRecoveryProfile::OrdinaryRuntimeSnapshotV1)
            }
            WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 => {
                Ok(WorldCommitmentRecoveryProfile::OrdinaryRuntimeSnapshotV2)
            }
            _ => unreachable!(),
        }
    }

    /// Validate schema version and required/forbidden sidecar root presence.
    ///
    /// - `v1` must omit `governance_sidecar_root` and `control_continuation_root`.
    /// - `v2` must include both `governance_sidecar_root` and `control_continuation_root`
    ///   (never silently downgrading to `v1`).
    /// - Any other `schema_version` is rejected.
    pub fn validate_structure(&self) -> Result<(), WorldError> {
        match self.schema_version {
            WORLD_COMMITMENT_ENVELOPE_SCHEMA_V1 => {
                if self.governance_sidecar_root.is_some()
                    || self.control_continuation_root.is_some()
                {
                    return Err(WorldError::DistributedValidationFailed {
                        reason: "WorldCommitmentEnvelope v1 must not carry v2 sidecar roots"
                            .to_string(),
                    });
                }
                Ok(())
            }
            WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 => {
                if self.governance_sidecar_root.is_none() {
                    return Err(WorldError::DistributedValidationFailed {
                        reason:
                            "WorldCommitmentEnvelope v2 missing required governance_sidecar_root"
                                .to_string(),
                    });
                }
                if self.control_continuation_root.is_none() {
                    return Err(WorldError::DistributedValidationFailed {
                        reason:
                            "WorldCommitmentEnvelope v2 missing required control_continuation_root"
                                .to_string(),
                    });
                }
                Ok(())
            }
            other => Err(WorldError::DistributedValidationFailed {
                reason: format!("unsupported WorldCommitmentEnvelope schema_version {other}"),
            }),
        }
    }

    /// Build the `v1` compatibility envelope view from a `Snapshot`.
    pub fn v1_from_snapshot(snapshot: &Snapshot) -> Result<Self, WorldError> {
        let business_state_root = compute_business_state_root_digest32(
            &snapshot.state,
            &snapshot.manifest,
            &snapshot.policies,
        )?;
        let authorization_root = snapshot_verified_authorization_root_digest32(snapshot)?;
        Ok(Self::new_v1(business_state_root, authorization_root))
    }

    /// Build the full `v2` commitment envelope from a `Snapshot`.
    pub fn v2_from_snapshot(snapshot: &Snapshot) -> Result<Self, WorldError> {
        let business_state_root = compute_business_state_root_digest32(
            &snapshot.state,
            &snapshot.manifest,
            &snapshot.policies,
        )?;
        let authorization_root = snapshot_verified_authorization_root_digest32(snapshot)?;
        let governance_sidecar_root =
            GovernanceSidecarCommitmentView::from_snapshot(snapshot).digest32()?;
        let control_continuation_root =
            ControlContinuationCommitmentView::from_snapshot(snapshot).digest32()?;
        Ok(Self::new_v2(
            business_state_root,
            authorization_root,
            governance_sidecar_root,
            control_continuation_root,
        ))
    }

    /// Verify this commitment envelope against a `Snapshot`.
    pub fn verify_against_snapshot(&self, snapshot: &Snapshot) -> Result<(), WorldError> {
        self.validate_structure()?;
        let expected_business = compute_business_state_root_digest32(
            &snapshot.state,
            &snapshot.manifest,
            &snapshot.policies,
        )?;
        if self.business_state_root != expected_business {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "WorldCommitmentEnvelope business_state_root mismatch: expected {}, actual {}",
                    digest32_to_hex(&expected_business),
                    digest32_to_hex(&self.business_state_root),
                ),
            });
        }
        let expected_auth = snapshot_verified_authorization_root_digest32(snapshot)?;
        if self.authorization_root != expected_auth {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "WorldCommitmentEnvelope authorization_root mismatch: expected {}, actual {}",
                    digest32_to_hex(&expected_auth),
                    digest32_to_hex(&self.authorization_root),
                ),
            });
        }
        if self.schema_version == WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 {
            let expected_governance =
                GovernanceSidecarCommitmentView::from_snapshot(snapshot).digest32()?;
            if self.governance_sidecar_root != Some(expected_governance) {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "WorldCommitmentEnvelope governance_sidecar_root mismatch: expected {}, actual {:?}",
                        digest32_to_hex(&expected_governance),
                        self.governance_sidecar_root.as_ref().map(digest32_to_hex),
                    ),
                });
            }
            let expected_control =
                ControlContinuationCommitmentView::from_snapshot(snapshot).digest32()?;
            if self.control_continuation_root != Some(expected_control) {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "WorldCommitmentEnvelope control_continuation_root mismatch: expected {}, actual {:?}",
                        digest32_to_hex(&expected_control),
                        self.control_continuation_root.as_ref().map(digest32_to_hex),
                    ),
                });
            }
        }
        Ok(())
    }

    /// Compute the domain-framed `oasis7.world_commitment_envelope.v2` digest for a `v2` envelope.
    ///
    /// `v1` is strictly a legacy 2-digest compatibility view and does not define a separate
    /// re-serialized envelope digest.
    pub fn envelope_digest(&self) -> Result<Digest32, WorldError> {
        self.validate_structure()?;
        if self.schema_version != WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 {
            return Err(WorldError::DistributedValidationFailed {
                reason:
                    "WorldCommitmentEnvelope v1 is a compatibility view and has no v2 envelope_digest"
                        .to_string(),
            });
        }
        domain_framed_canonical_cbor_sha256(WORLD_COMMITMENT_ENVELOPE_V2_DOMAIN, self)
    }

    /// Hex-encoded `envelope_digest()` for `v2` envelopes.
    pub fn envelope_digest_hex(&self) -> Result<String, WorldError> {
        Ok(digest32_to_hex(&self.envelope_digest()?))
    }

    pub fn to_json_pretty(&self) -> Result<String, WorldError> {
        self.validate_structure()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json_str(input: &str) -> Result<Self, WorldError> {
        let envelope: Self =
            serde_json::from_str(input).map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("invalid WorldCommitmentEnvelope JSON: {err}"),
            })?;
        envelope.validate_structure()?;
        Ok(envelope)
    }

    pub fn to_canonical_cbor(&self) -> Result<Vec<u8>, WorldError> {
        self.validate_structure()?;
        to_canonical_cbor(self)
    }

    pub fn from_cbor_slice(bytes: &[u8]) -> Result<Self, WorldError> {
        let envelope: Self = serde_cbor::from_slice(bytes).map_err(|err| {
            WorldError::DistributedValidationFailed {
                reason: format!("invalid WorldCommitmentEnvelope CBOR: {err}"),
            }
        })?;
        envelope.validate_structure()?;
        Ok(envelope)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), WorldError> {
        self.validate_structure()?;
        atomic_write_json_to_path(self, path.as_ref())
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, WorldError> {
        let envelope: Self = read_json_from_path(path.as_ref()).map_err(|err| {
            WorldError::DistributedValidationFailed {
                reason: format!("load WorldCommitmentEnvelope failed: {err:?}"),
            }
        })?;
        envelope.validate_structure()?;
        Ok(envelope)
    }
}

fn snapshot_verified_authorization_root_digest32(
    snapshot: &Snapshot,
) -> Result<Digest32, WorldError> {
    let computed_hex = compute_authorization_root_hex(&AuthorizationRootCommitmentView {
        grants: &snapshot.capability_grants_v2,
        revocation: &snapshot.capability_revocation_state,
        invocation_contexts: &snapshot.capability_invocation_contexts,
        budget_accounts: &snapshot.capability_budget_accounts,
        nonce_records: &snapshot.capability_nonce_records,
        receipts: &snapshot.capability_authorization_receipts,
        effect_receipt_links: &snapshot.capability_effect_receipt_links,
    })?;
    if !snapshot.capability_authorization_root.is_empty()
        && snapshot.capability_authorization_root != computed_hex
    {
        return Err(WorldError::CapabilityAuthorizationDenied {
            reason: "capability authorization root does not match durable state".to_string(),
        });
    }
    digest32_from_hex(&computed_hex)
}

/// Versioned local recovery binding for IWS (`#5209`) `RootSetV1` checkpoints.
///
/// Keeps the IWS `RootSetV1` digest and 6 namespace Merkle roots (`Business`, `Allocator`,
/// `Scheduler`, `Authorization`, `Effects`, `Artifacts`) distinct from legacy
/// `StateRootProjection` `business_state_root`, avoiding any full `World` clone or serialization
/// during incremental commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncrementalWorldCommitmentBindingV2 {
    pub schema_version: u16,
    pub world_id: String,
    pub height: u64,
    pub simulation_tick: u64,
    pub root_set_digest: Digest32,
    pub business_namespace_root: Digest32,
    pub allocator_namespace_root: Digest32,
    pub scheduler_namespace_root: Digest32,
    pub authorization_namespace_root: Digest32,
    pub effects_namespace_root: Digest32,
    pub artifacts_namespace_root: Digest32,
    pub governance_sidecar_root: Digest32,
    pub control_continuation_root: Digest32,
}

impl IncrementalWorldCommitmentBindingV2 {
    pub fn from_root_set(
        root_set: &RootSetV1,
        governance_sidecar_root: Digest32,
        control_continuation_root: Digest32,
    ) -> Result<Self, WorldError> {
        let root_set_digest =
            root_set
                .digest()
                .map_err(|err| WorldError::DistributedValidationFailed {
                    reason: format!("invalid IWS RootSetV1: {err}"),
                })?;
        let ns_digest = |ns: StateNamespaceV1| -> Result<Digest32, WorldError> {
            let bytes = root_set.state_root(ns).canonical_bytes().map_err(|err| {
                WorldError::DistributedValidationFailed {
                    reason: format!("invalid IWS namespace root {ns:?}: {err}"),
                }
            })?;
            Ok(*blake3::hash(&bytes).as_bytes())
        };
        Ok(Self {
            schema_version: WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2,
            world_id: root_set.world_id.clone(),
            height: root_set.height,
            simulation_tick: root_set.simulation_tick,
            root_set_digest,
            business_namespace_root: ns_digest(StateNamespaceV1::Business)?,
            allocator_namespace_root: ns_digest(StateNamespaceV1::Allocator)?,
            scheduler_namespace_root: ns_digest(StateNamespaceV1::Scheduler)?,
            authorization_namespace_root: ns_digest(StateNamespaceV1::Authorization)?,
            effects_namespace_root: ns_digest(StateNamespaceV1::Effects)?,
            artifacts_namespace_root: ns_digest(StateNamespaceV1::Artifacts)?,
            governance_sidecar_root,
            control_continuation_root,
        })
    }

    pub fn verify_against_root_set(
        &self,
        root_set: &RootSetV1,
        expected_governance_sidecar_root: &Digest32,
        expected_control_continuation_root: &Digest32,
    ) -> Result<(), WorldError> {
        if self.schema_version != WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "unsupported IncrementalWorldCommitmentBindingV2 schema_version {}",
                    self.schema_version
                ),
            });
        }
        let expected = Self::from_root_set(
            root_set,
            *expected_governance_sidecar_root,
            *expected_control_continuation_root,
        )?;
        if self != &expected {
            return Err(WorldError::DistributedValidationFailed {
                reason: "IncrementalWorldCommitmentBindingV2 mismatch against IWS RootSetV1"
                    .to_string(),
            });
        }
        Ok(())
    }

    pub fn binding_digest(&self) -> Result<Digest32, WorldError> {
        if self.schema_version != WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "unsupported IncrementalWorldCommitmentBindingV2 schema_version {}",
                    self.schema_version
                ),
            });
        }
        domain_framed_canonical_cbor_sha256(INCREMENTAL_COMMITMENT_BINDING_V2_DOMAIN, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::snapshot_from_json;
    use crate::governance::{GovernanceIdentityPenaltyStatus, ProposalStatus};

    fn fixture_snapshot() -> Snapshot {
        snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).expect("fixture snapshot")
    }

    #[test]
    fn v1_and_v2_envelope_preserve_exact_legacy_business_and_authorization_digest32_bytes() {
        let snapshot = fixture_snapshot();
        let manifest_hash = hash_json(&snapshot.manifest).expect("manifest hash");
        let policy_hash = hash_json(&snapshot.policies).expect("policy hash");
        let expected_business_hex = state_root_hash(&StateRootProjection {
            state: &WorldStateProjection::borrowed(&snapshot.state),
            manifest_hash: &manifest_hash,
            policy_hash: &policy_hash,
        })
        .expect("state_root_hash");
        let expected_auth_hex = compute_authorization_root_hex(&AuthorizationRootCommitmentView {
            grants: &snapshot.capability_grants_v2,
            revocation: &snapshot.capability_revocation_state,
            invocation_contexts: &snapshot.capability_invocation_contexts,
            budget_accounts: &snapshot.capability_budget_accounts,
            nonce_records: &snapshot.capability_nonce_records,
            receipts: &snapshot.capability_authorization_receipts,
            effect_receipt_links: &snapshot.capability_effect_receipt_links,
        })
        .expect("auth root hex");

        let v1 = WorldCommitmentEnvelope::v1_from_snapshot(&snapshot).expect("v1 envelope");
        let v2 = WorldCommitmentEnvelope::v2_from_snapshot(&snapshot).expect("v2 envelope");

        assert_eq!(
            digest32_to_hex(&v1.business_state_root),
            expected_business_hex
        );
        assert_eq!(digest32_to_hex(&v1.authorization_root), expected_auth_hex);
        assert_eq!(v1.business_state_root, v2.business_state_root);
        assert_eq!(v1.authorization_root, v2.authorization_root);
        assert_eq!(
            v1.recovery_profile().unwrap(),
            WorldCommitmentRecoveryProfile::OrdinaryRuntimeSnapshotV1
        );
        assert_eq!(
            v2.recovery_profile().unwrap(),
            WorldCommitmentRecoveryProfile::OrdinaryRuntimeSnapshotV2
        );
        v1.verify_against_snapshot(&snapshot).expect("verify v1");
        v2.verify_against_snapshot(&snapshot).expect("verify v2");
        assert!(
            v1.envelope_digest().is_err(),
            "v1 compatibility view must not fabricate a v2 envelope digest"
        );
    }

    #[test]
    fn v2_envelope_json_and_canonical_cbor_roundtrip_and_domain_framed_digest_are_deterministic() {
        let snapshot = fixture_snapshot();
        let v2 = WorldCommitmentEnvelope::v2_from_snapshot(&snapshot).expect("v2 envelope");
        let json = v2.to_json_pretty().expect("to json");
        let from_json = WorldCommitmentEnvelope::from_json_str(&json).expect("from json");
        assert_eq!(from_json, v2);

        let cbor = v2.to_canonical_cbor().expect("to canonical cbor");
        let from_cbor = WorldCommitmentEnvelope::from_cbor_slice(&cbor).expect("from cbor");
        assert_eq!(from_cbor, v2);

        let digest_a = v2.envelope_digest().expect("digest a");
        let digest_b = from_cbor.envelope_digest().expect("digest b");
        assert_eq!(digest_a, digest_b);
        assert_ne!(digest_a, v2.business_state_root);
        assert_ne!(digest_a, v2.authorization_root);
    }

    #[test]
    fn v2_envelope_detects_tampering_in_governance_sidecars_and_j1_j2_control_continuation() {
        let base = fixture_snapshot();
        let v2 = WorldCommitmentEnvelope::v2_from_snapshot(&base).expect("v2 envelope");

        // 1. Governance sidecar: proposal tampering
        let mut tampered_proposal = base.clone();
        tampered_proposal.proposals.insert(
            99,
            Proposal {
                id: 99,
                author: "tamper".into(),
                base_manifest_hash: "base-hash".into(),
                manifest: tampered_proposal.manifest.clone(),
                patch: None,
                queued_at_tick: None,
                not_before_tick: None,
                activate_epoch: None,
                timelock_ticks: 0,
                status: ProposalStatus::Proposed,
            },
        );
        let err = v2
            .verify_against_snapshot(&tampered_proposal)
            .expect_err("proposal tamper must be rejected");
        assert!(format!("{err:?}").contains("governance_sidecar_root mismatch"));

        // 2. Governance sidecar: emergency brake tampering
        let mut tampered_brake = base.clone();
        tampered_brake.governance_emergency_brake_until_tick = Some(42);
        let err = v2
            .verify_against_snapshot(&tampered_brake)
            .expect_err("emergency brake tamper must be rejected");
        assert!(format!("{err:?}").contains("governance_sidecar_root mismatch"));

        // 3. Governance sidecar: identity penalty tampering
        let mut tampered_penalty = base.clone();
        tampered_penalty.governance_identity_penalties.insert(
            1,
            GovernanceIdentityPenaltyRecord {
                penalty_id: 1,
                target_agent_id: "agent-x".into(),
                evidence_hash: "deadbeef".into(),
                reason: "sybil".into(),
                slash_stake: 10,
                appeal_deadline_tick: 100,
                status: GovernanceIdentityPenaltyStatus::Applied,
                ..GovernanceIdentityPenaltyRecord::default()
            },
        );
        let err = v2
            .verify_against_snapshot(&tampered_penalty)
            .expect_err("identity penalty tamper must be rejected");
        assert!(format!("{err:?}").contains("governance_sidecar_root mismatch"));

        // 4. J1 control continuation: action allocator cursor / era tampering
        let mut tampered_j1 = base.clone();
        tampered_j1.next_action_id = tampered_j1.next_action_id.saturating_add(1);
        let err = v2
            .verify_against_snapshot(&tampered_j1)
            .expect_err("J1 next_action_id tamper must be rejected");
        assert!(format!("{err:?}").contains("control_continuation_root mismatch"));

        let mut tampered_j1_era = base.clone();
        tampered_j1_era.action_id_era = tampered_j1_era.action_id_era.saturating_add(1);
        let err = v2
            .verify_against_snapshot(&tampered_j1_era)
            .expect_err("J1 action_id_era tamper must be rejected");
        assert!(format!("{err:?}").contains("control_continuation_root mismatch"));

        // 5. J2 control continuation: module tick wake schedule tampering
        let mut tampered_j2 = base.clone();
        tampered_j2
            .module_tick_schedule
            .insert("m.test.instance#1".into(), 77);
        let err = v2
            .verify_against_snapshot(&tampered_j2)
            .expect_err("J2 module_tick_schedule tamper must be rejected");
        assert!(format!("{err:?}").contains("control_continuation_root mismatch"));
    }

    #[test]
    fn unknown_schema_version_missing_v2_sidecar_roots_and_unknown_json_fields_fail_closed() {
        let snapshot = fixture_snapshot();
        let v2 = WorldCommitmentEnvelope::v2_from_snapshot(&snapshot).expect("v2 envelope");

        // Unknown schema_version (0, 3)
        for bad_version in [0u16, 3u16, 99u16] {
            let mut bad = v2.clone();
            bad.schema_version = bad_version;
            assert!(bad.validate_structure().is_err());
            assert!(bad.verify_against_snapshot(&snapshot).is_err());
        }

        // Missing governance_sidecar_root or control_continuation_root on v2 must NOT downgrade to v1
        let mut missing_gov = v2.clone();
        missing_gov.governance_sidecar_root = None;
        let err = missing_gov
            .verify_against_snapshot(&snapshot)
            .expect_err("missing governance_sidecar_root on v2 must fail closed");
        assert!(format!("{err:?}").contains("missing required governance_sidecar_root"));

        let mut missing_ctrl = v2.clone();
        missing_ctrl.control_continuation_root = None;
        let err = missing_ctrl
            .verify_against_snapshot(&snapshot)
            .expect_err("missing control_continuation_root on v2 must fail closed");
        assert!(format!("{err:?}").contains("missing required control_continuation_root"));

        // v1 with unexpected v2 sidecar root must fail closed
        let mut polluted_v1 =
            WorldCommitmentEnvelope::v1_from_snapshot(&snapshot).expect("v1 envelope");
        polluted_v1.governance_sidecar_root = v2.governance_sidecar_root;
        assert!(polluted_v1.validate_structure().is_err());

        // Unknown JSON field must fail closed (deny_unknown_fields)
        let mut raw_json: serde_json::Value =
            serde_json::from_str(&v2.to_json_pretty().unwrap()).unwrap();
        raw_json["unexpected_field"] = serde_json::json!("attack");
        assert!(
            WorldCommitmentEnvelope::from_json_str(&serde_json::to_string(&raw_json).unwrap())
                .is_err()
        );
    }

    #[test]
    fn iws_incremental_commitment_binding_v2_keeps_root_set_distinct_from_legacy_state_root_projection()
     {
        let snapshot = fixture_snapshot();
        let v2 = WorldCommitmentEnvelope::v2_from_snapshot(&snapshot).expect("v2 envelope");
        let gov_root = v2.governance_sidecar_root.unwrap();
        let ctrl_root = v2.control_continuation_root.unwrap();

        let root_set = RootSetV1::empty("iws-world-1".into(), [0x5a; 32]).expect("empty root set");
        let binding =
            IncrementalWorldCommitmentBindingV2::from_root_set(&root_set, gov_root, ctrl_root)
                .expect("iws binding");
        binding
            .verify_against_root_set(&root_set, &gov_root, &ctrl_root)
            .expect("verify iws binding");

        assert_ne!(binding.business_namespace_root, v2.business_state_root);
        let tampered_ctrl = [0xff; 32];
        assert!(
            binding
                .verify_against_root_set(&root_set, &gov_root, &tampered_ctrl)
                .is_err()
        );
    }
}
