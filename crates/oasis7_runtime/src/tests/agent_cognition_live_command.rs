//! LIVE-6 RED contract for the governed typed ModuleCommand lane.
//!
//! The existing v2 authorization tests prove individual catalog, grant,
//! nonce, and staged-sandbox checks.  They do not prove that one provider turn
//! is assembled by the host and then reaches a real `World` through the
//! governed command adapter with a semantic cost quote and a final live
//! recheck.  This integration contract intentionally calls the missing World
//! orchestration seams so the RED failure identifies the implementation gap.

use super::super::*;
use super::capability_grant_v2::{
    ConfiguredSandbox, catalog_json, fixture_world, fixture_world_with_revocations,
    fixture_world_with_revocations_and_budget_and_effect_grant, grant_json,
    install_invocation_context, prepared_invocation, signed_effect_grant, signed_grant,
};
use oasis7_agent_api::{ContinuousAgentTurnContextV1, GoalSnapshotV1, MemoryContextSnapshotV1};
use oasis7_agent_api::Digest32;
use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, ModuleEffectIntent,
    ModuleOutput, ModuleSandbox,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MODULE_ID: &str = "module.weather";
const MODULE_VERSION: &str = "1.0.0";
const SCHEMA_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn prepared_world(
    mut world: World,
) -> (
    World,
    CapabilityGrantV2,
    CapabilityCatalogSnapshot,
    AgentCommandResponse,
) {
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        super::capability_grant_v2::response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    (world, grant, catalog, response)
}

fn cost_quote(nonce: &str) -> Value {
    json!({
        "schema_version": "module-command-cost-quote.v1",
        "quote_id": format!("quote.{nonce}"),
        "quote_digest": format!("blake3:{:0>64}", nonce.len() + 1),
        "units": 1,
        "valid_until_tick": 100,
        "module_id": MODULE_ID,
        "module_version": MODULE_VERSION,
        "schema_hash": SCHEMA_HASH
    })
}

fn no_effect_sandbox() -> ConfiguredSandbox {
    ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        },
    }
}

fn turn_context(nonce: &str) -> ContinuousAgentTurnContextV1 {
    ContinuousAgentTurnContextV1 {
        agent_id: "agent-7".to_string(),
        agent_session_id: "session.live-6".to_string(),
        agent_turn_id: format!("turn.{nonce}"),
        decision_request_id: format!("request.{nonce}"),
        request_digest: Digest32::from(format!("blake3:{}", "a".repeat(64))),
        memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    }
}

fn execute(
    world: &mut World,
    grant: CapabilityGrantV2,
    catalog: CapabilityCatalogSnapshot,
    response: AgentCommandResponse,
    context: ContinuousAgentTurnContextV1,
    quote: Value,
    sandbox: &mut dyn ModuleSandbox,
) -> Value {
    // Target API: Runtime receives host-bound typed response, capability
    // context, and quote; provider output cannot choose any of those fields.
    world
        .execute_governed_module_command(grant, catalog, response, context, quote, sandbox)
        .expect("governed ModuleCommand outcome")
}

fn preview(
    world: &mut World,
    grant: CapabilityGrantV2,
    catalog: CapabilityCatalogSnapshot,
    response: AgentCommandResponse,
    context: ContinuousAgentTurnContextV1,
    quote: Value,
) -> Value {
    // Preview has a separate host-controlled entry point.  A provider cannot
    // set a `mode` or `fixture` field to turn an execution into preview.
    world
        .preview_governed_module_command(grant, catalog, response, context, quote)
        .expect("governed ModuleCommand preview")
}

fn assert_zero_side_effects(outcome: &Value) {
    assert_eq!(outcome["provider_invocation_count"], 0);
    assert_eq!(outcome["effect_count"], 0);
    assert_eq!(outcome["debit_count"], 0);
    assert_eq!(outcome["receipt_count"], 0);
    assert_eq!(outcome["world_receipt_linked_count"], 0);
}

