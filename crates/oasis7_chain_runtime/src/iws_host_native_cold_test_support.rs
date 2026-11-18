//! Native integration support for a genuine reopen after every live Host,
//! World, Node, projected owner and old page handle has been dropped.
//! Paths and selected fact locations remain untrusted DATA. The production
//! cold consumer revalidates the durable checkpoint, originals and all sources.
use super::*;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::{
        FormalEndpointAuthorizationV1, IwsCandidateSourceWorkV1, IwsColdParentHintsV1,
        IwsColdParentWorkV1, IwsCurrentLimitsV1, IwsPageStoreCapabilityV1,
    },
    incremental_page_store::{IncrementalPageStore, PageCompression, PageStoreError},
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            VerifiedAuthorityV1,
            bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
        },
        replicated_protocol::EndpointRole,
    },
    incremental_execution::TransactionLimits,
    incremental_material::FactLocationV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::{cbor_decode_preflight::CborPreflightWorkV1, memory_budget::MemoryReservation};
use oasis7_runtime::{
    AuthorizationRootCacheErrorV1, AuthorizationRootCacheLimitsV1, BodyMutationError,
    NormalHostWorkV1, ProvisionalAuthorizationProjectionV1, ReleaseSecurityPolicy,
    RuntimeSourceCompiler, WorldRuntimeMemoryLimits,
};
use oasis7_world_state::{
    incremental_cognition_history::CognitionHistoryIndexWorkV1,
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_json_records::JsonReadWorkV1,
    incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_result_router::WorldResultRouterWorkV1,
};
use std::{io::Read, path::Path, sync::Arc};

// Reuse the established explicit finite integration allowances verbatim. These
// factories produce local ceilings, never a copy of Common's remaining bank.
#[path = "../tests/support/genesis_endpoint_work.rs"]
mod endpoint_work;

#[derive(Debug)]
pub(crate) enum ColdFixtureErrorV1 {
    Formal(FormalError),
    Physical(PageStoreError),
    Value(ValueTreeError),
    Body(BodyMutationError),
    Projection(AuthorizationRootCacheErrorV1),
    Io(std::io::Error),
    Host(HostColdStartupErrorV1),
}
impl std::fmt::Display for ColdFixtureErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Formal(error) => write!(f, "cold fixture Formal: {error}"),
            Self::Physical(error) => write!(f, "cold fixture pages: {error}"),
            Self::Value(error) => write!(f, "cold fixture value: {error}"),
            Self::Body(error) => write!(f, "cold fixture Runtime work: {error:?}"),
            Self::Projection(error) => write!(f, "cold fixture projection hint: {error}"),
            Self::Io(error) => write!(f, "cold fixture hint IO: {error}"),
            Self::Host(error) => write!(f, "cold fixture strict Host startup: {error}"),
        }
    }
}
impl std::error::Error for ColdFixtureErrorV1 {}

/// Original finite accounts for one cold startup, retained on both success and
/// error. A retry must borrow this same value; reopening never calls its maker.
pub(crate) struct ColdFixtureWorkV1 {
    pub cold: IwsColdParentWorkV1,
    pub source: IwsCandidateSourceWorkV1,
    pub physical: HostColdIoWorkV1,
    pub runtime: NormalHostWorkV1,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}

fn structure() -> CborPreflightWorkV1 {
    CborPreflightWorkV1 {
        bytes_remaining: 512 << 20,
        values_remaining: 1_000_000,
        max_depth: 64,
    }
}
fn cognition_cbor() -> CognitionHistoryCborWorkV1 {
    CognitionHistoryCborWorkV1 {
        bytes_remaining: 512 << 20,
        nodes_remaining: 1_000_000,
        source_reads_remaining: 1_000_000,
        key_bytes_remaining: 64 << 20,
        max_depth: 64,
        max_key_bytes: 4096,
    }
}
fn json() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 32 << 20,
    }
}

