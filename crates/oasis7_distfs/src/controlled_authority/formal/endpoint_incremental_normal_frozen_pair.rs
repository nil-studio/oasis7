//! Dual receipt issuer. Only actual completed endpoint owners enter here.
use super::*;

pub struct PaidNormalFrozenPairV1 {
    owned: PaidNormalBoxV1<NormalFrozenPairStateV1>,
}
struct NormalFrozenPairStateV1 {
    primary: PaidNormalFrozenEndpointV1,
    replica: PaidNormalFrozenEndpointV1,
    certificate: ReservedValue<FormalFrozenCertificateV1>,
}
enum NormalFrozenPairRecoveryV1 {
    Both {
        primary: PaidNormalFrozenEndpointV1,
        replica: PaidNormalFrozenEndpointV1,
        cause: BoundedAcquireErrorV1,
    },
    PrimaryFailed {
        replica: PaidNormalFrozenEndpointV1,
        failed: NormalFrozenFailureV1,
    },
    ReplicaFailed {
        primary: PaidNormalFrozenEndpointV1,
        failed: NormalFrozenFailureV1,
    },
}
pub struct NormalFrozenPairFailureV1 {
    retained: NormalFrozenPairRecoveryV1,
}
// These two fixed outer conflicts describe where the original failure occurred.
// The actual failure, including any paid protocol refusal, stays in `failed`.
// Representing the fixed text by the private variant needs no second error owner.
struct NormalFrozenPairCauseV1<'a>(&'a NormalFrozenPairFailureV1);
struct NormalFrozenPairConflictV1(&'static str);
impl std::fmt::Debug for NormalFrozenPairConflictV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Conflict").field(&self.0).finish()
    }
}
impl std::fmt::Debug for NormalFrozenPairCauseV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let note = match &self.0.retained {
            NormalFrozenPairRecoveryV1::Both { cause, .. } => {
                return std::fmt::Debug::fmt(cause, f);
            }
            NormalFrozenPairRecoveryV1::PrimaryFailed { .. } => {
                "primary certificate persistence requires recovery"
            }
            NormalFrozenPairRecoveryV1::ReplicaFailed { .. } => {
                "replica certificate persistence requires recovery"
            }
        };
        f.debug_tuple("Work")
            .field(&NormalFrozenPairConflictV1(note))
            .finish()
    }
}
impl std::fmt::Debug for NormalFrozenPairFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalFrozenPairFailureV1")
            .field("cause", &NormalFrozenPairCauseV1(self))
            .finish()
    }
}
impl std::fmt::Display for NormalFrozenPairFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "normal dual Frozen: {:?}", NormalFrozenPairCauseV1(self))
    }
}
impl std::error::Error for NormalFrozenPairFailureV1 {}
impl PaidNormalFrozenPairV1 {
    pub fn certificate(&self) -> &FormalFrozenCertificateV1 {
        &self.owned.value.certificate
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.owned.value.primary.uses_budget(memory)
            && self.owned.value.replica.uses_budget(memory)
            && self.owned.value.certificate.uses_budget(memory)
            && self.owned.uses_budget(memory)
    }
    /// Both original parent/FD holders remain borrowed throughout checking and
    /// execution. Returned Checked borrows those holders and cannot outlive them.
    pub fn check_material<'a>(
        &'a self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        material_work: &mut IncrementalFrozenMaterialWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<ReservedValue<CheckedIncrementalFrozenMaterialV1<'a>>, FormalError> {
        work.consume_steps(1, stop)?;
        if !self.uses_budget(memory) {
            return Err(invalid("normal Frozen pair pool"));
        }
        let source = &self.owned.value.primary.owned.value.source;
        if source.identity != work.bounded_context(memory, stop).work().identity() {
            return Err(invalid("normal Frozen pair Core"));
        }
        let parent = source
            .endpoint
            .parent
            .as_ref()
            .ok_or_else(|| invalid("normal Frozen parent missing"))?;
        material::check_material(
            &self.owned.value.primary.owned.value.original,
            &self.owned.value.primary.owned.value.reservation,
            parent,
            memory,
            work,
            material_work,
            stop,
        )
    }
}
fn install(
    mut endpoint: PaidNormalFrozenEndpointV1,
    certificate: &FormalFrozenCertificateV1,
    provider: &RemoteMonotonicEndpointProvider,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNormalFrozenEndpointV1, NormalFrozenFailureV1> {
    let result = (|| {
        let (input, material) = state::input(&endpoint.owned.value.original)?;
        let complete = state::state(
            input,
            material,
            Some(endpoint.receipt()),
            Some(certificate),
            memory,
            work,
            stop,
        )?;
        endpoint
            .owned
            .value
            .source
            .endpoint
            .store
            .prepare_request_update_bounded(&complete, memory, work, stop)
    })();
    let update = match result {
        Ok(value) => value,
        Err(cause) => return Err(failure(endpoint.owned, cause.into())),
    };
    if let Err(cause) = endpoint.owned.value.source.publish(
        update,
        FormalEndpointPublicationRefsV1 {
            provider,
            policy,
            memory,
            work,
        },
        &endpoint.owned.fee,
        stop,
    ) {
        return Err(failure(endpoint.owned, cause));
    }
    Ok(endpoint)
}
pub fn join_normal_frozen_endpoints_bounded(
    primary: PaidNormalFrozenEndpointV1,
    replica: PaidNormalFrozenEndpointV1,
    publication: FormalEndpointPairPublicationRefsV1<'_>,
    stop: &mut impl FnMut() -> bool,
) -> Result<PaidNormalFrozenPairV1, NormalFrozenPairFailureV1> {
    let FormalEndpointPairPublicationRefsV1 {
        primary_provider,
        replica_provider,
        primary_policy,
        replica_policy,
        memory,
        work,
    } = publication;
    let prepared = (|| {
        work.consume_steps(1, stop)?;
        if !primary.uses_budget(memory)
            || !replica.uses_budget(memory)
            || primary.owned.value.source.identity != replica.owned.value.source.identity
            || primary.owned.value.source.identity
                != work.bounded_context(memory, stop).work().identity()
            || primary.owned.value.source.endpoint.role != EndpointRole::Primary
            || replica.owned.value.source.endpoint.role != EndpointRole::Replica
        {
            return Err(invalid("normal Frozen dual role/pool/Core"));
        }
        // Every receipt here was produced by its actual protected endpoint and
        // verified before its private owner was minted. Compare the originals,
        // not merely caller-supplied receipt digests or role booleans.
        if !store::bounded_transaction::same(
            primary.owned.value.source.endpoint.authority.definition(),
            replica.owned.value.source.endpoint.authority.definition(),
            memory,
            work,
            stop,
        )? || !store::bounded_transaction::same(
            primary.owned.value.original.state(),
            replica.owned.value.original.state(),
            memory,
            work,
            stop,
        )? {
            return Err(invalid("normal Frozen dual originals/authority"));
        }
        let (input, _) = state::input(&primary.owned.value.original)?;
        let measured = store::context_wire::encode_paid(
            &(input, primary.receipt(), replica.receipt()),
            64 << 20,
            &mut work.bounded_context(memory, stop),
            "normal Frozen certificate bound",
            "dual certificate",
        )?;
        let n = measured
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(size_of::<FormalFrozenCertificateV1>() + 16384))
            .ok_or_else(|| invalid("normal Frozen certificate layout"))?;
        let fee = memory
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|_| invalid("normal Frozen certificate admission"))?;
        let outer = memory
            .reserve(
                PaidNormalBoxV1::<NormalFrozenPairStateV1>::layout(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("normal Frozen pair admission"))?;
        work.charge(1, measured.len() as u64, stop)?;
        let certificate = fee.retain(FormalFrozenCertificateV1 {
            reservation: input.clone(),
            primary: primary.receipt().clone(),
            replica: replica.receipt().clone(),
        });
        let _verified_id = oasis7_formal_protocol::controlled_authority::formal::verify_normal_frozen_certificate_bounded(
            &certificate, primary.owned.value.source.endpoint.authority.definition(), primary.owned.value.source.endpoint.authority.activation_ref(),
            &mut work.bounded_context(memory, stop))?;
        Ok((certificate, outer))
    })();
    let (certificate, outer) = match prepared {
        Ok(value) => value,
        Err(cause) => {
            return Err(NormalFrozenPairFailureV1 {
                retained: NormalFrozenPairRecoveryV1::Both {
                    primary,
                    replica,
                    cause: cause.into(),
                },
            });
        }
    };
    let primary = match install(
        primary,
        &certificate,
        primary_provider,
        primary_policy,
        memory,
        work,
        stop,
    ) {
        Ok(value) => value,
        Err(failed) => {
            return Err(NormalFrozenPairFailureV1 {
                retained: NormalFrozenPairRecoveryV1::PrimaryFailed { replica, failed },
            });
        }
    };
    let replica = match install(
        replica,
        &certificate,
        replica_provider,
        replica_policy,
        memory,
        work,
        stop,
    ) {
        Ok(value) => value,
        Err(failed) => {
            return Err(NormalFrozenPairFailureV1 {
                retained: NormalFrozenPairRecoveryV1::ReplicaFailed { primary, failed },
            });
        }
    };
    Ok(PaidNormalFrozenPairV1 {
        owned: PaidNormalBoxV1::new(
            NormalFrozenPairStateV1 {
                primary,
                replica,
                certificate,
            },
            outer,
        ),
    })
}

