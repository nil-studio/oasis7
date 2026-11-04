//! Page-backed verification for IWS candidates at the formal endpoint.
//!
//! This path verifies the signed original IWS material, its page-backed
//! transition/core and complete state-value closure. It then runs the current
//! WorldState owner coverage gate. Until all WorldState, Runtime and Cognition
//! owner registries are typed, it fails closed before a Prepare/Decision
//! receipt can be written.

use super::*;
#[cfg(not(target_arch = "wasm32"))]
use crate::incremental_page_store::PageStoreError;
use crate::incremental_page_store::{IncrementalPageStore, PageIoBudget};
use oasis7_formal_protocol::controlled_authority::formal::{
    incremental::IWS_AUTHORITY_PROFILE_VERSION, verify_formal_candidate,
};
#[cfg(not(target_arch = "wasm32"))]
use oasis7_formal_protocol::{
    controlled_authority::replicated_protocol::ClosedRecord,
    incremental_material::{
        CoreBudget, CoreRequest, OriginalObjectReader, OriginalObjectStreamReader,
        WrittenValueBudget, verify_commit_core, verify_state_values, verify_written_values,
    },
    incremental_state::StateScanBudget,
    incremental_value::{ValueIoBudget, ValueReadContext},
};
#[cfg(not(target_arch = "wasm32"))]
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[cfg(not(target_arch = "wasm32"))]
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
    /// Independent physical/logical quotas for already retained original
    /// objects. Candidate-byte reads continue borrowing the ClosedRecord.
    pub original_page_io: PageIoBudget,
    pub original_value_nodes: u64,
    pub original_value_read_bytes: u64,
}

/// Explicit endpoint capability: physical IWS pages, one shared pool and
/// finite per-candidate core/state/value allowances. It does not bind or
/// publish a RootSet; every candidate still supplies its signed roots.
/// Wasm endpoints reject IWS owner verification even with this capability.
pub struct IwsPageStoreCapabilityV1 {
    pages: IncrementalPageStore,
    #[cfg(not(target_arch = "wasm32"))]
    budgets: IwsEndpointVerificationBudgetsV1,
}

impl IwsPageStoreCapabilityV1 {
    pub fn new(
        pages: IncrementalPageStore,
        budgets: IwsEndpointVerificationBudgetsV1,
    ) -> Result<Self, FormalError> {
        // This bounds an entire Merkle update, not one encoded page. Every
        // workspace still requires admission from the store's finite pool.
        if budgets.max_pending_node_bytes == 0 {
            return Err(invalid("IWS endpoint pending node bound"));
        }
        Ok(Self {
            pages,
            #[cfg(not(target_arch = "wasm32"))]
            budgets,
        })
    }

