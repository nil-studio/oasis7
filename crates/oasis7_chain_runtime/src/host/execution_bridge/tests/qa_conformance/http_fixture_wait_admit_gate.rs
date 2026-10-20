//! Exact, one-shot process-recovery gate for a committed hosted Wait Admit.
use super::*;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::ExitStatus};

const RENDEZVOUS_BUDGET: Duration = Duration::from_millis(1_200);
const HOLD_BUDGET: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdmitWaitArm {
    origin_request_digest: String,
    continuation_proposal_id_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdmitWaitCandidate {
    world: WorldIdentity,
    scope_id: String,
    origin_request_digest: String,
    continuation_proposal_id_digest: String,
    proposal_digest: String,
    identity_digest: String,
    correlation_digest: String,
    payload_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AdmitWaitViewSelector {
    world: WorldIdentity,
    scope_id: String,
    min_commit: CommitRef,
    origin_request_digest: String,
    continuation_proposal_id_digest: String,
    proposal_digest: String,
    identity_digest: String,
    correlation_digest: String,
    payload_digest: String,
    authenticated_receipt_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) struct WaitAdmitViewSelectorEvidence {
    pub(in super::super) commit: CommitRef,
    pub(in super::super) selector_digest: String,
    pub(in super::super) correlation_digest: String,
    pub(in super::super) payload_digest: String,
    pub(in super::super) receipt_digest: String,
}

#[derive(Default)]
pub(in super::super) struct AdmitWaitViewGateState {
    arm: Option<AdmitWaitArm>,
    candidate: Option<AdmitWaitCandidate>,
    candidate_conflict: bool,
    selector: Option<AdmitWaitViewSelector>,
    selector_digest: Option<String>,
    selector_installing: bool,
    selector_failed: bool,
    claimed: bool,
    owner: Option<thread::ThreadId>,
    released: bool,
    handler_returned: bool,
    joined: bool,
}

fn identity_digest(values: [&str; 9]) -> Result<String, String> {
    oasis7::world_service::authority::request_digest("wait-admit-continuation-identity-v1", &values)
}

fn opaque_digest(domain: &str, value: &str) -> Result<String, String> {
    oasis7::world_service::authority::request_digest(domain, &value)
}

fn parse_candidate(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<AdmitWaitCandidate, String> {
    request.validate().map_err(|error| error.to_string())?;
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return Err("Wait Admit candidate is not a scheduler payload".into());
    };
    let SchedulerOperationV1::AdmitContinuation(proposal) = &signed.request.operation else {
        return Err("scheduler candidate is not AdmitContinuation".into());
    };
    if proposal.proposal_digest != proposal.proposal_digest() {
        return Err("Wait Admit candidate canonical proposal digest mismatch".into());
    }
    let derived = correlation::derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )?;
    if derived != request.correlation || proposal.world_id != derived.key.world.world_id {
        return Err("Wait Admit signed identity or correlation mismatch".into());
    }
    let origin_request_digest = opaque_digest(
        "wait-admit-origin-request-v1",
        &proposal.origin_request_digest,
    )?;
    let continuation_proposal_id_digest = opaque_digest(
        "wait-admit-proposal-id-v1",
        &proposal.continuation_proposal_id,
    )?;
    let identity_digest = identity_digest([
        &proposal.world_id,
        &proposal.agent_id,
        &proposal.agent_session_id,
        &proposal.agent_turn_id,
        &proposal.decision_request_id,
        &proposal.origin_turn_id,
        &proposal.origin_request_digest,
        &proposal.continuation_proposal_id,
        &proposal.proposal_digest,
    ])?;
    Ok(AdmitWaitCandidate {
        world: derived.key.world.clone(),
        scope_id: format!("agent:{}", proposal.agent_id),
        origin_request_digest,
        continuation_proposal_id_digest,
        proposal_digest: proposal.proposal_digest.clone(),
        identity_digest,
        correlation_digest: correlation::key_digest(&derived.key)?,
        payload_digest: derived.payload_digest,
    })
}

impl super::WorldGate {
    pub(in super::super) fn arm_admit_wait_view_gate(
        &self,
        origin_request_digest: &str,
        continuation_proposal_id: &str,
    ) -> Result<(), String> {
        if origin_request_digest.trim().is_empty() || continuation_proposal_id.trim().is_empty() {
            return Err("Wait Admit gate arm identity is empty".into());
        }
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Wait Admit View gate root is not configured")?;
        let arm = AdmitWaitArm {
            origin_request_digest: opaque_digest(
                "wait-admit-origin-request-v1",
                origin_request_digest,
            )?,
            continuation_proposal_id_digest: opaque_digest(
                "wait-admit-proposal-id-v1",
                continuation_proposal_id,
            )?,
        };
        let mut state = self.admit_wait_view.lock().unwrap();
        if state.arm.is_some() || state.candidate.is_some() || state.selector_failed {
            return Err("Wait Admit View gate arm is one-shot".into());
        }
        super::write_new_json(
            &root.join("world-wait-admit-view-arm"),
            &serde_json::json!({
                "operation_kind": "AdmitContinuation",
                "origin_request_digest": arm.origin_request_digest,
                "continuation_proposal_id_digest": arm.continuation_proposal_id_digest,
            }),
        )
        .map_err(|error| format!("immutable Wait Admit gate arm failed: {error}"))?;
        state.arm = Some(arm);
        Ok(())
    }

    pub(in super::super) fn install_admit_wait_view_selector_from_authenticated_lookup(
        &self,
        client: &RemoteWorldServiceClient,
        scope_id: &str,
        original: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<WaitAdmitViewSelectorEvidence, String> {
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Wait Admit View gate root is not configured")?;
        let expected = parse_candidate(&original)?;
        if scope_id != expected.scope_id {
            return Err("Wait Admit selector scope differs from signed proposal".into());
        }
        {
            let mut state = self.admit_wait_view.lock().unwrap();
            let Some(arm) = state.arm.as_ref() else {
                return Err("Wait Admit View gate is not armed".into());
            };
            if arm.origin_request_digest != expected.origin_request_digest
                || arm.continuation_proposal_id_digest != expected.continuation_proposal_id_digest
            {
                return Err("Wait Admit selector differs from one-shot arm".into());
            }
            if state.selector.is_some() || state.selector_installing || state.selector_failed {
                return Err("Wait Admit View selector is one-shot".into());
            }
            state.selector_installing = true;
            let deadline = Instant::now() + RENDEZVOUS_BUDGET;
            while state.candidate.is_none()
                && !state.candidate_conflict
                && Instant::now() < deadline
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let (next, _) = self
                    .admit_wait_view_changed
                    .wait_timeout(state, remaining)
                    .unwrap();
                state = next;
            }
            if state.candidate.as_ref() != Some(&expected) || state.candidate_conflict {
                state.selector_installing = false;
                state.selector_failed = true;
                self.admit_wait_view_changed.notify_all();
                return Err("Wait Admit selector did not match one unique signed Admit".into());
            }
        }

        let result = (|| {
            let response = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                        key: original.correlation.key.clone(),
                    },
                    original.signed_payload.clone(),
                )
                .map_err(|_| "authenticated Wait Admit Lookup failed".to_string())?;
            response
                .validate(&original.correlation)
                .map_err(|_| "authenticated Wait Admit Lookup identity mismatch".to_string())?;
            let IntentOutcome::Committed { commit, receipt } = response.outcome else {
                return Err("authenticated Wait Admit Lookup is not committed".into());
            };
            commit
                .validate()
                .map_err(|_| "Wait Admit CommitRef is invalid".to_string())?;
            if commit.world != expected.world {
                return Err("Wait Admit CommitRef world differs from signed candidate".into());
            }
            let admitted: oasis7_runtime::AgentContinuation = serde_json::from_value(receipt)
                .map_err(|_| {
                    "authenticated Wait Admit receipt is not AgentContinuation".to_string()
                })?;
            admitted
                .validate_authoritative()
                .map_err(|_| "authenticated Wait Admit continuation is invalid".to_string())?;
            let receipt_identity_digest = identity_digest([
                &admitted.world_id,
                &admitted.agent_id,
                &admitted.agent_session_id,
                &admitted.agent_turn_id,
                &admitted.decision_request_id,
                &admitted.origin_turn_id,
                &admitted.origin_request_digest,
                &admitted.continuation_proposal_id,
                &admitted.proposal_digest,
            ])?;
            if receipt_identity_digest != expected.identity_digest {
                return Err(
                    "authenticated Wait Admit receipt tuple differs from signed proposal".into(),
                );
            }
            let receipt_digest = oasis7::world_service::authority::request_digest(
                "wait-admit-authenticated-receipt-v1",
                &admitted,
            )
            .map_err(|_| "authenticated Wait Admit receipt digest failed".to_string())?;
            Ok((commit, receipt_digest))
        })();

        let (commit, authenticated_receipt_digest) = match result {
            Ok(result) => result,
            Err(error) => {
                self.fail_admit_wait_selector_install();
                return Err(error);
            }
        };
        let selector = AdmitWaitViewSelector {
            world: expected.world.clone(),
            scope_id: expected.scope_id.clone(),
            min_commit: commit.as_ref().clone(),
            origin_request_digest: expected.origin_request_digest.clone(),
            continuation_proposal_id_digest: expected.continuation_proposal_id_digest.clone(),
            proposal_digest: expected.proposal_digest.clone(),
            identity_digest: expected.identity_digest.clone(),
            correlation_digest: expected.correlation_digest.clone(),
            payload_digest: expected.payload_digest.clone(),
            authenticated_receipt_digest: authenticated_receipt_digest.clone(),
        };
        let selector_digest = match oasis7::world_service::authority::request_digest(
            "wait-admit-view-selector-v1",
            &selector,
        ) {
            Ok(digest) => digest,
            Err(_) => {
                self.fail_admit_wait_selector_install();
                return Err("Wait Admit View selector digest failed".into());
            }
        };
        if let Err(error) =
            super::write_new_json(&root.join("world-wait-admit-view-selector.json"), &selector)
        {
            self.fail_admit_wait_selector_install();
            return Err(format!(
                "immutable Wait Admit selector write failed: {error}"
            ));
        }
        {
            let mut state = self.admit_wait_view.lock().unwrap();
            if state.candidate.as_ref() != Some(&expected)
                || state.candidate_conflict
                || !state.selector_installing
                || state.selector.is_some()
            {
                state.selector_installing = false;
                state.selector_failed = true;
                self.admit_wait_view_changed.notify_all();
                return Err("Wait Admit candidate changed during selector installation".into());
            }
            state.selector = Some(selector);
            state.selector_digest = Some(selector_digest.clone());
            state.selector_installing = false;
            self.admit_wait_view_changed.notify_all();
        }
        Ok(WaitAdmitViewSelectorEvidence {
            commit: *commit,
            selector_digest,
            correlation_digest: expected.correlation_digest,
            payload_digest: expected.payload_digest,
            receipt_digest: authenticated_receipt_digest,
        })
    }

    fn fail_admit_wait_selector_install(&self) {
        let mut state = self.admit_wait_view.lock().unwrap();
        state.selector_installing = false;
        state.selector_failed = true;
        self.admit_wait_view_changed.notify_all();
    }

    pub(in super::super) fn release_admit_wait_view_gate_after_child_exit73(
        &self,
        status: &ExitStatus,
    ) -> Result<(), String> {
        if status.code() != Some(73) {
            return Err("Wait Admit View release requires confirmed child exit 73".into());
        }
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Wait Admit View gate root is not configured")?;
        let mut state = self.admit_wait_view.lock().unwrap();
        if !state.claimed || state.selector.is_none() || state.released {
            return Err("Wait Admit View gate is not held for one-shot release".into());
        }
        super::write_new_json(
            &root.join("world-wait-admit-view-release"),
            &serde_json::json!({
                "child_exit_code": 73,
                "child_exit_confirmed": true,
                "abandoned_view_request": true,
                "node_view_handler_dispatched": false,
            }),
        )
        .map_err(|error| format!("immutable Wait Admit release marker failed: {error}"))?;
        state.released = true;
        self.admit_wait_view_changed.notify_all();
        Ok(())
    }

    pub(in super::super) fn observe_admit_wait_candidate(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) {
        let Ok(candidate) = parse_candidate(request) else {
            return;
        };
        let mut state = self.admit_wait_view.lock().unwrap();
        let Some(arm) = state.arm.as_ref() else {
            return;
        };
        if candidate.origin_request_digest != arm.origin_request_digest
            || candidate.continuation_proposal_id_digest != arm.continuation_proposal_id_digest
        {
            if candidate.origin_request_digest == arm.origin_request_digest
                || candidate.continuation_proposal_id_digest == arm.continuation_proposal_id_digest
            {
                state.candidate_conflict = true;
                self.admit_wait_view_changed.notify_all();
            }
            return;
        }
        if state.candidate.is_some() {
            state.candidate_conflict = true;
        } else {
            state.candidate = Some(candidate);
        }
        self.admit_wait_view_changed.notify_all();
    }

    pub(in super::super) fn pause_admit_wait_view(
        &self,
        root: &Path,
        request: &wire::SignedReadRequest<ReadWorldViewRequest>,
    ) -> bool {
        let Some(min_commit) = request.request.min_commit.as_ref() else {
            return false;
        };
        if request.request.fixed_commit.is_some() {
            return false;
        }
        let mut state = self.admit_wait_view.lock().unwrap();
        let Some(candidate) = state.candidate.clone() else {
            return false;
        };
        if state.candidate_conflict
            || candidate.world != request.request.world
            || candidate.scope_id != request.request.scope_id
        {
            return false;
        }
        let deadline = Instant::now() + RENDEZVOUS_BUDGET;
        while state.selector.is_none()
            && !state.selector_failed
            && !state.candidate_conflict
            && Instant::now() < deadline
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (next, _) = self
                .admit_wait_view_changed
                .wait_timeout(state, remaining)
                .unwrap();
            state = next;
        }
        let Some(selector) = state.selector.clone() else {
            return false;
        };
        if state.selector_failed
            || state.claimed
            || selector.world != request.request.world
            || selector.scope_id != request.request.scope_id
            || selector.min_commit != *min_commit
        {
            return false;
        }
        let selector_digest = state
            .selector_digest
            .clone()
            .expect("Wait Admit selector has digest");
        state.claimed = true;
        state.owner = Some(thread::current().id());
        drop(state);

        let view_request_digest =
            oasis7::world_service::authority::request_digest(VIEW_PATH, &request.request)
                .expect("verified Wait Admit View request has digest");
        super::write_new_json(
            &root.join("world-wait-admit-view-started"),
            &serde_json::json!({
                "operation_kind": "AdmitContinuation",
                "signature_verified": true,
                "selector_matches_signed_view": true,
                "fixed_commit_none": true,
                "node_view_handler_dispatched": false,
                "world": selector.world,
                "scope_id": selector.scope_id,
                "min_commit": selector.min_commit,
                "origin_request_digest": selector.origin_request_digest,
                "continuation_proposal_id_digest": selector.continuation_proposal_id_digest,
                "proposal_digest": selector.proposal_digest,
                "identity_digest": selector.identity_digest,
                "correlation_digest": selector.correlation_digest,
                "payload_digest": selector.payload_digest,
                "authenticated_receipt_digest": selector.authenticated_receipt_digest,
                "selector_digest": selector_digest,
                "view_request_digest": view_request_digest,
            }),
        )
        .expect("immutable Wait Admit View started marker must be unique");

        let deadline = Instant::now() + HOLD_BUDGET;
        while !wait_admit_release_confirmed(root) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        if wait_admit_release_confirmed(root) {
            super::write_new_json(
                &root.join("world-wait-admit-view-gate-returned"),
                &serde_json::json!({
                    "abandoned_after_confirmed_exit73": true,
                    "child_exit_code": 73,
                    "node_view_handler_dispatched": false,
                    "selector_digest": selector_digest,
                }),
            )
            .expect("immutable Wait Admit View gate-returned marker must be unique");
            true
        } else {
            let _ = super::write_new_json(
                &root.join("world-wait-admit-view-timeout"),
                &serde_json::json!({
                    "abandoned_after_confirmed_exit73": false,
                    "node_view_handler_dispatched": false,
                    "selector_digest": selector_digest,
                }),
            );
            let mut state = self.admit_wait_view.lock().unwrap();
            if state.owner == Some(thread::current().id()) {
                state.owner = None;
            }
            false
        }
    }

    pub(in super::super) fn admit_wait_connection_worker_finished(
        &self,
        root: &Path,
        current: thread::ThreadId,
    ) {
        let returned = {
            let mut state = self.admit_wait_view.lock().unwrap();
            if state.owner == Some(current) && state.released && !state.handler_returned {
                state.handler_returned = true;
                true
            } else {
                false
            }
        };
        if returned {
            super::write_new_json(
                &root.join("world-wait-admit-view-connection-handler-returned"),
                &serde_json::json!({
                    "connection_handler_returned": true,
                    "abandoned_after_confirmed_exit73": true,
                    "node_view_handler_dispatched": false,
                }),
            )
            .expect("immutable Wait Admit connection-handler marker must be unique");
        }
    }

    pub(in super::super) fn admit_wait_connection_worker_joined(
        &self,
        root: &Path,
        worker_id: thread::ThreadId,
    ) {
        let joined = {
            let mut state = self.admit_wait_view.lock().unwrap();
            if state.owner == Some(worker_id)
                && state.released
                && state.handler_returned
                && !state.joined
            {
                state.joined = true;
                true
            } else {
                false
            }
        };
        if joined {
            super::write_new_json(
                &root.join("world-wait-admit-view-worker-joined"),
                &serde_json::json!({
                    "listener_owner_join_observed": true,
                    "worker_join_succeeded": true,
                    "abandoned_after_confirmed_exit73": true,
                    "node_view_handler_dispatched": false,
                }),
            )
            .expect("immutable Wait Admit worker-joined marker must be unique");
        }
    }
}

fn wait_admit_release_confirmed(root: &Path) -> bool {
    fs::read(root.join("world-wait-admit-view-release"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| {
            value["child_exit_code"] == 73 && value["child_exit_confirmed"] == true
        })
}
