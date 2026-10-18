//! The v2 trusted module-command boundary.
//!
//! Provider values are candidates only.  Every value is validated against the
//! live module registry and the governed issuer/revocation view before a
//! cloned world is allowed to call the sandbox.  The clone is published only
//! after the call and its output have passed all checks.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oasis7_wasm_abi::CapabilityGrantV2;
use std::collections::BTreeSet;

use super::super::capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityRecord, CapabilityBudgetAccount,
    CapabilityInvocationContext,
};
use super::super::{CapabilityAuthorizationEvent, WorldError};
use super::World;
use super::capability_authorization_state::{capability_budget_key, validate_budget_account};

impl World {
    /// Legacy compatibility shim.  Authority cannot be installed from a
    /// process-local key/epoch pair; callers must provide finalized
    /// governance evidence through [`Self::install_capability_authority_record`].
    #[deprecated(note = "install a finalized CapabilityAuthorityRecord")]
    pub fn set_capability_trusted_issuer(
        &mut self,
        _issuer_id: &str,
        _public_key_hex: &str,
        _key_epoch: u64,
    ) -> Result<(), WorldError> {
        Err(deny(
            "direct issuer key registration is forbidden; install finalized authority evidence",
        ))
    }

    #[deprecated(note = "install a finalized CapabilityAuthorityRecord")]
    pub fn revoke_capability_grant_v2(&mut self, grant_id: &str) -> Result<(), WorldError> {
        let _ = grant_id;
        Err(deny(
            "direct grant revocation is forbidden; install finalized authority evidence",
        ))
    }

    #[deprecated(note = "install a finalized CapabilityAuthorityRecord")]
    pub fn set_capability_revocation_epoch(
        &mut self,
        _epoch: u64,
        _finalized_receipt_id: impl Into<String>,
    ) -> Result<(), WorldError> {
        Err(deny(
            "direct revocation epoch mutation is forbidden; install finalized authority evidence",
        ))
    }

    /// Reject the legacy unproven authority import path.
    ///
    /// Authority records are trust roots, so accepting a structurally valid
    /// record from a public API would let a caller choose its own issuer key.
    /// Use [`Self::install_capability_authority_record_with_finality`] with a
    /// certificate verified against the live governance signer set instead.
    #[deprecated(note = "install a CapabilityAuthorityRecord with finality proof")]
    pub fn install_capability_authority_record(
        &mut self,
        _record: CapabilityAuthorityRecord,
    ) -> Result<(), WorldError> {
        Err(deny(
            "authority installation requires a verified governance finality certificate",
        ))
    }

