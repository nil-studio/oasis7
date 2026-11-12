//! Same-parent-lease schema-4 Frozen persistence. No ordinary execution permit.
use super::*;
use crate::controlled_authority::formal::endpoint::store;
use oasis7_formal_protocol::controlled_authority::formal::{
    CheckedIncrementalFrozenMaterialV1, IncrementalFrozenMaterialErrorV1,
    IncrementalFrozenMaterialWorkV1, VerifiedFormalReservationV1,
    bounded_context::FormalBoundedContextV1, normal_frozen_hash_bounded,
    verify_formal_reservation_bounded, verify_incremental_frozen_material_v1,
};
#[path = "endpoint_incremental_normal_frozen_material.rs"]
mod material;
#[path = "endpoint_incremental_normal_frozen_pair.rs"]
mod pair;
#[path = "endpoint_incremental_normal_frozen_receipt.rs"]
mod receipt;
#[path = "endpoint_incremental_normal_frozen_source.rs"]
mod source;
#[path = "endpoint_incremental_normal_frozen_state.rs"]
mod state;
pub use pair::{
    NormalFrozenPairFailureV1, PaidNormalFrozenPairV1, join_normal_frozen_endpoints_bounded,
};
pub(in crate::controlled_authority) use receipt::PaidNormalFrozenReceiptSourceV1;
use source::{NormalFrozenRecoveryV1, NormalFrozenTransactionV1};
use store::bounded_request::PaidRequestStateV1;

