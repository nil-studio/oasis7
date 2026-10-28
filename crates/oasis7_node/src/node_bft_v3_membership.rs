//! Local installation is separate from globally verified schedule activation.
use super::{SelectedNodeBftConfigV3, error};
use crate::{NodeBftExecutionCommitResultV3, NodeError};
use oasis7_consensus::tendermint_v3::*;
use oasis7_distfs::protected_bft_v3::{
    BftLocalInstallEvidenceV3, BftSignerCheckpointV3, BftSignerLeaseV3,
    BftTransitionInstallPreparationV3,
};

pub(super) fn active_key(
    selected: &SelectedNodeBftConfigV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> Option<String> {
    let member = schedule
        .active_set()
        .validator(&selected.binding.local_validator_id)?;
    selected
        .binding
        .provisioned_signing_keys
        .iter()
        .find(|key| {
            key.validator_id == selected.binding.local_validator_id
                && key.public_key_hex == member.public_key_hex
        })
        .map(|key| key.public_key_hex.clone())
}

pub(super) fn install(
    selected: &SelectedNodeBftConfigV3,
    schedule: &BftHeaderHeightScheduleV3,
    result: &NodeBftExecutionCommitResultV3,
    lease: &mut dyn BftSignerLeaseV3,
    checkpoint: &mut BftSignerCheckpointV3,
) -> Result<(), NodeError> {
    if !selected.membership_enabled || schedule.last_activation_height() == 0 {
        return Ok(());
    }
    let Some(key) = active_key(selected, schedule) else {
        return Ok(());
    };
    let (confirmation, reference) = checkpoint
        .state
        .history
        .iter()
        .rev()
        .find_map(|c| {
            c.execution_commitment
                .transition_ref
                .as_ref()
                .filter(|r| r.activation_height == schedule.last_activation_height())
                .map(|r| (c, r))
        })
        .ok_or_else(|| error("wire-3 activation lacks exact confirmed ancestry"))?;
    if reference.new_validator_set_hash != schedule.active_set().set_hash()
        || reference.scheduled_height.checked_add(1)
            != Some(confirmation.execution_commitment.height)
        || result.height.checked_add(1) != Some(schedule.height())
        || checkpoint.state.history.last().is_none_or(|c| {
            c.execution_commitment.height != result.height
                || c.header.block_id().ok().as_deref() != Some(result.header_id.as_str())
        })
        || checkpoint.state.pending_driver_apply.is_some()
        || !checkpoint.state.applied_installs.iter().any(|r| {
            r.height == result.height
                && r.header_id == result.header_id
                && r.driver_install_digest == result.driver_install_digest
        })
    {
        return Err(error("wire-3 installation is not the actual confirmed tip"));
    }
    let source_height = reference.scheduled_height;
    let request_id = blake3::hash(
        format!(
            "oasis7.node.wire3.install/v1:{}:{}:{}:{}",
            selected
                .binding
                .digest()
                .map_err(|_| error("wire-3 binding digest"))?,
            source_height,
            reference.activation_height,
            key
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    let membership = checkpoint
        .state
        .membership_install
        .as_ref()
        .ok_or_else(|| error("wire-3 membership is not enabled in provider"))?;
    // A validated durable receipt remains valid after this install's tip.
    if membership.receipts.iter().any(|r| {
        r.body.request_id == request_id
            && r.body.activation_height == schedule.last_activation_height()
            && r.body.active_validator_set_hash == schedule.active_set().set_hash()
            && r.body.local_signing_public_key == key
    }) {
        return Ok(());
    }
    let (revision, digest) = membership
        .pending
        .as_ref()
        .filter(|i| i.body.request_id == request_id)
        .map(|i| {
            (
                i.body.expected_revision,
                i.body.expected_state_digest.clone(),
            )
        })
        .unwrap_or((
            checkpoint.state.revision,
            checkpoint.state.state_digest.clone(),
        ));
    let preparation = lease
        .prepare_transition_install(
            revision,
            &digest,
            request_id.clone(),
            source_height,
            selected.binding.operator_instance_id.clone(),
        )
        .map_err(|_| error("wire-3 installation preparation rejected"))?;
    let old_history = checkpoint.state.history.clone();
    let old_journal = checkpoint.state.journal.clone();
    let expected_wal = old_journal
        .as_ref()
        .ok_or_else(|| error("wire-3 install journal missing"))?
        .digest()
        .map_err(error)?;
    let (receipt, updated) = match preparation {
        BftTransitionInstallPreparationV3::Completed {
            receipt,
            checkpoint,
        } => (receipt, checkpoint),
        BftTransitionInstallPreparationV3::Pending {
            intent,
            checkpoint: prepared,
        } => {
            if prepared.state.history != old_history || prepared.state.journal != old_journal {
                return Err(error("wire-3 preparation changed actual history/journal"));
            }
            let wal = prepared
                .state
                .journal
                .as_ref()
                .ok_or_else(|| error("wire-3 install journal missing"))?
                .digest()
                .map_err(error)?;
            let evidence = BftLocalInstallEvidenceV3 {
                schema_version: 3,
                runtime_instance_id: selected.binding.operator_instance_id.clone(),
                installed_head_height: result.height,
                installed_head_commit_ref: result.header_id.clone(),
                installed_state_root: result.state_root.clone(),
                installed_manifest_hash: result.post_manifest_hash.clone(),
                installed_active_set_hash: schedule.active_set().set_hash().into(),
                installed_authority_epoch: schedule.authority_epoch(),
                local_validator_id: selected.binding.local_validator_id.clone(),
                local_signing_public_key: key.clone(),
                wal_install_digest: wal,
                driver_install_digest: result.driver_install_digest.clone(),
            };
            evidence
                .validate_for(&intent, &selected.binding)
                .map_err(|_| error("wire-3 install evidence mismatch"))?;
            let installed = lease
                .complete_transition_install(intent, evidence)
                .map_err(|_| error("wire-3 installation acknowledgement rejected"))?;
            (installed.receipt, installed.checkpoint)
        }
    };
    let b = &receipt.body;
    if updated.state.history != old_history
        || updated.state.journal != old_journal
        || b.request_id != request_id
        || b.activation_height != schedule.last_activation_height()
        || b.installed_head_height != result.height
        || b.installed_head_commit_ref != result.header_id
        || b.installed_state_root != result.state_root
        || b.installed_manifest_hash != result.post_manifest_hash
        || b.driver_install_digest != result.driver_install_digest
        || b.wal_install_digest != expected_wal
        || b.local_signing_public_key != key
        || b.active_validator_set_hash != schedule.active_set().set_hash()
        || b.authority_epoch != schedule.authority_epoch()
        || updated
            .state
            .membership_install
            .as_ref()
            .is_none_or(|m| !m.receipts.contains(&receipt))
    {
        return Err(error(
            "wire-3 install receipt differs from actual local installation",
        ));
    }
    *checkpoint = updated;
    Ok(())
}
