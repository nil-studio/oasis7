//! Mandatory versioned outbox owned by the actual V2 session/approval authority.
//! This state never owns a second approval book or a separate transport cap.
use super::*;
use oasis7_agent_api::game::{GameExecutorControlV1, GameTaskRevokeAndCloseV2};
use oasis7_client_api::{
    paid_cleanup_transport_v1::PaidCleanupTransportV1,
    world_service::{
        SignedReadRequest, SignedServiceResponse,
        task_cleanup_v2::{TaskCleanupLookupV2, TaskCleanupObservationV2},
    },
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionTerminationStateV2 {
    pub(in crate::provisioning) schema_revision: u8,
    pub(in crate::provisioning) approved_intent_digest: String,
    pub(in crate::provisioning) approved_input_digest: String,
    pub(in crate::provisioning) approved_plan_digest: String,
    pub(in crate::provisioning) termination_plan_digest: String,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) stop: Option<TerminationStopAllocationV2>,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) close: Option<TerminationCloseAllocationV2>,
    pub(in crate::provisioning) transports: BTreeMap<String, TerminationPaidTransportV2>,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) archived_fact: Option<TerminationArchivedFactV2>,
}
impl core::fmt::Debug for TestSessionTerminationStateV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionTerminationStateV2(<private>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationStopAllocationV2 {
    pub(in crate::provisioning) payload: GameExecutorControlV1,
    pub(in crate::provisioning) allocated_at: UnixMillis,
    pub(in crate::provisioning) allocation_record_digest: String,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) signed: Option<Box<OwnerSetupOriginalV1>>,
    pub(in crate::provisioning) recovery: TerminationStopRecoveryV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationCloseAllocationV2 {
    pub(in crate::provisioning) payload: GameTaskRevokeAndCloseV2,
    pub(in crate::provisioning) allocated_at: UnixMillis,
    pub(in crate::provisioning) allocation_record_digest: String,
    pub(in crate::provisioning) source_operation_id: String,
    pub(in crate::provisioning) source_record_digest: String,
    pub(in crate::provisioning) source_until: UnixMillis,
    pub(in crate::provisioning) native_until: UnixMillis,
    pub(in crate::provisioning) native_lifetime: SignedManagedNativeSessionLifetimeV1,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) signed: Option<SignedReadRequest<GameTaskRevokeAndCloseV2>>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationPaidTransportV2 {
    pub(in crate::provisioning) payload: PaidCleanupTransportV1,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) issuer_signature: Option<String>,
    /// Sender claim reserves one Native dispatch. It does not assert that the
    /// nested World send, receipt or canonical effect actually occurred.
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) dispatch_claimed_at: Option<UnixMillis>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationArchivedFactV2 {
    pub(in crate::provisioning) request: TaskCleanupLookupV2,
    pub(in crate::provisioning) response: TerminationSignedObservationV2,
}
/// Same three signature-codec fields, closed for persistence and Eq because
/// this particular typed payload has no floating-point/opaque JSON fields.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationSignedObservationV2 {
    pub(in crate::provisioning) request_digest: String,
    pub(in crate::provisioning) payload: TaskCleanupObservationV2,
    pub(in crate::provisioning) signature_hex: String,
}
impl From<SignedServiceResponse<TaskCleanupObservationV2>> for TerminationSignedObservationV2 {
    fn from(value: SignedServiceResponse<TaskCleanupObservationV2>) -> Self {
        Self {
            request_digest: value.request_digest,
            payload: value.payload,
            signature_hex: value.signature_hex,
        }
    }
}
impl TerminationSignedObservationV2 {
    pub(in crate::provisioning) fn as_signed(
        &self,
    ) -> SignedServiceResponse<TaskCleanupObservationV2> {
        SignedServiceResponse {
            request_digest: self.request_digest.clone(),
            payload: self.payload.clone(),
            signature_hex: self.signature_hex.clone(),
        }
    }
}
fn required<'de, D, T>(d: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
fn digest_string(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl TerminationCloseAllocationV2 {
    pub(in crate::provisioning) fn record_digest(&self, input: &str, plan: &str) -> Result<String> {
        crate::provisioning::types::digest(&(
            "oasis7.custody.close-allocation.v2",
            input,
            plan,
            &self.payload,
            &self.source_operation_id,
            &self.source_record_digest,
            &self.native_lifetime,
            self.allocated_at,
            self.source_until,
            self.native_until,
        ))
        .map_err(ProvisionApprovalError::from)
    }
}
impl TestSessionTerminationStateV2 {
    /// Saved facts only. A trusted external NativeStageScope is still required
    /// to verify Root signatures; this iterator cannot grant current authority.
    pub(in crate::provisioning) fn stored_native_lifetimes(
        &self,
    ) -> impl Iterator<Item = &SignedManagedNativeSessionLifetimeV1> {
        self.close
            .iter()
            .map(|c| &c.native_lifetime)
            .chain(self.stop.iter().map(|s| &s.recovery.native_lifetime))
            .chain(
                self.stop
                    .iter()
                    .filter_map(|s| s.recovery.admitted.as_ref().map(|a| &a.native_lifetime)),
            )
            .chain(self.stop.iter().flat_map(|s| {
                [&s.recovery.submit_claim, &s.recovery.lookup_claim]
                    .into_iter()
                    .flatten()
                    .map(|c| &c.native_lifetime)
            }))
    }

    pub(in crate::provisioning) fn allocated_request_count(&self) -> u64 {
        u64::from(self.stop.is_some()) + u64::from(self.close.is_some())
    }
    /// Two explicit billed indices in this candidate forwarding profile only.
    /// It does not count other Native challenge/signing/Source/RPC graph ports.
    pub(in crate::provisioning) fn paid_transport_count(&self) -> Result<u64> {
        u64::try_from(self.transports.len())
            .ok()
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| {
                n.checked_add(self.stop.as_ref().map_or(0, |s| {
                    u64::from(s.recovery.submit_claim.is_some())
                        + u64::from(s.recovery.lookup_claim.is_some())
                }))
            })
            .ok_or(ProvisionApprovalError::InvalidPersistence)
    }
    pub(in crate::provisioning) fn validate_source_binding(
        &self,
        sources: &BTreeMap<String, TestSessionSourceV2>,
        funding: &BTreeMap<String, SessionSourceFundingV1>,
    ) -> Result<()> {
        if let Some(stop) = &self.stop {
            stop.validate_source(sources, funding, &self.approved_input_digest)?;
        }
        if let Some(close) = &self.close {
            let wrapper = sources
                .get(&close.source_operation_id)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            if funding.get(&close.source_operation_id)
                != Some(&SessionSourceFundingV1::TerminationPlan)
                || wrapper.api_version != 2
                || wrapper.approved_input_digest != self.approved_input_digest
                || crate::provisioning::types::digest(wrapper)
                    .map_err(ProvisionApprovalError::from)?
                    != close.source_record_digest
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let v = &wrapper
                .source
                .verification
                .as_ref()
                .ok_or(ProvisionApprovalError::InvalidPersistence)?
                .verification;
            let source_until = v
                .observed_at
                .0
                .checked_add(5_000)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?
                .min(wrapper.source.expires_at.0)
                .min(v.proof_expires_at.0)
                .min(v.source_session_expires_at.0)
                .min(v.source_session_idle_expires_at.0);
            if close.source_until.get() != source_until
                || close.allocated_at.get() < v.observed_at.0
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        Ok(())
    }
    pub(in crate::provisioning) fn transport_id_in_use(&self, id: &str) -> bool {
        self.stop.as_ref().is_some_and(|s| {
            [&s.recovery.submit_claim, &s.recovery.lookup_claim]
                .into_iter()
                .flatten()
                .any(|c| c.attempt_id == id)
        }) || self.transports.contains_key(id)
            || self.transports.values().any(|record| {
                record.payload.native_transport_id == id || record.payload.world_transport_id == id
            })
    }
    pub(in crate::provisioning) fn armed(input: &ApprovalAdmissionInput) -> Result<Self> {
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        let state = Self {
            schema_revision: 2,
            approved_intent_digest: input
                .request
                .intent_digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
            approved_input_digest: crate::provisioning::types::digest(&(
                "oasis7.custody.test-session-approved-input.v2",
                input,
            ))
            .map_err(ProvisionApprovalError::from)?,
            approved_plan_digest: plan
                .digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
            termination_plan_digest: plan
                .termination
                .digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
            stop: None,
            close: None,
            transports: BTreeMap::new(),
            archived_fact: None,
        };
        state.validate_shape()?;
        Ok(state)
    }
    pub(in crate::provisioning) fn validate_input_binding(
        &self,
        input: &ApprovalAdmissionInput,
    ) -> Result<()> {
        let expected = Self::armed(input)?;
        if self.approved_intent_digest != expected.approved_intent_digest
            || self.approved_input_digest != expected.approved_input_digest
            || self.approved_plan_digest != expected.approved_plan_digest
            || self.termination_plan_digest != expected.termination_plan_digest
        {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
            return Err(ProvisionApprovalError::InvalidPersistence);
        };
        let term = &plan.termination;
        if self.stop.as_ref().is_some_and(|stop| {
            stop.payload.client_operation_id != term.stop_operation_id
                || stop.payload.fence != term.initial_fence
                || stop.payload.operation != oasis7_agent_api::game::GameExecutorOperationV1::Stop
        }) {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        if let Some(stop) = &self.stop {
            if stop.record_digest(&self.approved_input_digest, &self.approved_plan_digest)?
                != stop.allocation_record_digest
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            for result in stop.recovery.observations.values() {
                stop.validate_fact(term, result)?;
            }
            stop.validate_admitted(term, &self.approved_input_digest)?;
            if stop.recovery.original_until.get() > term.cleanup_until.get() {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        if let Some(close) = &self.close {
            if close.record_digest(&self.approved_input_digest, &self.approved_plan_digest)?
                != close.allocation_record_digest
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let p = &close.payload;
            let expected_stop = self
                .stop
                .as_ref()
                .map(|stop| {
                    oasis7_wasm_abi::canonical_hash(&(
                        oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN,
                        &term.initial_fence.owner_public_key,
                        &stop.payload,
                    ))
                    .map(
                        |intent_digest| oasis7_agent_api::game::GameOwnStopIntentV2 {
                            operation_id: stop.payload.client_operation_id.clone(),
                            intent_digest,
                        },
                    )
                    .map_err(|_| ProvisionApprovalError::InvalidPersistence)
                })
                .transpose()?;
            if p.client_operation_id != term.close_operation_id
                || p.custody_account_id != term.custody_account_id
                || p.permitted_branches.exact_initial_active != term.initial_fence
                || p.permitted_branches.own_stop_intent != expected_stop
                || p.signer_public_key != term.original_delegation.delegate_public_key
                || p.signer_generation.0 != term.original_delegation.generation
                || p.signer_nonce.0 != term.original_delegation.nonce
                || p.cleanup_until_ms.0 > term.cleanup_until.get()
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        if let Some(archive) = &self.archived_fact {
            let signed = self
                .close
                .as_ref()
                .and_then(|close| close.signed.as_ref())
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            if archive.request.submitted.original != *signed {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            oasis7_client_api::world_service::task_cleanup_v2::verify_cleanup_observation_v2(
                &archive.request,
                &archive.response.as_signed(),
                &term.world.service_public_key,
            )
            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if !matches!(archive.response.payload.outcome,
                oasis7_client_api::world_service::task_cleanup_v2::TaskCleanupOutcomeV2::KnownClosed {..}) {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let body = oasis7_client_api::cleanup_http_envelope_v1::CleanupTransportBodyV1::Lookup(
                Box::new(archive.request.clone()),
            );
            let native_digest = body
                .body_digest(
                    oasis7_client_api::paid_cleanup_transport_v1::CleanupConsumerRoleV1::Native,
                )
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            let world_digest = body
                .body_digest(
                    oasis7_client_api::paid_cleanup_transport_v1::CleanupConsumerRoleV1::World,
                )
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if !self.transports.values().any(|record| record.dispatch_claimed_at.is_some()
                && record.payload.operation==oasis7_client_api::paid_cleanup_transport_v1::CleanupHttpOperationV1::Lookup
                && record.payload.native_body_digest==native_digest
                && record.payload.world_body_digest==world_digest) {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        self.validate_shape()
    }
    pub(in crate::provisioning) fn validate_shape(&self) -> Result<()> {
        if self.schema_revision != 2
            || ![
                &self.approved_intent_digest,
                &self.approved_input_digest,
                &self.approved_plan_digest,
                &self.termination_plan_digest,
            ]
            .iter()
            .all(|v| digest_string(v))
        {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        if let Some(stop) = &self.stop {
            stop.validate_shape()?;
        }
        if let Some(close) = &self.close {
            close
                .payload
                .validate()
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if close.payload.termination_plan_digest != self.termination_plan_digest
                || close.allocated_at.get() == 0
                || close.allocated_at.get() >= close.payload.cleanup_until_ms.0
                || !digest_string(&close.allocation_record_digest)
                || !digest_string(&close.source_record_digest)
                || close.source_operation_id.trim().is_empty()
                || close.payload.cleanup_until_ms.0 > close.source_until.get()
                || close.payload.cleanup_until_ms.0 > close.native_until.get()
                || close.native_lifetime.validate().is_err()
                || close.native_until.get() > close.native_lifetime.lifetime.status.expires_at.0
                || close.native_until.get() > close.native_lifetime.lifetime.session_expires_at.0
                || close.native_until.get()
                    > close.native_lifetime.lifetime.session_idle_expires_at.0
                || close.signed.as_ref().is_some_and(|signed| {
                    signed.request != close.payload
                        || signed.subject_public_key
                            != close
                                .payload
                                .permitted_branches
                                .exact_initial_active
                                .owner_public_key
                })
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        } else if !self.transports.is_empty() || self.archived_fact.is_some() {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        if let Some(signed) = self.close.as_ref().and_then(|close| close.signed.as_ref()) {
            oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
                &signed.request,
                &signed
                    .request
                    .permitted_branches
                    .exact_initial_active
                    .owner_public_key,
                &signed.signature_hex,
            )
            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
        }
        let mut indexes = BTreeSet::new();
        let mut transport_ids = BTreeSet::new();
        if let Some(stop) = &self.stop {
            for c in [&stop.recovery.submit_claim, &stop.recovery.lookup_claim]
                .into_iter()
                .flatten()
            {
                if !indexes.insert(u64::from(c.paid_index))
                    || !transport_ids.insert(c.attempt_id.as_str())
                {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
            }
        }
        for (attempt, record) in &self.transports {
            let p = &record.payload;
            p.validate()
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            let signed = self
                .close
                .as_ref()
                .and_then(|c| c.signed.as_ref())
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            if !transport_ids.insert(p.attempt_id.as_str())
                || !transport_ids.insert(p.native_transport_id.as_str())
                || !transport_ids.insert(p.world_transport_id.as_str())
                || attempt != &p.attempt_id
                || p.approval_input_digest != self.approved_input_digest
                || p.approved_intent_digest != self.approved_intent_digest
                || p.test_session_plan_digest != self.approved_plan_digest
                || p.termination_plan_digest != self.termination_plan_digest
                || p.original_digest
                    != oasis7_wasm_abi::canonical_hash(signed)
                        .map_err(|_| ProvisionApprovalError::InvalidPersistence)?
                || !indexes.insert(p.native_paid_index.0)
                || !indexes.insert(p.world_paid_index.0)
                || record.dispatch_claimed_at.is_some_and(|at| {
                    record.issuer_signature.is_none()
                        || at.get() < p.issued_at.0
                        || at.get() >= p.expires_at.0
                })
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        Ok(())
    }
}
