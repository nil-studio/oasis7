//! Fixed normal producers share GEN's unique body/frame/copy/state/sign kernels.
use super::*;
use crate::controlled_authority::fencing::PaidRemoteLeaseV1;
use crate::controlled_authority::formal::endpoint::store::bounded_transaction::NativePublicationFencingRefsV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    PaidVerifiedNormalDecisionV1, normal_final_receipt_body_bounded,
};

/// Borrow the caller's existing sources and work for one receipt operation.
/// Independent lifetimes preserve each input's original borrow boundary.
pub(in crate::controlled_authority::formal::endpoint) struct ReceiptContext<
    'provider,
    'policy,
    'memory,
    'work,
    'stop,
> {
    pub(in crate::controlled_authority::formal::endpoint) provider:
        &'provider RemoteMonotonicEndpointProvider,
    pub(in crate::controlled_authority::formal::endpoint) policy: &'policy TrustedFencingPolicyV1,
    pub(in crate::controlled_authority::formal::endpoint) memory: &'memory MemoryBudget,
    pub(in crate::controlled_authority::formal::endpoint) work: &'work FormalArchiveReadWorkV1,
    pub(in crate::controlled_authority::formal::endpoint) stop: &'stop mut dyn FnMut() -> bool,
}

struct ReceiptWorkContext<'memory, 'work, 'stop> {
    memory: &'memory MemoryBudget,
    work: &'work FormalArchiveReadWorkV1,
    stop: &'stop mut dyn FnMut() -> bool,
}

