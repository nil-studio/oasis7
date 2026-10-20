//! Test-only issuer/witness simulation. This never establishes real custody,
//! nonrollback storage, or physical fault independence. Seals are supplied by
//! the actual test endpoint processes rather than invented by this helper.
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::controlled_authority::{fencing::*, formal::*};

#[expect(
    clippy::too_many_arguments,
    reason = "the fixture mirrors the complete signed recovery authorization inputs"
)]
pub(super) fn permission_for_sealed_endpoints(
    external: &VerifiedFormalReadAnchorV1,
    trust: &InitialActivationTrustV1,
    successor: AuthorityDefinitionV1,
    tail: RecoveryTailWitnessV1,
    seals: [EpochSealReceiptV1; 2],
    prior: Option<&VerifiedRecoveryBootstrapV1>,
    issuer_key: &SigningKey,
    witness_key: &SigningKey,
    independent_floor: issuer::IssuerAnchorV1,
) -> Result<(RecoveryPermissionBootstrapV1, VerifiedRecoveryBootstrapV1), String> {
    permission_for_fencing(
        external,
        trust,
        successor,
        tail,
        FencingBootstrapV1::Dual {
            primary: Box::new(seals[0].clone()),
            replica: Box::new(seals[1].clone()),
        },
        prior,
        issuer_key,
        witness_key,
        independent_floor,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "the fixture mirrors the complete signed recovery authorization inputs"
)]
pub(super) fn permission_for_fencing(
    external: &VerifiedFormalReadAnchorV1,
    trust: &InitialActivationTrustV1,
    successor: AuthorityDefinitionV1,
    tail: RecoveryTailWitnessV1,
    fencing: FencingBootstrapV1,
    prior: Option<&VerifiedRecoveryBootstrapV1>,
    issuer_key: &SigningKey,
    witness_key: &SigningKey,
    independent_floor: issuer::IssuerAnchorV1,
) -> Result<(RecoveryPermissionBootstrapV1, VerifiedRecoveryBootstrapV1), String> {
    let predecessor = external.authority();
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: predecessor.policy().fencing_provider_public_key.clone(),
        authorized_resource_bindings: predecessor
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let (p, r) = match &fencing {
        FencingBootstrapV1::Dual { primary, replica } => (
            verify_epoch_seal(primary, &policy).map_err(|e| e.to_string())?,
            Some(verify_epoch_seal(replica, &policy).map_err(|e| e.to_string())?),
        ),
        FencingBootstrapV1::NecessaryEndpoint { seal } => (
            verify_epoch_seal(seal, &policy).map_err(|e| e.to_string())?,
            None,
        ),
    };
    let coverage = if r.is_none() {
        Some(
            VerifiedAppendFencingCoverageV1::derive_from_necessary_endpoint_seal(
                predecessor,
                external.head(),
                &p,
            )
            .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let context = if let Some(previous) = prior {
        let original = if previous.recovery().body().tail == RecoveryTailV1::Empty
            && !matches!(&tail, RecoveryTailWitnessV1::Empty)
        {
            previous.recovery().successor()
        } else {
            previous.original_authority()
        };
        Some(
            VerifiedActiveRecoveryContextV2::from_previous(
                previous.recovery(),
                original,
                external.head(),
                &tail,
            )
            .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let body = RecoveryCertificateBodyV1 {
        schema_version: if context.is_some() { 2 } else { 1 },
        world: predecessor.definition().world.clone(),
        operation_id: p.seal_body().recovery_operation.clone(),
        predecessor_authority_id: predecessor.authority_id().map_err(|e| e.to_string())?,
        predecessor_epoch: predecessor.definition().epoch,
        sealed_cut: external.head().head(),
        tail: certificate_tail(&tail)?,
        fencing_evidence_digest: match &r {
            Some(r) => recovery_fencing_digest([&p, r]).map_err(|e| e.to_string())?,
            None => coverage
                .as_ref()
                .unwrap()
                .evidence_digest()
                .map_err(|e| e.to_string())?,
        },
        successor,
        first_normal_position: Some(
            external
                .head()
                .record()
                .position
                .checked_add(if matches!(&tail, RecoveryTailWitnessV1::Empty) {
                    1
                } else {
                    2
                })
                .ok_or("position overflow")?,
        ),
        retained_context: context.as_ref().map(|c| c.commitment().clone()),
    };
    // Run the same intent verifier as production before either fixture signs.
    let intent = if let Some(context) = &context {
        VerifiedRecoveryIntent::verify_retained(
            body.clone(),
            context,
            [&p, r.as_ref().ok_or("retained single fixture unsupported")?],
        )
    } else if let Some(coverage) = &coverage {
        VerifiedRecoveryIntent::verify_single_survivor(
            body.clone(),
            predecessor,
            external.head(),
            coverage,
            &tail,
        )
    } else {
        VerifiedRecoveryIntent::verify(
            body.clone(),
            predecessor,
            external.head(),
            [&p, r.as_ref().unwrap()],
            &tail,
        )
    }
    .map_err(|e| e.to_string())?;
    let certificate = SignedRecoveryCertificateV1 {
        signature_hex: hex::encode(
            issuer_key
                .sign(&recovery_signing_bytes(&body).map_err(|e| e.to_string())?)
                .to_bytes(),
        ),
        body: intent.body().clone(),
    };
    let policy_binding = issuer::IssuerAnchorV1::initial(predecessor.policy())
        .map_err(|e| e.to_string())?
        .digest;
    let revision = independent_floor
        .revision
        .checked_add(1)
        .ok_or("issuer revision overflow")?;
    let current = issuer::IssuerAnchorV1 {
        revision,
        digest: blake3::hash(
            &serde_cbor::to_vec(&(&independent_floor, &certificate)).map_err(|e| e.to_string())?,
        )
        .to_hex()
        .to_string(),
    };
    let witness = issuer::SignedIssuanceWitnessV1::sign(
        issuer::IssuanceWitnessBodyV1 {
            schema_version: 1,
            capability: issuer::IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
            request: issuer::IssuanceRequestBodyV1 {
                schema_version: 1,
                issuer_public_key: predecessor.policy().recovery_issuer_public_key.clone(),
                policy_binding,
                predecessor_authority_id: predecessor.authority_id().map_err(|e| e.to_string())?,
                certificate_digest: recovery_certificate_digest(&certificate)
                    .map_err(|e| e.to_string())?,
                observation_nonce: blake3::hash(&revision.to_be_bytes()).to_hex().to_string(),
                minimum_anchor: independent_floor.clone(),
            },
            grant_revision: revision,
            current_anchor: current,
        },
        witness_key,
    )
    .map_err(|e| e.to_string())?;
    let checkpoint = external.checkpoint();
    let raw = RecoveryPermissionBootstrapV1 {
        predecessor_checkpoint_digest: checkpoint.digest().map_err(|e| e.to_string())?,
        predecessor_checkpoint: checkpoint,
        predecessor_context_version: prior.map(|_| 2),
        certificate,
        fencing,
        tail,
        issuer_witness: witness,
        minimum_issuer_anchor: independent_floor,
        prior_permission: prior.map(|p| Box::new(p.permission_evidence().clone())),
    };
    let verified = raw.verify(trust, &raw.predecessor_checkpoint.initial_certificate)?;
    Ok((raw, verified))
}
fn certificate_tail(tail: &RecoveryTailWitnessV1) -> Result<RecoveryTailV1, String> {
    let (frozen, candidate, decision) = match tail {
        RecoveryTailWitnessV1::Empty => return Ok(RecoveryTailV1::Empty),
        RecoveryTailWitnessV1::Frozen(frozen) => (frozen, None, false),
        RecoveryTailWitnessV1::Prepared { frozen, candidate } => (frozen, Some(candidate), false),
        RecoveryTailWitnessV1::Decision { frozen, candidate } => (frozen, Some(candidate), true),
    };
    let position = frozen.reservation.body.next_position;
    let reservation_id = frozen
        .reservation
        .body
        .reservation_id()
        .map_err(|e| e.to_string())?;
    let handoff = frozen.reservation.body.handoff.clone();
    match candidate {
        None => Ok(RecoveryTailV1::Frozen {
            position,
            reservation_id,
            handoff,
        }),
        Some(candidate) => {
            let record_id = candidate.record.record_id().map_err(|e| e.to_string())?;
            if decision {
                Ok(RecoveryTailV1::Decision {
                    position,
                    reservation_id,
                    record_id,
                    handoff,
                })
            } else {
                Ok(RecoveryTailV1::Prepared {
                    position,
                    reservation_id,
                    record_id,
                    handoff,
                })
            }
        }
    }
}
