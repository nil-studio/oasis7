use oasis7::consensus_action_payload::{
    ConsensusActionPayloadBody, decode_consensus_action_payload_envelope,
};
use oasis7_node::{
    NodeExecutionCommitContext, decode_replicated_execution_input_action,
    validate_replicated_execution_input_actions,
};
use oasis7_runtime::Action as RuntimeAction;
use oasis7_runtime::ProviderBackedBootstrapAuthorityV1;
use oasis7_simulator::{Action as SimulatorAction, ActionSubmitter};

#[expect(
    clippy::type_complexity,
    reason = "Stable callback and channel topology remains explicit at this integration seam."
)]
pub(super) fn decode_committed_actions(
    context: &NodeExecutionCommitContext,
) -> Result<
    (
        Vec<(RuntimeAction, Option<oasis7_runtime::CommittedRecipeOrigin>)>,
        Vec<(SimulatorAction, ActionSubmitter)>,
        Option<Vec<ProviderBackedBootstrapAuthorityV1>>,
        Vec<(
            u64,
            oasis7::world_service::SubmitIntentRequest<
                oasis7::world_service::WorldServicePayloadV1,
            >,
        )>,
    ),
    String,
> {
    validate_replicated_execution_input_actions(
        context.committed_actions.as_slice(),
        context.height,
    )
    .map_err(|err| {
        format!(
            "execution driver replicated execution input validation failed at height {}: {}",
            context.height, err
        )
    })?;

    let mut decoded_runtime_actions = Vec::with_capacity(context.committed_actions.len());
    let mut decoded_simulator_actions = Vec::with_capacity(context.committed_actions.len());
    let mut replicated_provider_backed_bootstrap = None;
    let mut service_intents = Vec::new();
    for action in &context.committed_actions {
        if let Some(input) = decode_replicated_execution_input_action(action).map_err(|err| {
            format!(
                "execution driver decode replicated execution input failed action_id={} err={err}",
                action.action_id
            )
        })? {
            if input.target_height != context.height {
                return Err(format!(
                    "execution driver replicated execution input height mismatch: action_height={} context_height={}",
                    input.target_height, context.height
                ));
            }
            if replicated_provider_backed_bootstrap.is_some() {
                return Err(format!(
                    "execution driver received duplicate replicated ProviderBacked bootstrap inputs at height {}",
                    context.height
                ));
            }
            replicated_provider_backed_bootstrap = Some(
                super::provider_bootstrap::decode_provider_backed_bootstrap_execution_input(
                    &input,
                )?,
            );
            continue;
        }
        match decode_consensus_action_payload_envelope(action.payload_cbor.as_slice()) {
            Ok(envelope) => match envelope.body {
                ConsensusActionPayloadBody::WorldServiceIntent { request } => {
                    service_intents.push((action.action_id, request));
                }
                ConsensusActionPayloadBody::RuntimeAction { action: decoded } => {
                    if let RuntimeAction::WorldServiceIntent { request } = decoded {
                        let request = serde_json::from_value(request).map_err(|error| {
                            format!("invalid registered world-service intent: {error}")
                        })?;
                        service_intents.push((action.action_id, request));
                    } else {
                        let origin = envelope.gameplay_submission_origin.map(|submission| {
                            oasis7_runtime::CommittedRecipeOrigin {
                                submission,
                                consensus_action_id: action.action_id,
                                consensus_submitter_player_id: action.submitter_player_id.clone(),
                                action_payload_hash: action.payload_hash.clone(),
                                committed_height: context.height,
                                action_root: context.action_root.clone(),
                            }
                        });
                        decoded_runtime_actions.push((decoded, origin));
                    }
                }
                ConsensusActionPayloadBody::SimulatorAction { action, submitter } => {
                    decoded_simulator_actions.push((action, submitter));
                }
            },
            Err(err) => {
                return Err(format!(
                    "execution driver decode committed action failed action_id={} err={}",
                    action.action_id, err
                ));
            }
        }
    }

    Ok((
        decoded_runtime_actions,
        decoded_simulator_actions,
        replicated_provider_backed_bootstrap,
        service_intents,
    ))
}
