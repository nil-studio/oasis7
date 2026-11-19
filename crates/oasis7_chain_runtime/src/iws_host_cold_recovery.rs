//! A fixed cold reopen with the original request's remaining work. The input
//! paths, projection and fact locators remain DATA; durable authentication is
//! performed by the production endpoint and Runtime cold consumers.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalEndpointPublicationRefsV1, IwsCandidateSourceWorkV1, IwsColdParentHintsV1,
    IwsColdParentWorkV1, IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
    IwsPageStoreCapabilityV1,
};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageStoreError,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use oasis7_formal_protocol::controlled_authority::{
    formal::VerifiedAuthorityV1, replicated_protocol::EndpointRole,
};
use oasis7_formal_protocol::incremental_material::FactLocationV1;
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_runtime::{
    AuthorizationRootCacheLimitsV1, NormalHostWorkV1, ProvisionalAuthorizationProjectionV1,
    RuntimeSourceCompiler, WorldRuntimeMemoryLimits,
};
use std::{io::Read, path::Path, sync::Arc};

#[derive(Clone, Copy)]
pub(crate) struct ColdEndpointReopenRefsV1<'a> {
    pub endpoint_path: &'a Path,
    pub pages_path: &'a Path,
    pub authority: &'a VerifiedAuthorityV1,
    pub role: EndpointRole,
    pub provider: &'a Arc<RemoteMonotonicEndpointProvider>,
    pub policy: &'a TrustedFencingPolicyV1,
    pub budgets: &'a IwsEndpointVerificationBudgetsV1,
    pub current_limits: IwsCurrentLimitsV1,
    pub checkpoint:
        Option<oasis7_distfs::controlled_authority::formal::endpoint::IwsColdCheckpointSuffixV1>,
}
pub(crate) struct HostColdReopenInputV1<'a> {
    pub endpoint: ColdEndpointReopenRefsV1<'a>,
    pub authorization_hint_path: &'a Path,
    pub projection_limits: AuthorizationRootCacheLimitsV1,
    pub process: super::cold::ProcessInput<'a>,
    pub configuration: &'a WorldRuntimeMemoryLimits,
    pub worker: &'a WorkerLimitsV1,
    pub transaction: oasis7_formal_protocol::incremental_execution::TransactionLimits,
    pub facts: IwsColdParentHintsV1,
}
#[derive(Clone, Copy)]
pub(crate) struct HostColdRecoveryContextV1<'a> {
    pub memory: &'a MemoryBudget,
    pub archive: &'a FormalArchiveReadWorkV1,
    pub cancelled: &'a AtomicBool,
    pub compiler: &'a RuntimeSourceCompiler,
}

pub struct HostColdRecoveryPartsV1 {
    pub cold: IwsColdParentWorkV1,
    pub source: IwsCandidateSourceWorkV1,
    pub physical: HostColdIoWorkV1,
    pub runtime: NormalHostWorkV1,
}

