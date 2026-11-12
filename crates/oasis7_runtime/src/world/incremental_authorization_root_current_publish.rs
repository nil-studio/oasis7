//! Fixed publisher for a fully closed authorization projection. The source
//! closure owns the expected and next hashes; this module only performs the
//! real Current CAS and advances the private stage after success.
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeWriter, ValueReadContext};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_runtime_authorization_root::{
        RuntimeAuthorizationRootQualificationV1, replace_current_authorization_root_v1,
    },
};

#[allow(clippy::too_many_arguments)]
fn write_closed_root<M, R, E, C, W, D>(
    expected: &[u8; 64],
    after: &[u8; 64],
    qualified: &ReservedValue<RuntimeAuthorizationRootQualificationV1>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<bool, SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
{
    // Fixed digest/hex/key/qualification comparisons. All tree/key/codec IO
    // below is debited from the same exclusive child, including returned errors.
    common.charge(512, 256)?;
    if !write.memory.same_pool(common.memory()) || !overlay.values.memory.same_pool(common.memory())
    {
        return Err(invalid("authorization publisher source pool").into());
    }
    let guard = overlay.transaction.guard();
    overlay
        .transaction
        .validate_guard(&guard, live)
        .map_err(SourceError::Execution)?;
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child()?;
    let visits = child.remaining_steps() / 2;
    let bytes = child.remaining_read_bytes() / 5;
    macro_rules! steps {
        ($m:ident) => {
            $m!(overlay.values.io.nodes_remaining, write.io.nodes_remaining)
        };
    }
    macro_rules! byte_lanes {
        ($m:ident) => {
            $m!(
                overlay.values.io.read_bytes_remaining,
                write.io.read_bytes_remaining,
                write.io.write_bytes_remaining,
                overlay.key_work.bytes_remaining,
                write.key_work.bytes_remaining
            )
        };
    }
    macro_rules! clamp_steps {($($lane:expr),*)=>{$($lane=$lane.min(visits);)*};}
    macro_rules! clamp_bytes {($($lane:expr),*)=>{$($lane=$lane.min(bytes);)*};}
    macro_rules! snapshot {($($lane:expr),*)=>{[$($lane),*]};}
    steps!(clamp_steps);
    byte_lanes!(clamp_bytes);
    let before_steps = steps!(snapshot);
    let before_bytes = byte_lanes!(snapshot);
    let result = {
        let stop = std::cell::RefCell::new(stop);
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut *overlay.transaction,
            values: ValueReadContext {
                source: overlay.values.source,
                memory: overlay.values.memory,
                class: overlay.values.class,
                io: &mut *overlay.values.io,
            },
            key_work: &mut *overlay.key_work,
            semantic_bytes: overlay.semantic_bytes,
            record_bytes: overlay.record_bytes,
            take_state_error: &mut overlay.take_state_error,
            cancelled: || (overlay.cancelled)() || (stop.borrow_mut())(),
        };
        let mut writer = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || (write.cancelled)() || (stop.borrow_mut())(),
        };
        replace_current_authorization_root_v1(
            expected,
            after,
            qualified,
            &mut current,
            &mut writer,
            &guard,
            live,
        )
        .map_err(SourceError::from)
    };
    let after_steps = steps!(snapshot);
    let after_bytes = byte_lanes!(snapshot);
    let debit = |before: &[u64], after: &[u64]| {
        before
            .iter()
            .zip(after)
            .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
    };
    let Some(steps) = debit(&before_steps, &after_steps) else {
        return result.and(Err(
            invalid("authorization publisher step accounting").into()
        ));
    };
    let Some(bytes) = debit(&before_bytes, &after_bytes) else {
        return result.and(Err(
            invalid("authorization publisher byte accounting").into()
        ));
    };
    let paid = child.consume(steps, bytes);
    child.finish();
    let changed = result?;
    paid?;
    common.consume_steps(0)?;
    Ok(changed)
}

/// A single-use source for the fixed OutcomeRecorded event sink. It can only
/// follow the complete seven-source closure and a successful real Text CAS.
pub(in crate::world) struct RootPublishedOutcomeV1 {
    outcome: OwnedProjectedOutcomeV1,
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    fee: ReservedValue<()>,
}
impl RootPublishedOutcomeV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), WarmError> {
        self.outcome.validate(tx, common, live)?;
        tx.validate_guard(&self.guard, live)
            .map_err(SourceError::Execution)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(invalid("authorization actual root publication binding").into());
        }
        Ok(())
    }
    pub(in crate::world) fn authorization_event<'a, M: MerkleNodeReader>(
        &'a self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<&'a crate::CapabilityAuthorizationEvent, WarmError> {
        self.validate(tx, common, live)?;
        self.outcome.authorization_event(tx, common, live)
    }
}
impl FinalAuthorizationProjectionV1<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_root<M, R, E, C, W, D>(
        mut self,
        qualified: &ReservedValue<RuntimeAuthorizationRootQualificationV1>,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<RootPublishedOutcomeV1, WarmError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: ValueNodeWriter,
        D: FnMut() -> bool,
    {
        let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (|| {
                self.stage.validate(overlay.transaction, common)?;
                self.outcome.validate(overlay.transaction, common, live)?;
                overlay
                    .transaction
                    .validate_guard(&self.guard, live)
                    .map_err(SourceError::Execution)?;
                if self.core != common.work().identity()
                    || !self.fee.uses_budget(common.memory())
                    || self.stage.expected != self.expected
                {
                    return Err(WarmError::from(invalid(
                        "authorization final publisher identity",
                    )));
                }
                let fee = common
                    .memory()
                    .reserve(
                        std::mem::size_of::<RootPublishedOutcomeV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(SourceError::from)?
                    .retain(());
                write_closed_root(
                    &self.expected,
                    &self.after,
                    qualified,
                    overlay,
                    write,
                    common,
                    live,
                )?;
                // Register the actual resulting pending entry, including a prior
                // same-transaction root write. Caller data can never acknowledge it.
                self.stage
                    .register_authorization_root(overlay.transaction, remaining, common)?;
                self.stage
                    .validate_registered_writes(overlay.transaction, remaining, common)?;
                common.consume_steps(0)?;
                let guard = overlay.transaction.guard();
                overlay
                    .transaction
                    .validate_guard(&guard, live)
                    .map_err(SourceError::Execution)?;
                self.stage.expected = self.after;
                self.outcome.guard = overlay.transaction.guard();
                Ok((guard, fee))
            })()
        }));
        let result = match attempted {
            Ok(result) => result,
            Err(payload) => {
                // Catching an allocator/producer panic must not allow reuse of
                // an overlay containing the already written root.
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    overlay.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        };
        match result {
            Ok((guard, fee)) => Ok(RootPublishedOutcomeV1 {
                outcome: self.outcome,
                guard,
                core: self.core,
                fee,
            }),
            Err(error) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    overlay.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
        }
    }
}