    /// Bind the invocation identity supplied by the trusted host.  This is
    /// persisted and immutable per grant/response nonce, so provider DTOs
    /// cannot choose a subject, presenter, or audience at execution time.
    pub fn install_capability_invocation_context(
        &mut self,
        context: CapabilityInvocationContext,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        validate_invocation_context(&context)?;
        let key =
            super::capability_authorization_events::capability_invocation_context_key(&context)?;
        if let Some(existing) = self.capability_invocation_contexts.get(&key)
            && existing != &context
        {
            return Err(deny("invocation context is immutable"));
        }
        if self.capability_invocation_contexts.get(&key) == Some(&context) {
            return Ok(());
        }
        if let oasis7_wasm_abi::CapabilitySubject::System { system_id, epoch } = &context.subject
            && self
                .capability_revocation_state
                .system_identities
                .get(system_id)
                != Some(epoch)
        {
            let system_identity_event = CapabilityAuthorizationEvent::SystemIdentityInstalled {
                system_id: system_id.clone(),
                epoch: *epoch,
            };
            self.append_capability_authorization_event_batch(vec![
                system_identity_event,
                CapabilityAuthorizationEvent::InvocationContextInstalled { key, context },
            ])?;
            return Ok(());
        }
        self.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::InvocationContextInstalled { key, context },
        ])?;
        Ok(())
    }

    /// Install the durable logical budget for a subject/grant pair.  The
    /// account is immutable once consumed; governance may install a new grant
    /// and account rather than mutating historical spend.
    pub fn install_capability_budget_account(
        &mut self,
        account: CapabilityBudgetAccount,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        validate_budget_account(&account)?;
        let key = capability_budget_key(&account.subject, &account.grant_id)?;
        if let Some(existing) = self.capability_budget_accounts.get(&key)
            && existing != &account
        {
            return Err(deny("capability budget account is immutable"));
        }
        if self.capability_budget_accounts.get(&key) == Some(&account) {
            return Ok(());
        }
        self.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::BudgetAccountInstalled { key, account },
        ])?;
        Ok(())
    }

    /// Insert an issuer-authenticated immutable grant into the durable view.
    /// Normal callers should use the executor, which performs this admission
    /// as part of its staged transaction; this method is provided for loading
    /// a governed grant before its first command.
    pub fn register_capability_grant_v2(
        &mut self,
        grant: CapabilityGrantV2,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        grant
            .validate()
            .map_err(|error| deny(format!("grant validation: {error}")))?;
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
        if grant.expires_at_tick.is_none() {
            return Err(deny("grant must carry an explicit expiry"));
        }
        if grant.status != "verified" {
            return Err(deny(
                "only finalized and verified grants may enter durable authorization state",
            ));
        }
        if grant.issued_at_tick > self.state.time {
            return Err(deny("grant is not issued at the current logical tick"));
        }
        self.verify_issuer(&grant)?;
        self.verify_live_revocation(&grant)?;
        self.verify_parent_chain(&grant)?;
        if let oasis7_wasm_abi::CapabilitySubject::System { system_id, .. } = &grant.subject {
            let manifest = self
                .active_module_manifest(grant.scope.module_id.as_str())
                .map_err(|error| deny(format!("system subject module is not live: {error:?}")))?;
            let expected_system_id = grant
                .scope
                .module_id
                .strip_prefix("module.")
                .map(|suffix| format!("system-{suffix}"));
            if expected_system_id.as_deref() != Some(system_id.as_str())
                || manifest.version != grant.scope.module_version
            {
                return Err(deny("system subject is not bound to a live module system"));
            }
        }
        let encoded = serde_json::to_value(&grant)?;
        if let Some(existing) = self.capability_grants_v2.get(&grant.grant_id)
            && existing != &encoded
        {
            return Err(deny("immutable grant body changed"));
        }
        self.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::GrantRegistered { grant },
        ])?;
        Ok(())
    }

    /// Validate one manifest capability against the live admission state.
    ///
    /// Legacy capabilities remain available for legacy module manifests.  A
    /// capability reference that is also present in the durable v2 registry is
    /// deliberately routed through the v2 checks instead of falling back to a
    /// legacy `allow_all` grant.  This keeps module activation aligned with
    /// the trusted executor's authority boundary.
    pub(super) fn validate_module_required_capability(
        &self,
        cap_ref: &str,
    ) -> Result<(), WorldError> {
        if self.capability_grants_v2.contains_key(cap_ref) {
            return self.validate_registered_v2_capability_for_admission(cap_ref);
        }

        let grant =
            self.capabilities
                .get(cap_ref)
                .ok_or_else(|| WorldError::ModuleChangeInvalid {
                    reason: format!("module cap missing {cap_ref}"),
                })?;
        if grant.is_expired(self.state.time) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module cap expired {cap_ref}"),
            });
        }
        Ok(())
    }

    /// Check the durable v2 grant used by a module manifest during register or
    /// activation.  The manifest only establishes that the grant is a module
    /// dependency; command/effect scope is checked again at execution time.
    pub(super) fn validate_registered_v2_capability_for_admission(
        &self,
        cap_ref: &str,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        let encoded = self
            .capability_grants_v2
            .get(cap_ref)
            .ok_or_else(|| deny("v2 capability is not in the durable registry"))?;
        let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
            .map_err(|_| deny("v2 capability grant is malformed"))?;
        grant
            .validate()
            .map_err(|error| deny(format!("v2 capability grant validation: {error}")))?;
        if grant.grant_id != cap_ref
            || !grant
                .body_hash_matches()
                .map_err(|error| deny(format!("v2 capability body hash: {error}")))?
            || grant
                .expected_grant_id()
                .map_err(|error| deny(format!("v2 capability id hash: {error}")))?
                != grant.grant_id
        {
            return Err(deny("v2 capability canonical body hash or id mismatch"));
        }
        if grant.status != "verified" {
            return Err(deny("v2 capability is not finalized and verified"));
        }
        if grant.expires_at_tick.is_none()
            || grant
                .expires_at_tick
                .is_some_and(|expiry| self.state.time > expiry)
            || grant.issued_at_tick > self.state.time
        {
            return Err(deny("v2 capability lifetime is not currently valid"));
        }
        self.verify_issuer(&grant)?;
        self.verify_live_revocation(&grant)?;
        self.verify_parent_chain(&grant)
    }

    pub(super) fn verify_issuer(&self, grant: &CapabilityGrantV2) -> Result<(), WorldError> {
        let issuer = &grant.issuer;
        let authority = self
            .capability_revocation_state
            .authority_records
            .get(&issuer.issuer_id)
            .ok_or_else(|| deny("issuer has no finalized authority record"))?;
        validate_authority_record(authority)?;
        let proof = self
            .capability_revocation_state
            .authority_finality_proofs
            .get(&issuer.issuer_id)
            .ok_or_else(|| deny("issuer has no replayable finality proof"))?;
        self.verify_capability_authority_finality_proof(authority, proof)?;
        if issuer.issuer_kind != authority.issuer_kind
            || issuer.governance_epoch != authority.governance_epoch
            || issuer.finalized_receipt_id != authority.finalized_receipt_id
            || issuer.key_id != authority.key_id
            || issuer.issuer_key_epoch != authority.issuer_key_epoch
            || issuer.authority_rotation_receipt_id != authority.authority_rotation_receipt_id
            || grant.audience.world_id != authority.world_id
            || grant.audience.branch_id != authority.branch_id
            || grant.audience.finality_epoch != authority.finality_epoch
        {
            return Err(deny("grant issuer or governance finality binding mismatch"));
        }
        let key_bytes = hex::decode(authority.public_key_hex.trim())
            .map_err(|_| deny("issuer public key is invalid"))?;
        let key_bytes: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| deny("issuer public key length is invalid"))?;
        let key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| deny("issuer public key is invalid"))?;
        for encoded in [&issuer.signature, &grant.issuance_signature] {
            let signature_hex = encoded
                .strip_prefix("ed25519:")
                .ok_or_else(|| deny("issuer signature algorithm is not ed25519"))?;
            let bytes =
                hex::decode(signature_hex).map_err(|_| deny("issuer signature is invalid hex"))?;
            let bytes: [u8; 64] = bytes
                .try_into()
                .map_err(|_| deny("issuer signature length is invalid"))?;
            key.verify(
                grant
                    .canonical_body_bytes()
                    .map_err(|error| deny(format!("canonical grant body: {error}")))?
                    .as_slice(),
                &Signature::from_bytes(&bytes),
            )
            .map_err(|_| deny("issuer signature verification failed"))?;
        }
        Ok(())
    }

    pub(super) fn verify_live_revocation(
        &self,
        grant: &CapabilityGrantV2,
    ) -> Result<(), WorldError> {
        let state = &self.capability_revocation_state;
        if state.epoch < grant.revocation_epoch {
            return Err(deny("revocation registry is stale"));
        }
        if state.revoked_grant_ids.contains(&grant.grant_id)
            || state.superseded_by.contains_key(&grant.grant_id)
        {
            return Err(deny("grant is revoked or superseded"));
        }
        Ok(())
    }
}

