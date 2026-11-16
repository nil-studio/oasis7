//! A real normal-tick retention participant. It inherits only the accepted
//! signed-role source and survives the whole FIFO/tail on the same exact TX.
use crate::world::{
    incremental_body::BodyMutationError, incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, RootSetV1},
    incremental_retention::RetentionLedgerManifestV1,
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::incremental_cognition_retention_ledger::{
    CurrentCognitionRetentionLedgerV1, FrozenCognitionRetentionLedgerV1,
    begin_cognition_retention_current_v1,
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("normal signed retention participant source")
}
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
pub(in crate::world) struct NormalRetentionParticipantV1 {
    current: CurrentCognitionRetentionLedgerV1,
    parent: [u8; 32],
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: MemoryReservation,
}
pub(in crate::world) struct FrozenNormalRetentionV1 {
    source: FrozenCognitionRetentionLedgerV1,
    parent: [u8; 32],
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl NormalRetentionParticipantV1 {
    pub(in crate::world) fn begin<M: MerkleNodeReader>(
        parent: &BoundNormalTickParentV1<'_>,
        tx: &mut ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::begin_inner(parent, tx, common, live)
        }));
        match outcome {
            Ok(result) => {
                if result.is_err() {
                    let _: Result<
                        (),
                        oasis7_formal_protocol::incremental_execution::ExecutionError,
                    > = tx.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                }
                result
            }
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    tx.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }

    fn begin_inner<M: MerkleNodeReader>(
        parent: &BoundNormalTickParentV1<'_>,
        tx: &mut ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        if parent.fence() != tx.parent_fence() || !tx.uses_budget(common.memory()) {
            return Err(bad());
        }
        // None is legacy missing, never the descriptor's explicit payload absence.
        let accepted = parent.refs().retention.ok_or(BodyMutationError::Invalid(
            "normal retention signed commitment missing",
        ))?;
        accepted.validate_binding_v1(parent.refs().head, parent.refs().roots, common)?;
        common
            .charge(
                1,
                std::mem::size_of::<Self>().max(std::mem::size_of::<FrozenNormalRetentionV1>())
                    as u64,
            )
            .map_err(formal)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<Self>().max(std::mem::size_of::<FrozenNormalRetentionV1>()),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let current =
            begin_cognition_retention_current_v1(accepted, parent.refs().head, tx, common, live)?;
        Ok(Self {
            current,
            parent: parent.refs().head.verified_record_id(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            fee,
        })
    }
    pub(in crate::world) fn freeze<M: MerkleNodeReader>(
        self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FrozenNormalRetentionV1, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        if self.parent != parent.refs().head.verified_record_id()
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        let source = self.current.freeze(tx, common, live)?;
        Ok(FrozenNormalRetentionV1 {
            source,
            parent: self.parent,
            core: self.core,
            memory: self.memory,
            fee: self.fee,
        })
    }
}
impl FrozenNormalRetentionV1 {
    pub(in crate::world) fn source(&self) -> &FrozenCognitionRetentionLedgerV1 {
        &self.source
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_context(common)?;
        self.source.validate_context(tx, common, live)?;
        Ok(())
    }
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(formal)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
            || self.parent != self.source.parent_record_data_v1()
        {
            return Err(bad());
        }
        Ok(())
    }
    /// The caller must first validate the actual terminal seal's owns-pointer
    /// witness. This accessor only produces paid descriptor DATA from that source.
    pub(in crate::world) fn manifest(
        &self,
        after: &RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<RetentionLedgerManifestV1>, BodyMutationError> {
        self.validate_context(common)?;
        let manifest = self.source.manifest(after, common)?;
        if manifest.previous_record != Some(self.parent) {
            return Err(bad());
        }
        Ok(manifest)
    }
}
#[path = "incremental_normal_retention_projection.rs"]
pub(in crate::world) mod projection;
