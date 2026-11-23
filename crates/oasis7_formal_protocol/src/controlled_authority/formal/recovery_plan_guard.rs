//! Checks for the actual protected signer/storage CAS owner. Every check runs
//! under its existing exclusive lease, including the recovery credential fence.
use super::*;
use crate::controlled_authority::formal::proof::NativeHotBodyV1;
use crate::controlled_authority::replicated_protocol::EndpointRole;

pub fn validate_recovery_provider_credential(
    previous: &NativeHotBodyV1,
    credential_public_key: &str,
    mutation: bool,
) -> Result<(), FormalError> {
    if mutation
        && previous.recovery_plan.as_ref().is_some_and(|plan| {
            plan.body.authorization.recovery_credential_public_key != credential_public_key
        })
    {
        return Err(invalid(
            "old provider credential cannot mutate a planned recovery",
        ));
    }
    Ok(())
}

pub fn validate_recovery_checkpoint_transition(
    previous: &NativeHotBodyV1,
    next: &NativeHotBodyV1,
    authority: &VerifiedAuthorityV1,
) -> Result<(), FormalError> {
    if (previous.recovery_plan.is_some() || next.recovery_plan.is_some())
        && previous.revision.checked_add(1) != Some(next.revision)
    {
        return Err(invalid(
            "recovery checkpoint revision must advance exactly once",
        ));
    }
    if previous.seal.is_some() {
        return Err(invalid("sealed recovery checkpoint cannot mutate"));
    }
    if let Some(healthy) = &next.healthy_seal_authorization {
        let seal = next
            .seal
            .as_ref()
            .ok_or_else(|| invalid("healthy seal authorization without native seal"))?;
        let mut expected = previous.clone();
        expected.revision = next.revision;
        expected.healthy_seal_authorization = Some(healthy.clone());
        expected.seal = Some(seal.clone());
        if previous.healthy_seal_authorization.is_some()
            || previous.recovery_plan.is_some()
            || previous.revision.checked_add(1) != Some(next.revision)
            || seal.checkpoint_before.revision != previous.revision
            || next != &expected
        {
            return Err(invalid(
                "healthy sealing may only add its exact verified authorization and seal",
            ));
        }
        healthy.verify_seal(previous, authority, seal)?;
        return Ok(());
    }
    if previous.healthy_seal_authorization.is_some()
        || (authority.definition().profile_version == 2
            && next.seal.is_some()
            && next.recovery_plan.is_none())
    {
        return Err(invalid(
            "new profile2 seal requires its complete recovery plan or healthy authorization",
        ));
    }
    if let Some(plan) = &previous.recovery_plan {
        plan.verify(authority)?;
        let ack = previous
            .writer_plan_ack
            .as_ref()
            .ok_or_else(|| invalid("planned checkpoint writer acknowledgement missing"))?;
        ack.verify(plan, authority)?;
        if let Some(drained) = &previous.recovery_drained {
            let cut = drained.verify(plan, authority)?;
            let seal = next.seal.as_ref().ok_or_else(|| {
                invalid("drained recovery permits only its exact seal transition")
            })?;
            let writer = seal
                .writer_seal
                .as_ref()
                .ok_or_else(|| invalid("recovery independent writer seal missing"))?;
            writer.verify_with_ack(plan, authority, ack)?;
            let mut expected = previous.clone();
            expected.revision = next.revision;
            expected.seal = Some(seal.clone());
            expected.recovery_drained = None;
            if next != &expected
                || writer.drained != *drained
                || next.head != cut
                || next.pending.is_some()
                || next.recovery_progress.is_some()
                || seal.recovery_plan.as_ref() != Some(plan)
            {
                return Err(invalid(
                    "drained recovery seal changed original proof, plan or private state",
                ));
            }
            seal.validate()
                .map_err(|error| invalid(error.to_string()))?;
            return Ok(());
        }
        let prior = previous
            .recovery_progress
            .as_ref()
            .ok_or_else(|| invalid("recovery progress missing before draining"))?;
        prior.verify(plan, authority)?;
        if next.seal.is_some() {
            return Err(invalid(
                "recovery cannot seal before actual original qualification",
            ));
        }
        let mut expected = previous.clone();
        expected.revision = next.revision;
        expected.index = next.index.clone();
        expected.pending = next.pending.clone();
        expected.decision_head = next.decision_head.clone();
        expected.head = next.head.clone();
        expected.head_node = next.head_node.clone();
        expected.data_closure = next.data_closure.clone();
        expected.recovery_progress = next.recovery_progress.clone();
        expected.recovery_drained = next.recovery_drained.clone();
        if next != &expected {
            return Err(invalid(
                "draining changed original recovery authority or private handoff",
            ));
        }
        if let Some(drained) = &next.recovery_drained {
            let cut = drained.verify(plan, authority)?;
            if drained.fixed != prior.fixed
                || prior.decision.as_ref() != Some(&drained.qualified.decision)
                || next.head != cut
                || next.pending.is_some()
                || next.decision_head.is_some()
                || next.head_node.is_none()
                || next.recovery_progress.is_some()
            {
                return Err(invalid(
                    "draining publication must close exactly the original decision",
                ));
            }
        } else {
            let progress = next
                .recovery_progress
                .as_ref()
                .ok_or_else(|| invalid("draining progress removed"))?;
            progress.verify(plan, authority)?;
            if progress.fixed != prior.fixed
                || prior
                    .candidate
                    .as_ref()
                    .is_some_and(|candidate| progress.candidate.as_ref() != Some(candidate))
                || prior
                    .candidate_material_digest
                    .as_ref()
                    .is_some_and(|digest| {
                        progress.candidate_material_digest.as_ref() != Some(digest)
                    })
                || prior
                    .decision
                    .as_ref()
                    .is_some_and(|decision| progress.decision.as_ref() != Some(decision))
                || next.head != previous.head
                || next.head != plan.body.intent.sealed_cut
                || next.head_node != previous.head_node
                || next.pending.is_none()
                || previous
                    .decision_head
                    .as_ref()
                    .is_some_and(|head| next.decision_head.as_ref() != Some(head))
                || next.decision_head.as_ref().is_some_and(|head| {
                    progress.candidate.as_ref().is_none_or(|candidate| {
                        candidate.record.position != head.position
                            || !candidate
                                .record
                                .record_id()
                                .is_ok_and(|id| id == head.record_id)
                    })
                })
            {
                return Err(invalid(
                    "draining may advance only the original immutable candidate/decision",
                ));
            }
        }
    } else if let Some(plan) = &next.recovery_plan {
        plan.verify(authority)?;
        let ack = next
            .writer_plan_ack
            .as_ref()
            .ok_or_else(|| invalid("native recovery plan writer acknowledgement missing"))?;
        ack.verify(plan, authority)?;
        let mut expected = previous.clone();
        expected.revision = next.revision;
        expected.recovery_plan = Some(plan.clone());
        expected.writer_plan_ack = Some(ack.clone());
        expected.recovery_handoff = next.recovery_handoff.clone();
        expected.recovery_progress = next.recovery_progress.clone();
        expected.recovery_drained = next.recovery_drained.clone();
        if next != &expected
            || next.recovery_handoff.is_none()
            || plan.body.intent.sealed_cut != previous.head
            || plan.body.intent.predecessor_authority_id != previous.authority_id
            || previous.authority_id != authority.authority_id()?
        {
            return Err(invalid(
                "native recovery plan CAS may not replace original checkpoint",
            ));
        }
        hash_hex(next.recovery_handoff.as_ref().expect("checked handoff"))?;
        match (&next.recovery_progress, &next.recovery_drained) {
            (Some(progress), None) if previous.pending.is_some() => {
                progress.verify(plan, authority)?
            }
            (None, Some(drained))
                if previous.pending.is_none()
                    && drained.verify(plan, authority)? == previous.head
                    && matches!(drained.fixed, RecoveryTailWitnessV1::Empty) => {}
            _ => {
                return Err(invalid(
                    "native plan must preserve complete fixed progress or actual empty-cut qualification",
                ));
            }
        }
    } else if next.writer_plan_ack.is_some()
        || previous.writer_plan_ack.is_some()
        || next.recovery_progress.is_some()
        || next.recovery_drained.is_some()
    {
        return Err(invalid("recovery progress without native plan"));
    }
    Ok(())
}

