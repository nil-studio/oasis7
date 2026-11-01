//! Original capability execution records. This does not change authorization,
//! generate a nonce, alter effect identity or dispatch an external operation.
use super::World;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_runtime_capability_records::{
        RuntimeCapabilityRecordsRefV1, RuntimeCapabilityRecordsWorkV1,
        prepare_runtime_capability_records_genesis_v1,
    },
};
impl World {
    /// Borrow the three actual maps; output remains provisional until all owners,
    /// receiver verification and the existing commit gate accept the candidate.
    pub fn write_iws_capability_execution_genesis<
        W: ValueNodeReader + StatePageWriter,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        context: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeCapabilityRecordsWorkV1,
        sink: &mut S,
    ) -> Result<u64, MemberRecordError> {
        prepare_runtime_capability_records_genesis_v1(
            &RuntimeCapabilityRecordsRefV1 {
                nonces: &self.capability_nonce_records,
                budgets: &self.capability_budget_accounts,
                effect_links: &self.capability_effect_receipt_links,
            },
            context,
            work,
            sink,
        )
    }
}

use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_capability_records::*,
};
impl World {
    fn iws_capability_record(
        &self,
        key: RuntimeCapabilityRecordKeyV1<'_>,
    ) -> Option<RuntimeCapabilityRecordRefV1<'_>> {
        match key.kind {
            RuntimeCapabilityRecordKindV1::Nonce => self
                .capability_nonce_records
                .get(key.id)
                .map(RuntimeCapabilityRecordRefV1::Nonce),
            RuntimeCapabilityRecordKindV1::Budget => self
                .capability_budget_accounts
                .get(key.id)
                .map(RuntimeCapabilityRecordRefV1::Budget),
            RuntimeCapabilityRecordKindV1::EffectLink => self
                .capability_effect_receipt_links
                .get(key.id)
                .map(RuntimeCapabilityRecordRefV1::EffectLink),
        }
    }
    /// Lower one selected field from the actual prepared/current Runtime row.
    /// Other authorization root/events/effects remain required driver contributions.
    pub fn contribute_iws_capability_cell<
        'a,
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        CW: FnMut() -> bool,
    >(
        &'a self,
        mut change: RuntimeCapabilityPointChangeV1<'a>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<
        Option<ReservedValue<RuntimeCapabilityRecordQualificationV1>>,
        WorldFieldOverlayErrorV1,
    > {
        change.after = context.transaction.contribute(|tx| {
            tx.validate_guard(guard, live)?;
            self.iws_capability_record(change.request.key)
                .ok_or(ValueTreeError::Invalid(
                    "Runtime capability current row absent",
                ))?
                .column_value(change.request.column)
                .map_err(WorldFieldOverlayErrorV1::Value)
        })?;
        change_qualified_runtime_capability_cell_v1(change, context, write, guard, live)
    }
    pub fn contribute_iws_capability_subject<
        'a,
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        CW: FnMut() -> bool,
    >(
        &'a self,
        mut change: RuntimeCapabilitySubjectChangeV1<'a>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<
        Option<ReservedValue<RuntimeCapabilityRecordQualificationV1>>,
        WorldFieldOverlayErrorV1,
    > {
        change.after = context.transaction.contribute(|tx| {
            tx.validate_guard(guard, live)?;
            match self.iws_capability_record(change.request.key) {
                Some(RuntimeCapabilityRecordRefV1::Budget(value)) => {
                    Ok::<_, WorldFieldOverlayErrorV1>(&value.subject)
                }
                _ => {
                    Err(ValueTreeError::Invalid("Runtime capability current subject absent").into())
                }
            }
        })?;
        change_qualified_runtime_capability_subject_v1(change, context, write, guard, live)
    }
    pub fn contribute_iws_capability_creation<
        'a,
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter + ValueNodeReader,
        CW: FnMut() -> bool,
    >(
        &'a self,
        mut request: RuntimeCapabilityCreateV1<'a>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        work: &mut RuntimeCapabilityRecordsWorkV1,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<ReservedValue<RuntimeCapabilityRecordQualificationV1>, WorldFieldOverlayErrorV1>
    {
        request.source = context.transaction.contribute(|tx| {
            tx.validate_guard(guard, live)?;
            self.iws_capability_record(request.key).ok_or_else(|| {
                WorldFieldOverlayErrorV1::from(ValueTreeError::Invalid(
                    "Runtime capability current create row absent",
                ))
            })
        })?;
        create_qualified_runtime_capability_record_v1(request, context, write, work, guard, live)
    }
    pub fn contribute_iws_capability_deletion<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        &self,
        request: RuntimeCapabilityDeleteV1<'_>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        context.transaction.contribute(|tx| {
            tx.validate_guard(guard, live)?;
            if self.iws_capability_record(request.key).is_some() {
                Err::<(), WorldFieldOverlayErrorV1>(
                    ValueTreeError::Invalid("Runtime capability current row is not deleted").into(),
                )
            } else {
                Ok(())
            }
        })?;
        delete_qualified_runtime_capability_record_v1(request, context, guard, live)
    }
}
