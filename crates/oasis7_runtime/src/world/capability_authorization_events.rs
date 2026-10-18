//! Replay application for trusted capability authorization transitions.
//!
//! Authorization maps are part of the world state, but their mutations must
//! also be journal evidence.  This module applies those evidence records
//! during both normal append and stale-snapshot recovery.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oasis7_wasm_abi::{
    CapabilityAudience, CapabilityCatalogEntry, CapabilityGrantV2, ModuleCallCaller,
    canonical_hash, capability_scope_hash,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use super::super::capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityFinalityBinding, CapabilityAuthorityFinalityProof,
    CapabilityAuthorityRecord, CapabilityAuthorizationAuditReceipt,
    CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
    CapabilityInvocationContext,
};
use super::super::governance::{GovernanceFinalityCertificate, ProposalStatus};
use super::super::{CapabilityAuthorizationEvent, WorldError, WorldTime};
use super::World;
use super::capability_authorization::{
    deny, validate_authority_record, validate_invocation_context,
};
use super::capability_authorization_state::{capability_budget_key, validate_budget_account};

impl World {
    pub(super) fn prepare_raw_command_commit(
        &self,
        event: &CapabilityAuthorizationEvent,
        time: WorldTime,
    ) -> Result<
        super::capability_authorization_command_projection::PreparedCapabilityCommandCommit,
        WorldError,
    > {
        let CapabilityAuthorizationEvent::CommandCommitted {
            budget_key,
            budget_before_remaining_units,
            budget_before_spent_units,
            state_hash_before,
            receipt_hash,
            budget_account,
            grant,
            nonce_key,
            nonce_record,
            receipt,
            effect_receipt_links,
        } = event
        else {
            return Err(deny("command projection requires a command commit event"));
        };
        apply_command_commit(
            self,
            event,
            budget_key,
            *budget_before_remaining_units,
            *budget_before_spent_units,
            state_hash_before,
            receipt_hash,
            budget_account,
            grant,
            nonce_key,
            nonce_record,
            receipt,
            effect_receipt_links,
            time,
        )
    }

    pub(super) fn prepare_raw_effect_receipt_commit(
        &self,
        event: &CapabilityAuthorizationEvent,
    ) -> Result<
        super::capability_effect_receipt_projection::PreparedCapabilityEffectReceipt,
        WorldError,
    > {
        let CapabilityAuthorizationEvent::EffectReceiptCommitted {
            intent_id,
            authorization_receipt_id,
            effect_receipt_id,
        } = event
        else {
            return Err(deny(
                "effect receipt projection requires an effect receipt commit event",
            ));
        };
        prepare_effect_receipt_commit(
            self,
            event,
            intent_id,
            authorization_receipt_id,
            effect_receipt_id,
        )
    }

    pub(super) fn verify_capability_authority_finality(
        &self,
        record: &CapabilityAuthorityRecord,
        certificate: &GovernanceFinalityCertificate,
    ) -> Result<(), WorldError> {
        if !self
            .governance_finality_epoch_snapshots
            .contains_key(&certificate.epoch_id)
        {
            return Err(deny(
                "authority finality requires a historical governance epoch snapshot",
            ));
        }
        let proposal = self
            .proposals
            .get(&certificate.proposal_id)
            .ok_or_else(|| deny("authority finality proposal is not known"))?;
        let proposal_manifest_hash = match &proposal.status {
            ProposalStatus::Approved { manifest_hash, .. }
            | ProposalStatus::Applied { manifest_hash } => manifest_hash,
            _ => return Err(deny("authority finality proposal is not approved")),
        };
        if proposal_manifest_hash != &certificate.manifest_hash {
            return Err(deny("authority finality proposal manifest hash mismatch"));
        }
        self.validate_governance_finality_certificate(
            certificate.proposal_id,
            certificate.manifest_hash.as_str(),
            certificate.epoch_id,
            certificate,
        )
        .map_err(|error| deny(format!("authority finality certificate: {error:?}")))?;
        if record.finality_epoch != certificate.epoch_id {
            return Err(deny("authority finality epoch does not match certificate"));
        }
        if !certificate.signatures.contains_key(&record.issuer_id) {
            return Err(deny(
                "authority issuer identity is not one of the finality signers",
            ));
        }
        let signer_public_key = self
            .node_identity_public_key(record.issuer_id.as_str())
            .ok_or_else(|| deny("authority issuer identity is not trusted"))?;
        if !signer_public_key.eq_ignore_ascii_case(record.public_key_hex.trim()) {
            return Err(deny(
                "authority issuer key does not match its finality signer identity",
            ));
        }
        Ok(())
    }