pub(super) fn validate_authority_record(
    record: &CapabilityAuthorityRecord,
) -> Result<(), WorldError> {
    for (field, value) in [
        ("issuer_id", record.issuer_id.as_str()),
        ("issuer_kind", record.issuer_kind.as_str()),
        ("key_id", record.key_id.as_str()),
        ("public_key_hex", record.public_key_hex.as_str()),
        ("finalized_receipt_id", record.finalized_receipt_id.as_str()),
        ("world_id", record.world_id.as_str()),
        ("branch_id", record.branch_id.as_str()),
        ("finality_block_hash", record.finality_block_hash.as_str()),
        ("finality_status", record.finality_status.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(deny(format!("authority record {field} is required")));
        }
    }
    if !matches!(
        record.issuer_kind.as_str(),
        "governance" | "system" | "kernel_migration"
    ) {
        return Err(deny("authority record issuer kind is invalid"));
    }
    if record.finality_status != "finalized" {
        return Err(deny("authority record is not finalized"));
    }
    if record
        .authority_rotation_receipt_id
        .as_deref()
        .is_some_and(|receipt| receipt.trim().is_empty())
    {
        return Err(deny(
            "capability authority rotation receipt cannot be empty",
        ));
    }
    let key_bytes = hex::decode(record.public_key_hex.trim())
        .map_err(|_| deny("authority record public key is invalid hex"))?;
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| deny("authority record public key length is invalid"))?;
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| deny("authority record public key is invalid"))?;
    if record
        .revoked_grant_ids
        .iter()
        .any(|grant_id| grant_id.trim().is_empty())
        || record.superseded_by.iter().any(|(grant_id, replacement)| {
            grant_id == replacement
                || !is_canonical_grant_id(grant_id)
                || !is_canonical_grant_id(replacement)
                || record.revoked_grant_ids.contains(replacement)
        })
    {
        return Err(deny("authority record contains an invalid supersession id"));
    }
    for (grant_id, replacement_id) in &record.superseded_by {
        let mut cursor = replacement_id.as_str();
        let mut visited = BTreeSet::new();
        while let Some(next_target) = record.superseded_by.get(cursor) {
            if !visited.insert(cursor.to_string()) || next_target == grant_id {
                return Err(deny("authority record supersession graph contains a cycle"));
            }
            cursor = next_target.as_str();
        }
    }
    Ok(())
}

