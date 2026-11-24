//! Canonical portable formal-protocol owner for bounded original provider material identities.
//!
//! These are store-local recovery and protected-root contracts, not World
//! acceptance or a global GC registry. Legacy simulator, WorldService and Viewer consumers
//! must share these exact nominal types rather than duplicating DTOs.
use std::any::Any;
use std::fmt;
use std::sync::Arc;

use crate::incremental_state::StateRootV1;
use oasis7_proto::memory_budget::ReservedValue;

/// Immutable new-format owner scope for a provider pending ledger.
///
/// The native owner creates this only after it has verified the world,
/// genesis, exact parent, authority snapshot, and separate pending-protection
/// root. It is deliberately not derivable from the legacy lineage checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderMaterialOwnerBindingV1 {
    pub world_id: String,
    pub genesis_root: [u8; 32],
    /// Stable identifier for this independently owned pending store.
    pub owner_id: [u8; 32],
}

impl ProviderMaterialOwnerBindingV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.world_id.trim().is_empty() || self.world_id.len() > 256 {
            return Err("provider pending owner world id is empty or too long");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderMaterialAttemptV1 {
    pub owner: ProviderMaterialOwnerBindingV1,
    /// Existing canonical cognition invocation key (`blake3:<hex>`).
    pub operation_id: String,
    pub transport_attempt: u64,
    pub attempt_id: [u8; 32],
    /// Exact authenticated parent and authority for this invocation. These
    /// belong to the attempt, not the long-lived owner scope.
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
    pub request_object_id: [u8; 32],
    pub request_context_object_id: [u8; 32],
}

/// References returned only after both the original response body and its
/// typed semantic context have been persisted by the installed owner.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderOriginalMaterialRefsV1 {
    pub owner: ProviderMaterialOwnerBindingV1,
    /// Existing canonical cognition invocation key (`blake3:<hex>`).
    pub operation_id: String,
    pub transport_attempt: u64,
    pub attempt_id: [u8; 32],
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
    pub request_object_id: [u8; 32],
    pub request_context_object_id: [u8; 32],
    pub response_object_id: [u8; 32],
    pub response_context_object_id: [u8; 32],
    pub response_status: u16,
}

/// Per-invocation binding returned by the already verified new-format Runtime
/// owner. `RuntimeBindingV1::base_world_hash` is not an exact RootSet and must
/// not be substituted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderAttemptAuthorityBindingV1 {
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
}

/// Evidence must be checked by the installed trusted authority before an
/// unresolved provider attempt can leave the protected set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderDispositionEvidenceV1 {
    pub evidence_object_id: [u8; 32],
    pub disposition_digest: [u8; 32],
}

/// Opaque continuation for a single protected-roots snapshot. Callers can
/// retain and return a cursor produced by the owner, but cannot choose an
/// arbitrary offset that skips unresolved attempts. Owner implementations must
/// downcast to their private cursor type and bind it to the exact root snapshot.
pub struct ProviderPendingRootsCursorV1 {
    state: Box<dyn Any + Send + Sync>,
}

impl ProviderPendingRootsCursorV1 {
    /// Wrap a private owner cursor. The concrete type should remain private to
    /// the owner crate; other implementations cannot fabricate that type.
    #[doc(hidden)]
    pub fn from_owner_state<T: Any + Send + Sync>(state: T) -> Self {
        Self {
            state: Box::new(state),
        }
    }

    /// Access a cursor only when it has the concrete type owned by the caller.
    #[doc(hidden)]
    pub fn owner_state<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.state.downcast_ref::<T>()
    }
}

