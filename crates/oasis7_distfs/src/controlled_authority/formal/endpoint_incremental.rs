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
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
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
    #[cfg(not(target_arch = "wasm32"))]
    current_limits: Option<current::IwsCurrentLimitsV1>,
    #[cfg(not(target_arch = "wasm32"))]
    source_work: std::cell::RefCell<Option<work::IwsCandidateSourceWorkV1>>,
    #[cfg(not(target_arch = "wasm32"))]
    jobs: jobs::EndpointIwsState,
    #[cfg(not(target_arch = "wasm32"))]
    state_memo: Option<(usize, usize)>,
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
            #[cfg(not(target_arch = "wasm32"))]
            current_limits: None,
            #[cfg(not(target_arch = "wasm32"))]
            source_work: std::cell::RefCell::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            jobs: jobs::EndpointIwsState::new(),
            #[cfg(not(target_arch = "wasm32"))]
            state_memo: None,
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(capability) = &self.iws_page_store {
            if capability.jobs.matches(candidate, cancelled)? {
                if cancelled() {
                    return Err(invalid("IWS candidate reuse cancelled"));
                }
                return Ok(context);
            }
        }
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
        closure::consume_candidate_data_closure_v1(self, candidate, context, cancelled)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn page_store_formal_error(error: PageStoreError) -> FormalError {
    FormalError::Io(format!("IWS page store: {error}"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "endpoint_incremental_stream_tests.rs"]
mod stream_tests;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_current.rs"]
mod current;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_scratch.rs"]
mod scratch;

#[cfg(not(target_arch = "wasm32"))]
pub use current::IwsCurrentLimitsV1;
#[cfg(not(target_arch = "wasm32"))]
impl IwsPageStoreCapabilityV1 {
    /// Explicitly enable complete Current checks; receipts still require the
    /// independent History/source and formal lifecycle closure.
    pub fn with_current_limits(mut self, limits: IwsCurrentLimitsV1) -> Result<Self, FormalError> {
        limits
            .validate()
            .map_err(|e| invalid(format!("IWS Current limits: {e}")))?;
        self.current_limits = Some(limits);
        Ok(self)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_provisional.rs"]
mod provisional;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_mailbox.rs"]
mod mailbox;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_persist.rs"]
mod persist;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_history_originals.rs"]
mod history_originals;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_sources.rs"]
mod sources;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_jobs.rs"]
mod jobs;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_work.rs"]
mod work;
#[cfg(not(target_arch = "wasm32"))]
pub use work::IwsCandidateSourceWorkV1;
#[cfg(not(target_arch = "wasm32"))]
impl FileFormalEndpoint {
    pub fn supply_iws_candidate_work(
        &mut self,
        work: IwsCandidateSourceWorkV1,
    ) -> Result<(), FormalError> {
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("IWS page capability missing"))?;
        let mut slot = capability
            .source_work
            .try_borrow_mut()
            .map_err(|_| invalid("IWS work slot borrowed"))?;
        if slot.is_some() {
            return Err(invalid("IWS work already supplied"));
        }
        *slot = Some(work);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl FileFormalEndpoint {
    pub(super) fn iws_local_data_reference(
        &self,
        commit: &VerifiedFormalCommitV1,
    ) -> Result<Option<String>, FormalError> {
        self.iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("IWS local closure capability missing"))?
            .jobs
            .local_data_reference(commit)
    }
    pub(super) fn check_iws_qualified_install(
        &self,
        commit: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("IWS accepted capability missing"))?;
        if !capability.jobs.ready(commit.verified_record_id())? {
            return Err(invalid(
                "IWS complete prepared candidate required before Qualified",
            ));
        }
        // A staged source proof is not the still-missing execution/native and
        // durable closure permit, even when a genuine external quorum signed.
        Err(invalid("IWS execution and durable closure incomplete"))
    }
    pub(super) fn finish_iws_qualified_install(
        &self,
        commit: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        self.iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("IWS accepted capability missing"))?
            .jobs
            .accepted(commit)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_parent.rs"]
mod parent_binding;

#[cfg(not(target_arch = "wasm32"))]
impl IwsPageStoreCapabilityV1 {
    /// Optional, explicitly admitted Ordinary-only memo. No quotas grow.
    /// Cold recovery must choose a separate Recovery memo configuration.
    pub fn with_state_read_memo(
        mut self,
        max_entries: usize,
        max_retained_bytes: usize,
    ) -> Result<Self, FormalError> {
        if max_entries == 0
            || max_retained_bytes == 0
            || max_entries == usize::MAX
            || max_retained_bytes == usize::MAX
        {
            return Err(invalid("IWS finite positive memo allowances"));
        }
        self.state_memo = Some((max_entries, max_retained_bytes));
        Ok(self)
    }
}

#[path = "endpoint_incremental_closure.rs"]
mod closure;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold.rs"]
mod cold;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold_cognition.rs"]
mod cold_cognition;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold_mailbox.rs"]
mod cold_mailbox;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold_restore.rs"]
mod cold_restore;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold_roots.rs"]
mod cold_roots;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold_runtime.rs"]
mod cold_runtime;
#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_mailbox_history.rs"]
mod mailbox_history;
#[cfg(not(target_arch = "wasm32"))]
pub use cold_restore::{IwsColdParentHintsV1, IwsColdParentWorkV1};

#[path = "endpoint_incremental_local_closure.rs"]
pub(super) mod local_closure;
