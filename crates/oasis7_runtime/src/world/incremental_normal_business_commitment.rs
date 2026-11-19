//! Actual terminal world-service-result changes and original audiences.
//! This private source is DATA until Root binds its actual capture to the seal.
use super::incremental_authorization_root_current::{
    AuthorizationRootSourceReadWorkV1, SourceError, with_authorization_source_read_work_v1,
    with_business_source_read_work_v1,
};
use super::{
    incremental_body::BodyMutationError, incremental_module_invocation::*,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use crate::service_admission::CanonicalIntentResultV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        CurrentBusinessResultDeltaV1, RuntimeRevocationCurrentV1,
        read_current_business_participant_v1, read_current_business_result_delta_v1,
    },
    incremental_world_native::{StarterBindingIndexWorkV1, WorldNativeQualificationWorkV1},
};
#[path = "incremental_normal_business_commitment_genesis.rs"]
mod genesis;
pub use genesis::PaidProjectedGenesisBusinessSourceV1;
#[path = "incremental_normal_business_commitment_current_scope.rs"]
mod current_scope;
#[path = "incremental_normal_business_commitment_metadata.rs"]
pub(in crate::world) mod metadata;
#[path = "incremental_normal_business_commitment_metadata_parent.rs"]
pub(in crate::world) mod metadata_parent;
#[path = "incremental_normal_business_commitment_scope.rs"]
mod scope;
pub use metadata::{NormalTerminalMetadataReadWorkV1, NormalTerminalReadMetadataV1};
pub use metadata_parent::{
    NormalBusinessMetadataParentSourceV1, PreparedNormalBusinessMetadataGenesisV1,
};
pub struct NormalBusinessReadWorkV1 {
    pub authorization: AuthorizationRootSourceReadWorkV1,
    pub processing:
        oasis7_world_state::incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    pub starter_native: WorldNativeQualificationWorkV1,
    pub starter_scan: StateScanBudget,
    pub starter: StarterBindingIndexWorkV1,
}
pub struct ActualCurrentBusinessResultV1 {
    key: String,
    result: CanonicalIntentResultV1,
    scope: String,
    _phase: MemoryReservation,
    _scope_fee: MemoryReservation,
    _sources: Vec<current_scope::ScopeSourceV1>,
}
impl ActualCurrentBusinessResultV1 {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn result(&self) -> &CanonicalIntentResultV1 {
        &self.result
    }
    pub fn visibility_scope(&self) -> &str {
        &self.scope
    }
}
pub struct ActualCurrentBusinessDeltaV1 {
    rows: Vec<ActualCurrentBusinessResultV1>,
    actual: CurrentBusinessResultDeltaV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl ActualCurrentBusinessDeltaV1 {
    pub fn results(&self) -> &[ActualCurrentBusinessResultV1] {
        &self.rows
    }
    pub fn retained_result_layout_bytes_v1(&self) -> Option<usize> {
        self.rows.iter().try_fold(self._fee.bytes(), |n, r| {
            n.checked_add(r._phase.bytes())?
                .checked_add(r._scope_fee.bytes())
        })
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn before_len(&self) -> u64 {
        self.actual.before_len()
    }
    pub fn after_len(&self) -> u64 {
        self.actual.after_len()
    }
    pub(in crate::world) fn captured_guard(&self) -> &PreparedGuard {
        &self.guard
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_provenance(tx, common, live)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        Ok(())
    }
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("actual normal business commitment source")
}
#[derive(Debug)]
struct WorldCause(crate::WorldError);
impl std::fmt::Display for WorldCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for WorldCause {}
fn world_error(e: crate::WorldError) -> BodyMutationError {
    BodyMutationError::Original(Box::new(WorldCause(e)))
}
fn original(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
#[derive(Debug)]
struct SourceCause(SourceError);
impl std::fmt::Display for SourceCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for SourceCause {}
fn source(e: SourceError) -> BodyMutationError {
    original(SourceCause(e))
}
fn note<E>(
    note: String,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<RootModuleExecutionErrorV1<E>, BodyMutationError> {
    let bytes = note
        .capacity()
        .checked_add(std::mem::size_of::<crate::WorldError>())
        .ok_or_else(bad)?;
    common.charge(1, bytes as u64).map_err(original)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let error = crate::WorldError::DistributedValidationFailed { reason: note };
    Ok(RootModuleExecutionErrorV1::Business(fee.retain(error)))
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn capture_normal_business_delta_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        revocation: &RuntimeRevocationCurrentV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalBusinessReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ActualCurrentBusinessDeltaV1, RootModuleExecutionErrorV1<O::Error>> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(original)?;
            let refs = parent.refs();
            let guard = self.ctx.transaction.guard();
            let live = live_owner();
            let actual = with_business_source_read_work_v1(
                refs.roots,
                self.ctx,
                read,
                &mut work.authorization,
                &mut work.processing,
                common,
                |ctx, read, w, processing| {
                    read_current_business_result_delta_v1(
                        refs.storage.current().runtime().revocation(),
                        revocation,
                        ctx,
                        read,
                        &mut w.scan,
                        &mut w.typed,
                        processing,
                        &guard,
                        &live,
                    )
                    .map_err(SourceError::from)
                },
            )
            .map_err(source)?;
            actual
                .validate(self.ctx.transaction, common.memory(), &live)
                .map_err(current_scope::overlay)?;
            let slots = actual
                .changes()
                .len()
                .checked_mul(std::mem::size_of::<ActualCurrentBusinessResultV1>())
                .and_then(|n| n.checked_add(std::mem::size_of::<ActualCurrentBusinessDeltaV1>()))
                .ok_or_else(bad)?;
            common.charge(1, slots as u64).map_err(original)?;
            let fee = common
                .memory()
                .reserve(slots, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut rows = Vec::new();
            rows.try_reserve_exact(actual.changes().len())
                .map_err(|_| bad())?;
            if rows.capacity() > actual.changes().len() {
                return Err(bad().into());
            }
            for change in actual.changes() {
                let Some(value) = change.after() else {
                    continue;
                };
                let graph = with_authorization_source_read_work_v1(
                    refs.roots,
                    self.ctx,
                    read,
                    &mut work.authorization,
                    common,
                    |ctx, _, w| {
                        change
                            .retained_allocation_bytes_v1(
                                &mut w.typed.json.cells_remaining,
                                &mut ctx.cancelled,
                            )
                            .map_err(SourceError::from)
                    },
                )
                .map_err(source)?;
                // Closed CanonicalIntentResult has bounded typed structure plus arbitrary
                // receipt JSON. The original recovered JSON graph pays the complete clone,
                // typed decode, correlation/signature payload and terminal-check peak.
                let peak = graph
                    .checked_mul(64)
                    .and_then(|n| n.checked_add(128 * 1024))
                    .ok_or_else(bad)?;
                let ops = u64::try_from(peak).map_err(|_| bad())?;
                common.charge(ops, ops).map_err(original)?;
                let phase = common
                    .memory()
                    .reserve(peak, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let result =
                    match oasis7_world_state::service_admission::correlation::validate_result_record_uncached(
                        change.key(),
                        value,
                    ) {
                        Ok(r) => r,
                        Err(e) => return Err(note(e, common)?),
                    };
                let (scope, scope_fee, sources) = self.read_current_business_scope_v1(
                    &result.request,
                    parent,
                    revocation,
                    read,
                    work,
                    common,
                    live_owner,
                )?;
                rows.push(ActualCurrentBusinessResultV1 {
                    key: change.key().to_owned(),
                    result,
                    scope,
                    _phase: phase,
                    _scope_fee: scope_fee,
                    _sources: sources,
                });
            }
            let guard = self.ctx.transaction.guard();
            self.ctx.transaction.validate_guard(&guard, &live_owner())?;
            Ok(ActualCurrentBusinessDeltaV1 {
                rows,
                actual,
                guard,
                core: common.work().identity(),
                memory: common.memory().clone(),
                _fee: fee,
            })
        }));
        match attempt {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => {
                let _: Result<(), _> = self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
                Err(e)
            }
            Err(p) => {
                let _: Result<(), _> = self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
                std::panic::resume_unwind(p)
            }
        }
    }
}

/// Only Root's genuine Prepared/same-seal pointer-validation path constructs
/// this borrow. It carries no caller-selected roots, source or metadata DTO.
pub struct PaidBorrowedNormalBusinessSourceV1<'a> {
    pub(in crate::world) delta: &'a ActualCurrentBusinessDeltaV1,
    pub(in crate::world) metadata: &'a NormalTerminalReadMetadataV1,
    pub(in crate::world) parent:
        &'a oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    pub(in crate::world) after_roots: &'a oasis7_formal_protocol::incremental_execution::RootSetV1,
    pub(in crate::world) seal:
        &'a crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
    pub(in crate::world) raw_state_hash: &'a str,
    pub(in crate::world) core: WorkIdentityV1,
    pub(in crate::world) memory: MemoryBudget,
    pub(in crate::world) fee: MemoryReservation,
}
impl PaidBorrowedNormalBusinessSourceV1<'_> {
    pub fn delta(&self) -> &ActualCurrentBusinessDeltaV1 {
        self.delta
    }
    pub fn metadata(&self) -> &NormalTerminalReadMetadataV1 {
        self.metadata
    }
    pub fn actual_parent(
        &self,
    ) -> &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1 {
        self.parent
    }
    pub fn after_roots(&self) -> &oasis7_formal_protocol::incremental_execution::RootSetV1 {
        self.after_roots
    }
    pub fn raw_state_hash(&self) -> &str {
        self.raw_state_hash
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.delta.validate_context(common)?;
        self.metadata.validate_context(common)?;
        let bytes = crate::world::incremental_normal_sources::root_bytes(
            self.after_roots,
            self.after_roots,
        )
        .map_err(original)?;
        common
            .charge(4, bytes.checked_mul(4).ok_or_else(bad)?)
            .map_err(original)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
            || self.metadata.parent_record() != self.parent.verified_record_id()
            || self.after_roots.world_id != self.parent.record().world.world_id
        {
            return Err(bad());
        }
        self.seal
            .validate_projection_raw_hash_v1(self.raw_state_hash, common)?;
        self.seal.validate_business_source_v1(self.delta, common)?;
        self.seal
            .validate_business_metadata_source_v1(self.metadata, common)?;
        if self.seal.after_roots() != self.after_roots.digest()? {
            return Err(bad());
        }
        Ok(())
    }
}

pub(in crate::world) use metadata_parent::OriginalProjectionReadMetadataV1;

// Actual Current ChainResourceManifest world identity; readonly DATA only.
pub(in crate::world) use metadata::read_manifest_world;
