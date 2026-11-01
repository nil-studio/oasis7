//! Account service proof for one exact Executor Prepare original.
//! Public wire is not authority until configured pin/floor and exact recipient
//! checks succeed; it cannot authorize Native Term/N, raw signing, or a send.
use super::*;
use crate::{
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2,
    },
    role_key_slot::RoleKeySlotScopeV1,
    time_units::UnixMillis,
};
pub const EXECUTION_PREPARE_SIGNER_DOMAIN_V2: &str = "oasis7.custody.execution-prepare-signer.v2";
pub const EXECUTION_PREPARE_SIGNER_ROUTE_V2: &str = "/internal/role-key/execution-prepare-v2";
pub const EXECUTION_PREPARE_SIGNER_INITIALIZE_ROUTE_V2: &str =
    "/internal/role-key/execution-prepare-initialize-v2";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareSignerOperationV2 {
    InitializeSigner,
    SignPrepare,
}
impl ExecutionPrepareSignerOperationV2 {
    fn route(self) -> &'static str {
        match self {
            Self::InitializeSigner => EXECUTION_PREPARE_SIGNER_INITIALIZE_ROUTE_V2,
            Self::SignPrepare => EXECUTION_PREPARE_SIGNER_ROUTE_V2,
        }
    }
}
pub struct ExecutionPrepareRecipientV2<'a> {
    pub scope: &'a RoleKeySlotScopeV1,
    pub key: &'a PublicRoleKey,
    pub operation: ExecutionPrepareSignerOperationV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPreparePermitsV2 {
    pub initialize: AccountExecutionPreparePermitV2,
    pub sign: AccountExecutionPreparePermitV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedExecutionPreparePermitsV2 {
    pub initialize: SignedAccountExecutionPreparePermitV2,
    pub sign: SignedAccountExecutionPreparePermitV2,
}
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountExecutionPreparePermitV2 {
    pub physical_graph: ExecutionPrepareRkvGraphV2,
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub purpose: AccountMachinePurposeV2,
    pub recipient_role: Role,
    pub operation: ExecutionPrepareSignerOperationV2,
    pub recipient_route: String,
    pub recipient_scope: RoleKeySlotScopeV1,
    pub recipient_key: PublicRoleKey,
    pub session_id: String,
    pub approved_input_digest: String,
    pub action_request_id: String,
    pub stage_operation_id: String,
    pub allocation_sequence: DecimalU64,
    pub credential_generation: DecimalU64,
    pub bundle_digest: String,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub source_observed_at: UnixMillis,
    pub source_expires_at: UnixMillis,
    pub physical_ingress_key: String,
    pub rfc_signed_attempt_digest: String,
    pub issued_at: UnixMillis,
    pub expires_at: UnixMillis,
    pub directory_observed_at: DecimalU64,
    pub fence_started: DecimalU64,
}
impl core::fmt::Debug for AccountExecutionPreparePermitV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountExecutionPreparePermitV2(<unverified-public-proof>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountExecutionPreparePermitV2 {
    pub permit: AccountExecutionPreparePermitV2,
    pub signature_hex: String,
}
impl core::fmt::Debug for SignedAccountExecutionPreparePermitV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountExecutionPreparePermitV2(<unverified-public-proof>)")
    }
}
impl ExecutionPrepareSigningAuthorityV2 {
    /// Consumed by the fixed issuer child inside the same Account transaction.
    /// No caller Bundle/raw JSON can construct this authority or select purpose.
    pub fn into_service_permits(
        self,
        issuer: AccountMachineIssuerV2,
        directory_observed_at: DecimalU64,
        fence_started: DecimalU64,
        observation_until: UnixMillis,
    ) -> Result<ExecutionPreparePermitsV2> {
        let b = self.bundle();
        if !matches!(b.action.as_ref(), ExecutionSigningActionV2::Prepare { .. })
            || issuer.environment != self.executor_scope().context.identity.environment
        {
            return Err(invalid());
        }
        let sign = AccountExecutionPreparePermitV2 {
            physical_graph: ExecutionPrepareRkvGraphV2::ExecutorServiceAndActorV2,
            api_version: 2,
            issuer,
            purpose: AccountMachinePurposeV2::ExecutionPrepareSignerV2,
            recipient_role: Role::Executor,
            operation: ExecutionPrepareSignerOperationV2::SignPrepare,
            recipient_route: EXECUTION_PREPARE_SIGNER_ROUTE_V2.into(),
            recipient_scope: self.executor_scope().clone(),
            recipient_key: self.executor_key().clone(),
            session_id: b.session_id.clone(),
            approved_input_digest: b.approved_input_digest.clone(),
            action_request_id: b.action_request_id.clone(),
            stage_operation_id: b.stage_operation_id.clone(),
            allocation_sequence: b.allocation_sequence,
            credential_generation: self.credential_generation(),
            bundle_digest: b.digest()?,
            source_operation_id: b.source_operation_id.clone(),
            source_nonce: b.source_nonce.clone(),
            source_observed_at: self.source_observed_at(),
            source_expires_at: self.source_expires_at(),
            physical_ingress_key: self.paid_key().into(),
            rfc_signed_attempt_digest: self.paid_signed_attempt_digest().into(),
            issued_at: self.admitted_at(),
            expires_at: UnixMillis::new(self.expires_at().get().min(observation_until.get())),
            directory_observed_at,
            fence_started,
        };
        let mut initialize = sign.clone();
        initialize.operation = ExecutionPrepareSignerOperationV2::InitializeSigner;
        initialize.recipient_route = EXECUTION_PREPARE_SIGNER_INITIALIZE_ROUTE_V2.into();
        Ok(ExecutionPreparePermitsV2 { initialize, sign })
    }
}
impl AccountExecutionPreparePermitV2 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        oasis7_wasm_abi::encode_canonical_cbor(&(EXECUTION_PREPARE_SIGNER_DOMAIN_V2, self))
            .map_err(|_| invalid())
    }
    /// `scope` and `key` are read from the actual receiving Vault slot; `bundle`
    /// comes from the fixed operation RPC and is committed by this strong proof.
    /// This never substitutes for fresh RoleKeyCurrent/DFRF, once admission,
    /// KEK checks, immutable slot checks or post-publish revalidation.
    pub fn validate_for(
        &self,
        bundle: &ExecutionSigningBundleV2,
        recipient: ExecutionPrepareRecipientV2<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let ExecutionPrepareRecipientV2 {
            scope,
            key,
            operation,
        } = recipient;
        scope.validate().map_err(|_| invalid())?;
        if self.api_version != 2
            || self.purpose != AccountMachinePurposeV2::ExecutionPrepareSignerV2
            || self.recipient_role != Role::Executor
            || scope.context.identity.role != Role::Executor
            || self.operation != operation
            || self.recipient_route != operation.route()
            || &self.recipient_scope != scope
            || &self.recipient_key != key
            || self.issuer.environment != scope.context.identity.environment
            || key.key_id != scope.context.identity.key_id
            || key.key_version != scope.context.identity.key_version
            || key.public_key == bundle.fence.owner_public_key
            || bundle
                .fence
                .executor_binding
                .as_ref()
                .is_none_or(|v| v.executor_public_key != key.public_key)
            || !matches!(
                bundle.action.as_ref(),
                ExecutionSigningActionV2::Prepare { .. }
            )
            || self.bundle_digest != bundle.digest()?
            || self.session_id != bundle.session_id
            || self.approved_input_digest != bundle.approved_input_digest
            || self.action_request_id != bundle.action_request_id
            || self.stage_operation_id != bundle.stage_operation_id
            || self.allocation_sequence != bundle.allocation_sequence
            || self.credential_generation.0 == 0
            || self.source_operation_id != bundle.source_operation_id
            || self.source_nonce != bundle.source_nonce
            || self.source_observed_at.get() > self.issued_at.get()
            || self.source_expires_at.get() <= self.source_observed_at.get()
            || self.directory_observed_at.0 > self.issued_at.get()
            || self.fence_started.0 > self.issued_at.get()
            || self
                .directory_observed_at
                .0
                .checked_add(5000)
                .is_none_or(|until| self.expires_at.get() > until)
            || self
                .fence_started
                .0
                .checked_add(5000)
                .is_none_or(|until| self.expires_at.get() > until)
            || self.issued_at.get() > now.get()
            || now.get() >= self.expires_at.get()
            || self.expires_at.get() <= self.issued_at.get()
            || self.expires_at.get() > bundle.execution_deadline.get()
            || self.expires_at.get() > self.source_expires_at.get()
            || self
                .source_observed_at
                .get()
                .checked_add(5000)
                .is_none_or(|until| self.expires_at.get() > until)
            || self.physical_ingress_key.len() <= 256
            || !self.physical_ingress_key.is_ascii()
            || self.physical_ingress_key.chars().any(char::is_control)
            || self.rfc_signed_attempt_digest.len() != 64
            || !self
                .rfc_signed_attempt_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SignedAccountExecutionPreparePermitV2 {
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        bundle: &ExecutionSigningBundleV2,
        recipient: ExecutionPrepareRecipientV2<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        self.permit.validate_for(bundle, recipient, now)?;
        self.permit
            .issuer
            .verify_pin(
                pin,
                floor,
                AccountMachinePurposeV2::ExecutionPrepareSignerV2,
                self.permit.issued_at.get(),
                self.permit.expires_at.get(),
                now.get(),
            )
            .map_err(|_| invalid())?;
        crate::native_cleanup_v2::verify(
            EXECUTION_PREPARE_SIGNER_DOMAIN_V2,
            &self.permit,
            &self.signature_hex,
            &pin.public_key,
        )
        .map_err(|_| invalid())
    }
}

