//! Actual Host GEN fanout to two independent endpoint page namespaces.
//! This produces provisional bytes and installs capabilities, never receipts.
use super::incremental_genesis::*;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::{
        FileFormalEndpoint, IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
        IwsPageStoreCapabilityV1,
    },
    incremental_page_store::{
        IncrementalPageStore, PageIoBudget, PageStoreError, objects::IncrementalObjectStore,
    },
};
use oasis7_formal_protocol::{
    incremental_state::{StateScanBudget, StateScanError},
    incremental_value::{ValueIoBudget, ValueTreeError},
};
use oasis7_runtime::{
    BodyMutationError, World,
    incremental_action_original::{
        ActionOriginalStreamV1, persist_action_original_v1, persist_world_event_original_v1,
    },
    incremental_genesis::RuntimeGenesisWorkV1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::WorldEventBody;
use std::{cell::RefCell, convert::Infallible, sync::atomic::Ordering};
#[path = "formal_iws_originals.rs"]
mod originals;
#[path = "formal_iws_pages.rs"]
mod pages;
/// Every account belongs to the caller. Three object accounts distinguish the
/// producer and both replicas; neither a retry nor a second original resets any.
pub(crate) struct IwsGenesisTransferWorkV1 {
    pub objects_physical: [PageIoBudget; 3],
    pub objects_logical: [ValueIoBudget; 3],
    pub page_publish: [PageIoBudget; 2],
    pub outer_reads: PageIoBudget,
    pub outer_scan: StateScanBudget,
    pub canonical: CanonicalCborWorkV1,
    pub originals_remaining: u64,
    pub max_original_bytes: usize,
    pub mailbox_search_steps: u64,
}
#[derive(Debug)]
pub(crate) enum IwsGenesisTransferErrorV1 {
    Value(ValueTreeError),
    Store(PageStoreError),
    Body(BodyMutationError),
    Genesis(RuntimeGenesisPhysicalErrorV1),
    Scan(StateScanError<Infallible>),
}
impl std::fmt::Display for IwsGenesisTransferErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Body(e) => e.fmt(f),
            Self::Genesis(e) => e.fmt(f),
            Self::Scan(e) => write!(f, "{e:?}"),
        }
    }
}
impl std::error::Error for IwsGenesisTransferErrorV1 {}
impl From<ValueTreeError> for IwsGenesisTransferErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<PageStoreError> for IwsGenesisTransferErrorV1 {
    fn from(e: PageStoreError) -> Self {
        Self::Store(e)
    }
}
fn lease_id(store: &IncrementalPageStore) -> Result<(u64, u64), IwsGenesisTransferErrorV1> {
    use std::os::unix::fs::MetadataExt;
    let metadata =
        std::fs::symlink_metadata(store.root().join("lease")).map_err(|e| PageStoreError::Io(e))?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() != 0 {
        return Err(ValueTreeError::Invalid("IWS GEN endpoint lease").into());
    }
    Ok((metadata.dev(), metadata.ino()))
}
/// No Snapshot/Journal material is accepted by this entry. The actual borrowed
/// initial World is only a GEN producer; ordinary execution uses a root driver.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_iws_genesis_for_endpoints_v1(
    world: &World,
    source: &IncrementalPageStore,
    targets: [&IncrementalPageStore; 2],
    config: &oasis7_formal_protocol::incremental_history::GenesisConfigV1,
    context: &mut RuntimeGenesisPhysicalContextV1<'_>,
    producer: &mut RuntimeGenesisWorkV1,
    physical: RuntimeGenesisPhysicalBudgetsV1,
    work: &mut IwsGenesisTransferWorkV1,
) -> Result<ProvisionalRuntimeGenesisIndexV1, IwsGenesisTransferErrorV1> {
    if context.cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !context.memory.same_pool(source.memory_budget()) {
        return Err(ValueTreeError::Invalid("IWS GEN source pool").into());
    }
    let identity = [
        lease_id(source)?,
        lease_id(targets[0])?,
        lease_id(targets[1])?,
    ];
    if identity[0] == identity[1] || identity[0] == identity[2] || identity[1] == identity[2] {
        return Err(
            ValueTreeError::Invalid("IWS GEN replicas must be independent namespaces").into(),
        );
    }
    let stores = [
        IncrementalObjectStore::create(source, &mut work.objects_physical[0], context.cancelled)?,
        IncrementalObjectStore::create(
            targets[0],
            &mut work.objects_physical[1],
            context.cancelled,
        )?,
        IncrementalObjectStore::create(
            targets[1],
            &mut work.objects_physical[2],
            context.cancelled,
        )?,
    ];
    let failed = RefCell::new(None);
    let body_error = RefCell::new(None);
    let originals = RefCell::new(originals::Originals {
        stores: &stores,
        physical: &mut work.objects_physical,
        logical: &mut work.objects_logical,
        occurrences: &mut work.originals_remaining,
        max_bytes: work.max_original_bytes,
        cancelled: context.cancelled,
        error: &failed,
    });
    let canonical = RefCell::new(&mut work.canonical);
    let cancelled = context.cancelled;
    let memory = context.memory;
    let max_original = work.max_original_bytes;
    let mailbox_steps = &mut work.mailbox_search_steps;
    let mut first_cancel = || cancelled.load(std::sync::atomic::Ordering::Acquire);
    let mut second_cancel = || cancelled.load(std::sync::atomic::Ordering::Acquire);
    let targets = pages::Targets::new(
        targets,
        &mut work.page_publish,
        cancelled,
        [&mut first_cancel, &mut second_cancel],
    )?;
    let RuntimeGenesisPhysicalBudgetsV1 {
        producer_pages,
        index_reads,
        index_writes,
        index,
    } = physical;
    let mut primary = source.value_session_with_cancelled(producer_pages, cancelled);
    let mut fanout = pages::Fanout {
        primary: &mut primary,
        targets: &targets,
        memory,
    };
    let result = prepare_runtime_genesis_index_with_writer_v1(
        world,
        source,
        config,
        context,
        producer,
        RuntimeGenesisIndexBudgetsV1 {
            index_reads,
            index_writes,
            index,
        },
        &mut fanout,
        &mut |reference, bytes| {
            originals
                .borrow_mut()
                .raw(reference.id, bytes)
                .map_err(|error| {
                    *body_error.borrow_mut() = Some(error);
                    ValueTreeError::Invalid("IWS GEN artifact original transfer failed")
                })
        },
        &mut |_ordinal, action| {
            let receipt = persist_action_original_v1(
                action,
                memory,
                &mut canonical.borrow_mut(),
                &mut || cancelled.load(Ordering::Acquire),
                max_original,
                &mut |stream: &mut dyn ActionOriginalStreamV1, poll: &mut dyn FnMut() -> bool| {
                    originals.borrow_mut().stream(stream, poll)
                },
            )
            .map_err(|error| {
                *body_error.borrow_mut() = Some(error);
                ValueTreeError::Invalid("IWS GEN action original transfer failed")
            })?;
            Ok(
                oasis7_formal_protocol::incremental_artifacts::OriginalArtifactRefV1 {
                    id: receipt.original_id(),
                    byte_len: receipt.original_bytes(),
                },
            )
        },
        &mut |_agent, _ordinal, event| {
            let mut selected = None;
            for original in &world.journal().events {
                *mailbox_steps = mailbox_steps
                    .checked_sub(1)
                    .ok_or(BodyMutationError::Invalid(
                        "IWS GEN mailbox original search allowance",
                    ))?;
                if matches!(&original.body,WorldEventBody::Domain(domain) if domain==event) {
                    selected = Some(original);
                    break;
                }
            }
            let original = selected.ok_or(BodyMutationError::Invalid(
                "IWS GEN mailbox has no actual WorldEvent original",
            ))?;
            let receipt = persist_world_event_original_v1(
                original,
                memory,
                &mut canonical.borrow_mut(),
                &mut || cancelled.load(Ordering::Acquire),
                max_original,
                &mut |stream: &mut dyn ActionOriginalStreamV1, poll: &mut dyn FnMut() -> bool| {
                    originals.borrow_mut().stream(stream, poll)
                },
            )?;
            Ok(receipt.original_id())
        },
    );
    if let Some(error) = failed.borrow_mut().take() {
        return Err(IwsGenesisTransferErrorV1::Store(error));
    }
    if let Some(error) = targets.take_error() {
        return Err(IwsGenesisTransferErrorV1::Store(error));
    }
    if let Some(error) = body_error.borrow_mut().take() {
        return Err(IwsGenesisTransferErrorV1::Body(error));
    }
    let result = result.map_err(IwsGenesisTransferErrorV1::Genesis)?;
    targets.check_healthy()?;
    // Move the allowance out before IO. Even a poisoned read account cannot
    // leave the original allowance available for an implicit retry.
    let outer_io = std::mem::replace(
        &mut work.outer_reads,
        PageIoBudget {
            reads_remaining: 0,
            read_bytes_remaining: 0,
            writes_remaining: 0,
            write_bytes_remaining: 0,
        },
    );
    let state = source.state_session_with_cancelled(
        outer_io,
        oasis7_proto::memory_budget::MemoryClass::Ordinary,
        cancelled,
    )?;
    let transferred = pages::send_outer_indexes(
        &result.roots,
        &state,
        &targets,
        memory,
        &mut work.outer_scan,
    );
    let actual_error = state.take_read_error();
    let remaining = state.remaining_io();
    if let Ok(remaining) = &remaining {
        work.outer_reads = remaining.clone();
    }
    if let Some(error) = actual_error {
        return Err(IwsGenesisTransferErrorV1::Store(error));
    }
    remaining?;
    transferred?;
    targets.finish()?;
    targets.check_complete()?;
    Ok(result)
}
/// Install only the actual transferred namespace. The endpoint still performs
/// its complete candidate checks and retains both receipt/Qualified guards.
pub(crate) fn install_iws_endpoint_pages_v1(
    endpoint: &mut FileFormalEndpoint,
    pages: IncrementalPageStore,
    budgets: IwsEndpointVerificationBudgetsV1,
    current: IwsCurrentLimitsV1,
    memo: Option<(usize, usize)>,
) -> Result<(), oasis7_distfs::controlled_authority::formal::FormalError> {
    let mut capability =
        IwsPageStoreCapabilityV1::new(pages, budgets)?.with_current_limits(current)?;
    if let Some((entries, bytes)) = memo {
        capability = capability.with_state_read_memo(entries, bytes)?;
    }
    endpoint.install_iws_page_store(capability)
}
