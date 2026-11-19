//! Fixed Chain -> Runtime normal consumer. No caller closure receives the
//! actual worker, transaction, page handles or a Root context.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
    incremental_value::ValueTreeError,
};
use oasis7_runtime::{
    NormalCommitHeaderLimitsV1, NormalHostCognitionParentSourcesV1, NormalHostWorkV1,
    PreparedNormalHostRuntimeV1, RuntimeSourceCompiler, prepare_normal_host_from_resident_v1,
};
impl IwsHostRuntimeV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_frozen_normal_runtime_bounded(
        &self,
        input: &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        action_limits: &CborDataLimitsV1,
        physical: &mut super::io::HostNormalIoWorkV1,
        work: &mut NormalHostWorkV1,
        cognition: NormalHostCognitionParentSourcesV1<'_>,
        compiler: &RuntimeSourceCompiler,
        policy: NormalCommitHeaderLimitsV1,
        max_action_bytes: usize,
        max_grant_payload_bytes: u64,
        cancelled: &AtomicBool,
    ) -> Result<PreparedNormalHostRuntimeV1, IwsHostRuntimeErrorV1> {
        let continuation = self
            .continuation
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("normal Host continuation missing".into()))?;
        let resident = continuation.resident.as_ref().ok_or_else(|| {
            FormalError::Invalid("normal Host genuine resident sources missing".into())
        })?;
        self.with_normal_parent_io_worker(
            memory,
            archive,
            input,
            input_limits,
            physical,
            cancelled,
            |bound, parent, input, plan, state, values, originals, worker, common| {
                // The original input consumer still rejects unsupported side inputs;
                // a decoded Bootstrap/service graph is never silently omitted.
                plan.validate_context(common)
                    .map_err(IwsHostRuntimeErrorV1::InputDecode)?;
                let mut members = values.handle();
                let mut fees = values.handle();
                let mut events = values.handle();
                let read = values.handle();
                let mut original = originals.reader();
                let mut observations = originals.observation_sink();
                let take_error = || {
                    state.take_read_error().map(|e| match e {
                        oasis7_distfs::incremental_page_store::PageStoreError::Cancelled => {
                            ValueTreeError::Cancelled
                        }
                        oasis7_distfs::incremental_page_store::PageStoreError::Memory(e) => {
                            ValueTreeError::Memory(e)
                        }
                        oasis7_distfs::incremental_page_store::PageStoreError::ResourceLimited(
                            s,
                        ) => ValueTreeError::ResourceLimited(s),
                        e => ValueTreeError::Io(std::io::Error::other(e)),
                    })
                };
                let prepared = prepare_normal_host_from_resident_v1(
                    bound,
                    input.data(),
                    state,
                    &read,
                    &mut members,
                    &mut fees,
                    &mut events,
                    &mut original,
                    |producer| originals.persist_event(producer),
                    worker,
                    resident,
                    compiler,
                    cognition,
                    work,
                    take_error,
                    || cancelled.load(Ordering::Acquire),
                    action_limits,
                    max_action_bytes,
                    max_grant_payload_bytes,
                    &mut observations,
                    policy,
                    common,
                    &mut || parent.fence(),
                )
                .map_err(IwsHostRuntimeErrorV1::NormalFixed)?;
                super::persist::persist_sealed_normal_commit_v1(
                    &prepared,
                    input,
                    &mut members,
                    &mut original,
                    common,
                    &parent.fence(),
                )
                .map_err(IwsHostRuntimeErrorV1::NormalPersist)?;
                Ok(prepared)
            },
        )
    }
}

