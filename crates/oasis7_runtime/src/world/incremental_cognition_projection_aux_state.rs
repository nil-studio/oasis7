//! One exclusive Common child for the actual Cognition JSON20/control10
//! source. Caller work is loaned once and reconciled on success/error/panic.
use crate::world::{
    incremental_body::BodyMutationError, incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_projection_aux::{
        CognitionProjectionAuxWorkV1, CurrentCognitionStateProjectionAuxV1,
        capture_current_cognition_state_projection_aux_v1,
        capture_qualified_cognition_state_projection_aux_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Cognition projection actual Common source")
}
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
pub(in crate::world) struct PaidCognitionStateProjectionAuxV1 {
    source: CurrentCognitionStateProjectionAuxV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl PaidCognitionStateProjectionAuxV1 {
    pub(in crate::world) fn source(&self) -> &CurrentCognitionStateProjectionAuxV1 {
        &self.source
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(formal)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.source
            .validate(tx, common.memory(), live)
            .map_err(overlay)
    }
}
use oasis7_world_state::incremental_cognition_proof_ledger::{
    AcceptedCognitionProofLedgerV1, CognitionProofLedgerProjectionAuxDataV1,
    FrozenCognitionProofLedgerManifestSourceV1,
    capture_current_proof_ledger_projection_aux_data_v1,
    capture_qualified_proof_ledger_projection_aux_data_v1,
};
enum Scope<'a, 'w> {
    Current,
    Qualified,
    CurrentProof {
        before: &'a ReservedValue<AcceptedCognitionProofLedgerV1>,
        after: &'a ReservedValue<FrozenCognitionProofLedgerManifestSourceV1>,
    },
    QualifiedProof {
        before: &'a ReservedValue<AcceptedCognitionProofLedgerV1>,
    },
    HistoryRow {
        request: history::HistoryRowRequestV1<'a>,
        source: &'a mut dyn history::OriginalDataReaderV1,
        work: &'a mut history::CognitionHistoryProjectionAuxWorkV1<'w>,
    },
}
impl Scope<'_, '_> {
    fn history_steps(&self) -> Option<[u64; 4]> {
        match self {
            Self::HistoryRow { work, .. } => Some(work.steps()),
            _ => None,
        }
    }
    fn history_bytes(&self) -> Option<[u64; 3]> {
        match self {
            Self::HistoryRow { work, .. } => Some(work.bytes()),
            _ => None,
        }
    }
    fn clamp_history(&mut self, s: u64, b: u64) {
        if let Self::HistoryRow { work, .. } = self {
            work.clamp(s, b)
        }
    }
    fn restore_history(
        &mut self,
        original_s: Option<[u64; 4]>,
        before_s: Option<[u64; 4]>,
        after_s: Option<[u64; 4]>,
        original_b: Option<[u64; 3]>,
        before_b: Option<[u64; 3]>,
        after_b: Option<[u64; 3]>,
    ) {
        if let (
            Self::HistoryRow { work, .. },
            Some(os),
            Some(bs),
            Some(as_),
            Some(ob),
            Some(bb),
            Some(ab),
        ) = (
            self, original_s, before_s, after_s, original_b, before_b, after_b,
        ) {
            work.restore(os, bs, as_, ob, bb, ab)
        }
    }
}

enum Component {
    State(CurrentCognitionStateProjectionAuxV1),
    Proof(CognitionProofLedgerProjectionAuxDataV1),
    History(history::HistoryPacketV1),
}
struct PaidComponent {
    source: Component,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl PaidComponent {
    fn into_state(self) -> Result<PaidCognitionStateProjectionAuxV1, BodyMutationError> {
        let Self {
            source,
            core,
            memory,
            _fee,
        } = self;
        match source {
            Component::State(source) => Ok(PaidCognitionStateProjectionAuxV1 {
                source,
                core,
                memory,
                _fee,
            }),
            Component::Proof(_) | Component::History(_) => Err(bad()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn capture_inner<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CognitionProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    mut scope: Scope<'_, '_>,
) -> Result<PaidComponent, BodyMutationError> {
    parent.validate_context(common).map_err(formal)?;
    if ctx.transaction.parent_fence() != parent.fence()
        || !ctx.transaction.uses_budget(common.memory())
        || !ctx.values.memory.same_pool(common.memory())
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    let bytes = std::mem::size_of::<PaidComponent>() + 4096;
    common.charge(1, bytes as u64).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let core = common.work().identity();
    let memory = common.memory().clone();
    macro_rules! apply {
        (clamp_b; $($args:tt)*) => { clamp_b!($($args)*) };
        (clamp_s; $($args:tt)*) => { clamp_s!($($args)*) };
        (restore_b; $($args:tt)*) => { restore_b!($($args)*) };
        (restore_s; $($args:tt)*) => { restore_s!($($args)*) };
        (snap; $($args:tt)*) => { snap!($($args)*) };
        (zero; $($args:tt)*) => { zero!($($args)*) };
    }
    macro_rules! steps {
        ($m:ident) => {
            apply!($m;
                ctx.values.io.nodes_remaining,
                work.json.json.cells_remaining,
                work.json.json.json_nodes_remaining,
                work.json.paths_remaining,
                *work.controls_remaining,
                work.processing.steps_remaining
            )
        };
    }
    macro_rules! bytes {
        ($m:ident) => {
            apply!($m;
                ctx.values.io.read_bytes_remaining,
                ctx.values.io.write_bytes_remaining,
                ctx.key_work.bytes_remaining,
                work.processing.bytes_remaining
            )
        };
    }
    macro_rules! snap{($($lane:expr),*)=>{[$($lane),*]};}
    let original_s = steps!(snap);
    let original_b = bytes!(snap);
    let original_n = *work.pending_entries;
    let original_hs = scope.history_steps();
    let original_hb = scope.history_bytes();
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child().map_err(formal)?;
    let sc = child.remaining_steps() / (7 + if original_hs.is_some() { 4 } else { 0 });
    let bc = child.remaining_read_bytes() / (4 + if original_hb.is_some() { 3 } else { 0 });
    macro_rules! clamp_s{($($lane:expr),*)=>{$($lane=$lane.min(sc);)*};}
    macro_rules! clamp_b{($($lane:expr),*)=>{$($lane=$lane.min(bc);)*};}
    steps!(clamp_s);
    bytes!(clamp_b);
    *work.pending_entries = (*work.pending_entries).min(usize::try_from(sc).unwrap_or(usize::MAX));
    scope.clamp_history(sc, bc);
    let before_s = steps!(snap);
    let before_b = bytes!(snap);
    let before_n = *work.pending_entries;
    let before_hs = scope.history_steps();
    let before_hb = scope.history_bytes();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cancel = || stop() || (ctx.cancelled)();
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut cancel,
        };
        let q = parent.refs().storage.current().cognition();
        let roots = parent.refs().roots;
        match &mut scope {
            Scope::Current => capture_current_cognition_state_projection_aux_v1(
                q,
                roots,
                &mut current,
                work,
                live,
            )
            .map(Component::State),
            Scope::Qualified => capture_qualified_cognition_state_projection_aux_v1(
                q,
                roots,
                &mut current,
                work,
                live,
            )
            .map(Component::State),
            Scope::CurrentProof { before, after } => {
                capture_current_proof_ledger_projection_aux_data_v1(
                    before,
                    after,
                    &mut current,
                    work.json,
                    work.processing,
                    live,
                )
                .map(Component::Proof)
            }
            Scope::QualifiedProof { before } => {
                capture_qualified_proof_ledger_projection_aux_data_v1(
                    before,
                    &mut current,
                    work.json,
                    work.processing,
                    live,
                )
                .map(Component::Proof)
            }
            Scope::HistoryRow {
                request,
                source,
                work: history,
            } => {
                return history::read_packet(
                    request,
                    &mut current,
                    &mut **source,
                    work,
                    history,
                    live,
                )
                .map(Component::History);
            }
        }
        .map_err(overlay)
    }));
    let outcome = match outcome {
        Ok(result) => result,
        Err(panic) => {
            macro_rules! zero{($($lane:expr),*)=>{$($lane=0;)*};}
            steps!(zero);
            bytes!(zero);
            *work.pending_entries = 0;
            scope.clamp_history(0, 0);
            drop(child);
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    };
    let after_s = steps!(snap);
    let after_b = bytes!(snap);
    let after_n = *work.pending_entries;
    let after_hs = scope.history_steps();
    let after_hb = scope.history_bytes();
    let debit = |before: &[u64], after: &[u64]| {
        before
            .iter()
            .zip(after)
            .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
    };
    let used = debit(&before_s, &after_s)
        .and_then(|n| n.checked_add(u64::try_from(before_n.checked_sub(after_n)?).ok()?))
        .zip(debit(&before_b, &after_b));
    let used = used.and_then(|(s, b)| match (before_hs, after_hs, before_hb, after_hb) {
        (Some(bs), Some(as_), Some(bb), Some(ab)) => s
            .checked_add(debit(&bs, &as_)?)
            .zip(b.checked_add(debit(&bb, &ab)?)),
        (None, None, None, None) => Some((s, b)),
        _ => None,
    });
    let mut i = 0;
    macro_rules! restore_s{($($lane:expr),*)=>{$({$lane=original_s[i].saturating_sub(before_s[i].saturating_sub(after_s[i]));i+=1;})*};}
    steps!(restore_s);
    let _ = i;
    let mut i = 0;
    macro_rules! restore_b{($($lane:expr),*)=>{$({$lane=original_b[i].saturating_sub(before_b[i].saturating_sub(after_b[i]));i+=1;})*};}
    bytes!(restore_b);
    let _ = i;
    *work.pending_entries = original_n.saturating_sub(before_n.saturating_sub(after_n));
    scope.restore_history(
        original_hs,
        before_hs,
        after_hs,
        original_hb,
        before_hb,
        after_hb,
    );
    let paid = match used {
        Some((s, b)) => child.consume(s, b).map_err(formal),
        None => Err(bad()),
    };
    child.finish();
    let result = (|| {
        let source = outcome?;
        paid?;
        common.consume_steps(0).map_err(formal)?;
        Ok(PaidComponent {
            source,
            core,
            memory,
            _fee: fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn capture<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CognitionProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    scope: Scope<'_, '_>,
) -> Result<PaidComponent, BodyMutationError> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        capture_inner(parent, ctx, work, common, live, scope)
    }));
    match outcome {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
pub(in crate::world) fn capture_current_cognition_state_projection_aux_bounded_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CognitionProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCognitionStateProjectionAuxV1, BodyMutationError> {
    capture(parent, ctx, work, common, live, Scope::Current)?.into_state()
}
pub(in crate::world) fn capture_qualified_cognition_state_projection_aux_bounded_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CognitionProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCognitionStateProjectionAuxV1, BodyMutationError> {
    capture(parent, ctx, work, common, live, Scope::Qualified)?.into_state()
}

#[path = "incremental_cognition_projection_aux_proof.rs"]
mod proof;
pub(in crate::world) use proof::{
    PaidCognitionProofProjectionAuxV1, capture_current_cognition_proof_projection_aux_bounded_v1,
    capture_qualified_absent_cognition_proof_projection_aux_bounded_v1,
    capture_qualified_cognition_proof_projection_aux_bounded_v1,
};

#[path = "incremental_cognition_projection_aux_history.rs"]
mod history;
pub(in crate::world) use history::{
    CognitionHistoryProjectionAuxWorkV1, CognitionHistoryProjectionErrorV1,
    PaidCognitionHistoryProjectionAuxV1,
    capture_current_cognition_history_projection_aux_bounded_v1,
    capture_qualified_cognition_history_projection_aux_bounded_v1,
};