/// Export only occurrence-location DATA from the actual accepted source.
/// A subsequent cold read must independently authenticate these positions;
/// retaining a hint neither retains nor recreates a live Host qualification.
pub(crate) fn selected_fact_hints_bounded_v1(
    host: &IwsHostRuntimeV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<IwsColdParentHintsV1, IwsHostRuntimeErrorV1> {
    let current = host
        .current
        .current()
        .ok_or_else(|| FormalError::Invalid("cold hint actual Host missing".into()))?;
    current.source.with_parent_read_view_bounded(
        memory,
        archive,
        &mut || cancelled.load(Ordering::Acquire),
        |parent| {
            archive.charge(2, 96, &mut || cancelled.load(Ordering::Acquire))?;
            Ok(IwsColdParentHintsV1 {
                runtime: parent.runtime().occurrence().location(),
                cognition: parent.cognition().selected().occurrence().location(),
            })
        },
    )
}

pub(super) fn read_projection_hint_bounded_v1(
    path: &Path,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ProvisionalAuthorizationProjectionV1, IwsHostRuntimeErrorV1> {
    let io = |error| IwsHostRuntimeErrorV1::Physical(PageStoreError::Io(error));
    let invalid = |message: &'static str| FormalError::Invalid(message.into());
    common.charge(2, path.as_os_str().as_encoded_bytes().len() as u64)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Inspect the opened FD without following links or blocking on FIFO.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    let length = usize::try_from(metadata.len())
        .ok()
        .filter(|length| *length <= limits.max_wire_bytes)
        .ok_or_else(|| invalid("cold projection hint file bound"))?;
    if !metadata.is_file() {
        return Err(invalid("cold projection hint regular file").into());
    }
    let layout = length
        .checked_add(size_of::<Vec<u8>>())
        .ok_or_else(|| invalid("cold projection hint layout"))?;
    common.charge(1, layout as u64)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|error| IwsHostRuntimeErrorV1::Move(error.into()))?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(length)
        .map_err(|_| invalid("cold projection hint allocation"))?;
    if raw.capacity() > length {
        return Err(invalid("cold projection hint capacity").into());
    }
    raw.resize(length, 0);
    for chunk in raw.chunks_mut(8192) {
        common.charge(1, chunk.len() as u64)?;
        file.read_exact(chunk).map_err(io)?;
    }
    common.charge(1, 1)?;
    if file.read(&mut [0u8; 1]).map_err(io)? != 0 {
        return Err(invalid("cold projection hint changed length").into());
    }
    ProvisionalAuthorizationProjectionV1::read_hint_data(
        &fee.retain(raw),
        limits,
        common,
        MemoryClass::Ordinary,
    )
    .map_err(IwsHostRuntimeErrorV1::ColdProjection)
}

pub(crate) fn reopen_host_bounded_v1(
    input: HostColdReopenInputV1<'_>,
    work: &mut HostColdRecoveryWorkV1,
    mailbox_hint: &mut dyn FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
    context: HostColdRecoveryContextV1<'_>,
) -> Result<IwsHostRuntimeV1, IwsHostRuntimeErrorV1> {
    let HostColdRecoveryContextV1 {
        memory,
        archive,
        cancelled,
        compiler,
    } = context;
    let projection = {
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut common = archive.bounded_context(memory, &mut stop);
        work.validate_context(&mut common)?;
        read_projection_hint_bounded_v1(
            input.authorization_hint_path,
            input.projection_limits,
            &mut common,
        )?
    };
    let endpoint_input = input.endpoint;
    archive.charge(
        1,
        endpoint_input
            .pages_path
            .as_os_str()
            .as_encoded_bytes()
            .len() as u64,
        &mut || cancelled.load(Ordering::Acquire),
    )?;
    let pages = IncrementalPageStore::open(
        endpoint_input.pages_path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .map_err(IwsHostRuntimeErrorV1::Physical)?;
    let mut endpoint = FileFormalEndpoint::reopen_authenticated_existing_bounded_v1(
        endpoint_input.endpoint_path,
        endpoint_input.authority,
        endpoint_input.role,
        endpoint_input.provider.clone(),
        FormalEndpointPublicationRefsV1 {
            provider: endpoint_input.provider.as_ref(),
            policy: endpoint_input.policy,
            memory,
            work: archive,
        },
        &mut || cancelled.load(Ordering::Acquire),
    )
    .map_err(IwsHostRuntimeErrorV1::Source)?;
    endpoint.install_iws_page_store(
        IwsPageStoreCapabilityV1::new(pages, endpoint_input.budgets.clone())?
            .with_current_limits(endpoint_input.current_limits)?,
    )?;
    let limits = WorkerLimitsV1 {
        address_space_bytes: input.worker.address_space_bytes,
        max_request_bytes: input.worker.max_request_bytes,
        max_output_bytes: input.worker.max_output_bytes,
        wall_time_ms: input.worker.wall_time_ms,
    };
    let source = match endpoint_input.checkpoint {
        Some(selection) => endpoint.into_authenticated_cold_host_source_from_checkpoint_bounded(
            FormalEndpointPublicationRefsV1 {
                provider: endpoint_input.provider,
                policy: endpoint_input.policy,
                memory,
                work: archive,
            },
            selection,
            &mut work.cold,
            &mut work.source,
            mailbox_hint,
            &mut || cancelled.load(Ordering::Acquire),
        ),
        None => endpoint.into_authenticated_cold_host_source_bounded(
            endpoint_input.provider,
            endpoint_input.policy,
            memory,
            archive,
            &mut work.cold,
            &mut work.source,
            input.facts,
            mailbox_hint,
            &mut || cancelled.load(Ordering::Acquire),
        ),
    }
    .map_err(IwsHostRuntimeErrorV1::ColdSource)?;
    let mut host = IwsHostRuntimeV1::default();
    let outcome = host.install_authenticated_cold_source_bounded_v1(
        source,
        super::cold::HostColdResidentInputsV1 {
            process: input.process,
            limits,
            configuration: input.configuration.clone(),
            projection,
            transaction: input.transaction,
        },
        (&mut work.physical, &mut work.runtime),
        HostColdRecoveryContextV1 {
            memory,
            archive,
            cancelled,
            compiler,
        },
    );
    outcome.map_err(|error| match error {
        HostColdStartupErrorV1::Host(error) => error,
        HostColdStartupErrorV1::Source(error) => IwsHostRuntimeErrorV1::ColdSource(error),
    })?;
    Ok(host)
}

/// Created once at admission, before the Node request starts. It is moved
/// with that request, and every recovery attempt borrows its actual balances.
pub struct HostColdRecoveryWorkV1 {
    pub cold: IwsColdParentWorkV1,
    pub source: IwsCandidateSourceWorkV1,
    pub physical: HostColdIoWorkV1,
    pub runtime: NormalHostWorkV1,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl HostColdRecoveryWorkV1 {
    pub fn from_parts_bounded(
        parts: HostColdRecoveryPartsV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, IwsHostRuntimeErrorV1> {
        parts
            .runtime
            .validate_context_bounded_v1(common)
            .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
        common.charge(1, size_of::<Self>() as u64)?;
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(|error| IwsHostRuntimeErrorV1::Move(error.into()))?;
        Ok(Self {
            cold: parts.cold,
            source: parts.source,
            physical: parts.physical,
            runtime: parts.runtime,
            core: common.work().identity(),
            fee,
        })
    }
    pub(crate) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(
                FormalError::Invalid("cold recovery original work Core/pool".into()).into(),
            );
        }
        self.runtime
            .validate_context_bounded_v1(common)
            .map_err(IwsHostRuntimeErrorV1::NormalSource)
    }
}