impl fmt::Debug for ProviderPendingRootsCursorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderPendingRootsCursorV1")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPendingAttemptPhaseV1 {
    RequestPrepared,
    DispatchedOrUnknown,
    ResponseBodyPlanned,
    ResponseBodyDurable,
    ResponseContextPlanned,
    ResponseCaptured,
    RejectionEvidencePlanned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPendingAttemptSummaryV1 {
    pub attempt: ProviderMaterialAttemptV1,
    pub phase: ProviderPendingAttemptPhaseV1,
    pub response_object_id: Option<[u8; 32]>,
    pub response_context_object_id: Option<[u8; 32]>,
    pub response_status: Option<u16>,
    pub terminal_evidence_object_id: Option<[u8; 32]>,
}

/// Actual roots needed to retain the independent owner's event and index pages.
/// `pending_root` in the page is a cursor binding digest, not a substitute for
/// these Merkle root descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPendingStateRootsV1 {
    pub events: StateRootV1,
    pub pending: StateRootV1,
    pub seen: StateRootV1,
}

#[derive(Debug)]
pub struct ProviderPendingRootsPageV1 {
    pub owner: Arc<ReservedValue<ProviderMaterialOwnerBindingV1>>,
    pub pending_root: [u8; 32],
    /// Charged page of event-record and original-material content IDs. The
    /// cursor bounds this output; callers retain the carrier while using the
    /// roots.
    pub object_ids: Arc<ReservedValue<Vec<[u8; 32]>>>,
    /// Charged unresolved identities and current phase, for recovery to resume
    /// only by same-identity inspection. This is not retry authorization.
    pub attempts: Arc<ReservedValue<Vec<ProviderPendingAttemptSummaryV1>>>,
    /// Charged actual Merkle roots backing the independent pending ledger.
    pub state_roots: Arc<ReservedValue<ProviderPendingStateRootsV1>>,
    /// A continuation exists only when this page has not reached the end of
    /// the bound snapshot. Exhausting these pages proves this store-local root
    /// enumeration is complete; it does not register the owner with a global
    /// World or GC registry.
    pub next_cursor: Option<Arc<ReservedValue<ProviderPendingRootsCursorV1>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_identity_fails_closed_for_empty_or_unbounded_world_id() {
        let mut owner = ProviderMaterialOwnerBindingV1 {
            world_id: "world-alpha".to_owned(),
            genesis_root: [0x11; 32],
            owner_id: [0x22; 32],
        };
        assert!(owner.validate().is_ok());
        owner.world_id = " ".to_owned();
        assert!(owner.validate().is_err());
        owner.world_id = "x".repeat(257);
        assert!(owner.validate().is_err());
    }

    #[test]
    fn opaque_pending_cursor_preserves_private_owner_type() {
        struct OwnerCursor(u64);
        struct ForeignCursor;
        let cursor = ProviderPendingRootsCursorV1::from_owner_state(OwnerCursor(7));
        assert_eq!(cursor.owner_state::<OwnerCursor>().map(|c| c.0), Some(7));
        assert!(cursor.owner_state::<ForeignCursor>().is_none());
    }

    #[test]
    fn material_attempt_retains_exact_parent_and_authority_binding() {
        let owner = ProviderMaterialOwnerBindingV1 {
            world_id: "world-alpha".to_owned(),
            genesis_root: [1; 32],
            owner_id: [2; 32],
        };
        let attempt = ProviderMaterialAttemptV1 {
            owner: owner.clone(),
            operation_id: "blake3:operation".to_owned(),
            transport_attempt: 3,
            attempt_id: [4; 32],
            parent_root_set: [5; 32],
            authority_snapshot: [6; 32],
            request_object_id: [7; 32],
            request_context_object_id: [8; 32],
        };
        let bytes = serde_json::to_vec(&attempt).expect("serialize canonical attempt");
        let decoded: ProviderMaterialAttemptV1 =
            serde_json::from_slice(&bytes).expect("decode canonical attempt");
        assert_eq!(decoded, attempt);
        assert_eq!(decoded.owner, owner);
        assert_eq!(decoded.parent_root_set, [5; 32]);
        assert_eq!(decoded.authority_snapshot, [6; 32]);
    }
}