    pub fn page_store_root(&self) -> &std::path::Path {
        self.pages.root()
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ClosedRecordOriginalReader<'a> {
    record: &'a ClosedRecord,
    memory: &'a MemoryBudget,
}

#[cfg(not(target_arch = "wasm32"))]
struct EndpointOriginalReader<'a, 'record, O> {
    candidate: ClosedRecordOriginalReader<'record>,
    retained: &'a mut O,
}
#[cfg(not(target_arch = "wasm32"))]
impl<O> EndpointOriginalReader<'_, '_, O> {
    fn candidate_contains(&self, id: &[u8; 32]) -> bool {
        let mut encoded = [0u8; 64];
        // Fixed-size input/output; this cannot fail and does not allocate.
        if hex::encode_to_slice(id, &mut encoded).is_err() {
            return false;
        }
        self.candidate
            .record
            .objects
            .iter()
            .any(|object| object.content_hash.as_bytes() == encoded)
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl<O: OriginalObjectReader> OriginalObjectReader for EndpointOriginalReader<'_, '_, O>
where
    O::Error: std::fmt::Display,
{
    type Error = String;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.candidate.memory
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
        if self.candidate_contains(id) {
            self.candidate.read_original(id, max_bytes, memory)
        } else {
            self.retained
                .read_original(id, max_bytes, memory)
                .map_err(|error| error.to_string())
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl<O: OriginalObjectStreamReader> OriginalObjectStreamReader for EndpointOriginalReader<'_, '_, O>
where
    O::Error: std::fmt::Display,
{
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn std::io::Write,
    ) -> Result<u64, Self::Error> {
        if self.candidate_contains(id) {
            self.candidate
                .stream_original(id, max_bytes, memory, output)
        } else {
            self.retained
                .stream_original(id, max_bytes, memory, output)
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
impl OriginalObjectStreamReader for ClosedRecordOriginalReader<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn std::io::Write,
    ) -> Result<u64, Self::Error> {
        if !self.memory.same_pool(memory) {
            return Err("IWS original stream pool mismatch".into());
        }
        let mut id_hex = [0u8; 64];
        hex::encode_to_slice(id, &mut id_hex)
            .map_err(|_| "IWS original stream ID encoding".to_owned())?;
        let object = self
            .record
            .objects
            .iter()
            .find(|object| object.content_hash.as_bytes() == id_hex)
            .ok_or_else(|| "IWS original stream not in closed record".to_owned())?;
        let len = u64::try_from(object.bytes.len())
            .map_err(|_| "IWS original stream length overflow".to_owned())?;
        if len > max_bytes {
            return Err("IWS original stream byte bound".into());
        }
        // The retained ClosedRecord owns these bytes. Borrow bounded slices;
        // never duplicate a large original just to hash or observe its facts.
        let _phase = memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(|_| "IWS original stream workspace admission".to_owned())?;
        let mut hash = blake3::Hasher::new();
        for chunk in object
            .bytes
            .chunks(oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES)
        {
            // write_all honors partial writes, Interrupted and WriteZero. Any
            // failure stops this provisional stream and cannot return success.
            output
                .write_all(chunk)
                .map_err(|_| "IWS original stream output stopped".to_owned())?;
            hash.update(chunk);
        }
        if hash.finalize().as_bytes() != id {
            return Err("IWS original stream checksum".into());
        }
        Ok(len)
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
        if candidate.branch != FormalCompletionBranchV1::Normal {
            return Err(invalid("IWS recovery material verification unsupported"));
        }
        if candidate.frozen.is_some() != candidate.frozen_material.is_some() {
            return Err(invalid("IWS frozen certificate/material presence"));
        }
        candidate
            .material
            .validate_iws()
            .map_err(|error| invalid(error.to_string()))?;
        let context = verify_formal_candidate(
            &candidate.candidate,
            candidate.frozen.as_ref(),
            &self.authority,
            self.parent.as_ref(),
            None,
            candidate.branch,
        )?;
        self.verify_iws_page_backed_material(candidate, &context, cancelled)?;
        Ok(context)
    }

    #[cfg(target_arch = "wasm32")]
    fn verify_iws_page_backed_material(
        &self,
        _candidate: &FormalCandidateMaterialV1,
        _context: &VerifiedCandidateContextV1,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), FormalError> {
        if cancelled() {
            return Err(invalid("IWS owner verification cancelled"));
        }
        // The native owner registry is unavailable on this target. Never
        // treat omitted owner verification as receipt-producing evidence.
        Err(invalid("IWS typed owner coverage unsupported on wasm32"))
    }

    #[cfg(not(target_arch = "wasm32"))]
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
        let mut original_pages = limits.original_page_io.clone();
        let physical_cancelled = std::sync::atomic::AtomicBool::new(false);
        // This API supplies cooperative polling, not an externally shared
        // AtomicBool. Latch every caller observation into the physical reader
        // signal as well. Streaming observers poll between bounded chunks;
        // core owned reads retain their explicit per-original byte bound.
        let mut poll_cancelled = || {
            if physical_cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return true;
            }
            let stop = cancelled();
            if stop {
                physical_cancelled.store(true, std::sync::atomic::Ordering::Release);
            }
            stop
        };
        let cancelled = &mut poll_cancelled;
        if cancelled() {
            return Err(invalid("IWS original capability open cancelled"));
        }
        let original_store = crate::incremental_page_store::objects::IncrementalObjectStore::open(
            pages,
            MemoryClass::Ordinary,
            &mut original_pages,
            &physical_cancelled,
        )
        .map_err(page_store_formal_error)?;
        let mut original_values = ValueIoBudget {
            nodes_remaining: limits.original_value_nodes,
            read_bytes_remaining: limits.original_value_read_bytes,
            write_bytes_remaining: 0,
        };
        let mut original_io = crate::incremental_page_store::objects::ObjectIo {
            physical: &mut original_pages,
            logical: &mut original_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical_cancelled,
        };
        let mut retained = original_store.original_reader(&mut original_io);
        let mut objects = EndpointOriginalReader {
            candidate: ClosedRecordOriginalReader {
                record: &candidate.material,
                memory,
            },
            retained: &mut retained,
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
            oasis7_world_state::incremental_owner_closure::verify_world_state_owner_closure_v1(
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
        let report = coverage.report();
        if checked.body_id()
            != core
                .body()
                .digest()
                .map_err(|error| invalid(error.to_string()))?
            || coverage.root_set_digest() != core.body().roots
            || report.checked_u64_fields() != 7
            || report.checked_id_set_fields() != 5
            || report.checked_number_map_fields() != 11
            || report.checked_text_byte_fields() != 4
            || report.checked_governance_fields() != 3
            || report.checked_factory_authority_fields() != 2
            || report.checked_main_token_policy_fields() != 5
            || report.checked_material_fields() != 2
            || report.checked_logistics_fields() != 2
            || report.checked_fixed_economy_fields() != 4
            || report.checked_agent_authority_fields() != 3
            || report.checked_reward_fields() != 2
            || report.checked_organization_fields() != 2
            || report.checked_supply_fields() != 2
            || report.checked_diplomacy_fields() != 4
            || !report.unsupported_fields().is_empty()
            || report.runtime_registry_missing()
            || report.cognition_registry_missing()
        {
            return Err(invalid("IWS typed owner coverage incomplete"));
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn page_store_formal_error(error: PageStoreError) -> FormalError {
    FormalError::Io(format!("IWS page store: {error}"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "endpoint_incremental_stream_tests.rs"]
mod stream_tests;