impl IwsHostRuntimeV1 {
    /// Builds the whole prepaid resident successor before Qualified publication.
    /// The real Ready FD and staged-source lease remain borrowed throughout.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_normal_resident_successor_bounded(
        &self,
        ready: &oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalReadyPairV1,
        prepared: PreparedNormalHostRuntimeV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        physical: &mut super::io::HostNormalIoWorkV1,
        work: &mut NormalHostWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<oasis7_runtime::PreparedNormalHostResidentV1, IwsHostRuntimeErrorV1> {
        let expected = self.continuation.as_ref().ok_or_else(|| {
            FormalError::Invalid("normal successor actual continuation missing".into())
        })?;
        let resident = expected.resident.as_ref().ok_or_else(|| {
            FormalError::Invalid("normal successor actual resident missing".into())
        })?;
        ready.with_primary_candidate_read_view_bounded(memory,archive,&mut ||cancelled.load(Ordering::Acquire),|candidate|{
            archive.charge(1,64,&mut ||cancelled.load(Ordering::Acquire))?;
            if candidate.parent_record_id()!=expected.record||candidate.namespace_identities()!=(expected.page_namespace,expected.native_namespace){return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));}
            let zero=||oasis7_distfs::incremental_page_store::PageIoBudget{reads_remaining:0,read_bytes_remaining:0,writes_remaining:0,write_bytes_remaining:0};
            let loan=std::mem::replace(&mut physical.state_pages,zero());let before_constructor=loan.clone();
            let state=match candidate.pages().state_session_with_cancelled(loan,MemoryClass::Ordinary,cancelled){Ok(state)=>state,Err(error)=>{physical.state_pages=before_constructor;return Err(IwsHostRuntimeErrorV1::Physical(error));}};
            let mut values=candidate.pages().value_session_with_cancelled(std::mem::replace(&mut physical.value_pages,zero()),cancelled);
            let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                let views=super::values::HostNormalValuesV1::new(&mut values,memory).map_err(IwsHostRuntimeErrorV1::Value)?;
                let read=views.handle();let mut stop=||cancelled.load(Ordering::Acquire);let mut common=archive.bounded_context(memory,&mut stop);
                oasis7_runtime::prepare_normal_host_resident_promotion_from_io_v1(prepared,resident,candidate.core(),candidate.storage().runtime(),candidate.storage().cognition(),candidate.proof_ledger(),candidate.quorum(),candidate.roots(),&state,&read,work,||state.take_read_error().map(|e|match e{oasis7_distfs::incremental_page_store::PageStoreError::Cancelled=>ValueTreeError::Cancelled,oasis7_distfs::incremental_page_store::PageStoreError::Memory(e)=>ValueTreeError::Memory(e),oasis7_distfs::incremental_page_store::PageStoreError::ResourceLimited(s)=>ValueTreeError::ResourceLimited(s),e=>ValueTreeError::Io(std::io::Error::other(e))}),||cancelled.load(Ordering::Acquire),&mut common).map_err(IwsHostRuntimeErrorV1::NormalSource)
            }));
            let state_error=match state.remaining_io(){Ok(left)=>{physical.state_pages=left;None},Err(e)=>Some(e)};
            let value_error=match values.remaining_io(){Ok(left)=>{physical.value_pages=left;None},Err(e)=>Some(e)};
            match outcome{Err(payload)=>std::panic::resume_unwind(payload),Ok(result)=>{let value=result?;if let Some(error)=state_error.or(value_error){return Err(IwsHostRuntimeErrorV1::Physical(error));}Ok(value)}}
        })
    }
}

impl IwsHostRuntimeV1 {
    /// The replica's complete staged source is checked independently before
    /// sharing immutable prepaid logical caches from the one Runtime execution.
    pub fn prepare_replica_normal_resident_successor_bounded(
        &self,
        ready: &oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalReadyPairV1,
        primary: &oasis7_runtime::PreparedNormalHostResidentV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<oasis7_runtime::PreparedNormalHostResidentV1, IwsHostRuntimeErrorV1> {
        let expected = self
            .continuation
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("replica actual continuation missing".into()))?;
        if expected.resident.is_none() {
            return Err(FormalError::Invalid("replica genuine resident missing".into()).into());
        }
        ready.with_replica_candidate_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |candidate| {
                archive.charge(1, 64, &mut || cancelled.load(Ordering::Acquire))?;
                if candidate.parent_record_id() != expected.record
                    || candidate.namespace_identities()
                        != (expected.page_namespace, expected.native_namespace)
                {
                    return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
                }
                primary
                    .prepare_secondary_candidate_v1(
                        expected.resident.as_ref().ok_or_else(|| {
                            FormalError::Invalid("replica genuine process source missing".into())
                        })?,
                        candidate.core(),
                        candidate.storage().runtime(),
                        candidate.storage().cognition(),
                        candidate.proof_ledger(),
                        candidate.quorum(),
                        &mut archive
                            .bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                    )
                    .map_err(IwsHostRuntimeErrorV1::NormalSource)
            },
        )
    }
}

