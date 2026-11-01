//! Existing sixth-stage fixed-profile funding. This is not a new transport cap.
use super::*;
use oasis7_client_api::world_service::{entry_control::*, resource_basis as resource};
fn public_bytes(value: &str) -> Result<[u8; 32]> {
    if !hex64(value) {
        return Err(invalid());
    }
    let mut bytes = [0; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| invalid())?;
    }
    Ok(bytes)
}
pub struct ResourceFinalIssueContextV2<'a> {
    /// Trusted deployment pin; never supplied by management request JSON.
    pub issuer: &'a PaidResourceReadIssuerPinV1,
    pub resource_feature_generation: DecimalU64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceFinalNativeOriginalV2 {
    api_version: u8,
    input: ApprovalAdmissionInput,
    request: EntryOwnerControlRequestV1,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceFinalDispatchRecordV2 {
    paid_attempt_id: String,
    paid_at: DecimalU64,
    current_window: ResourceFinalCurrentWindowV2,
    expires_at: DecimalU64,
    native_request_digest: String,
    source_record_digest: String,
    directory_generation: DecimalU64,
    resource_feature_generation: DecimalU64,
    issuer: PaidResourceReadIssuerPinV1,
}
/// Issuer child can sign only this actual committed typed producer, never bytes
/// supplied by a client. It must recheck its configured identity/purpose/window.
pub struct PreparedOwnerSetupResourceDispatchV2 {
    payload: PaidResourceReadPayloadV1,
    original: ResourceFinalNativeOriginalV2,
}
impl PreparedOwnerSetupResourceDispatchV2 {
    pub fn payload(&self) -> &PaidResourceReadPayloadV1 {
        &self.payload
    }
    pub fn request(&self) -> &EntryOwnerControlRequestV1 {
        &self.original.request
    }
    pub fn input(&self) -> &ApprovalAdmissionInput {
        &self.original.input
    }
}
impl AccountProvisionApprovalState {
    pub fn claim_owner_setup_resource_dispatch_v2(
        &mut self,
        id: &str,
        configured: &ResourceFinalIssueContextV2<'_>,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedOwnerSetupResourceDispatchV2> {
        self.validate_persistence()?;
        verifier.verify(&self.owner_setup_stored_admission_checks(
            id,
            OwnerSetupStageV1::FinalObservation,
            current,
            now,
        )?)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let r = v
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        let outbox = v.resource_final.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id
            || v.plan.selection.resource().is_none()
            || outbox.dispatch.is_some()
            || configured.resource_feature_generation.0 == 0
        {
            return Err(invalid());
        }
        let pin = configured.issuer;
        if pin.allowed_stage != PaidResourceReadStageV1::OwnerSetupFinal
            || pin.environment != self.draft.entry.environment
            || pin.world_audience != v.plan.world_identity
            || pin.native_audience_origin != current.native_scope.http_origin
            || now < pin.not_before.0
            || now >= pin.not_after.0
        {
            return Err(invalid());
        }
        let public = ed25519_dalek::VerifyingKey::from_bytes(&public_bytes(&pin.public_key)?)
            .map_err(|_| invalid())?;
        if public.is_weak() {
            return Err(invalid());
        }
        let q: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2> =
            decode(outbox.original_bytes.as_ref().ok_or_else(invalid)?)?;
        resource::verify_resource_goal_request(&q).map_err(|_| invalid())?;
        let common = &outbox.common;
        let current_window = ResourceFinalCurrentWindowV2::capture(current, now);
        let expires = current_window
            .deadline(&v.plan, &common.source)?
            .min(
                outbox
                    .signing_window
                    .as_ref()
                    .ok_or_else(invalid)?
                    .deadline(&v.plan, &common.source)?,
            )
            .min(
                outbox
                    .published_window
                    .as_ref()
                    .ok_or_else(invalid)?
                    .deadline(&v.plan, &common.source)?,
            )
            .min(pin.not_after.0);
        if now >= expires {
            return Err(invalid());
        }
        let original = ResourceFinalNativeOriginalV2 {
            api_version: 2,
            input: r.input().clone(),
            request: EntryOwnerControlRequestV1 {
                access: common.observation.clone().ok_or_else(invalid)?,
                operation: EntryOwnerOperationV1::ReadResourceGoal(q.clone()),
            },
        };
        let paid_attempt_id = format!(
            "owner-resource-final-v2:{}",
            digest(&(
                "oasis7.custody.owner-resource-final-paid-attempt.v2",
                r.input(),
                &original,
                &common.source,
            ))?
        );
        let record = ResourceFinalDispatchRecordV2 {
            paid_attempt_id: paid_attempt_id.clone(),
            paid_at: DecimalU64(now),
            current_window,
            expires_at: DecimalU64(expires),
            native_request_digest: digest(&(
                "oasis7.custody.resource-native-original.v2",
                &original,
            ))?,
            source_record_digest: digest(&common.source)?,
            // The existing Worker owner authority factory derives this exact
            // policy version from DirectoryConfig.version (no new RPC).
            directory_generation: DecimalU64(current.provision.policy.version),
            resource_feature_generation: configured.resource_feature_generation,
            issuer: pin.clone(),
        };
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let payload = PaidResourceReadPayloadV1 {
            api_version: 1,
            issuer_namespace: pin.namespace.clone(),
            issuer_service_id: pin.service_id.clone(),
            issuer_environment: pin.environment.clone(),
            issuer_key_id: pin.key_id.clone(),
            issuer_epoch: pin.issuer_epoch,
            issuer_config_generation: pin.issuer_config_generation,
            native_audience_origin: pin.native_audience_origin.clone(),
            world_audience: pin.world_audience.clone(),
            native_path: "/entry-owner/control".into(),
            world_path: resource::RESOURCE_GOAL_PATH_V2.into(),
            stage: PaidResourceReadStageV1::OwnerSetupFinal,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            manager_native_account_id: self.draft.entry.manager.native_account_id.clone(),
            participant_native_account_id: participant
                .descriptor
                .native_binding
                .native_account_id
                .clone(),
            provision_id: v.plan.provision_id.clone(),
            setup_id: id.into(),
            test_session_id: None,
            approval_operation_id: id.into(),
            approval_input_digest: digest(&(
                "oasis7.custody.owner-setup-approved-input.v2",
                r.input(),
            ))?,
            plan_digest: v.plan.digest().map_err(|_| invalid())?,
            prepared_resource_basis_digest: None,
            selection_digest: digest(&(
                "oasis7.custody.resource-selection.v1",
                &q.request.selection,
            ))?,
            owner_public_key: q.subject_public_key.clone(),
            owner_binding: q.request.expected_fence.owner_binding.clone(),
            task_fence_digest: digest(&(
                "oasis7.custody.resource-task-fence.v1",
                &q.request.expected_fence,
            ))?,
            signer_record_digest: digest(&(
                "oasis7.custody.resource-signer-record.v1",
                &q.request.expected_signer_delegation,
            ))?,
            native_request_digest: record.native_request_digest.clone(),
            world_request_digest: paid_world_request_digest(resource::RESOURCE_GOAL_PATH_V2, &q)?,
            preflight_world_request_digest: Some(paid_world_request_digest(
                ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                &original.request.access,
            )?),
            source_operation_id: common.source.operation_id.clone(),
            source_record_digest: record.source_record_digest.clone(),
            source_security_epoch: r.input().request.context.security_version,
            service_epoch: r.input().request.context.service_epoch,
            directory_generation: record.directory_generation,
            resource_flag_generation: record.resource_feature_generation,
            paid_attempt_id: paid_attempt_id.clone(),
            paid_record_digest: digest(&record)?,
            // A subset of the existing 96-unit stage allocation, not new units.
            native_http_units: DecimalU64(1),
            world_http_units: DecimalU64(2),
            issued_at: DecimalU64(now),
            expires_at: DecimalU64(expires),
        };
        payload.validate_shape().map_err(|_| invalid())?;
        let mut next = self.clone();
        next.owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .resource_final
            .as_mut()
            .ok_or_else(invalid)?
            .dispatch = Some(record);
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedOwnerSetupResourceDispatchV2 { payload, original })
    }
}

