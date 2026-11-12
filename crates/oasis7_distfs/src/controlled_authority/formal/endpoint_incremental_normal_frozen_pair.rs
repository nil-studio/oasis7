//! Dual receipt issuer. Only actual completed endpoint owners enter here.
use super::*;

pub struct PaidNormalFrozenPairV1 {
    primary: PaidNormalFrozenEndpointV1,
    replica: PaidNormalFrozenEndpointV1,
    certificate: ReservedValue<FormalFrozenCertificateV1>,
    _outer: ReservedValue<()>,
}
pub struct NormalFrozenPairFailureV1 {
    primary: Option<PaidNormalFrozenEndpointV1>,
    replica: Option<PaidNormalFrozenEndpointV1>,
    failed: Option<NormalFrozenFailureV1>,
    cause: BoundedAcquireErrorV1,
}
impl std::fmt::Debug for NormalFrozenPairFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalFrozenPairFailureV1")
            .field("cause", &self.cause)
            .finish()
    }
}
impl std::fmt::Display for NormalFrozenPairFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "normal dual Frozen: {:?}", self.cause)
    }
}
impl std::error::Error for NormalFrozenPairFailureV1 {}
impl PaidNormalFrozenPairV1 {
    pub fn certificate(&self) -> &FormalFrozenCertificateV1 {
        &self.certificate
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.primary.uses_budget(memory)
            && self.replica.uses_budget(memory)
            && self.certificate.uses_budget(memory)
            && self._outer.uses_budget(memory)
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
        let source = &self.primary.source;
        if source.identity != work.bounded_context(memory, stop).work().identity() {
            return Err(invalid("normal Frozen pair Core"));
        }
        let parent = source
            .endpoint
            .parent
            .as_ref()
            .ok_or_else(|| invalid("normal Frozen parent missing"))?;
        material::check_material(
            &self.primary.original,
            &self.primary.reservation,
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
        let (input, material) = state::input(&endpoint.original)?;
        let complete = state::state(
            input,
            material,
            Some(&endpoint.receipt),
            Some(certificate),
            memory,
            work,
            stop,
        )?;
        endpoint
            .source
            .endpoint
            .store
            .prepare_request_update_bounded(&complete, memory, work, stop)
    })();
    let update = match result {
        Ok(value) => value,
        Err(cause) => {
            drop(endpoint.source.lease.take());
            return Err(failure(
                NormalFrozenRecoveryV1 {
                    source: endpoint.source,
                    cause: cause.into(),
                },
                endpoint.original,
                endpoint.reservation,
            ));
        }
    };
    endpoint.source = match endpoint
        .source
        .publish(update, provider, policy, memory, work, stop)
    {
        Ok(value) => value,
        Err(recovery) => return Err(failure(recovery, endpoint.original, endpoint.reservation)),
    };
    Ok(endpoint)
}
pub fn join_normal_frozen_endpoints_bounded(
    primary: PaidNormalFrozenEndpointV1,
    replica: PaidNormalFrozenEndpointV1,
    primary_provider: &RemoteMonotonicEndpointProvider,
    replica_provider: &RemoteMonotonicEndpointProvider,
    primary_policy: &TrustedFencingPolicyV1,
    replica_policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<PaidNormalFrozenPairV1, NormalFrozenPairFailureV1> {
    let prepared = (|| {
        work.consume_steps(1, stop)?;
        if !primary.uses_budget(memory)
            || !replica.uses_budget(memory)
            || primary.source.identity != replica.source.identity
            || primary.source.identity != work.bounded_context(memory, stop).work().identity()
            || primary.source.endpoint.role != EndpointRole::Primary
            || replica.source.endpoint.role != EndpointRole::Replica
        {
            return Err(invalid("normal Frozen dual role/pool/Core"));
        }
        // Every receipt here was produced by its actual protected endpoint and
        // verified before its private owner was minted. Compare the originals,
        // not merely caller-supplied receipt digests or role booleans.
        if !store::bounded_transaction::same(
            primary.source.endpoint.authority.definition(),
            replica.source.endpoint.authority.definition(),
            memory,
            work,
            stop,
        )? || !store::bounded_transaction::same(
            primary.original.state(),
            replica.original.state(),
            memory,
            work,
            stop,
        )? {
            return Err(invalid("normal Frozen dual originals/authority"));
        }
        let (input, _) = state::input(&primary.original)?;
        let measured = store::context_wire::encode_paid(
            &(input, &*primary.receipt, &*replica.receipt),
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
            .reserve(size_of::<PaidNormalFrozenPairV1>(), MemoryClass::Ordinary)
            .map_err(|_| invalid("normal Frozen pair admission"))?;
        work.charge(1, measured.len() as u64, stop)?;
        let certificate = fee.retain(FormalFrozenCertificateV1 {
            reservation: input.clone(),
            primary: (*primary.receipt).clone(),
            replica: (*replica.receipt).clone(),
        });
        let _verified_id = oasis7_formal_protocol::controlled_authority::formal::verify_normal_frozen_certificate_bounded(
            &certificate, primary.source.endpoint.authority.definition(), primary.source.endpoint.authority.activation_ref(),
            &mut work.bounded_context(memory, stop))?;
        Ok((certificate, outer.retain(())))
    })();
    let (certificate, outer) = match prepared {
        Ok(value) => value,
        Err(cause) => {
            return Err(NormalFrozenPairFailureV1 {
                primary: Some(primary),
                replica: Some(replica),
                failed: None,
                cause: cause.into(),
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
                primary: None,
                replica: Some(replica),
                cause: FormalError::Conflict(
                    "primary certificate persistence requires recovery".into(),
                )
                .into(),
                failed: Some(failed),
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
                primary: Some(primary),
                replica: None,
                cause: FormalError::Conflict(
                    "replica certificate persistence requires recovery".into(),
                )
                .into(),
                failed: Some(failed),
            });
        }
    };
    Ok(PaidNormalFrozenPairV1 {
        primary,
        replica,
        certificate,
        _outer: outer,
    })
}

impl NormalFrozenPairFailureV1 {
    /// A certificate write may have committed at one endpoint before failure at
    /// the other. Preserve both holders and retry only after the failed side's
    /// actual disk/provider reconciliation. Reinstalling the identical cert is
    /// metadata-only and keeps the original receipt and parent lease.
    pub fn recover_bounded(
        self,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<PaidNormalFrozenPairV1, Self> {
        let Self {
            mut primary,
            mut replica,
            failed,
            cause,
        } = self;
        if let Some(failed) = failed {
            let is_primary = failed
                .source
                .as_ref()
                .is_some_and(|recovery| recovery.source.endpoint.role == EndpointRole::Primary);
            let result = if is_primary {
                failed.recover_bounded(primary_provider, primary_policy, memory, work, stop)
            } else {
                failed.recover_bounded(replica_provider, replica_policy, memory, work, stop)
            };
            match result {
                Ok(value) if is_primary => primary = Some(value),
                Ok(value) => replica = Some(value),
                Err(failed) => {
                    return Err(Self {
                        primary,
                        replica,
                        failed: Some(failed),
                        cause,
                    });
                }
            }
        }
        match (primary, replica) {
            (Some(primary), Some(replica)) => join_normal_frozen_endpoints_bounded(
                primary,
                replica,
                primary_provider,
                replica_provider,
                primary_policy,
                replica_policy,
                memory,
                work,
                stop,
            ),
            (primary, replica) => Err(Self {
                primary,
                replica,
                failed: None,
                cause,
            }),
        }
    }
}