fn is_canonical_grant_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn validate_agent_identity(
    agent_id: &str,
    identity: &CapabilityAgentIdentity,
) -> Result<(), WorldError> {
    if agent_id.trim().is_empty() || identity.owner_binding.trim().is_empty() {
        return Err(deny("capability agent identity fields are required"));
    }
    if identity.generation == 0 {
        return Err(deny(
            "capability agent identity generation must be positive",
        ));
    }
    Ok(())
}

pub(super) fn validate_invocation_context(
    context: &CapabilityInvocationContext,
) -> Result<(), WorldError> {
    for (field, value) in [
        ("grant_id", context.grant_id.as_str()),
        ("catalog_snapshot_id", context.catalog_snapshot_id.as_str()),
        ("module_id", context.module_id.as_str()),
        ("module_version", context.module_version.as_str()),
        ("response_nonce", context.response_nonce.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(deny(format!("invocation context {field} is required")));
        }
    }
    context
        .subject
        .validate()
        .map_err(|error| deny(format!("invocation context subject: {error}")))?;
    context
        .presenter
        .validate()
        .map_err(|error| deny(format!("invocation context presenter: {error}")))?;
    context
        .audience
        .validate()
        .map_err(|error| deny(format!("invocation context audience: {error}")))?;
    Ok(())
}

pub(super) fn deny(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}
