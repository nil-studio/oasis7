//! Trusted v2 module-command execution and staged commit.
//!
//! Provider values are candidates only. Every value is validated against the
//! live module registry and the governed issuer/revocation view before a
//! borrowed-base stage is allowed to call the sandbox. The typed stage is
//! installed only after the call and its output have passed all checks.

use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, ModuleCallInput,
    ModuleCallOrigin, ModuleKind, ModuleSandbox, canonical_hash, capability_scope_hash,
    validate_module_command_declarations, validate_module_command_envelope,
};
use std::collections::{BTreeMap, BTreeSet};

use super::super::capability_authorization::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord,
    CapabilityBudgetAccount, CapabilityEffectReceiptLink,
};
use super::super::{
    CapabilityAuthorizationEvent, EffectIntent, EffectOrigin, PolicyDecisionRecord, WorldError,
    WorldEventBody,
};
use super::World;
use super::capability_authorization::deny;
use super::capability_authorization_command_stage::TrustedCommandStage;
use super::capability_authorization_state::{
    budget_reservation_units, capability_actual_units, capability_budget_key,
    validate_budget_account,
};

impl World {
    /// Execute one authenticated v2 command. The executor argument is kept
    /// generic so the runtime remains buildable on wasm targets where the
    /// native executor dependency is unavailable; sandbox is the actual call
    /// boundary, as in the existing module runtime.
    pub fn execute_trusted_module_command<E>(
        &mut self,
        grant: CapabilityGrantV2,
        catalog: CapabilityCatalogSnapshot,
        response: AgentCommandResponse,
        _executor: &mut E,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<CapabilityAuthorizationAuditReceipt, WorldError> {
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
        self.verify_capability_authorization_root()?;
        catalog
            .validate()
            .map_err(|error| deny(format!("catalog validation: {error}")))?;
        response
            .validate()
            .map_err(|error| deny(format!("response validation: {error}")))?;
        let catalog_hash = catalog
            .canonical_hash()
            .map_err(|error| deny(format!("catalog hash: {error}")))?;
        if catalog.snapshot_id != catalog_hash {
            return Err(deny("catalog snapshot id is not its canonical hash"));
        }
        if !response.matches_catalog(&catalog) {
            return Err(deny("response does not match catalog snapshot"));
        }
        self.verify_invocation_context(&grant, &catalog, &response)?;
        if grant.issued_at_tick > self.state.time {
            return Err(deny("grant is not issued at the current logical tick"));
        }
        let request_hash = response
            .canonical_request_hash()
            .map_err(|error| deny(format!("request hash: {error}")))?;
        let nonce_key_hash = super::capability_authorization_events::authorization_nonce_key(
            &grant,
            &response.response_nonce,
        )?;

        if let Some(record) = self.capability_nonce_records.get(&nonce_key_hash) {
            if record.request_hash != request_hash {
                return Err(WorldError::CapabilityNonceConflict {
                    nonce_key_hash,
                    committed_request_hash: record.request_hash.clone(),
                    supplied_request_hash: request_hash,
                });
            }
            if let Some(receipt_id) = &record.committed_receipt_id
                && let Some(receipt) = self.capability_authorization_receipts.get(receipt_id)
            {
                let mut idempotent = receipt.clone();
                idempotent.decision = "idempotent".to_string();
                return Ok(idempotent);
            }
            return Err(deny("committed nonce has no durable receipt"));
        }

        // A committed nonce may have been moved to the authenticated archive.
        // Treat an unavailable archive as an error; never rerun its module call.
        if self.state.retained_history_root.is_some()
            && let Some(retained) = self.read_retained_capability_nonce_record(&nonce_key_hash)?
        {
            if retained.record.request_hash != request_hash {
                return Err(WorldError::CapabilityNonceConflict {
                    nonce_key_hash,
                    committed_request_hash: retained.record.request_hash,
                    supplied_request_hash: request_hash,
                });
            }
            let mut idempotent = retained.receipt;
            idempotent.decision = "idempotent".to_string();
            return Ok(idempotent);
        }

        if grant.status != "verified" {
            return Err(deny("grant is not finalized and verified"));
        }
        if grant.expires_at_tick.is_none() {
            return Err(deny("grant lifetime is not currently valid"));
        }
        if grant
            .expires_at_tick
            .is_some_and(|expiry| self.state.time > expiry)
        {
            return Err(deny("grant expired"));
        }
        if !super::capability_authorization_events::audience_matches(&grant, &catalog.audience)
            || grant.audience != response.audience
            || catalog.audience != response.audience
            || catalog.subject != response.subject
            || grant.subject != response.subject
            || catalog.presenter != response.presenter
        {
            return Err(deny("subject, presenter, or audience mismatch"));
        }
        if response.provider_id.as_deref() != Some(response.presenter.presenter_id.as_str()) {
            return Err(deny("provider identity is not the presented presenter"));
        }
        self.verify_live_capability_audience(&grant, &catalog, &response)?;

        self.verify_issuer(&grant)?;
        self.verify_live_revocation(&grant)?;
        self.verify_parent_chain(&grant)?;

        let manifest = self
            .active_module_manifest(response.selected_entry.module_id.as_str())
            .map_err(|error| deny(format!("active module: {error:?}")))?
            .clone();
        if manifest.version != response.selected_entry.module_version
            || manifest.module_id != grant.scope.module_id
            || manifest.version != grant.scope.module_version
        {
            return Err(deny("module identity or active version mismatch"));
        }
        validate_module_command_declarations(&manifest.abi_contract.declarations)
            .map_err(|error| deny(format!("module declaration: {error}")))?;
        validate_module_command_envelope(&response.envelope, &manifest.abi_contract.declarations)
            .map_err(|error| deny(format!("module envelope: {error}")))?;
        let entry = &response.selected_entry;
        if entry.module_id != manifest.module_id
            || entry.module_version != manifest.version
            || entry.namespace != response.envelope.namespace
            || entry.command != response.envelope.name
            || entry.schema_version != response.envelope.schema_version
            || entry.schema_hash != response.envelope.schema_hash
        {
            return Err(deny("selected declaration or envelope mismatch"));
        }
        let declaration = manifest
            .abi_contract
            .declarations
            .commands
            .iter()
            .find(|decl| {
                decl.namespace == entry.namespace
                    && decl.name == entry.command
                    && decl.schema_version == entry.schema_version
                    && decl.schema_hash == entry.schema_hash
            })
            .ok_or_else(|| deny("selected declaration is not active"))?;
        if entry.max_payload_bytes != declaration.max_payload_bytes {
            return Err(deny(
                "catalog payload bound does not match active declaration",
            ));
        }
        super::capability_authorization_events::validate_subject_for_manifest(
            self,
            &grant.subject,
            &manifest,
        )?;
        if !super::capability_authorization_events::scope_matches_command(
            &grant,
            entry,
            response.envelope.payload.as_slice(),
        ) {
            return Err(deny("grant scope does not exactly authorize command"));
        }
        let catalog_entry = catalog
            .entries
            .iter()
            .find(|candidate| {
                candidate.module_id == entry.module_id
                    && candidate.module_version == entry.module_version
                    && candidate.namespace == entry.namespace
                    && candidate.command == entry.command
                    && candidate.schema_version == entry.schema_version
                    && candidate.schema_hash == entry.schema_hash
                    && candidate.max_payload_bytes == entry.max_payload_bytes
            })
            .ok_or_else(|| deny("selected catalog entry is not present in the live catalog"))?;
        if catalog_entry.eligible_grant_ids.is_empty()
            || !catalog_entry
                .eligible_grant_ids
                .iter()
                .any(|id| id == &grant.grant_id)
        {
            return Err(deny("grant is not eligible for selected catalog entry"));
        }
        self.verify_catalog_freshness(&catalog, &manifest)?;

        let encoded_grant = serde_json::to_value(&grant)?;
        if let Some(existing) = self.capability_grants_v2.get(&grant.grant_id)
            && existing != &encoded_grant
        {
            return Err(deny("immutable grant body changed"));
        }

        let state_hash_before =
            canonical_hash(&self.state).map_err(|error| deny(format!("state hash: {error}")))?;
        let world_head_before = self
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or(0);
        let budget_key = capability_budget_key(&grant.subject, &grant.grant_id)?;
        let budget_before = self
            .capability_budget_accounts
            .get(&budget_key)
            .ok_or_else(|| deny("capability budget account is not available"))?
            .remaining_units;
        let budget_before_spent = self
            .capability_budget_accounts
            .get(&budget_key)
            .ok_or_else(|| deny("capability budget account is not available"))?
            .spent_units;
        let reservation_units =
            budget_reservation_units(response.envelope.payload.len(), &manifest.limits)?;
        if budget_before < reservation_units {
            return Err(deny("capability budget is insufficient before sandbox"));
        }
        let mut budget_account = self
            .capability_budget_accounts
            .get(&budget_key)
            .cloned()
            .ok_or_else(|| deny("capability budget account is not available"))?;
        let mut staged = TrustedCommandStage::new(self)?;
        staged.reserve_capability_budget(&mut budget_account, reservation_units)?;
        let output = self.execute_trusted_module_sandbox(
            &mut staged,
            &manifest,
            &response,
            &grant.subject,
            sandbox,
        )?;
        let effect_intent_ids = staged.effect_intent_ids(world_head_before);
        let budget_after = staged.settle_capability_budget(
            &mut budget_account,
            reservation_units,
            capability_actual_units(response.envelope.payload.len(), &output)?,
        )?;
        self.verify_live_authorization_before_commit(
            &grant,
            &catalog,
            &response,
            &manifest,
            world_head_before,
        )?;
        let state_hash_after = staged.state_hash()?;
        let result_hash =
            canonical_hash(&output).map_err(|error| deny(format!("result hash: {error}")))?;
        let receipt_id = format!("capability-authz-{request_hash}");
        let finality_block_hash = self
            .capability_revocation_state
            .authority_records
            .get(&grant.issuer.issuer_id)
            .map(|record| record.finality_block_hash.clone());
        let receipt = CapabilityAuthorizationAuditReceipt {
            receipt_id: receipt_id.clone(),
            root_receipt_id: None,
            grant_id: Some(grant.grant_id.clone()),
            subject: serde_json::to_value(&grant.subject)?,
            presenter: Some(serde_json::to_value(&response.presenter)?),
            audience: serde_json::to_value(&grant.audience)?,
            scope_hash: capability_scope_hash(&grant.scope)
                .map_err(|error| deny(format!("scope hash: {error}")))?,
            module_id: Some(manifest.module_id.clone()),
            module_version: Some(manifest.version.clone()),
            manifest_hash: Some(
                canonical_hash(&manifest)
                    .map_err(|error| deny(format!("manifest hash: {error}")))?,
            ),
            catalog_snapshot_id: Some(catalog.snapshot_id.clone()),
            response_nonce: Some(response.response_nonce.clone()),
            authorization_nonce_key_hash: Some(nonce_key_hash.clone()),
            decision: "accepted".to_string(),
            denial_code: None,
            budget_before,
            budget_after: Some(budget_after),
            world_head_before,
            // `CommandCommitted` is the next journal event. Capture its
            // allocated id before append_event applies and journals it so the
            // audit receipt's after-head includes the authorization commit,
            // not merely the sandbox/output events that precede it.
            world_head_after: Some(staged.next_event_id()),
            branch_id: grant.audience.branch_id.clone(),
            finality_epoch: grant.audience.finality_epoch,
            finality_block_hash,
            finality_status: "verified".to_string(),
            state_hash_before,
            state_hash_after: Some(state_hash_after.clone()),
            committed_effect_receipt_id: None,
            committed_effect_receipt_ids: BTreeSet::new(),
            canonical_request_hash: request_hash.clone(),
            canonical_result_hash: result_hash.clone(),
        };
        let nonce_record = CapabilityAuthorizationNonceRecord {
            request_hash,
            outcome_hash: result_hash,
            committed_receipt_id: Some(receipt_id.clone()),
            state: "committed".to_string(),
        };
        let mut effect_receipt_links = BTreeMap::new();
        for intent_id in effect_intent_ids {
            effect_receipt_links.insert(
                intent_id,
                CapabilityEffectReceiptLink {
                    authorization_receipt_id: receipt_id.clone(),
                },
            );
        }
        let mut projected_grants_v2 = self.capability_grants_v2.clone();
        projected_grants_v2.insert(grant.grant_id.clone(), serde_json::to_value(&grant)?);
        let mut projected_nonce_records = self.capability_nonce_records.clone();
        projected_nonce_records.insert(nonce_key_hash.clone(), nonce_record.clone());
        let mut projected_receipts = self.capability_authorization_receipts.clone();
        projected_receipts.insert(receipt.receipt_id.clone(), receipt.clone());
        let mut projected_budget_accounts = self.capability_budget_accounts.clone();
        projected_budget_accounts.insert(budget_key.clone(), budget_account.clone());
        let mut projected_effect_receipt_links = self.capability_effect_receipt_links.clone();
        projected_effect_receipt_links.extend(effect_receipt_links.clone());
        self.validate_projected_command_commit(
            &budget_key,
            budget_before,
            budget_before_spent,
            &receipt,
            &budget_account,
            &grant,
            &nonce_key_hash,
            &nonce_record,
            &effect_receipt_links,
            &staged,
        )?;
        let capability_authorization_root = self
            .compute_capability_authorization_root_with_full_projection(
                &projected_grants_v2,
                &self.capability_revocation_state,
                &self.capability_invocation_contexts,
                &projected_budget_accounts,
                &projected_nonce_records,
                &projected_receipts,
                &projected_effect_receipt_links,
            )?;
        staged.append_event(WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted {
                budget_key,
                budget_before_remaining_units: budget_before,
                budget_before_spent_units: budget_before_spent,
                state_hash_before: receipt.state_hash_before.clone(),
                receipt_hash: super::capability_authorization_events::authorization_receipt_hash(
                    &receipt,
                )?,
                budget_account,
                grant,
                nonce_key: nonce_key_hash,
                nonce_record,
                receipt: receipt.clone(),
                effect_receipt_links,
            },
        ))?;
        let consensus_state_root = staged.consensus_state_root_hash()?;
        let prepared = staged.prepare(consensus_state_root)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        prepared
            .with_capability_authorization_projection(
                super::capability_authorization_command_stage::
                    PreparedCapabilityAuthorizationProjection {
                    capability_grants_v2: projected_grants_v2,
                    capability_nonce_records: projected_nonce_records,
                    capability_authorization_receipts: projected_receipts,
                    capability_budget_accounts: projected_budget_accounts,
                    capability_effect_receipt_links: projected_effect_receipt_links,
                    capability_authorization_root,
                },
            )
            .install(self)?;
        Ok(receipt)
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_projected_command_commit(
        &self,
        budget_key: &str,
        budget_before: i64,
        budget_before_spent: i64,
        receipt: &CapabilityAuthorizationAuditReceipt,
        budget_account: &CapabilityBudgetAccount,
        grant: &CapabilityGrantV2,
        nonce_key: &str,
        nonce_record: &CapabilityAuthorizationNonceRecord,
        effect_receipt_links: &BTreeMap<String, CapabilityEffectReceiptLink>,
        staged: &TrustedCommandStage<'_>,
    ) -> Result<(), WorldError> {
        validate_budget_account(budget_account)?;
        if budget_account.reserved_units != 0
            || capability_budget_key(&budget_account.subject, &budget_account.grant_id)?
                != budget_key
            || budget_account.subject != grant.subject
            || budget_account.grant_id != grant.grant_id
            || budget_before < 0
            || budget_before_spent < 0
        {
            return Err(deny("capability command budget binding is invalid"));
        }
        if receipt.budget_before != budget_before
            || receipt.budget_after != Some(budget_account.remaining_units)
            || budget_account.spent_units < budget_before_spent
            || budget_account.remaining_units
                != budget_before.saturating_sub(budget_account.spent_units - budget_before_spent)
        {
            return Err(deny("capability command budget transition is invalid"));
        }
        if let Some(existing) = self.capability_budget_accounts.get(budget_key)
            && existing != budget_account
            && (existing.remaining_units != budget_before
                || existing.reserved_units != 0
                || existing.spent_units != budget_before_spent)
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
        if super::capability_authorization_events::authorization_nonce_key(grant, response_nonce)?
            != nonce_key
        {
            return Err(deny("capability command nonce key is not canonical"));
        }
        let context_key =
            super::capability_authorization_events::capability_invocation_context_key_for_values(
                grant.grant_id.as_str(),
                response_nonce,
            )?;
        let context = self
            .capability_invocation_contexts
            .get(&context_key)
            .or_else(|| self.capability_invocation_contexts.get(&grant.grant_id))
            .ok_or_else(|| deny("capability command invocation context is missing"))?;
        if context.grant_id != grant.grant_id
            || context.subject != grant.subject
            || context.audience != grant.audience
            || context.module_id != grant.scope.module_id
            || context.module_version != grant.scope.module_version
            || context.response_nonce != response_nonce
            || serde_json::to_value(&context.presenter)?
                != receipt.presenter.clone().unwrap_or_default()
            || context.catalog_snapshot_id
                != receipt.catalog_snapshot_id.clone().unwrap_or_default()
        {
            return Err(deny(
                "capability command receipt invocation context binding is invalid",
            ));
        }
        let is_hash =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
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
            || !is_hash(receipt.state_hash_before.as_str())
            || !is_hash(receipt.state_hash_after.as_deref().unwrap_or_default())
            || !is_hash(receipt.canonical_request_hash.as_str())
            || !is_hash(receipt.canonical_result_hash.as_str())
        {
            return Err(deny(
                "capability command receipt journal binding is invalid",
            ));
        }
        let authority = self
            .capability_revocation_state
            .authority_records
            .get(&grant.issuer.issuer_id)
            .ok_or_else(|| deny("capability command receipt issuer authority is missing"))?;
        if receipt.branch_id != authority.branch_id
            || receipt.finality_epoch != authority.finality_epoch
            || receipt.finality_status != "verified"
            || receipt.finality_block_hash.as_deref()
                != Some(authority.finality_block_hash.as_str())
        {
            return Err(deny(
                "capability command receipt finality binding is invalid",
            ));
        }
        let active_manifest = self
            .active_module_manifest(grant.scope.module_id.as_str())
            .map_err(|error| {
                deny(format!(
                    "capability command receipt module is missing: {error:?}"
                ))
            })?;
        let manifest_hash = canonical_hash(&active_manifest)
            .map_err(|error| deny(format!("capability command receipt manifest hash: {error}")))?;
        let state_hash_after = staged.state_hash()?;
        if receipt.manifest_hash.as_deref() != Some(manifest_hash.as_str())
            || receipt.state_hash_after.as_deref() != Some(state_hash_after.as_str())
        {
            return Err(deny(
                "capability command receipt state or manifest hash is invalid",
            ));
        }
        if receipt.world_head_before > staged.current_head()
            || receipt.world_head_after != Some(staged.next_event_id())
            || receipt.world_head_before >= staged.next_event_id()
        {
            return Err(deny(
                "capability command receipt journal head binding is invalid",
            ));
        }
        let encoded_grant = serde_json::to_value(grant)?;
        if let Some(existing) = self.capability_grants_v2.get(&grant.grant_id)
            && existing != &encoded_grant
        {
            return Err(deny("immutable grant body changed"));
        }
        if let Some(existing) = self.capability_nonce_records.get(nonce_key)
            && existing != nonce_record
        {
            return Err(deny("capability nonce journal record changed"));
        }
        if let Some(existing) = self
            .capability_authorization_receipts
            .get(&receipt.receipt_id)
            && existing != receipt
        {
            return Err(deny("capability authorization receipt changed"));
        }
        if let Some(existing) = self.capability_budget_accounts.get(budget_key)
            && (existing.remaining_units < budget_account.remaining_units
                || existing.spent_units > budget_account.spent_units)
        {
            return Err(deny("capability budget journal transition regressed"));
        }
        for (intent_id, link) in effect_receipt_links {
            if intent_id.trim().is_empty() || link.authorization_receipt_id != receipt.receipt_id {
                return Err(deny("capability effect receipt journal binding is invalid"));
            }
            let effect_is_durable = staged
                .pending_effects()
                .iter()
                .any(|intent| intent.identity_key() == *intent_id)
                || self.inflight_effects.contains_key(intent_id);
            if !effect_is_durable {
                return Err(deny(
                    "capability authorization-linked effect is missing from durable queues",
                ));
            }
            if let Some(existing) = self.capability_effect_receipt_links.get(intent_id)
                && existing != link
            {
                return Err(deny("capability effect receipt link changed"));
            }
        }
        Ok(())
    }

    fn verify_invocation_context(
        &self,
        grant: &CapabilityGrantV2,
        catalog: &CapabilityCatalogSnapshot,
        response: &AgentCommandResponse,
    ) -> Result<(), WorldError> {
        let key =
            super::capability_authorization_events::capability_invocation_context_key_for_values(
                grant.grant_id.as_str(),
                response.response_nonce.as_str(),
            )?;
        let hot_context = self
            .capability_invocation_contexts
            .get(&key)
            // Snapshots written before the nonce-key migration used the grant
            // id alone. Keep those records readable, but only when their
            // complete bound nonce still matches this response.
            .or_else(|| self.capability_invocation_contexts.get(&grant.grant_id));
        let retained_context;
        let context = if let Some(context) = hot_context {
            context
        } else if self.state.retained_history_root.is_some() {
            let nonce_key = super::capability_authorization_events::authorization_nonce_key(
                grant,
                &response.response_nonce,
            )?;
            retained_context = self
                .read_retained_capability_invocation_context(&nonce_key)?
                .ok_or_else(|| deny("trusted host invocation context is not bound"))?;
            &retained_context
        } else {
            return Err(deny("trusted host invocation context is not bound"));
        };
        if context.grant_id != grant.grant_id
            || context.subject != grant.subject
            || context.subject != catalog.subject
            || context.subject != response.subject
            || context.presenter != catalog.presenter
            || context.presenter != response.presenter
            || context.audience != grant.audience
            || context.audience != catalog.audience
            || context.audience != response.audience
            || context.catalog_snapshot_id != catalog.snapshot_id
            || context.module_id != response.selected_entry.module_id
            || context.module_version != response.selected_entry.module_version
            || context.response_nonce != response.response_nonce
        {
            return Err(deny("invocation fields do not match trusted host context"));
        }
        Ok(())
    }

    fn verify_live_authorization_before_commit(
        &self,
        grant: &CapabilityGrantV2,
        catalog: &CapabilityCatalogSnapshot,
        response: &AgentCommandResponse,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        world_head_before: u64,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        if self
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or(0)
            < world_head_before
        {
            return Err(deny("staged world head moved backwards"));
        }
        if self.state.time > catalog.valid_until_tick {
            return Err(deny("catalog expired before commit"));
        }
        let catalog_hash = catalog
            .canonical_hash()
            .map_err(|error| deny(format!("catalog hash: {error}")))?;
        if catalog.snapshot_id != catalog_hash || !response.matches_catalog(catalog) {
            return Err(deny("catalog or response changed before commit"));
        }
        let registry_hash = canonical_hash(&self.module_registry)
            .map_err(|error| deny(format!("module registry hash: {error}")))?;
        let policy_hash = canonical_hash(&self.policies)
            .map_err(|error| deny(format!("policy hash: {error}")))?;
        if catalog.module_registry_hash != registry_hash
            || catalog.policy_hash != policy_hash
            || catalog.revocation_epoch != self.capability_revocation_state.epoch
        {
            return Err(deny("live authorization changed before commit"));
        }
        self.verify_live_capability_audience(grant, catalog, response)?;
        if manifest.module_id != response.selected_entry.module_id
            || manifest.version != response.selected_entry.module_version
        {
            return Err(deny("active module changed before commit"));
        }
        let active_manifest = self
            .active_module_manifest(manifest.module_id.as_str())
            .map_err(|error| deny(format!("active module changed before commit: {error:?}")))?;
        if active_manifest != manifest {
            return Err(deny("active module manifest changed before commit"));
        }
        if grant.status != "verified" {
            return Err(deny("grant status changed before commit"));
        }
        self.verify_invocation_context(grant, catalog, response)?;
        self.verify_issuer(grant)?;
        self.verify_live_revocation(grant)?;
        self.verify_parent_chain(grant)
    }

    pub(super) fn verify_catalog_freshness(
        &self,
        catalog: &CapabilityCatalogSnapshot,
        manifest: &oasis7_wasm_abi::ModuleManifest,
    ) -> Result<(), WorldError> {
        let head = self
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or(0);
        if catalog.world_head != head
            || catalog.logical_tick != self.state.time
            || self.state.time > catalog.valid_until_tick
        {
            return Err(deny("catalog is stale"));
        }
        let registry_hash = canonical_hash(&self.module_registry)
            .map_err(|error| deny(format!("module registry hash: {error}")))?;
        let policy_hash = canonical_hash(&self.policies)
            .map_err(|error| deny(format!("policy hash: {error}")))?;
        if catalog.module_registry_hash != registry_hash || catalog.policy_hash != policy_hash {
            return Err(deny("catalog registry or policy hash is stale"));
        }
        if catalog.revocation_epoch != self.capability_revocation_state.epoch {
            return Err(deny("catalog revocation or subject binding is stale"));
        }
        if !manifest
            .abi_contract
            .declarations
            .commands
            .iter()
            .any(|decl| {
                catalog.entries.iter().any(|entry| {
                    decl.namespace == entry.namespace
                        && decl.name == entry.command
                        && decl.schema_version == entry.schema_version
                        && decl.schema_hash == entry.schema_hash
                })
            })
        {
            return Err(deny("catalog declaration is not active"));
        }
        Ok(())
    }

    fn execute_trusted_module_sandbox(
        &self,
        staged: &mut TrustedCommandStage<'_>,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        response: &AgentCommandResponse,
        subject: &oasis7_wasm_abi::CapabilitySubject,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<oasis7_wasm_abi::ModuleOutput, WorldError> {
        // The provider-supplied trace id is observability data, not an
        // authority binding. Use the host-bound response nonce for the
        // durable trusted-command trace so replay can distinguish this
        // command's state update from an unrelated module tail event.
        let trace_id = format!("trusted-command-{}", response.response_nonce);
        let input = response
            .envelope
            .encode_canonical()
            .map_err(|error| deny(format!("command encoding: {error}")))?;
        let state = (manifest.kind == ModuleKind::Reducer)
            .then(|| staged.module_state(manifest.module_id.as_str()));
        let call_input = ModuleCallInput {
            ctx: oasis7_wasm_abi::ModuleContext {
                v: "wasm-1".to_string(),
                module_id: manifest.module_id.clone(),
                trace_id: trace_id.clone(),
                time: self.state.time,
                origin: ModuleCallOrigin {
                    kind: "trusted_module_command".to_string(),
                    id: response.response_nonce.clone(),
                },
                caller: super::capability_authorization_events::module_call_caller(subject),
                limits: manifest.limits.clone(),
                stage: Some("trusted_module_command".to_string()),
                world_config_hash: Some(self.current_manifest_hash()?),
                manifest_hash: Some(super::super::util::hash_json(manifest)?),
                journal_height: Some(self.journal.events.len() as u64),
                module_version: Some(manifest.version.clone()),
                module_kind: Some(format!("{:?}", manifest.kind)),
                module_role: Some(format!("{:?}", manifest.role)),
            },
            event: None,
            action: Some(input),
            state,
        };
        let input_bytes = super::super::util::to_canonical_cbor(&call_input)?;
        self.check_module_runtime_resources(
            &manifest.module_id,
            &trace_id,
            manifest,
            input_bytes.len() as u64,
        )
        .map_err(|failure| deny(format!("runtime upper-bound charge: {}", failure.detail)))?;
        let output = staged
            .call_module_raw(
                &manifest.module_id,
                &trace_id,
                input_bytes.clone(),
                manifest,
                sandbox,
            )
            .map_err(|failure| deny(format!("sandbox: {}", failure.detail)))?;
        if manifest.kind == ModuleKind::Pure && output.new_state.is_some() {
            return Err(deny("pure module returned state in trusted command"));
        }
        if output.tick_lifecycle.is_some() {
            return Err(deny(
                "trusted module commands cannot return tick lifecycle directives",
            ));
        }
        self.validate_module_output_limits(
            &manifest.module_id,
            &manifest.limits,
            output.effects.len(),
            output.emits.len(),
            output.output_bytes,
        )?;
        let mut intents = Vec::with_capacity(output.effects.len());
        for effect in &output.effects {
            let cap_ref = self.resolve_trusted_effect_cap_ref(manifest, effect)?;
            self.verify_trusted_effect_grant(&cap_ref, manifest, response, effect)?;
            let intent = EffectIntent {
                birth_era: staged.next_effect_birth_era(),
                intent_id: format!("intent-{}", staged.allocate_next_intent_seq()),
                kind: effect.kind.clone(),
                params: effect.params.clone(),
                cap_ref,
                origin: EffectOrigin::Module {
                    module_id: manifest.module_id.clone(),
                },
            };
            let decision = self.policies.decide(&intent);
            staged.append_event(WorldEventBody::PolicyDecisionRecorded(
                PolicyDecisionRecord::from_intent(&intent, decision.clone()),
            ))?;
            if !decision.is_allowed() {
                return Err(deny(format!(
                    "trusted effect policy denied {}",
                    decision
                        .reason()
                        .unwrap_or_else(|| "policy_deny".to_string())
                )));
            }
            intents.push(intent);
        }
        staged
            .try_charge_module_runtime(
                &manifest.module_id,
                &trace_id,
                manifest,
                input_bytes.len() as u64,
                &output,
            )
            .map_err(|failure| deny(format!("runtime charge: {}", failure.detail)))?;
        if let Some(state) = &output.new_state {
            staged.append_event(WorldEventBody::ModuleStateUpdated(
                oasis7_wasm_abi::ModuleStateUpdate {
                    module_id: manifest.module_id.clone(),
                    trace_id: trace_id.clone(),
                    state: state.clone(),
                },
            ))?;
        }
        for intent in intents {
            staged.append_event(WorldEventBody::EffectQueued(intent))?;
        }
        for emit in &output.emits {
            staged.append_event(WorldEventBody::ModuleEmitted(
                oasis7_wasm_abi::ModuleEmitEvent {
                    module_id: manifest.module_id.clone(),
                    trace_id: trace_id.clone(),
                    kind: emit.kind.clone(),
                    payload: emit.payload.clone(),
                },
            ))?;
        }
        Ok(output)
    }

    fn resolve_trusted_effect_cap_ref(
        &self,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        effect: &oasis7_wasm_abi::ModuleEffectIntent,
    ) -> Result<String, WorldError> {
        let cap_ref = if let Some(slot) = effect.cap_slot.as_deref() {
            let bound = manifest
                .abi_contract
                .cap_slots
                .get(slot)
                .ok_or_else(|| deny(format!("trusted effect cap slot is not bound: {slot}")))?;
            if !effect.cap_ref.trim().is_empty() && effect.cap_ref != *bound {
                return Err(deny("trusted effect cap slot conflicts with cap_ref"));
            }
            bound.clone()
        } else if effect.cap_ref.trim().is_empty() {
            return Err(deny("trusted effect cap_ref is empty"));
        } else {
            effect.cap_ref.clone()
        };
        if !manifest
            .required_caps
            .iter()
            .any(|required| required == &cap_ref)
        {
            return Err(deny("trusted effect cap_ref is not declared by manifest"));
        }
        Ok(cap_ref)
    }

    fn verify_trusted_effect_grant(
        &self,
        cap_ref: &str,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        response: &AgentCommandResponse,
        effect: &oasis7_wasm_abi::ModuleEffectIntent,
    ) -> Result<(), WorldError> {
        let encoded = self
            .capability_grants_v2
            .get(cap_ref)
            .ok_or_else(|| deny("trusted effect grant is not in the durable registry"))?;
        let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
            .map_err(|_| deny("trusted effect grant is malformed"))?;
        grant
            .validate()
            .map_err(|error| deny(format!("trusted effect grant validation: {error}")))?;
        if !grant
            .body_hash_matches()
            .map_err(|error| deny(format!("trusted effect grant body hash: {error}")))?
            || grant
                .expected_grant_id()
                .map_err(|error| deny(format!("trusted effect grant id hash: {error}")))?
                != grant.grant_id
        {
            return Err(deny("trusted effect grant canonical hash mismatch"));
        }
        if grant.grant_id != cap_ref
            || grant.status != "verified"
            || grant.subject != response.subject
            || grant.audience != response.audience
            || grant.expires_at_tick.is_none()
            || grant
                .expires_at_tick
                .is_some_and(|expiry| self.state.time > expiry)
            || grant.issued_at_tick > self.state.time
            || grant.scope.module_id != manifest.module_id
            || grant.scope.module_version != manifest.version
            || grant.scope.object_kind != "effect"
            || grant.scope.object_name != effect.kind
            || !matches!(grant.scope.operation.as_str(), "invoke" | "execute")
        {
            return Err(deny("trusted effect grant scope does not match output"));
        }
        if !super::capability_authorization_events::scope_selectors_match_json(
            &grant.scope,
            &effect.params,
        ) {
            return Err(deny("trusted effect output is outside the grant selectors"));
        }
        let params_bytes = serde_json::to_vec(&effect.params)?;
        if !grant
            .scope
            .max_payload_bytes
            .is_some_and(|limit| u64::try_from(params_bytes.len()).is_ok_and(|size| size <= limit))
        {
            return Err(deny("trusted effect payload exceeds grant bound"));
        }
        self.verify_issuer(&grant)?;
        self.verify_live_revocation(&grant)?;
        self.verify_parent_chain(&grant)
    }
}
