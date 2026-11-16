//! Immutable original World source for the independent signed retention DATA.
use super::World;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::RootSetV1,
    incremental_retention::RetentionLedgerManifestV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_retention_ledger::{
        CognitionRetentionWorkV1, prepare_cognition_retention_ledger_genesis_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
};
impl World {
    pub fn prepare_iws_cognition_retention_ledger_genesis_v1<
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
    >(
        &self,
        roots: &RootSetV1,
        write: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut CognitionRetentionWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<RetentionLedgerManifestV1>, ValueTreeError> {
        common.charge(8192, 8192).map_err(|e| match e {
            oasis7_formal_protocol::controlled_authority::formal::FormalError::Invalid(ref s)
                if s == "bounded archive read cancelled" =>
            {
                ValueTreeError::Cancelled
            }
            _ => ValueTreeError::ResourceLimited("retention GEN Common roots"),
        })?;
        let _phase = common.memory().reserve(8192, MemoryClass::Ordinary)?;
        roots
            .validate()
            .map_err(|_| ValueTreeError::Invalid("retention GEN roots"))?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        let envelope = if self.cognition().is_null() {
            None
        } else {
            Some(
                self.borrow_iws_cognition_envelope_v1()
                    .map_err(ValueTreeError::Invalid)?,
            )
        };
        prepare_cognition_retention_ledger_genesis_v1(
            envelope,
            roots
                .digest()
                .map_err(|_| ValueTreeError::Invalid("retention GEN roots"))?,
            write,
            work,
            common,
        )
    }
}
