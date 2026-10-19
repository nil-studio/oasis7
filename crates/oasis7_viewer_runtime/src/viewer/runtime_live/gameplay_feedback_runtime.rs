//! Update retained request displays only from exact Runtime action/job evidence.
use super::*;
use crate::runtime::WorldEvent as RuntimeWorldEvent;

struct RuntimeFeedbackUpdate {
    action_id: u64,
    stage: &'static str,
    effect: &'static str,
    reason: Option<String>,
    terminal: bool,
}

fn runtime_feedback_update(event: &RuntimeWorldEvent) -> Option<RuntimeFeedbackUpdate> {
    let RuntimeWorldEventBody::Domain(domain) = &event.body else {
        return None;
    };
    let caused_action = match event.caused_by.as_ref() {
        Some(crate::runtime::CausedBy::Action(action_id)) => Some(*action_id),
        _ => None,
    };
    let (action_id, stage, effect, reason, terminal) = match domain {
        RuntimeDomainEvent::ActionRejected { action_id, reason } => (
            *action_id,
            "rejected",
            "Runtime rejected the submitted action",
            Some(format!("{reason:?}")),
            true,
        ),
        RuntimeDomainEvent::FactoryBuildStarted { job_id, .. } => (
            *job_id,
            "executing",
            "Factory construction started; the construction job is still running",
            None,
            false,
        ),
        RuntimeDomainEvent::RecipeStarted { job_id, .. } => (
            *job_id,
            "executing",
            "Production started; the recipe job is still running",
            None,
            false,
        ),
        RuntimeDomainEvent::FactoryBuilt { job_id, .. } => (
            *job_id,
            "completed_advanced",
            "The submitted factory construction job completed",
            None,
            true,
        ),
        RuntimeDomainEvent::RecipeCompleted { job_id, .. } => (
            *job_id,
            "completed_advanced",
            "The submitted production job completed",
            None,
            true,
        ),
        RuntimeDomainEvent::FactoryProductionBlocked {
            action_id,
            blocker_kind,
            blocker_detail,
            ..
        } => (
            *action_id,
            "blocked",
            "The submitted production job stopped at a Runtime constraint",
            Some(format!("{blocker_kind}: {blocker_detail}")),
            true,
        ),
        RuntimeDomainEvent::AgentRegistered { .. }
        | RuntimeDomainEvent::StarterOcClaimed { .. }
        | RuntimeDomainEvent::AgentClaimed { .. }
        | RuntimeDomainEvent::DataCollected { .. }
        | RuntimeDomainEvent::DataCollectedAuthenticated { .. } => (
            caused_action?,
            "completed_advanced",
            "Runtime applied the submitted action",
            None,
            true,
        ),
        RuntimeDomainEvent::AgentClaimReleaseRequested { .. } => (
            caused_action?,
            "applied",
            "Runtime applied the release request; the claim cooldown continues",
            None,
            true,
        ),
        // Acceptance notices, provider/effect receipts and unrelated world
        // events do not establish completion of a queued gameplay action.
        _ => return None,
    };
    if caused_action.is_some_and(|caused_action| caused_action != action_id) {
        return None;
    }
    Some(RuntimeFeedbackUpdate {
        action_id,
        stage,
        effect,
        reason,
        terminal,
    })
}

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn update_gameplay_feedback_from_runtime_events(
        &mut self,
        events: &[RuntimeWorldEvent],
    ) {
        self.prune_scoped_gameplay_feedback();
        for event in events {
            let Some(update) = runtime_feedback_update(event) else {
                continue;
            };
            let Some(player_id) = self.runtime_action_players.get(&update.action_id) else {
                continue;
            };
            let Some(current) = self.scoped_gameplay_feedback.get(player_id) else {
                continue;
            };
            if current.origin.runtime_action_id != Some(update.action_id) {
                continue;
            }
            // The action association lives only in the bounded current
            // display. A superseding request or eviction removes it, so a
            // late action event cannot restore an older display record.
            let mut origin = current.origin.clone();
            let mut feedback = current.feedback.clone();
            feedback.stage = update.stage.into();
            feedback.effect = format!("{} (action {})", update.effect, update.action_id);
            feedback.reason = update.reason;
            feedback.hint = Some(
                if update.terminal {
                    "inspect the current world state and the action result before choosing the next action"
                } else {
                    "continue observing the running job for its matching completion or blocker"
                }
                .into(),
            );
            feedback.delta_event_seq = feedback.delta_event_seq.saturating_add(1);
            if update.terminal {
                origin.runtime_action_id = None;
            }
            self.set_scoped_gameplay_feedback(origin, feedback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(action_id: u64) -> (ViewerRuntimeLiveServer, VerifiedPlayerAuth) {
        let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
            WorldScenario::Minimal,
        ))
        .unwrap();
        server
            .session_policy
            .register_session("owner", "key")
            .unwrap();
        let verified = VerifiedPlayerAuth {
            player_id: "owner".into(),
            public_key: "key".into(),
            nonce: 1,
            hosted_registration_nonce: None,
        };
        server
            .runtime_action_players
            .insert(action_id, verified.player_id.clone());
        server.set_verified_player_gameplay_feedback_for_action(
            &verified,
            Some(action_id),
            ViewerRuntimeLiveServer::make_player_gameplay_feedback(
                "gameplay_action:build_factory_smelter_mk1",
                "queued",
                "queued request",
                None,
                None,
                None,
                None,
                0,
                0,
            ),
        );
        (server, verified)
    }

    fn event(body: RuntimeDomainEvent, caused_action: Option<u64>) -> RuntimeWorldEvent {
        RuntimeWorldEvent {
            id: 1,
            time: 1,
            caused_by: caused_action.map(crate::runtime::CausedBy::Action),
            body: RuntimeWorldEventBody::Domain(body),
        }
    }

    #[test]
    fn runtime_job_feedback_ignores_acceptance_and_waits_for_exact_job_completion() {
        let (mut server, _) = queued(7);
        let RuntimeAction::BuildFactory { spec, .. } =
            crate::viewer::gameplay_actions::runtime_factory_build_action(
                "agent",
                "site",
                "factory.smelter.mk1",
                "factory.smelter.mk1",
            )
            .unwrap()
        else {
            panic!("factory action")
        };
        let accepted = event(
            RuntimeDomainEvent::ActionAccepted {
                action_id: 7,
                action_kind: "build_factory".into(),
                actor_id: "agent".into(),
                eta_ticks: 1,
                notes: Vec::new(),
            },
            Some(7),
        );
        server.update_gameplay_feedback_from_runtime_events(&[accepted]);
        assert_eq!(
            server
                .scoped_feedback_for_player(Some("owner"))
                .unwrap()
                .stage,
            "queued"
        );
        let started = event(
            RuntimeDomainEvent::FactoryBuildStarted {
                job_id: 7,
                builder_agent_id: "agent".into(),
                site_id: "site".into(),
                spec: spec.clone(),
                consume_ledger: crate::runtime::MaterialLedgerId::agent("agent"),
                ready_at: 5,
                contract_version: None,
                site_authority_revision: None,
                site_location_id: None,
                location_anchor_revision: None,
                construction_power_obligation: None,
            },
            Some(7),
        );
        server.update_gameplay_feedback_from_runtime_events(&[started]);
        assert_eq!(
            server
                .scoped_feedback_for_player(Some("owner"))
                .unwrap()
                .stage,
            "executing"
        );
        let completed = |job_id| {
            event(
                RuntimeDomainEvent::FactoryBuilt {
                    job_id,
                    builder_agent_id: "agent".into(),
                    site_id: "site".into(),
                    spec: spec.clone(),
                },
                None,
            )
        };
        server.update_gameplay_feedback_from_runtime_events(&[completed(8)]);
        assert_eq!(
            server
                .scoped_feedback_for_player(Some("owner"))
                .unwrap()
                .stage,
            "executing"
        );
        server.update_gameplay_feedback_from_runtime_events(&[completed(7)]);
        assert_eq!(
            server
                .scoped_feedback_for_player(Some("owner"))
                .unwrap()
                .stage,
            "completed_advanced"
        );
        assert!(
            server
                .scoped_gameplay_feedback
                .get("owner")
                .unwrap()
                .origin
                .runtime_action_id
                .is_none()
        );
        assert_eq!(
            server.runtime_action_players.get(&7).map(String::as_str),
            Some("owner"),
            "presentation does not delete recovery attribution"
        );
        assert!(server.latest_player_gameplay_feedback.is_none());
    }

    #[test]
    fn runtime_action_result_cannot_replace_a_newer_request_or_cross_a_session_epoch() {
        for change in ["newer_request", "session_epoch"] {
            let (mut server, verified) = queued(7);
            if change == "newer_request" {
                server.set_verified_player_gameplay_feedback_for_action(
                    &verified,
                    Some(8),
                    ViewerRuntimeLiveServer::make_player_gameplay_feedback(
                        "gameplay_action:collect_data",
                        "queued",
                        "newer request",
                        None,
                        None,
                        None,
                        None,
                        0,
                        0,
                    ),
                );
            } else {
                server
                    .session_policy
                    .rotate_session("owner", "key", "new-key")
                    .unwrap();
            }
            let result = event(
                RuntimeDomainEvent::DataCollected {
                    collector_agent_id: "agent".into(),
                    electricity_cost: 1,
                    data_amount: 1,
                },
                Some(7),
            );
            server.update_gameplay_feedback_from_runtime_events(&[result]);
            if change == "newer_request" {
                assert_eq!(
                    server
                        .scoped_feedback_for_player(Some("owner"))
                        .unwrap()
                        .effect,
                    "newer request"
                );
            } else {
                assert!(server.scoped_feedback_for_player(Some("owner")).is_none());
            }
            assert!(server.latest_player_gameplay_feedback.is_none());
        }
    }
}
