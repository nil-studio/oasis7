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

pub(super) fn validate_consensus_action_payload_auth(
    config: &NodeConfig,
    payload_cbor: &[u8],
) -> Result<(), NodeError> {
    let envelope = decode_local_consensus_action_payload_envelope(payload_cbor).map_err(|err| {
        NodeError::Consensus {
            reason: format!("decode consensus action payload auth envelope failed: {err}"),
        }
    })?;
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
    let envelope = decode_local_consensus_action_payload_envelope(payload_cbor)
        .map_err(|reason| NodeError::Consensus { reason })?;
    if !matches!(
        envelope.auth,
        Some(LocalConsensusActionAuthEnvelope::GovernanceAction(_))
    ) {
        return Ok(());
    }
    let hook = execution_hook
        .as_ref()
        .ok_or_else(|| NodeError::Consensus {
            reason: "governance authorization requires an execution preflight hook".into(),
        })?;
    let hook = hook.lock().map_err(|_| NodeError::Consensus {
        reason: "governance authorization preflight hook lock poisoned".into(),
    })?;
    hook.validate_governance_action_payload(payload_cbor)
        .map_err(|reason| NodeError::Consensus {
            reason: format!("governance authorization preflight rejected: {reason}"),
        })
}