    pub(super) fn verify_capability_authority_finality_proof(
        &self,
        record: &CapabilityAuthorityRecord,
        proof: &CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        if proof.proof_version != CapabilityAuthorityFinalityProof::PROOF_VERSION_V1 {
            return Err(deny(
                "capability authority finality proof version is unsupported",
            ));
        }
        let expected_binding = CapabilityAuthorityFinalityBinding::from_record(record)
            .map_err(|error| deny(format!("capability authority binding: {error}")))?;
        if proof.binding != expected_binding {
            return Err(deny(
                "capability authority finality proof does not match authority record",
            ));
        }
        self.verify_capability_authority_finality(record, &proof.certificate)?;
        let certificate_signers: BTreeSet<&String> = proof.certificate.signatures.keys().collect();
        let proof_signers: BTreeSet<&String> = proof.signatures.keys().collect();
        if proof_signers != certificate_signers {
            return Err(deny(
                "capability authority finality proof signer set does not match certificate",
            ));
        }
        for (node_id, signature_with_prefix) in &proof.signatures {
            let signature_hex = signature_with_prefix
                .strip_prefix(CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1)
                .ok_or_else(|| {
                    deny(format!(
                        "capability authority proof signature prefix mismatch for {node_id}"
                    ))
                })?;
            let signer_public_key =
                self.node_identity_public_key(node_id.as_str())
                    .ok_or_else(|| {
                        deny(format!(
                            "capability authority proof signer is not trusted: {node_id}"
                        ))
                    })?;
            let public_key_bytes: [u8; 32] = hex::decode(signer_public_key)
                .map_err(|_| {
                    deny(format!(
                        "capability authority proof signer key is invalid: {node_id}"
                    ))
                })?
                .try_into()
                .map_err(|_| {
                    deny(format!(
                        "capability authority proof signer key length is invalid: {node_id}"
                    ))
                })?;
            let signature_bytes: [u8; 64] = hex::decode(signature_hex)
                .map_err(|_| {
                    deny(format!(
                        "capability authority proof signature is invalid: {node_id}"
                    ))
                })?
                .try_into()
                .map_err(|_| {
                    deny(format!(
                        "capability authority proof signature length is invalid: {node_id}"
                    ))
                })?;
            let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|_| {
                deny(format!(
                    "capability authority proof signer key is invalid: {node_id}"
                ))
            })?;
            let payload = proof
                .signing_payload_v1(node_id.as_str())
                .map_err(|error| deny(format!("capability authority proof payload: {error}")))?;
            verifying_key
                .verify(payload.as_slice(), &Signature::from_bytes(&signature_bytes))
                .map_err(|_| {
                    deny(format!(
                        "capability authority proof signature failed: {node_id}"
                    ))
                })?;
        }
        Ok(())
    }

    pub(super) fn apply_capability_authorization_event(
        &mut self,
        event: &CapabilityAuthorizationEvent,
        time: WorldTime,
    ) -> Result<(), WorldError> {
        match event {
            CapabilityAuthorizationEvent::AuthorityInstalled { .. } => {
                return Err(deny(
                    "record-only capability authority event has no replayable finality certificate",
                ));
            }
            CapabilityAuthorizationEvent::AuthorityInstalledWithFinality { .. } => {
                return Err(deny(
                    "certificate-only capability authority event has no binding proof",
                ));
            }
            CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, proof } => {
                apply_authority_record(self, record, proof)?;
            }
            CapabilityAuthorizationEvent::AgentIdentityInstalled { agent_id, identity } => {
                apply_agent_identity(self, agent_id, identity)?;
            }
            event @ (CapabilityAuthorizationEvent::AgentSignerDelegationInstalled { .. }
            | CapabilityAuthorizationEvent::WorldServiceIntentRecorded { .. }) => {
                super::world_service::apply_service_authorization_event(self, event)?
            }
            CapabilityAuthorizationEvent::SystemIdentityInstalled { system_id, epoch } => {
                apply_system_identity(self, system_id, *epoch)?;
            }
            CapabilityAuthorizationEvent::InvocationContextInstalled { key, context } => {
                apply_invocation_context(self, key, context)?;
            }
            CapabilityAuthorizationEvent::BudgetAccountInstalled { key, account } => {
                apply_budget_account(self, key, account)?;
            }
            CapabilityAuthorizationEvent::GrantRegistered { grant } => {
                apply_registered_grant(self, grant, time)?;
            }
            CapabilityAuthorizationEvent::CommandCommitted { .. } => {
                self.prepare_raw_command_commit(event, time)?.install(self);
                return Ok(());
            }
            CapabilityAuthorizationEvent::EffectReceiptCommitted { .. } => {
                self.prepare_raw_effect_receipt_commit(event)?.install(self);
                return Ok(());
            }
        }
        self.refresh_capability_authorization_root()
    }
}

