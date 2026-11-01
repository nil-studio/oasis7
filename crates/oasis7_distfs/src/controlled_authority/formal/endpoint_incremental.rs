//! Page-backed verification for IWS candidates at the formal endpoint.
//!
//! This path verifies the signed original IWS material, its page-backed
//! transition/core and complete state-value closure. It then runs the current
//! WorldState owner coverage gate. Until all WorldState, Runtime and Cognition
//! owner registries are typed, it fails closed before a Prepare/Decision
//! receipt can be written.

use super::*;
use crate::incremental_page_store::{IncrementalPageStore, PageIoBudget, PageStoreError};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        incremental::IWS_AUTHORITY_PROFILE_VERSION, verify_formal_candidate,
    },
    controlled_authority::replicated_protocol::ClosedRecord,
    incremental_material::{
        CoreBudget, CoreRequest, OriginalObjectReader, WrittenValueBudget, verify_commit_core,
        verify_state_values, verify_written_values,
    },
    incremental_state::StateScanBudget,
    incremental_value::{ValueIoBudget, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;

/// Explicit total allowances for one page-backed IWS candidate verification.
/// State scan allowances are shared across generic closure and typed-owner
/// passes; value page/read allowances are shared across every pass.
#[derive(Debug, Clone)]
pub struct IwsEndpointVerificationBudgetsV1 {
    pub original_objects: u64,
    pub original_bytes: u64,
    pub new_pages: u64,
    pub core_merkle_reads: u64,
    pub state_scan_nodes: u64,
    pub state_scan_bytes: u64,
    pub state_scan_leaves: u64,
    pub full_value_occurrences: u64,
    pub full_value_bytes: u64,
    pub max_full_value_bytes: u64,
    pub transition_value_occurrences: u64,
    pub transition_value_bytes: u64,
    pub max_transition_value_bytes: u64,
    pub value_nodes: u64,
    pub value_read_bytes: u64,
    pub max_pending_node_bytes: usize,
    pub state_page_io: PageIoBudget,
    pub value_page_io: PageIoBudget,
}

/// Explicit endpoint capability: physical IWS pages, one shared pool and
/// finite per-candidate core/state/value allowances. It does not bind or
/// publish a RootSet; every candidate still supplies its signed roots.
pub struct IwsPageStoreCapabilityV1 {
    pages: IncrementalPageStore,
    budgets: IwsEndpointVerificationBudgetsV1,
}

impl IwsPageStoreCapabilityV1 {
    pub fn new(
        pages: IncrementalPageStore,
        budgets: IwsEndpointVerificationBudgetsV1,
    ) -> Result<Self, FormalError> {
        if budgets.max_pending_node_bytes == 0
            || budgets.max_pending_node_bytes
                > oasis7_formal_protocol::incremental_state::STATE_MAX_PAGE_BYTES
        {
            return Err(invalid("IWS endpoint pending node bound"));
        }
        Ok(Self { pages, budgets })
    }

    pub fn page_store_root(&self) -> &std::path::Path {
        self.pages.root()
    }
}

struct ClosedRecordOriginalReader<'a> {
    record: &'a ClosedRecord,
    memory: &'a MemoryBudget,
}

impl OriginalObjectReader for ClosedRecordOriginalReader<'_> {
    type Error = String;

    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }

    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }

    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        if !self.memory.same_pool(memory) {
            return Err("IWS original object pool mismatch".into());
        }
        let mut id_hex = [0u8; 64];
        hex::encode_to_slice(id, &mut id_hex)
            .map_err(|_| "IWS original object ID encoding".to_owned())?;
        let object = self
            .record
            .objects
            .iter()
            .find(|object| object.content_hash.as_bytes() == id_hex)
            .ok_or_else(|| "IWS original object not in closed record".to_owned())?;
        if object.bytes.len() > max_bytes || blake3::hash(&object.bytes).as_bytes() != id {
            return Err("IWS original object bound/hash".into());
        }
        let charge_bytes = object
            .bytes
            .len()
            .checked_add(128)
            .ok_or_else(|| "IWS original object allocation overflow".to_owned())?;
        let charge = memory
            .reserve(charge_bytes, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(object.bytes.len());
        bytes.extend_from_slice(&object.bytes);
        if bytes.capacity() > charge.bytes() {
            return Err("IWS original object allocation bound".into());
        }
        Ok(Arc::new(charge.retain(bytes)))
    }
}

impl FileFormalEndpoint {
    /// Attach a page store explicitly. Profile 1/2 endpoints cannot install it,
    /// and profile 3 receipt paths reject until it is present.
    pub fn install_iws_page_store(
        &mut self,
        capability: IwsPageStoreCapabilityV1,
    ) -> Result<(), FormalError> {
        if self.authority.definition().profile_version != IWS_AUTHORITY_PROFILE_VERSION
            || self.iws_page_store.is_some()
        {
            return Err(invalid("IWS endpoint page-store capability binding"));
        }
        self.iws_page_store = Some(capability);
        Ok(())
    }

