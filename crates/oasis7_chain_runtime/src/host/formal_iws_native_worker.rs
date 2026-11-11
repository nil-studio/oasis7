//! Accepted native lease scope for the actual Runtime target, not a raw request executor.
use oasis7_distfs::controlled_authority::formal::{FormalError, endpoint::FileFormalEndpoint};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{VerifiedAuthorityV1, VerifiedFormalCommitV1, incremental::authority_binding},
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_runtime::{
    RootModuleExecutionErrorV1, incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
};
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, WorkerExecutableV1, WorkerLimitsV1,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub(super) enum NativeRootScopeErrorV1<OE> {
    Material(FormalError),
    Execution(ExecutionError),
    Invocation(RootModuleExecutionErrorV1<OE>),
}
impl<OE: std::fmt::Debug> std::fmt::Display for NativeRootScopeErrorV1<OE> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Material(error) => write!(formatter, "native material: {error}"),
            Self::Execution(error) => write!(formatter, "native transaction: {error:?}"),
            Self::Invocation(error) => write!(formatter, "native target: {error:?}"),
        }
    }
}
impl<OE> From<FormalError> for NativeRootScopeErrorV1<OE> {
    fn from(error: FormalError) -> Self {
        Self::Material(error)
    }
}
impl<OE> From<ExecutionError> for NativeRootScopeErrorV1<OE> {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}

pub(super) struct NativeWorkerCallContextV1<'a> {
    pub endpoint: &'a FileFormalEndpoint,
    pub commit: &'a VerifiedFormalCommitV1,
    pub authority: &'a VerifiedAuthorityV1,
    pub memory: &'a MemoryBudget,
    pub limits: WorkerLimitsV1,
    pub cancelled: &'a AtomicBool,
}

fn validate_parent<M: MerkleNodeReader>(
    commit: &VerifiedFormalCommitV1,
    authority: &VerifiedAuthorityV1,
    memory: &MemoryBudget,
    cancelled: &AtomicBool,
    transaction: &mut ExecutionTransaction<'_, M>,
    live: &ExecutionFence,
) -> Result<(), ExecutionError> {
    let result =
        (|| {
            if cancelled.load(Ordering::Acquire) || !transaction.uses_budget(memory) {
                return Err(ExecutionError::Invalid(
                    "native Runtime worker cancellation/pool",
                ));
            }
            let root =
                |role| -> Result<[u8; 32], ExecutionError> {
                    let value = commit.record().artifact_roots.get(&role).ok_or(
                        ExecutionError::Invalid("native worker accepted IWS parent missing"),
                    )?;
                    let mut digest = [0; 32];
                    hex::decode_to_slice(value, &mut digest).map_err(|_| {
                        ExecutionError::Invalid("native worker accepted parent digest")
                    })?;
                    Ok(digest)
                };
            let parent = transaction.parent_fence();
            if parent.parent_commit != root(ArtifactRole::IncrementalCommitBody)?
                || parent.parent_roots != root(ArtifactRole::IncrementalRootSet)?
                || parent.authority
                    != authority_binding(authority, memory).map_err(|_| {
                        ExecutionError::Invalid("native worker actual authority binding")
                    })?
            {
                return Err(ExecutionError::Invalid(
                    "native worker exact accepted transaction parent",
                ));
            }
            transaction.validate_guard(&transaction.guard(), live)
        })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// This private Host boundary is called inside the sole Runtime apply closure.
/// The allocator is made here from the transaction's pool. The accepted endpoint
/// retains its complete-data borrow, sealed executable and descriptor pins until
/// the actual Runtime target and every bounded child wait have returned.
///
/// The callback must construct its value/event contexts inside this scope; the
/// worker lifetime is deliberately not a caller-selected value-context lifetime.
/// Runtime's scoped target entry performs the real selection and request, then
/// its request-bound hook verifies the native protocol before the typed call.
/// No worker/lease or accepted/cache-install capability escapes this function.
pub(super) fn with_accepted_root_module_scope_v1<'source, M, T, OE>(
    context: NativeWorkerCallContextV1<'_>,
    transaction: &mut ExecutionTransaction<'source, M>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
    consume: impl for<'worker> FnOnce(
        &mut ExecutionTransaction<'source, M>,
        &mut BoundedWorkerSandboxV1<'worker, RootModuleWorkerAllocatorV1>,
        &VerifiedFormalCommitV1,
        &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<T, RootModuleExecutionErrorV1<OE>>,
) -> Result<T, NativeRootScopeErrorV1<OE>>
where
    M: MerkleNodeReader,
{
    let result = (|| {
        validate_parent(
            context.commit,
            context.authority,
            context.memory,
            context.cancelled,
            transaction,
            &live_owner(),
        )?;
        let allocator = RootModuleWorkerAllocatorV1::new(context.memory);
        context.endpoint.with_iws_native_worker_v1(
            context.commit,
            context.memory,
            &mut || context.cancelled.load(Ordering::Acquire),
            |lease| {
                validate_parent(
                    context.commit,
                    context.authority,
                    context.memory,
                    context.cancelled,
                    transaction,
                    &live_owner(),
                )?;
                let mut worker = BoundedWorkerSandboxV1 {
                    executable: WorkerExecutableV1::NativeBoundSealedFd {
                        file: lease.file(),
                        descriptor: lease.descriptor_bytes(),
                    },
                    allocator: &allocator,
                    limits: WorkerLimitsV1 {
                        address_space_bytes: context.limits.address_space_bytes,
                        max_request_bytes: context.limits.max_request_bytes,
                        max_output_bytes: context.limits.max_output_bytes,
                        wall_time_ms: context.limits.wall_time_ms,
                    },
                    cancelled: context.cancelled,
                };
                let output = consume(transaction, &mut worker, context.commit, live_owner)
                    .map_err(NativeRootScopeErrorV1::Invocation)?;
                // Business writes legitimately advance revision; recheck the
                // actual terminal guard against the live owner, not the old one.
                validate_parent(
                    context.commit,
                    context.authority,
                    context.memory,
                    context.cancelled,
                    transaction,
                    &live_owner(),
                )?;
                Ok(output)
            },
        )
    })();
    if result.as_ref().err().is_some_and(|error| {
        !matches!(
            error,
            NativeRootScopeErrorV1::Invocation(RootModuleExecutionErrorV1::Business(_))
        )
    }) {
        let _: Result<(), ExecutionError> =
            transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[cfg(test)]
#[path = "formal_iws_native_worker_tests.rs"]
mod tests;
