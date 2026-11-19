//! Signed Frozen content consistency, not a Node decision or resume permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    CheckedIncrementalFrozenMaterialV1, FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

pub struct PaidBoundNormalInputDataV1<'f> {
    checked: ReservedValue<CheckedIncrementalFrozenMaterialV1<'f>>,
    originals: PaidNormalContextOriginalsV1,
    identity: WorkIdentityV1,
    _outer: ReservedValue<()>,
}
impl PaidBoundNormalInputDataV1<'_> {
    pub fn context(&self) -> &NodeExecutionCommitContext {
        self.originals.context()
    }
    pub fn continuation(&self) -> &NodeLocalExecutionContinuation {
        self.originals.continuation()
    }
    pub fn checked(&self) -> &CheckedIncrementalFrozenMaterialV1<'_> {
        &self.checked
    }
    pub fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if self.identity != ctx.work().identity()
            || !self.checked.uses_budget(ctx.memory())
            || !self.originals.uses_budget(ctx.memory())
            || !self._outer.uses_budget(ctx.memory())
        {
            return Err(FormalError::Invalid("normal input data pool/Core".into()));
        }
        Ok(())
    }
}
pub fn bind_normal_input_data_bounded<'f>(
    checked: ReservedValue<CheckedIncrementalFrozenMaterialV1<'f>>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidBoundNormalInputDataV1<'f>, FormalError> {
    let invalid = || FormalError::Invalid("normal input Frozen/context binding".into());
    ctx.consume_steps(1)?;
    if !checked.uses_budget(ctx.memory()) {
        return Err(invalid());
    }
    let originals = read_normal_context_originals_bounded(&checked, ctx)?;
    let c = originals.context();
    let saved = originals.continuation();
    let total = c.committed_actions.iter().try_fold(0usize, |sum, action| {
        let bytes = action
            .submitter_player_id
            .len()
            .checked_add(action.payload_hash.len())
            .and_then(|n| n.checked_add(action.payload_cbor.len()))
            .ok_or_else(invalid)?;
        ctx.charge(1, u64::try_from(bytes).map_err(|_| invalid())?)?;
        sum.checked_add(action.payload_cbor.len())
            .ok_or_else(invalid)
    })?;
    ctx.charge(
        1,
        checked
            .input()
            .byte_len
            .checked_add(checked.continuation().byte_len)
            .ok_or_else(invalid)?,
    )?;
    if c != &saved.context
        || saved.schema_version != 1
        || c.world_id != checked.reservation().body().world.world_id
        || c.node_id != c.proposer_id
        || saved.predecessor_execution_height != saved.predecessor_committed_height
        || saved.predecessor_committed_height.checked_add(1) != Some(c.height)
        || saved.predecessor_execution_state_root.as_deref()
            != Some(checked.parent().record().after_state_root.as_str())
        || total != saved.reserved_action_bytes
    {
        return Err(invalid());
    }
    let root = crate::node_consensus_action::compute_consensus_action_root_bounded(
        &c.committed_actions,
        ctx,
    )?;
    ctx.charge(1, c.action_root.len() as u64)?;
    if *root != c.action_root {
        return Err(invalid());
    }
    let fee = ctx
        .memory()
        .reserve(
            std::mem::size_of::<PaidBoundNormalInputDataV1<'f>>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid())?;
    ctx.consume_steps(1)?;
    Ok(PaidBoundNormalInputDataV1 {
        checked,
        originals,
        identity: ctx.work().identity(),
        _outer: fee.retain(()),
    })
}
