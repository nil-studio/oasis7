//! Frozen-bound Bootstrap DATA. The Chain caller keeps its real Node and FD
//! sources borrowed; this owner never grants Node execution permission.
use super::{
    incremental_body::BodyMutationError, incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_consensus::{
    node_consensus_action::NodeConsensusAction,
    normal_tick_input::PaidBoundNormalInputDataV1,
    replicated_execution_input::{
        self as wire, NodeReplicatedExecutionInputV1, ReplicatedExecutionInputValidationOpsV1,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        bounded_cbor_data::{self as cbor, CborDataLayoutV1, CborDataLimitsV1, PaidCborDataV1},
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use std::borrow::Borrow;

pub(in crate::world) struct BoundCurrentProviderBootstrapInputV1<'a, 'f> {
    input: &'a PaidBoundNormalInputDataV1<'f>,
    node: PaidCborDataV1<NodeReplicatedExecutionInputV1>,
    values: PaidCborDataV1<Vec<crate::ProviderBackedBootstrapAuthorityV1>>,
    core: WorkIdentityV1,
    index: usize,
    _fee: MemoryReservation,
}
#[derive(Debug)]
struct Refusal {
    note: String,
    _fee: MemoryReservation,
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.note)
    }
}
impl std::error::Error for Refusal {}
struct CodecRefusal(cbor::PaidCborRefusalV1, MemoryReservation);
impl std::fmt::Debug for CodecRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BootstrapCodec")
            .field(&self.0.message())
            .finish()
    }
}
impl std::fmt::Display for CodecRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.message())
    }
}
impl std::error::Error for CodecRefusal {}
fn original<E: std::error::Error + Send + Sync + 'static>(e: E) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Bootstrap actual Frozen input binding")
}
struct Ops<'a, 'w> {
    limits: &'a CborDataLimitsV1,
    common: &'a mut FormalBoundedContextV1<'w>,
}
impl Ops<'_, '_> {
    fn decode(
        &mut self,
        a: &NodeConsensusAction,
    ) -> Result<
        Result<Option<PaidCborDataV1<NodeReplicatedExecutionInputV1>>, String>,
        BodyMutationError,
    > {
        // Original kind.trim/version/payload predicates run after the paid decoder;
        // pay their byte traversal before entering that original pure kernel.
        self.common
            .charge(1, u64::try_from(a.payload_cbor.len()).map_err(|_| bad())?)
            .map_err(original)?;
        wire::decode_replicated_execution_input_action_with(a, |raw| {
            cbor::decode_closed_cbor_data_v1(
                raw,
                CborDataLayoutV1 {
                    root: std::mem::size_of::<NodeReplicatedExecutionInputV1>(),
                    sequence_element: 1,
                    map_pair: 0,
                    set_element: 0,
                    box_pointee: 0,
                    default_heap_per_value: 0,
                    symbols_bytes: 0,
                    visits: 1,
                    missing: None,
                },
                self.limits,
                self.common,
            )
            .map_err(original)
        })
    }
}
impl ReplicatedExecutionInputValidationOpsV1 for Ops<'_, '_> {
    type Owner = PaidCborDataV1<NodeReplicatedExecutionInputV1>;
    type Fatal = BodyMutationError;
    fn before_action(&mut self, a: &NodeConsensusAction) -> Result<(), Self::Fatal> {
        self.common
            .charge(1, a.submitter_player_id.len() as u64)
            .map_err(original)
    }
    fn decode_action(
        &mut self,
        a: &NodeConsensusAction,
    ) -> Result<Result<Option<Self::Owner>, String>, Self::Fatal> {
        self.decode(a)
    }
}
impl BoundCurrentProviderBootstrapInputV1<'_, '_> {
    pub(in crate::world) fn matches_input_index_v1(
        &self,
        input: &PaidBoundNormalInputDataV1<'_>,
        index: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        self.input.validate_context(common).map_err(original)?;
        input.validate_context(common).map_err(original)?;
        self.node.validate_context(common).map_err(original)?;
        self.values.validate_context(common).map_err(original)?;
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(bad());
        }
        Ok(std::ptr::eq(self.input, input) && self.index == index)
    }

    pub(in crate::world) fn inputs(&self) -> &[crate::ProviderBackedBootstrapAuthorityV1] {
        Borrow::<Vec<crate::ProviderBackedBootstrapAuthorityV1>>::borrow(&self.values)
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.input.validate_context(common).map_err(original)?;
        self.node.validate_context(common).map_err(original)?;
        self.values.validate_context(common).map_err(original)?;
        parent.validate_context(common).map_err(original)?;
        common.charge(1, 256).map_err(original)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || tx.parent_fence() != parent.fence()
            || self.input.checked().parent_record_id() != parent.refs().head.verified_record_id()
        {
            return Err(bad());
        }
        let node: &NodeReplicatedExecutionInputV1 = self.node.borrow();
        if node.target_height != self.input.context().height
            || node.target_height == 0
            || node.kind != wire::PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND
        {
            return Err(bad());
        }
        Ok(())
    }
}
/// Only the fixed Runtime driver can produce this borrowed DATA owner. The
/// unique original reserved-input kernel validates the entire list first.
pub(in crate::world) fn bind_current_provider_bootstrap_input_v1<'a, 'f>(
    input: &'a PaidBoundNormalInputDataV1<'f>,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<BoundCurrentProviderBootstrapInputV1<'a, 'f>>, BodyMutationError> {
    input.validate_context(common).map_err(original)?;
    let mut n = 4096usize;
    for action in &input.context().committed_actions {
        // The refusal-layout census itself is work, even when every row is
        // ordinary and the unique reserved-input kernel has nothing to decode.
        common.consume_steps(1).map_err(original)?;
        n = n
            .checked_add(action.submitter_player_id.len())
            .and_then(|n| n.checked_add(256))
            .ok_or_else(bad)?;
    }
    common.charge(1, n as u64).map_err(original)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(original)?;
    let mut ops = Ops { limits, common };
    if let Err(note) = wire::validate_replicated_execution_input_actions_with(
        &input.context().committed_actions,
        input.context().height,
        &mut ops,
    )? {
        return Err(original(Refusal { note, _fee: fee }));
    }
    for (index, action) in input.context().committed_actions.iter().enumerate() {
        ops.before_action(action)?;
        let node = match ops.decode(action)? {
            Ok(Some(node)) => node,
            Ok(None) => continue,
            Err(note) => return Err(original(Refusal { note, _fee: fee })),
        };
        let data: &NodeReplicatedExecutionInputV1 = node.borrow();
        ops.common
            .charge(1, data.kind.len() as u64)
            .map_err(original)?;
        if data.kind != wire::PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND {
            return Err(bad());
        }
        let values=match super::incremental_normal_provider_bootstrap_decode::decode_provider_bootstrap_data_v1(&data.payload_cbor,limits,ops.common).map_err(original)?{Ok(v)=>v,Err(e)=>return Err(original(CodecRefusal(e, fee)))};
        if Borrow::<Vec<crate::ProviderBackedBootstrapAuthorityV1>>::borrow(&values).is_empty() {
            return Err(bad());
        }
        return Ok(Some(BoundCurrentProviderBootstrapInputV1 {
            input,
            node,
            values,
            core: ops.common.work().identity(),
            index,
            _fee: fee,
        }));
    }
    Ok(None)
}