impl ColdFixtureWorkV1 {
    pub(crate) fn new_bounded(
        limits: &IwsCurrentLimitsV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, ColdFixtureErrorV1> {
        common
            .charge(1, size_of::<Self>() as u64)
            .map_err(ColdFixtureErrorV1::Formal)?;
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(|error| ColdFixtureErrorV1::Value(error.into()))?;
        let runtime =
            NormalHostWorkV1::new_bounded(oasis7_runtime::two_agent_host_work_limits_v1(), common)
                .map_err(ColdFixtureErrorV1::Body)?;
        let cold = IwsColdParentWorkV1 {
            objects_physical: endpoint_work::physical(),
            objects_values: endpoint_work::logical(),
            state_recovery: endpoint_work::physical(),
            values_recovery: endpoint_work::physical(),
            read_recovery: endpoint_work::logical(),
            keys_recovery: SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            },
            state_ordinary: endpoint_work::physical(),
            values_ordinary: endpoint_work::physical(),
            read_ordinary: endpoint_work::logical(),
            keys_ordinary: SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            },
            recovery_memo: None,
            core: endpoint_work::core_budget(),
            facts: endpoint_work::facts(),
            router: WorldResultRouterWorkV1 {
                facts: endpoint_work::facts(),
                envelope: structure(),
                runtime: RuntimeHistoryDecodeWorkV1 {
                    structure: structure(),
                    canonical: endpoint_work::canonical_work(),
                    max_original_bytes: 16 << 20,
                    max_heap_bytes: 32 << 20,
                },
                cognition: cognition_cbor(),
            },
            proof_ledger_read: json(),
            // The strict paid cold path reads the ledger on the caller's actual
            // Common, outside its scalar child. This legacy-only field has no
            // spendable bank; accidentally selecting the old path fails closed.
            proof_ledger_readmission: FormalArchiveReadWorkV1::new(0, 0)
                .map_err(ColdFixtureErrorV1::Formal)?,
            current: limits.current.work().map_err(ColdFixtureErrorV1::Value)?,
            admission: limits.current.work().map_err(ColdFixtureErrorV1::Value)?,
            runtime_index: endpoint_work::work(),
            cognition_index: CognitionHistoryIndexWorkV1 {
                cells_remaining: 1_000_000,
                max_pending_node_bytes: 16 << 20,
            },
            hashes: CognitionHeadHashWorkV1 {
                input_bytes_remaining: 512 << 20,
                hash_nodes_remaining: 1_000_000,
            },
            mailbox_decode: WorldEventDecodeWorkV1 {
                structure: structure(),
                canonical: endpoint_work::canonical_work(),
                max_original_bytes: limits.decode_max_original_bytes,
                max_heap_bytes: limits.decode_max_heap_bytes,
            },
            mailbox_occurrences: limits.current.mailbox_occurrences_remaining,
            scratch_pages: limits.scratch_pages,
            semantic_bytes: limits.current.max_semantic_bytes as u64,
            record_bytes: limits.current.max_record_bytes as u64,
            local_records_remaining: limits.current.records_remaining,
            local_original_bytes_remaining: limits.current.original_bytes_remaining,
        };
        Ok(Self {
            cold,
            source: endpoint_work::source_work(),
            physical: HostColdIoWorkV1 {
                process_canonical: endpoint_work::canonical_work(),
                state: endpoint_work::physical(),
                values: endpoint_work::physical(),
                members: endpoint_work::physical(),
                originals: endpoint_work::physical(),
                original_values: endpoint_work::logical(),
            },
            runtime,
            core: common.work().identity(),
            fee,
        })
    }
    fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ColdFixtureErrorV1> {
        common
            .consume_steps(1)
            .map_err(ColdFixtureErrorV1::Formal)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(ColdFixtureErrorV1::Formal(FormalError::Invalid(
                "cold fixture work Core/pool".into(),
            )));
        }
        Ok(())
    }
}

pub(crate) enum ColdFixtureProcessV1<'a> {
    Governed(&'a ReleaseSecurityPolicy),
    #[cfg(feature = "node-libp2p")]
    DevLocal(&'a crate::execution_config::DevLocalParticipantConfig),
}

/// Borrow only raw paths/configuration and original external trust. No prior
/// accepted source, resident, page session, projected cache or World is input.
pub(crate) struct ColdFixtureInputV1<'a> {
    pub endpoint_path: &'a Path,
    pub pages_path: &'a Path,
    pub authorization_hint_path: &'a Path,
    pub authority: &'a VerifiedAuthorityV1,
    pub role: EndpointRole,
    pub provider: &'a Arc<RemoteMonotonicEndpointProvider>,
    pub pinned: &'a TrustedFencingPolicyV1,
    pub process: ColdFixtureProcessV1<'a>,
    pub configuration: WorldRuntimeMemoryLimits,
    pub worker: WorkerLimitsV1,
    pub transaction: TransactionLimits,
    pub current_limits: IwsCurrentLimitsV1,
    pub projection_limits: AuthorizationRootCacheLimitsV1,
    pub facts: IwsColdParentHintsV1,
}

