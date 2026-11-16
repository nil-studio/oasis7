//! Fixed Chain-only normal execution and commit. No caller execution callback,
//! replacement candidate or detached worker/Root context crosses this boundary.
use super::*;
use crate::iws_normal_input::PaidNormalTickSourcesV1;
use crate::iws_normal_input_decode::NormalInputDecodeLimitsV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1;
use oasis7_runtime::{
    NormalCommitHeaderLimitsV1, NormalHostCognitionParentSourcesV1, NormalTailDirectoryInputsV1,
    RuntimeSourceCompiler,
};

pub(crate) enum NormalExecutionCommitFailureV1<'w, 'common> {
    BeforePreparation {
        cause: IwsHostRuntimeErrorV1,
        sources: PaidNormalTickSourcesV1,
        work: NormalCommitWorkV1<'w, 'common>,
    },
    /// Once prepared, recovery exclusively uses the retained output/publication
    /// owner. It cannot call the Action dispatcher a second time.
    AfterPreparation(NormalOutputCommitFailureV1<'w, 'common>),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_and_commit_frozen_normal_bounded_v1<'w, 'common>(
    primary: &mut IwsHostRuntimeV1,
    replica: &mut IwsHostRuntimeV1,
    sources: PaidNormalTickSourcesV1,
    mut work: NormalCommitWorkV1<'w, 'common>,
    writer: &ed25519_dalek::SigningKey,
    input_limits: &NormalInputDecodeLimitsV1,
    action_limits: &CborDataLimitsV1,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    directories: NormalTailDirectoryInputsV1<'_>,
    cognition: NormalHostCognitionParentSourcesV1<'_>,
    compiler: &RuntimeSourceCompiler,
    policy: NormalCommitHeaderLimitsV1,
    max_action_bytes: usize,
    max_grant_payload_bytes: u64,
    primary_provider: &RemoteMonotonicEndpointProvider,
    replica_provider: &RemoteMonotonicEndpointProvider,
    primary_policy: &TrustedFencingPolicyV1,
    replica_policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<(), NormalExecutionCommitFailureV1<'w, 'common>> {
    let preparation = (|| {
        let input = sources.input(memory, archive, work.input_material, &mut || {
            cancelled.load(Ordering::Acquire)
        })?;
        primary.prepare_frozen_normal_runtime_bounded(
            &input,
            memory,
            archive,
            input_limits,
            action_limits,
            work.physical,
            work.runtime,
            directories,
            cognition,
            compiler,
            policy,
            max_action_bytes,
            max_grant_payload_bytes,
            cancelled,
        )
    })();
    let prepared = match preparation {
        Ok(prepared) => prepared,
        Err(cause) => {
            return Err(NormalExecutionCommitFailureV1::BeforePreparation {
                cause,
                sources,
                work,
            });
        }
    };
    commit_prepared_normal_output_bounded_v1(
        primary,
        replica,
        sources,
        prepared,
        work,
        writer,
        input_limits,
        canonical,
        primary_provider,
        replica_provider,
        primary_policy,
        replica_policy,
        memory,
        archive,
        cancelled,
    )
    .map_err(NormalExecutionCommitFailureV1::AfterPreparation)
}
