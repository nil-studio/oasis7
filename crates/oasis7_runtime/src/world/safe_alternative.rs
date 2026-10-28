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
        let command = &signed.request;
        let key = lineage_key(command);
        let now = self.state.time;
        let previous = self.capability_revocation_state.safe_alternatives.get(&key);
        let command_digest = digest(&json!(command));
        if let Some(old) = previous
            && let Some(recorded) = old["command_digests"].get(&command.client_operation_id)
        {
            if *recorded != command_digest {
                return Err("identity_conflict".into());
            }
            return Ok((key, old.clone()));
        }

        match &command.operation {
            Op::Propose { .. } => {
                self.project_safe_alternative_proposal(signed, key, previous, now, &command_digest)
            }
            _ => self.project_safe_alternative_existing_operation(
                signed,
                key,
                previous,
                now,
                &command_digest,
            ),
        }
    }

    fn project_safe_alternative_proposal(
        &self,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
        key: String,
        previous: Option<&Value>,
        now: u64,
        command_digest: &str,
    ) -> Result<(String, Value), String> {
        let command = &signed.request;
        let Op::Propose {
            original,
            valid_until_tick,
            predecessor_request_id,
            ..
        } = &command.operation
        else {
            unreachable!("proposal routed to proposal projector")
        };
        let original_digest = digest(&serde_json::to_value(original).map_err(|e| e.to_string())?);
        if let Some(old) = previous {
            if old["original_digest"] != original_digest
                || old["binding"]["operation"] != json!(command.operation)
            {
                return Err("identity_conflict".into());
            }
            return Ok((key, old.clone()));
        }
        if self.capability_revocation_state.safe_alternatives.len() >= MAX_SAFE_ALTERNATIVE_LINEAGES
        {
            return Err("evaluation_unavailable".into());
        }
        if *valid_until_tick <= now {
            return Err("candidate_stale".into());
        }
        let evaluation_unavailable =
            !self.state.module_instances.is_empty() || !self.module_registry.active.is_empty();
        if let Some(predecessor) = predecessor_request_id {
            let mut predecessor_command = command.clone();
            predecessor_command.operation = Op::Read {
                request_id: predecessor.clone(),
            };
            let valid = self
                .capability_revocation_state
                .safe_alternatives
                .get(&lineage_key(&predecessor_command))
                .is_some_and(|record| record["outcome"]["status"] == "rejected");
            if !valid {
                return Err("original_outcome_unknown".into());
            }
        }
        let agent = self
            .state
            .agents
            .get(&command.agent_id)
            .ok_or("authority_changed")?;
        let candidates = self.build_safe_alternative_candidates(
            &command.agent_id,
            &agent.state.pos,
            &original.waypoints,
            None,
            *valid_until_tick,
            evaluation_unavailable,
        );
        let state = if evaluation_unavailable {
            "evaluation_unavailable"
        } else if candidates.is_empty() {
            "no_safe_alternative"
        } else {
            "awaiting_confirmation"
        };
        Ok((
            key.clone(),
            json!({"request_id":command.operation.request_id(),"agent_id":command.agent_id,"original":original,"original_digest":original_digest,
            "command_digests":{command.client_operation_id.clone():command_digest},"propose_signed":signed,"owner_public_key":signed.subject_public_key,"binding":command,"set_id":format!("{key}:generation:1"),"content_digest":digest(&json!(candidates)),"generation":1,"state":state,"blocker":if evaluation_unavailable {json!("module_evaluation_unavailable")} else {Value::Null},"valid_from_tick":now,"valid_until_tick":valid_until_tick,
            "initial_position":agent.state.pos,"policy_digest":digest(&json!(self.state.gameplay_policy)),
            "predecessor_request_id":predecessor_request_id,"original_no_effect":true,"candidates":candidates,"confirmation":null,"authorization":null,"outcome":null}),
        ))
    }

    fn project_safe_alternative_existing_operation(
        &self,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
        key: String,
        previous: Option<&Value>,
        now: u64,
        command_digest: &str,
    ) -> Result<(String, Value), String> {
        let command = &signed.request;
        let mut record = previous.cloned().ok_or("original_not_eligible")?;
        if record["owner_public_key"] != signed.subject_public_key {
            return Err("authority_changed".into());
        }

        match &command.operation {
            Op::Confirm { .. } => {
                if self.apply_safe_alternative_confirmation(&mut record, signed, now)? {
                    return Ok((key, record));
                }
            }
            Op::Execute {
                authorization_id,
                execution_id,
                ..
            } => {
                if self.apply_safe_alternative_execution(
                    &mut record,
                    signed,
                    now,
                    authorization_id,
                    execution_id,
                )? {
                    return Ok((key, record));
                }
            }
            Op::Cancel { .. } => Self::apply_safe_alternative_cancellation(&mut record),
            Op::Reevaluate {
                valid_until_tick, ..
            } => self.reevaluate_safe_alternative(
                &mut record,
                signed,
                command,
                &key,
                now,
                *valid_until_tick,
            )?,
            Op::Read { .. } => {}
            Op::Propose { .. } => {
                unreachable!("proposal routed to proposal projector")
            }
        }

        if record["command_digests"].is_null() {
            record["command_digests"] = json!({});
        }
        record["command_digests"][&command.client_operation_id] = json!(command_digest);
        if serde_json::to_vec(&record)
            .map_err(|e| e.to_string())?
            .len()
            > MAX_SAFE_ALTERNATIVE_RECORD_BYTES
        {
            return Err("evaluation_unavailable".into());
        }
        Ok((key, record))
    }

    fn apply_safe_alternative_confirmation(
        &self,
        record: &mut Value,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
        now: u64,
    ) -> Result<bool, String> {
        let Op::Confirm {
            generation,
            candidate_id,
            candidate_digest,
            confirmation_id,
            valid_until_tick,
            ..
        } = &signed.request.operation
        else {
            unreachable!("confirmation routed to confirmation projector")
        };
        let generation = *generation;
        let valid_until_tick = *valid_until_tick;
        let candidate_id = candidate_id.as_str();
        let candidate_digest = candidate_digest.as_str();
        let confirmation_id = confirmation_id.as_str();
        let confirmation = json!({"confirmation_id":confirmation_id,"candidate_id":candidate_id,"candidate_digest":candidate_digest,"generation":generation,"valid_until_tick":valid_until_tick,"signed":signed});
        if !record["confirmation"].is_null() {
            if record["confirmation"] == confirmation {
                return Ok(true);
            }
            return Err(
                if record["confirmation"]["confirmation_id"] == confirmation_id {
                    "identity_conflict"
                } else {
                    "execution_already_bound"
                }
                .into(),
            );
        }
        if record["state"] != "awaiting_confirmation" {
            return Err("confirmation_rejected".into());
        }
        if Some(generation) != record["generation"].as_u64()
            || now >= record["valid_until_tick"].as_u64().unwrap_or(0)
            || valid_until_tick <= now
            || valid_until_tick > record["valid_until_tick"].as_u64().unwrap_or(0)
        {
            return Err("candidate_stale".into());
        }
        let candidate = record["candidates"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["candidate_id"] == candidate_id)
            })
            .ok_or("candidate_stale")?;
        if candidate["candidate_digest"] != candidate_digest {
            return Err("identity_conflict".into());
        }
        self.validate_alternative_scope(record, candidate, now)?;
        record["confirmation"] = confirmation;
        record["authorization"] = json!({"authorization_id":format!("alternative-auth-{}",digest(&record["confirmation"])),"state":"available","execution_id":null,"action_id":null});
        record["state"] = json!("confirmed");
        Ok(false)
    }

    fn apply_safe_alternative_execution(
        &self,
        record: &mut Value,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
        now: u64,
        authorization_id: &str,
        execution_id: &str,
    ) -> Result<bool, String> {
        if record["authorization"].is_null() {
            return Err("confirmation_required".into());
        }
        if record["authorization"]["authorization_id"] != authorization_id {
            return Err("identity_conflict".into());
        }
        if !record["authorization"]["execution_id"].is_null() {
            if record["authorization"]["execution_id"] == execution_id {
                return Ok(true);
            }
            return Err("execution_already_bound".into());
        }
        if record["state"] != "confirmed"
            || now
                >= record["confirmation"]["valid_until_tick"]
                    .as_u64()
                    .unwrap_or(0)
        {
            return Err("authorization_expired_or_revoked".into());
        }
        let candidate = record["candidates"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["candidate_id"] == record["confirmation"]["candidate_id"])
            })
            .ok_or("candidate_stale")?;
        self.validate_alternative_scope(record, candidate, now)?;
        let mut allocator = self.clone();
        let action_id = allocator.allocate_next_action_id();
        record["authorization"]["execution_id"] = json!(execution_id);
        record["authorization"]["action_id"] = json!(action_id);
        record["authorization"]["state"] = json!("bound");
        record["state"] = json!("execution_bound");
        record["execution_signed"] = json!(signed);
        Ok(false)
    }

    fn apply_safe_alternative_cancellation(record: &mut Value) {
        if record["outcome"].is_null() {
            record["state"] = json!("declined");
            if !record["authorization"].is_null() {
                record["authorization"]["state"] = json!("revoked");
            }
        }
    }

    fn reevaluate_safe_alternative(
        &self,
        record: &mut Value,
        signed: &SignedReadRequest<SafeAlternativeCommandV1>,
        command: &SafeAlternativeCommandV1,
        key: &str,
        now: u64,
        valid_until_tick: u64,
    ) -> Result<(), String> {
        if !record["authorization"]["execution_id"].is_null() {
            return Err("execution_already_bound".into());
        }
        if valid_until_tick <= now {
            return Err("candidate_stale".into());
        }
        let generation = record["generation"].as_u64().unwrap_or(0) + 1;
        if generation > MAX_SAFE_ALTERNATIVE_GENERATIONS {
            return Err("evaluation_unavailable".into());
        }
        let mut history = record["history"].as_array().cloned().unwrap_or_default();
        let mut old = record.clone();
        old.as_object_mut().unwrap().remove("history");
        history.push(old);
        let agent = self
            .state
            .agents
            .get(&command.agent_id)
            .ok_or("authority_changed")?;
        let evaluation_unavailable =
            !self.state.module_instances.is_empty() || !self.module_registry.active.is_empty();
        record["generation"] = json!(generation);
        record["valid_until_tick"] = json!(valid_until_tick);
        record["valid_from_tick"] = json!(now);
        record["initial_position"] = json!(agent.state.pos);
        record["policy_digest"] = json!(digest(&json!(self.state.gameplay_policy)));
        let original: oasis7_client_api::safe_alternative::FineGrainedMoveRequestV1 =
            serde_json::from_value(record["original"].clone()).map_err(|_| "outcome_unknown")?;
        let candidates = self.build_safe_alternative_candidates(
            &command.agent_id,
            &agent.state.pos,
            &original.waypoints,
            Some(generation),
            valid_until_tick,
            evaluation_unavailable,
        );
        record["reevaluation_signed"] = json!(signed);
        record["content_digest"] = json!(digest(&json!(candidates)));
        record["set_id"] = json!(format!("{key}:generation:{generation}"));
        record["candidates"] = json!(candidates);
        record["history"] = json!(history);
        record["confirmation"] = Value::Null;
        record["authorization"] = Value::Null;
        record["outcome"] = Value::Null;
        record["blocker"] = if evaluation_unavailable {
            json!("module_evaluation_unavailable")
        } else {
            Value::Null
        };
        record["state"] = json!(if evaluation_unavailable {
            "evaluation_unavailable"
        } else if record["candidates"].as_array().unwrap().is_empty() {
            "no_safe_alternative"
        } else {
            "awaiting_confirmation"
        });
        Ok(())
    }

    fn build_safe_alternative_candidates(
        &self,
        agent_id: &str,
        from: &oasis7_agent_api::world_action::GeoPos,
        waypoints: &[oasis7_agent_api::world_action::GeoPos],
        generation: Option<u64>,
        valid_until_tick: u64,
        evaluation_unavailable: bool,
    ) -> Vec<Value> {
        let mut candidates = Vec::new();
        for to in waypoints {
            if evaluation_unavailable {
                continue;
            }
            let location = format!("{}:{}:{}", to.x_cm, to.y_cm, to.z_cm);
            if self
                .state
                .gameplay_policy
                .forbidden_location_ids
                .contains(&location)
                || candidates
                    .iter()
                    .any(|candidate: &Value| candidate["to"] == json!(to))
            {
                continue;
            }
            let candidate_id = match generation {
                Some(generation) => {
                    format!("generation-{generation}-endpoint-{}", candidates.len() + 1)
                }
                None => format!("endpoint-{}", candidates.len() + 1),
            };
            let mut candidate = json!({"candidate_id":candidate_id,"to":to,
                "goal":"Move directly to this endpoint; omit the requested path and arrival time",
                "scope":{"agent_id":agent_id,"from":from,"to":to,"maximum_effects":1},
                "cost_upper_bound":0,"risk":"position_change","consequences":"Immediate canonical relocation only; no route or arrival guarantee",
                "retractability":"A later move requires separate authority; this move is not undone by cancelling",
                "recommendation":"Supported direct endpoint with no resource charge",
                "required_authority":"current_agent_owner","valid_until_tick":valid_until_tick});
            candidate["candidate_digest"] = json!(digest(&candidate));
            candidates.push(candidate);
        }
        candidates
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
            .current_agent_owner(&c.agent_id)
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
