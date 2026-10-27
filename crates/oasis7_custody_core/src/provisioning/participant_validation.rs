use super::*;
use oasis7_agent_api::game::{GAME_PARTICIPANT_ADMISSION_DOMAIN, GAME_PARTICIPANT_REGISTER_DOMAIN};
use oasis7_client_api::world_service::{
    participant_discovery::PARTICIPANT_OWNER_VIEW_PATH, private_goal::verify_owner_signature_strict,
};
pub(super) fn policy(o: &ParticipantOriginalV1, p: &ParticipantPolicyOriginalV1) -> Result<()> {
    p.verify_strict(
        &o.descriptor.policy_nonce,
        &world(&o.descriptor),
        &o.descriptor.world.service_public_key,
    )
    .map_err(|_| bad())?;
    let v = &p.response.payload;
    let policy = &v["policy"];
    let d = &o.descriptor;
    let namespace: oasis7_agent_api::game::GameIdentityNamespaceV1 =
        serde_json::from_value(policy["identity_namespace"].clone()).map_err(|_| bad())?;
    if v["world_id"] != d.world.world_id
        || v["genesis_digest"] != d.world.genesis_digest
        || v["branch_id"] != d.world.branch_id
        || amount(&v["reorg_epoch"]) != Some(d.world.reorg_epoch.0)
        || policy["world_id"] != d.world.world_id
        || policy["genesis_digest"] != d.world.genesis_digest
        || amount(&policy["api_rev"]) != Some(1)
        || namespace != d.register.identity_namespace
        || policy["enabled"] != true
        || policy["registry_complete"] != true
        || amount(&policy["generation"]).is_none_or(|n| n == 0)
        || amount(&policy["max_grant_ticks"]).is_none_or(|n| n == 0 || n > 256)
        || amount(&v["logical_tick"]).is_none()
    {
        return Err(bad());
    }
    Ok(())
}
pub(super) fn issuer(o: &ParticipantOriginalV1) -> Result<&str> {
    o.policy
        .as_ref()
        .and_then(|p| p.response.payload["policy"]["issuer_public_key"].as_str())
        .ok_or_else(bad)
}
pub(super) fn bundle(o: &ParticipantOriginalV1, b: &ParticipantSignBundleV1) -> Result<()> {
    let d = &o.descriptor;
    if b.owner.request != d.register
        || b.read.request != d.owner_read
        || b.owner.subject_public_key != d.owner_key.public_key
        || b.read.subject_public_key != d.owner_key.public_key
    {
        return Err(bad());
    }
    verify_owner_signature_strict(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        &b.owner.request,
        &d.owner_key.public_key,
        &b.owner.signature_hex,
    )
    .map_err(|_| bad())?;
    verify_owner_signature_strict(
        ws::VIEW_JSON_PATH,
        &b.read.request,
        &d.owner_key.public_key,
        &b.read.signature_hex,
    )
    .map_err(|_| bad())?;
    Ok(())
}
pub(super) fn admission(
    o: &ParticipantOriginalV1,
    a: &SignedReadRequest<GameParticipantAdmissionV1>,
) -> Result<()> {
    let policy = o.policy.as_ref().ok_or_else(bad)?;
    let d = &o.descriptor;
    let r = &a.request;
    if r.api_rev != 1
        || r.identity_namespace != d.register.identity_namespace
        || r.account_id != d.register.account_id
        || r.world_id != d.world.world_id
        || r.genesis_digest != d.world.genesis_digest
        || r.owner_public_key != d.owner_key.public_key
        || r.branch_id != d.world.branch_id
        || r.reorg_epoch != d.world.reorg_epoch
        || r.business_digest
            != oasis7_client_api::world_service_correlation::participant_business_digest(
                &d.register,
            )
            .map_err(|_| bad())?
        || Some(r.policy_generation.0) != amount(&policy.response.payload["policy"]["generation"])
        || r.valid_from_tick.0 < amount(&policy.response.payload["logical_tick"]).ok_or_else(bad)?
        || r.valid_until_tick.0 <= r.valid_from_tick.0
        || r.valid_until_tick.0 - r.valid_from_tick.0
            > amount(&policy.response.payload["policy"]["max_grant_ticks"]).ok_or_else(bad)?
        || a.subject_public_key != issuer(o)?
    {
        return Err(bad());
    }
    verify_owner_signature_strict(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        r,
        issuer(o)?,
        &a.signature_hex,
    )
    .map_err(|_| bad())?;
    Ok(())
}
pub(super) fn wire(
    o: &ParticipantOriginalV1,
    a: &SignedReadRequest<GameParticipantAdmissionV1>,
) -> Result<(
    ws::SubmitIntentRequest<WorldServicePayloadV1>,
    AuthenticatedLookup,
    ParticipantOwnerViewRequestV1,
)> {
    let b = o.bundle.as_ref().ok_or_else(bad)?;
    let payload = WorldServicePayloadV1::GameParticipantRegister {
        owner: b.owner.clone(),
        admission: a.clone(),
    };
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
        world(&o.descriptor),
        &payload,
    )
    .map_err(|_| bad())?;
    let submit = ws::SubmitIntentRequest {
        contract_version: 1,
        correlation: correlation.clone(),
        deadline_unix_ms: o.descriptor.owner_read.deadline_unix_ms,
        signed_payload: payload.clone(),
    };
    submit.validate().map_err(|_| bad())?;
    let lookup = AuthenticatedLookup {
        request: ws::LookupIntentRequest {
            contract_version: 1,
            key: correlation.key,
        },
        original: payload,
    };
    let owner = ParticipantOwnerViewRequestV1 {
        owner: b.owner.clone(),
        admission: a.clone(),
        read: b.read.clone(),
        response_profile: Some(
            ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerControlV1,
        ),
    };
    Ok((submit, lookup, owner))
}
pub(super) fn result(
    o: &ParticipantOriginalV1,
    r: &ParticipantFinalResultV1,
) -> Result<GameParticipantReceiptV1> {
    let submit = o.submit.as_ref().ok_or_else(bad)?;
    let owner = o.owner_view.as_ref().ok_or_else(bad)?;
    let response_digest = match r.response_source {
        ParticipantResultSourceV1::Submit => ws::request_digest(ws::SUBMIT_PATH, submit),
        ParticipantResultSourceV1::Lookup => {
            ws::request_digest(ws::LOOKUP_PATH, o.lookup.as_ref().ok_or_else(bad)?)
        }
    }
    .map_err(|_| bad())?;
    if r.response.request_digest != response_digest
        || r.owner_response.request_digest
            != ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, owner).map_err(|_| bad())?
    {
        return Err(bad());
    }
    verify_owner_signature_strict(
        r.response_source.response_domain(),
        &(&r.response.request_digest, &r.response.payload),
        &o.descriptor.world.service_public_key,
        &r.response.signature_hex,
    )
    .map_err(|_| bad())?;
    verify_owner_signature_strict(
        PARTICIPANT_OWNER_VIEW_PATH,
        &(&r.owner_response.request_digest, &r.owner_response.payload),
        &o.descriptor.world.service_public_key,
        &r.owner_response.signature_hex,
    )
    .map_err(|_| bad())?;
    r.response
        .payload
        .validate(&submit.correlation)
        .map_err(|_| bad())?;
    let ws::IntentOutcome::Committed { commit, receipt } = &r.response.payload.outcome else {
        return Err(bad());
    };
    let receipt: GameParticipantReceiptV1 =
        serde_json::from_value(receipt.clone()).map_err(|_| bad())?;
    let d = &o.descriptor;
    let account = ws::request_digest(
        "participant-account-world",
        &(
            &d.register.identity_namespace.service_id,
            &d.register.identity_namespace.environment_id,
            &d.register.account_id,
            &d.register.world_id,
            &d.register.genesis_digest,
        ),
    )
    .map_err(|_| bad())?;
    let agent = format!(
        "participant:{}",
        account.strip_prefix("blake3:").unwrap_or(&account)
    );
    let a = &o.admission.as_ref().ok_or_else(bad)?.request;
    if receipt.api_rev != 1
        || receipt.imported
        || receipt.identity_namespace != d.register.identity_namespace
        || receipt.account_id != d.register.account_id
        || receipt.world_id != d.world.world_id
        || receipt.genesis_digest != d.world.genesis_digest
        || receipt.owner_public_key != d.owner_key.public_key
        || receipt.owner_generation.0 != 1
        || receipt.owner_binding != account
        || receipt.agent_id != agent
        || receipt.client_operation_id != d.register.client_operation_id
        || receipt.business_digest
            != oasis7_client_api::world_service_correlation::participant_business_digest(
                &d.register,
            )
            .map_err(|_| bad())?
        || receipt.registered_tick.0 < a.valid_from_tick.0
        || receipt.registered_tick.0 > a.valid_until_tick.0
        || commit.binding.branch_id != d.world.branch_id
        || commit.binding.reorg_generation != d.world.reorg_epoch.0
    {
        return Err(bad());
    }
    let view = &r.owner_response.payload;
    view.validate(owner).map_err(|_| bad())?;
    if !view
        .version
        .commit
        .satisfies_minimum(commit)
        .map_err(|_| bad())?
    {
        return Err(bad());
    }
    let actual = &view.view.canonical_agent_owner;
    if actual.agent_id != receipt.agent_id
        || actual.player_id != receipt.owner_binding
        || actual.public_key != receipt.owner_public_key
        || actual.world_id != receipt.world_id
        || actual.canonical_authority.agent_identity_generation != 1
        || actual.canonical_authority.branch_id != d.world.branch_id
        || actual.reorg_epoch != d.world.reorg_epoch.0
    {
        return Err(bad());
    }
    Ok(receipt)
}
