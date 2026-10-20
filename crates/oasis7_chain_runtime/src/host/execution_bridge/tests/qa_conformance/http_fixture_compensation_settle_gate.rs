//! Real committed compensation settlement, held before its authenticated minimum View.
use super::*;
#[derive(Default)]
pub(in super::super) struct CompensationGateState {
    arm: Option<(WorldIdentity, String, String)>,
    candidate: Option<SubmitIntentRequest<WorldServicePayloadV1>>,
    commit: Option<CommitRef>,
    owner: Option<thread::ThreadId>,
    released: bool,
    returned: bool,
    joined: bool,
}
impl WorldGate {
    pub(in super::super) fn arm_compensation_settle_view(
        &self,
        world: WorldIdentity,
        agent: &str,
        lease: &str,
    ) -> Result<(), String> {
        let mut state = self.compensation_settle.lock().unwrap();
        if state.arm.is_some() {
            return Err("compensation gate arm is one-shot".into());
        }
        state.arm = Some((world, agent.into(), lease.into()));
        Ok(())
    }
    pub(super) fn observe_compensation_settle(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) {
        let mut state = self.compensation_settle.lock().unwrap();
        let Some((world, agent, lease)) = &state.arm else {
            return;
        };
        let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
            return;
        };
        let SchedulerOperationV1::SettleLease {
            lease_id,
            consumed_amount,
        } = &signed.request.operation
        else {
            return;
        };
        if &request.correlation.key.world != world
            || &signed.request.agent_id != agent
            || lease_id != lease
        {
            return;
        }
        assert_eq!(*consumed_amount, 1, "fixed original compensation amount");
        request.validate().unwrap();
        assert_eq!(
            correlation::derive_correlation(world.clone(), &request.signed_payload).unwrap(),
            request.correlation
        );
        if let Some(previous) = &state.candidate {
            assert_eq!(previous, request, "original signed settlement immutable")
        } else {
            state.candidate = Some(request.clone());
        }
        self.compensation_changed.notify_all();
    }
    pub(in super::super) fn install_compensation_settle_selector(
        &self,
        client: &RemoteWorldServiceClient,
    ) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let original = loop {
            if let Some(request) = self.compensation_settle.lock().unwrap().candidate.clone() {
                break request;
            }
            if Instant::now() >= deadline {
                return Err("actual signed compensation candidate absent".into());
            }
            thread::sleep(Duration::from_millis(2));
        };
        let (commit, receipt) = loop {
            let response = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: 1,
                        key: original.correlation.key.clone(),
                    },
                    original.signed_payload.clone(),
                )
                .map_err(|e| e.to_string())?;
            match response.outcome {
                IntentOutcome::Committed { commit, receipt } => break (commit, receipt),
                IntentOutcome::Unknown
                | IntentOutcome::Pending
                | IntentOutcome::Received { .. } => {}
                _ => return Err("original compensation not committed".into()),
            }
            if Instant::now() >= deadline {
                return Err("actual compensation commit absent".into());
            }
            thread::sleep(Duration::from_millis(2));
        };
        let receipt: oasis7_runtime::CognitionReceiptV1 =
            serde_json::from_value(receipt).map_err(|e| e.to_string())?;
        receipt.validate().map_err(|e| e.to_string())?;
        let mut state = self.compensation_settle.lock().unwrap();
        let (world, agent, lease) = state.arm.as_ref().unwrap();
        if receipt.operation != "settle"
            || receipt.status != oasis7_runtime::CognitionLeaseStatusV1::Settled
            || &receipt.lease_id != lease
            || &receipt.agent_id != agent
            || receipt.consumed_amount != 1
            || receipt.net_amount != 1
            || receipt.reserved_amount != 1
            || receipt.released_amount != 0
            || receipt.refunded_amount != 0
        {
            return Err("actual compensation receipt mismatch".into());
        }
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("gate root absent")?;
        write_new_json(&root.join("world-compensation-settle-selector.json"),&serde_json::json!({"world":world,"scope_id":format!("agent:{agent}"),"min_commit":commit,"correlation_digest":correlation::key_digest(&original.correlation.key)?,"payload_digest":oasis7::world_service::authority::request_digest("compensation-payload-v1",&original.signed_payload)?,"receipt_digest":oasis7::world_service::authority::request_digest("compensation-receipt-v1",&receipt)?})).map_err(|e|e.to_string())?;
        state.commit = Some(*commit);
        self.compensation_changed.notify_all();
        Ok(original)
    }
    pub(super) fn pause_compensation_view(
        &self,
        root: &Path,
        request: &wire::SignedReadRequest<ReadWorldViewRequest>,
    ) -> bool {
        let mut state = self.compensation_settle.lock().unwrap();
        let Some((world, agent, _)) = &state.arm else {
            return false;
        };
        if &request.request.world != world
            || request.request.scope_id != format!("agent:{agent}")
            || request.request.fixed_commit.is_some()
            || request.request.min_commit.is_none()
            || state.candidate.is_none()
            || state.owner.is_some()
        {
            return false;
        }
        let deadline = Instant::now() + Duration::from_millis(1200);
        while state.commit.is_none() && Instant::now() < deadline {
            state = self
                .compensation_changed
                .wait_timeout(state, Duration::from_millis(2))
                .unwrap()
                .0;
        }
        if request.request.min_commit.as_ref() != state.commit.as_ref() {
            return false;
        }
        state.owner = Some(thread::current().id());
        write_new_json(&root.join("world-compensation-settle-view-started"),&serde_json::json!({"world":request.request.world,"scope_id":request.request.scope_id,"min_commit":request.request.min_commit,"fixed_commit_none":true,"signature_verified":true,"node_view_handler_dispatched":false})).unwrap();
        drop(state);
        let deadline = Instant::now() + Duration::from_secs(30);
        while !root.join("world-compensation-settle-view-release").exists()
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(2));
        }
        let released = self.compensation_settle.lock().unwrap().released;
        if released {
            write_new_json(&root.join("world-compensation-settle-view-gate-returned"),&serde_json::json!({"abandoned_after_confirmed_exit73":true,"node_view_handler_dispatched":false})).unwrap();
        }
        released
    }
    pub(in super::super) fn release_compensation_settle_after_exit73(
        &self,
        status: &ExitStatus,
    ) -> Result<(), String> {
        if status.code() != Some(73) {
            return Err("actual child exit73 required".into());
        }
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("gate root absent")?;
        let mut state = self.compensation_settle.lock().unwrap();
        if state.owner.is_none() || state.released {
            return Err("exact compensation View not held".into());
        }
        write_new_json(
            &root.join("world-compensation-settle-view-release"),
            &serde_json::json!({"confirmed_child_exit_code":73}),
        )
        .map_err(|e| e.to_string())?;
        state.released = true;
        Ok(())
    }
    pub(super) fn compensation_handler_returned(&self, root: &Path, current: thread::ThreadId) {
        let mut state = self.compensation_settle.lock().unwrap();
        if state.owner == Some(current) && state.released && !state.returned {
            state.returned = true;
            write_new_json(&root.join("world-compensation-settle-view-handler-returned"),&serde_json::json!({"connection_handler_returned":true,"abandoned_after_confirmed_exit73":true,"node_view_handler_dispatched":false})).unwrap();
        }
    }
    pub(super) fn compensation_worker_joined(&self, root: &Path, current: thread::ThreadId) {
        let mut state = self.compensation_settle.lock().unwrap();
        if state.owner == Some(current) && state.returned && !state.joined {
            state.joined = true;
            write_new_json(&root.join("world-compensation-settle-view-worker-joined"),&serde_json::json!({"listener_owner_join_observed":true,"worker_join_succeeded":true,"abandoned_after_confirmed_exit73":true,"node_view_handler_dispatched":false})).unwrap();
        }
    }
}
