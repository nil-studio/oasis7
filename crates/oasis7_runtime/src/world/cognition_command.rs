//! Host-governed typed module-command orchestration for Agent cognition.
//!
//! A provider supplies only the typed command response.  The host supplies
//! the grant, live catalog, invocation context and cost quote; this module
//! validates those bindings before handing the command to the existing staged
//! capability executor.  A staged result is published only when it contains
//! an observable module side effect.

use super::super::{WorldError, WorldEventBody};
use super::World;
use oasis7_agent_api::ContinuousAgentTurnContextV1;
use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, CapabilitySubject,
    ModuleSandbox, validate_module_command_declarations, validate_module_command_envelope,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

const COST_QUOTE_SCHEMA: &str = "module-command-cost-quote.v1";

/// Host-issued cost evidence for one typed module command.
///
/// The digest is opaque evidence from the host quote service.  Runtime binds
/// the evidence to the selected command and verifies its shape and lifetime;
/// it never accepts provider-supplied accounting or disposition fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleCommandCostQuoteV1 {
    schema_version: String,
    quote_id: String,
    quote_digest: String,
    units: u64,
    valid_until_tick: u64,
    module_id: String,
    module_version: String,
    schema_hash: String,
}

impl ModuleCommandCostQuoteV1 {
    fn from_json(value: JsonValue) -> Result<Self, &'static str> {
        serde_json::from_value(value).map_err(|_| "runtime_denied")
    }

    fn validate_for(
        &self,
        world: &World,
        response: &AgentCommandResponse,
    ) -> Result<(), &'static str> {
        if self.schema_version != COST_QUOTE_SCHEMA
            || self.quote_id.trim().is_empty()
            || self.module_id != response.selected_entry.module_id
            || self.module_version != response.selected_entry.module_version
            || self.schema_hash != response.selected_entry.schema_hash
        {
            return Err("runtime_denied");
        }
        if self.units == 0
            || self.valid_until_tick < world.state().time
            || !valid_blake3_digest(&self.quote_digest)
            || !valid_hex_digest(&self.schema_hash)
        {
            return Err("stale");
        }
        Ok(())
    }
}