/// A complete one-endpoint receipt still needs the other endpoint and durable
/// certificate installation. Its fields cannot be constructed or detached.
pub struct PaidNormalFrozenEndpointV1 {
    source: NormalFrozenTransactionV1,
    original: PaidRequestStateV1,
    reservation: ReservedValue<VerifiedFormalReservationV1>,
    receipt: ReservedValue<FormalFrozenReceiptV1>,
}
pub struct NormalFrozenFailureV1 {
    source: Option<NormalFrozenRecoveryV1>,
    original: Option<PaidRequestStateV1>,
    reservation: Option<ReservedValue<VerifiedFormalReservationV1>>,
    cause: BoundedAcquireErrorV1,
}
impl std::fmt::Debug for NormalFrozenFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalFrozenFailureV1")
            .field("cause", &self.cause)
            .field("requires_recovery", &self.source.is_some())
            .finish()
    }
}
impl std::fmt::Display for NormalFrozenFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "normal Frozen: {:?}", self.cause)
    }
}
impl std::error::Error for NormalFrozenFailureV1 {}
impl From<BoundedAcquireErrorV1> for NormalFrozenFailureV1 {
    fn from(cause: BoundedAcquireErrorV1) -> Self {
        Self {
            source: None,
            original: None,
            reservation: None,
            cause,
        }
    }
}
impl From<FormalError> for NormalFrozenFailureV1 {
    fn from(cause: FormalError) -> Self {
        BoundedAcquireErrorV1::from(cause).into()
    }
}
impl PaidNormalFrozenEndpointV1 {
    pub fn receipt(&self) -> &FormalFrozenReceiptV1 {
        &self.receipt
    }
    pub fn reservation(&self) -> &VerifiedFormalReservationV1 {
        &self.reservation
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.original.uses_budget(memory)
            && self.reservation.uses_budget(memory)
            && self.receipt.uses_budget(memory)
            && self.source.endpoint_fee.uses_budget(memory)
            && self.source._outer.uses_budget(memory)
            && self
                .source
                .lease
                .as_ref()
                .is_some_and(|lease| lease.uses_budget(memory))
    }
}
fn clone_receipt(
    value: &FormalFrozenReceiptV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<FormalFrozenReceiptV1>, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let measured = store::context_wire::encode_paid(
        value,
        64 << 20,
        &mut ctx,
        "formal object byte limit",
        "retained Frozen receipt",
    )?;
    let n = measured
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(size_of::<FormalFrozenReceiptV1>() + 4096))
        .ok_or_else(|| invalid("retained Frozen receipt layout"))?;
    let fee = memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("retained Frozen receipt admission"))?;
    ctx.charge(1, measured.len() as u64)?;
    Ok(fee.retain(value.clone()))
}
fn exact_receipt(
    endpoint: &FileFormalEndpoint,
    input: &SignedFormalFrozenV1,
    material: &ClosedRecord,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<ReservedValue<FormalFrozenReceiptV1>>, FormalError> {
    let Some(retained) =
        endpoint
            .store
            .read_request_bounded(&input.body.request_digest, memory, work, stop)?
    else {
        return Ok(None);
    };
    let found = match retained.state() {
        FormalRequestStateV1::Frozen {
            input: old,
            material: old_material,
            receipt,
            ..
        } if store::bounded_transaction::same(old, input, memory, work, stop)?
            && store::bounded_transaction::same(old_material, material, memory, work, stop)? =>
        {
            receipt.as_ref()
        }
        other => {
            let candidate = other.candidate().ok_or_else(|| {
                FormalError::Conflict("formal request identity already bound".into())
            })?;
            let certificate = candidate.frozen.as_ref().ok_or_else(|| {
                FormalError::Conflict("formal request identity already bound".into())
            })?;
            let old_material = candidate.frozen_material.as_ref().ok_or_else(|| {
                FormalError::Conflict("formal request identity already bound".into())
            })?;
            if !store::bounded_transaction::same(
                &certificate.reservation,
                input,
                memory,
                work,
                stop,
            )? || !store::bounded_transaction::same(old_material, material, memory, work, stop)?
            {
                return Err(FormalError::Conflict(
                    "formal request identity already bound".into(),
                ));
            }
            Some(match endpoint.role {
                EndpointRole::Primary => &certificate.primary,
                EndpointRole::Replica => &certificate.replica,
            })
        }
    };
    found
        .map(|value| clone_receipt(value, memory, work, stop))
        .transpose()
}
impl PaidAcceptedGenesisHostSourceV1 {
    /// Historical exact retry is evaluated before any fresh-parent check. This
    /// returns only the persisted role receipt, never a new execution owner.
    pub fn exact_frozen_receipt_bounded(
        &self,
        input: &SignedFormalFrozenV1,
        material: &ClosedRecord,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<Option<ReservedValue<FormalFrozenReceiptV1>>, FormalError> {
        {
            let mut ctx = work.bounded_context(memory, stop);
            self.validate_context(&mut ctx)?;
        }
        exact_receipt(&self.endpoint, input, material, memory, work, stop)
    }
}
fn fresh_slot(
    endpoint: &FileFormalEndpoint,
    input: &SignedFormalFrozenV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    work.charge(1, input.body.parent.record_id.len() as u64 + 16, stop)?;
    if endpoint.store.body.seal.is_some() {
        return Err(FormalError::Conflict("formal epoch sealed".into()));
    }
    if endpoint.store.body.head.position.checked_add(1) != Some(input.body.next_position)
        || endpoint.store.body.head.record_id != input.body.parent.record_id
    {
        return Err(FormalError::Conflict(
            "formal exact canonical parent/next slot".into(),
        ));
    }
    if let Some(id) = &endpoint.store.body.pending {
        let raw = endpoint
            .store
            .raw_object_bounded(id, 64 << 20, memory, work, stop)?
            .ok_or_else(|| invalid("normal Frozen pending entry missing"))?;
        let pending = store::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
        work.charge(
            1,
            (pending.state().request_digest().len() + input.body.request_digest.len()) as u64,
            stop,
        )?;
        if pending.state().request_digest() != input.body.request_digest {
            return Err(FormalError::Conflict(
                "formal one durable reservation slot".into(),
            ));
        }
    }
    Ok(())
}
impl PaidAcceptedGenesisHostSourceV1 {
    /// The actual source and unique FD move into this operation. Every failed
    /// publication retains a private recovery owner rather than a live source.
    pub fn freeze_normal_bounded(
        self,
        input: &SignedFormalFrozenV1,
        material: &ClosedRecord,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        material_work: &mut IncrementalFrozenMaterialWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<PaidNormalFrozenEndpointV1, NormalFrozenFailureV1> {
        let original = state::state(input, material, None, None, memory, work, stop)?;
        // Persistent exact matches are inspected before fresh-parent admission.
        let retained_receipt = exact_receipt(&self.endpoint, input, material, memory, work, stop)?;
        // The private execution owner requires its original live parent. A
        // separate retained-receipt API below can return historical originals.
        fresh_slot(&self.endpoint, input, memory, work, stop)?;
        let parent = self.commit()?;
        let reservation = {
            let mut ctx = work.bounded_context(memory, stop);
            verify_formal_reservation_bounded(input, &self.endpoint.authority, parent, &mut ctx)?
        };
        material::check_material(
            &original,
            &reservation,
            parent,
            memory,
            work,
            material_work,
            stop,
        )?;
        work.charge(1, 128, stop)?;
        if parent.record().artifact_roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment)
            != material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment)
        { return Err(invalid("native frozen environment differs from committed parent").into()); }
        let source = self.consume_normal_frozen_source(memory, work, stop)?;
        if let Some(receipt) = retained_receipt {
            return Ok(PaidNormalFrozenEndpointV1 {
                source,
                original,
                reservation,
                receipt,
            });
        }
        finish_freeze(
            source,
            original,
            reservation,
            provider,
            policy,
            memory,
            work,
            stop,
        )
    }
}
fn failure(
    recovery: NormalFrozenRecoveryV1,
    original: PaidRequestStateV1,
    reservation: ReservedValue<VerifiedFormalReservationV1>,
) -> NormalFrozenFailureV1 {
    NormalFrozenFailureV1 {
        cause: recovery.cause,
        source: Some(NormalFrozenRecoveryV1 {
            source: recovery.source,
            cause: FormalError::Conflict(
                "normal Frozen outcome requires disk reconciliation".into(),
            )
            .into(),
        }),
        original: Some(original),
        reservation: Some(reservation),
    }
}
fn finish_freeze(
    source: NormalFrozenTransactionV1,
    original: PaidRequestStateV1,
    reservation: ReservedValue<VerifiedFormalReservationV1>,
    provider: &RemoteMonotonicEndpointProvider,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNormalFrozenEndpointV1, NormalFrozenFailureV1> {
    let update = source
        .endpoint
        .store
        .prepare_request_update_bounded(&original, memory, work, stop)?;
    let mut source = match source.publish(update, provider, policy, memory, work, stop) {
        Ok(source) => source,
        Err(recovery) => return Err(failure(recovery, original, reservation)),
    };
    let signed = (|| {
        let lease = source
            .lease
            .as_ref()
            .ok_or_else(|| invalid("normal Frozen lease missing"))?;
        let receipt_source = receipt::source(
            &source.endpoint,
            lease,
            &reservation,
            &original,
            memory,
            work,
            stop,
        )?;
        let lease = source
            .lease
            .take()
            .ok_or_else(|| invalid("normal Frozen lease missing"))?;
        let mut ctx = work.bounded_context(memory, stop);
        let signed = fencing::sign_normal_frozen_receipt_bounded_v1(
            provider,
            lease,
            receipt_source,
            &mut ctx,
        )?;
        let receipt = signed
            .source()
            .receipt(signed.endpoint_signature_hex(), &mut ctx)?;
        let lease = signed.into_lease();
        Ok::<_, BoundedAcquireErrorV1>((receipt, lease))
    })();
    let (receipt, lease) = match signed {
        Ok(value) => value,
        Err(cause) => {
            drop(source.lease.take());
            return Err(failure(
                NormalFrozenRecoveryV1 { source, cause },
                original,
                reservation,
            ));
        }
    };
    source.lease = Some(lease);
    let (input, material) = state::input(&original)?;
    let complete = match state::state(input, material, Some(&receipt), None, memory, work, stop) {
        Ok(value) => value,
        Err(cause) => {
            drop(source.lease.take());
            return Err(failure(
                NormalFrozenRecoveryV1 {
                    source,
                    cause: cause.into(),
                },
                original,
                reservation,
            ));
        }
    };
    let update = match source
        .endpoint
        .store
        .prepare_request_update_bounded(&complete, memory, work, stop)
    {
        Ok(value) => value,
        Err(cause) => {
            drop(source.lease.take());
            return Err(failure(
                NormalFrozenRecoveryV1 {
                    source,
                    cause: cause.into(),
                },
                original,
                reservation,
            ));
        }
    };
    let source = match source.publish(update, provider, policy, memory, work, stop) {
        Ok(source) => source,
        Err(recovery) => return Err(failure(recovery, original, reservation)),
    };
    Ok(PaidNormalFrozenEndpointV1 {
        source,
        original,
        reservation,
        receipt,
    })
}

impl NormalFrozenFailureV1 {
    /// Reconcile the real disk/provider pending intent before acquiring a new
    /// FD. An unknown prior Begin is never replayed through its closed lease.
    pub fn recover_bounded(
        self,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<PaidNormalFrozenEndpointV1, Self> {
        let Self {
            source,
            original,
            reservation,
            cause,
        } = self;
        let (Some(mut recovery), Some(original), Some(reservation)) =
            (source, original, reservation)
        else {
            return Err(Self {
                source: None,
                original: None,
                reservation: None,
                cause,
            });
        };
        let result = (|| {
            let source = &mut recovery.source;
            work.consume_steps(1, stop)?;
            if source.identity != work.bounded_context(memory, stop).work().identity()
                || !source.endpoint_fee.uses_budget(memory)
                || !source._outer.uses_budget(memory)
            {
                return Err(invalid("normal Frozen recovery pool/Core").into());
            }
            drop(source.lease.take());
            let endpoint = &mut *source.endpoint;
            let instance = match endpoint.role {
                EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
                EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
            };
            endpoint.store.reconcile_pending_update_with_scope(
                provider,
                instance,
                policy,
                memory,
                work,
                stop,
                &store::bounded_transaction::RecoveryScope::Strict,
            )?;
            let lease = {
                let mut ctx = work.bounded_context(memory, stop);
                fencing::acquire_provider_lease_bounded_v1(
                    provider,
                    instance,
                    &endpoint.store.checkpoint,
                    policy,
                    &mut ctx,
                )?
            };
            source.lease = Some(lease);
            let (input, material) = state::input(&original)?;
            // Exact persistent receipt first, including a certificate installed
            // just before the transport failed. No parent advancement veto here.
            let receipt = exact_receipt(endpoint, input, material, memory, work, stop)?;
            // Historical retry can return a receipt through the separate data
            // API; a live execution holder must still have its original parent.
            fresh_slot(endpoint, input, memory, work, stop)?;
            Ok::<_, BoundedAcquireErrorV1>(receipt)
        })();
        match result {
            Ok(Some(receipt)) => Ok(PaidNormalFrozenEndpointV1 {
                source: recovery.source,
                original,
                reservation,
                receipt,
            }),
            Ok(None) => finish_freeze(
                recovery.source,
                original,
                reservation,
                provider,
                policy,
                memory,
                work,
                stop,
            ),
            Err(cause) => {
                drop(recovery.source.lease.take());
                Err(failure(
                    NormalFrozenRecoveryV1 {
                        source: recovery.source,
                        cause,
                    },
                    original,
                    reservation,
                ))
            }
        }
    }
}
