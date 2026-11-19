//! Original request/response validation and original action preimage. These
//! inputs do not imply that a retry, authority or full step has been admitted.
use super::*;
use oasis7_wasm_abi::CanonicalCborAllocator;
use std::io::{self, Read};
type Snapshot =
    crate::world::incremental_normal_projection_commitment::ActualCurrentStateHashSnapshotV1;
type Parent<'a> = crate::world::incremental_normal_sources::BoundNormalTickParentV1<'a>;
pub(in crate::world) struct CurrentCognitionCommitInputV1<'a> {
    request: &'a RuntimeCognitionCommitRequestV1,
    action: &'a crate::Action,
    response: &'a RuntimeCognitionResponseArtifactV1,
    action_value: ReservedValue<serde_json::Value>,
    response_value: ReservedValue<serde_json::Value>,
    action_digest: oasis7_agent_api::world_cognition::PaidCognitionDigestDataV1<MemoryReservation>,
    snapshot: &'a Snapshot,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CurrentCognitionCommitInputV1<'_> {
    pub(in crate::world) fn before_state_hash(&self) -> &str {
        self.snapshot.hash()
    }
    pub(in crate::world) fn before_manifest_hash(&self) -> &str {
        self.snapshot.manifest_hash_data_v1()
    }
    pub(in crate::world) fn request(&self) -> &RuntimeCognitionCommitRequestV1 {
        self.request
    }
    pub(in crate::world) fn action(&self) -> &crate::Action {
        self.action
    }
    pub(in crate::world) fn action_json(&self) -> &serde_json::Value {
        &self.action_value
    }
    pub(in crate::world) fn action_digest(&self) -> &str {
        self.action_digest.as_str()
    }
    pub(in crate::world) fn response_artifact_json(&self) -> &serde_json::Value {
        &self.response_value
    }
    pub(in crate::world) fn response(&self) -> &RuntimeCognitionResponseArtifactV1 {
        self.response
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &Parent<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.charge(6, 128).map_err(original)?;
        self.snapshot
            .validate_current_context(parent, tx, common, live)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !self.action_value.uses_budget(common.memory())
            || !self.response_value.uses_budget(common.memory())
        {
            return Err(bad("Current Cognition commit input context"));
        }
        Ok(())
    }
}
struct Input<'a, 'b, 'c> {
    bytes: &'a [u8],
    position: usize,
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    first: Option<BodyMutationError>,
}
impl Read for Input<'_, '_, '_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        let n = out.len().min(self.bytes.len() - self.position).min(8192);
        if let Err(e) = self.common.borrow_mut().charge(1, n as u64) {
            self.first = Some(original(e));
            return Err(io::ErrorKind::Other.into());
        }
        out[..n].copy_from_slice(&self.bytes[self.position..self.position + n]);
        self.position += n;
        Ok(n)
    }
}
#[derive(Debug)]
struct PaidJsonError {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Display for PaidJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for PaidJsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
fn value(
    bytes: &[u8],
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<ReservedValue<serde_json::Value>, BodyMutationError> {
    // This decoder is only serde_json::Value: every owned string byte appears in
    // the wire, containers are empty by default, and 2048/byte covers the maximum
    // Value/String/map-node/Vec capacity headers and original decoder scratch.
    let n = bytes
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(2048))
        .and_then(|n| n.checked_add(65536))
        .ok_or_else(|| bad("Cognition action JSON layout"))?;
    common.borrow_mut().charge(1, n as u64).map_err(original)?;
    let phase = common
        .borrow()
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut input = Input {
        bytes,
        position: 0,
        common,
        first: None,
    };
    let decoded = serde_json::from_reader(&mut input);
    if let Some(first) = input.first {
        return Err(first);
    }
    common.borrow_mut().consume_steps(0).map_err(original)?;
    match decoded {
        Ok(value) => Ok(phase.retain(value)),
        Err(error) => Err(original(PaidJsonError { error, _fee: phase })),
    }
}
pub(in crate::world) fn prepare_current_cognition_commit_inputs_v1<'a, M: MerkleNodeReader>(
    request: &'a RuntimeCognitionCommitRequestV1,
    action: &'a crate::Action,
    response: &'a RuntimeCognitionResponseArtifactV1,
    snapshot: &'a Snapshot,
    parent: &Parent<'_>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    live: &ExecutionFence,
) -> Result<CurrentCognitionCommitInputV1<'a>, CurrentCognitionCommitErrorV1> {
    snapshot.validate_current_context(parent, tx, common, live)?;
    let common = RefCell::new(common);
    let source = work::Source::new(&common, canonical);
    // Same original request predicate. The measured original serializer counts
    // every real field/string before the existing trim/identifier checks run.
    let raw =
        crate::world::incremental_normal_projection_commitment::capture_projection_json_data_v1(
            request,
            &mut common.borrow_mut(),
        )?;
    source
        .result(source.charge_work((raw.len() as u64).saturating_mul(4) + 64, raw.len() as u64))?;
    if let Err(error) = request.validate() {
        return Err(source.refusal(error.code())?);
    }
    if let Err(error) =
        source.result(response.validate_for_request_with_work_v1(request, &source))?
    {
        return Err(source.refusal(error.code())?);
    }
    let raw_action =
        crate::world::incremental_normal_projection_commitment::capture_projection_json_data_v1(
            action,
            &mut common.borrow_mut(),
        )?;
    let action_value = value(&raw_action, &common)?;
    let response_value = source.result(
        crate::world::cognition_persistence::response_artifact_with_outer_lineage_with_work_v1(
            response, request, &source,
        ),
    )?;
    let response_value = match response_value {
        Ok((value, fee)) => fee.retain(value),
        Err((error, fee)) => {
            return Err(CurrentCognitionCommitErrorV1::Body(original(
                PaidJsonError { error, _fee: fee },
            )));
        }
    };
    let action_digest = source.result(
        oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
            "oasis7.cognition.action.v1",
            &*action_value,
            &source,
        ),
    )?;
    let n = size_of::<CurrentCognitionCommitInputV1<'_>>();
    source.result(source.charge_work(1, n as u64))?;
    let fee = source.result(source.reserve(n))?;
    let c = common.borrow();
    let output = CurrentCognitionCommitInputV1 {
        request,
        action,
        response,
        action_value,
        response_value,
        action_digest,
        snapshot,
        guard: tx.guard(),
        core: c.work().identity(),
        memory: c.memory().clone(),
        _fee: fee,
    };
    drop(c);
    output.validate(parent, tx, &mut common.borrow_mut(), live)?;
    Ok(output)
}