fn apply_authority_record(
    world: &mut World,
    record: &CapabilityAuthorityRecord,
    proof: &CapabilityAuthorityFinalityProof,
) -> Result<(), WorldError> {
    validate_authority_record(record)?;
    world.verify_capability_authority_finality_proof(record, proof)?;
    if world.chain_resource_manifest.world_id != "unbound"
        && world.chain_resource_manifest.world_id != record.world_id
    {
        return Err(deny("authority record world does not match live world"));
    }
    if let Some(existing) = world
        .capability_revocation_state
        .authority_records
        .get(&record.issuer_id)
        && existing != record
    {
        validate_authority_record_transition(existing, record)?;
    }
    if let Some(existing) = world
        .capability_revocation_state
        .authority_finality_proofs
        .get(&record.issuer_id)
        && existing != proof
        && world
            .capability_revocation_state
            .authority_records
            .get(&record.issuer_id)
            == Some(record)
    {
        return Err(deny("authority finality proof is immutable"));
    }
    world.capability_revocation_state.epoch = world
        .capability_revocation_state
        .epoch
        .max(record.revocation_epoch);
    world
        .capability_revocation_state
        .revoked_grant_ids
        .extend(record.revoked_grant_ids.iter().cloned());
    world
        .capability_revocation_state
        .superseded_by
        .extend(record.superseded_by.clone());
    world.capability_revocation_state.finalized_receipt_id =
        Some(record.finalized_receipt_id.clone());
    world
        .capability_revocation_state
        .authority_records
        .insert(record.issuer_id.clone(), record.clone());
    world
        .capability_revocation_state
        .authority_finality_proofs
        .insert(record.issuer_id.clone(), proof.clone());
    Ok(())
}

/// Authority updates are themselves governed transitions.  A proof over a
/// replacement record is not enough on its own: replay must also establish
/// that the replacement did not erase a prior revocation/supersession or
/// silently change an issuer's governance context.
pub(super) fn validate_authority_record_transition(
    previous: &CapabilityAuthorityRecord,
    next: &CapabilityAuthorityRecord,
) -> Result<(), WorldError> {
    if previous.issuer_id != next.issuer_id
        || previous.issuer_kind != next.issuer_kind
        || previous.world_id != next.world_id
        || previous.branch_id != next.branch_id
        || previous.finality_epoch != next.finality_epoch
        || previous.finality_block_hash != next.finality_block_hash
        || previous.finality_status != next.finality_status
    {
        return Err(deny(
            "authority transition changes its finalized governance context",
        ));
    }
    let key_rotation = previous.issuer_key_epoch != next.issuer_key_epoch
        || previous.key_id != next.key_id
        || previous.public_key_hex != next.public_key_hex;
    if key_rotation {
        if next.issuer_key_epoch <= previous.issuer_key_epoch
            || next.authority_rotation_receipt_id.is_none()
            || next.authority_rotation_receipt_id == previous.authority_rotation_receipt_id
            || next.governance_epoch < previous.governance_epoch
        {
            return Err(deny(
                "authority key rotation requires a strictly newer key epoch and receipt",
            ));
        }
    } else if previous.authority_rotation_receipt_id != next.authority_rotation_receipt_id
        || previous.governance_epoch != next.governance_epoch
        || previous.finalized_receipt_id != next.finalized_receipt_id
    {
        return Err(deny(
            "authority rotation or governance receipt changed without a key rotation",
        ));
    }
    if next.revocation_epoch < previous.revocation_epoch
        || !next
            .revoked_grant_ids
            .is_superset(&previous.revoked_grant_ids)
    {
        return Err(deny("authority transition regressed revocation state"));
    }
    let revocation_changed = next.revoked_grant_ids != previous.revoked_grant_ids
        || next.superseded_by != previous.superseded_by;
    if revocation_changed && !key_rotation && next.revocation_epoch <= previous.revocation_epoch {
        return Err(deny(
            "revocation or supersession transition requires a newer registry epoch",
        ));
    }
    for (grant_id, replacement_id) in &previous.superseded_by {
        if next.superseded_by.get(grant_id) != Some(replacement_id) {
            return Err(deny("authority transition erased supersession state"));
        }
    }
    for (grant_id, replacement_id) in &next.superseded_by {
        if grant_id == replacement_id
            || !is_sha256_hex(grant_id)
            || !is_sha256_hex(replacement_id)
            || next.revoked_grant_ids.contains(replacement_id)
        {
            return Err(deny(
                "authority transition contains an invalid supersession target",
            ));
        }
        let mut cursor = replacement_id.as_str();
        let mut visited = BTreeSet::new();
        while let Some(next_target) = next.superseded_by.get(cursor) {
            if !visited.insert(cursor.to_string()) || next_target == grant_id {
                return Err(deny("authority supersession graph contains a cycle"));
            }
            cursor = next_target.as_str();
        }
    }
    Ok(())
}

#[path = "capability_agent_identity_events.rs"]
mod agent_identity;
use agent_identity::apply_agent_identity;

fn apply_system_identity(world: &mut World, system_id: &str, epoch: u64) -> Result<(), WorldError> {
    if system_id.trim().is_empty() || epoch > world.state.time {
        return Err(deny("capability system identity is not live"));
    }
    if let Some(existing) = world
        .capability_revocation_state
        .system_identities
        .get(system_id)
    {
        if *existing != epoch {
            return Err(deny(
                "capability system identity changed without a new epoch",
            ));
        }
        return Ok(());
    }
    world
        .capability_revocation_state
        .system_identities
        .insert(system_id.to_string(), epoch);
    Ok(())
}

