//! Consensus ingress authentication and governance payload classification.
use std::sync::{Arc, Mutex};

use super::*;

pub(super) fn payload_nonce(payload: &[u8]) -> Result<Option<(String, u64)>, NodeError> {
    let envelope = decode_local_consensus_action_payload_envelope(payload)
        .map_err(|reason| NodeError::Consensus { reason })?;
    validate(&envelope)?;
    let Some(LocalConsensusActionAuthEnvelope::GovernanceAction(proof)) = envelope.auth else {
        return Ok(None);
    };
    let controller = proof
        .get("controller_account_id")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| NodeError::Consensus {
            reason: "governance controller missing".into(),
        })?;
    let nonce = proof
        .get("nonce")
        .and_then(JsonValue::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| NodeError::Consensus {
            reason: "governance nonce missing".into(),
        })?;
    Ok(Some((controller.into(), nonce)))
}

/// Apply the proposal-time governance rule shared by PoS and BFT selectors.
/// Callers must supply actions in canonical action-id order so controller
/// nonce high-water selection is deterministic across runtimes.
pub(crate) fn retain_for_proposal(
    action: &NodeConsensusAction,
    hook: Option<&dyn NodeExecutionHook>,
    highwater: &mut std::collections::BTreeMap<String, u64>,
) -> bool {
    match payload_nonce(&action.payload_cbor) {
        Ok(None) => true,
        Err(_) => false,
        Ok(Some((controller, nonce))) => {
            let authorized = hook.is_some_and(|hook| {
                hook.validate_governance_action_payload(&action.payload_cbor)
                    .is_ok()
            });
            let previous = highwater.entry(controller).or_default();
            if authorized && nonce > *previous {
                *previous = nonce;
                true
            } else {
                false
            }
        }
    }
}

/// Discard only locally decidable stale/malformed input. This deliberately
/// does not invoke an execution hook: failures reading/verifying the current
/// World or storage remain fatal in the V3 batch-validation path.
pub(crate) fn retain_for_bft_v3_parent(
    action: &NodeConsensusAction,
    context: &crate::NodeBftExecutionContextV3,
    highwater: &mut std::collections::BTreeMap<String, u64>,
) -> bool {
    let (controller, nonce) = match payload_nonce(&action.payload_cbor) {
        Ok(None) => return true,
        Err(_) => return false,
        Ok(Some(pair)) => pair,
    };
    let Ok(envelope) = decode_local_consensus_action_payload_envelope(&action.payload_cbor) else {
        return false;
    };
    let Some(LocalConsensusActionAuthEnvelope::GovernanceAction(value)) = envelope.auth else {
        return false;
    };
    let Ok(proof) =
        serde_json::from_value::<oasis7_agent_api::world_action::GovernanceAuthorizationV1>(value)
    else {
        return false;
    };
    if proof.validate().is_err()
        || proof.world_id != context.scope.world_id
        || proof.chain_id != context.scope.chain_id
        || proof.genesis_digest != context.scope.genesis_hash
        || proof.parent_position.checked_add(1) != Some(context.height)
        || proof.parent_record_id != context.parent_commit_ref
        || proof.parent_state_root != context.parent_state_root
        || context.height < proof.valid_from_position
        || context.height > proof.valid_through_position
    {
        return false;
    }
    let previous = highwater.entry(controller).or_default();
    if nonce <= *previous {
        return false;
    }
    *previous = nonce;
    true
}

/// Return true when governance validation fully handles the payload.
pub(super) fn validate(envelope: &LocalConsensusActionPayloadEnvelope) -> Result<bool, NodeError> {
    let governance_proof = match envelope.auth.as_ref() {
        Some(LocalConsensusActionAuthEnvelope::GovernanceAction(proof)) => Some(proof),
        _ => None,
    };
    let LocalConsensusActionPayloadBody::RuntimeAction { action } = &envelope.body else {
        if governance_proof.is_some() {
            return Err(NodeError::Consensus {
                reason: "governance authorization requires a validator admission action".into(),
            });
        }
        return Ok(true);
    };
    let governance_required = matches!(
        local_runtime_action_kind(action),
        Some(
            "SubmitGovernanceValidatorAdmission"
                | "ApproveGovernanceValidatorAdmission"
                | "ActivateGovernanceValidatorAdmission"
                | "RevokeGovernanceValidatorAdmission"
        )
    );
    if governance_required {
        let proof = governance_proof.ok_or_else(|| NodeError::Consensus {
            reason: "missing_governance_auth for validator admission action".into(),
        })?;
        if proof.get("schema_version").and_then(JsonValue::as_u64) != Some(1)
            || proof
                .get("signatures")
                .and_then(JsonValue::as_object)
                .is_none_or(|v| {
                    v.is_empty()
                        || v.values()
                            .any(|value| value.as_str().is_none_or(str::is_empty))
                })
        {
            return Err(NodeError::Consensus {
                reason: "invalid governance authorization structure".into(),
            });
        }
        // World verifies signatures against the committed parent policy, not NodeConfig.
        return Ok(true);
    }
    if governance_proof.is_some() {
        return Err(NodeError::Consensus {
            reason: "governance authorization requires a validator admission action".into(),
        });
    }
    Ok(false)
}

