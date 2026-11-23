//! The successor's first record is a state-preserving authority control record.
use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::ArtifactRole;

const INPUT_DOMAIN: &str = "oasis7.formal.recovery_activation.input.v1";
const INPUT_BYTES_LIMIT: usize = 64 * 1024;

pub fn recovery_activation_request_digest(
    authority: &VerifiedAuthorityV1,
) -> Result<String, FormalError> {
    let activation = authority
        .recovery_activation()
        .ok_or_else(|| invalid("authority has no planned recovery activation"))?;
    hash(
        "oasis7.formal.recovery_activation.request.v1",
        &(
            &authority.definition().world,
            authority.authority_id()?,
            authority.definition().epoch,
            activation,
        ),
    )
}

pub fn recovery_activation_input_bytes(
    certificate: &SignedRecoveryCertificateV1,
) -> Result<Vec<u8>, FormalError> {
    if certificate.body.schema_version != 3 || certificate.body.tail != RecoveryTailV1::Empty {
        return Err(invalid(
            "recovery activation requires a drained schema3 certificate",
        ));
    }
    let encoded = bytes(INPUT_DOMAIN, certificate)?;
    if encoded.len() > INPUT_BYTES_LIMIT {
        return Err(invalid("recovery activation input frame/size"));
    }
    Ok(encoded)
}

pub fn verify_recovery_activation_input(
    encoded: &[u8],
    record: &CanonicalRecordV1,
    authority: &VerifiedAuthorityV1,
) -> Result<(), FormalError> {
    let certificate = decode_recovery_activation_input(encoded)?;
    let authorization = authority
        .recovery_activation()
        .ok_or_else(|| invalid("recovery activation authorization missing"))?;
    if recovery_certificate_digest(&certificate)? != authorization.certificate_digest
        || certificate.body.successor != *authority.definition()
        || certificate.body.sealed_cut
            != *authority
                .recovery_cut
                .as_ref()
                .ok_or_else(|| invalid("recovery activation cut missing"))?
        || certificate.body.first_normal_position != authority.first_normal_position
        || blake3::hash(encoded).to_hex().as_str() != record.payload_digest
    {
        return Err(invalid(
            "recovery activation original certificate/input binding",
        ));
    }
    verify_with_preimage(
        &certificate.signature_hex,
        &authority.policy().recovery_issuer_public_key,
        || recovery_signing_bytes(&certificate.body),
        |_| Ok(()),
    )
}

pub fn decode_recovery_activation_input(
    encoded: &[u8],
) -> Result<SignedRecoveryCertificateV1, FormalError> {
    let prefix = bytes(INPUT_DOMAIN, &())?;
    let prefix = &prefix[..8 + INPUT_DOMAIN.len()];
    if encoded.len() > INPUT_BYTES_LIMIT || !encoded.starts_with(prefix) {
        return Err(invalid("recovery activation input frame/size"));
    }
    let certificate: SignedRecoveryCertificateV1 = serde_cbor::from_slice(&encoded[prefix.len()..])
        .map_err(|error| invalid(error.to_string()))?;
    if recovery_activation_input_bytes(&certificate)? != encoded {
        return Err(invalid("recovery activation canonical input bytes"));
    }
    Ok(certificate)
}

pub(crate) fn verify_recovery_activation_record(
    record: &CanonicalRecordV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    frozen: Option<&FormalFrozenCertificateV1>,
) -> Result<(), FormalError> {
    verify_recovery_activation_identity(record, authority, frozen)?;
    if authority.recovery_cut.as_ref() != Some(&parent.head())
        || record.world != parent.record().world
        || record.before_state_root != parent.record().after_state_root
        || record.after_state_root != parent.record().after_state_root
        || record.runtime_manifest_digest != parent.record().runtime_manifest_digest
        || record.governing_manifest_digest != parent.record().governing_manifest_digest
        || record.execution_policy_digest != parent.record().execution_policy_digest
        || record.action_root != parent.record().action_root
        || record.event_root != parent.record().event_root
        || record.projection_root != parent.record().projection_root
        || parent.record().handoff.is_some()
        || record
            .artifact_roots
            .get(&ArtifactRole::NativeExecutionEnvironment)
            != parent
                .record()
                .artifact_roots
                .get(&ArtifactRole::NativeExecutionEnvironment)
    {
        return Err(invalid(
            "recovery activation must preserve the exact qualified parent state",
        ));
    }
    Ok(())
}

/// Only the private historical-inclusion and signed-plan verifiers use this
/// parent-free check: each already pins the complete control record through its
/// own verified original cut. It does not construct or return a reader anchor.
pub(crate) fn verify_recovery_activation_identity(
    record: &CanonicalRecordV1,
    authority: &VerifiedAuthorityV1,
    frozen: Option<&FormalFrozenCertificateV1>,
) -> Result<(), FormalError> {
    let FormalRecordKindV1::RecoveryActivation {
        certificate_digest,
        plan_digest,
        pending_handoff_root,
    } = &record.kind
    else {
        return Err(invalid("recovery activation record kind"));
    };
    let authorization = authority
        .recovery_activation()
        .ok_or_else(|| invalid("recovery activation original permission missing"))?;
    authorization.validate()?;
    let cut = authority
        .recovery_cut
        .as_ref()
        .ok_or_else(|| invalid("recovery activation original cut missing"))?;
    if certificate_digest != &authorization.certificate_digest
        || plan_digest != &authorization.plan_digest
        || pending_handoff_root != &authorization.pending_handoff_root
        || record.position != authority.definition().valid_from_position
        || Some(record.position) != cut.position.checked_add(1)
        || record.position.checked_add(1) != authority.first_normal_position
        || record.parent_record_id != cut.record_id
        || record.request_digest != recovery_activation_request_digest(authority)?
        || record.before_state_root != record.after_state_root
        || record.handoff.is_some()
        || frozen.is_some()
        || record.frozen_reservation_id.is_some()
    {
        return Err(invalid(
            "recovery activation must be the sole state-preserving successor control record",
        ));
    }
    Ok(())
}
