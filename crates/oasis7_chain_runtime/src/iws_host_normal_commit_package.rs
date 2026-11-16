//! One consuming tail from the exact executed tick to dual accepted install.
//! Failures before publication keep the paid graph and genuine input owners;
//! publication failures retain the existing lease/reconciliation state machine.
use super::*;
use crate::iws_host_runtime::normal::NormalPackageBuildFailureV1;
use crate::iws_host_runtime::normal_package::PaidNormalCandidatePackageV1;
use crate::iws_normal_input_decode::NormalInputDecodeLimitsV1;
use oasis7_wasm_abi::CanonicalCborWorkV1;

pub(crate) enum NormalOutputCommitFailureV1<'w, 'common> {
    Input {
        cause: FormalError,
        sources: PaidNormalTickSourcesV1,
        prepared: PreparedNormalHostRuntimeV1,
        work: NormalCommitWorkV1<'w, 'common>,
    },
    Build {
        cause: NormalPackageBuildFailureV1,
        sources: PaidNormalTickSourcesV1,
        prepared: PreparedNormalHostRuntimeV1,
        work: NormalCommitWorkV1<'w, 'common>,
    },
    Persist {
        cause: IwsHostRuntimeErrorV1,
        package: PaidNormalCandidatePackageV1,
        sources: PaidNormalTickSourcesV1,
        prepared: PreparedNormalHostRuntimeV1,
        work: NormalCommitWorkV1<'w, 'common>,
    },
    Publication(NormalCommitStartFailureV1<'w, 'common>),
}

/// Executes no Action and grants no authority from candidate DATA. The actual
/// Frozen source, final preparation, persistence receipt and publication owners
/// are moved once through the fixed existing consumers, never recreated.
#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_prepared_normal_output_bounded_v1<'w, 'common>(
    primary: &mut IwsHostRuntimeV1,
    replica: &mut IwsHostRuntimeV1,
    sources: PaidNormalTickSourcesV1,
    prepared: PreparedNormalHostRuntimeV1,
    mut work: NormalCommitWorkV1<'w, 'common>,
    writer: &ed25519_dalek::SigningKey,
    input_limits: &NormalInputDecodeLimitsV1,
    canonical: &mut CanonicalCborWorkV1,
    primary_provider: &RemoteMonotonicEndpointProvider,
    replica_provider: &RemoteMonotonicEndpointProvider,
    primary_policy: &TrustedFencingPolicyV1,
    replica_policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<(), NormalOutputCommitFailureV1<'w, 'common>> {
    // A borrowed verified input cannot escape its genuine source owner. End
    // that borrow before moving the same sources into publication or failure.
    let result = (|| {
        let input = sources
            .input(memory, archive, work.input_material, &mut || {
                cancelled.load(Ordering::Acquire)
            })
            .map_err(TailCause::Input)?;
        let package = primary
            .prepare_normal_output_package_bounded_v1(
                &prepared,
                &input,
                writer,
                input_limits,
                work.physical,
                canonical,
                memory,
                archive,
                cancelled,
            )
            .map_err(TailCause::Build)?;
        primary
            .persist_normal_candidate_package_bounded_v1(
                package,
                &input,
                &prepared,
                input_limits,
                work.physical,
                archive,
                memory,
                cancelled,
            )
            .map_err(|(cause, package)| TailCause::Persist(cause, package))
    })();
    let package = match result {
        Ok(package) => package,
        Err(TailCause::Input(cause)) => {
            return Err(NormalOutputCommitFailureV1::Input {
                cause,
                sources,
                prepared,
                work,
            });
        }
        Err(TailCause::Build(cause)) => {
            return Err(NormalOutputCommitFailureV1::Build {
                cause,
                sources,
                prepared,
                work,
            });
        }
        Err(TailCause::Persist(cause, package)) => {
            return Err(NormalOutputCommitFailureV1::Persist {
                cause,
                package,
                sources,
                prepared,
                work,
            });
        }
    };
    publish_prepared_normal_runtime_bounded(
        primary,
        replica,
        sources,
        prepared,
        package,
        work,
        primary_provider,
        replica_provider,
        primary_policy,
        replica_policy,
        memory,
        archive,
        cancelled,
    )
    .map_err(NormalOutputCommitFailureV1::Publication)
}

enum TailCause {
    Input(FormalError),
    Build(NormalPackageBuildFailureV1),
    Persist(IwsHostRuntimeErrorV1, PaidNormalCandidatePackageV1),
}

#[path = "iws_host_normal_commit_output_recovery.rs"]
mod recovery;
