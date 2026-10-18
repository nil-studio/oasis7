use oasis7_proto::player_auth::{
    verify_collect_data_auth_proof, verify_gameplay_action_auth_proof,
};
use oasis7_proto::viewer::{CollectDataCommand, GameplayActionError, GameplayActionRequest};
use oasis7_runtime::{Action, World, WorldState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectDataQualificationError {
    OwnerClaimMissing,
    OwnerClaimAmbiguous,
}

impl CollectDataQualificationError {
    pub fn code(self) -> &'static str {
        match self {
            Self::OwnerClaimMissing => "collect_data_owner_claim_missing",
            Self::OwnerClaimAmbiguous => "collect_data_owner_claim_ambiguous",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::OwnerClaimMissing => "canonical Agent owner claim unavailable",
            Self::OwnerClaimAmbiguous => "ambiguous Agent owner claim",
        }
    }
}

/// Resolve the same canonical player/key qualification for a read-only quote,
/// submission admission and execution. This does not consume an auth nonce or
/// mutate the world; each caller retains its existing authentication rules.
pub fn resolve_collect_data_collector<'a>(
    state: &'a WorldState,
    player_id: &str,
    public_key: &str,
) -> Result<&'a str, CollectDataQualificationError> {
    let mut claims = state.starter_oc_claims.values().filter(|claim| {
        claim.player_id == player_id && claim.public_key.as_deref() == Some(public_key)
    });
    let claim = claims
        .next()
        .ok_or(CollectDataQualificationError::OwnerClaimMissing)?;
    if claims.next().is_some() {
        return Err(CollectDataQualificationError::OwnerClaimAmbiguous);
    }
    Ok(claim.agent_id.as_str())
}

/// A consumed legacy nonce alone does not identify the historical effect or
/// receipt. Callers must prefer a real service correlation record, if present.
pub fn consumed_collect_data_nonce(world: &World, bytes: &[u8]) -> Result<bool, String> {
    // Generic gameplay has a different nonce authority; do not infer its
    // outcome from the durable CollectData player/key high-water mark.
    if serde_json::from_slice::<GameplayActionRequest>(bytes).is_ok() {
        return Ok(false);
    }
    let command: CollectDataCommand = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let CollectDataCommand::Submit { request } = &command else {
        return Err("unsupported collect_data".into());
    };
    let proof = request
        .auth
        .as_ref()
        .ok_or("missing collect_data signature")?;
    let verified = verify_collect_data_auth_proof(&command, proof)?;
    Ok(world
        .state()
        .authenticated_collect_data_last_nonces
        .get(&verified.player_id)
        .and_then(|nonces| nonces.get(&verified.public_key))
        .is_some_and(|last| verified.nonce <= *last))
}

pub fn authenticated_action(
    world: &World,
    bytes: &[u8],
    build_action: impl FnOnce(&GameplayActionRequest) -> Result<Action, GameplayActionError>,
) -> Result<Action, String> {
    authenticated_action_with_origin(world, bytes, build_action).map(|(action, _)| action)
}

/// Preserve verified browser provenance for committed recipe submission.
pub fn authenticated_action_with_origin(
    world: &World,
    bytes: &[u8],
    build_action: impl FnOnce(&GameplayActionRequest) -> Result<Action, GameplayActionError>,
) -> Result<(Action, Option<oasis7_runtime::GameplaySubmissionOrigin>), String> {
    if let Ok(request) = serde_json::from_slice::<GameplayActionRequest>(bytes) {
        let proof = request.auth.as_ref().ok_or("missing gameplay signature")?;
        let verified = verify_gameplay_action_auth_proof(&request, proof)?;
        let action = build_action(&request).map_err(|e| e.message)?;
        let origin = match &action {
            Action::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                ..
            } => Some(oasis7_runtime::GameplaySubmissionOrigin {
                verified_player_id: verified.player_id,
                public_key: verified.public_key,
                auth_nonce: verified.nonce,
                hosted_registration_nonce: verified.hosted_registration_nonce,
                requester_agent_id: requester_agent_id.clone(),
                factory_id: factory_id.clone(),
                recipe_id: recipe_id.clone(),
            }),
            _ => None,
        };
        return Ok((action, origin));
    }
    let command: CollectDataCommand = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let CollectDataCommand::Submit { request } = &command else {
        return Err("unsupported collect_data".into());
    };
    let proof = request
        .auth
        .as_ref()
        .ok_or("missing collect_data signature")?;
    let verified = verify_collect_data_auth_proof(&command, proof)?;
    let collector_agent_id =
        resolve_collect_data_collector(world.state(), &verified.player_id, &verified.public_key)
            .map_err(|error| error.message().to_owned())?;
    Ok((
        Action::CollectDataAuthenticated {
            collector_agent_id: collector_agent_id.to_owned(),
            electricity_cost: request.electricity_cost,
            data_amount: request.data_amount,
            player_id: verified.player_id,
            public_key: verified.public_key,
            nonce: verified.nonce,
            signature: proof.signature.clone(),
        },
        None,
    ))
}

#[cfg(test)]
mod qualification_tests {
    use super::*;

    #[test]
    fn collect_data_qualification_is_unique_subject_bound_and_read_only() {
        let mut state = WorldState::default();
        assert_eq!(
            resolve_collect_data_collector(&state, "player-a", "key-a"),
            Err(CollectDataQualificationError::OwnerClaimMissing)
        );
        state.starter_oc_claims.insert("agent-a".into(), serde_json::from_value(serde_json::json!({
            "agent_id":"agent-a", "player_id":"player-a", "public_key":"key-a", "amount":0, "claimed_at":0
        })).unwrap());
        let claim = state.starter_oc_claims["agent-a"].clone();
        let before = serde_json::to_value(&state).unwrap();
        for _ in 0..2 {
            assert_eq!(
                resolve_collect_data_collector(&state, "player-a", "key-a"),
                Ok("agent-a")
            );
        }
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert_eq!(
            resolve_collect_data_collector(&state, "player-a", "other-key"),
            Err(CollectDataQualificationError::OwnerClaimMissing)
        );
        assert_eq!(
            resolve_collect_data_collector(&state, "other-player", "key-a"),
            Err(CollectDataQualificationError::OwnerClaimMissing)
        );
        state.starter_oc_claims.insert("duplicate".into(), claim);
        assert_eq!(
            resolve_collect_data_collector(&state, "player-a", "key-a"),
            Err(CollectDataQualificationError::OwnerClaimAmbiguous)
        );
    }
}
