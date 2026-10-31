//! Local component only: no production route until approved V2 scope/prepayment exists.
use super::*;
use oasis7_client_api::world_service::resource_basis as wire;
#[path = "world_service_resource_adapter.rs"]
pub(crate) mod adapter;
#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ComponentReply {
    Goal(Box<wire::SignedResourceResponseV1<wire::OwnerSetupFinalResourceObservationV2>>),
    Owner(Box<wire::SignedResourceResponseV1<wire::ParticipantOwnerResourceObservationV1>>),
}
pub(super) fn handles(path: &str) -> bool {
    [
        wire::RESOURCE_GOAL_PATH_V2,
        wire::OWNER_RESOURCE_VIEW_PATH_V1,
    ]
    .contains(&path)
}
pub(super) fn process(
    service: &Service<'_>,
    store: Option<&PrivateGoalStore>,
    path: &str,
    body: &[u8],
    before_delivery: impl FnOnce(),
) -> Result<ComponentReply, PrivateError> {
    if body.len() > wire::MAX_RESOURCE_ENVELOPE_BYTES || service.guarded.is_some() {
        return Err(PrivateError::Forbidden);
    }
    match path {
        wire::RESOURCE_GOAL_PATH_V2 => read_goal(
            service,
            store.ok_or(PrivateError::Unavailable)?,
            parse(body).map_err(|_| PrivateError::Invalid)?,
            before_delivery,
        )
        .map(|response| ComponentReply::Goal(Box::new(response))),
        wire::OWNER_RESOURCE_VIEW_PATH_V1 => read_owner(
            service,
            parse(body).map_err(|_| PrivateError::Invalid)?,
            before_delivery,
        )
        .map(|response| ComponentReply::Owner(Box::new(response))),
        _ => Err(PrivateError::Forbidden),
    }
}
fn owner_state(
    service: &Service<'_>,
    pinned: &PinnedWorld,
    agent: &str,
) -> Result<wire::ResourceOwnerStateV1, PrivateError> {
    let owner = agent_chat::owner_view(&pinned.world, agent)?.ok_or(PrivateError::Forbidden)?;
    let control =
        game_task::current_control(&pinned.world, agent)?.ok_or(PrivateError::Forbidden)?;
    let record = pinned
        .world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(agent);
    Ok(wire::ResourceOwnerStateV1 {
        schema_revision: 1,
        world: (&service.identity).into(),
        read_commit: (&pinned.commit).into(),
        logical_tick: wire::DecimalU64(pinned.world.state().time),
        owner: wire::ResourceOwnerFenceV1 {
            agent_id: owner.agent_id,
            player_id: owner.player_id,
            public_key: owner.public_key,
            world_id: owner.world_id,
            reorg_epoch: wire::DecimalU64(owner.reorg_epoch),
            branch_id: owner.canonical_authority.branch_id,
            agent_identity_generation: wire::DecimalU64(
                owner.canonical_authority.agent_identity_generation,
            ),
            current_intent_id: owner.current_intent_id,
        },
        control,
        signer_delegation: record.map(|r| wire::ResourceSignerRecordV1 {
            world_id: r.world_id.clone(),
            branch_id: r.branch_id.clone(),
            agent_id: r.agent_id.clone(),
            owner_binding: r.owner_binding.clone(),
            agent_identity_generation: wire::DecimalU64(r.agent_identity_generation),
            generation: wire::DecimalU64(r.generation),
            delegate_public_key: r.delegate_public_key.clone(),
            revoked: r.revoked,
            nonce: wire::DecimalU64(r.nonce),
        }),
    })
}
fn sign<Q: Serialize, T: Serialize + Clone>(
    service: &Service<'_>,
    path: &str,
    q: &Q,
    payload: T,
) -> Result<wire::SignedResourceResponseV1<T>, PrivateError> {
    let response: wire::SignedResourceResponseV1<T> =
        oasis7_client_api::world_service::sign_service_response(
            path,
            request_digest(path, q)?,
            payload,
            &service.signer.private_key_hex,
        )?
        .into();
    // Measure the actual final Native envelope, not an estimated wrapper allowance.
    wire::encode_native_resource_envelope(response.clone())?;
    Ok(response)
}
fn same_pin(service: &Service<'_>, first: &PinnedWorld) -> Result<(), PrivateError> {
    let fresh = service.pin(None)?;
    if fresh.commit != first.commit {
        return Err(PrivateError::Conflict);
    }
    Ok(())
}
fn failure(stage: u8) {
    eprintln!("resource_basis_component_failure stage={stage}");
}
fn failed(stage: u8) -> PrivateError {
    failure(stage);
    PrivateError::Forbidden
}
fn read_goal(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    q: wire::SignedResourceReadV1<wire::OwnerSetupFinalResourceReadV2>,
    before_delivery: impl FnOnce(),
) -> Result<wire::SignedResourceResponseV1<wire::OwnerSetupFinalResourceObservationV2>, PrivateError>
{
    wire::verify_resource_goal_request(&q).map_err(|_| failed(1))?;
    if oasis7_client_api::world_service::WorldIdentity::from(q.request.world.clone())
        != service.identity
    {
        return Err(failed(2));
    }
    let r = &q.request;
    let fence = &r.expected_fence;
    let (pinned, control) = current(
        service,
        &fence.agent_id,
        &fence.task_id,
        &q.subject_public_key,
    )
    .inspect_err(|_| failure(3))?;
    let owner = owner_state(service, &pinned, &fence.agent_id).inspect_err(|_| failure(4))?;
    owner
        .validate(fence, &q.subject_public_key, &r.expected_signer_delegation)
        .map_err(|_| failed(5))?;
    let (activation_commit, active_goal) =
        current_goal_history(service, store, &pinned, &fence.agent_id).inspect_err(|_| failure(6))?;
    let current_goal = CurrentGoalResponse {
        schema_revision: 1,
        control,
        read_commit: pinned.commit.clone(),
        activation_commit,
        active_goal,
    };
    let resource =
        adapter::resolution(&pinned.world, &fence.agent_id, &r.selection).inspect_err(|_| failure(7))?;
    let payload = wire::OwnerSetupFinalResourceObservationV2 {
        api_version: 2,
        owner,
        current_goal: (&current_goal).into(),
        selection: r.selection.clone(),
        resource,
    };
    before_delivery();
    same_pin(service, &pinned).inspect_err(|_| failure(8))?;
    let response = sign(service, wire::RESOURCE_GOAL_PATH_V2, &q, payload).inspect_err(|_| failure(9))?;
    wire::verify_resource_goal_response(&q, &response, &service.signer.public_key_hex)
        .map_err(|_| {
            failure(10);
            if let wire::ResourceBasisResolutionV1::Ready { fact } = &response.payload.resource {
                let valid = fact.validate(&r.selection, &response.payload.owner).is_ok();
                if let wire::ResourceBasisQuoteV1::Transfer(v) = &fact.quote {
                    eprintln!("resource_basis_component_diagnostic fact_valid={} tick={} ticks={} ready={} runtime_ready={}", u8::from(valid), fact.quoted_at_tick.0, v.quote.ticks_until_arrival.0, v.quote.ready_at.0, fact.quoted_at_tick.0.saturating_add(1).saturating_add(v.quote.ticks_until_arrival.0));
                }
            }
            PrivateError::Forbidden
        })?;
    Ok(response)
}
fn read_owner(
    service: &Service<'_>,
    q: wire::ParticipantOwnerResourceViewRequestV1,
    before_delivery: impl FnOnce(),
) -> Result<wire::SignedResourceResponseV1<wire::ParticipantOwnerResourceObservationV1>, PrivateError>
{
    wire::verify_owner_resource_request(&q).map_err(|_| PrivateError::Forbidden)?;
    if oasis7_client_api::world_service::WorldIdentity::from(q.read.request.world.clone())
        != service.identity
    {
        return Err(PrivateError::Forbidden);
    }
    let pinned = service.pin(None)?;
    let binding = &pinned.commit.binding;
    oasis7_runtime::service_admission::participant::validate_admission(
        &pinned.world,
        &q.owner.legacy(),
        &q.admission.legacy(),
        &binding.branch_id,
        binding.reorg_generation,
    )
    .map_err(|_| PrivateError::Forbidden)?;
    let key = oasis7_runtime::service_admission::participant::account_key(&q.owner.request)?;
    let participant = pinned
        .world
        .capability_revocation_state()
        .participants
        .bindings
        .get(&key)
        .ok_or(PrivateError::Forbidden)?;
    let r = &q.read.request;
    if participant.agent_id != r.expected_fence.agent_id
        || participant.owner_public_key != q.read.subject_public_key
    {
        return Err(PrivateError::Forbidden);
    }
    let owner = owner_state(service, &pinned, &participant.agent_id)?;
    owner
        .validate(
            &r.expected_fence,
            &q.read.subject_public_key,
            &r.expected_signer_delegation,
        )
        .map_err(|_| PrivateError::Forbidden)?;
    let resource = adapter::resolution(&pinned.world, &participant.agent_id, &r.selection)?;
    let payload = wire::ParticipantOwnerResourceObservationV1 {
        api_version: 1,
        owner,
        selection: r.selection.clone(),
        resource,
    };
    before_delivery();
    same_pin(service, &pinned)?;
    let response = sign(service, wire::OWNER_RESOURCE_VIEW_PATH_V1, &q, payload)?;
    wire::verify_owner_resource_response(&q, &response, &service.signer.public_key_hex)
        .map_err(|_| PrivateError::Forbidden)?;
    Ok(response)
}

#[cfg(test)]
#[path = "world_service_resource_adapter_tests.rs"]
mod adapter_tests;