fn apply_invocation_context(
    world: &mut World,
    key: &str,
    context: &CapabilityInvocationContext,
) -> Result<(), WorldError> {
    validate_invocation_context(context)?;
    let expected_key = capability_invocation_context_key(context)?;
    if key != expected_key {
        return Err(deny(
            "invocation context journal key does not match context",
        ));
    }
    if let Some(existing) = world.capability_invocation_contexts.get(key)
        && existing != context
    {
        return Err(deny("invocation context is immutable"));
    }
    world
        .capability_invocation_contexts
        .insert(key.to_string(), context.clone());
    Ok(())
}

fn apply_budget_account(
    world: &mut World,
    key: &str,
    account: &CapabilityBudgetAccount,
) -> Result<(), WorldError> {
    validate_budget_account(account)?;
    if capability_budget_key(&account.subject, &account.grant_id)? != key {
        return Err(deny("capability budget journal key does not match account"));
    }
    if let Some(existing) = world.capability_budget_accounts.get(key)
        && existing != account
    {
        return Err(deny("capability budget account is immutable"));
    }
    world
        .capability_budget_accounts
        .insert(key.to_string(), account.clone());
    Ok(())
}

fn apply_registered_grant(
    world: &mut World,
    grant: &CapabilityGrantV2,
    time: WorldTime,
) -> Result<(), WorldError> {
    let mut projected = world.capability_grants_v2.clone();
    validate_and_project_registered_grant(world, &mut projected, grant, time)?;
    world.capability_grants_v2 = projected;
    Ok(())
}

