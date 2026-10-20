//! Signed test-only recovery evidence; no external checkpoint replacement.
use super::*;
use crate::formal_live_recovery_test_support as recovery_test_support;
use oasis7::world_service::formal_read::{FormalReadFencingEvidenceV1, FormalRecoveryReadStepV1};
use oasis7_distfs::controlled_authority::fencing::*;
use oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor;
use serde::Serialize;

pub(super) fn empty_recovery_fixture(
    original: &FullSyncCutFixture,
    trust: &oasis7::world_service::client::FormalReadTrustConfigV1,
) -> Result<(FullSyncCutFixture, Vec<FormalReadStepV1>), String> {
    let cut = &original.anchor;
    let next = successor(cut.authority(), cut, 2, [101, 102, 103]);
    let seals = [
        epoch_seal_with_tail(
            cut.authority(),
            cut,
            2,
            5,
            EndpointRole::Primary,
            SealedTailV1::empty(),
        ),
        epoch_seal_with_tail(
            cut.authority(),
            cut,
            2,
            6,
            EndpointRole::Replica,
            SealedTailV1::empty(),
        ),
    ];
    let floor =
        issuer::IssuerAnchorV1::initial(cut.authority().policy()).map_err(|e| e.to_string())?;
    let (raw, _) = recovery_test_support::permission_for_sealed_endpoints(
        cut,
        &trust.initial_trust,
        next,
        RecoveryTailWitnessV1::Empty,
        seals.clone(),
        None,
        &fixtures::key(1),
        &fixtures::key(2),
        floor.clone(),
    )?;
    let verified =
        raw.verify_at_minimum(&trust.initial_trust, cut.initial_certificate(), &floor)?;
    let recovered = cut
        .with_empty_recovery(verified.recovery())
        .map_err(|e| e.to_string())?;
    let step = FormalReadStepV1::Recovery(Box::new(FormalRecoveryReadStepV1 {
        certificate: raw.certificate,
        issuance: raw.issuer_witness,
        fencing: FormalReadFencingEvidenceV1::HealthyPair(Box::new(seals)),
        original_tail: RecoveryTailWitnessV1::Empty,
    }));
    Ok((
        FullSyncCutFixture {
            world: original.world.clone(),
            anchor: recovered,
            material: original.material.clone(),
            proof: original.proof.clone(),
        },
        vec![step],
    ))
}

fn domain_bytes<T: Serialize>(domain: &str, value: &T) -> Vec<u8> {
    let mut bytes = (domain.len() as u64).to_be_bytes().to_vec();
    bytes.extend(domain.as_bytes());
    bytes.extend(serde_cbor::to_vec(value).unwrap());
    bytes
}

fn epoch_seal_with_tail(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalReadAnchorV1,
    next_epoch: u64,
    signer_key: u8,
    role: EndpointRole,
    tail: SealedTailV1,
) -> EpochSealReceiptV1 {
    let instance = match role {
        EndpointRole::Primary => predecessor.definition().primary.instance.clone(),
        EndpointRole::Replica => predecessor.definition().replica.instance.clone(),
    };
    let complete = HeadAnchor {
        position: cut.head().record().position,
        decision_hash: cut.head().head().record_id.clone(),
        qualified: true,
    };
    let checkpoint_before = EndpointCheckpointBindingV1 {
        revision: 4,
        commitment: fixtures::h(120 + signer_key),
        head: complete.clone(),
    };
    let body = EpochSealBodyV1 {
        schema_version: 1,
        instance: instance.clone(),
        recovery_operation: fixtures::h(150 + next_epoch as u8),
        highest_complete: complete.clone(),
        checkpoint_before: checkpoint_before.clone(),
        tail,
        next_epoch_floor: next_epoch,
    };
    let checkpoint = EndpointCheckpointBindingV1 {
        revision: checkpoint_before.revision + 1,
        commitment: fixtures::h(160 + signer_key),
        head: complete,
    };
    let resource = predecessor.policy().authorized_fencing_resource_bindings[0].clone();
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: resource.clone(),
        instance_digest: instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        epoch_floor: next_epoch,
        seal_digest: Some(body.digest().unwrap()),
    };
    let provider = MonotonicProviderEvidenceV1 {
        signature: fixtures::sign(
            domain_bytes("oasis7.endpoint.monotonic-provider.v1", &statement),
            3,
        ),
        statement,
    };
    let mut receipt =
        EpochSealReceiptV1::sign(body.clone(), provider, &fixtures::key(signer_key)).unwrap();
    let request = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().unwrap(),
        checkpoint,
        challenge: fixtures::h(170 + signer_key),
        purpose: ProtectedEndpointSignaturePurposeV1::EpochSeal,
        preimage: domain_bytes("oasis7.endpoint.epoch-seal.v1", &body),
    };
    let protected_body = ProtectedEndpointSignatureBodyV1 {
        schema_version: 1,
        custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
        instance_digest: instance.digest().unwrap(),
        resource_binding: resource,
        checkpoint: request.checkpoint.clone(),
        request_digest: request.digest().unwrap(),
        challenge: request.challenge.clone(),
        endpoint_signature: receipt.endpoint_signature.clone(),
    };
    receipt.protected_signature = Some(ProtectedEndpointSignatureV1 {
        provider_signature: fixtures::sign(
            domain_bytes(
                "oasis7.formal.protected_endpoint_signature.v1",
                &protected_body,
            ),
            3,
        ),
        body: protected_body,
    });
    receipt
}

fn successor(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalReadAnchorV1,
    epoch: u64,
    signer_keys: [u8; 3],
) -> AuthorityDefinitionV1 {
    let mut next = predecessor.definition().clone();
    next.epoch = epoch;
    next.writer_public_key = fixtures::public(signer_keys[0]);
    next.predecessor_authority_id = Some(predecessor.authority_id().unwrap());
    next.valid_from_position = cut.head().record().position + 1;
    for (binding, signer, role) in [
        (&mut next.primary, signer_keys[1], EndpointRole::Primary),
        (&mut next.replica, signer_keys[2], EndpointRole::Replica),
    ] {
        binding.instance.authority_epoch = epoch;
        binding.instance.endpoint_role = role;
        binding.instance.endpoint_id = format!("history-{epoch}-{role:?}");
        binding.instance.endpoint_public_key = fixtures::public(signer);
        binding.instance.instance_id = fixtures::h(signer + 10);
        binding.instance.storage_id = fixtures::h(signer + 20);
        binding.instance.storage_fingerprint = fixtures::h(signer + 30);
        binding.fault_domain_id = format!("history-domain-{epoch}-{role:?}");
    }
    next
}
