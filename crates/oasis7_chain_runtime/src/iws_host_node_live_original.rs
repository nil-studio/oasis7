//! Paid copies of a continuation received from the actual Node callback. These
//! are restart DATA, never an alternate mint for verified Node provenance.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1;
use oasis7_node::NodeConsensusAction;

pub(super) struct PaidNodeOriginalV1 {
    value: ReservedValue<NodeLocalExecutionContinuation>,
    core: WorkIdentityV1,
    returned: MemoryReservation,
    max_bytes: usize,
}

fn bad() -> FormalError {
    FormalError::Invalid("live Node original continuation pool/Core/layout changed".into())
}

fn add(
    bytes: &mut usize,
    extra: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), FormalError> {
    common.consume_steps(1)?;
    *bytes = bytes.checked_add(extra).ok_or_else(bad)?;
    Ok(())
}

fn context_layout(
    value: &NodeExecutionCommitContext,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, FormalError> {
    let mut bytes = size_of::<NodeExecutionCommitContext>();
    for text in [
        &value.world_id,
        &value.node_id,
        &value.proposer_id,
        &value.node_block_hash,
        &value.action_root,
    ] {
        add(&mut bytes, text.len(), common)?;
    }
    add(
        &mut bytes,
        value
            .committed_actions
            .len()
            .checked_mul(size_of::<NodeConsensusAction>())
            .ok_or_else(bad)?,
        common,
    )?;
    for action in &value.committed_actions {
        add(&mut bytes, action.submitter_player_id.len(), common)?;
        add(&mut bytes, action.payload_cbor.len(), common)?;
        add(&mut bytes, action.payload_hash.len(), common)?;
    }
    Ok(bytes)
}

fn layout(
    value: &NodeLocalExecutionContinuation,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, FormalError> {
    let mut bytes = context_layout(&value.context, common)?;
    add(
        &mut bytes,
        size_of::<NodeLocalExecutionContinuation>() - size_of::<NodeExecutionCommitContext>(),
        common,
    )?;
    for text in [
        value.predecessor_node_block_hash.as_deref(),
        value.predecessor_execution_block_hash.as_deref(),
        value.predecessor_execution_state_root.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        add(&mut bytes, text.len(), common)?;
    }
    if bytes > max {
        return Err(bad());
    }
    Ok(bytes)
}

fn copy_bytes(
    source: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Vec<u8>, FormalError> {
    common.consume_steps(1)?;
    let mut out = Vec::new();
    out.try_reserve_exact(source.len()).map_err(|_| bad())?;
    if out.capacity() > source.len() {
        return Err(bad());
    }
    for chunk in source.chunks(8192) {
        common.charge(1, chunk.len() as u64)?;
        out.extend_from_slice(chunk);
    }
    Ok(out)
}

fn copy_text(source: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String, FormalError> {
    let bytes = copy_bytes(source.as_bytes(), common)?;
    common.charge(1, bytes.len() as u64)?;
    String::from_utf8(bytes).map_err(|_| bad())
}

fn copy_original(
    source: &NodeLocalExecutionContinuation,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(NodeLocalExecutionContinuation, MemoryReservation), FormalError> {
    let bytes = layout(source, max, common)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| bad())?;
    let value = &source.context;
    let mut actions = Vec::new();
    actions
        .try_reserve_exact(value.committed_actions.len())
        .map_err(|_| bad())?;
    if actions.capacity() > value.committed_actions.len() {
        return Err(bad());
    }
    for action in &value.committed_actions {
        common.consume_steps(1)?;
        actions.push(NodeConsensusAction {
            action_id: action.action_id,
            submitter_player_id: copy_text(&action.submitter_player_id, common)?,
            payload_cbor: copy_bytes(&action.payload_cbor, common)?,
            payload_hash: copy_text(&action.payload_hash, common)?,
        });
    }
    let context = NodeExecutionCommitContext {
        world_id: copy_text(&value.world_id, common)?,
        node_id: copy_text(&value.node_id, common)?,
        proposer_id: copy_text(&value.proposer_id, common)?,
        height: value.height,
        slot: value.slot,
        epoch: value.epoch,
        node_block_hash: copy_text(&value.node_block_hash, common)?,
        action_root: copy_text(&value.action_root, common)?,
        committed_actions: actions,
        committed_at_unix_ms: value.committed_at_unix_ms,
    };
    Ok((
        NodeLocalExecutionContinuation {
            schema_version: source.schema_version,
            context,
            predecessor_committed_height: source.predecessor_committed_height,
            predecessor_node_block_hash: source
                .predecessor_node_block_hash
                .as_deref()
                .map(|s| copy_text(s, common))
                .transpose()?,
            predecessor_execution_height: source.predecessor_execution_height,
            predecessor_execution_block_hash: source
                .predecessor_execution_block_hash
                .as_deref()
                .map(|s| copy_text(s, common))
                .transpose()?,
            predecessor_execution_state_root: source
                .predecessor_execution_state_root
                .as_deref()
                .map(|s| copy_text(s, common))
                .transpose()?,
            next_height: source.next_height,
            next_slot: source.next_slot,
            reserved_action_bytes: source.reserved_action_bytes,
            approved_stake: source.approved_stake,
            rejected_stake: source.rejected_stake,
            required_stake: source.required_stake,
            total_stake: source.total_stake,
        },
        fee,
    ))
}

impl PaidNodeOriginalV1 {
    pub(super) fn capture(
        source: &NodeLocalExecutionContinuation,
        max_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        let (value, fee) = copy_original(source, max_bytes, common)?;
        Ok(Self {
            value: fee.retain(value),
            core: common.work().identity(),
            returned: common
                .memory()
                .reserve(0, MemoryClass::Ordinary)
                .map_err(|_| bad())?,
            max_bytes,
        })
    }

    pub(super) fn value(&self) -> &NodeLocalExecutionContinuation {
        &self.value
    }

    fn validate(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        if self.core != common.work().identity()
            || !self.value.uses_budget(common.memory())
            || !self.returned.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn validate_same(
        &self,
        value: &NodeLocalExecutionContinuation,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        self.validate(common)?;
        let bytes = layout(value, self.max_bytes, common)?
            .checked_add(layout(&self.value, self.max_bytes, common)?)
            .ok_or_else(bad)?;
        common.charge(1, u64::try_from(bytes).map_err(|_| bad())?)?;
        if *self.value != *value {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn validate_callback(
        &self,
        value: &NodeExecutionCommitContext,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        self.validate(common)?;
        let bytes = context_layout(value, common)?
            .checked_add(context_layout(&self.value.context, common)?)
            .ok_or_else(bad)?;
        common.charge(1, u64::try_from(bytes).map_err(|_| bad())?)?;
        if self.value.context != *value {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn copy_bounded(
        &mut self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NodeLocalExecutionContinuation, FormalError> {
        self.validate(common)?;
        let (value, mut fee) = copy_original(&self.value, self.max_bytes, common)?;
        // Node returns the original through completion before this owner drops.
        self.returned.merge(&mut fee).map_err(|_| bad())?;
        Ok(value)
    }
}
