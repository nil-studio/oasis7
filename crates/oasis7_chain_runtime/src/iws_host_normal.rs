//! Private fixed-driver seam. No external callback receives a raw sandbox.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1;

impl IwsHostRuntimeV1 {
    /// Only the fixed normal driver in this private module may consume this
    /// scope. All borrowed parent holders and native FD remain alive together.
    fn with_normal_parent_worker<T>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
        run: impl FnOnce(
            HostAcceptedParentReadViewV1<'_>,
            &mut BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
        ) -> Result<T, IwsHostRuntimeErrorV1>,
    ) -> Result<T, IwsHostRuntimeErrorV1> {
        let current = self
            .current
            .current()
            .ok_or_else(|| FormalError::Invalid("Host current owner missing".into()))?;
        with_admitted_worker(current, memory, work, cancelled, |worker| {
            current.source.with_parent_read_view_bounded(
                memory,
                work,
                &mut || cancelled.load(Ordering::Acquire),
                |parent| run(parent, worker),
            )
        })
    }
}
