//! Durable, independently confirmed waypoint/timing alternatives. V1 executes
//! exactly one zero-cost canonical MoveAgent, never a path or timed promise.
use super::*;
use crate::cognition_recovery::cognition_digest_v1;
use crate::service_admission::{SignedReadRequest, safe_alternative::validate_owner};
use crate::{Action, CausedBy, DomainEvent, WorldEventBody};
use oasis7_client_api::safe_alternative::{
    SafeAlternativeCommandV1, SafeAlternativeOperationV1 as Op,
};
use serde_json::{Value, json};

pub const MAX_SAFE_ALTERNATIVE_GENERATIONS: u64 = 8;
pub const MAX_SAFE_ALTERNATIVE_LINEAGES: usize = 1024;
pub const MAX_SAFE_ALTERNATIVE_RECORD_BYTES: usize = 131072;

pub fn lineage_key(c: &SafeAlternativeCommandV1) -> String {
    cognition_digest_v1(
        "oasis7.safe-alternative.lineage.v1",
        &json!([
            c.world_id,
            c.genesis_digest,
            c.branch_id,
            c.reorg_epoch,
            c.agent_id,
            c.owner_binding,
            c.owner_generation,
            c.operation.request_id()
        ]),
    )
}
fn digest(v: &Value) -> String {
    cognition_digest_v1("oasis7.safe-alternative.content.v1", v)
}
fn error(reason: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}