impl World {
    /// Execute a host-bound typed command through the staged capability lane.
    /// Invalid input is represented as a deterministic rejection outcome so a
    /// provider cannot turn validation failures into transport retries.
    pub fn execute_governed_module_command(
        &mut self,
        grant: CapabilityGrantV2,
        catalog: CapabilityCatalogSnapshot,
        response: AgentCommandResponse,
        context: ContinuousAgentTurnContextV1,
        quote: JsonValue,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<JsonValue, WorldError> {
        let quote = match ModuleCommandCostQuoteV1::from_json(quote) {
            Ok(quote) => quote,
            Err(reason) => return Ok(rejected_outcome(reason)),
        };
        if let Some(receipt_id) = self.governed_command_replay(&grant, &response, &context)? {
            return Ok(command_outcome(
                "idempotent",
                Some(receipt_id.as_str()),
                0,
                0,
                0,
                0,
                0,
                0,
            ));
        }
        if let Err(reason) = quote.validate_for(self, &response) {
            return Ok(rejected_outcome(reason));
        }
        if let Err(error) =
            self.validate_governed_command_inputs(&grant, &catalog, &response, &context)
        {
            return Ok(rejected_outcome(classify_rejection(&error)));
        }

        let journal_len = self.journal.events.len();
        let receipt_len = self.capability_authorization_receipts.len();
        let link_len = self.capability_effect_receipt_links.len();
        let mut staged = self.clone();
        let receipt =
            match staged.execute_trusted_module_command(grant, catalog, response, &mut (), sandbox)
            {
                Ok(receipt) => receipt,
                Err(error) => return Ok(rejected_outcome(classify_rejection(&error))),
            };

        if receipt.decision == "idempotent" {
            return Ok(command_outcome(
                "idempotent",
                Some(receipt.receipt_id.as_str()),
                0,
                0,
                0,
                0,
                0,
                0,
            ));
        }

        let new_events = &staged.journal.events[journal_len..];
        let effect_count = new_events
            .iter()
            .filter(|event| matches!(event.body, WorldEventBody::EffectQueued(_)))
            .count() as u64;
        let has_module_output = new_events.iter().any(|event| {
            matches!(
                event.body,
                WorldEventBody::EffectQueued(_)
                    | WorldEventBody::ModuleStateUpdated(_)
                    | WorldEventBody::ModuleEmitted(_)
            )
        });
        if !has_module_output {
            // The sandbox ran only against the staged clone.  Discarding it
            // makes an empty command free: no debit, receipt or journal event
            // becomes visible in the live World.
            return Ok(rejected_outcome("no_effect"));
        }

        let receipt_count = staged
            .capability_authorization_receipts
            .len()
            .saturating_sub(receipt_len) as u64;
        let world_receipt_linked_count = staged
            .capability_effect_receipt_links
            .len()
            .saturating_sub(link_len) as u64;
        let world_event_count = new_events.len() as u64;
        *self = staged;
        Ok(command_outcome(
            "committed",
            Some(receipt.receipt_id.as_str()),
            effect_count,
            receipt_count,
            1,
            world_receipt_linked_count,
            0,
            world_event_count,
        ))
    }

    /// Preview uses the same host bindings and live authorization checks, but
    /// never invokes a sandbox or mutates the World.
    pub fn preview_governed_module_command(
        &mut self,
        grant: CapabilityGrantV2,
        catalog: CapabilityCatalogSnapshot,
        response: AgentCommandResponse,
        context: ContinuousAgentTurnContextV1,
        quote: JsonValue,
    ) -> Result<JsonValue, WorldError> {
        let quote = match ModuleCommandCostQuoteV1::from_json(quote) {
            Ok(quote) => quote,
            Err(reason) => return Ok(rejected_outcome(reason)),
        };
        if let Err(reason) = quote.validate_for(self, &response) {
            return Ok(rejected_outcome(reason));
        }
        if let Err(error) =
            self.validate_governed_command_inputs(&grant, &catalog, &response, &context)
        {
            return Ok(rejected_outcome(classify_rejection(&error)));
        }
        Ok(command_outcome("preview", None, 0, 0, 0, 0, 0, 0))
    }

    fn validate_governed_command_inputs(
        &self,
        grant: &CapabilityGrantV2,
        catalog: &CapabilityCatalogSnapshot,
        response: &AgentCommandResponse,
        context: &ContinuousAgentTurnContextV1,
    ) -> Result<(), WorldError> {
        grant
            .validate()
            .map_err(|error| command_denied(format!("grant validation: {error}")))?;
        if !grant
            .body_hash_matches()
            .map_err(|error| command_denied(format!("grant body hash: {error}")))?
            || grant
                .expected_grant_id()
                .map_err(|error| command_denied(format!("grant id hash: {error}")))?
                != grant.grant_id
        {
            return Err(command_denied("grant canonical body hash or id mismatch"));
        }
        self.verify_capability_authorization_root()?;
        catalog
            .validate()
            .map_err(|error| command_denied(format!("catalog validation: {error}")))?;
        response
            .validate()
            .map_err(|error| command_denied(format!("response validation: {error}")))?;
        let catalog_hash = catalog
            .canonical_hash()
            .map_err(|error| command_denied(format!("catalog hash: {error}")))?;
        if catalog.snapshot_id != catalog_hash || !response.matches_catalog(catalog) {
            return Err(command_denied("catalog or response does not match"));
        }
        context
            .validate_for_agent(&context.agent_id)
            .map_err(|error| command_denied(format!("turn context: {error}")))?;
        if !matches!(
            &grant.subject,
            CapabilitySubject::Agent { agent_id, .. } if agent_id == &context.agent_id
        ) {
            return Err(command_denied(
                "turn context agent is not the granted subject",
            ));
        }
        if !self.capability_invocation_contexts.values().any(|stored| {
            stored.grant_id == grant.grant_id && stored.response_nonce == response.response_nonce
        }) {
            return Err(command_denied("invocation context is not host-bound"));
        }
        if grant.status != "verified"
            || grant.expires_at_tick.is_none()
            || grant
                .expires_at_tick
                .is_some_and(|expiry| self.state.time > expiry)
            || grant.issued_at_tick > self.state.time
        {
            return Err(command_denied(
                "grant lifetime or status is not currently valid",
            ));
        }
        if !super::capability_authorization_events::audience_matches(grant, &catalog.audience)
            || grant.audience != response.audience
            || catalog.audience != response.audience
            || catalog.subject != response.subject
            || grant.subject != response.subject
            || catalog.presenter != response.presenter
        {
            return Err(command_denied("subject, presenter, or audience mismatch"));
        }
        if response.provider_id.as_deref() != Some(response.presenter.presenter_id.as_str()) {
            return Err(command_denied(
                "provider identity is not the presented presenter",
            ));
        }
        self.verify_live_capability_audience(grant, catalog, response)?;
        self.verify_issuer(grant)?;
        self.verify_live_revocation(grant)?;
        self.verify_parent_chain(grant)?;

        let manifest = self
            .active_module_manifest(response.selected_entry.module_id.as_str())?
            .clone();
        if manifest.version != response.selected_entry.module_version
            || manifest.module_id != grant.scope.module_id
            || manifest.version != grant.scope.module_version
        {
            return Err(command_denied("module identity or active version mismatch"));
        }
        validate_module_command_declarations(&manifest.abi_contract.declarations)
            .map_err(|error| command_denied(format!("module declaration: {error}")))?;
        validate_module_command_envelope(&response.envelope, &manifest.abi_contract.declarations)
            .map_err(|error| command_denied(format!("module envelope: {error}")))?;
        let entry = &response.selected_entry;
        if entry.module_id != manifest.module_id
            || entry.module_version != manifest.version
            || entry.namespace != response.envelope.namespace
            || entry.command != response.envelope.name
            || entry.schema_version != response.envelope.schema_version
            || entry.schema_hash != response.envelope.schema_hash
        {
            return Err(command_denied("selected declaration or envelope mismatch"));
        }
        let declaration = manifest
            .abi_contract
            .declarations
            .commands
            .iter()
            .find(|declaration| {
                declaration.namespace == entry.namespace
                    && declaration.name == entry.command
                    && declaration.schema_version == entry.schema_version
                    && declaration.schema_hash == entry.schema_hash
            })
            .ok_or_else(|| command_denied("selected declaration is not active"))?;
        if entry.max_payload_bytes != declaration.max_payload_bytes {
            return Err(command_denied(
                "catalog payload bound does not match declaration",
            ));
        }
        super::capability_authorization_events::validate_subject_for_manifest(
            self,
            &grant.subject,
            &manifest,
        )?;
        if !super::capability_authorization_events::scope_matches_command(
            grant,
            entry,
            response.envelope.payload.as_slice(),
        ) {
            return Err(command_denied("grant scope does not authorize command"));
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
            .ok_or_else(|| command_denied("selected catalog entry is not present"))?;
        if catalog_entry.eligible_grant_ids.is_empty()
            || !catalog_entry
                .eligible_grant_ids
                .iter()
                .any(|id| id == &grant.grant_id)
        {
            return Err(command_denied("grant is not eligible for catalog entry"));
        }
        self.verify_catalog_freshness(catalog, &manifest)?;
        if let Some(existing) = self.capability_grants_v2.get(&grant.grant_id)
            && existing != &serde_json::to_value(grant)?
        {
            return Err(command_denied("immutable grant body changed"));
        }
        Ok(())
    }

    fn governed_command_replay(
        &self,
        grant: &CapabilityGrantV2,
        response: &AgentCommandResponse,
        context: &ContinuousAgentTurnContextV1,
    ) -> Result<Option<String>, WorldError> {
        context
            .validate_for_agent(&context.agent_id)
            .map_err(|error| command_denied(format!("turn context: {error}")))?;
        if !matches!(
            &grant.subject,
            CapabilitySubject::Agent { agent_id, .. } if agent_id == &context.agent_id
        ) {
            return Err(command_denied(
                "turn context agent is not the granted subject",
            ));
        }
        let request_hash = response
            .canonical_request_hash()
            .map_err(|error| command_denied(format!("request hash: {error}")))?;
        let nonce_key = super::capability_authorization_events::authorization_nonce_key(
            grant,
            &response.response_nonce,
        )?;
        let Some(record) = self.capability_nonce_records.get(&nonce_key) else {
            return Ok(None);
        };
        if record.request_hash != request_hash {
            return Err(WorldError::CapabilityNonceConflict {
                nonce_key_hash: nonce_key,
                committed_request_hash: record.request_hash.clone(),
                supplied_request_hash: request_hash,
            });
        }
        let receipt_id = record
            .committed_receipt_id
            .as_ref()
            .filter(|receipt_id| {
                self.capability_authorization_receipts
                    .contains_key(*receipt_id)
            })
            .cloned()
            .ok_or_else(|| command_denied("committed nonce has no durable receipt"))?;
        Ok(Some(receipt_id))
    }
}

fn command_denied(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}

fn classify_rejection(error: &WorldError) -> &'static str {
    let reason = match error {
        WorldError::CapabilityAuthorizationDenied { reason } => reason.as_str(),
        WorldError::CapabilityNonceConflict { .. } => "nonce conflict",
        _ => "runtime denied",
    };
    if reason.contains("stale")
        || reason.contains("expired")
        || reason.contains("catalog")
        || reason.contains("live authorization")
        || reason.contains("changed before")
    {
        "stale"
    } else {
        "runtime_denied"
    }
}

fn valid_blake3_digest(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(valid_hex_digest)
}

fn valid_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn rejected_outcome(reason: &str) -> JsonValue {
    command_outcome("rejected", None, 0, 0, 0, 0, 0, 0)
        .as_object()
        .map(|base| {
            let mut result = base.clone();
            result.insert("reject_reason".to_string(), json!(reason));
            JsonValue::Object(result)
        })
        .unwrap_or_else(|| json!({"disposition": "rejected", "reject_reason": reason}))
}

#[expect(
    clippy::too_many_arguments,
    reason = "The command outcome keeps its explicit stable telemetry fields for wire compatibility."
)]
fn command_outcome(
    disposition: &str,
    receipt_id: Option<&str>,
    effect_count: u64,
    debit_count: u64,
    receipt_count: u64,
    world_receipt_linked_count: u64,
    provider_invocation_count: u64,
    world_event_count: u64,
) -> JsonValue {
    let mut value = json!({
        "disposition": disposition,
        "provider_invocation_count": provider_invocation_count,
        "world_event_count": world_event_count,
        "effect_count": effect_count,
        "debit_count": debit_count,
        "receipt_count": receipt_count,
        "world_receipt_linked_count": world_receipt_linked_count,
    });
    if let Some(receipt_id) = receipt_id {
        value["receipt_id"] = json!(receipt_id);
    }
    value
}
