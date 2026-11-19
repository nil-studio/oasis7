//! Recovery resumes the actual retained phase; it never reruns tick execution
//! or invents a replacement lease after an ambiguous publication response.
use super::*;
impl NormalOutputCommitFailureV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn recover_bounded_v1(
        self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        writer: &ed25519_dalek::SigningKey,
        input_limits: &NormalInputDecodeLimitsV1,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
        cold: &NormalColdRecoveryInputsV1<'_>,
        compiler: &oasis7_runtime::RuntimeSourceCompiler,
    ) -> Result<NormalCommitWorkV1, Self> {
        match self {
            Self::Input {
                sources,
                prepared,
                work,
                ..
            }
            | Self::Build {
                sources,
                prepared,
                work,
                ..
            } => commit_prepared_normal_output_bounded_v1(
                primary,
                replica,
                sources,
                prepared,
                work,
                writer,
                input_limits,
                primary_provider,
                replica_provider,
                primary_policy,
                replica_policy,
                memory,
                archive,
                cancelled,
            ),
            Self::Persist {
                package,
                sources,
                prepared,
                mut work,
                ..
            } => {
                // A partial Original write is content-addressed DATA. Resume
                // the real dual writer/readback with the same graph and work;
                // no Begin was issued and no candidate is reconstructed.
                let input =
                    match sources.input(memory, archive, &mut work.input_material, &mut || {
                        cancelled.load(Ordering::Acquire)
                    }) {
                        Ok(input) => input,
                        Err(cause) => {
                            return Err(Self::Persist {
                                cause: IwsHostRuntimeErrorV1::Formal(cause),
                                package,
                                sources,
                                prepared,
                                work,
                            });
                        }
                    };
                let persisted = primary.persist_normal_candidate_package_bounded_v1(
                    package,
                    &input,
                    &prepared,
                    input_limits,
                    &mut work.physical,
                    archive,
                    memory,
                    cancelled,
                );
                drop(input);
                let persisted = match persisted {
                    Ok(value) => value,
                    Err((cause, package)) => {
                        return Err(Self::Persist {
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
                    persisted,
                    work,
                    primary_provider,
                    replica_provider,
                    primary_policy,
                    replica_policy,
                    memory,
                    archive,
                    cancelled,
                )
                .map_err(Self::Publication)
            }
            Self::Publication(NormalCommitStartFailureV1::Package {
                package,
                sources,
                prepared,
                work,
                ..
            }) => publish_prepared_normal_runtime_bounded(
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
            .map_err(Self::Publication),
            Self::Publication(NormalCommitStartFailureV1::Publication(failure)) => failure
                .recover_bounded(
                    primary,
                    replica,
                    primary_provider,
                    replica_provider,
                    primary_policy,
                    replica_policy,
                    memory,
                    archive,
                    cancelled,
                )
                .or_else(|failure| {
                    failure.recover_cold_bounded_v1(
                        primary,
                        replica,
                        cold,
                        crate::iws_host_runtime::cold_recovery::HostColdRecoveryContextV1 {
                            memory,
                            archive,
                            cancelled,
                            compiler,
                        },
                    )
                })
                .map_err(|failure| {
                    Self::Publication(NormalCommitStartFailureV1::Publication(failure))
                }),
        }
    }
}
