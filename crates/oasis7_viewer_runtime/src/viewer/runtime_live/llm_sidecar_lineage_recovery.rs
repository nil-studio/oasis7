use super::lineage_persistence::ProviderTerminalState;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn wake_identity_without_digest_matches(
    sidecar: &crate::runtime::SchedulerWakeV1,
    runtime: &crate::runtime::SchedulerWakeV1,
) -> bool {
    sidecar.wake_id == runtime.wake_id
        && sidecar.continuation_id == runtime.continuation_id
        && sidecar.world_id == runtime.world_id
        && sidecar.branch_id == runtime.branch_id
        && sidecar.finality_epoch == runtime.finality_epoch
        && sidecar.finality_block_hash == runtime.finality_block_hash
        && sidecar.finality_status == runtime.finality_status
        && sidecar.reorg_epoch == runtime.reorg_epoch
        && sidecar.runtime_manifest_hash == runtime.runtime_manifest_hash
        && sidecar.agent_id == runtime.agent_id
        && sidecar.agent_session_id == runtime.agent_session_id
        && sidecar.agent_turn_id == runtime.agent_turn_id
        && sidecar.decision_request_id == runtime.decision_request_id
}

fn wake_matches_terminal_identity(
    wake: &crate::runtime::SchedulerWakeV1,
    terminal: &ProviderTerminalState,
) -> bool {
    !terminal.agent_id.is_empty()
        && !terminal.agent_session_id.is_empty()
        && !terminal.agent_turn_id.is_empty()
        && !terminal.decision_request_id.is_empty()
        && !terminal.request_digest.is_empty()
        && wake.agent_id == terminal.agent_id
        && wake.agent_session_id == terminal.agent_session_id
        && wake.agent_turn_id == terminal.agent_turn_id
        && wake.decision_request_id == terminal.decision_request_id
}

/// Normalize a legacy sidecar wake's missing request digest from a Runtime
/// in-flight wake or a complete terminal marker. Both authorities are
/// compared on every persisted identity field before hydration; an explicit
/// digest disagreement is a checkpoint conflict and must fail closed.
pub(super) fn hydrate_pending_runtime_wake_identities(
    pending_runtime_wakes: &mut BTreeMap<String, crate::runtime::SchedulerWakeV1>,
    runtime_wakes: &[crate::runtime::SchedulerWakeV1],
    terminal_states: &BTreeMap<String, ProviderTerminalState>,
) -> Result<bool, String> {
    let mut migrated = false;
    for wake in pending_runtime_wakes.values_mut() {
        if let Some(runtime_wake) = runtime_wakes
            .iter()
            .find(|runtime_wake| runtime_wake.wake_id == wake.wake_id)
        {
            if !wake_identity_without_digest_matches(wake, runtime_wake) {
                return Err(format!(
                    "pending Runtime wake identity mismatch for {}",
                    wake.wake_id
                ));
            }
            if !runtime_wake.request_digest.is_empty() {
                if wake.request_digest.is_empty() {
                    *wake = runtime_wake.clone();
                    migrated = true;
                } else if wake.request_digest != runtime_wake.request_digest {
                    return Err(format!(
                        "pending Runtime wake request_digest conflict for {}",
                        wake.wake_id
                    ));
                }
                continue;
            }
        }

        let terminal = terminal_states
            .values()
            .find(|terminal| wake_matches_terminal_identity(wake, terminal));
        let Some(terminal) = terminal else {
            continue;
        };
        if wake.request_digest.is_empty() {
            wake.request_digest = terminal.request_digest.clone();
            migrated = true;
        } else if wake.request_digest != terminal.request_digest {
            return Err(format!(
                "pending Runtime wake terminal identity conflict for {}",
                wake.wake_id
            ));
        }
    }
    Ok(migrated)
}

/// V1 checkpoints predate the explicit provider/tool budget limits. A
/// missing limit is an explicit zero deny after migration; treating it as an
/// unlimited/default budget could spend credits that the checkpoint cannot
/// account for. Only nested budget objects are changed, preserving all saved
/// session, turn, request, and recovery identities.
pub(super) fn migrate_legacy_budget_contracts(value: &mut Value) -> Result<(), String> {
    fn visit(value: &mut Value, migrated: &mut usize) {
        match value {
            Value::Object(fields) => {
                if let Some(Value::Object(budget)) = fields.get_mut("budget_contract") {
                    if !budget.contains_key("max_model_calls") {
                        budget.insert("max_model_calls".to_string(), json!(0));
                        *migrated = migrated.saturating_add(1);
                    }
                    if !budget.contains_key("max_tool_calls") {
                        budget.insert("max_tool_calls".to_string(), json!(0));
                        *migrated = migrated.saturating_add(1);
                    }
                }
                for child in fields.values_mut() {
                    visit(child, migrated);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, migrated);
                }
            }
            _ => {}
        }
    }

    let mut migrated = 0;
    visit(value, &mut migrated);
    // An empty V1 checkpoint is valid and needs only a schema bump. Any
    // non-empty budget object still receives explicit zero-deny limits above;
    // malformed objects fail closed during the typed decode below.
    Ok(())
}