fn assert_tamper_rejected<F>(label: &str, mutate: F)
where
    F: FnOnce(
        &mut CapabilityGrantV2,
        &mut CapabilityCatalogSnapshot,
        &mut AgentCommandResponse,
        &mut ContinuousAgentTurnContextV1,
        &mut Value,
    ),
{
    let (mut world, mut grant, mut catalog, mut response) = prepared_world(fixture_world());
    let mut context = turn_context("response-1");
    let mut quote = cost_quote("response-1");
    mutate(
        &mut grant,
        &mut catalog,
        &mut response,
        &mut context,
        &mut quote,
    );
    let journal_events_before = world.journal().events.len();
    let mut sandbox = no_effect_sandbox();
    let outcome = execute(
        &mut world,
        grant,
        catalog,
        response,
        context,
        quote,
        &mut sandbox,
    );
    assert_eq!(outcome["disposition"], "rejected", "tamper={label}");
    assert_zero_side_effects(&outcome);
    assert_eq!(sandbox.calls, 0, "tamper={label} must fail before sandbox");
    assert_eq!(
        world.journal().events.len(),
        journal_events_before,
        "tamper={label} must not append a journal record"
    );
}

#[test]
fn governed_typed_command_binds_turn_context_quote_and_live_recheck() {
    let (world, grant, catalog, response) = prepared_world(fixture_world());
    let outcome = preview(
        &mut world.clone(),
        grant,
        catalog,
        response,
        turn_context("response-1"),
        cost_quote("response-1"),
    );
    assert_eq!(outcome["disposition"], "preview");
    assert_zero_side_effects(&outcome);

    // Any live-head, catalog, or quote drift invalidates the old intent.  The
    // command must fail before sandbox, metering, journal, or partial state.
    for drift in ["live_head", "catalog", "quote"] {
        let (mut stale_world, stale_grant, mut stale_catalog, stale_response) =
            prepared_world(fixture_world());
        let mut stale_quote = cost_quote("response-1");
        match drift {
            "live_head" => stale_world
                .step()
                .expect("advance live head after discovery"),
            "catalog" => {
                stale_catalog.world_head = stale_catalog.world_head.saturating_add(1);
                stale_catalog.snapshot_id = stale_catalog
                    .canonical_hash()
                    .expect("re-hash drifted catalog");
            }
            "quote" => stale_quote["quote_digest"] = json!("blake3:stale-quote"),
            _ => unreachable!("bounded stale drift fixture"),
        }
        let mut stale_sandbox = no_effect_sandbox();
        let stale = execute(
            &mut stale_world,
            stale_grant,
            stale_catalog,
            stale_response,
            turn_context("response-1"),
            stale_quote,
            &mut stale_sandbox,
        );
        assert_eq!(stale["disposition"], "rejected", "drift={drift}");
        assert_eq!(stale["reject_reason"], "stale", "drift={drift}");
        assert_zero_side_effects(&stale);
        assert_eq!(stale_sandbox.calls, 0, "drift={drift}");
    }

    // Revocation is a live authority change, not a provider-selected denial.
    let denied_grant = signed_grant(grant_json(json!({})));
    let revoked = BTreeSet::from([denied_grant.grant_id.clone()]);
    let (mut denied_world, _, denied_catalog, denied_response) =
        prepared_world(fixture_world_with_revocations(revoked));
    let denied = execute(
        &mut denied_world,
        denied_grant,
        denied_catalog,
        denied_response,
        turn_context("response-1"),
        cost_quote("response-1"),
        &mut no_effect_sandbox(),
    );
    assert_eq!(denied["disposition"], "rejected");
    assert_eq!(denied["reject_reason"], "runtime_denied");
    assert_zero_side_effects(&denied);
}

