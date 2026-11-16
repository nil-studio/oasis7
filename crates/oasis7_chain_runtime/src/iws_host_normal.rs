//! Private fixed-driver seam. No external callback receives a raw sandbox.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::incremental_normal_sources::{
    BoundNormalTickParentV1, NormalTickParentRefsV1, bind_normal_tick_parent_v1,
};

impl IwsHostRuntimeV1 {
    /// Only the fixed normal driver in this private module may consume this
    /// scope. All borrowed parent holders and native FD remain alive together.
    fn with_normal_parent_worker<T>(
        &self,
        input: &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
        run: impl FnOnce(
            &ReservedValue<BoundNormalTickParentV1<'_>>,
            &HostAcceptedParentReadViewV1<'_>,
            &mut BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
        ) -> Result<T, IwsHostRuntimeErrorV1>,
    ) -> Result<T, IwsHostRuntimeErrorV1> {
        let expected = self
            .continuation
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("Host normal continuation missing".into()))?;
        if !self.current.is_empty() || self.current.generation() != expected.generation {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut context = work.bounded_context(memory, &mut stop);
            input.validate_context(&mut context)?;
            if expected.identity != context.work().identity() || !expected.fee.uses_budget(memory) {
                return Err(
                    FormalError::Invalid("Host Frozen continuation pool/Core".into()).into(),
                );
            }
        }
        let frozen = input.frozen_sources();
        frozen.with_primary_parent_read_view_bounded(
            memory,
            work,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| {
                work.charge(1, 192, &mut || cancelled.load(Ordering::Acquire))?;
                if parent.head().verified_record_id() != expected.record
                    || parent.endpoint_role() != expected.role
                    || parent.namespace_identities()
                        != (expected.page_namespace, expected.native_namespace)
                    || parent.fence() != expected.fence
                {
                    return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
                }
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = work.bounded_context(memory, &mut stop);
                let bound = bind_normal_tick_parent_v1(
                    NormalTickParentRefsV1 {
                        core: parent.core(),
                        storage: parent.storage(),
                        roots: parent.roots(),
                        runtime: parent.runtime(),
                        cognition: parent.cognition(),
                        retention: parent.retention_ledger(),
                        head: parent.head(),
                        authority: parent.authority(),
                    },
                    &mut common,
                )?;
                frozen.with_primary_native_worker_bounded(
                    memory,
                    work,
                    &mut || cancelled.load(Ordering::Acquire),
                    |lease| {
                        with_admitted_native_lease(
                            lease,
                            expected.environment,
                            &expected.limits,
                            memory,
                            work,
                            cancelled,
                            |worker| run(&bound, &parent, worker),
                        )
                    },
                )
            },
        )
    }
}

#[path = "iws_host_normal_io.rs"]
pub(in crate::iws_host_runtime) mod io;
pub(super) use io::HostNormalIoWorkV1;

#[path = "iws_host_normal_originals.rs"]
pub(in crate::iws_host_runtime) mod originals;

#[path = "iws_host_normal_values.rs"]
pub(in crate::iws_host_runtime) mod values;

#[path = "iws_host_normal_fixed.rs"]
mod fixed;
pub(in crate::iws_host_runtime) use fixed::NormalPackageBuildFailureV1;

#[path = "iws_host_normal_persist.rs"]
mod persist;
pub use persist::NormalCommitPersistErrorV1;
