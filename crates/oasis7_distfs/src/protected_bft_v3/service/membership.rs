use super::*;
use crate::protected_bft_v3::install::*;
use crate::protected_bft_v3::membership::{BftMembershipInstallStateV3, contexts};
impl BftSignerServiceV3 {
    pub(super) fn membership_command(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let state = &snapshot.state;
        let mut candidate = state.candidate();
        let result = match &request.body.command {
            BftSignerCommandV3::EnableMembership {
                expected_revision,
                expected_state_digest,
            } => {
                // Explicit enable is idempotent, but still binds its caller's current CAS.
                if state.revision != *expected_revision
                    || state.state_digest != *expected_state_digest
                {
                    return Err(invalid("BFT V3 membership enable stale"));
                }
                if state.membership_install.is_some() {
                    return self.state_reply(
                        snapshot,
                        request,
                        request_digest,
                        BftSignerReplyKindV3::MembershipEnabled,
                    );
                }
                candidate.membership_install = Some(BftMembershipInstallStateV3::enabled());
                BftSignerReplyKindV3::MembershipEnabled
            }
            BftSignerCommandV3::PrepareTransitionInstall {
                expected_revision,
                expected_state_digest,
                request_id,
                transition_source_height,
                runtime_instance_id,
            } => {
                let m = state
                    .membership_install
                    .as_ref()
                    .ok_or_else(|| invalid("BFT V3 membership not explicitly enabled"))?;
                if let Some(receipt) = m.receipts.iter().find(|r| r.body.request_id == *request_id)
                {
                    if receipt.body.installed_revision.checked_sub(2) != Some(*expected_revision)
                        || receipt.body.pre_install_state_digest != *expected_state_digest
                        || receipt.body.source_height != *transition_source_height
                        || receipt.body.runtime_instance_id != *runtime_instance_id
                    {
                        return Err(invalid("BFT V3 completed install request reused"));
                    }
                    return self.state_reply(
                        snapshot.clone(),
                        request,
                        request_digest,
                        BftSignerReplyKindV3::InstallPreparedCompleted {
                            receipt: Box::new(receipt.clone()),
                        },
                    );
                }
                if let Some(intent) = &m.pending {
                    let b = &intent.body;
                    if b.request_id != *request_id
                        || b.expected_revision != *expected_revision
                        || b.expected_state_digest != *expected_state_digest
                        || b.transition_source_height != *transition_source_height
                        || b.runtime_instance_id != *runtime_instance_id
                    {
                        return Err(invalid("BFT V3 pending original install differs"));
                    }
                    return self.state_reply(
                        snapshot.clone(),
                        request,
                        request_digest,
                        BftSignerReplyKindV3::InstallPreparedPending {
                            intent: Box::new(intent.clone()),
                        },
                    );
                }
                if state.revision != *expected_revision
                    || state.state_digest != *expected_state_digest
                    || runtime_instance_id != &self.binding.operator_instance_id
                {
                    return Err(invalid("BFT V3 install preparation stale/instance"));
                }
                if state.journal.as_ref().is_none_or(|j| {
                    j.pending_commit().is_some()
                        || j.pending_proposal_intent().is_some()
                        || j.pending_vote_intent().is_some()
                        || !j.events.is_empty()
                }) {
                    return Err(invalid(
                        "BFT V3 install requires fresh replayed active-key WAL",
                    ));
                }
                let mut body = contexts(&self.binding, state)?
                    .into_iter()
                    .find(|c| c.transition_source_height == *transition_source_height)
                    .ok_or_else(|| invalid("BFT V3 source not confirmed"))?;
                let schedule = &runtime
                    .verified
                    .as_ref()
                    .ok_or_else(|| invalid("BFT V3 verified schedule missing"))?
                    .schedule;
                if body.activation_height != schedule.last_activation_height()
                    || body.new_validator_set_hash != schedule.active_set().set_hash()
                    || m.receipts
                        .iter()
                        .any(|r| r.body.source_height == *transition_source_height)
                {
                    return Err(invalid(
                        "BFT V3 source is not current uninstalled activation",
                    ));
                }
                body.expected_revision = *expected_revision;
                body.expected_state_digest = expected_state_digest.clone();
                body.lease_generation = credential.generation;
                body.request_id = request_id.clone();
                body.intent_nonce = random_nonce()?;
                body.install_head_height = schedule
                    .height()
                    .checked_sub(1)
                    .ok_or_else(|| invalid("BFT V3 install head overflow"))?;
                body.install_parent_commit_ref = schedule.parent_commit_ref().into();
                body.install_state_root = schedule.parent_state_root().into();
                body.install_manifest_hash = schedule.parent_manifest_hash().into();
                validate_intent_body(&body, &self.binding)?;
                let intent = BftTransitionInstallIntentV3::sign(body, self.provider_key.as_ref())?;
                intent.verify_for(&self.binding, &self.provider_key.public_key_hex())?;
                candidate.membership_install.as_mut().unwrap().pending = Some(intent.clone());
                BftSignerReplyKindV3::InstallPreparedPending {
                    intent: Box::new(intent),
                }
            }
            BftSignerCommandV3::CompleteTransitionInstall {
                intent,
                acknowledgement,
            } => {
                let m = state
                    .membership_install
                    .as_ref()
                    .ok_or_else(|| invalid("BFT V3 membership not enabled"))?;
                intent.verify_for(&self.binding, &self.provider_key.public_key_hex())?;
                acknowledgement.verify_for(intent, &self.binding)?;
                let ack_digest = acknowledgement.digest()?;
                let intent_digest = intent.digest()?;
                if m.pending.as_ref() != Some(intent.as_ref()) {
                    let receipt = m
                        .receipts
                        .iter()
                        .find(|r| {
                            r.body.request_id == intent.body.request_id
                                && r.body.intent_digest == intent_digest
                                && r.body.acknowledgement_digest == ack_digest
                        })
                        .ok_or_else(|| {
                            invalid("BFT V3 completion differs from original pending intent")
                        })?;
                    return self.state_reply(
                        snapshot.clone(),
                        request,
                        request_digest,
                        BftSignerReplyKindV3::InstallPreparedCompleted {
                            receipt: Box::new(receipt.clone()),
                        },
                    );
                }
                if intent.body.lease_generation > credential.generation {
                    return Err(invalid("BFT V3 install from future lease"));
                }
                let evidence = &acknowledgement.body.evidence;
                if state
                    .journal
                    .as_ref()
                    .map(|j| j.digest())
                    .transpose()
                    .map_err(invalid)?
                    .as_deref()
                    != Some(evidence.wal_install_digest.as_str())
                    || !state.applied_installs.iter().any(|r| {
                        r.height == evidence.installed_head_height
                            && r.header_id == evidence.installed_head_commit_ref
                            && r.driver_install_digest == evidence.driver_install_digest
                    })
                {
                    return Err(invalid("BFT V3 install WAL/actual driver receipt differs"));
                }
                let next_revision = state
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("BFT V3 install revision overflow"))?;
                let receipt_body = BftTransitionInstallReceiptBodyV3 {
                    schema_version: 3,
                    binding_digest: self.binding.digest()?,
                    request_id: intent.body.request_id.clone(),
                    intent_digest: intent.digest()?,
                    acknowledgement_digest: ack_digest,
                    source_height: intent.body.transition_source_height,
                    transition_source_parent_commit_ref: intent
                        .body
                        .transition_source_parent_commit_ref
                        .clone(),
                    transition_block_id: intent.body.transition_block_id.clone(),
                    transition_digest: intent.body.transition_digest.clone(),
                    confirmation_height: intent.body.confirmation_height,
                    confirmation_block_id: intent.body.confirmation_block_id.clone(),
                    activation_height: intent.body.activation_height,
                    installed_head_height: evidence.installed_head_height,
                    old_validator_set_hash: intent.body.old_validator_set_hash.clone(),
                    new_validator_set_hash: intent.body.new_validator_set_hash.clone(),
                    active_validator_set_hash: evidence.installed_active_set_hash.clone(),
                    local_validator_id: evidence.local_validator_id.clone(),
                    local_signing_public_key: evidence.local_signing_public_key.clone(),
                    authority_epoch: evidence.installed_authority_epoch,
                    runtime_instance_id: evidence.runtime_instance_id.clone(),
                    installed_head_commit_ref: evidence.installed_head_commit_ref.clone(),
                    installed_state_root: evidence.installed_state_root.clone(),
                    installed_manifest_hash: evidence.installed_manifest_hash.clone(),
                    wal_install_digest: evidence.wal_install_digest.clone(),
                    driver_install_digest: evidence.driver_install_digest.clone(),
                    lease_generation: credential.generation,
                    installed_revision: next_revision,
                    pre_install_state_digest: intent.body.expected_state_digest.clone(),
                };
                let receipt =
                    BftTransitionInstallReceiptV3::sign(receipt_body, self.provider_key.as_ref())?;
                let extension = candidate.membership_install.as_mut().unwrap();
                extension.pending = None;
                extension.receipts.push(receipt.clone());
                BftSignerReplyKindV3::InstallPreparedCompleted {
                    receipt: Box::new(receipt),
                }
            }
            _ => return Err(invalid("BFT V3 wrong membership command")),
        };
        self.commit_candidate(
            runtime,
            credential,
            token_digest,
            state,
            candidate,
            None,
            request,
            request_digest,
            result,
        )
    }
}
