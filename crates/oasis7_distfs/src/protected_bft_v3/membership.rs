//! Explicit extension of the original V3 state; absence retains legacy bytes.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::install::*;
use super::state::{BftSignerCandidateV3, BftSignerStateV3};
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, verify_formal_bft_header_certificate_v3,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftMembershipInstallStateV3 {
    pub schema_version: u32,
    pub pending: Option<BftTransitionInstallIntentV3>,
    #[serde(deserialize_with = "super::state::bounded_sequence")]
    pub receipts: Vec<BftTransitionInstallReceiptV3>,
}
impl BftMembershipInstallStateV3 {
    pub(crate) fn enabled() -> Self {
        Self {
            schema_version: 1,
            pending: None,
            receipts: Vec::new(),
        }
    }
}

pub(super) fn contexts(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
) -> Result<Vec<BftTransitionInstallIntentBodyV3>, BftSignerErrorV3> {
    let mut schedule = binding.initial_schedule().clone();
    let mut result = Vec::new();
    for (index, certificate) in state.history.iter().enumerate() {
        let next =
            verify_formal_bft_header_certificate_v3(&schedule, certificate).map_err(invalid)?;
        if let Some(source) = index.checked_sub(1).and_then(|i| state.history.get(i))
            && let Some(transition) = &source.execution_commitment.transition
        {
            let activated = next.next_height_schedule();
            let commitment = &certificate.execution_commitment;
            let public_key = activated
                .active_set()
                .validator(&binding.local_validator_id)
                .map(|v| v.public_key_hex.clone())
                .unwrap_or_default();
            result.push(BftTransitionInstallIntentBodyV3 {
                schema_version: 3,
                binding_digest: binding.digest()?,
                expected_revision: 0,
                expected_state_digest: String::new(),
                lease_generation: 0,
                request_id: String::new(),
                intent_nonce: String::new(),
                runtime_instance_id: binding.operator_instance_id.clone(),
                transition_source_height: source.execution_commitment.height,
                transition_source_parent_commit_ref: transition.source_commit_ref.clone(),
                transition_block_id: source.header.block_id().map_err(invalid)?,
                transition_digest: transition.transition_digest().map_err(invalid)?,
                confirmation_height: commitment.height,
                confirmation_block_id: certificate.header.block_id().map_err(invalid)?,
                activation_height: activated.height(),
                install_head_height: commitment.height,
                old_validator_set_hash: transition.old_validator_set_hash.clone(),
                new_validator_set_hash: transition.new_validator_set_hash.clone(),
                install_active_set_hash: activated.active_set().set_hash().into(),
                target_authority_epoch: activated.authority_epoch(),
                install_parent_commit_ref: activated.parent_commit_ref().into(),
                install_state_root: activated.parent_state_root().into(),
                install_manifest_hash: activated.parent_manifest_hash().into(),
                local_validator_id: binding.local_validator_id.clone(),
                local_signing_public_key: public_key,
            });
        }
        schedule = next.next_height_schedule().clone();
    }
    Ok(result)
}
fn context_matches(
    a: &BftTransitionInstallIntentBodyV3,
    b: &BftTransitionInstallIntentBodyV3,
) -> bool {
    a.transition_source_height == b.transition_source_height
        && a.transition_source_parent_commit_ref == b.transition_source_parent_commit_ref
        && a.transition_block_id == b.transition_block_id
        && a.transition_digest == b.transition_digest
        && a.confirmation_height == b.confirmation_height
        && a.confirmation_block_id == b.confirmation_block_id
        && a.activation_height == b.activation_height
        && a.old_validator_set_hash == b.old_validator_set_hash
        && a.new_validator_set_hash == b.new_validator_set_hash
        && a.install_active_set_hash == b.install_active_set_hash
        && a.target_authority_epoch == b.target_authority_epoch
        && a.local_signing_public_key == b.local_signing_public_key
}
pub(super) fn validate_intent_context(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
    intent: &BftTransitionInstallIntentV3,
) -> Result<(), BftSignerErrorV3> {
    let b = &intent.body;
    let list = contexts(binding, state)?;
    let c = list
        .iter()
        .find(|c| c.transition_source_height == b.transition_source_height)
        .ok_or_else(|| invalid("BFT V3 install source is not confirmed"))?;
    let head = state
        .history
        .iter()
        .find(|h| h.execution_commitment.height == b.install_head_height)
        .ok_or_else(|| invalid("BFT V3 installed head missing"))?;
    let active = list
        .iter()
        .filter(|c| c.activation_height <= b.install_head_height.saturating_add(1))
        .max_by_key(|c| c.activation_height)
        .ok_or_else(|| invalid("BFT V3 active install context absent"))?;
    if !context_matches(b, c)
        || active.transition_source_height != c.transition_source_height
        || head.header.block_id().map_err(invalid)? != b.install_parent_commit_ref
        || head.execution_commitment.state_root != b.install_state_root
        || head.execution_commitment.post_manifest_hash != b.install_manifest_hash
    {
        return Err(invalid(
            "BFT V3 install context differs from verified history",
        ));
    }
    Ok(())
}
pub(super) fn validate_records(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
    provider: Option<&str>,
) -> Result<(), BftSignerErrorV3> {
    let Some(m) = &state.membership_install else {
        return Ok(());
    };
    if m.schema_version != 1 || m.receipts.len() > 64 {
        return Err(invalid("BFT V3 membership extension schema/capacity"));
    }
    super::crypto::encode_bounded(m, 1024 * 1024)?;
    let mut previous_source = 0;
    let mut previous_revision = 0;
    let mut requests = std::collections::BTreeSet::new();
    for receipt in &m.receipts {
        receipt.verify_for(binding, provider.unwrap_or(&receipt.provider_public_key))?;
        let b = &receipt.body;
        let context = BftTransitionInstallIntentBodyV3 {
            schema_version: 3,
            binding_digest: b.binding_digest.clone(),
            expected_revision: b
                .installed_revision
                .checked_sub(2)
                .ok_or_else(|| invalid("BFT V3 install receipt revision"))?,
            expected_state_digest: b.pre_install_state_digest.clone(),
            lease_generation: b.lease_generation,
            request_id: b.request_id.clone(),
            intent_nonce: "0".repeat(64),
            runtime_instance_id: b.runtime_instance_id.clone(),
            transition_source_height: b.source_height,
            transition_source_parent_commit_ref: b.transition_source_parent_commit_ref.clone(),
            transition_block_id: b.transition_block_id.clone(),
            transition_digest: b.transition_digest.clone(),
            confirmation_height: b.confirmation_height,
            confirmation_block_id: b.confirmation_block_id.clone(),
            activation_height: b.activation_height,
            install_head_height: b.installed_head_height,
            old_validator_set_hash: b.old_validator_set_hash.clone(),
            new_validator_set_hash: b.new_validator_set_hash.clone(),
            install_active_set_hash: b.active_validator_set_hash.clone(),
            target_authority_epoch: b.authority_epoch,
            install_parent_commit_ref: b.installed_head_commit_ref.clone(),
            install_state_root: b.installed_state_root.clone(),
            install_manifest_hash: b.installed_manifest_hash.clone(),
            local_validator_id: b.local_validator_id.clone(),
            local_signing_public_key: b.local_signing_public_key.clone(),
        };
        validate_intent_context(
            binding,
            state,
            &BftTransitionInstallIntentV3 {
                body: context,
                provider_public_key: String::new(),
                provider_signature_hex: String::new(),
            },
        )?;
        if b.source_height <= previous_source
            || b.installed_revision <= previous_revision
            || b.installed_revision > state.revision
            || !requests.insert(&b.request_id)
            || !state.applied_installs.iter().any(|r| {
                r.height == b.installed_head_height
                    && r.header_id == b.installed_head_commit_ref
                    && r.driver_install_digest == b.driver_install_digest
            })
        {
            return Err(invalid("BFT V3 install receipt order/driver binding"));
        }
        previous_source = b.source_height;
        previous_revision = b.installed_revision;
    }
    if let Some(intent) = &m.pending {
        intent.verify_for(binding, provider.unwrap_or(&intent.provider_public_key))?;
        validate_intent_context(binding, state, intent)?;
        if state.revision
            != intent
                .body
                .expected_revision
                .checked_add(1)
                .ok_or_else(|| invalid("BFT V3 pending revision overflow"))?
            || requests.contains(&intent.body.request_id)
            || m.receipts
                .iter()
                .any(|r| r.body.source_height == intent.body.transition_source_height)
            || state.pending_driver_apply.is_some()
            || state.history.last().map(|c| c.execution_commitment.height)
                != Some(intent.body.install_head_height)
        {
            return Err(invalid("BFT V3 pending installation state differs"));
        }
    }
    Ok(())
}
pub(super) fn signing_gate(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> Result<(), BftSignerErrorV3> {
    let Some(m) = &state.membership_install else {
        return if schedule.pending_transition().is_some() || schedule.last_activation_height() > 0 {
            Err(invalid("BFT V3 membership installation unsupported"))
        } else {
            Ok(())
        };
    };
    if m.pending.is_some() {
        return Err(invalid("BFT V3 membership installation pending"));
    }
    if schedule.last_activation_height() > 0 {
        let key = schedule
            .active_set()
            .validator(&binding.local_validator_id)
            .ok_or_else(|| invalid("BFT V3 follower cannot sign"))?;
        if !m.receipts.iter().any(|r| {
            r.body.activation_height == schedule.last_activation_height()
                && r.body.active_validator_set_hash == schedule.active_set().set_hash()
                && r.body.authority_epoch == schedule.authority_epoch()
                && r.body.local_signing_public_key == key.public_key_hex
        }) {
            return Err(invalid("BFT V3 active key install not acknowledged"));
        }
    }
    Ok(())
}
pub(super) fn validate_change(
    binding: &BftSignerBindingV3,
    old: &BftSignerStateV3,
    next: &BftSignerCandidateV3,
    provider: &str,
) -> Result<(), BftSignerErrorV3> {
    if old.membership_install == next.membership_install {
        return Ok(());
    }
    if next.history != old.history
        || next.journal != old.journal
        || next.completed_journals != old.completed_journals
        || next.pending_driver_apply != old.pending_driver_apply
        || next.applied_installs != old.applied_installs
        || old.pending_driver_apply.is_some()
    {
        return Err(invalid(
            "BFT V3 membership mutation changed consensus state",
        ));
    }
    let m = next
        .membership_install
        .as_ref()
        .ok_or_else(|| invalid("BFT V3 membership cannot be disabled"))?;
    match &old.membership_install {
        None if *m == BftMembershipInstallStateV3::enabled()
            && old.journal.as_ref().is_none_or(|j| {
                j.pending_proposal_intent().is_none()
                    && j.pending_vote_intent().is_none()
                    && j.pending_commit().is_none()
            }) =>
        {
            Ok(())
        }
        Some(before) if before.pending.is_none() && m.receipts == before.receipts => {
            let intent = m
                .pending
                .as_ref()
                .ok_or_else(|| invalid("BFT V3 missing install intent"))?;
            intent.verify_for(binding, provider)?;
            if intent.body.expected_revision != old.revision
                || intent.body.expected_state_digest != old.state_digest
            {
                return Err(invalid("BFT V3 install intent stale"));
            }
            Ok(())
        }
        Some(before)
            if m.pending.is_none()
                && m.receipts.len() == before.receipts.len() + 1
                && m.receipts.starts_with(&before.receipts) =>
        {
            let intent = before
                .pending
                .as_ref()
                .ok_or_else(|| invalid("BFT V3 completion missing pending intent"))?;
            let r = m
                .receipts
                .last()
                .ok_or_else(|| invalid("BFT V3 receipt missing"))?;
            if r.body.intent_digest != intent.digest()?
                || r.body.request_id != intent.body.request_id
                || r.body.pre_install_state_digest != intent.body.expected_state_digest
                || r.body.installed_revision
                    != old
                        .revision
                        .checked_add(1)
                        .ok_or_else(|| invalid("BFT V3 revision overflow"))?
            {
                return Err(invalid("BFT V3 completion original intent mismatch"));
            }
            r.verify_for(binding, provider)
        }
        _ => Err(invalid("BFT V3 illegal membership mutation")),
    }
}