/// Fixed output construction preserves the actual typed first cause. The
/// Node trait's sentinel never replaces its retained physical/resource error.
#[derive(Debug)]
pub(in crate::iws_host_runtime) enum NormalPackageBuildFailureV1 {
    Host(IwsHostRuntimeErrorV1),
    Context(FormalError),
    Body(BodyMutationError),
    Material(
        oasis7::world_service::formal_result_iws_material::IwsBusinessMaterialErrorV1<
            super::super::normal_package::HostBusinessOriginalReadErrorV1,
        >,
    ),
    Reader(super::super::normal_package::HostBusinessOriginalReadErrorV1),
    Node(super::super::normal_package::PaidHostBusinessOriginalFailureV1),
    Business(oasis7::world_service::formal_result::PaidFormalBusinessErrorV1),
    Package(super::super::normal_package::NormalPackageErrorV1),
}
impl IwsHostRuntimeV1 {
    /// One fixed consumer joins the genuine Frozen/Node input, actual terminal
    /// output and the exact Stored Qualified parent. No caller supplies a
    /// result, descriptor, proof, root, status or replacement candidate body.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::iws_host_runtime) fn prepare_normal_output_package_bounded_v1(
        &self,
        prepared: &PreparedNormalHostRuntimeV1,
        input: &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
        writer: &ed25519_dalek::SigningKey,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        physical: &mut super::io::HostNormalIoWorkV1,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<
        super::super::normal_package::PaidNormalCandidatePackageV1,
        NormalPackageBuildFailureV1,
    > {
        use super::super::normal_package::HostNormalBusinessOriginalReaderV1;
        use NormalPackageBuildFailureV1 as E;
        self.with_normal_parent_io_worker(memory,archive,input,input_limits,physical,cancelled,
            |bound,parent,input,_,_,_,originals,_,common| {
                let mut run=|| -> Result<_,E> {
                    prepared.validate_input_source_bounded_v1(input.data(),common).map_err(E::Body)?;
                    let proof=parent.read_qualified_parent_proof_bounded(memory,archive,
                        &mut || cancelled.load(Ordering::Acquire)).map_err(E::Context)?;
                    proof.validate_parent_bounded(parent,common).map_err(E::Context)?;
                    let mut objects=HostNormalBusinessOriginalReaderV1::new(originals,memory,common).map_err(E::Reader)?;
                    let nodes=HostNormalBusinessOriginalReaderV1::new(originals,memory,common).map_err(E::Reader)?;
                    let previous=oasis7::world_service::formal_result_iws_material::read_iws_qualified_parent_business_descriptor_bounded_v1(
                        parent.head(),&proof,&mut objects,&nodes,common);
                    let previous=match previous {
                        Ok(value)=>value,
                        Err(error)=>return Err(match nodes.into_failure(){Some(error)=>E::Node(error),None=>E::Material(error)}),
                    };
                    let source=prepared.business_source_bounded_v1(bound,common).map_err(E::Body)?;
                    let update=oasis7::world_service::formal_result::build_current_formal_business_result_bounded_v1(
                        source,&previous,&proof,&nodes,common,canonical);
                    if let Some(error)=nodes.into_failure(){return Err(E::Node(error));}
                    let update=update.map_err(E::Business)?;
                    // The original request, descriptor and append delta remain
                    // paid/alive until the package owns its complete graph.
                    super::super::normal_package::prepare_normal_candidate_package_bounded_v1(
                        prepared,input,parent,writer,&update,canonical,common,
                    ).map_err(E::Package)
                };
                Ok(run())
            }).map_err(E::Host)?
    }
}

use oasis7_runtime::BodyMutationError;