/// Establish the true complete job and current durable state before any signer
/// source is created. Raw candidate/role metadata never supplies that evidence.
pub(in crate::controlled_authority::formal::endpoint) fn admit_normal(
    endpoint: &FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    certificate: &FormalFrozenCertificateV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<store::PaidRequestStateV1, FormalError> {
    work.consume_steps(1, stop)?;
    if endpoint.poisoned
        || endpoint.recovery.is_some()
        || candidate.branch != FormalCompletionBranchV1::Normal
        || candidate.candidate.record.kind != FormalRecordKindV1::Execution
        || !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(
            candidate.candidate.record.schema_version,
        )
        || certificate.reservation.body.schema_version != 4
    {
        return Err(invalid("normal Ready exact execution/Frozen profile"));
    }
    if !same(
        &candidate.frozen.as_ref(),
        &Some(certificate),
        memory,
        work,
        stop,
    )? {
        return Err(invalid("normal Ready actual Frozen certificate"));
    }
    let parent = endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("normal Ready actual parent missing"))?;
    // This original fixed bounded kernel verifies writer, actual parent,
    // authority, all Frozen predicates and role-specific receipt context.
    let _body = normal_final_receipt_body_bounded(
        &candidate.candidate,
        certificate,
        &endpoint.authority,
        parent,
        endpoint.role,
        &mut work.bounded_context(memory, stop),
    )?;
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("normal Ready page capability missing"))?;
    if !capability
        .jobs
        .matches_bounded(candidate, memory, work, stop)?
    {
        return Err(invalid(
            "normal Ready genuine complete staged material missing",
        ));
    }
    let _job = capability.jobs.normal_job(parent, memory)?;
    work.charge(
        1,
        (endpoint.store.body.head.record_id.len()
            + candidate.candidate.record.parent_record_id.len()) as u64,
        stop,
    )?;
    if endpoint.store.body.seal.is_some() {
        return Err(FormalError::Conflict("formal epoch sealed".into()));
    }
    if endpoint.store.body.head.record_id != candidate.candidate.record.parent_record_id
        || endpoint.store.body.head.position.checked_add(1)
            != Some(candidate.candidate.record.position)
    {
        return Err(FormalError::Conflict(
            "formal exact canonical parent/next slot".into(),
        ));
    }
    if let Some(id) = &endpoint.store.body.pending {
        let raw = endpoint
            .store
            .raw_object_bounded(id, 64 << 20, memory, work, stop)?
            .ok_or_else(|| invalid("normal Ready pending entry missing"))?;
        let pending = store::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
        if !same(
            &pending.state().request_digest(),
            &candidate.candidate.record.request_digest.as_str(),
            memory,
            work,
            stop,
        )? {
            return Err(FormalError::Conflict(
                "formal one durable reservation slot".into(),
            ));
        }
    }
    let state = endpoint
        .store
        .read_request_bounded(
            &candidate.candidate.record.request_digest,
            memory,
            work,
            stop,
        )?
        .ok_or_else(|| invalid("normal Ready durable Frozen missing"))?;
    match state.state() {
        FormalRequestStateV1::Frozen {
            input,
            material,
            certificate: Some(cert),
            ..
        } => {
            if !same(input, &certificate.reservation, memory, work, stop)?
                || !same(cert, certificate, memory, work, stop)?
                || !same(
                    &candidate.frozen_material.as_ref(),
                    &Some(material),
                    memory,
                    work,
                    stop,
                )?
            {
                return Err(invalid("normal Ready actual durable Frozen originals"));
            }
        }
        FormalRequestStateV1::Prepared { candidate: old, .. }
        | FormalRequestStateV1::Decided { candidate: old, .. }
        | FormalRequestStateV1::Ready { candidate: old, .. } => {
            if !same(old, candidate, memory, work, stop)? {
                return Err(FormalError::Conflict(
                    "normal retained original candidate".into(),
                ));
            }
        }
        _ => return Err(invalid("normal Ready durable preparation state required")),
    }
    Ok(state)
}
fn persist_same_lease(
    endpoint: &mut FileFormalEndpoint,
    lease: &mut Option<PaidRemoteLeaseV1>,
    next: Next<'_>,
    context: ReceiptContext<'_, '_, '_, '_, '_>,
) -> ResultV<()> {
    let ReceiptContext {
        provider,
        policy,
        memory,
        work,
        stop,
    } = context;
    let state = next_state(next, memory, work, stop)?;
    let update = endpoint
        .store
        .prepare_request_update_bounded(&state, memory, work, stop)?;
    let held = lease
        .take()
        .ok_or_else(|| invalid("normal Ready lease requires recovery"))?;
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    let next = endpoint.store.publish_pending_update_with_lease_bounded(
        update,
        held,
        NativePublicationFencingRefsV1 {
            provider,
            instance,
            policy,
        },
        memory,
        work,
        stop,
    )?;
    *lease = Some(next);
    Ok(())
}
fn sign_same_lease<'a>(
    provider: &RemoteMonotonicEndpointProvider,
    lease: &mut Option<PaidRemoteLeaseV1>,
    source: PaidGenesisReceiptSourceV1<'a>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> ResultV<fencing::PaidGenesisReceiptSignatureV1<'a>> {
    let held = lease
        .take()
        .ok_or_else(|| invalid("normal Ready signer lease requires recovery"))?;
    fencing::sign_genesis_receipt_bounded_v1(
        provider,
        held,
        source,
        &mut work.bounded_context(memory, stop),
    )
}
fn normal_prepare_body(
    endpoint: &FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    certificate: &FormalFrozenCertificateV1,
    role: EndpointRole,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<FormalPrepareReceiptBodyV1>, FormalError> {
    let parent = endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("normal Prepare actual parent missing"))?;
    let body = normal_final_receipt_body_bounded(
        &candidate.candidate,
        certificate,
        &endpoint.authority,
        parent,
        role,
        &mut work.bounded_context(memory, stop),
    )?;
    prepare_body_from_final(candidate, &body, memory, work, stop)
}
fn verify_normal_prepare(
    endpoint: &FileFormalEndpoint,
    receipt: &FormalPrepareReceiptV1,
    role: EndpointRole,
    candidate: &FormalCandidateMaterialV1,
    certificate: &FormalFrozenCertificateV1,
    context: ReceiptWorkContext<'_, '_, '_>,
) -> Result<(), FormalError> {
    let ReceiptWorkContext { memory, work, stop } = context;
    let expected = normal_prepare_body(endpoint, candidate, certificate, role, memory, work, stop)?;
    if !same(&receipt.body, &*expected, memory, work, stop)? {
        return Err(invalid("normal Prepare original candidate context"));
    }
    let key = match role {
        EndpointRole::Primary => {
            &endpoint
                .authority
                .definition()
                .primary
                .instance
                .endpoint_public_key
        }
        EndpointRole::Replica => {
            &endpoint
                .authority
                .definition()
                .replica
                .instance
                .endpoint_public_key
        }
    };
    verify_body_signature(
        &receipt.body,
        &receipt.signature_hex,
        key,
        ProtectedEndpointSignaturePurposeV1::PrepareReceipt,
        memory,
        work,
        stop,
    )
}
pub(in crate::controlled_authority::formal::endpoint) fn prepare(
    endpoint: &mut FileFormalEndpoint,
    lease: &mut Option<PaidRemoteLeaseV1>,
    candidate: &FormalCandidateMaterialV1,
    cert: &FormalFrozenCertificateV1,
    context: ReceiptContext<'_, '_, '_, '_, '_>,
) -> ResultV<PaidGenesisPrepareReceiptV1> {
    let ReceiptContext {
        provider,
        policy,
        memory,
        work,
        stop,
    } = context;
    let state = admit_normal(endpoint, candidate, cert, memory, work, stop)?;
    let existing = match state.state() {
        FormalRequestStateV1::Prepared {
            receipt: Some(r), ..
        }
        | FormalRequestStateV1::Decided { prepare: r, .. }
        | FormalRequestStateV1::Ready { prepare: r, .. } => Some(r),
        _ => None,
    };
    if let Some(r) = existing {
        verify_normal_prepare(
            endpoint,
            r,
            endpoint.role,
            candidate,
            cert,
            ReceiptWorkContext {
                memory,
                work,
                stop: &mut *stop,
            },
        )?;
        return Ok(copy_prepare(&r.body, &r.signature_hex, memory, work, stop)?);
    }
    persist_same_lease(
        endpoint,
        lease,
        Next::Prepared {
            candidate,
            receipt: None,
        },
        ReceiptContext {
            provider,
            policy,
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    let body = normal_prepare_body(endpoint, candidate, cert, endpoint.role, memory, work, stop)?;
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    let signed = sign_same_lease(
        provider,
        lease,
        source(
            ReceiptBody::Prepare(body),
            instance,
            &endpoint.store,
            memory,
            work,
            stop,
        )?,
        memory,
        work,
        stop,
    )?;
    let ReceiptBody::Prepare(body) = &signed.source().body else {
        unreachable!("fixed normal Prepare body")
    };
    let receipt = copy_prepare(body, signed.endpoint_signature_hex(), memory, work, stop)?;
    *lease = Some(signed.into_lease());
    persist_same_lease(
        endpoint,
        lease,
        Next::Prepared {
            candidate,
            receipt: Some(receipt.receipt()),
        },
        ReceiptContext {
            provider,
            policy,
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    Ok(receipt)
}
pub(in crate::controlled_authority::formal::endpoint) fn finalize(
    endpoint: &mut FileFormalEndpoint,
    lease: &mut Option<PaidRemoteLeaseV1>,
    candidate: &FormalCandidateMaterialV1,
    cert: &FormalFrozenCertificateV1,
    primary: &PaidGenesisPrepareReceiptV1,
    replica: &PaidGenesisPrepareReceiptV1,
    context: ReceiptContext<'_, '_, '_, '_, '_>,
) -> ResultV<PaidGenesisFinalReceiptV1> {
    let ReceiptContext {
        provider,
        policy,
        memory,
        work,
        stop,
    } = context;
    let state = admit_normal(endpoint, candidate, cert, memory, work, stop)?;
    let (prepare, existing) = match state.state() {
        FormalRequestStateV1::Prepared {
            receipt: Some(p), ..
        } => (p, None),
        FormalRequestStateV1::Decided {
            prepare, receipt, ..
        } => (prepare, Some(receipt)),
        FormalRequestStateV1::Ready {
            prepare, decision, ..
        } => (
            prepare,
            Some(match endpoint.role {
                EndpointRole::Primary => &decision.primary,
                EndpointRole::Replica => &decision.replica,
            }),
        ),
        _ => return Err(invalid("normal Final durable Prepare receipt missing").into()),
    };
    verify_normal_prepare(
        endpoint,
        primary.receipt(),
        EndpointRole::Primary,
        candidate,
        cert,
        ReceiptWorkContext {
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    verify_normal_prepare(
        endpoint,
        replica.receipt(),
        EndpointRole::Replica,
        candidate,
        cert,
        ReceiptWorkContext {
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    let local = match endpoint.role {
        EndpointRole::Primary => primary.receipt(),
        EndpointRole::Replica => replica.receipt(),
    };
    if !same(prepare, local, memory, work, stop)? {
        return Err(invalid("formal decision original local prepare").into());
    }
    let parent = endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("normal Final actual parent missing"))?;
    let body = normal_final_receipt_body_bounded(
        &candidate.candidate,
        cert,
        &endpoint.authority,
        parent,
        endpoint.role,
        &mut work.bounded_context(memory, stop),
    )?;
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    if let Some(receipt) = existing {
        if !same(&receipt.body, &*body, memory, work, stop)? {
            return Err(invalid("normal retained Final context").into());
        }
        verify_body_signature(
            &receipt.body,
            &receipt.signature_hex,
            &instance.endpoint_public_key,
            ProtectedEndpointSignaturePurposeV1::FinalReceipt,
            memory,
            work,
            stop,
        )?;
        return Ok(copy_final(
            &receipt.body,
            &receipt.signature_hex,
            memory,
            work,
            stop,
        )?);
    }
    let signed = sign_same_lease(
        provider,
        lease,
        source(
            ReceiptBody::Final(body),
            instance,
            &endpoint.store,
            memory,
            work,
            stop,
        )?,
        memory,
        work,
        stop,
    )?;
    let ReceiptBody::Final(body) = &signed.source().body else {
        unreachable!("fixed normal Final body")
    };
    let receipt = copy_final(body, signed.endpoint_signature_hex(), memory, work, stop)?;
    *lease = Some(signed.into_lease());
    persist_same_lease(
        endpoint,
        lease,
        Next::Decided {
            candidate,
            prepare,
            receipt: receipt.receipt(),
        },
        ReceiptContext {
            provider,
            policy,
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    Ok(receipt)
}
pub(in crate::controlled_authority::formal::endpoint) fn complete(
    endpoint: &mut FileFormalEndpoint,
    lease: &mut Option<PaidRemoteLeaseV1>,
    candidate: &FormalCandidateMaterialV1,
    cert: &FormalFrozenCertificateV1,
    decision: &PaidVerifiedNormalDecisionV1,
    context: ReceiptContext<'_, '_, '_, '_, '_>,
) -> ResultV<PaidGenesisCompleteReceiptV1> {
    let ReceiptContext {
        provider,
        policy,
        memory,
        work,
        stop,
    } = context;
    decision.validate_context(&mut work.bounded_context(memory, stop))?;
    let state = admit_normal(endpoint, candidate, cert, memory, work, stop)?;
    let raw = decision.decision();
    let (prepare, local, existing) = match state.state() {
        FormalRequestStateV1::Decided {
            prepare, receipt, ..
        } => (prepare, receipt, None),
        FormalRequestStateV1::Ready {
            prepare,
            decision: old,
            complete,
            ..
        } => {
            if !same(old, raw, memory, work, stop)? {
                return Err(
                    FormalError::Conflict("normal retained original decision".into()).into(),
                );
            }
            (
                prepare,
                match endpoint.role {
                    EndpointRole::Primary => &old.primary,
                    EndpointRole::Replica => &old.replica,
                },
                complete.as_ref(),
            )
        }
        _ => return Err(invalid("normal Complete durable Final missing").into()),
    };
    let expected_local = match endpoint.role {
        EndpointRole::Primary => &raw.primary,
        EndpointRole::Replica => &raw.replica,
    };
    if !same(local, expected_local, memory, work, stop)? {
        return Err(invalid("formal proof installation exact retained original decision").into());
    }
    let body = decision
        .complete_receipt_body_bounded(endpoint.role, &mut work.bounded_context(memory, stop))?;
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    if let Some(receipt) = existing {
        if !same(&receipt.body, &*body, memory, work, stop)? {
            return Err(invalid("normal retained Complete context").into());
        }
        verify_body_signature(
            &receipt.body,
            &receipt.signature_hex,
            &instance.endpoint_public_key,
            ProtectedEndpointSignaturePurposeV1::ProofComplete,
            memory,
            work,
            stop,
        )?;
        return Ok(copy_complete(
            &receipt.body,
            &receipt.signature_hex,
            memory,
            work,
            stop,
        )?);
    }
    persist_same_lease(
        endpoint,
        lease,
        Next::Ready {
            candidate,
            prepare,
            decision: raw,
            complete: None,
        },
        ReceiptContext {
            provider,
            policy,
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    let signed = sign_same_lease(
        provider,
        lease,
        source(
            ReceiptBody::Complete(body),
            instance,
            &endpoint.store,
            memory,
            work,
            stop,
        )?,
        memory,
        work,
        stop,
    )?;
    let ReceiptBody::Complete(body) = &signed.source().body else {
        unreachable!("fixed normal Complete body")
    };
    let receipt = copy_complete(body, signed.endpoint_signature_hex(), memory, work, stop)?;
    *lease = Some(signed.into_lease());
    persist_same_lease(
        endpoint,
        lease,
        Next::Ready {
            candidate,
            prepare,
            decision: raw,
            complete: Some(receipt.receipt()),
        },
        ReceiptContext {
            provider,
            policy,
            memory,
            work,
            stop: &mut *stop,
        },
    )?;
    Ok(receipt)
}
