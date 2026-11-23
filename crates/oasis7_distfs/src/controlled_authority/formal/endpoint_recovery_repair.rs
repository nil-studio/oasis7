//! Repair one lost endpoint directory from surviving immutable material. The
//! original provider supplies the only authority over the restored P/R roots.
use super::*;
use std::collections::BTreeMap;
#[path = "endpoint_recovery_repair_pending.rs"]
mod pending_validation;

pub struct ProtectedRecoveryRepairInputV1 {
    pub authority: VerifiedAuthorityV1,
    pub plan: SignedRecoveryPlanV1,
    pub fixed: RecoveryTailWitnessV1,
    pub pending: RecoveryPendingHandoffV1,
    pub writer_ack: SignedWriterRecoveryPlanAckV1,
    /// This signed survivor image supplies only immutable CAS object locations.
    /// It never substitutes for the lost endpoint's protected native image.
    pub survivor_checkpoint_bytes: Vec<u8>,
    pub source: Arc<dyn FormalArchiveSourceV1>,
    pub minimum_revision: u64,
    pub copy_max_bytes: u64,
}

struct RepairSource {
    local: FileFormalArchiveSourceV1,
    survivor: Arc<dyn FormalArchiveSourceV1>,
}
impl FormalArchiveSourceV1 for RepairSource {
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        match self.local.read_object(id)? {
            Some(bytes) => Ok(Some(bytes)),
            None => self.survivor.read_object(id),
        }
    }
}

