//! Original formal audience rules over fixed Legacy/Current source adapters.
use crate::{World, service_admission::WorldServicePayloadV1};
use oasis7_client_api::world_service::SubmitIntentRequest;
pub(super) trait Facts {
    fn participant(&mut self, account: &str) -> Result<Option<String>, String>;
    fn collect_data(&mut self, player: &str, key: &str) -> Result<String, String>;
}
pub(super) enum Query<'a> {
    Participant(&'a str),
    CollectData { player: &'a str, key: &'a str },
}
pub(super) struct CurrentQueries<F>(pub F);
impl<F: for<'a> FnMut(Query<'a>) -> Result<Option<String>, String>> Facts for CurrentQueries<F> {
    fn participant(&mut self, account: &str) -> Result<Option<String>, String> {
        (self.0)(Query::Participant(account))
    }
    fn collect_data(&mut self, player: &str, key: &str) -> Result<String, String> {
        (self.0)(Query::CollectData { player, key })?
            .ok_or_else(|| "formal result CollectData owner Agent claim is unavailable".into())
    }
}
struct Legacy<'a>(&'a World);
impl Facts for Legacy<'_> {
    fn participant(&mut self, key: &str) -> Result<Option<String>, String> {
        Ok(self
            .0
            .capability_revocation_state()
            .participants
            .bindings
            .get(key)
            .map(|r| r.agent_id.clone()))
    }
    fn collect_data(&mut self, player: &str, key: &str) -> Result<String, String> {
        let mut claims = self
            .0
            .state()
            .starter_oc_claims
            .values()
            .filter(|r| r.player_id == player && r.public_key.as_deref() == Some(key));
        let claim = claims
            .next()
            .ok_or("formal result CollectData owner Agent claim is unavailable")?;
        if claims.next().is_some() {
            return Err("formal result CollectData owner Agent claim is ambiguous".into());
        }
        Ok(claim.agent_id.clone())
    }
}
impl World {
    /// Original audience DATA, not a read authorization or a Current source.
    pub fn formal_business_scope_for_request_v1(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<String, String> {
        for_request(&mut Legacy(self), request)
    }
    pub fn formal_business_outcome_agent_id_v1(
        &self,
        payload: &WorldServicePayloadV1,
    ) -> Result<String, String> {
        outcome_agent_id(&mut Legacy(self), payload)
    }
}
pub(super) fn for_request(
    facts: &mut impl Facts,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<String, String> {
    if let WorldServicePayloadV1::GameplayJson(bytes) = &request.signed_payload
        && let Ok(action) =
            serde_json::from_slice::<oasis7_proto::viewer::GameplayActionRequest>(bytes)
        && action.action_id == "claim_first_agent"
        && action.target_agent_id == "starter-agent-0"
    {
        let derived = crate::service_admission::correlation::derive_correlation(
            request.correlation.key.world.clone(),
            &request.signed_payload,
        )?;
        if derived != request.correlation {
            return Err("bootstrap result differs from its authenticated original request".into());
        }
        return Ok("public".into());
    }
    Ok(format!(
        "agent:{}",
        outcome_agent_id(facts, &request.signed_payload)?
    ))
}
pub(super) fn outcome_agent_id(
    facts: &mut impl Facts,
    payload: &WorldServicePayloadV1,
) -> Result<String, String> {
    match payload {
        WorldServicePayloadV1::GameStarterClaim(signed) => Ok(signed.request.agent_id.clone()),
        WorldServicePayloadV1::GameParticipantRegister { owner, .. } => {
            let key = crate::service_admission::participant::account_key(&owner.request)?;
            facts
                .participant(&key)?
                .ok_or_else(|| "participant binding unavailable".into())
        }
        WorldServicePayloadV1::SafeAlternative(signed) => Ok(signed.request.agent_id.clone()),
        WorldServicePayloadV1::AgentChat(request) => Ok(request.agent_id.clone()),
        WorldServicePayloadV1::Cognition(request) => Ok(request.request.request.agent_id.clone()),
        WorldServicePayloadV1::FeedbackAck(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::Delegation(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::Scheduler(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::GameTaskCreate(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::GameGoalPublish(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::GameExecutorControl(request) => {
            Ok(request.request.fence.agent_id.clone())
        }
        WorldServicePayloadV1::GameplayJson(bytes) => {
            if let Ok(request) =
                serde_json::from_slice::<oasis7_proto::viewer::GameplayActionRequest>(bytes)
            {
                if request.target_agent_id.trim().is_empty()
                    || request.target_agent_id != request.target_agent_id.trim()
                {
                    return Err("formal result gameplay target Agent identity is invalid".into());
                }
                return Ok(request.target_agent_id);
            }
            let command: oasis7_proto::viewer::CollectDataCommand =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            let oasis7_proto::viewer::CollectDataCommand::Submit { request } = &command else {
                return Err("formal result cannot publish a non-submit CollectData outcome".into());
            };
            let proof = request
                .auth
                .as_ref()
                .ok_or("formal result CollectData request has no verified owner proof")?;
            let verified =
                oasis7_proto::player_auth::verify_collect_data_auth_proof(&command, proof)?;
            facts.collect_data(&verified.player_id, &verified.public_key)
        }
    }
}