pub fn validate_recovery_protected_signing_request(
    previous: &NativeHotBodyV1,
    authority: &VerifiedAuthorityV1,
    request: &fencing::ProtectedEndpointSigningRequestV1,
) -> Result<(), FormalError> {
    use fencing::ProtectedEndpointSignaturePurposeV1 as Purpose;
    request
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    match request.purpose {
        Purpose::NativeCheckpoint => {
            let next: NativeHotBodyV1 = decode_request(request)?;
            if next
                .seal
                .as_ref()
                .is_some_and(|seal| seal.checkpoint_before != request.checkpoint)
            {
                return Err(invalid(
                    "native seal must consume the exact protected checkpoint",
                ));
            }
            return validate_recovery_checkpoint_transition(previous, &next, authority);
        }
        Purpose::EpochSeal => {
            let seal = previous
                .seal
                .as_ref()
                .ok_or_else(|| invalid("endpoint cannot sign a seal before its native CAS"))?;
            if bytes(request.purpose.domain(), seal)? != request.preimage {
                return Err(invalid(
                    "endpoint can sign only its exact persisted native seal",
                ));
            }
            return Ok(());
        }
        _ => {}
    }
    if previous.seal.is_some() && request.purpose.mutation() {
        return Err(invalid("sealed epoch cannot issue mutation signatures"));
    }
    let Some(plan) = &previous.recovery_plan else {
        return Ok(());
    };
    match request.purpose {
        Purpose::RpcResponse => Ok(()),
        _ => {
            if previous.recovery_drained.is_some() || previous.seal.is_some() {
                return Err(invalid(
                    "drained old epoch cannot issue more mutation signatures",
                ));
            }
            let progress = previous
                .recovery_progress
                .as_ref()
                .ok_or_else(|| invalid("planned signing original progress missing"))?;
            progress.verify(plan, authority)?;
            let role = if previous.instance_digest
                == authority
                    .definition()
                    .primary
                    .instance
                    .digest()
                    .map_err(|error| invalid(error.to_string()))?
            {
                EndpointRole::Primary
            } else if previous.instance_digest
                == authority
                    .definition()
                    .replica
                    .instance
                    .digest()
                    .map_err(|error| invalid(error.to_string()))?
            {
                EndpointRole::Replica
            } else {
                return Err(invalid("planned signing endpoint instance"));
            };
            let frozen = match &progress.fixed {
                RecoveryTailWitnessV1::Frozen(frozen)
                | RecoveryTailWitnessV1::Prepared { frozen, .. }
                | RecoveryTailWitnessV1::Decision { frozen, .. } => frozen,
                RecoveryTailWitnessV1::Empty => {
                    return Err(invalid("empty plan has no old-epoch signature allowlist"));
                }
            };
            if request.purpose == Purpose::FrozenReceipt {
                let body: FormalFrozenReceiptBodyV1 = decode_request(request)?;
                let expected = if role == EndpointRole::Primary {
                    &frozen.primary.body
                } else {
                    &frozen.replica.body
                };
                return if &body == expected {
                    Ok(())
                } else {
                    Err(invalid("draining replaced original Frozen receipt"))
                };
            }
            let candidate = progress
                .candidate
                .as_ref()
                .ok_or_else(|| invalid("draining candidate not yet durably fixed"))?;
            let expected = FormalFinalReceiptBodyV1 {
                schema_version: 1,
                record_id: candidate.record.record_id()?,
                position: candidate.record.position,
                finalizing_authority_id: authority.authority_id()?,
                role,
                instance_digest: previous.instance_digest.clone(),
                branch: FormalCompletionBranchV1::Normal,
                recovery_certificate_digest: None,
            };
            match request.purpose {
                Purpose::PrepareReceipt => {
                    let body: FormalPrepareReceiptBodyV1 = decode_request(request)?;
                    if body.final_receipt != expected
                        || progress.candidate_material_digest.as_ref()
                            != Some(&body.candidate_material_digest)
                    {
                        return Err(invalid(
                            "draining prepare differs from original candidate/material",
                        ));
                    }
                }
                Purpose::FinalReceipt => {
                    if decode_request::<FormalFinalReceiptBodyV1>(request)? != expected {
                        return Err(invalid(
                            "draining final receipt differs from original candidate",
                        ));
                    }
                }
                Purpose::ProofComplete => {
                    let decision = progress
                        .decision
                        .as_ref()
                        .ok_or_else(|| invalid("draining full decision not yet durable"))?;
                    let body: FormalProofCompleteBodyV1 = decode_request(request)?;
                    let wanted = FormalProofCompleteBodyV1 {
                        schema_version: 1,
                        record_id: expected.record_id,
                        position: expected.position,
                        decision_proof_digest: formal_decision_proof_digest(decision)?,
                        finalizing_authority_id: expected.finalizing_authority_id,
                        role,
                        instance_digest: previous.instance_digest.clone(),
                        branch: FormalCompletionBranchV1::Normal,
                        recovery_certificate_digest: None,
                    };
                    if body != wanted {
                        return Err(invalid(
                            "draining completion differs from original dual decision",
                        ));
                    }
                }
                _ => return Err(invalid("unrecognized recovery mutation signing purpose")),
            }
            Ok(())
        }
    }
}

fn decode_request<T: serde::de::DeserializeOwned + Serialize>(
    request: &fencing::ProtectedEndpointSigningRequestV1,
) -> Result<T, FormalError> {
    let prefix = 8 + request.purpose.domain().len();
    let value: T = serde_cbor::from_slice(
        request
            .preimage
            .get(prefix..)
            .ok_or_else(|| invalid("planned signing frame"))?,
    )
    .map_err(|error| invalid(error.to_string()))?;
    if bytes(request.purpose.domain(), &value)? != request.preimage {
        return Err(invalid("planned signing canonical original frame"));
    }
    Ok(value)
}