impl FileFormalEndpoint {
    pub fn repair_protected_original(
        path: &Path,
        role: EndpointRole,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        input: ProtectedRecoveryRepairInputV1,
    ) -> Result<Self, FormalError> {
        let authority = input.authority;
        input.plan.verify(&authority)?;
        input.writer_ack.verify(&input.plan, &authority)?;
        if input.copy_max_bytes == 0
            || input.pending.root()? != input.plan.body.pending_handoff_root
            || input.survivor_checkpoint_bytes.len() > 64 * 1024
        {
            return Err(invalid(
                "repair requires complete original pending and an explicit copy budget",
            ));
        }
        let (instance, survivor_instance) = match role {
            EndpointRole::Primary => (
                &authority.definition().primary.instance,
                &authority.definition().replica.instance,
            ),
            EndpointRole::Replica => (
                &authority.definition().replica.instance,
                &authority.definition().primary.instance,
            ),
        };
        let survivor = Store::verify_public(
            &input.survivor_checkpoint_bytes,
            &survivor_instance.endpoint_public_key,
        )?;
        if survivor.instance_digest != survivor_instance.digest().map_err(fencing_error)?
            || survivor.authority_id != authority.authority_id()?
            || survivor.activation_ref != authority.activation_ref()
            || survivor.epoch != authority.definition().epoch
            || survivor.head != input.plan.body.intent.sealed_cut
            || survivor.seal.is_some()
        {
            return Err(invalid(
                "repair survivor image differs from original authority and cut",
            ));
        }
        let request = fencing::ProtectedRecoveryRepairRequestV1 {
            schema_version: 1,
            plan: input.plan.clone(),
            fixed: input.fixed.clone(),
            directory_fingerprint: fencing::directory_fingerprint(path).map_err(fencing_error)?,
            minimum_revision: input.minimum_revision,
        };
        let provider: Arc<dyn fencing::MonotonicEndpointProvider> = Arc::new(
            fencing::RecoveryRepairEndpointProviderV1::new(
                provider,
                path.to_path_buf(),
                &authority,
                request.clone(),
            )
            .map_err(fencing_error)?,
        );
        let mut guard = provider
            .repair_protected(instance, request.clone())
            .map_err(fencing_error)?;
        let grant = guard
            .recovery_repair_grant()
            .ok_or(FormalError::FencingUnproven)?
            .clone();
        grant
            .verify_request(instance, &request)
            .map_err(fencing_error)?;
        let policy = fencing::TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        };
        if guard.pending().is_none() {
            fencing::verify_monotonic_checkpoint_evidence(
                &guard.evidence().map_err(fencing_error)?,
                instance,
                guard.checkpoint(),
                &policy,
            )
            .map_err(fencing_error)?;
        }
        let current_bytes = guard
            .protected_native_checkpoint()
            .ok_or(FormalError::FencingUnproven)?
            .to_vec();
        let current = Store::verify_public(&current_bytes, &instance.endpoint_public_key)?;
        let original = Store::verify_public(
            &grant.original_native_checkpoint,
            &instance.endpoint_public_key,
        )?;
        if Store::binding(&original, &grant.original_native_checkpoint) != grant.original_checkpoint
            || original.pending != grant.original_pending
            || current.authority_id != original.authority_id
            || current.instance_digest != original.instance_digest
            || current.epoch != original.epoch
            || current.activation_ref != original.activation_ref
            || current
                .recovery_plan
                .as_ref()
                .is_some_and(|plan| plan != &input.plan)
            || Store::binding(&current, &current_bytes) != *guard.checkpoint()
        {
            return Err(invalid("repair original protected checkpoint changed"));
        }
        let public_key = instance.endpoint_public_key.clone();
        // The native file is still the provider's original signed checkpoint.
        // Holding this Store retains the ordinary directory lease throughout
        // repair, complete graph copying, independent readback and plan CAS.
        let mut store = Store::open_signed(
            path,
            current.clone(),
            &public_key,
            current_bytes,
            vec![input.source.clone()],
        )?;
        if let Some(original_pending) = original.pending.as_ref() {
            let pending = match store.raw_object(original_pending)? {
                Some(bytes) => {
                    if blake3::hash(&bytes).to_hex().as_str() != original_pending {
                        return Err(invalid("repair original pending hash"));
                    }
                    serde_cbor::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?
                }
                None => restore_original_pending(
                    &authority,
                    instance,
                    &request,
                    &grant,
                    &survivor,
                    input.source.as_ref(),
                    guard.as_mut(),
                    &policy,
                )?,
            };
            let id = store.put(&pending)?;
            if &id != original_pending {
                return Err(invalid(
                    "repair original pending bytes differ from protected root",
                ));
            }
            let original_index = original
                .index
                .as_ref()
                .ok_or_else(|| invalid("repair original pending index missing"))?;
            match store.raw_object(original_index)? {
                Some(bytes) => {
                    // A completed repair retains P's own index. Its changed
                    // receipt path need not retain R's different index root,
                    // so a cold reopen must not reconstruct it from R again.
                    // The full graph and every child are verified below.
                    if blake3::hash(&bytes).to_hex().as_str() != original_index {
                        return Err(invalid("repair original permanent index hash"));
                    }
                }
                None => {
                    let root =
                        store.insert(survivor.index.as_deref(), pending.request_digest(), &id)?;
                    if original_index != &root {
                        return Err(invalid(
                            "repair cannot recreate the original permanent request index",
                        ));
                    }
                }
            }
        } else if !matches!(input.fixed, RecoveryTailWitnessV1::Empty) {
            return Err(invalid("repair original fixed continuation disappeared"));
        }
        let source = RepairSource {
            local: FileFormalArchiveSourceV1::new(path.to_path_buf())?,
            survivor: input.source,
        };
        let copied = store::durable_set::copy_recovery_repair_closure(
            path,
            &grant.original_native_checkpoint,
            &public_key,
            &source,
            &grant,
            input.copy_max_bytes,
            true,
        )?;
        let local = FileFormalArchiveSourceV1::new(path.to_path_buf())?;
        let checked = store::durable_set::copy_recovery_repair_closure(
            path,
            &grant.original_native_checkpoint,
            &public_key,
            &local,
            &grant,
            input.copy_max_bytes,
            false,
        )?;
        if copied != checked {
            return Err(invalid(
                "repair independent registered-local inventory differs",
            ));
        }
        store.finish_recovery_repair_sources();
        store.reconcile_public(guard.as_mut(), &public_key)?;
        let evidence = guard.evidence().map_err(fencing_error)?;
        fencing::verify_monotonic_checkpoint_evidence(
            &evidence,
            instance,
            &store.checkpoint,
            &policy,
        )
        .map_err(fencing_error)?;
        drop(guard);
        let mut endpoint = Self {
            store,
            original: authority.clone(),
            authority,
            recovery: None,
            role,
            key: None,
            public_key,
            provider,
            parent: None,
            poisoned: false,
            physical_maintenance: storage_maintenance::MaintenanceState::default(),
            native_environment_jobs: BTreeMap::new(),
            published_closure_jobs: BTreeMap::new(),
            iws_page_store: None,
            #[cfg(test)]
            controlled: false,
        };
        // Restore through the same private native-provenance verifier used by
        // ordinary protected opening, never through a caller supplied head.
        endpoint.reload_parent(&evidence)?;
        endpoint.verify_business_checkpoint()?;
        endpoint.verify_pending_for_recovery_repair()?;
        let expected = endpoint.store.checkpoint.clone();
        endpoint.prepare_recovery_plan(input.plan, input.pending, input.writer_ack, &expected)?;
        Ok(endpoint)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Original role reconstruction keeps the signed plan, provider grant, independent survivor source and leased signer separate."
)]
fn restore_original_pending(
    authority: &VerifiedAuthorityV1,
    instance: &fencing::EndpointInstanceBindingV1,
    request: &fencing::ProtectedRecoveryRepairRequestV1,
    grant: &fencing::ProtectedRecoveryRepairGrantV1,
    survivor: &HotBody,
    source: &dyn FormalArchiveSourceV1,
    guard: &mut dyn fencing::MonotonicEndpointGuard,
    policy: &fencing::TrustedFencingPolicyV1,
) -> Result<FormalRequestStateV1, FormalError> {
    let id = survivor
        .pending
        .as_ref()
        .ok_or_else(|| invalid("survivor original pending missing"))?;
    let bytes = source
        .read_object(id)?
        .ok_or_else(|| invalid("survivor pending object missing"))?;
    if bytes.len() > 64 * 1024 * 1024 || blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("survivor original pending hash/bound"));
    }
    let original_id = grant
        .original_pending
        .as_ref()
        .ok_or_else(|| invalid("original provider pending missing"))?;
    let mut pending: FormalRequestStateV1 =
        serde_cbor::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?;
    match (&mut pending, &request.fixed) {
        (
            FormalRequestStateV1::Frozen {
                input,
                material,
                receipt,
                certificate,
            },
            RecoveryTailWitnessV1::Frozen(fixed),
        ) => {
            if input != &fixed.reservation || certificate.as_ref() != Some(fixed) {
                return Err(invalid(
                    "survivor changed original complete Frozen certificate",
                ));
            }
            verify_frozen_material(&input.body, material)?;
            *receipt = Some(if instance.endpoint_role == EndpointRole::Primary {
                fixed.primary.clone()
            } else {
                fixed.replica.clone()
            });
        }
        (
            FormalRequestStateV1::Prepared { candidate, receipt },
            RecoveryTailWitnessV1::Prepared {
                frozen,
                candidate: fixed,
            },
        ) => {
            if &candidate.candidate != fixed
                || candidate.frozen.as_ref() != Some(frozen)
                || candidate.branch != FormalCompletionBranchV1::Normal
            {
                return Err(invalid("survivor changed original Prepared candidate"));
            }
            verify_record_material(&candidate.candidate.record, &candidate.material)?;
            verify_frozen_material(
                &frozen.reservation.body,
                candidate
                    .frozen_material
                    .as_ref()
                    .ok_or_else(|| invalid("repair Prepared Frozen material missing"))?,
            )?;
            let body = FormalPrepareReceiptBodyV1 {
                final_receipt: FormalFinalReceiptBodyV1 {
                    schema_version: 1,
                    record_id: fixed.record.record_id()?,
                    position: fixed.record.position,
                    finalizing_authority_id: authority.authority_id()?,
                    role: instance.endpoint_role,
                    instance_digest: instance.digest().map_err(fencing_error)?,
                    branch: FormalCompletionBranchV1::Normal,
                    recovery_certificate_digest: None,
                },
                candidate_material_digest: formal_candidate_material_digest(candidate)?,
            };
            *receipt = Some(FormalPrepareReceiptV1 {
                body,
                signature_hex: "a".repeat(128),
            });
        }
        _ => {
            return Err(invalid(
                "repair survivor stage differs from original signed plan",
            ));
        }
    }
    let encoded = |value: &FormalRequestStateV1| {
        serde_cbor::to_vec(value).map_err(|error| invalid(error.to_string()))
    };
    if blake3::hash(&encoded(&pending)?).to_hex().as_str() == original_id {
        return Ok(pending);
    }
    // The original provider may have crashed after candidate persistence but
    // before persisting its role receipt. Preserve that exact None stage too.
    let mut without_receipt = pending.clone();
    match &mut without_receipt {
        FormalRequestStateV1::Frozen { receipt, .. } => *receipt = None,
        FormalRequestStateV1::Prepared { receipt, .. } => *receipt = None,
        _ => unreachable!(),
    }
    if blake3::hash(&encoded(&without_receipt)?).to_hex().as_str() == original_id {
        return Ok(without_receipt);
    }
    let FormalRequestStateV1::Prepared {
        receipt: Some(receipt),
        ..
    } = &pending
    else {
        return Err(invalid(
            "original Frozen role receipt cannot reconstruct protected pending",
        ));
    };
    let signing = fencing::ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().map_err(fencing_error)?,
        checkpoint: guard.checkpoint().clone(),
        purpose: fencing::ProtectedEndpointSignaturePurposeV1::PrepareReceipt,
        preimage: formal_prepare_receipt_signing_bytes(&receipt.body)?,
        challenge: fencing::fresh_challenge().map_err(fencing_error)?,
    };
    let first = encoded(&pending)?;
    let mut alternate = pending.clone();
    let FormalRequestStateV1::Prepared {
        receipt: Some(receipt),
        ..
    } = &mut alternate
    else {
        unreachable!()
    };
    receipt.signature_hex = "b".repeat(128);
    let second = encoded(&alternate)?;
    let prefix = first
        .iter()
        .zip(&second)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = first
        .iter()
        .rev()
        .zip(second.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if first.len() != second.len()
        || prefix.checked_add(128).and_then(|n| n.checked_add(suffix)) != Some(first.len())
    {
        return Err(invalid(
            "original receipt CBOR signature slot is not unique",
        ));
    }
    let replay = fencing::ProtectedRecoveryReceiptReplayV1 {
        signing: signing.clone(),
        entry_prefix: first[..prefix].to_vec(),
        entry_suffix: first[first.len() - suffix..].to_vec(),
    };
    let signed = guard
        .replay_recovery_receipt(replay.clone())
        .map_err(fencing_error)?;
    let signature =
        fencing::verify_protected_endpoint_signature(&signed, &signing, instance, policy)
            .map_err(fencing_error)?;
    fencing::verify_protected_recovery_receipt_replay(
        authority, instance, request, grant, &replay, &signature,
    )
    .map_err(fencing_error)?;
    let FormalRequestStateV1::Prepared {
        receipt: Some(receipt),
        ..
    } = &mut pending
    else {
        unreachable!()
    };
    receipt.signature_hex = signature;
    if blake3::hash(&encoded(&pending)?).to_hex().as_str() != original_id {
        return Err(invalid(
            "repaired Prepared bytes differ from original native root",
        ));
    }
    Ok(pending)
}