fn read_projection_hint(
    path: &Path,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ProvisionalAuthorizationProjectionV1, ColdFixtureErrorV1> {
    common
        .consume_steps(2)
        .map_err(ColdFixtureErrorV1::Formal)?;
    let mut file = std::fs::File::open(path).map_err(ColdFixtureErrorV1::Io)?;
    let metadata = file.metadata().map_err(ColdFixtureErrorV1::Io)?;
    let length = usize::try_from(metadata.len())
        .ok()
        .filter(|length| *length <= limits.max_wire_bytes)
        .ok_or(ColdFixtureErrorV1::Projection(
            AuthorizationRootCacheErrorV1::Invalid("cold hint file bound"),
        ))?;
    if !metadata.is_file() {
        return Err(ColdFixtureErrorV1::Projection(
            AuthorizationRootCacheErrorV1::Invalid("cold hint must be a regular file"),
        ));
    }
    let layout = length
        .checked_add(size_of::<Vec<u8>>())
        .ok_or(ColdFixtureErrorV1::Projection(
            AuthorizationRootCacheErrorV1::Invalid("cold hint layout"),
        ))?;
    common
        .charge(1, layout as u64)
        .map_err(ColdFixtureErrorV1::Formal)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|error| ColdFixtureErrorV1::Value(error.into()))?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(length).map_err(|_| {
        ColdFixtureErrorV1::Projection(AuthorizationRootCacheErrorV1::Invalid(
            "cold hint allocation",
        ))
    })?;
    if raw.capacity() > length {
        return Err(ColdFixtureErrorV1::Projection(
            AuthorizationRootCacheErrorV1::Invalid("cold hint capacity"),
        ));
    }
    raw.resize(length, 0);
    for chunk in raw.chunks_mut(8192) {
        common
            .charge(1, chunk.len() as u64)
            .map_err(ColdFixtureErrorV1::Formal)?;
        file.read_exact(chunk).map_err(ColdFixtureErrorV1::Io)?;
    }
    common.charge(1, 1).map_err(ColdFixtureErrorV1::Formal)?;
    if file.read(&mut [0u8; 1]).map_err(ColdFixtureErrorV1::Io)? != 0 {
        return Err(ColdFixtureErrorV1::Projection(
            AuthorizationRootCacheErrorV1::Invalid("cold hint changed length"),
        ));
    }
    ProvisionalAuthorizationProjectionV1::read_hint_data(
        &fee.retain(raw),
        limits,
        common,
        MemoryClass::Ordinary,
    )
    .map_err(ColdFixtureErrorV1::Projection)
}

/// Exactly one strict cold attempt. All work is borrowed and remains visible
/// to the caller after refusal. No cached source or same-process reopen path is
/// available, and missing/incorrect fact hints are actual verification errors.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reopen_host_bounded<L>(
    input: ColdFixtureInputV1<'_>,
    work: &mut ColdFixtureWorkV1,
    mailbox_hint: L,
    compiler: &RuntimeSourceCompiler,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<IwsHostRuntimeV1, ColdFixtureErrorV1>
where
    L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
{
    let projection = {
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut common = archive.bounded_context(memory, &mut stop);
        work.validate_context(&mut common)?;
        read_projection_hint(
            input.authorization_hint_path,
            input.projection_limits,
            &mut common,
        )?
    };
    // Opening these actual paths requires the old fixed namespace lease to be
    // gone. No new directory, snapshot restore or caller parent is supplied.
    let pages = IncrementalPageStore::open(
        input.pages_path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .map_err(ColdFixtureErrorV1::Physical)?;
    let mut endpoint = FileFormalEndpoint::open_protected(
        input.endpoint_path,
        FormalEndpointAuthorizationV1 {
            authority: input.authority.clone(),
            original: input.authority.clone(),
            recovery: None,
            parent: None,
        },
        input.role,
        input.provider.clone(),
        None,
    )
    .map_err(ColdFixtureErrorV1::Formal)?;
    endpoint
        .install_iws_page_store(
            IwsPageStoreCapabilityV1::new(pages, endpoint_work::budgets())
                .and_then(|capability| capability.with_current_limits(input.current_limits))
                .map_err(ColdFixtureErrorV1::Formal)?,
        )
        .map_err(ColdFixtureErrorV1::Formal)?;
    let mut host = IwsHostRuntimeV1::default();
    let result = match input.process {
        ColdFixtureProcessV1::Governed(policy) => host.install_authenticated_cold_governed_bounded(
            endpoint,
            input.provider.as_ref(),
            input.pinned,
            policy,
            input.worker,
            input.configuration,
            projection,
            &mut work.cold,
            &mut work.source,
            input.facts,
            mailbox_hint,
            &mut work.physical,
            &mut work.runtime,
            compiler,
            input.transaction,
            memory,
            archive,
            cancelled,
        ),
        #[cfg(feature = "node-libp2p")]
        ColdFixtureProcessV1::DevLocal(control) => host
            .install_authenticated_cold_devlocal_bounded(
                endpoint,
                input.provider.as_ref(),
                input.pinned,
                control,
                input.worker,
                input.configuration,
                projection,
                &mut work.cold,
                &mut work.source,
                input.facts,
                mailbox_hint,
                &mut work.physical,
                &mut work.runtime,
                compiler,
                input.transaction,
                memory,
                archive,
                cancelled,
            ),
    };
    result.map_err(ColdFixtureErrorV1::Host)?;
    Ok(host)
}

/// A fixed small snapshot of current selected positions for later *untrusted*
/// cold input. Call after the last accepted tick, then release every live owner.
/// The cold reader independently verifies these positions against durable head.
pub(crate) fn selected_fact_hints_bounded(
    host: &IwsHostRuntimeV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<IwsColdParentHintsV1, IwsHostRuntimeErrorV1> {
    let current = host
        .current
        .current()
        .ok_or_else(|| FormalError::Invalid("cold hint actual Host missing".into()))?;
    current.source.with_parent_read_view_bounded(
        memory,
        archive,
        &mut || cancelled.load(Ordering::Acquire),
        |parent| {
            archive.charge(2, 96, &mut || cancelled.load(Ordering::Acquire))?;
            Ok(IwsColdParentHintsV1 {
                runtime: parent.runtime().occurrence().location(),
                cognition: parent.cognition().selected().occurrence().location(),
            })
        },
    )
}
