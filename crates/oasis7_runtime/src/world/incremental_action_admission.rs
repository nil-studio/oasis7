//! Common admission for actual selected FIFO input. A safe-alternative match
//! remains a separate scope requirement, never an implicit allow.
use super::incremental_action_execution::SelectedPendingActionV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        CurrentSafeAlternativeRecordV1, RevocationReadWorkV1, RuntimeRevocationCurrentV1,
        RuntimeRevocationQualificationV1, read_current_safe_alternative_for_action_v1,
    },
};

#[derive(Debug)]
pub(super) enum ActionAdmissionErrorV1 {
    Execution(ExecutionError),
    Overlay(WorldFieldOverlayErrorV1),
    Original(crate::WorldError),
    QualifiedGovernanceOwnerRequired,
}
impl From<ExecutionError> for ActionAdmissionErrorV1 {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl From<WorldFieldOverlayErrorV1> for ActionAdmissionErrorV1 {
    fn from(error: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(error)
    }
}
/// This proves only the original common guard for an unprotected envelope with
/// no matching alternative at this exact revision. Any later revision requires
/// a genuine Current re-read. Rule, economy, business and settlement remain due.
pub(super) struct UnboundActionAdmissionV1<'selected> {
    selected: &'selected SelectedPendingActionV1,
    identity: PreparedGuard,
    memory: MemoryBudget,
}
impl UnboundActionAdmissionV1<'_> {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        selected.validate(tx)?;
        tx.validate_guard(&self.identity, live)?;
        if !std::ptr::eq(self.selected, selected)
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
        {
            return Err(ExecutionError::Invalid("selected common admission binding"));
        }
        Ok(())
    }
    /// Historical observation of a real admission. This does not re-admit an
    /// action after writes; only the fixed pre-Rule module rejection path uses
    /// it to retain the original step's already-passed common guard.
    pub(super) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        selected.validate(tx)?;
        tx.validate_transaction_identity(&self.identity)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !std::ptr::eq(self.selected, selected)
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
        {
            return Err(ExecutionError::Invalid(
                "historical selected admission binding",
            ));
        }
        Ok(())
    }
}
/// Fixed Core scope alternatives. A matching token can never become Unbound.
pub(super) enum CoreActionAdmissionV1<'borrow, 'selected, 'source> {
    Unbound(&'borrow UnboundActionAdmissionV1<'selected>),
    Matched(
        &'borrow super::incremental_safe_alternative_matching::PaidMatchingSafeAlternativeV1<
            'selected,
            'source,
        >,
    ),
}
impl CoreActionAdmissionV1<'_, '_, '_> {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        common:Option<&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>>,
        live: &ExecutionFence,
    ) -> Result<(), super::incremental_body::BodyMutationError> {
        match self {
            Self::Unbound(value) => value
                .validate(selected, tx, memory, live)
                .map_err(Into::into),
            Self::Matched(value) => value.validate(
                selected,
                tx,
                memory,
                common.ok_or(super::incremental_body::BodyMutationError::Invalid(
                    "matching SafeAlternative Common missing",
                ))?,
                live,
            ),
        }
    }
}
/// The record keeps its original data charge. Its fixed storage uses the
/// admission already acquired before the Current read, including when this
/// owner is moved into a scope-required error. Drop the box before that charge.
pub(super) struct PaidSafeAlternativeAdmissionRecordV1 {
    record: Box<ReservedValue<CurrentSafeAlternativeRecordV1>>,
    _storage: MemoryReservation,
}
impl PaidSafeAlternativeAdmissionRecordV1 {
    fn storage_bytes() -> usize {
        std::mem::size_of::<ReservedValue<CurrentSafeAlternativeRecordV1>>()
            + std::mem::size_of::<Self>()
    }

    fn from_admitted(
        record: ReservedValue<CurrentSafeAlternativeRecordV1>,
        storage: MemoryReservation,
    ) -> Self {
        assert!(storage.bytes() >= Self::storage_bytes());
        Self {
            record: Box::new(record),
            _storage: storage,
        }
    }

    /// The signed-record consumer takes the same original paid record. Moving
    /// it out releases this box before returning its fixed storage charge.
    pub(super) fn into_record(self) -> ReservedValue<CurrentSafeAlternativeRecordV1> {
        let Self {
            record,
            _storage: storage,
        } = self;
        let original = {
            let boxed = record;
            *boxed
        };
        drop(storage);
        original
    }
}
impl std::ops::Deref for PaidSafeAlternativeAdmissionRecordV1 {
    type Target = ReservedValue<CurrentSafeAlternativeRecordV1>;

    fn deref(&self) -> &Self::Target {
        &self.record
    }
}

pub(super) enum SelectedActionAdmissionV1<'selected> {
    Unbound(ReservedValue<UnboundActionAdmissionV1<'selected>>),
    RequiresSafeAlternativeScope(PaidSafeAlternativeAdmissionRecordV1),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn admit_selected_action_v1<'selected, M, R, E, C, P, V, ER, CR>(
    selected: &'selected SelectedPendingActionV1,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<SelectedActionAdmissionV1<'selected>, ActionAdmissionErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        overlay.transaction.validate_guard(guard, live)?;
        selected.validate(overlay.transaction)?;
        if !super::governance_authorization::is_unprotected_governance_envelope(selected.envelope())
            .map_err(ActionAdmissionErrorV1::Original)?
        {
            return Err(ActionAdmissionErrorV1::QualifiedGovernanceOwnerRequired);
        }
        let fee = overlay
            .values
            .memory
            .reserve(
                1024.max(PaidSafeAlternativeAdmissionRecordV1::storage_bytes()),
                MemoryClass::Ordinary,
            )
            .map_err(|error| ActionAdmissionErrorV1::Execution(ExecutionError::Memory(error)))?;
        let record = read_current_safe_alternative_for_action_v1(
            selected.envelope().id,
            qualification,
            current,
            overlay,
            read,
            scan,
            work,
            guard,
            live,
        )?;
        if let Some(record) = record {
            record.validate(overlay.transaction, live)?;
            return Ok(SelectedActionAdmissionV1::RequiresSafeAlternativeScope(
                PaidSafeAlternativeAdmissionRecordV1::from_admitted(record, fee),
            ));
        }
        Ok(SelectedActionAdmissionV1::Unbound(fee.retain(
            UnboundActionAdmissionV1 {
                selected,
                identity: overlay.transaction.guard(),
                memory: overlay.values.memory.clone(),
            },
        )))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = overlay
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