impl World {
    pub fn apply_authenticated_safe_alternative(
        &mut self,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
    ) -> Result<Value, String> {
        validate_owner(self, signed)?;
        let key = lineage_key(&signed.request);
        if matches!(signed.request.operation, Op::Read { .. }) {
            return self
                .capability_revocation_state
                .safe_alternatives
                .get(&key)
                .cloned()
                .ok_or("original_not_eligible".into());
        }
        // The event carries the complete signed command; replay runs the same
        // deterministic transition and installs the same unique action binding.
        self.append_capability_authorization_event_batch(vec![
            crate::CapabilityAuthorizationEvent::SafeAlternativeCommandRecorded {
                signed: signed.clone(),
            },
        ])
        .map_err(|e| format!("{e:?}"))?;
        self.capability_revocation_state
            .safe_alternatives
            .get(&key)
            .cloned()
            .ok_or("outcome_unknown".into())
    }
    pub(super) fn project_safe_alternative(
        &self,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
    ) -> Result<(String, Value), String> {
        validate_owner(self, signed)?;
        let c = &signed.request;
        let key = lineage_key(c);
        let now = self.state.time;
        let previous = self.capability_revocation_state.safe_alternatives.get(&key);
        let command_digest = digest(&json!(c));
        if let Some(old) = previous
            && let Some(recorded) = old["command_digests"].get(&c.client_operation_id)
        {
            if *recorded != command_digest {
                return Err("identity_conflict".into());
            }
            return Ok((key, old.clone()));
        }
        if let Op::Propose {
            original,
            valid_until_tick,
            predecessor_request_id,
            ..
        } = &c.operation
        {
            let original_digest =
                digest(&serde_json::to_value(original).map_err(|e| e.to_string())?);
            if let Some(old) = previous {
                if old["original_digest"] != original_digest
                    || old["binding"]["operation"] != json!(c.operation)
                {
                    return Err("identity_conflict".into());
                }
                return Ok((key, old.clone()));
            }
            if self.capability_revocation_state.safe_alternatives.len()
                >= MAX_SAFE_ALTERNATIVE_LINEAGES
            {
                return Err("evaluation_unavailable".into());
            }
            if *valid_until_tick <= now {
                return Err("candidate_stale".into());
            }
            let evaluation_unavailable =
                !self.state.module_instances.is_empty() || !self.module_registry.active.is_empty();
            if let Some(predecessor) = predecessor_request_id {
                let mut predecessor_command = c.clone();
                predecessor_command.operation = Op::Read {
                    request_id: predecessor.clone(),
                };
                let valid = self
                    .capability_revocation_state
                    .safe_alternatives
                    .get(&lineage_key(&predecessor_command))
                    .is_some_and(|r| r["outcome"]["status"] == "rejected");
                if !valid {
                    return Err("original_outcome_unknown".into());
                }
            }
            let agent = self
                .state
                .agents
                .get(&c.agent_id)
                .ok_or("authority_changed")?;
            let mut candidates = Vec::new();
            for to in &original.waypoints {
                if evaluation_unavailable {
                    continue;
                }
                if candidates.iter().any(|v: &Value| v["to"] == json!(to)) {
                    continue;
                }
                let location = format!("{}:{}:{}", to.x_cm, to.y_cm, to.z_cm);
                if self
                    .state
                    .gameplay_policy
                    .forbidden_location_ids
                    .contains(&location)
                {
                    continue;
                }
                let mut candidate = json!({"candidate_id":format!("endpoint-{}",candidates.len()+1),"to":to,
                    "goal":"Move directly to this endpoint; omit the requested path and arrival time",
                    "scope":{"agent_id":c.agent_id,"from":agent.state.pos,"to":to,"maximum_effects":1},
                    "cost_upper_bound":0,"risk":"position_change","consequences":"Immediate canonical relocation only; no route or arrival guarantee",
                    "retractability":"A later move requires separate authority; this move is not undone by cancelling",
                    "recommendation":"Supported direct endpoint with no resource charge",
                    "required_authority":"current_agent_owner","valid_until_tick":valid_until_tick});
                candidate["candidate_digest"] = json!(digest(&candidate));
                candidates.push(candidate);
            }
            let state = if evaluation_unavailable {
                "evaluation_unavailable"
            } else if candidates.is_empty() {
                "no_safe_alternative"
            } else {
                "awaiting_confirmation"
            };
            return Ok((
                key.clone(),
                json!({"request_id":c.operation.request_id(),"agent_id":c.agent_id,"original":original,"original_digest":original_digest,
                "command_digests":{c.client_operation_id.clone():command_digest},"propose_signed":signed,"owner_public_key":signed.subject_public_key,"binding":c,"set_id":format!("{key}:generation:1"),"content_digest":digest(&json!(candidates)),"generation":1,"state":state,"blocker":if evaluation_unavailable {json!("module_evaluation_unavailable")} else {Value::Null},"valid_from_tick":now,"valid_until_tick":valid_until_tick,
                "initial_position":agent.state.pos,"policy_digest":digest(&json!(self.state.gameplay_policy)),
                "predecessor_request_id":predecessor_request_id,"original_no_effect":true,"candidates":candidates,"confirmation":null,"authorization":null,"outcome":null}),
            ));
        }
        let mut r = previous.cloned().ok_or("original_not_eligible")?;
        if r["owner_public_key"] != signed.subject_public_key {
            return Err("authority_changed".into());
        }
        match &c.operation {
            Op::Confirm {
                generation,
                candidate_id,
                candidate_digest,
                confirmation_id,
                valid_until_tick,
                ..
            } => {
                let confirmation = json!({"confirmation_id":confirmation_id,"candidate_id":candidate_id,"candidate_digest":candidate_digest,"generation":generation,"valid_until_tick":valid_until_tick,"signed":signed});
                if !r["confirmation"].is_null() {
                    if r["confirmation"] == confirmation {
                        return Ok((key, r));
                    }
                    return Err(
                        if r["confirmation"]["confirmation_id"] == *confirmation_id {
                            "identity_conflict"
                        } else {
                            "execution_already_bound"
                        }
                        .into(),
                    );
                }
                if r["state"] != "awaiting_confirmation" {
                    return Err("confirmation_rejected".into());
                }
                if Some(*generation) != r["generation"].as_u64()
                    || now >= r["valid_until_tick"].as_u64().unwrap_or(0)
                    || *valid_until_tick <= now
                    || *valid_until_tick > r["valid_until_tick"].as_u64().unwrap_or(0)
                {
                    return Err("candidate_stale".into());
                }
                let candidate = r["candidates"]
                    .as_array()
                    .and_then(|v| v.iter().find(|v| v["candidate_id"] == *candidate_id))
                    .ok_or("candidate_stale")?;
                if candidate["candidate_digest"] != *candidate_digest {
                    return Err("identity_conflict".into());
                }
                self.validate_alternative_scope(&r, candidate, now)?;
                r["confirmation"] = confirmation;
                r["authorization"] = json!({"authorization_id":format!("alternative-auth-{}",digest(&r["confirmation"])),"state":"available","execution_id":null,"action_id":null});
                r["state"] = json!("confirmed");
            }
            Op::Execute {
                authorization_id,
                execution_id,
                ..
            } => {
                if r["authorization"].is_null() {
                    return Err("confirmation_required".into());
                }
                if r["authorization"]["authorization_id"] != *authorization_id {
                    return Err("identity_conflict".into());
                }
                if !r["authorization"]["execution_id"].is_null() {
                    if r["authorization"]["execution_id"] == *execution_id {
                        return Ok((key, r));
                    }
                    return Err("execution_already_bound".into());
                }
                if r["state"] != "confirmed"
                    || now >= r["confirmation"]["valid_until_tick"].as_u64().unwrap_or(0)
                {
                    return Err("authorization_expired_or_revoked".into());
                }
                let candidate = r["candidates"]
                    .as_array()
                    .and_then(|v| {
                        v.iter()
                            .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                    })
                    .ok_or("candidate_stale")?;
                self.validate_alternative_scope(&r, candidate, now)?;
                let mut allocator = self.clone();
                let action_id = allocator.allocate_next_action_id();
                r["authorization"]["execution_id"] = json!(execution_id);
                r["authorization"]["action_id"] = json!(action_id);
                r["authorization"]["state"] = json!("bound");
                r["state"] = json!("execution_bound");
                r["execution_signed"] = json!(signed);
            }
            Op::Cancel { .. } => {
                if r["outcome"].is_null() {
                    r["state"] = json!("declined");
                    if !r["authorization"].is_null() {
                        r["authorization"]["state"] = json!("revoked");
                    }
                }
            }
            Op::Reevaluate {
                valid_until_tick, ..
            } => {
                if !r["authorization"]["execution_id"].is_null() {
                    return Err("execution_already_bound".into());
                }
                if *valid_until_tick <= now {
                    return Err("candidate_stale".into());
                }
                let generation = r["generation"].as_u64().unwrap_or(0) + 1;
                if generation > MAX_SAFE_ALTERNATIVE_GENERATIONS {
                    return Err("evaluation_unavailable".into());
                }
                let mut history = r["history"].as_array().cloned().unwrap_or_default();
                let mut old = r.clone();
                old.as_object_mut().unwrap().remove("history");
                history.push(old);
                let agent = self
                    .state
                    .agents
                    .get(&c.agent_id)
                    .ok_or("authority_changed")?;
                let evaluation_unavailable = !self.state.module_instances.is_empty()
                    || !self.module_registry.active.is_empty();
                r["generation"] = json!(generation);
                r["valid_until_tick"] = json!(valid_until_tick);
                r["valid_from_tick"] = json!(now);
                r["initial_position"] = json!(agent.state.pos);
                r["policy_digest"] = json!(digest(&json!(self.state.gameplay_policy)));
                let original: oasis7_client_api::safe_alternative::FineGrainedMoveRequestV1 =
                    serde_json::from_value(r["original"].clone()).map_err(|_| "outcome_unknown")?;
                let mut candidates = Vec::new();
                for to in original.waypoints {
                    if evaluation_unavailable {
                        continue;
                    }
                    let location = format!("{}:{}:{}", to.x_cm, to.y_cm, to.z_cm);
                    if self
                        .state
                        .gameplay_policy
                        .forbidden_location_ids
                        .contains(&location)
                        || candidates.iter().any(|v: &Value| v["to"] == json!(to))
                    {
                        continue;
                    }
                    let mut candidate = json!({"candidate_id":format!("generation-{generation}-endpoint-{}",candidates.len()+1),"to":to,
                        "goal":"Move directly to this endpoint; omit the requested path and arrival time",
                        "scope":{"agent_id":c.agent_id,"from":agent.state.pos,"to":to,"maximum_effects":1},
                        "cost_upper_bound":0,"risk":"position_change","consequences":"Immediate canonical relocation only; no route or arrival guarantee",
                        "retractability":"A later move requires separate authority; this move is not undone by cancelling",
                        "recommendation":"Supported direct endpoint with no resource charge","required_authority":"current_agent_owner","valid_until_tick":valid_until_tick});
                    candidate["candidate_digest"] = json!(digest(&candidate));
                    candidates.push(candidate);
                }
                r["reevaluation_signed"] = json!(signed);
                r["content_digest"] = json!(digest(&json!(candidates)));
                r["set_id"] = json!(format!("{key}:generation:{generation}"));
                r["candidates"] = json!(candidates);
                r["history"] = json!(history);
                r["confirmation"] = Value::Null;
                r["authorization"] = Value::Null;
                r["outcome"] = Value::Null;
                r["blocker"] = if evaluation_unavailable {
                    json!("module_evaluation_unavailable")
                } else {
                    Value::Null
                };
                r["state"] = json!(if evaluation_unavailable {
                    "evaluation_unavailable"
                } else if r["candidates"].as_array().unwrap().is_empty() {
                    "no_safe_alternative"
                } else {
                    "awaiting_confirmation"
                });
            }
            Op::Read { .. } | Op::Propose { .. } => {}
        }
        if r["command_digests"].is_null() {
            r["command_digests"] = json!({});
        }
        r["command_digests"][&c.client_operation_id] = json!(command_digest);
        if serde_json::to_vec(&r).map_err(|e| e.to_string())?.len()
            > MAX_SAFE_ALTERNATIVE_RECORD_BYTES
        {
            return Err("evaluation_unavailable".into());
        }
        Ok((key, r))
    }
    fn validate_alternative_scope(
        &self,
        r: &Value,
        candidate: &Value,
        now: u64,
    ) -> Result<(), String> {
        let c: SafeAlternativeCommandV1 =
            serde_json::from_value(r["binding"].clone()).map_err(|_| "outcome_unknown")?;
        let identity = self
            .current_agent_identity(&c.agent_id)
            .ok_or("authority_changed")?;
        let binding = self
            .current_cognition_runtime_binding()
            .map_err(|_| "authority_changed")?;
        let owner = self
            .state
            .resolved_agent_owner(&c.agent_id)
            .ok_or("authority_changed")?;
        if !self.state.module_instances.is_empty() || !self.module_registry.active.is_empty() {
            return Err("evaluation_unavailable".into());
        }
        if identity.owner_binding != c.owner_binding
            || identity.generation != c.owner_generation
            || owner.player_id != c.owner_binding
            || crate::service_admission::agent_authority::owner_public_key(self, &c.agent_id)
                .ok()
                .as_deref()
                != r["owner_public_key"].as_str()
            || binding.world_id != c.world_id
            || binding.branch_id != c.branch_id
            || binding.reorg_epoch != c.reorg_epoch
            || self
                .state
                .agents
                .get(&c.agent_id)
                .is_none_or(|a| json!(a.state.pos) != r["initial_position"])
            || self
                .state
                .gameplay_policy
                .forbidden_location_ids
                .contains(&format!(
                    "{}:{}:{}",
                    candidate["to"]["x_cm"], candidate["to"]["y_cm"], candidate["to"]["z_cm"]
                ))
        {
            return Err("authority_changed".into());
        }
        if now >= r["valid_until_tick"].as_u64().unwrap_or(0) || candidate["cost_upper_bound"] != 0
        {
            return Err("candidate_stale".into());
        }
        Ok(())
    }
    pub(super) fn install_safe_alternative_queue(&mut self, previous: Option<&Value>, r: &Value) {
        if !r["outcome"].is_null() {
            if let Some(action_id) = r["authorization"]["action_id"].as_u64() {
                self.pending_actions.retain(|a| a.id != action_id);
            }
            return;
        }
        if previous.is_some_and(|p| !p["authorization"]["execution_id"].is_null())
            || r["authorization"]["action_id"].is_null()
        {
            return;
        }
        if let (Some(action_id), Some(agent_id)) = (
            r["authorization"]["action_id"].as_u64(),
            r["agent_id"].as_str(),
        ) {
            let to = serde_json::from_value(
                r["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                    .unwrap()["to"]
                    .clone(),
            )
            .unwrap();
            let allocated = self.allocate_next_action_id();
            debug_assert_eq!(allocated, action_id);
            self.submit_action_with_id(
                action_id,
                Action::MoveAgent {
                    agent_id: agent_id.into(),
                    to,
                },
            );
        }
    }
    pub(super) fn safe_alternative_action_guard(
        &self,
        envelope: &ActionEnvelope,
    ) -> Result<(), String> {
        for r in self.capability_revocation_state.safe_alternatives.values() {
            if r["authorization"]["action_id"].as_u64() != Some(envelope.id) {
                continue;
            }
            if r["state"] != "execution_bound"
                || r["authorization"]["state"] != "bound"
                || self.state.time >= r["confirmation"]["valid_until_tick"].as_u64().unwrap_or(0)
            {
                return Err("authorization_expired_or_revoked".into());
            }
            let candidate = r["candidates"]
                .as_array()
                .and_then(|v| {
                    v.iter()
                        .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                })
                .ok_or("outcome_unknown")?;
            self.validate_alternative_scope(r, candidate, self.state.time)?;
            let expected = json!({"MoveAgent":{"agent_id":r["agent_id"],"to":candidate["to"]}});
            // Match the typed action rather than trusting arbitrary queued parameters.
            match &envelope.action {
                Action::MoveAgent { agent_id, to }
                    if json!(agent_id) == r["agent_id"] && json!(to) == candidate["to"] => {}
                _ => return Err(format!("identity_conflict:{expected}")),
            }
        }
        Ok(())
    }
    pub(super) fn safe_alternative_outcome_projection(
        &self,
        key: &str,
        action_id: u64,
        event_id: u64,
    ) -> Result<Value, WorldError> {
        let mut r = self
            .capability_revocation_state
            .safe_alternatives
            .get(key)
            .cloned()
            .ok_or_else(|| error("outcome_unknown"))?;
        if r["authorization"]["action_id"].as_u64() != Some(action_id) {
            return Err(error("identity_conflict"));
        }
        let event = self
            .journal
            .events
            .iter()
            .find(|e| {
                e.id == event_id
                    && matches!(e.caused_by,Some(CausedBy::Action(id)) if id==action_id)
            })
            .ok_or_else(|| error("outcome_unknown"))?;
        let (status, reason) = match &event.body {
            WorldEventBody::Domain(DomainEvent::AgentMoved { agent_id, from, to }) => {
                let candidate = r["candidates"]
                    .as_array()
                    .and_then(|v| {
                        v.iter()
                            .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                    })
                    .ok_or_else(|| error("outcome_unknown"))?;
                if json!(agent_id) != r["agent_id"]
                    || json!(from) != r["initial_position"]
                    || json!(to) != candidate["to"]
                {
                    return Err(error("identity_conflict"));
                }
                ("applied", Value::Null)
            }
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id: rejected_id,
                reason,
            }) if *rejected_id == action_id => ("rejected", json!(reason)),
            _ => return Err(error("outcome_unknown")),
        };
        let outcome =
            json!({"status":status,"action_id":action_id,"event_id":event_id,"reason":reason});
        if !r["outcome"].is_null() && r["outcome"] != outcome {
            return Err(error("identity_conflict"));
        }
        r["outcome"] = outcome;
        r["state"] = json!(status);
        r["authorization"]["state"] = json!(if status == "applied" {
            "consumed"
        } else {
            "revoked"
        });
        Ok(r)
    }
    pub(super) fn validate_safe_alternative_history(&self) -> Result<(), WorldError> {
        let mut action_ids = BTreeSet::new();
        for (key, r) in &self.capability_revocation_state.safe_alternatives {
            let fail = || error("invalid_safe_alternative_history");
            let signed: SignedReadRequest<SafeAlternativeCommandV1> =
                serde_json::from_value(r["propose_signed"].clone()).map_err(|_| fail())?;
            signed.request.validate().map_err(|_| fail())?;
            crate::service_admission::verify_read_request(signed.request.signing_domain(), &signed)
                .map_err(|_| fail())?;
            let Op::Propose { original, .. } = &signed.request.operation else {
                return Err(fail());
            };
            if lineage_key(&signed.request) != *key
                || r["binding"] != json!(signed.request)
                || r["owner_public_key"] != signed.subject_public_key
                || r["original"] != json!(original)
                || r["original_digest"] != digest(&json!(original))
                || r["original_no_effect"] != true
                || r["generation"].as_u64().is_none_or(|g| g == 0)
                || r["agent_id"] != signed.request.agent_id
            {
                return Err(fail());
            }
            let candidates = r["candidates"].as_array().ok_or_else(fail)?;
            if candidates.len() > 8 {
                return Err(fail());
            }
            for candidate in candidates {
                let mut contents = candidate.clone();
                let claimed = contents
                    .as_object_mut()
                    .ok_or_else(fail)?
                    .remove("candidate_digest")
                    .ok_or_else(fail)?;
                let to: oasis7_agent_api::world_action::GeoPos =
                    serde_json::from_value(candidate["to"].clone()).map_err(|_| fail())?;
                if claimed != digest(&contents)
                    || !original.waypoints.contains(&to)
                    || candidate["scope"]["agent_id"] != signed.request.agent_id
                    || candidate["scope"]["from"] != r["initial_position"]
                    || candidate["scope"]["to"] != candidate["to"]
                    || candidate["scope"]["maximum_effects"] != 1
                    || candidate["cost_upper_bound"] != 0
                    || candidate["valid_until_tick"] != r["valid_until_tick"]
                {
                    return Err(fail());
                }
            }
            if !r["confirmation"].is_null() {
                let confirmation: SignedReadRequest<SafeAlternativeCommandV1> =
                    serde_json::from_value(r["confirmation"]["signed"].clone())
                        .map_err(|_| fail())?;
                crate::service_admission::verify_read_request(
                    confirmation.request.signing_domain(),
                    &confirmation,
                )
                .map_err(|_| fail())?;
                let Op::Confirm {
                    generation,
                    candidate_id,
                    candidate_digest,
                    confirmation_id,
                    valid_until_tick,
                    ..
                } = &confirmation.request.operation
                else {
                    return Err(fail());
                };
                if lineage_key(&confirmation.request) != *key
                    || confirmation.subject_public_key != signed.subject_public_key
                    || Some(*generation) != r["generation"].as_u64()
                    || r["confirmation"]["confirmation_id"] != *confirmation_id
                    || r["confirmation"]["candidate_id"] != *candidate_id
                    || r["confirmation"]["candidate_digest"] != *candidate_digest
                    || r["confirmation"]["valid_until_tick"] != *valid_until_tick
                    || !candidates.iter().any(|v| {
                        v["candidate_id"] == *candidate_id
                            && v["candidate_digest"] == *candidate_digest
                    })
                    || r["authorization"]["authorization_id"]
                        != format!("alternative-auth-{}", digest(&r["confirmation"]))
                {
                    return Err(fail());
                }
            } else if !r["authorization"].is_null() {
                return Err(fail());
            }
            if let Some(action_id) = r["authorization"]["action_id"].as_u64() {
                if !action_ids.insert(action_id) {
                    return Err(fail());
                }
                let execution: SignedReadRequest<SafeAlternativeCommandV1> =
                    serde_json::from_value(r["execution_signed"].clone()).map_err(|_| fail())?;
                crate::service_admission::verify_read_request(
                    execution.request.signing_domain(),
                    &execution,
                )
                .map_err(|_| fail())?;
                let Op::Execute {
                    authorization_id,
                    execution_id,
                    ..
                } = &execution.request.operation
                else {
                    return Err(fail());
                };
                if lineage_key(&execution.request) != *key
                    || execution.subject_public_key != signed.subject_public_key
                    || r["authorization"]["execution_id"] != *execution_id
                    || r["authorization"]["authorization_id"] != *authorization_id
                {
                    return Err(fail());
                }
                if r["outcome"].is_null() {
                    let queued = self
                        .pending_actions
                        .iter()
                        .find(|a| a.id == action_id)
                        .ok_or_else(fail)?;
                    let candidate = candidates
                        .iter()
                        .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                        .ok_or_else(fail)?;
                    if !matches!(&queued.action,Action::MoveAgent{agent_id,to} if json!(agent_id)==r["agent_id"]&&json!(to)==candidate["to"])
                    {
                        return Err(fail());
                    }
                } else if self.pending_actions.iter().any(|a| a.id == action_id)
                    || r["outcome"]["action_id"] != action_id
                    || r["outcome"]["event_id"].as_u64().is_none()
                {
                    return Err(fail());
                }
            } else if !r["authorization"]["execution_id"].is_null() || !r["outcome"].is_null() {
                return Err(fail());
            }
        }
        Ok(())
    }
    pub(super) fn finalize_safe_alternative(
        &mut self,
        envelope: &ActionEnvelope,
        event_id: u64,
    ) -> Result<(), WorldError> {
        let key = self
            .capability_revocation_state
            .safe_alternatives
            .iter()
            .find(|(_, r)| r["authorization"]["action_id"].as_u64() == Some(envelope.id))
            .map(|(k, _)| k.clone());
        if let Some(key) = key {
            self.append_capability_authorization_event_batch(vec![
                crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
                    key,
                    action_id: envelope.id,
                    event_id,
                },
            ])?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "safe_alternative_tests.rs"]
mod tests;
