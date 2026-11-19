//! Native integration support for a genuine reopen after every live Host,
//! World, Node, projected owner and old page handle has been dropped.
//! Paths and selected fact locations remain untrusted DATA. The production
//! cold consumer revalidates the durable checkpoint, originals and all sources.
use super::*;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::{
        IwsColdParentHintsV1, IwsColdParentWorkV1, IwsCurrentLimitsV1,
    },
    incremental_page_store::PageStoreError,
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{VerifiedAuthorityV1, bounded_context::FormalBoundedContextV1},
        replicated_protocol::EndpointRole,
    },
    incremental_execution::TransactionLimits,
    incremental_material::FactLocationV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_runtime::{
    AuthorizationRootCacheErrorV1, AuthorizationRootCacheLimitsV1, BodyMutationError,
    NormalHostWorkV1, ReleaseSecurityPolicy, RuntimeSourceCompiler, WorldRuntimeMemoryLimits,
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
use std::{path::Path, sync::Arc};

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
pub(crate) type ColdFixtureWorkV1 = super::cold_recovery::HostColdRecoveryWorkV1;

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
        Self::from_parts_bounded(
            super::cold_recovery::HostColdRecoveryPartsV1 {
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
            },
            common,
        )
        .map_err(|error| ColdFixtureErrorV1::Host(HostColdStartupErrorV1::Host(error)))
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
    pub checkpoint:
        Option<oasis7_distfs::controlled_authority::formal::endpoint::IwsColdCheckpointSuffixV1>,
}

/// Exactly one strict cold attempt. All work is borrowed and remains visible
/// to the caller after refusal. No cached source or same-process reopen path is
/// available, and missing/incorrect fact hints are actual verification errors.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reopen_host_bounded<L>(
    input: ColdFixtureInputV1<'_>,
    work: &mut ColdFixtureWorkV1,
    mut mailbox_hint: L,
    compiler: &RuntimeSourceCompiler,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<IwsHostRuntimeV1, ColdFixtureErrorV1>
where
    L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
{
    let process = match input.process {
        ColdFixtureProcessV1::Governed(policy) => super::cold::ProcessInput::Governed(policy),
        #[cfg(feature = "node-libp2p")]
        ColdFixtureProcessV1::DevLocal(control) => super::cold::ProcessInput::DevLocal(control),
    };
    let budgets = endpoint_work::budgets();
    super::cold_recovery::reopen_host_bounded_v1(
        super::cold_recovery::HostColdReopenInputV1 {
            endpoint: super::cold_recovery::ColdEndpointReopenRefsV1 {
                endpoint_path: input.endpoint_path,
                pages_path: input.pages_path,
                authority: input.authority,
                role: input.role,
                provider: input.provider,
                policy: input.pinned,
                budgets: &budgets,
                current_limits: input.current_limits,
                checkpoint: input.checkpoint,
            },
            authorization_hint_path: input.authorization_hint_path,
            projection_limits: input.projection_limits,
            process,
            configuration: &input.configuration,
            worker: &input.worker,
            transaction: input.transaction,
            facts: input.facts,
        },
        work,
        &mut mailbox_hint,
        super::cold_recovery::HostColdRecoveryContextV1 {
            memory,
            archive,
            cancelled,
            compiler,
        },
    )
    .map_err(|error| match error {
        IwsHostRuntimeErrorV1::ColdProjection(error) => ColdFixtureErrorV1::Projection(error),
        IwsHostRuntimeErrorV1::Physical(PageStoreError::Io(error)) => ColdFixtureErrorV1::Io(error),
        IwsHostRuntimeErrorV1::Physical(error) => ColdFixtureErrorV1::Physical(error),
        IwsHostRuntimeErrorV1::Value(error) => ColdFixtureErrorV1::Value(error),
        IwsHostRuntimeErrorV1::NormalSource(error) => ColdFixtureErrorV1::Body(error),
        IwsHostRuntimeErrorV1::Formal(error) => ColdFixtureErrorV1::Formal(error),
        IwsHostRuntimeErrorV1::ColdSource(error) => {
            ColdFixtureErrorV1::Host(HostColdStartupErrorV1::Source(error))
        }
        error => ColdFixtureErrorV1::Host(HostColdStartupErrorV1::Host(error)),
    })
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
    super::cold_recovery::selected_fact_hints_bounded_v1(host, memory, archive, cancelled)
}
