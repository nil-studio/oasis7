//! Selected source -> actual guarded sandbox execution. This stage deliberately
//! returns charged output to the shared policy/fee/effect application kernel;
//! execution alone never grants publication or an accepted head.
use super::*;
use crate::world::incremental_module_execution_output::{
    RootModuleOutputErrorV1, RootModuleOutputWorkV1, call_admitted_root_module_output_v1,
};
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
use oasis7_wasm_abi::{
    ModuleCallRequest, ModuleOutput, module_raw_output::ModuleSandboxRawOutputV1,
};

/// The prepared selection is borrowed through the whole real backend call:
/// the actual WASM Arc cannot outlive its source charge via this interface.
/// `input` must be assembled from the actual due/command context by the caller;
/// its additional request copy is admitted here before allocation.
#[expect(
    clippy::too_many_arguments,
    reason = "Real selection, transaction fence, sandbox, input and independent finite work remain explicit."
)]
pub fn execute_selected_root_module_output_v1<M, S, C>(
    selected: &PreparedRootModuleInvocationV1,
    txn: &mut ExecutionTransaction<'_, M>,
    live: &ExecutionFence,
    trace_id: &str,
    input: &[u8],
    sandbox: &mut S,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &mut RootModuleOutputWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<ModuleOutput>, RootModuleOutputErrorV1>
where
    M: MerkleNodeReader,
    S: ModuleSandboxRawOutputV1,
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(RootModuleOutputErrorV1::Value(ValueTreeError::Cancelled));
    }
    if !txn.uses_budget(memory)
        || !selected.manifest.uses_budget(memory)
        || selected.manifest.allocation_class() != MemoryClass::Ordinary
        || selected.parent != txn.parent_fence().parent_roots
        || selected.module_id.len() > work.max_identity_bytes
        || trace_id.len() > work.max_identity_bytes
    {
        return Err(RootModuleOutputErrorV1::Value(invalid()));
    }
    txn.validate_guard(&selected.guard, live)
        .map_err(|_| RootModuleOutputErrorV1::Value(invalid()))?;
    let manifest = selected.manifest.manifest();
    let bytes = input
        .len()
        .checked_add(selected.module_id.len())
        .and_then(|n| n.checked_add(trace_id.len()))
        .and_then(|n| n.checked_add(manifest.wasm_hash.len()))
        .and_then(|n| n.checked_add(manifest.kind.entrypoint().len()))
        .and_then(|n| n.checked_add(std::mem::size_of::<ModuleCallRequest>() + 256))
        .ok_or(RootModuleOutputErrorV1::Value(
            ValueTreeError::ResourceLimited("selected module call request"),
        ))?;
    let _request_charge = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|e| RootModuleOutputErrorV1::Value(ValueTreeError::Memory(e)))?;
    let request = ModuleCallRequest {
        module_id: selected.module_id.clone(),
        wasm_hash: manifest.wasm_hash.clone(),
        trace_id: trace_id.to_owned(),
        entrypoint: manifest.kind.entrypoint().to_owned(),
        input: input.to_vec(),
        limits: manifest.limits.clone(),
        wasm_bytes: Arc::clone(&selected.wasm),
    };
    let result =
        call_admitted_root_module_output_v1(sandbox, &request, memory, work, &mut cancelled);
    if cancelled() {
        return Err(RootModuleOutputErrorV1::Value(ValueTreeError::Cancelled));
    }
    txn.validate_guard(&selected.guard, live)
        .map_err(|_| RootModuleOutputErrorV1::Value(invalid()))?;
    result
}
