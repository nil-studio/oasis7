//! Exact canonical storage for the complete typed Resource final observation.
//! Eq compares stored bytes, not floating-point quote semantics. These historical
//! facts alone do not accept a phase, issue permission, or prove payment.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
#[path = "owner_setup_resource_final_window_v2.rs"]
mod window;
use window::ResourceFinalCurrentWindowV2;
#[path = "owner_setup_resource_result_callback_v2.rs"]
mod callback;
use callback::ResourceFinalResultCallbackV2;
#[path = "owner_setup_resource_final_request_v2.rs"]
mod request;
pub use request::{OwnerSetupResourceCandidateV2, PreparedOwnerSetupResourceReadV2};
#[path = "owner_setup_resource_final_dispatch_v2.rs"]
mod dispatch;
use dispatch::ResourceFinalDispatchRecordV2;
pub use dispatch::{
    OwnerSetupResourceBodyDigestsV2, OwnerSetupResourceFinalHttpV2, OwnerSetupResourceWorldBodyV2,
    PreparedOwnerSetupResourceDispatchV2, RESOURCE_READ_PROOF_HEADER, ResourceFinalIssueContextV2,
};
#[path = "owner_setup_resource_final_result_v2.rs"]
mod result;
pub use result::CompleteOwnerSetupResourceFinalV2;
#[path = "owner_setup_resource_final_restore_v2.rs"]
mod restore;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerSetupResourceFinalProofV2 {
    api_version: u8,
    setup_id: String,
    plan_digest: String,
    original_bytes: Vec<u8>,
    response_bytes: Vec<u8>,
}
impl core::fmt::Debug for OwnerSetupResourceFinalProofV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupResourceFinalProofV2(<private canonical evidence>)")
    }
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|_| invalid())
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let value: T = serde_cbor::from_slice(bytes).map_err(|_| invalid())?;
    if encode(&value)? != bytes {
        return Err(invalid());
    }
    Ok(value)
}
// The SDK serializes its typed request hash as `blake3:<64lowerhex>`;
// the existing paid-proof wire stores the identical hash as 64 lower hex.
fn paid_world_request_digest<T: Serialize>(path: &str, request: &T) -> Result<String> {
    let sdk = ws::request_digest(path, request).map_err(|_| invalid())?;
    let raw = sdk.strip_prefix("blake3:").ok_or_else(invalid)?;
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(raw.to_owned())
}
impl OwnerSetupResourceFinalProofV2 {
    pub(super) fn from_typed(
        plan: &OwnerSetupPlanV2,
        original: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2>,
        response: resource::SignedResourceResponseV1<
            resource::OwnerSetupFinalResourceObservationV2,
        >,
    ) -> Result<Self> {
        if serde_json::to_vec(&original).map_err(|_| invalid())?.len()
            > resource::MAX_RESOURCE_ENVELOPE_BYTES
        {
            return Err(invalid());
        }
        // Measure the actual complete Native response wrapper, not a partial
        // quote or a guessed reserve for signature/envelope overhead.
        resource::encode_native_resource_envelope(response.clone()).map_err(|_| invalid())?;
        let value = Self {
            api_version: 2,
            setup_id: plan.setup_id.clone(),
            plan_digest: plan.digest().map_err(|_| invalid())?,
            original_bytes: encode(&original)?,
            response_bytes: encode(&response)?,
        };
        value.typed(plan)?;
        Ok(value)
    }
    pub(super) fn typed(
        &self,
        plan: &OwnerSetupPlanV2,
    ) -> Result<(
        resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2>,
        resource::SignedResourceResponseV1<resource::OwnerSetupFinalResourceObservationV2>,
    )> {
        if self.api_version != 2
            || self.setup_id != plan.setup_id
            || self.plan_digest != plan.digest().map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        let original: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2> =
            decode(&self.original_bytes)?;
        let response: resource::SignedResourceResponseV1<
            resource::OwnerSetupFinalResourceObservationV2,
        > = decode(&self.response_bytes)?;
        let q = &original.request;
        if q.setup_id != plan.setup_id
            || q.provision_id != plan.provision_id
            || q.plan_digest != self.plan_digest
            || plan.selection.resource() != Some(&q.selection)
            || q.world.world_id != plan.world_identity.world_id
            || q.world.genesis_digest != plan.world_identity.genesis_digest
            || q.expected_fence.branch_id != plan.world_identity.branch_id
            || q.expected_fence.reorg_epoch.0 != plan.world_identity.reorg_epoch.0
        {
            return Err(invalid());
        }
        resource::verify_resource_goal_response(
            &original,
            &response,
            &plan.world_identity.service_public_key,
        )
        .map_err(|_| invalid())?;
        resource::encode_native_resource_envelope(response.clone()).map_err(|_| invalid())?;
        Ok((original, response))
    }
    pub(super) fn prepared(&self, plan: &OwnerSetupPlanV2) -> Result<PreparedResourceBasisV1> {
        let (original, response) = self.typed(plan)?;
        PreparedResourceBasisV1::from_accepted_final_v2(plan, original, response)
            .map_err(|_| invalid())
    }
}

