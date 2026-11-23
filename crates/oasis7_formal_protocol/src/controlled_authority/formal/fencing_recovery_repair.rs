//! A lost data directory may be repaired only under the original protected
//! ledger and one issuer plan. This never changes the logical endpoint binding.
use super::*;
use crate::controlled_authority::formal::{
    FormalPrepareReceiptBodyV1, RecoveryTailWitnessV1, SignedRecoveryPlanV1, VerifiedAuthorityV1,
    formal_prepare_receipt_signing_bytes, frozen_receipt_signing_bytes, proof::NativeHotBodyV1,
    recovery_requests_commitment, recovery_tail_from_witness,
    validate_recovery_checkpoint_transition, validate_recovery_protected_signing_request,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedRecoveryRepairRequestV1 {
    pub schema_version: u32,
    pub plan: SignedRecoveryPlanV1,
    pub fixed: RecoveryTailWitnessV1,
    pub directory_fingerprint: String,
    pub minimum_revision: u64,
}

impl ProtectedRecoveryRepairRequestV1 {
    pub fn validate(&self, authority: &VerifiedAuthorityV1) -> Result<(), FencingError> {
        self.plan.verify(authority).map_err(formal_error)?;
        canonical_hash(&self.directory_fingerprint)?;
        if self.schema_version != 1
            || self.plan.body.intent.retained_context.is_some()
            || recovery_requests_commitment(&self.fixed).map_err(formal_error)?
                != self.plan.body.original_requests_commitment
            || recovery_tail_from_witness(&self.fixed).map_err(formal_error)?
                != self.plan.body.intent.tail
        {
            return Err(invalid(
                "repair requires the exact independently signed original plan",
            ));
        }
        let frozen = match &self.fixed {
            RecoveryTailWitnessV1::Empty => return Ok(()),
            RecoveryTailWitnessV1::Frozen(frozen)
            | RecoveryTailWitnessV1::Prepared { frozen, .. }
            | RecoveryTailWitnessV1::Decision { frozen, .. } => frozen,
        };
        frozen
            .verify(authority.definition(), authority.activation_ref())
            .map_err(formal_error)?;
        if frozen.reservation.body.parent != self.plan.body.intent.sealed_cut {
            return Err(invalid("repair original Frozen parent differs from plan"));
        }
        if let RecoveryTailWitnessV1::Prepared { candidate, .. }
        | RecoveryTailWitnessV1::Decision { candidate, .. } = &self.fixed
        {
            candidate.verify_original(authority).map_err(formal_error)?;
            frozen
                .matches_record(&candidate.record)
                .map_err(formal_error)?;
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String, FencingError> {
        Ok(hash_encoded(&bytes(
            "oasis7.formal.recovery_repair.request.v1",
            self,
        )?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedRecoveryRepairGrantV1 {
    pub schema_version: u32,
    pub request_digest: String,
    pub instance_digest: String,
    pub plan_digest: String,
    pub directory_fingerprint: String,
    pub recovery_credential_public_key: String,
    pub original_checkpoint: EndpointCheckpointBindingV1,
    pub original_native_checkpoint: Vec<u8>,
    pub original_pending: Option<String>,
}

impl ProtectedRecoveryRepairGrantV1 {
    pub fn verify_request(
        &self,
        instance: &EndpointInstanceBindingV1,
        request: &ProtectedRecoveryRepairRequestV1,
    ) -> Result<(), FencingError> {
        self.original_checkpoint.validate()?;
        if self.original_native_checkpoint.is_empty()
            || self.original_native_checkpoint.len() > 64 * 1024
        {
            return Err(invalid("repair original native checkpoint bound"));
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SignedNative {
            body: NativeHotBodyV1,
            signature: String,
        }
        let native: SignedNative = serde_cbor::from_slice(&self.original_native_checkpoint)
            .map_err(|error| invalid(error.to_string()))?;
        verify(
            "oasis7.formal.endpoint_checkpoint.sign.v1",
            &native.body,
            &native.signature,
            &instance.endpoint_public_key,
        )?;
        let head = native
            .body
            .decision_head
            .as_ref()
            .unwrap_or(&native.body.head);
        if hash_encoded(&self.original_native_checkpoint) != self.original_checkpoint.commitment
            || native.body.version != 1
            || native.body.authority_id != request.plan.body.intent.predecessor_authority_id
            || native.body.activation_ref != request.plan.body.predecessor_activation_ref
            || native.body.epoch != instance.authority_epoch
            || native.body.seal.is_some()
            || native.body.revision != self.original_checkpoint.revision
            || native.body.pending != self.original_pending
            || native.body.instance_digest != self.instance_digest
            || native.body.head != request.plan.body.intent.sealed_cut
            || head.position != self.original_checkpoint.head.position
            || head.record_id != self.original_checkpoint.head.decision_hash
            || self.original_checkpoint.head.qualified != native.body.decision_head.is_none()
        {
            return Err(invalid(
                "repair grant original signed native bytes differ from its floor",
            ));
        }
        if let Some(pending) = &self.original_pending {
            canonical_hash(pending)?;
        }
        if self.schema_version != 1
            || self.request_digest != request.digest()?
            || self.instance_digest != instance.digest()?
            || self.plan_digest != request.plan.digest().map_err(formal_error)?
            || self.directory_fingerprint != request.directory_fingerprint
            || self.directory_fingerprint == instance.storage_fingerprint
            || self.recovery_credential_public_key
                != request
                    .plan
                    .body
                    .authorization
                    .recovery_credential_public_key
            || self.original_checkpoint.revision < request.minimum_revision
        {
            return Err(invalid(
                "protected repair grant changed original scope or floor",
            ));
        }
        Ok(())
    }
}

/// Called by the protected provider while holding its original exclusive lease.
/// `native` and `checkpoint` must come from that provider's existing ledger,
/// never from the surviving endpoint or from a replacement registration.
pub fn authorize_protected_recovery_repair(
    authority: &VerifiedAuthorityV1,
    instance: &EndpointInstanceBindingV1,
    native: &NativeHotBodyV1,
    native_checkpoint_bytes: &[u8],
    checkpoint: &EndpointCheckpointBindingV1,
    request: &ProtectedRecoveryRepairRequestV1,
) -> Result<ProtectedRecoveryRepairGrantV1, FencingError> {
    request.validate(authority)?;
    let binding = match instance.endpoint_role {
        EndpointRole::Primary => &authority.definition().primary.instance,
        EndpointRole::Replica => &authority.definition().replica.instance,
    };
    if binding != instance
        || native.version != 1
        || native.instance_digest != instance.digest()?
        || native.authority_id != authority.authority_id().map_err(formal_error)?
        || native.activation_ref != authority.activation_ref()
        || native.epoch != instance.authority_epoch
        || native.revision != checkpoint.revision
        || native.head != request.plan.body.intent.sealed_cut
        || native.seal.is_some()
        || native
            .recovery_plan
            .as_ref()
            .is_some_and(|plan| plan != &request.plan)
        || native.pending.is_none() != matches!(request.fixed, RecoveryTailWitnessV1::Empty)
    {
        return Err(invalid(
            "repair must retain the original provider checkpoint and fixed tail",
        ));
    }
    let head = native.decision_head.as_ref().unwrap_or(&native.head);
    if checkpoint.head.position != head.position
        || checkpoint.head.decision_hash != head.record_id
        || checkpoint.head.qualified != native.decision_head.is_none()
    {
        return Err(invalid("repair native and protected checkpoint differ"));
    }
    let grant = ProtectedRecoveryRepairGrantV1 {
        schema_version: 1,
        request_digest: request.digest()?,
        instance_digest: instance.digest()?,
        plan_digest: request.plan.digest().map_err(formal_error)?,
        directory_fingerprint: request.directory_fingerprint.clone(),
        recovery_credential_public_key: request
            .plan
            .body
            .authorization
            .recovery_credential_public_key
            .clone(),
        original_checkpoint: checkpoint.clone(),
        original_native_checkpoint: native_checkpoint_bytes.to_vec(),
        original_pending: native.pending.clone(),
    };
    grant.verify_request(instance, request)?;
    Ok(grant)
}

/// An already persisted role receipt may be reproduced after its directory is
/// lost. The provider signs internally, then releases the signature ONLY if
/// replacing this exact CBOR string slot recreates its original pending hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedRecoveryReceiptReplayV1 {
    pub signing: ProtectedEndpointSigningRequestV1,
    pub entry_prefix: Vec<u8>,
    pub entry_suffix: Vec<u8>,
}

/// The signed receipt slot is supplied by the original provider. Only the two
/// surrounding byte ranges travel in bounded chunks on the same held lease.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedRecoveryReceiptReplayHeaderV1 {
    pub signing: ProtectedEndpointSigningRequestV1,
    pub prefix_bytes: u64,
    pub suffix_bytes: u64,
}

/// Holds the internally computed signature until the complete original entry
/// digest has been checked. Neither an incomplete stream nor a failed stream
/// can release it; the provider must discard this value when its lease ends.
pub struct ProtectedRecoveryReceiptReplayVerifierV1 {
    signing: ProtectedEndpointSigningRequestV1,
    endpoint_signature: String,
    wanted: String,
    prefix_bytes: u64,
    total_bytes: u64,
    offset: u64,
    inserted_signature: bool,
    failed: bool,
    digest: blake3::Hasher,
}

impl ProtectedRecoveryReceiptReplayVerifierV1 {
    pub fn new(
        authority: &VerifiedAuthorityV1,
        instance: &EndpointInstanceBindingV1,
        repair: &ProtectedRecoveryRepairRequestV1,
        grant: &ProtectedRecoveryRepairGrantV1,
        header: ProtectedRecoveryReceiptReplayHeaderV1,
        endpoint_signature: String,
    ) -> Result<Self, FencingError> {
        let total_bytes = header
            .prefix_bytes
            .checked_add(header.suffix_bytes)
            .filter(|total| *total <= 64 * 1024 * 1024 - 128)
            .ok_or_else(|| invalid("original receipt replay exceeds its byte bound"))?;
        validate_receipt_replay_scope(
            authority,
            instance,
            repair,
            grant,
            &header.signing,
            &endpoint_signature,
        )?;
        Ok(Self {
            signing: header.signing,
            endpoint_signature,
            wanted: grant
                .original_pending
                .clone()
                .ok_or_else(|| invalid("empty repair has no original receipt"))?,
            prefix_bytes: header.prefix_bytes,
            total_bytes,
            offset: 0,
            inserted_signature: false,
            failed: false,
            digest: blake3::Hasher::new(),
        })
    }

    pub fn push(&mut self, offset: u64, chunk: &[u8]) -> Result<(), FencingError> {
        let next = self.offset.checked_add(chunk.len() as u64);
        if self.failed
            || offset != self.offset
            || chunk.is_empty()
            || chunk.len() > 64 * 1024
            || next.is_none_or(|next| next > self.total_bytes)
        {
            self.failed = true;
            return Err(invalid(
                "original receipt replay chunk is out of bounds or order",
            ));
        }
        let before_signature = self
            .prefix_bytes
            .saturating_sub(self.offset)
            .min(chunk.len() as u64) as usize;
        self.digest.update(&chunk[..before_signature]);
        if !self.inserted_signature && self.offset + before_signature as u64 == self.prefix_bytes {
            self.digest.update(self.endpoint_signature.as_bytes());
            self.inserted_signature = true;
        }
        self.digest.update(&chunk[before_signature..]);
        self.offset += chunk.len() as u64;
        Ok(())
    }

    pub fn finish(mut self) -> Result<(ProtectedEndpointSigningRequestV1, String), FencingError> {
        if self.failed || self.offset != self.total_bytes {
            return Err(invalid("original receipt replay is incomplete or failed"));
        }
        if !self.inserted_signature {
            self.digest.update(self.endpoint_signature.as_bytes());
        }
        if self.digest.finalize().to_hex().as_str() != self.wanted {
            return Err(invalid(
                "replayed role receipt does not recreate original native pending bytes",
            ));
        }
        Ok((self.signing, self.endpoint_signature))
    }
}

pub fn verify_protected_recovery_receipt_replay(
    authority: &VerifiedAuthorityV1,
    instance: &EndpointInstanceBindingV1,
    repair: &ProtectedRecoveryRepairRequestV1,
    grant: &ProtectedRecoveryRepairGrantV1,
    replay: &ProtectedRecoveryReceiptReplayV1,
    endpoint_signature: &str,
) -> Result<(), FencingError> {
    let mut verifier = ProtectedRecoveryReceiptReplayVerifierV1::new(
        authority,
        instance,
        repair,
        grant,
        ProtectedRecoveryReceiptReplayHeaderV1 {
            signing: replay.signing.clone(),
            prefix_bytes: replay.entry_prefix.len() as u64,
            suffix_bytes: replay.entry_suffix.len() as u64,
        },
        endpoint_signature.to_owned(),
    )?;
    let mut offset = 0;
    for chunk in replay
        .entry_prefix
        .chunks(64 * 1024)
        .chain(replay.entry_suffix.chunks(64 * 1024))
    {
        verifier.push(offset, chunk)?;
        offset += chunk.len() as u64;
    }
    verifier.finish().map(|_| ())
}

fn validate_receipt_replay_scope(
    authority: &VerifiedAuthorityV1,
    instance: &EndpointInstanceBindingV1,
    repair: &ProtectedRecoveryRepairRequestV1,
    grant: &ProtectedRecoveryRepairGrantV1,
    signing: &ProtectedEndpointSigningRequestV1,
    endpoint_signature: &str,
) -> Result<(), FencingError> {
    repair.validate(authority)?;
    grant.verify_request(instance, repair)?;
    signing.validate()?;
    if endpoint_signature.len() != 128
        || !endpoint_signature
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || signing.instance_digest != grant.instance_digest
    {
        return Err(invalid("original receipt replay frame or instance"));
    }
    grant
        .original_pending
        .as_ref()
        .ok_or_else(|| invalid("empty repair has no original receipt"))?;
    verify_with_preimage(
        endpoint_signature,
        &instance.endpoint_public_key,
        || Ok::<_, FencingError>(&signing.preimage),
        |_| Ok(()),
    )?;
    let frozen = match &repair.fixed {
        RecoveryTailWitnessV1::Frozen(frozen) | RecoveryTailWitnessV1::Prepared { frozen, .. } => {
            frozen
        }
        _ => {
            return Err(invalid(
                "receipt repair requires an original Frozen or Prepared slot",
            ));
        }
    };
    match signing.purpose {
        ProtectedEndpointSignaturePurposeV1::FrozenReceipt => {
            let receipt = if instance.endpoint_role == EndpointRole::Primary {
                &frozen.primary
            } else {
                &frozen.replica
            };
            if signing.preimage
                != frozen_receipt_signing_bytes(&receipt.body).map_err(formal_error)?
                || endpoint_signature != receipt.signature_hex
            {
                return Err(invalid(
                    "repair cannot replace the original role Frozen receipt",
                ));
            }
        }
        ProtectedEndpointSignaturePurposeV1::PrepareReceipt => {
            let RecoveryTailWitnessV1::Prepared { candidate, .. } = &repair.fixed else {
                return Err(invalid("repair cannot create a new Prepared candidate"));
            };
            let body: FormalPrepareReceiptBodyV1 = decode_signing(signing)?;
            canonical_hash(&body.candidate_material_digest)?;
            let final_body = &body.final_receipt;
            if final_body.schema_version != 1
                || final_body.record_id != candidate.record.record_id().map_err(formal_error)?
                || final_body.position != candidate.record.position
                || final_body.finalizing_authority_id
                    != authority.authority_id().map_err(formal_error)?
                || final_body.role != instance.endpoint_role
                || final_body.instance_digest != grant.instance_digest
                || final_body.branch
                    != crate::controlled_authority::formal::FormalCompletionBranchV1::Normal
                || final_body.recovery_certificate_digest.is_some()
                || signing.preimage
                    != formal_prepare_receipt_signing_bytes(&body).map_err(formal_error)?
            {
                return Err(invalid(
                    "repair prepare signature differs from the fixed original",
                ));
            }
        }
        _ => return Err(invalid("repair receipt signing purpose is not allowlisted")),
    }
    Ok(())
}

/// The repair grant fences ordinary mutations even before the native plan CAS.
pub fn validate_protected_recovery_repair_signing(
    authority: &VerifiedAuthorityV1,
    native: &NativeHotBodyV1,
    repair: &ProtectedRecoveryRepairRequestV1,
    signing: &ProtectedEndpointSigningRequestV1,
) -> Result<(), FencingError> {
    repair.validate(authority)?;
    signing.validate()?;
    if let Some(plan) = &native.recovery_plan {
        if plan != &repair.plan {
            return Err(invalid("repair provider native plan changed"));
        }
        return validate_recovery_protected_signing_request(native, authority, signing)
            .map_err(formal_error);
    }
    match signing.purpose {
        ProtectedEndpointSignaturePurposeV1::RpcResponse => Ok(()),
        ProtectedEndpointSignaturePurposeV1::NativeCheckpoint => {
            let next: NativeHotBodyV1 = decode_signing(signing)?;
            validate_protected_recovery_repair_transition(authority, native, &next, repair)
        }
        _ => Err(invalid(
            "repair permits only exact plan installation before draining",
        )),
    }
}

pub fn validate_protected_recovery_repair_transition(
    authority: &VerifiedAuthorityV1,
    previous: &NativeHotBodyV1,
    next: &NativeHotBodyV1,
    repair: &ProtectedRecoveryRepairRequestV1,
) -> Result<(), FencingError> {
    if next.recovery_plan.as_ref() != Some(&repair.plan) {
        return Err(invalid(
            "repair cannot publish a different or absent native plan",
        ));
    }
    validate_recovery_checkpoint_transition(previous, next, authority).map_err(formal_error)
}

fn decode_signing<T: serde::de::DeserializeOwned + Serialize>(
    signing: &ProtectedEndpointSigningRequestV1,
) -> Result<T, FencingError> {
    let prefix = 8 + signing.purpose.domain().len();
    let value: T = serde_cbor::from_slice(
        signing
            .preimage
            .get(prefix..)
            .ok_or_else(|| invalid("repair signing frame"))?,
    )
    .map_err(|error| invalid(error.to_string()))?;
    if bytes(signing.purpose.domain(), &value)? != signing.preimage {
        return Err(invalid("repair signing frame is not canonical"));
    }
    Ok(value)
}

fn formal_error(error: impl std::fmt::Display) -> FencingError {
    invalid(error.to_string())
}
