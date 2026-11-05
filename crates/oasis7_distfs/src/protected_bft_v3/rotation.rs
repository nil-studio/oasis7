//! Completed typed consent/possession proofs, independent of consensus slots.
//! This archive proves original signing results, never controller authority.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, decode_key, invalid};
use super::state::{BftSignerCandidateV3, BftSignerStateV3, VerifiedBftSignerStateV3};
use oasis7_agent_api::world_action::{
    FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
};
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, verify_formal_bft_header_certificate_v3,
};
use serde::{Deserialize, Serialize};

pub const BFT_FINALITY_ROTATION_MAX_PROOFS_V3: usize = 64;
const ARCHIVE_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftFinalityRotationProofRecordV3 {
    pub body: FinalityKeyRotationBodyV1,
    pub public_key_hex: String,
    pub proof: FinalityKeyRotationProofV1,
    pub recorded_revision: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftFinalityRotationProofArchiveV3 {
    pub schema_version: u32,
    #[serde(deserialize_with = "super::state::bounded_sequence")]
    pub records: Vec<BftFinalityRotationProofRecordV3>,
}
pub(super) fn selected_key(
    body: &FinalityKeyRotationBodyV1,
    role: FinalityKeyRotationRoleV1,
) -> &str {
    match role {
        FinalityKeyRotationRoleV1::OldKeyAuthorization => &body.old_public_key_hex,
        FinalityKeyRotationRoleV1::NewKeyPossession => &body.new_public_key_hex,
    }
}
/// Verify the exact typed preimage. Shape-only API proof validation is not enough.
pub(super) fn verify_proof(
    body: &FinalityKeyRotationBodyV1,
    proof: &FinalityKeyRotationProofV1,
    role: FinalityKeyRotationRoleV1,
) -> Result<(), BftSignerErrorV3> {
    proof.validate_for(body, role).map_err(invalid)?;
    let key = decode_key(selected_key(body, role))?;
    let signature: [u8; 64] = hex::decode(&proof.signature_hex)
        .map_err(|_| invalid("BFT V3 rotation signature encoding"))?
        .try_into()
        .map_err(|_| invalid("BFT V3 rotation signature length"))?;
    key.verify_strict(
        &body.signing_bytes(role).map_err(invalid)?,
        &ed25519_dalek::Signature::from_bytes(&signature),
    )
    .map_err(|_| invalid("BFT V3 rotation proof signature rejected"))
}
fn validate_context(
    binding: &BftSignerBindingV3,
    schedule: &BftHeaderHeightScheduleV3,
    body: &FinalityKeyRotationBodyV1,
) -> Result<(), BftSignerErrorV3> {
    body.validate().map_err(invalid)?;
    // Enforce the preimage's aggregate bound before invoking a key handle.
    body.signing_bytes(FinalityKeyRotationRoleV1::OldKeyAuthorization)
        .map_err(invalid)?;
    let auth = &body.authorization;
    let current = schedule
        .active_set()
        .validator(&binding.local_validator_id)
        .ok_or_else(|| invalid("BFT V3 inactive old key cannot consent to rotation"))?;
    if auth.world_id != binding.scope.world_id
        || auth.chain_id != binding.scope.chain_id
        || auth.genesis_digest != binding.scope.genesis_hash
        || auth.parent_position != schedule.parent_tick()
        || auth.parent_record_id != schedule.parent_commit_ref()
        || auth.parent_state_root != schedule.parent_state_root()
        || body.node_id != binding.local_validator_id
        || body.old_public_key_hex != current.public_key_hex
        || body.decision_height != schedule.height()
        || body.decision_height.checked_add(2) != Some(body.activation_height)
        || body
            .target_epoch
            .checked_mul(schedule.policy().epoch_length_ticks)
            != Some(body.activation_height)
        || schedule.pending_transition().is_some()
        || schedule
            .active_set()
            .validators()
            .values()
            .any(|v| v.public_key_hex == body.new_public_key_hex)
        || [&body.old_public_key_hex, &body.new_public_key_hex]
            .iter()
            .any(|public| {
                !binding.provisioned_signing_keys.iter().any(|k| {
                    k.validator_id == body.node_id && k.public_key_hex.as_str() == public.as_str()
                })
            })
    {
        return Err(invalid(
            "BFT V3 rotation body differs from protected current context/provisioned keys",
        ));
    }
    // Admission/generation/controller policy/nonce remain World-owned checks.
    // The opaque header schedule contains none of those private governance maps.
    Ok(())
}
pub(super) fn validate_fresh(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
    schedule: &BftHeaderHeightScheduleV3,
    body: &FinalityKeyRotationBodyV1,
) -> Result<(), BftSignerErrorV3> {
    if state.membership_install.is_none() {
        return Err(invalid(
            "BFT V3 rotation proofs require explicit membership enable",
        ));
    }
    validate_context(binding, schedule, body)?;
    super::state::validate_signing_install_gate(binding, state, schedule)?;
    Ok(())
}
pub(super) fn validate_archive(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
) -> Result<(), BftSignerErrorV3> {
    let Some(archive) = &state.finality_rotation_proofs else {
        return Ok(());
    };
    if archive.schema_version != 1
        || archive.records.is_empty()
        || archive.records.len() > BFT_FINALITY_ROTATION_MAX_PROOFS_V3
        || state.membership_install.is_none()
    {
        return Err(invalid(
            "BFT V3 finality proof archive schema/capacity/enable",
        ));
    }
    super::crypto::encode_bounded(archive, ARCHIVE_BYTES)?;
    let mut schedules = Vec::with_capacity(state.history.len() + 1);
    let mut schedule = binding.initial_schedule().clone();
    schedules.push(schedule.clone());
    for certificate in &state.history {
        schedule = verify_formal_bft_header_certificate_v3(&schedule, certificate)
            .map_err(invalid)?
            .next_height_schedule()
            .clone();
        schedules.push(schedule.clone());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut previous_revision = 0;
    for record in &archive.records {
        let role = record.proof.role;
        let role_id = match role {
            FinalityKeyRotationRoleV1::OldKeyAuthorization => 0u8,
            FinalityKeyRotationRoleV1::NewKeyPossession => 1u8,
        };
        if record.recorded_revision <= previous_revision
            || record.recorded_revision > state.revision
            || !seen.insert((&record.body.rotation_id, role_id))
            || record.public_key_hex != selected_key(&record.body, role)
        {
            return Err(invalid(
                "BFT V3 finality proof archive original order/key conflict",
            ));
        }
        let original = schedules
            .iter()
            .find(|s| s.height() == record.body.decision_height)
            .ok_or_else(|| invalid("BFT V3 archived proof parent not in protected prefix"))?;
        validate_context(binding, original, &record.body)?;
        if original.last_activation_height() > 0
            && state.membership_install.as_ref().is_none_or(|m| {
                !m.receipts.iter().any(|r| {
                    r.body.activation_height == original.last_activation_height()
                        && r.body.active_validator_set_hash == original.active_set().set_hash()
                        && r.body.local_signing_public_key == record.body.old_public_key_hex
                        && r.body.authority_epoch == original.authority_epoch()
                        && r.body.installed_revision < record.recorded_revision
                })
            })
        {
            return Err(invalid(
                "BFT V3 archived proof predates old-key installation",
            ));
        }
        verify_proof(&record.body, &record.proof, role)?;
        previous_revision = record.recorded_revision;
    }
    Ok(())
}
pub(super) fn validate_change(
    binding: &BftSignerBindingV3,
    previous: &VerifiedBftSignerStateV3,
    next: &BftSignerCandidateV3,
) -> Result<(), BftSignerErrorV3> {
    let old = &previous.state;
    if old.finality_rotation_proofs == next.finality_rotation_proofs {
        return Ok(());
    }
    if next.history != old.history
        || next.journal != old.journal
        || next.completed_journals != old.completed_journals
        || next.membership_install != old.membership_install
        || next.pending_driver_apply != old.pending_driver_apply
        || next.applied_installs != old.applied_installs
    {
        return Err(invalid(
            "BFT V3 finality proof append changed consensus/install state",
        ));
    }
    let archive = next
        .finality_rotation_proofs
        .as_ref()
        .ok_or_else(|| invalid("BFT V3 finality proof archive cannot be removed"))?;
    let original = old
        .finality_rotation_proofs
        .as_ref()
        .map_or(&[][..], |a| a.records.as_slice());
    if archive.records.len() != original.len() + 1
        || !archive.records.starts_with(original)
        || archive.records.len() > BFT_FINALITY_ROTATION_MAX_PROOFS_V3
    {
        return Err(invalid(
            "BFT V3 finality proof archive is not exact single append",
        ));
    }
    let record = archive
        .records
        .last()
        .ok_or_else(|| invalid("BFT V3 finality proof record absent"))?;
    if record.recorded_revision
        != old
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V3 proof revision overflow"))?
    {
        return Err(invalid("BFT V3 proof append revision mismatch"));
    }
    validate_fresh(binding, old, &previous.schedule, &record.body)?;
    verify_proof(&record.body, &record.proof, record.proof.role)
}

#[cfg(test)]
#[path = "rotation_tests.rs"]
mod tests;