pub(super) fn validate_and_project_registered_grant(
    world: &World,
    capability_grants_v2: &mut BTreeMap<String, serde_json::Value>,
    grant: &CapabilityGrantV2,
    time: WorldTime,
) -> Result<(), WorldError> {
    validate_grant_body(grant, time)?;
    world.verify_issuer(grant)?;
    world.verify_live_revocation(grant)?;
    world.verify_parent_chain(grant)?;
    let encoded = serde_json::to_value(grant)?;
    if let Some(existing) = capability_grants_v2.get(&grant.grant_id)
        && existing != &encoded
    {
        return Err(deny("immutable grant body changed"));
    }
    capability_grants_v2.insert(grant.grant_id.clone(), encoded);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_command_commit(
    world: &World,
    event: &CapabilityAuthorizationEvent,
    budget_key: &str,
    budget_before_remaining_units: i64,
    budget_before_spent_units: i64,
    state_hash_before: &str,
    receipt_hash: &str,
    budget_account: &CapabilityBudgetAccount,
    grant: &CapabilityGrantV2,
    nonce_key: &str,
    nonce_record: &CapabilityAuthorizationNonceRecord,
    receipt: &CapabilityAuthorizationAuditReceipt,
    effect_receipt_links: &std::collections::BTreeMap<String, CapabilityEffectReceiptLink>,
    time: WorldTime,
) -> Result<
    super::capability_authorization_command_projection::PreparedCapabilityCommandCommit,
    WorldError,
> {
    let mut capability_grants_v2 = world.capability_grants_v2.clone();
    let mut capability_nonce_records = world.capability_nonce_records.clone();
    let mut capability_authorization_receipts = world.capability_authorization_receipts.clone();
    let mut capability_budget_accounts = world.capability_budget_accounts.clone();
    let mut capability_effect_receipt_links = world.capability_effect_receipt_links.clone();
    validate_grant_body(grant, time)?;
    world.verify_issuer(grant)?;
    world.verify_live_revocation(grant)?;
    world.verify_parent_chain(grant)?;
    validate_budget_account(budget_account)?;
    if state_hash_before != receipt.state_hash_before
        || !is_sha256_hex(receipt_hash)
        || authorization_receipt_hash(receipt)? != receipt_hash
    {
        return Err(deny(
            "capability authorization receipt integrity check failed",
        ));
    }
    if capability_budget_key(&budget_account.subject, &budget_account.grant_id)? != budget_key
        || budget_account.subject != grant.subject
        || budget_account.grant_id != grant.grant_id
        || budget_before_remaining_units < 0
        || budget_before_spent_units < 0
        || budget_account.reserved_units != 0
    {
        return Err(deny("capability command budget binding is invalid"));
    }
    if receipt.budget_before != budget_before_remaining_units
        || receipt.budget_after != Some(budget_account.remaining_units)
        || budget_account.spent_units < budget_before_spent_units
        || budget_account.remaining_units
            != budget_before_remaining_units
                .saturating_sub(budget_account.spent_units - budget_before_spent_units)
    {
        return Err(deny("capability command budget transition is invalid"));
    }
    if let Some(existing) = world.capability_budget_accounts.get(budget_key)
        && existing != budget_account
        && (existing.remaining_units != budget_before_remaining_units
            || existing.reserved_units != 0
            || existing.spent_units != budget_before_spent_units)
    {
        return Err(deny("capability command budget predecessor is invalid"));
    }
    if nonce_record.state != "committed"
        || nonce_record.request_hash.trim().is_empty()
        || nonce_record.outcome_hash.trim().is_empty()
        || nonce_record.committed_receipt_id.as_deref() != Some(receipt.receipt_id.as_str())
        || nonce_record.request_hash != receipt.canonical_request_hash
        || nonce_record.outcome_hash != receipt.canonical_result_hash
    {
        return Err(deny("capability command nonce journal record is invalid"));
    }
    let response_nonce = receipt
        .response_nonce
        .as_deref()
        .ok_or_else(|| deny("capability command receipt nonce is required"))?;
    if authorization_nonce_key(grant, response_nonce)? != nonce_key {
        return Err(deny("capability command nonce key is not canonical"));
    }
    let context_key =
        capability_invocation_context_key_for_values(grant.grant_id.as_str(), response_nonce)?;
    let context = world
        .capability_invocation_contexts
        .get(&context_key)
        .or_else(|| world.capability_invocation_contexts.get(&grant.grant_id))
        .ok_or_else(|| deny("capability command invocation context is missing"))?;
    let context_presenter = serde_json::to_value(&context.presenter)?;
    if context.grant_id != grant.grant_id
        || context.subject != grant.subject
        || context.audience != grant.audience
        || context.module_id != grant.scope.module_id
        || context.module_version != grant.scope.module_version
        || context.response_nonce != response_nonce
        || context_presenter != receipt.presenter.clone().unwrap_or_default()
        || context.catalog_snapshot_id != receipt.catalog_snapshot_id.clone().unwrap_or_default()
    {
        return Err(deny(
            "capability command receipt invocation context binding is invalid",
        ));
    }
    if receipt.grant_id.as_deref() != Some(grant.grant_id.as_str())
        || receipt.authorization_nonce_key_hash.as_deref() != Some(nonce_key)
        || receipt.decision != "accepted"
        || receipt.receipt_id.trim().is_empty()
        || receipt.canonical_request_hash.trim().is_empty()
        || receipt.canonical_result_hash.trim().is_empty()
        || receipt.subject != serde_json::to_value(&grant.subject)?
        || receipt.audience != serde_json::to_value(&grant.audience)?
        || receipt.presenter.is_none()
        || receipt.scope_hash
            != capability_scope_hash(&grant.scope)
                .map_err(|error| deny(format!("scope hash: {error}")))?
        || receipt.module_id.as_deref() != Some(grant.scope.module_id.as_str())
        || receipt.module_version.as_deref() != Some(grant.scope.module_version.as_str())
        || receipt
            .catalog_snapshot_id
            .as_deref()
            .is_none_or(str::is_empty)
        || receipt.state_hash_before.trim().is_empty()
        || receipt
            .state_hash_after
            .as_deref()
            .is_none_or(str::is_empty)
        || !is_sha256_hex(receipt.state_hash_before.as_str())
        || !is_sha256_hex(receipt.state_hash_after.as_deref().unwrap_or_default())
        || !is_sha256_hex(receipt.canonical_request_hash.as_str())
        || !is_sha256_hex(receipt.canonical_result_hash.as_str())
    {
        return Err(deny(
            "capability command receipt journal binding is invalid",
        ));
    }
    let authority = world
        .capability_revocation_state
        .authority_records
        .get(&grant.issuer.issuer_id)
        .ok_or_else(|| deny("capability command receipt issuer authority is missing"))?;
    if receipt.branch_id != authority.branch_id
        || receipt.finality_epoch != authority.finality_epoch
        || receipt.finality_status != "verified"
        || receipt.finality_block_hash.as_deref() != Some(authority.finality_block_hash.as_str())
    {
        return Err(deny(
            "capability command receipt finality binding is invalid",
        ));
    }
    let active_manifest = world
        .active_module_manifest(grant.scope.module_id.as_str())
        .map_err(|error| {
            deny(format!(
                "capability command receipt module is missing: {error:?}"
            ))
        })?;
    let manifest_hash = canonical_hash(&active_manifest)
        .map_err(|error| deny(format!("capability command receipt manifest hash: {error}")))?;
    if receipt.manifest_hash.as_deref() != Some(manifest_hash.as_str())
        || receipt.state_hash_after.as_deref()
            != Some(
                canonical_hash(&world.state)
                    .map_err(|error| {
                        deny(format!("capability command receipt state hash: {error}"))
                    })?
                    .as_str(),
            )
    {
        return Err(deny(
            "capability command receipt state or manifest hash is invalid",
        ));
    }
    // Sandbox output events are journaled between the preflight head and the
    // authorization commit, so the current tail is not necessarily the
    // receipt's `world_head_before`.  Validate ordering and bind the after
    // head to the event id allocated for this commit in both live and replay
    // paths.
    let current_head = world
        .journal
        .events
        .last()
        .map(|event| event.id)
        .unwrap_or(0);
    if receipt.world_head_before > current_head
        || receipt.world_head_after != Some(world.next_event_id)
        || receipt.world_head_before >= world.next_event_id
    {
        return Err(deny(
            "capability command receipt journal head binding is invalid",
        ));
    }
    let encoded = serde_json::to_value(grant)?;
    if let Some(existing) = world.capability_grants_v2.get(&grant.grant_id)
        && existing != &encoded
    {
        return Err(deny("immutable grant body changed"));
    }
    capability_grants_v2.insert(grant.grant_id.clone(), encoded);

    if let Some(existing) = world.capability_nonce_records.get(nonce_key)
        && existing != nonce_record
    {
        return Err(deny("capability nonce journal record changed"));
    }
    capability_nonce_records.insert(nonce_key.to_string(), nonce_record.clone());

    if let Some(existing) = world
        .capability_authorization_receipts
        .get(&receipt.receipt_id)
        && existing != receipt
    {
        return Err(deny("capability authorization receipt changed"));
    }
    capability_authorization_receipts.insert(receipt.receipt_id.clone(), receipt.clone());

    if let Some(existing) = world.capability_budget_accounts.get(budget_key)
        && (existing.remaining_units < budget_account.remaining_units
            || existing.spent_units > budget_account.spent_units)
    {
        return Err(deny("capability budget journal transition regressed"));
    }
    capability_budget_accounts.insert(budget_key.to_string(), budget_account.clone());

    for (intent_id, link) in effect_receipt_links {
        if intent_id.trim().is_empty() || link.authorization_receipt_id != receipt.receipt_id {
            return Err(deny("capability effect receipt journal binding is invalid"));
        }
        let effect_is_durable = world
            .pending_effects
            .iter()
            .any(|intent| intent.identity_key() == *intent_id)
            || world.inflight_effects.contains_key(intent_id);
        if !effect_is_durable {
            return Err(deny(
                "capability authorization-linked effect is missing from durable queues",
            ));
        }
        if let Some(existing) = world.capability_effect_receipt_links.get(intent_id)
            && existing != link
        {
            return Err(deny("capability effect receipt link changed"));
        }
        capability_effect_receipt_links.insert(intent_id.clone(), link.clone());
    }
    let capability_authorization_root = world
        .compute_capability_authorization_root_with_full_projection(
            &capability_grants_v2,
            &world.capability_revocation_state,
            &world.capability_invocation_contexts,
            &capability_budget_accounts,
            &capability_nonce_records,
            &capability_authorization_receipts,
            &capability_effect_receipt_links,
        )?;
    Ok(
        super::capability_authorization_command_projection::PreparedCapabilityCommandCommit {
            event: event.clone(),
            capability_grants_v2,
            capability_nonce_records,
            capability_authorization_receipts,
            capability_budget_accounts,
            capability_effect_receipt_links,
            capability_authorization_root,
        },
    )
}

fn prepare_effect_receipt_commit(
    world: &World,
    event: &CapabilityAuthorizationEvent,
    intent_id: &str,
    authorization_receipt_id: &str,
    effect_receipt_id: &str,
) -> Result<super::capability_effect_receipt_projection::PreparedCapabilityEffectReceipt, WorldError>
{
    let mut capability_authorization_receipts = world.capability_authorization_receipts.clone();
    let mut capability_effect_receipt_links = world.capability_effect_receipt_links.clone();
    if intent_id.trim().is_empty()
        || authorization_receipt_id.trim().is_empty()
        || effect_receipt_id.trim().is_empty()
    {
        return Err(deny(
            "effect receipt authorization journal binding is required",
        ));
    }
    let Some(link) = world.capability_effect_receipt_links.get(intent_id) else {
        let already_committed = world
            .capability_authorization_receipts
            .get(authorization_receipt_id)
            .is_some_and(|receipt| {
                receipt
                    .committed_effect_receipt_ids
                    .contains(effect_receipt_id)
                    || receipt.committed_effect_receipt_id.as_deref() == Some(effect_receipt_id)
            });
        if already_committed {
            let capability_authorization_root = world
                .compute_capability_authorization_root_with_full_projection(
                    &world.capability_grants_v2,
                    &world.capability_revocation_state,
                    &world.capability_invocation_contexts,
                    &world.capability_budget_accounts,
                    &world.capability_nonce_records,
                    &capability_authorization_receipts,
                    &capability_effect_receipt_links,
                )?;
            return Ok(
                super::capability_effect_receipt_projection::PreparedCapabilityEffectReceipt {
                    event: event.clone(),
                    capability_authorization_receipts,
                    capability_effect_receipt_links,
                    capability_authorization_root,
                },
            );
        }
        return Err(deny("effect receipt authorization link is missing"));
    };
    if link.authorization_receipt_id != authorization_receipt_id {
        return Err(deny("effect receipt authorization link does not match"));
    }
    let audit = capability_authorization_receipts
        .get_mut(authorization_receipt_id)
        .ok_or_else(|| deny("effect receipt authorization link has no audit receipt"))?;
    // One authorization command may emit multiple independently receipted
    // effects.  Keep the historical first-id projection for compatibility,
    // while the set records every closure and makes replay/idempotency
    // deterministic for each linked intent.
    if audit.committed_effect_receipt_id.is_none() {
        audit.committed_effect_receipt_id = Some(effect_receipt_id.to_string());
    }
    audit
        .committed_effect_receipt_ids
        .insert(effect_receipt_id.to_string());
    capability_effect_receipt_links.remove(intent_id);
    let capability_authorization_root = world
        .compute_capability_authorization_root_with_full_projection(
            &world.capability_grants_v2,
            &world.capability_revocation_state,
            &world.capability_invocation_contexts,
            &world.capability_budget_accounts,
            &world.capability_nonce_records,
            &capability_authorization_receipts,
            &capability_effect_receipt_links,
        )?;
    Ok(
        super::capability_effect_receipt_projection::PreparedCapabilityEffectReceipt {
            event: event.clone(),
            capability_authorization_receipts,
            capability_effect_receipt_links,
            capability_authorization_root,
        },
    )
}

fn validate_grant_body(grant: &CapabilityGrantV2, time: WorldTime) -> Result<(), WorldError> {
    grant
        .validate()
        .map_err(|error| deny(format!("grant validation: {error}")))?;
    if grant.status != "verified" {
        return Err(deny(
            "only finalized and verified grants may enter durable authorization state",
        ));
    }
    if !grant
        .body_hash_matches()
        .map_err(|error| deny(format!("grant body hash: {error}")))?
        || grant
            .expected_grant_id()
            .map_err(|error| deny(format!("grant id hash: {error}")))?
            != grant.grant_id
    {
        return Err(deny("grant canonical body hash or id mismatch"));
    }
    if grant.issued_at_tick > time {
        return Err(deny("grant is not issued at the current logical tick"));
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn authorization_receipt_hash(
    receipt: &CapabilityAuthorizationAuditReceipt,
) -> Result<String, WorldError> {
    canonical_hash(receipt).map_err(|error| deny(format!("authorization receipt hash: {error}")))
}

pub(super) fn module_call_caller(subject: &oasis7_wasm_abi::CapabilitySubject) -> ModuleCallCaller {
    match subject {
        oasis7_wasm_abi::CapabilitySubject::Agent { agent_id, .. } => ModuleCallCaller::Agent {
            agent_id: agent_id.clone(),
        },
        oasis7_wasm_abi::CapabilitySubject::Module { module_id, .. } => ModuleCallCaller::Module {
            module_id: module_id.clone(),
        },
        oasis7_wasm_abi::CapabilitySubject::System { system_id, .. } => ModuleCallCaller::System {
            system_id: system_id.clone(),
        },
    }
}

pub(super) fn validate_subject_for_manifest(
    world: &World,
    subject: &oasis7_wasm_abi::CapabilitySubject,
    manifest: &oasis7_wasm_abi::ModuleManifest,
) -> Result<(), WorldError> {
    match subject {
        oasis7_wasm_abi::CapabilitySubject::Agent {
            agent_id,
            owner_binding,
            generation,
        } => {
            let Some(agent) = world.state.agents.get(agent_id) else {
                return Err(deny("agent subject does not identify a live agent"));
            };
            if agent.state.agent_id != *agent_id {
                return Err(deny("live agent state id does not match its registry key"));
            }
            let Some(identity) = world
                .capability_revocation_state
                .agent_identities
                .get(agent_id)
            else {
                return Err(deny("live agent capability identity is not bound"));
            };
            if identity.owner_binding != *owner_binding || identity.generation != *generation {
                return Err(deny(
                    "agent subject owner or generation does not match live identity",
                ));
            }
        }
        oasis7_wasm_abi::CapabilitySubject::Module {
            module_id,
            module_version,
            instance_id,
        } => {
            if module_id != &manifest.module_id || module_version != &manifest.version {
                return Err(deny(
                    "module subject identity does not match the active module",
                ));
            }
            let Some(instance) = world.state.module_instances.get(instance_id) else {
                return Err(deny("module subject does not identify a live instance"));
            };
            if !instance.active
                || instance.instance_id != *instance_id
                || instance.module_id != *module_id
                || instance.module_version != *module_version
                || instance.wasm_hash != manifest.wasm_hash
            {
                return Err(deny(
                    "module subject instance does not match the active module identity",
                ));
            }
        }
        oasis7_wasm_abi::CapabilitySubject::System { system_id, epoch } => {
            if world
                .capability_revocation_state
                .system_identities
                .get(system_id)
                != Some(epoch)
            {
                return Err(deny("system subject does not identify a live binding"));
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct InvocationContextKey<'a> {
    grant_id: &'a str,
    response_nonce: &'a str,
}

#[derive(Serialize)]
struct AuthorizationNonceKey<'a> {
    subject: &'a oasis7_wasm_abi::CapabilitySubject,
    issuer_id: &'a str,
    issuer_key_epoch: u64,
    grant_id: &'a str,
    audience: &'a CapabilityAudience,
    branch_id: &'a str,
    finality_epoch: u64,
    nonce: &'a str,
}

pub(super) fn authorization_nonce_key(
    grant: &CapabilityGrantV2,
    response_nonce: &str,
) -> Result<String, WorldError> {
    if response_nonce.trim().is_empty() {
        return Err(deny("authorization response nonce is required"));
    }
    canonical_hash(&AuthorizationNonceKey {
        subject: &grant.subject,
        issuer_id: &grant.issuer.issuer_id,
        issuer_key_epoch: grant.issuer.issuer_key_epoch,
        grant_id: &grant.grant_id,
        audience: &grant.audience,
        branch_id: &grant.audience.branch_id,
        finality_epoch: grant.audience.finality_epoch,
        nonce: response_nonce,
    })
    .map_err(|error| deny(format!("authorization nonce key: {error}")))
}

pub(super) fn capability_invocation_context_key(
    context: &CapabilityInvocationContext,
) -> Result<String, WorldError> {
    capability_invocation_context_key_for_values(
        context.grant_id.as_str(),
        context.response_nonce.as_str(),
    )
}

pub(super) fn capability_invocation_context_key_for_values(
    grant_id: &str,
    response_nonce: &str,
) -> Result<String, WorldError> {
    if grant_id.trim().is_empty() || response_nonce.trim().is_empty() {
        return Err(deny(
            "invocation context grant id and response nonce are required",
        ));
    }
    canonical_hash(&InvocationContextKey {
        grant_id,
        response_nonce,
    })
    .map_err(|error| deny(format!("invocation context key: {error}")))
}

pub(super) fn audience_matches(grant: &CapabilityGrantV2, audience: &CapabilityAudience) -> bool {
    grant.audience == *audience
}

pub(super) fn scope_matches_command(
    grant: &CapabilityGrantV2,
    entry: &CapabilityCatalogEntry,
    payload: &[u8],
) -> bool {
    grant.scope.module_id == entry.module_id
        && grant.scope.module_version == entry.module_version
        && grant.scope.namespace == entry.namespace
        && grant.scope.object_kind == "command"
        && grant.scope.object_name == entry.command
        && grant.scope.operation == "execute"
        && grant.scope.max_payload_bytes.is_some_and(|max| {
            u64::try_from(payload.len()).is_ok_and(|payload_len| payload_len <= max)
        })
        && serde_json::from_slice::<serde_json::Value>(payload)
            .ok()
            .or_else(|| serde_cbor::from_slice::<serde_json::Value>(payload).ok())
            .is_some_and(|value| scope_selectors_match_json(&grant.scope, &value))
}

/// Selector-bearing grants are only valid when the command/effect payload has
/// an explicit, typed target for every constrained dimension.  A malformed or
/// target-less payload is denied instead of being interpreted as an implicit
/// wildcard.  Selector lists are allowlists; an omitted (`None`) selector
/// authorizes only an un-targeted value for that dimension.
pub(super) fn scope_selectors_match_json(
    scope: &oasis7_wasm_abi::CapabilityScope,
    payload: &serde_json::Value,
) -> bool {
    if !canonical_target_payload(payload) {
        return false;
    }
    selector_matches_json(scope.entity_selector.as_deref(), payload, "entity_id")
        && selector_matches_json(scope.resource_selector.as_deref(), payload, "resource_id")
}

/// Commands and effects use a deliberately small target schema.  Arbitrary
/// `*_id`/recipient/target fields are not accepted as an implicit target: a
/// producer must use `entity_id` and/or `resource_id`, and the grant must
/// carry the corresponding selector.  Walk nested JSON too, otherwise a
/// provider could hide an opaque target under a metadata object.
fn canonical_target_payload(payload: &serde_json::Value) -> bool {
    match payload {
        serde_json::Value::Object(fields) => fields.iter().all(|(field, value)| {
            let normalized = field.to_ascii_lowercase();
            let allowed = matches!(normalized.as_str(), "entity_id" | "resource_id");
            let target_bearing = allowed
                || normalized == "target"
                || normalized == "target_id"
                || normalized == "target_ids"
                || normalized == "object_id"
                || normalized == "object_ids"
                || normalized == "recipient"
                || normalized == "recipient_id"
                || normalized == "recipient_ids"
                || normalized == "owner_id"
                || normalized == "owner_ids"
                || normalized == "destination_id"
                || normalized == "destination_ids"
                || normalized.ends_with("_id")
                || normalized.ends_with("_ids");
            (!target_bearing || allowed && value.is_string()) && canonical_target_payload(value)
        }),
        serde_json::Value::Array(values) => values.iter().all(canonical_target_payload),
        _ => true,
    }
}

fn selector_matches_json(
    selectors: Option<&[String]>,
    payload: &serde_json::Value,
    field: &str,
) -> bool {
    let mut target_values = Vec::new();
    collect_target_values(payload, field, &mut target_values);

    match selectors {
        // An omitted selector is not an implicit wildcard.  It can authorize
        // an un-targeted payload, but must reject a payload that attempts to
        // supply a target without a corresponding grant allowlist.
        None => target_values.is_empty(),
        Some(selectors) => {
            !target_values.is_empty()
                && target_values
                    .iter()
                    .all(|value| selectors.iter().any(|selector| selector == value))
        }
    }
}

fn collect_target_values<'a>(
    payload: &'a serde_json::Value,
    field: &str,
    values: &mut Vec<&'a str>,
) {
    match payload {
        serde_json::Value::Object(fields) => {
            for (name, value) in fields {
                if name.eq_ignore_ascii_case(field) {
                    if let Some(value) = value.as_str() {
                        values.push(value);
                    }
                } else {
                    collect_target_values(value, field, values);
                }
            }
        }
        serde_json::Value::Array(values_array) => {
            for value in values_array {
                collect_target_values(value, field, values);
            }
        }
        _ => {}
    }
}
