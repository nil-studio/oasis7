//! Sealed execution permission. DATA proofs never expose a constructor.
//! The sole actual-Current scope builder must finish every original predicate
//! before constructing this type; storage readers cannot mint it.
use super::{
    incremental_action_execution::SelectedPendingActionV1, incremental_body::BodyMutationError,
    incremental_safe_alternative_record::PaidSignedSafeAlternativeRecordV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
pub(super) struct PaidMatchingSafeAlternativeV1<'selected, 'source> {
    input: PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    guard: PreparedGuard,
    _outer: ReservedValue<()>,
    _scope_fee: ReservedValue<()>,
}
impl PaidMatchingSafeAlternativeV1<'_, '_> {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.input.validate_after_result(tx, common)?;
        tx.validate_guard(&self.guard, live)?;
        if !std::ptr::eq(self.input.selected, selected)
            || !self._outer.uses_budget(memory)
            || !self._scope_fee.uses_budget(memory)
            || !common.memory().same_pool(memory)
        {
            return Err(BodyMutationError::Invalid(
                "matching SafeAlternative scope binding",
            ));
        }
        Ok(())
    }
}

/// A genuine matching authorization whose original guard rejected execution.
/// It follows Accepted -> RuleDenied; it is never converted to Unbound.
pub(super) struct PaidRejectedSafeAlternativeV1<'selected, 'source> {
    input: PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    reason: String,
    guard: PreparedGuard,
    _outer: ReservedValue<()>,
    _scope_fee: ReservedValue<()>,
}
impl PaidRejectedSafeAlternativeV1<'_, '_> {
    pub(super) fn reason(&self) -> &str {
        &self.reason
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.input.validate_after_result(tx, common)?;
        tx.validate_guard(&self.guard, live)?;
        if !std::ptr::eq(self.input.selected, selected)
            || !self._outer.uses_budget(common.memory())
            || !self._scope_fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "rejected SafeAlternative scope binding",
            ));
        }
        Ok(())
    }
}
pub(super) enum CheckedMatchingSafeAlternativeV1<'selected, 'source> {
    Matched(PaidMatchingSafeAlternativeV1<'selected, 'source>),
    Rejected(PaidRejectedSafeAlternativeV1<'selected, 'source>),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_matching_safe_alternative_v1<
    'selected,
    'source,
    M,
    R,
    E,
    C,
    P,
    V,
    ER,
    CR,
    MC,
>(
    input: PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    sources: &super::incremental_industrial_eligibility::SafeAlternativeCurrentSourcesV1<'_, '_>,
    now: u64,
    state: &M,
    overlay: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
        '_,
        P,
        V,
        ER,
        CR,
    >,
    modules: &mut super::incremental_module_read::ModuleReadContext<'_, R, MC>,
    work: &mut super::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    hash_work: &mut super::incremental_module_execution_manifest_hash::RootWorldManifestHashWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CheckedMatchingSafeAlternativeV1<'selected, 'source>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    ER: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    CR: FnMut() -> bool,
    MC: FnMut() -> bool,
{
    let result = (|| {
        input.validate_before_scope(overlay.transaction, common, live)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<PaidMatchingSafeAlternativeV1<'selected, 'source>>()
                    .checked_add(std::mem::size_of::<
                        PaidRejectedSafeAlternativeV1<'selected, 'source>,
                    >())
                    .ok_or(
                        oasis7_formal_protocol::incremental_value::ValueTreeError::ResourceLimited(
                            "SafeAlternative admission layout",
                        ),
                    )?,
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
        let scope =
            super::incremental_industrial_eligibility::check_current_safe_alternative_scope_v1(
                &input, sources, now, state, overlay, read, modules, work, hash_work, common, live,
            )?;
        let (reason, scope_fee) = scope.into_result(&input, overlay.transaction, common, live)?;
        let guard = overlay.transaction.guard();
        Ok(match reason {
            Some(reason) => {
                CheckedMatchingSafeAlternativeV1::Rejected(PaidRejectedSafeAlternativeV1 {
                    input,
                    reason,
                    guard,
                    _outer: fee.retain(()),
                    _scope_fee: scope_fee,
                })
            }
            None => CheckedMatchingSafeAlternativeV1::Matched(PaidMatchingSafeAlternativeV1 {
                input,
                guard,
                _outer: fee.retain(()),
                _scope_fee: scope_fee,
            }),
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            overlay.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[path = "incremental_safe_alternative_finalize.rs"]
mod finalize;
#[path = "incremental_safe_alternative_rejection.rs"]
mod rejection;
pub(in crate::world) use finalize::{
    PreparedSafeAlternativeOutcomeRecordedV1, SafeAlternativeResultOwnerV1,
    apply_safe_alternative_outcome_v1,
};
pub(in crate::world) use rejection::{
    CompletedSafeAlternativeRejectionV1, publish_safe_alternative_rejection_v1,
};
