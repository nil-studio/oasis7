use oasis7::{
    runtime::{CausedBy, DomainEvent, World, WorldEventBody},
    world_service::*,
};
use oasis7_node::NodeExecutionCommitContext;

pub(super) struct StagedResult {
    result: CanonicalIntentResultV1,
    runtime_action_id: Option<u64>,
}

pub(super) fn apply_intents(
    world: &mut World,
    context: &NodeExecutionCommitContext,
    expected_world: Option<&WorldIdentity>,
    intents: Vec<(u64, SubmitIntentRequest<WorldServicePayloadV1>)>,
) -> Result<Vec<StagedResult>, String> {
    let mut staged = Vec::new();
    for (action_id, request) in intents {
        request.validate().map_err(|e| e.to_string())?;
        let correlation = derive_correlation(
            request.correlation.key.world.clone(),
            &request.signed_payload,
        )?;
        if request.correlation != correlation
            || correlation.key.world.world_id != context.world_id
            || expected_world != Some(&correlation.key.world)
        {
            return Err("canonical service correlation/world mismatch".into());
        }
        let key = correlation::key_digest(&correlation.key)?;
        if let Some(existing) = world
            .capability_revocation_state()
            .world_service_results
            .get(&key)
        {
            let existing: CanonicalIntentResultV1 =
                serde_json::from_value(existing.clone()).map_err(|e| e.to_string())?;
            if existing.request.correlation != correlation {
                return Err("canonical service idempotency conflict".into());
            }
            continue;
        }
        if world.state().retained_history_root.is_some()
            && let Some(existing) = world
                .read_retained_world_service_result(&key)
                .map_err(|error| error.to_string())?
        {
            if existing.request.correlation != correlation {
                return Err("retained canonical service idempotency conflict".into());
            }
            // Authenticated archival presence retains the first execution.
            // Missing CAS is an error above, never a license to repeat it.
            continue;
        }
        // Within-height duplicates also resolve to one canonical execution.
        if let Some(existing) = staged
            .iter()
            .find(|entry: &&StagedResult| entry.result.request.correlation.key == correlation.key)
        {
            if existing.result.request.correlation != correlation {
                return Err("within-block service idempotency conflict".into());
            }
            continue;
        }
        if matches!(&request.signed_payload, WorldServicePayloadV1::GameplayJson(bytes)
            if gameplay::consumed_collect_data_nonce(world, bytes)?)
        {
            // A legacy commit may win after HTTP admission. Do not turn its
            // consumed nonce into a new rejection under the original key.
            // Without a correlated record, Lookup truthfully remains Unknown.
            continue;
        }
        let mut candidate = world.clone();
        let mut runtime_action_id = None;
        let mut rejected_cognition = None;
        let applied = match request.signed_payload.clone() {
            WorldServicePayloadV1::GameExecutorControl(_) => {
                oasis7::world_service::game_task::prepare_executor_control(
                    &candidate,
                    &request,
                    staged.iter().map(|entry: &StagedResult| &entry.result),
                )
                .and_then(|receipt| {
                    let WorldServicePayloadV1::GameExecutorControl(signed) =
                        &request.signed_payload
                    else {
                        unreachable!()
                    };
                    oasis7::world_service::game_task::retire_task_contexts(
                        &mut candidate,
                        &signed.request.fence.agent_id,
                        &staged
                            .iter()
                            .map(|entry: &StagedResult| entry.result.clone())
                            .collect::<Vec<_>>(),
                    )?;
                    Ok(receipt)
                })
            }
            WorldServicePayloadV1::GameGoalPublish(_) => {
                oasis7::world_service::game_task::prepare_publish(
                    &candidate,
                    &request,
                    staged.iter().map(|entry: &StagedResult| &entry.result),
                    context.height,
                )
                .and_then(|receipt| {
                    let WorldServicePayloadV1::GameGoalPublish(signed) = &request.signed_payload
                    else {
                        unreachable!()
                    };
                    oasis7::world_service::game_task::retire_task_contexts(
                        &mut candidate,
                        &signed.request.agent_id,
                        &staged
                            .iter()
                            .map(|entry: &StagedResult| entry.result.clone())
                            .collect::<Vec<_>>(),
                    )?;
                    Ok(receipt)
                })
            }
            WorldServicePayloadV1::GameTaskCreate(_) => {
                oasis7::world_service::game_task::prepare_create(
                    &candidate,
                    &request,
                    staged.iter().map(|entry: &StagedResult| &entry.result),
                )
            }
            WorldServicePayloadV1::GameplayJson(bytes) => {
                gameplay::authenticated_action_with_origin(&candidate, &bytes).and_then(
                    |(action, submission)| {
                        let id = if let Some(submission) = submission {
                            let committed = context
                                .committed_actions
                                .iter()
                                .find(|committed| committed.action_id == action_id)
                                .ok_or("service recipe consensus action missing")?;
                            let origin = oasis7_runtime::CommittedRecipeOrigin {
                                submission,
                                consensus_action_id: committed.action_id,
                                consensus_submitter_player_id: committed
                                    .submitter_player_id
                                    .clone(),
                                action_payload_hash: committed.payload_hash.clone(),
                                committed_height: context.height,
                                action_root: context.action_root.clone(),
                            };
                            candidate
                                .submit_recipe_action_with_origin(action, origin)
                                .map_err(|error| {
                                    format!(
                                        "submit service committed recipe origin failed: {error:?}"
                                    )
                                })?
                        } else {
                            candidate.submit_action(action)
                        };
                        runtime_action_id = Some(id);
                        let committed = context
                            .committed_actions
                            .iter()
                            .find(|committed| committed.action_id == action_id)
                            .ok_or("service gameplay consensus action missing")?;
                        Ok(
                            serde_json::json!({"action_id": action_id, "runtime_action_id": id,
                        "consensus_action_payload_hash": committed.payload_hash}),
                        )
                    },
                )
            }
            WorldServicePayloadV1::AgentChat(request) => {
                let proof = request
                    .auth
                    .as_ref()
                    .ok_or("canonical chat proof required")?;
                let staged_nonce_consumed = staged.iter().any(|entry: &StagedResult| {
                    matches!(&entry.result.request.signed_payload, WorldServicePayloadV1::AgentChat(previous)
                        if previous.auth.as_ref().is_some_and(|old| old.public_key == proof.public_key && old.nonce >= proof.nonce))
                });
                if staged_nonce_consumed {
                    Err("canonical chat nonce did not advance within block".into())
                } else {
                    candidate.apply_authenticated_agent_chat(&request)
                }
            }
            WorldServicePayloadV1::Cognition(signed) => {
                match candidate.commit_authenticated_cognition_disposition_staged(
                    signed,
                    &staged
                        .iter()
                        .map(|entry: &StagedResult| entry.result.clone())
                        .collect::<Vec<_>>(),
                ) {
                    Ok(AuthenticatedCognitionDisposition::Committed(receipt)) => Ok(receipt),
                    Ok(AuthenticatedCognitionDisposition::RejectedStale(rejection)) => {
                        let reason = rejection.rejection_text();
                        rejected_cognition = Some(rejection);
                        Err(reason)
                    }
                    Err(error) => Err(error),
                }
            }
            WorldServicePayloadV1::FeedbackAck(signed) => {
                candidate.apply_authenticated_feedback_ack(signed)
            }
            WorldServicePayloadV1::Delegation(signed) => candidate
                .apply_agent_signer_delegation(&signed)
                .map(|()| serde_json::json!({"delegation": signed.request})),
            WorldServicePayloadV1::Scheduler(signed) => candidate
                .apply_authenticated_scheduler_staged(
                    signed,
                    &staged
                        .iter()
                        .map(|entry: &StagedResult| entry.result.clone())
                        .collect::<Vec<_>>(),
                ),
        };
        let (receipt, rejected) = match applied {
            Ok(value) => {
                *world = candidate;
                (value, None)
            }
            Err(reason) => {
                if let Some(rejection) = rejected_cognition {
                    // Failed candidate causal/action/event mutations are never
                    // adopted. Only approved terminal metadata and outbox are retained.
                    let mut rejected_world = world.clone();
                    rejection.publish(&mut rejected_world)?;
                    *world = rejected_world;
                }
                (serde_json::Value::Null, Some(reason))
            }
        };
        staged.push(StagedResult {
            result: CanonicalIntentResultV1 {
                request,
                action_id,
                committed_height: context.height,
                receipt,
                rejected,
            },
            runtime_action_id,
        });
    }
    Ok(staged)
}

pub(super) fn finalize_intents(world: &mut World, staged: Vec<StagedResult>) -> Result<(), String> {
    for mut entry in staged {
        if let Some(id) = entry.runtime_action_id {
            let events: Vec<_> = world
                .journal()
                .events
                .iter()
                .filter(|event| event.caused_by == Some(CausedBy::Action(id)))
                .cloned()
                .collect();
            if let Some(reason) = events.iter().find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::ActionRejected { action_id, reason })
                    if *action_id == id =>
                {
                    Some(format!("{reason:?}"))
                }
                _ => None,
            }) {
                entry.result.rejected = Some(reason);
            }
            // No terminal evidence cannot be represented as a successful receipt.
            if events.is_empty() {
                return Err(
                    "service gameplay execution produced no correlated event evidence".into(),
                );
            }
            entry.result.receipt["events"] =
                serde_json::to_value(events).map_err(|e| e.to_string())?;
        }
        world.record_world_service_result(entry.result)?;
    }
    Ok(())
}