/// Consumed before the first Cognition write. The retained pre-write bytes and
/// guards are historical DATA afterwards, never refreshed Current evidence or
/// a declaration that prefix/admission/action/step execution has succeeded.
pub(in crate::world) struct HistoricalCurrentCognitionInputV1<'a> {
    input: CurrentCognitionCommitInputV1<'a>,
    before_guard: PreparedGuard,
    parent_record: [u8; 32],
    parent_body: [u8; 32],
    parent_roots: [u8; 32],
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl<'a> CurrentCognitionCommitInputV1<'a> {
    pub(in crate::world) fn into_historical_before_write_v1<M: MerkleNodeReader>(
        self,
        parent: &Parent<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<HistoricalCurrentCognitionInputV1<'a>, BodyMutationError> {
        self.validate(parent, tx, common, live)?;
        let n = size_of::<HistoricalCurrentCognitionInputV1<'_>>();
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let refs = parent.refs();
        let output = HistoricalCurrentCognitionInputV1 {
            input: self,
            before_guard: tx.guard(),
            parent_record: refs.head.verified_record_id(),
            parent_body: refs.runtime.occurrence().head_body_id(),
            parent_roots: refs.core.body().roots,
            core: common.work().identity(),
            memory: common.memory().clone(),
            _fee: fee,
        };
        output.validate_historical(parent, tx, common, live)?;
        Ok(output)
    }
}
impl<'a> HistoricalCurrentCognitionInputV1<'a> {
    pub(in crate::world) fn before_inputs(&self) -> &CurrentCognitionCommitInputV1<'a> {
        &self.input
    }
    pub(in crate::world) fn validate_historical<M: MerkleNodeReader>(
        &self,
        parent: &Parent<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(original)?;
        common.charge(8, 256).map_err(original)?;
        tx.validate_exact_transaction_identity(&self.before_guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        let refs = parent.refs();
        if self.core != common.work().identity()
            || self.input.core != self.core
            || !self.memory.same_pool(common.memory())
            || !self.input.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !self.input._fee.uses_budget(common.memory())
            || !self.input.action_value.uses_budget(common.memory())
            || !self.input.response_value.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || self.parent_record != refs.head.verified_record_id()
            || self.parent_body != refs.runtime.occurrence().head_body_id()
            || self.parent_roots != refs.core.body().roots
            || tx.parent_fence().parent_commit != self.parent_body
            || tx.parent_fence().parent_roots != self.parent_roots
        {
            return Err(bad("historical Cognition before-input identity"));
        }
        Ok(())
    }
}