impl NormalFrozenPairFailureV1 {
    /// A certificate write may have committed at one endpoint before failure at
    /// the other. Preserve both holders and retry only after the failed side's
    /// actual disk/provider reconciliation. Reinstalling the identical cert is
    /// metadata-only and keeps the original receipt and parent lease.
    pub fn recover_bounded(
        self,
        publication: FormalEndpointPairPublicationRefsV1<'_>,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<PaidNormalFrozenPairV1, Self> {
        let FormalEndpointPairPublicationRefsV1 {
            primary_provider,
            replica_provider,
            primary_policy,
            replica_policy,
            memory,
            work,
        } = publication;
        let (other, failed, was_primary) = match self.retained {
            NormalFrozenPairRecoveryV1::Both {
                primary,
                replica,
                cause,
            } => {
                // The original failure remains owned throughout this retry.
                let result =
                    join_normal_frozen_endpoints_bounded(primary, replica, publication, stop);
                drop(cause);
                return result;
            }
            NormalFrozenPairRecoveryV1::PrimaryFailed { replica, failed } => {
                (replica, failed, true)
            }
            NormalFrozenPairRecoveryV1::ReplicaFailed { primary, failed } => {
                (primary, failed, false)
            }
        };
        // Provider selection still uses the actual failed source's role.
        let is_primary = failed
            .owned
            .as_ref()
            .is_some_and(|owned| owned.value.source.endpoint.role == EndpointRole::Primary);
        let result = if is_primary {
            failed.recover_bounded(primary_provider, primary_policy, memory, work, stop)
        } else {
            failed.recover_bounded(replica_provider, replica_policy, memory, work, stop)
        };
        let recovered = match result {
            Ok(value) => value,
            Err(failed) => {
                return Err(Self {
                    retained: if was_primary {
                        NormalFrozenPairRecoveryV1::PrimaryFailed {
                            replica: other,
                            failed,
                        }
                    } else {
                        NormalFrozenPairRecoveryV1::ReplicaFailed {
                            primary: other,
                            failed,
                        }
                    },
                });
            }
        };
        let (primary, replica) = if is_primary {
            (recovered, other)
        } else {
            (other, recovered)
        };
        join_normal_frozen_endpoints_bounded(primary, replica, publication, stop)
    }
}

#[path = "endpoint_incremental_normal_frozen_parent.rs"]
mod parent_view;

#[path = "endpoint_incremental_normal_ready_pair.rs"]
mod normal_ready;
pub use normal_ready::{
    NormalReadyCandidateReadViewV1, NormalReadyPairFailureV1, NormalReadyPublicationFailureV1,
    PaidNormalAcceptedPairV1, PaidNormalReadyPairV1,
};
