//! Actual OwnerVault fixed Resource signing lane on the existing private RPC.
use super::*;
use crate::role_key_store::{OwnerResourceSigningCurrentV2, owner_resource_bundle_digest_v2};
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_custody_core::provisioning::OwnerSetupStageV1;
pub(super) struct ResourceObservationV2 {
    pub view: OwnerSetupResourceSignerPermitV2,
    pub fence: RecoveryFence,
    pub fence_started: u64,
}
impl ResourceObservationV2 {
    fn role(&self, env: &Env, selector: &OwnerSetupStageRpc) -> JResult<RoleKeyCurrent> {
        let v = &self.view.permit;
        if selector.request.api_version != 2
            || selector.stage != OwnerSetupStageV1::FinalObservation
            || v.approved_plan_v2
                .as_ref()
                .is_none_or(|p| p.selection.resource().is_none())
        {
            return Err(rejected());
        }
        checked_role(v, env, selector, &self.fence, self.fence_started)
    }
    fn signing_current(
        &self,
        env: &Env,
        selector: &OwnerSetupStageRpc,
    ) -> JResult<OwnerResourceSigningCurrentV2> {
        let role = self.role(env, selector)?;
        let OwnerSetupResourceSigningReadV2::VaultClaim { bundle } = &self.view.resource_signing
        else {
            return Err(rejected());
        };
        let v = &self.view.permit;
        let header = v.approved_plan_v2.as_ref().ok_or_else(rejected)?;
        if bundle.request.setup_id != v.setup_id
            || bundle.request.provision_id != v.plan.provision_id
            || bundle.request.plan_digest != header.digest().map_err(|_| rejected())?
            || header.selection.resource() != Some(&bundle.request.selection)
            || bundle.request.world.world_id != header.world_identity.world_id
            || bundle.request.world.genesis_digest != header.world_identity.genesis_digest
            || bundle.owner_public_key != v.participant.owner_key.public_key
            || bundle.source_record_digest != self.view.resource_source_record_digest
            || bundle.valid_until.0 > v.expires_at.0
            || clock().map_err(|_| rejected())? >= bundle.valid_until.0
        {
            return Err(rejected());
        }
        Ok(OwnerResourceSigningCurrentV2 {
            role,
            setup_id: v.setup_id.clone(),
            bundle_digest: owner_resource_bundle_digest_v2(bundle).map_err(|_| rejected())?,
        })
    }
}
fn check_published(
    observed: &ResourceObservationV2,
    p: &OwnerSetupResourcePublishedV2,
) -> JResult<()> {
    let v = &observed.view.permit;
    let header = v.approved_plan_v2.as_ref().ok_or_else(rejected)?;
    let q = &p.candidate.original;
    resource::verify_resource_goal_request(q).map_err(|_| rejected())?;
    if p.permit.kind != OwnerSetupPermitKindV1::Preflight
        || p.permit.setup_id != v.setup_id
        || p.permit.stage != v.stage
        || p.permit.plan != v.plan
        || p.permit.approved_plan_v2 != v.approved_plan_v2
        || p.permit.participant != v.participant
        || p.resource_source_record_digest != observed.view.resource_source_record_digest
        || p.permit.source_revision != v.source_revision
        || p.permit.authority_revision != v.authority_revision
        || p.permit.service_epoch != v.service_epoch
        || p.permit.target_security_epoch != v.target_security_epoch
        || p.permit.expires_at.0 > v.expires_at.0
        || p.permit.observation.as_ref() != Some(&p.candidate.observation)
        || q.subject_public_key != v.participant.owner_key.public_key
        || q.request.setup_id != v.setup_id
        || q.request.plan_digest != header.digest().map_err(|_| rejected())?
        || header.selection.resource() != Some(&q.request.selection)
    {
        return Err(rejected());
    }
    Ok(())
}
pub(super) async fn sign(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    observed: ResourceObservationV2,
) -> Result<String> {
    observed.role(env, selector).map_err(|_| denied())?;
    if !matches!(
        observed.view.resource_signing,
        OwnerSetupResourceSigningReadV2::OuterAccess
    ) {
        return Err(denied());
    }
    let name = observed
        .view
        .permit
        .participant
        .owner_scope
        .task_digest()
        .map_err(|_| denied())?;
    let stub = env
        .durable_object("ROLE_KEYS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(serde_json::to_string(selector)?.into()))
        .with_redirect(RequestRedirect::Manual);
    let mut request =
        Request::new_with_init("https://role-key.internal/internal/owner-setup-sign", &init)?;
    request
        .headers_mut()?
        .set("Content-Type", "application/json")?;
    let response = stub.fetch_with_request(request).await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let response: web_sys::Response = response.into();
    let raw = bounded_body(response.as_ref(), 128 * 1024)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    observed.role(env, selector).map_err(|_| denied())?;
    let published: OwnerSetupResourcePublishedV2 =
        OwnerSetupResourcePublishedV2::from_exact_json(&raw).map_err(|_| denied())?;
    check_published(&observed, &published).map_err(|_| denied())?;
    Ok(raw)
}
pub(super) async fn sign_in_vault(
    state: JsValue,
    env: &Env,
    selector: &OwnerSetupStageRpc,
    observed: ResourceObservationV2,
) -> Result<Response> {
    observed
        .signing_current(env, selector)
        .map_err(|_| denied())?;
    let OwnerSetupResourceSigningReadV2::VaultClaim { bundle } = &observed.view.resource_signing
    else {
        return Err(denied());
    };
    let text = Zeroizing::new(env.secret("CUSTODY_ROLE_KEY_KEK")?.to_string());
    let mut kek = Zeroizing::new([0u8; 32]);
    if text.len() != 64 {
        return Err(denied());
    }
    hex::decode_to_slice(text.as_bytes(), kek.as_mut_slice()).map_err(|_| denied())?;
    if hex::encode(kek.as_slice()) != text.as_str() {
        return Err(denied());
    }
    let wrapping_version: DecimalU64 = serde_json::from_value(
        env.var("CUSTODY_ROLE_WRAPPING_KEY_VERSION")?
            .to_string()
            .into(),
    )?;
    let published = std::cell::RefCell::new(None);
    let observed_ref = &observed;
    let published_ref = &published;
    let publish =
        move |candidate: oasis7_custody_core::provisioning::OwnerSetupResourceCandidateV2| async move {
            let expected = candidate.clone();
            let raw: serde_json::Value = rpc(
                env,
                selector,
                OwnerSetupRuntimeAction::PublishResourceCandidate {
                    candidate: Box::new(candidate),
                },
            )
            .await?;
            let result = OwnerSetupResourcePublishedV2::from_exact_json(
                &serde_json::to_string(&raw).map_err(|_| rejected())?,
            )
            .map_err(|_| rejected())?;
            let (fresh, started) = fence(env).await?;
            let now = clock().map_err(|_| rejected())?;
            if started > now
                || now - started > 5000
                || fresh
                    .check_epoch(observed_ref.view.permit.service_epoch.0)
                    .is_err()
                || result.candidate != expected
            {
                return Err(rejected());
            }
            check_published(observed_ref, &result)?;
            observed_ref.signing_current(env, selector)?;
            published_ref.replace(Some(result));
            Ok(expected)
        };
    RoleKeyStore::new(state)
        .sign_owner_resource_final_v2(
            ParticipantSigningKey {
                scope: &observed.view.permit.participant.owner_scope,
                wrapping_version,
                kek: &kek,
            },
            bundle,
            || observed.signing_current(env, selector),
            publish,
        )
        .await
        .map_err(|_| denied())?;
    Response::from_json(&published.into_inner().ok_or_else(denied)?)
}