/// Dedicated sixth-stage storage. The V1 stage DTO never carries this original
/// or result. Its common local Source/operation/once fields are reused intact.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning::approval) struct OwnerSetupResourceFinalOutboxV2 {
    api_version: u8,
    plan_digest: String,
    common: OwnerSetupOutboxV1,
    #[serde(deserialize_with = "required_final_original")]
    original_bytes: Option<Vec<u8>>,
    #[serde(deserialize_with = "required_final_original")]
    unsigned_request_bytes: Option<Vec<u8>>,
    #[serde(deserialize_with = "required_completed")]
    signing_claimed_at: Option<DecimalU64>,
    #[serde(deserialize_with = "window::required_window")]
    signing_window: Option<ResourceFinalCurrentWindowV2>,
    #[serde(deserialize_with = "required_completed")]
    published_at: Option<DecimalU64>,
    #[serde(deserialize_with = "window::required_window")]
    published_window: Option<ResourceFinalCurrentWindowV2>,
    #[serde(deserialize_with = "required_final_result")]
    result: Option<OwnerSetupResourceFinalProofV2>,
    #[serde(deserialize_with = "callback::required_callback")]
    result_callback: Option<ResourceFinalResultCallbackV2>,
    #[serde(deserialize_with = "required_completed")]
    completed_at: Option<DecimalU64>,
    #[serde(deserialize_with = "required_dispatch")]
    dispatch: Option<ResourceFinalDispatchRecordV2>,
    #[serde(deserialize_with = "required_session")]
    session: Option<ParticipantSessionEvidenceV1>,
}
impl core::fmt::Debug for OwnerSetupResourceFinalOutboxV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupResourceFinalOutboxV2(<private>)")
    }
}
fn required_session<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<ParticipantSessionEvidenceV1>, D::Error> {
    Option::deserialize(d)
}
fn required_dispatch<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<ResourceFinalDispatchRecordV2>, D::Error> {
    Option::deserialize(d)
}
fn required_completed<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<DecimalU64>, D::Error> {
    Option::deserialize(d)
}
fn required_final_original<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<Vec<u8>>, D::Error> {
    Option::deserialize(d)
}
fn required_final_result<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<OwnerSetupResourceFinalProofV2>, D::Error> {
    Option::deserialize(d)
}

impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_owner_setup_resource_final_binding_v2(
        &self,
    ) -> Result<()> {
        let Some(v) = &self.owner_setup_v2 else {
            return Ok(());
        };
        let Some(f) = &v.resource_final else {
            return Ok(());
        };
        self.owner_setup_engine_record(&v.plan.setup_id)?;
        if f.api_version != 2
            || v.plan.selection.resource().is_none()
            || f.plan_digest != v.plan.digest().map_err(|_| invalid())?
            || f.common.stage != OwnerSetupStageV1::FinalObservation
            || f.common.original.is_some()
            || f.common.result.is_some()
            || v.book
                .original(&v.plan.setup_id)
                .is_none_or(|r| r.stage() != ApprovalAdmissionStage::Consumed)
            || f.signing_claimed_at.is_some() != f.signing_window.is_some()
            || f.published_at.is_some() != f.published_window.is_some()
            || f.signing_claimed_at.is_some() != f.unsigned_request_bytes.is_some()
            || f.original_bytes.is_some() != f.published_at.is_some()
            || f.original_bytes.is_some() && f.signing_claimed_at.is_none()
            || f.result.is_some() && f.result_callback.is_none()
            || f.completed_at.is_some() != f.result.is_some()
            || f.session.is_some() != f.result.is_some()
            || f.dispatch.is_some() && f.original_bytes.is_none()
            || f.result.is_some() && f.dispatch.is_none()
        {
            return Err(invalid());
        }
        if let Some(callback) = &f.result_callback {
            callback.validate(f)?;
            if f.completed_at
                .is_some_and(|at| at.0 < callback.claimed_at())
            {
                return Err(invalid());
            }
        }
        if let Some(window) = &f.signing_window {
            window.validate(
                &f.common.source,
                f.signing_claimed_at.ok_or_else(invalid)?.0,
            )?;
        }
        if let Some(window) = &f.published_window {
            let at = f.published_at.ok_or_else(invalid)?.0;
            window.validate(&f.common.source, at)?;
            if at
                >= f.signing_window
                    .as_ref()
                    .ok_or_else(invalid)?
                    .deadline(&v.plan, &f.common.source)?
            {
                return Err(invalid());
            }
        }
        if let Some(bytes) = &f.unsigned_request_bytes {
            let q: resource::OwnerSetupFinalResourceReadV2 = decode(bytes)?;
            q.validate().map_err(|_| invalid())?;
            let expected_control = self
                .owner_setup_expected_control(
                    &v.plan.setup_id,
                    OwnerSetupStageV1::FinalObservation,
                )?
                .ok_or_else(invalid)?;
            let task = &expected_control;
            let phase = self.owner_setup_engine_record(&v.plan.setup_id)?;
            let Some(OwnerSetupOriginalV1::Canonical { submit, .. }) = phase
                .stages
                .get(&OwnerSetupStageV1::Delegation)
                .and_then(|o| o.original.as_ref())
            else {
                return Err(invalid());
            };
            let WorldServicePayloadV1::Delegation(original) = &submit.signed_payload else {
                return Err(invalid());
            };
            let d = &original.request;
            self.owner_setup_delegation(&v.plan.setup_id, Some(d))?;
            let expected_signer = resource::ResourceSignerRecordV1 {
                world_id: d.world_id.clone(),
                branch_id: d.branch_id.clone(),
                agent_id: d.agent_id.clone(),
                owner_binding: d.owner_binding.clone(),
                agent_identity_generation: DecimalU64(d.agent_identity_generation),
                generation: DecimalU64(d.generation),
                delegate_public_key: d.delegate_public_key.clone(),
                revoked: d.revoked,
                nonce: DecimalU64(d.nonce),
            };
            let source = f.common.source.verification.as_ref().ok_or_else(invalid)?;
            let at = f.signing_claimed_at.ok_or_else(invalid)?.0;
            let until = v.plan.deadline.0.min(
                source
                    .verification
                    .observed_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
            );
            if q.setup_id != v.plan.setup_id
                || q.provision_id != v.plan.provision_id
                || q.plan_digest != f.plan_digest
                || v.plan.selection.resource() != Some(&q.selection)
                || q.world.world_id != v.plan.world_identity.world_id
                || q.world.genesis_digest != v.plan.world_identity.genesis_digest
                || q.expected_fence != oasis7_agent_api::game::GameTaskFenceV1::from_control(task)
                || q.expected_signer_delegation != expected_signer
                || q.goal_reference
                    != task
                        .active_goal_reference
                        .as_ref()
                        .ok_or_else(invalid)?
                        .goal_ref
                || q.observation_nonce
                    != digest(&(
                        "oasis7.custody.owner-setup-resource-observation-nonce.v2",
                        v.plan.digest().map_err(|_| invalid())?,
                        &f.common.session_nonce,
                    ))?
                || source.verification.observed_at.0 > at
                || at >= until
                || f.common.access_recorded_at.is_none_or(|v| v.0 > at)
                || f.published_at.is_some_and(|v| v.0 < at || v.0 >= until)
            {
                return Err(invalid());
            }
        }
        if let Some(bytes) = &f.original_bytes {
            let q: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2> =
                decode(bytes)?;
            resource::verify_resource_goal_request(&q).map_err(|_| invalid())?;
            let expected: resource::OwnerSetupFinalResourceReadV2 =
                decode(f.unsigned_request_bytes.as_ref().ok_or_else(invalid)?)?;
            if q.request != expected {
                return Err(invalid());
            }
            if q.request.setup_id != v.plan.setup_id
                || q.request.plan_digest != f.plan_digest
                || v.plan.selection.resource() != Some(&q.request.selection)
            {
                return Err(invalid());
            }
        }
        if let Some(dispatch) = &f.dispatch {
            dispatch.validate(self, &v.plan.setup_id)?;
        }
        if let Some(proof) = &f.result {
            let (q, _) = proof.typed(&v.plan)?;
            if Some(encode(&q)?).as_ref() != f.original_bytes.as_ref() {
                return Err(invalid());
            }
            self.validate_resource_final_prefix_v2(&v.plan.setup_id, proof)?;
            let at = f.completed_at.ok_or_else(invalid)?.0;
            let source = f.common.source.verification.as_ref().ok_or_else(invalid)?;
            if f.dispatch
                .as_ref()
                .is_none_or(|d| d.paid_at() > at || at >= d.expires_at())
                || source.verification.observed_at.0 > at
                || at - source.verification.observed_at.0 > 5000
                || at >= v.plan.deadline.0
            {
                return Err(invalid());
            }
            self.validate_owner_setup_session(
                &v.plan.setup_id,
                &f.common.session_nonce,
                f.session.as_ref().ok_or_else(invalid)?,
                at,
                v.book
                    .original(&v.plan.setup_id)
                    .ok_or_else(invalid)?
                    .input()
                    .request
                    .context
                    .service_epoch,
            )?;
        }
        Ok(())
    }
    pub(in crate::provisioning) fn owner_setup_resource_final_common_v2(
        &self,
        id: &str,
    ) -> Result<Option<&OwnerSetupOutboxV1>> {
        let Some(v) = self.owner_setup_v2.as_ref() else {
            return Ok(None);
        };
        if v.plan.setup_id != id {
            return Err(invalid());
        }
        Ok(v.resource_final.as_ref().map(|f| &f.common))
    }
    pub(in crate::provisioning) fn owner_setup_resource_final_common_mut_v2(
        &mut self,
        id: &str,
    ) -> Result<&mut OwnerSetupOutboxV1> {
        let v = self.owner_setup_v2.as_mut().ok_or_else(invalid)?;
        if v.plan.setup_id != id || v.plan.selection.resource().is_none() {
            return Err(invalid());
        }
        Ok(&mut v.resource_final.as_mut().ok_or_else(invalid)?.common)
    }
    pub(in crate::provisioning) fn install_owner_setup_resource_final_common_v2(
        &mut self,
        id: &str,
        common: OwnerSetupOutboxV1,
    ) -> Result<()> {
        let v = self.owner_setup_v2.as_mut().ok_or_else(invalid)?;
        if v.plan.setup_id != id
            || v.plan.selection.resource().is_none()
            || v.resource_final.is_some()
            || common.stage != OwnerSetupStageV1::FinalObservation
            || v.book
                .original(id)
                .is_none_or(|r| r.stage() != ApprovalAdmissionStage::Consumed)
        {
            return Err(invalid());
        }
        v.resource_final = Some(OwnerSetupResourceFinalOutboxV2 {
            api_version: 2,
            plan_digest: v.plan.digest().map_err(|_| invalid())?,
            common,
            original_bytes: None,
            result: None,
            result_callback: None,
            completed_at: None,
            unsigned_request_bytes: None,
            signing_claimed_at: None,
            signing_window: None,
            published_at: None,
            published_window: None,
            dispatch: None,
            session: None,
        });
        Ok(())
    }
}
