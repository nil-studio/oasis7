//! Closed paid Native frame. A matching signed payload is not Current authority.
use super::*;
use crate::approval::CustodyIntentV1;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
#[path = "owner_setup_resource_world_body_v2.rs"]
mod world;
pub use world::OwnerSetupResourceWorldBodyV2;
pub const RESOURCE_READ_PROOF_HEADER: &str = "Oasis7-Custody-Resource-Read";
#[derive(Clone)]
pub struct OwnerSetupResourceFinalHttpV2 {
    api_version: u8,
    input: ApprovalAdmissionInput,
    participant: ParticipantDescriptorV1,
    request: EntryOwnerControlRequestV1,
    paid: PaidResourceReadProofV1,
}
impl core::fmt::Debug for OwnerSetupResourceFinalHttpV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupResourceFinalHttpV2(<private paid original>)")
    }
}
impl PreparedOwnerSetupResourceDispatchV2 {
    /// The caller must persist the original Core dispatch before signing or I/O.
    pub fn into_http(self, paid: PaidResourceReadProofV1) -> Result<OwnerSetupResourceFinalHttpV2> {
        if paid.payload != self.payload {
            return Err(invalid());
        }
        let value = OwnerSetupResourceFinalHttpV2 {
            api_version: 2,
            input: self.original.input,
            participant: self.original.participant,
            request: self.original.request,
            paid,
        };
        value.validate_bindings()?;
        value.body_bytes()?;
        value.proof_header()?;
        Ok(value)
    }
}
/// Fixed canonical correlations only. These hashes are not an issuer, payment or Current assertion.
pub struct OwnerSetupResourceBodyDigestsV2 {
    pub approval_input: String,
    pub native_body: String,
    pub selection: String,
    pub task_fence: String,
    pub signer_record: String,
}
impl OwnerSetupResourceFinalHttpV2 {
    pub fn expected_body_digests(
        input: &ApprovalAdmissionInput,
        participant: &ParticipantDescriptorV1,
        request: &EntryOwnerControlRequestV1,
    ) -> Result<OwnerSetupResourceBodyDigestsV2> {
        let CustodyIntentV1::OwnerSetupV2 { plan } = &input.request.intent else {
            return Err(invalid());
        };
        let EntryOwnerOperationV1::ReadResourceGoal(q) = &request.operation else {
            return Err(invalid());
        };
        plan.validate().map_err(|_| invalid())?;
        resource::verify_resource_goal_request(q).map_err(|_| invalid())?;
        let body = ResourceFinalNativeOriginalV2 {
            api_version: 2,
            input: input.clone(),
            participant: participant.clone(),
            request: request.clone(),
        };
        Ok(OwnerSetupResourceBodyDigestsV2 {
            approval_input: digest(&("oasis7.custody.owner-setup-approved-input.v2", input))?,
            native_body: digest(&("oasis7.custody.resource-native-original.v2", body))?,
            selection: digest(&("oasis7.custody.resource-selection.v1", &q.request.selection))?,
            task_fence: digest(&(
                "oasis7.custody.resource-task-fence.v1",
                &q.request.expected_fence,
            ))?,
            signer_record: digest(&(
                "oasis7.custody.resource-signer-record.v1",
                &q.request.expected_signer_delegation,
            ))?,
        })
    }