pub(crate) fn validate_consensus_action_payload_auth(
    config: &NodeConfig,
    payload_cbor: &[u8],
) -> Result<(), NodeError> {
    let envelope = decode_local_consensus_action_payload_envelope(payload_cbor).map_err(|err| {
        NodeError::Consensus {
            reason: format!("decode consensus action payload auth envelope failed: {err}"),
        }
    })?;
    if let LocalConsensusActionPayloadBody::WorldGuardedStopV2 { request } = &envelope.body {
        if envelope.version != CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION
            || envelope.auth.is_some()
            || envelope.gameplay_submission_origin.is_some()
        {
            return Err(NodeError::Consensus {
                reason: "invalid guarded Stop V2 catalog envelope".into(),
            });
        }
        request
            .verify_owner_signature()
            .map_err(|_| NodeError::Consensus {
                reason: "invalid guarded Stop V2 Owner signature".into(),
            })?;
        return Ok(());
    }
    if let LocalConsensusActionPayloadBody::WorldCleanupV2 { request } = &envelope.body {
        if envelope.version != CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION
            || envelope.auth.is_some()
            || envelope.gameplay_submission_origin.is_some()
        {
            return Err(NodeError::Consensus {
                reason: "invalid cleanup v2 catalog envelope".into(),
            });
        }
        request
            .verify_owner_signature()
            .map_err(|_| NodeError::Consensus {
                reason: "invalid cleanup v2 Owner signature".into(),
            })?;
        // Canonical Owner authentication does not assert current World authority.
        // Runtime checks it inside its begun committed-tick transaction.
        return Ok(());
    }
    if validate(&envelope)? {
        return Ok(());
    }
    let LocalConsensusActionPayloadBody::RuntimeAction { action } = &envelope.body else {
        return Ok(());
    };
    if !local_main_token_action_auth_required(action) {
        return Ok(());
    }
    let Some(LocalConsensusActionAuthEnvelope::MainTokenAction(proof)) = envelope.auth.as_ref()
    else {
        return Err(NodeError::Consensus {
            reason: format!(
                "missing_main_token_auth for runtime action {}",
                local_runtime_action_kind(action).unwrap_or("unknown")
            ),
        });
    };
    verify_local_main_token_action_auth(action, proof, &config.main_token_controller_binding)
        .map_err(|err| NodeError::Consensus {
            reason: format!("main token action auth rejected: {err}"),
        })?;
    Ok(())
}

/// Structural admission checks alone never establish authority to enter consensus.
pub(super) fn preflight(runtime: &NodeRuntime, payload_cbor: &[u8]) -> Result<(), NodeError> {
    preflight_with_hook(&runtime.execution_hook, payload_cbor)
}

pub(super) fn preflight_with_hook(
    execution_hook: &Option<Arc<Mutex<Box<dyn NodeExecutionHook>>>>,
    payload_cbor: &[u8],
) -> Result<(), NodeError> {
    if !governance_preflight_required(payload_cbor)? {
        return Ok(());
    }
    if super::is_closed_task_catalog(payload_cbor) {
        let hook = execution_hook
            .as_ref()
            .ok_or_else(|| NodeError::Consensus {
                reason: "closed task authorization requires an execution preflight hook".into(),
            })?;
        let hook = hook.try_lock().map_err(|error| NodeError::Consensus {
            reason: match error {
                std::sync::TryLockError::WouldBlock => "closed task preflight busy; not queued",
                std::sync::TryLockError::Poisoned(_) => {
                    "closed task preflight hook poisoned; not queued"
                }
            }
            .into(),
        })?;
        return validate_governance_preflight(&**hook, payload_cbor);
    }
    let hook = execution_hook
        .as_ref()
        .map(|hook| {
            hook.lock().map_err(|_| NodeError::Consensus {
                reason: "governance authorization preflight hook lock poisoned".into(),
            })
        })
        .transpose()?
        .ok_or_else(|| NodeError::Consensus {
            reason: "governance authorization requires an execution preflight hook".into(),
        })?;
    validate_governance_preflight(&**hook, payload_cbor)
}

pub(crate) fn preflight_with_execution_hook(
    hook: &dyn NodeExecutionHook,
    payload_cbor: &[u8],
) -> Result<(), NodeError> {
    if !governance_preflight_required(payload_cbor)? {
        return Ok(());
    }
    validate_governance_preflight(hook, payload_cbor)
}

fn governance_preflight_required(payload_cbor: &[u8]) -> Result<bool, NodeError> {
    let envelope = decode_local_consensus_action_payload_envelope(payload_cbor)
        .map_err(|reason| NodeError::Consensus { reason })?;
    Ok(matches!(
        envelope.auth,
        Some(LocalConsensusActionAuthEnvelope::GovernanceAction(_))
    ) || matches!(
        envelope.body,
        LocalConsensusActionPayloadBody::WorldCleanupV2 { .. }
            | LocalConsensusActionPayloadBody::WorldGuardedStopV2 { .. }
    ))
}

fn validate_governance_preflight(
    hook: &dyn NodeExecutionHook,
    payload_cbor: &[u8],
) -> Result<(), NodeError> {
    let envelope = decode_local_consensus_action_payload_envelope(payload_cbor)
        .map_err(|reason| NodeError::Consensus { reason })?;
    if let LocalConsensusActionPayloadBody::WorldGuardedStopV2 { request } = &envelope.body {
        return hook
            .validate_guarded_stop_v2_action_payload(request)
            .map_err(|reason| NodeError::Consensus {
                reason: format!("guarded Stop V2 profile preflight rejected: {reason}"),
            });
    }
    if let LocalConsensusActionPayloadBody::WorldCleanupV2 { request } = &envelope.body {
        return hook
            .validate_cleanup_v2_action_payload(request)
            .map_err(|reason| NodeError::Consensus {
                reason: format!("cleanup V2 profile preflight rejected: {reason}"),
            });
    }
    hook.validate_governance_action_payload(payload_cbor)
        .map_err(|reason| NodeError::Consensus {
            reason: format!("governance authorization preflight rejected: {reason}"),
        })
}