    pub(super) fn verify_iws_candidate(
        &self,
        candidate: &FormalCandidateMaterialV1,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        if self.authority.definition().profile_version != IWS_AUTHORITY_PROFILE_VERSION {
            return Err(invalid("IWS authority profile required"));
        }
        if candidate.branch != FormalCompletionBranchV1::Normal
            || candidate.frozen.is_some()
            || candidate.frozen_material.is_some()
        {
            return Err(invalid(
                "IWS recovery/frozen material verification unsupported",
            ));
        }
        candidate
            .material
            .validate_iws()
            .map_err(|error| invalid(error.to_string()))?;
        let context = verify_formal_candidate(
            &candidate.candidate,
            None,
            &self.authority,
            self.parent.as_ref(),
            None,
            candidate.branch,
        )?;
        self.verify_iws_page_backed_material(candidate, &context, cancelled)?;
        Ok(context)
    }

    fn verify_iws_page_backed_material(
        &self,
        candidate: &FormalCandidateMaterialV1,
        context: &VerifiedCandidateContextV1,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("IWS endpoint page-store capability missing"))?;
        let pages = &capability.pages;
        let memory = pages.memory_budget();
        let limits = &capability.budgets;
        let mut objects = ClosedRecordOriginalReader {
            record: &candidate.material,
            memory,
        };
        let state = pages
            .state_session(limits.state_page_io.clone(), MemoryClass::Ordinary)
            .map_err(page_store_formal_error)?;
        let mut core_budget = CoreBudget {
            objects_remaining: limits.original_objects,
            original_bytes_remaining: limits.original_bytes,
            new_pages_remaining: limits.new_pages,
            merkle_reads_remaining: limits.core_merkle_reads,
        };
        let core_request = CoreRequest {
            candidate: context,
            original_authority: &self.authority,
            parent: self.parent.as_ref(),
            max_pending_node_bytes: limits.max_pending_node_bytes,
        };
        let core = verify_commit_core(
            &core_request,
            &mut objects,
            &state,
            memory,
            &mut core_budget,
            &mut *cancelled,
        )
        .map_err(|error| {
            state
                .take_read_error()
                .map(page_store_formal_error)
                .unwrap_or_else(|| invalid(format!("IWS structural core: {error}")))
        })?;
        if core.body().roots
            != core
                .roots()
                .digest()
                .map_err(|error| invalid(error.to_string()))?
        {
            return Err(invalid("IWS checked core root/body binding"));
        }

        let values = pages.value_session(limits.value_page_io.clone());
        let mut value_io = ValueIoBudget {
            nodes_remaining: limits.value_nodes,
            read_bytes_remaining: limits.value_read_bytes,
            write_bytes_remaining: 0,
        };
        let mut value_context = ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        };

        if core.transition().is_some() {
            let mut transition_budget = WrittenValueBudget {
                values_remaining: limits.transition_value_occurrences,
                logical_bytes_remaining: limits.transition_value_bytes,
                max_value_bytes: limits.max_transition_value_bytes,
            };
            verify_written_values(&core, &mut value_context, &mut transition_budget, || {
                cancelled()
            })
            .map_err(|error| invalid(format!("IWS final written values: {error}")))?;
            return Err(invalid(
                "IWS ordinary transition owner delta verifier unavailable",
            ));
        }

        let mut scan = StateScanBudget {
            nodes_remaining: limits.state_scan_nodes,
            read_bytes_remaining: limits.state_scan_bytes,
            leaves_remaining: limits.state_scan_leaves,
        };
        let mut full_values = WrittenValueBudget {
            values_remaining: limits.full_value_occurrences,
            logical_bytes_remaining: limits.full_value_bytes,
            max_value_bytes: limits.max_full_value_bytes,
        };
        let checked = verify_state_values(
            &core,
            &state,
            &mut value_context,
            &mut scan,
            &mut full_values,
            &mut *cancelled,
        )
        .map_err(|error| {
            state
                .take_read_error()
                .map(page_store_formal_error)
                .unwrap_or_else(|| invalid(format!("IWS state/value closure: {error}")))
        })?;
        // The owner pass consumes the remainder of the same scan allowance;
        // it cannot silently double the per-candidate state traversal budget.
        let coverage =
            oasis7_world_state::incremental_owner_closure::verify_world_state_owner_coverage(
                core.roots(),
                &state,
                &mut value_context,
                &mut scan,
                limits.max_full_value_bytes,
                cancelled,
            )
            .map_err(|error| {
                state
                    .take_read_error()
                    .map(page_store_formal_error)
                    .unwrap_or_else(|| invalid(format!("IWS typed owner coverage: {error}")))
            })?;
        if checked.body_id()
            != core
                .body()
                .digest()
                .map_err(|error| invalid(error.to_string()))?
            || coverage.root_set_digest() != core.body().roots
            || coverage.checked_u64_fields() != 7
            || coverage.checked_id_set_fields() != 5
            || coverage.checked_number_map_fields() != 11
            || coverage.checked_text_byte_fields() != 4
            || coverage.checked_governance_fields() != 3
            || coverage.checked_factory_authority_fields() != 2
            || coverage.checked_main_token_policy_fields() != 5
            || coverage.checked_material_fields() != 2
            || coverage.checked_logistics_fields() != 2
            || coverage.checked_fixed_economy_fields() != 4
            || coverage.checked_agent_authority_fields() != 3
            || coverage.checked_reward_fields() != 2
            || !coverage.unsupported_fields().is_empty()
            || coverage.runtime_registry_missing()
            || coverage.cognition_registry_missing()
        {
            return Err(invalid("IWS typed owner coverage incomplete"));
        }
        Ok(())
    }
}

fn page_store_formal_error(error: PageStoreError) -> FormalError {
    FormalError::Io(format!("IWS page store: {error}"))
}