/// Strong, exact recipient verification is the only construction path. This
/// token is neither a physical once claim nor a key-open permission.
pub struct VerifiedExecutionPreparePermitV2 {
    proof: SignedAccountExecutionPreparePermitV2,
    bundle: ExecutionSigningBundleV2,
}
impl SignedAccountExecutionPreparePermitV2 {
    pub fn verify_owned(
        self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        bundle: ExecutionSigningBundleV2,
        recipient: ExecutionPrepareRecipientV2<'_>,
        now: UnixMillis,
    ) -> Result<VerifiedExecutionPreparePermitV2> {
        self.verify(pin, floor, &bundle, recipient, now)?;
        Ok(VerifiedExecutionPreparePermitV2 {
            proof: self,
            bundle,
        })
    }
}
impl VerifiedExecutionPreparePermitV2 {
    pub fn proof(&self) -> &SignedAccountExecutionPreparePermitV2 {
        &self.proof
    }
    pub fn bundle(&self) -> &ExecutionSigningBundleV2 {
        &self.bundle
    }
    pub fn once_operation_digest(&self) -> Result<String> {
        digest(&(
            "oasis7.custody.execution-signer.once-operation.v2",
            &self.proof.permit.recipient_scope,
            &self.proof.permit.session_id,
            &self.proof.permit.action_request_id,
            &self.proof.permit.stage_operation_id,
        ))
    }
    pub fn proof_digest(&self) -> Result<String> {
        digest(&("oasis7.custody.execution-signer.permit.v2", &self.proof))
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareServiceRecordV2 {
    pub port_ids: [String; 4],
    pub permits: SignedExecutionPreparePermitsV2,
    pub recorded_at: UnixMillis,
}
impl core::fmt::Debug for ExecutionPrepareServiceRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionPrepareServiceRecordV2(<public-service-proofs-not-dispatch>)")
    }
}
