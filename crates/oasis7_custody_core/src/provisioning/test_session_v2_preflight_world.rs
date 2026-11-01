//! Actual two-port preflight profile allocation; no fungible budget is added.
use super::*;
use crate::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    AccountMachinePurposeV2, AccountPreflightAccessV2, NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
    NATIVE_PREFLIGHT_POLICY_V2_PATH, NativePreflightWorldOperationV2, NativePreflightWorldPortV2,
    PreflightPolicyBodyV2, SignedAccountPreflightAccessV2, SignedNativePreflightWorldReceiptV2,
};
use base64::Engine as _;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPreflightWorldV2 {
    pub(in crate::provisioning) certificate: Box<AccountPreflightAccessV2>,
    pub(in crate::provisioning) issuer_pin: AccountMachineIssuerPinV2,
    pub(in crate::provisioning) issuer_floor: AccountMachineIssuerFloorV2,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) signature_hex: Option<String>,
    pub(in crate::provisioning) receipts: [Option<SignedNativePreflightWorldReceiptV2>; 2],
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) admitted_at: Option<UnixMillis>,
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> core::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
/// Fixed issuer metadata, not a permission or a caller-supplied signature result.
pub struct NativePreflightAccessIssuerV2<'a> {
    pub issuer: AccountMachineIssuerV2,
    pub pin: &'a AccountMachineIssuerPinV2,
    pub floor: &'a AccountMachineIssuerFloorV2,
}
/// Only this Account-state factory creates the exact next signing input.
pub struct PreparedNativePreflightAccessV2 {
    id: String,
    certificate: AccountPreflightAccessV2,
    pin: AccountMachineIssuerPinV2,
}
impl PreparedNativePreflightAccessV2 {
    pub fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.certificate.issuer
    }
    pub fn issuer_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.pin
    }
    pub fn validity(&self) -> (u64, u64) {
        (self.certificate.issued_at.0, self.certificate.expires_at.0)
    }
    pub fn signing_bytes(&self) -> core::result::Result<Vec<u8>, String> {
        self.certificate.signing_bytes()
    }
}
fn fixed_root_signature(bytes: &[u8], signature: &str, key: &str) -> Result<()> {
    let signature: [u8; 64] = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| invalid())?
        .try_into()
        .map_err(|_| invalid())?;
    let key = crate::native_cleanup_v2::decode::<32>(key).map_err(|_| invalid())?;
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|_| invalid())?
        .verify_strict(bytes, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| invalid())
}
impl AccountProvisionApprovalState {
    /// New closed producer reserves both profiles in one Account transaction.
    /// No caller-selected step or bypass flag reaches the private mutation.
    pub fn claim_test_session_v2_preflight_world(
        &mut self,
        id: &str,
        issuer: NativePreflightAccessIssuerV2<'_>,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
    ) -> Result<PreparedNativePreflightAccessV2> {
        if self
            .test_session_v2_record(id)?
            .header()?
            .0
            .termination
            .cleanup_transport_policy
            != CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.reserve_preflight_step_fixed_v2(id, SessionPreflightStepV1::FreshAccessPair, now)?;
        let prepared = next.prepare_test_session_v2_preflight_world(id, issuer, native, now)?;
        *self = next;
        Ok(prepared)
    }
    /// This is called in the SAME SQL mutation immediately after FreshAccessPair.
    /// It creates only the explicitly approved two fixed World claims. The four
    /// Native requests were charged by the original step; neither pays the other.
    pub fn prepare_test_session_v2_preflight_world(
        &mut self,
        id: &str,
        issuer_scope: NativePreflightAccessIssuerV2<'_>,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
    ) -> Result<PreparedNativePreflightAccessV2> {
        let NativePreflightAccessIssuerV2 { issuer, pin, floor } = issuer_scope;
        let s = self.test_session_v2_record(id)?;
        let (plan, _) = s.header()?;
        let book = s
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        let _ = book;
        let claimed = *s
            .preflight_steps
            .get(&SessionPreflightStepV1::FreshAccessPair)
            .ok_or_else(invalid)?;
        if self.frozen()
            || s.closing.is_some()
            || s.approved_at.is_some()
            || !s.roots.is_empty()
            || s.preflight_world_access.is_some()
            || s.preflight_oracle_reads != 2
            || claimed != now
            || plan.termination.cleanup_transport_policy
                != CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
            || pin.native_audience_origin != native.http_origin
            || s.input.request.context.security_version != self.provision.security_epoch()
        {
            return Err(invalid());
        }
        let d = self.test_session_v2_preflight_descriptor(id)?;
        if s.source_oracle_reads.get(&d.source_operation_id) != Some(&3) {
            return Err(invalid());
        }
        let source = s.sources.get(&d.source_operation_id).ok_or_else(invalid)?;
        let verification = source.source.verification.as_ref().ok_or_else(invalid)?;
        let v = &verification.verification;
        let proof = &s.input.approval.proof;
        if proof.key_id != native.native_root_key_id
            || v.key_id != native.native_root_key_id
            || proof.identity_namespace != s.session.scope.identity_namespace
            || v.identity_namespace != proof.identity_namespace
            || v.version != 1
            || v.purpose != crate::approval::VerificationPurposeV1::InspectOrigin
            || v.proof_state != crate::approval::ProofStateV1::Active
            || v.origin_state != crate::approval::OriginStateV1::Active
            || v.approval_request_id != s.input.request.approval_request_id
            || v.audience != s.input.request.context.audience
            || v.manager_security_version != proof.manager_security_version
            || v.manager_policy_version != proof.manager_policy_version
            || v.origin_credential_version != proof.origin_credential_version
            || v.identity_instance_epoch != proof.identity_instance_epoch
            || v.verify_nonce != source.source.verify_nonce
            || v.intent_digest != s.input.request.intent_digest().map_err(|_| invalid())?
            || v.proof_digest != s.input.approval.digest().map_err(|_| invalid())?
            || v.observed_at.0 > now.get()
            || source.approved_input_digest != s.input_digest()?
        {
            return Err(invalid());
        }
        s.input
            .request
            .validate_native_summary_contract(
                proof.model_budget_summary_contract.as_ref(),
                &proof.manager_account_id,
                Some(native.native_root_public_key),
                proof.issued_at.0,
            )
            .map_err(|_| invalid())?;
        fixed_root_signature(
            &proof.signing_bytes().map_err(|_| invalid())?,
            &s.input.approval.signature,
            native.native_root_public_key,
        )?;
        fixed_root_signature(
            &v.signing_bytes().map_err(|_| invalid())?,
            &verification.signature,
            native.native_root_public_key,
        )?;
        let expires = [
            plan.write_deadline.0,
            proof.expires_at.0,
            source.source.expires_at.0,
            v.observed_at.0.checked_add(5000).ok_or_else(invalid)?,
            v.source_session_expires_at.0,
            v.source_session_idle_expires_at.0,
            v.proof_expires_at.0,
            pin.not_after.0,
        ]
        .into_iter()
        .min()
        .ok_or_else(invalid)?;
        issuer
            .verify_pin(
                pin,
                floor,
                AccountMachinePurposeV2::NativePreflightAccessV2,
                now.get(),
                expires,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let source_digest = digest(source)?;
        let certificate_id = digest(&(
            "oasis7.custody.preflight-world-allocation.v2",
            id,
            s.input_digest()?,
            &source_digest,
            claimed,
        ))?;
        let body = PreflightPolicyBodyV2 {
            observation_nonce: d.access_nonces.policy.clone(),
        };
        let port = |operation, path: &str, body_digest| -> Result<NativePreflightWorldPortV2> {
            let attempt = digest(&(
                "oasis7.custody.preflight-world-physical.v2",
                &certificate_id,
                operation,
                path,
                &body_digest,
            ))?;
            self.test_session_v2_transport_id_available(id, &attempt)?;
            Ok(NativePreflightWorldPortV2 {
                operation,
                physical_attempt_id: attempt,
                native_path: path.into(),
                world_path: "/v1/world/participant-policy".into(),
                body_digest,
            })
        };
        let ports = [
            port(
                NativePreflightWorldOperationV2::PolicyOriginal,
                NATIVE_PREFLIGHT_POLICY_V2_PATH,
                digest(&body)?,
            )?,
            port(
                NativePreflightWorldOperationV2::AuthorizePolicy,
                NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
                digest(&d.participant_owner)?,
            )?,
        ];
        let c = AccountPreflightAccessV2 {
            api_version: 2,
            issuer,
            certificate_id,
            input: Box::new(s.input.clone()),
            approved_input_digest: s.input_digest()?,
            descriptor: Box::new(d),
            source_record_digest: source_digest,
            fresh_access_claimed_at: DecimalU64(claimed.get()),
            consumed_at: DecimalU64(s.consumed_at.ok_or_else(invalid)?.get()),
            issued_at: DecimalU64(now.get()),
            expires_at: DecimalU64(expires),
            ports,
        };
        c.validate_shape().map_err(|_| invalid())?;
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(id)?;
        saved.preflight_world_access = Some(TestSessionPreflightWorldV2 {
            certificate: Box::new(c.clone()),
            issuer_pin: pin.clone(),
            issuer_floor: floor.clone(),
            signature_hex: None,
            receipts: [None, None],
            admitted_at: None,
        });
        append_session_event_v2(saved, SessionEventKindV2::PreflightWorldAccessReserved, now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedNativePreflightAccessV2 {
            id: id.into(),
            certificate: c,
            pin: pin.clone(),
        })
    }
    pub fn finish_test_session_v2_preflight_world(
        &mut self,
        prepared: PreparedNativePreflightAccessV2,
        signature_hex: String,
        now: UnixMillis,
    ) -> Result<SignedAccountPreflightAccessV2> {
        let s = self.test_session_v2_record(&prepared.id)?;
        let saved = s.preflight_world_access.as_ref().ok_or_else(invalid)?;
        if saved.certificate.as_ref() != &prepared.certificate
            || saved.signature_hex.is_some()
            || self.frozen()
            || s.closing.is_some()
            || s.approved_at.is_some()
        {
            return Err(invalid());
        }
        let signed = SignedAccountPreflightAccessV2 {
            certificate: prepared.certificate,
            signature_hex,
        };
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: prepared.pin.issuer_epoch,
            config_generation: prepared.pin.config_generation,
        };
        signed
            .verify_issuer(&prepared.pin, &floor, now.get())
            .map_err(|_| invalid())?;
        let mut next = self.clone();
        next.test_session_v2_record_mut(&prepared.id)?
            .preflight_world_access
            .as_mut()
            .ok_or_else(invalid)?
            .signature_hex = Some(signed.signature_hex.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(signed)
    }
}