#[test]
fn governed_typed_command_no_effect_is_free_and_effect_is_debited_once_on_retry() {
    let (mut no_effect_world, no_effect_grant, no_effect_catalog, no_effect_response) =
        prepared_world(fixture_world());
    let no_effect = execute(
        &mut no_effect_world,
        no_effect_grant,
        no_effect_catalog,
        no_effect_response,
        turn_context("response-1"),
        cost_quote("response-1"),
        &mut no_effect_sandbox(),
    );
    assert_eq!(no_effect["disposition"], "rejected");
    assert_eq!(no_effect["reject_reason"], "no_effect");
    assert_zero_side_effects(&no_effect);

    let effect_grant = signed_effect_grant();
    // The command capability and the output-effect capability are intentionally
    // distinct governed grants.  The former binds the typed command turn;
    // the latter is the exact effect cap_ref installed in this fixture.
    let (mut world, grant, catalog, response) =
        prepared_world(fixture_world_with_revocations_and_budget_and_effect_grant(
            BTreeSet::new(),
            128,
            effect_grant.clone(),
        ));
    let mut effect_sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: None,
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".to_string(),
                params: json!({"station": "station-1"}),
                cap_ref: effect_grant.grant_id,
                cap_slot: None,
            }],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        },
    };
    let first = execute(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        turn_context("response-1"),
        cost_quote("response-1"),
        &mut effect_sandbox,
    );
    assert_eq!(first["disposition"], "committed");
    assert_eq!(first["provider_invocation_count"], 0);
    assert_eq!(first["effect_count"], 1);
    assert_eq!(first["debit_count"], 1);
    assert_eq!(first["receipt_count"], 1);
    assert_eq!(first["world_receipt_linked_count"], 1);
    assert!(first["receipt_id"].is_string());

    let retry = execute(
        &mut world,
        grant,
        catalog,
        response,
        turn_context("response-1"),
        cost_quote("response-1"),
        &mut effect_sandbox,
    );
    assert_eq!(retry["disposition"], "idempotent");
    assert_eq!(retry["receipt_id"], first["receipt_id"]);
    assert_zero_side_effects(&retry);
    assert_eq!(effect_sandbox.calls, 1);
}

#[test]
fn governed_typed_command_rejects_binding_and_provider_accounting_tamper_before_effects() {
    // These are host-bound fields.  A provider may not rewrite identity,
    // catalog/schema/hash/nonce, or smuggle an outcome/accounting decision
    // into its candidate payload.  Every case must stop before sandbox,
    // metering, journal, receipt, or world-effect work.
    assert_tamper_rejected("subject", |_, _, response, _, _| {
        response.subject = oasis7_wasm_abi::CapabilitySubject::Agent {
            agent_id: "agent-forged".to_string(),
            owner_binding: "owner-forged".to_string(),
            generation: 1,
        };
    });
    assert_tamper_rejected("presenter", |_, _, response, _, _| {
        response.presenter.presenter_id = "provider-forged".to_string();
        response.provider_id = Some("provider-forged".to_string());
    });
    assert_tamper_rejected("audience", |_, _, response, _, _| {
        response.audience.world_id = "world-forged".to_string();
    });
    assert_tamper_rejected("catalog", |_, catalog, _, _, _| {
        catalog.policy_hash = "policy-forged".to_string();
    });
    assert_tamper_rejected("schema", |_, catalog, _response, _, _| {
        catalog.entries[0].schema_version = 2;
    });
    assert_tamper_rejected("hash", |_, catalog, response, _, _| {
        catalog.entries[0].schema_hash = "f".repeat(64);
        response.selected_entry.schema_hash = "f".repeat(64);
        response.envelope.schema_hash = "f".repeat(64);
    });
    assert_tamper_rejected("nonce", |_, _, response, _, _| {
        response.response_nonce = "nonce-forged".to_string();
    });
    assert_tamper_rejected(
        "forged-disposition-receipt-debit-effect",
        |_, _, _, _, quote| {
            quote["disposition"] = json!("committed");
            quote["receipt_id"] = json!("receipt-forged");
            quote["debit_count"] = json!(99);
            quote["effect_count"] = json!(99);
        },
    );
}