    pub fn input(&self) -> &ApprovalAdmissionInput {
        &self.input
    }
    pub fn world_body(&self) -> Result<OwnerSetupResourceWorldBodyV2> {
        self.validate_bindings()?;
        OwnerSetupResourceWorldBodyV2::from_frame(self)
    }
    pub fn participant(&self) -> &ParticipantDescriptorV1 {
        &self.participant
    }
    pub fn request(&self) -> &EntryOwnerControlRequestV1 {
        &self.request
    }
    pub fn paid(&self) -> &PaidResourceReadProofV1 {
        &self.paid
    }
    /// Fixed issuer/floor crypto and saved Native consent/Current checks remain
    /// mandatory at the receiver; this routine proves only typed correlations.
    pub fn validate_bindings(&self) -> Result<()> {
        let CustodyIntentV1::OwnerSetupV2 { plan } = &self.input.request.intent else {
            return Err(invalid());
        };
        let EntryOwnerOperationV1::ReadResourceGoal(q) = &self.request.operation else {
            return Err(invalid());
        };
        plan.validate().map_err(|_| invalid())?;
        resource::verify_resource_goal_request(q).map_err(|_| invalid())?;
        self.paid.payload.validate_shape().map_err(|_| invalid())?;
        let p = &self.paid.payload;
        let original = ResourceFinalNativeOriginalV2 {
            api_version: self.api_version,
            input: self.input.clone(),
            participant: self.participant.clone(),
            request: self.request.clone(),
        };
        if self.api_version != 2
            || plan.selection.resource() != Some(&q.request.selection)
            || self.input.operation_id != plan.setup_id
            || self.input.approval.proof.intent_digest
                != self.input.request.intent_digest().map_err(|_| invalid())?
            || p.stage != PaidResourceReadStageV1::OwnerSetupFinal
            || p.setup_id != plan.setup_id
            || p.provision_id != plan.provision_id
            || p.approval_operation_id != self.input.operation_id
            || p.source_security_epoch != self.input.request.context.security_version
            || p.service_epoch != self.input.request.context.service_epoch
            || p.plan_digest != plan.digest().map_err(|_| invalid())?
            || p.approval_input_digest
                != digest(&("oasis7.custody.owner-setup-approved-input.v2", &self.input))?
            || p.world_audience != plan.world_identity
            || self.participant.world != plan.world_identity
            || self.participant.owner_key.public_key != q.subject_public_key
            || self.participant.native_binding.native_account_id != p.participant_native_account_id
            || self.participant.session.session_ref.is_empty()
            || self.participant.session.private_sid_generation.0 == 0
            || self.participant.session.native_binding_generation.0 == 0
            || q.request.plan_digest != p.plan_digest
            || q.request.setup_id != p.setup_id
            || q.request.provision_id != p.provision_id
            || p.owner_public_key != q.subject_public_key
            || p.owner_binding != q.request.expected_fence.owner_binding
            || p.selection_digest
                != digest(&("oasis7.custody.resource-selection.v1", &q.request.selection))?
            || p.task_fence_digest
                != digest(&(
                    "oasis7.custody.resource-task-fence.v1",
                    &q.request.expected_fence,
                ))?
            || p.signer_record_digest
                != digest(&(
                    "oasis7.custody.resource-signer-record.v1",
                    &q.request.expected_signer_delegation,
                ))?
            || p.native_request_digest
                != digest(&("oasis7.custody.resource-native-original.v2", &original))?
            || p.world_request_digest
                != paid_world_request_digest(resource::RESOURCE_GOAL_PATH_V2, q)?
            || p.preflight_world_request_digest.as_deref()
                != Some(
                    paid_world_request_digest(
                        ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                        &self.request.access,
                    )?
                    .as_str(),
                )
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Bounds these exact fragments. The real transport/parser must still apply
    /// its original limit to the complete request (line, all headers and body).
    pub fn body_bytes(&self) -> Result<Vec<u8>> {
        let body = ResourceFinalNativeOriginalV2 {
            api_version: self.api_version,
            input: self.input.clone(),
            participant: self.participant.clone(),
            request: self.request.clone(),
        };
        let bytes = serde_json::to_vec(&body).map_err(|_| invalid())?;
        if bytes.len() > 64 * 1024 {
            return Err(invalid());
        }
        Ok(bytes)
    }
    pub fn proof_header(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&self.paid).map_err(|_| invalid())?;
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        if encoded.len() > 64 * 1024 {
            return Err(invalid());
        }
        Ok(encoded)
    }
    /// Header is the authentication-layer proof, never a field in the body.
    /// This codec does not replace configured issuer/floor verification.
    pub fn decode_http(body: &[u8], header: &str) -> Result<Self> {
        if body.len() > 64 * 1024 || header.len() > 64 * 1024 {
            return Err(invalid());
        }
        let original: ResourceFinalNativeOriginalV2 =
            serde_json::from_slice(body).map_err(|_| invalid())?;
        if serde_json::to_vec(&original).map_err(|_| invalid())? != body {
            return Err(invalid());
        }
        let bytes = URL_SAFE_NO_PAD.decode(header).map_err(|_| invalid())?;
        let paid: PaidResourceReadProofV1 =
            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if serde_json::to_vec(&paid).map_err(|_| invalid())? != bytes
            || URL_SAFE_NO_PAD.encode(&bytes) != header
        {
            return Err(invalid());
        }
        let value = Self {
            api_version: original.api_version,
            input: original.input,
            participant: original.participant,
            request: original.request,
            paid,
        };
        value.validate_bindings()?;
        Ok(value)
    }
}