impl ResourceFinalDispatchRecordV2 {
    pub(super) fn validate(&self, state: &AccountProvisionApprovalState, id: &str) -> Result<()> {
        let v = state.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        let f = v.resource_final.as_ref().ok_or_else(invalid)?;
        let common = &f.common;
        let q: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2> =
            decode(f.original_bytes.as_ref().ok_or_else(invalid)?)?;
        resource::verify_resource_goal_request(&q).map_err(|_| invalid())?;
        let original = ResourceFinalNativeOriginalV2 {
            api_version: 2,
            input: r.input().clone(),
            request: EntryOwnerControlRequestV1 {
                access: common.observation.clone().ok_or_else(invalid)?,
                operation: EntryOwnerOperationV1::ReadResourceGoal(q),
            },
        };
        let source = common.source.verification.as_ref().ok_or_else(invalid)?;
        let expected_attempt = format!(
            "owner-resource-final-v2:{}",
            digest(&(
                "oasis7.custody.owner-resource-final-paid-attempt.v2",
                r.input(),
                &original,
                &common.source,
            ))?
        );
        self.current_window
            .validate(&common.source, self.paid_at.0)?;
        let expected_until = self
            .current_window
            .deadline(&v.plan, &common.source)?
            .min(
                f.signing_window
                    .as_ref()
                    .ok_or_else(invalid)?
                    .deadline(&v.plan, &common.source)?,
            )
            .min(
                f.published_window
                    .as_ref()
                    .ok_or_else(invalid)?
                    .deadline(&v.plan, &common.source)?,
            )
            .min(self.issuer.not_after.0);
        let public =
            ed25519_dalek::VerifyingKey::from_bytes(&public_bytes(&self.issuer.public_key)?)
                .map_err(|_| invalid())?;
        if self.paid_attempt_id != expected_attempt
            || self.native_request_digest
                != digest(&("oasis7.custody.resource-native-original.v2", &original))?
            || self.source_record_digest != digest(&common.source)?
            || self.directory_generation != v.plan.bootstrap_config_version
            || self.resource_feature_generation.0 == 0
            || self.issuer.allowed_stage != PaidResourceReadStageV1::OwnerSetupFinal
            || self.issuer.environment != state.draft.entry.environment
            || self.issuer.world_audience != v.plan.world_identity
            || public.is_weak()
            || self.paid_at.0 < self.issuer.not_before.0
            || self.expires_at.0 != expected_until
            || self.paid_at.0 >= expected_until
            || source.verification.observed_at.0 > self.paid_at.0
            || f.published_at.is_none_or(|at| at.0 > self.paid_at.0)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn expires_at(&self) -> u64 {
        self.expires_at.0
    }
    pub(super) fn paid_at(&self) -> u64 {
        self.paid_at.0
    }
}
