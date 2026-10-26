//! Read-only native facts. Digests identify originals; only fixed Root verification authenticates them.
use crate::{
    DecimalU64, approval::bytes_digest, http::Role, provisioning::NativeStageResultV1,
    role_key_slot::RoleKeySlotScopeV1,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidNativeHistory;
type Result<T> = core::result::Result<T, InvalidNativeHistory>;
fn id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
fn hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn check(v: bool) -> Result<()> {
    if v { Ok(()) } else { Err(InvalidNativeHistory) }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHistoryScopeV1 {
    pub role_scope: RoleKeySlotScopeV1,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub registration_operation_id: String,
    pub initial_login_operation_id: String,
}
impl NativeHistoryScopeV1 {
    pub fn validate(&self) -> Result<()> {
        self.role_scope
            .validate()
            .map_err(|_| InvalidNativeHistory)?;
        check(
            self.role_scope.context.identity.role == Role::Identity
                && id(&self.identity_namespace)
                && self.identity_instance_epoch.0 > 0
                && id(&self.registration_operation_id)
                && id(&self.initial_login_operation_id)
                && self.registration_operation_id != self.initial_login_operation_id,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeHistoryPhaseV1 {
    Register,
    Binding,
    InitialLogin,
    ManagedLogin,
}
/// A trusted fixed-Identity tightening fact, explicitly not a Root signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMissingSessionV1 {
    pub api_version: u8,
    pub scope: NativeHistoryScopeV1,
    pub phase: NativeHistoryPhaseV1,
    pub native_operation_id: String,
    pub original_permit_id: String,
    pub result_permit_id: String,
    pub stable_intent_digest: String,
}
impl NativeMissingSessionV1 {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        check(
            self.api_version == 1
                && matches!(
                    self.phase,
                    NativeHistoryPhaseV1::InitialLogin | NativeHistoryPhaseV1::ManagedLogin
                )
                && id(&self.original_permit_id)
                && self.result_permit_id == self.original_permit_id
                && self.stable_intent_digest
                    == stable_native_intent_digest(
                        &self.scope,
                        self.phase,
                        &self.native_operation_id,
                    )?,
        )
    }
}
/// This tuple and field projection preserve the original Identity manifest byte contract.
pub fn stable_native_intent_digest(
    scope: &NativeHistoryScopeV1,
    phase: NativeHistoryPhaseV1,
    native_operation_id: &str,
) -> Result<String> {
    scope.validate()?;
    check(id(native_operation_id))?;
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7.custody.native-manifest-intent.v1",
        scope,
        phase,
        native_operation_id,
    ))
    .map_err(|_| InvalidNativeHistory)?;
    Ok(bytes_digest(&bytes))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSessionStorageAttestationV1 {
    pub status_digest: String,
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
}
impl NativeSessionStorageAttestationV1 {
    pub fn validate(&self) -> Result<()> {
        check(
            hex(&self.status_digest) && id(&self.session_ref) && self.private_sid_generation.0 > 0,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHistoryEntryV1 {
    pub api_version: u8,
    pub environment: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub scope: NativeHistoryScopeV1,
    pub phase: NativeHistoryPhaseV1,
    pub native_operation_id: String,
    pub original_permit_id: String,
    pub result_permit_id: String,
    pub stable_intent_digest: String,
    pub result: NativeStageResultV1,
    pub session_attestation: Option<NativeSessionStorageAttestationV1>,
}
impl NativeHistoryEntryV1 {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        check(
            self.api_version == 1
                && self.environment == self.scope.role_scope.context.identity.environment
                && self.custody_account_id == self.scope.role_scope.context.identity.account_id
                && self.provision_id == self.scope.role_scope.provision_id
                && id(&self.original_permit_id)
                && id(&self.result_permit_id)
                && self.stable_intent_digest
                    == stable_native_intent_digest(
                        &self.scope,
                        self.phase,
                        &self.native_operation_id,
                    )?,
        )?;
        match (&self.phase, &self.result) {
            (NativeHistoryPhaseV1::Register, NativeStageResultV1::RegistrationChallenge(v)) => {
                v.validate().map_err(|_| InvalidNativeHistory)?
            }
            (NativeHistoryPhaseV1::Binding, NativeStageResultV1::BindingReceipt(v)) => {
                v.validate().map_err(|_| InvalidNativeHistory)?
            }
            (
                NativeHistoryPhaseV1::InitialLogin | NativeHistoryPhaseV1::ManagedLogin,
                NativeStageResultV1::SessionStatus(v),
            ) => {
                v.validate().map_err(|_| InvalidNativeHistory)?;
                if let Some(a) = &self.session_attestation {
                    a.validate()?;
                    check(
                        a.status_digest == v.status.digest().map_err(|_| InvalidNativeHistory)?
                            && a.session_ref == v.status.session_ref,
                    )?;
                }
            }
            _ => return Err(InvalidNativeHistory),
        }
        if !matches!(
            self.phase,
            NativeHistoryPhaseV1::InitialLogin | NativeHistoryPhaseV1::ManagedLogin
        ) && self.session_attestation.is_some()
        {
            return Err(InvalidNativeHistory);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.native-history-entry.v1",
                self,
            ))
            .map_err(|_| InvalidNativeHistory)?,
        ))
    }
}

/// Public, fixed historical verification pins. No Owner signing key, KEK or live grant is needed.
pub struct NativeHistoryVerificationContext<'a> {
    pub native_root_key_id: &'a str,
    pub native_root_public_key: &'a str,
    pub http_origin: &'a str,
}
