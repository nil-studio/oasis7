//! One Account service identity. Only child modules may sign typed Core tokens.
//! No HTTP handler or public arbitrary-byte signing method exists here.
use crate::directory_config::{DirectoryConfig, denied};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    AccountMachinePurposeV2,
};
use worker::{Env, Result};
use zeroize::Zeroizing;

pub(crate) struct AccountServiceIssuer {
    issuer: AccountMachineIssuerV2,
    pins: Vec<AccountMachineIssuerPinV2>,
    seed: Zeroizing<[u8; 32]>,
    native_root_key_id: String,
    native_root_public_key: String,
    native_owner_binding_pin: oasis7_custody_core::native_binding::NativeBindingPinV1,
}
impl AccountServiceIssuer {
    pub(crate) fn load(env: &Env) -> Result<Self> {
        let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
        if value("CUSTODY_ACCOUNT_SERVICE_ISSUER_ENABLED")? != "true" {
            return Err(denied());
        }
        let directory = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
        let pins: Vec<AccountMachineIssuerPinV2> =
            serde_json::from_str(&value("CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS")?)
                .map_err(|_| denied())?;
        let issuer: AccountMachineIssuerV2 =
            serde_json::from_str(&value("CUSTODY_ACCOUNT_SERVICE_ISSUER")?)
                .map_err(|_| denied())?;
        if pins.is_empty()
            || pins.len() > 16
            || issuer.environment != directory.environment
            || issuer.issuer_epoch.0 == 0
            || issuer.config_generation.0 == 0
        {
            return Err(denied());
        }
        let seed_text = Zeroizing::new(
            env.secret("CUSTODY_ACCOUNT_SERVICE_SIGNING_SEED")
                .map_err(|_| denied())?
                .to_string(),
        );
        let mut seed = Zeroizing::new([0u8; 32]);
        if seed_text.len() != 64 {
            return Err(denied());
        }
        hex::decode_to_slice(seed_text.as_bytes(), seed.as_mut_slice()).map_err(|_| denied())?;
        if hex::encode(seed.as_slice()) != seed_text.as_str() {
            return Err(denied());
        }
        let public = hex::encode(SigningKey::from_bytes(&seed).verifying_key().as_bytes());
        let native_root = value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?;
        let root_bytes: [u8; 32] = hex::decode(&native_root)
            .map_err(|_| denied())?
            .try_into()
            .map_err(|_| denied())?;
        if hex::encode(root_bytes) != native_root {
            return Err(denied());
        }
        let native_root_id = value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?;
        let binding: oasis7_custody_core::native_binding::NativeBindingPinV1 =
            serde_json::from_str(&value("CUSTODY_NATIVE_BINDING_OWNER_PIN")?)
                .map_err(|_| denied())?;
        binding.validate().map_err(|_| denied())?;
        if public == native_root
            || binding.public_key == public
            || issuer.key_id == native_root_id
            || issuer.key_id == binding.key_id
            || issuer.service_id == binding.service_identity
        {
            return Err(denied());
        }
        for (index, pin) in pins.iter().enumerate() {
            if pin.public_key != public
                || pins[..index]
                    .iter()
                    .any(|p| p.allowed_purpose == pin.allowed_purpose)
                || pin.namespace != issuer.namespace
                || pin.service_id != issuer.service_id
                || pin.environment != issuer.environment
                || pin.key_id != issuer.key_id
                || pin.issuer_epoch != issuer.issuer_epoch
                || pin.config_generation != issuer.config_generation
                || pin.native_audience_origin != issuer.native_audience_origin
                || pin.not_before.0 >= pin.not_after.0
            {
                return Err(denied());
            }
        }
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: issuer.issuer_epoch,
            config_generation: issuer.config_generation,
        };
        for pin in &pins {
            issuer
                .verify_pin(
                    pin,
                    &floor,
                    pin.allowed_purpose,
                    pin.not_before.0,
                    pin.not_after.0,
                    pin.not_before.0,
                )
                .map_err(|_| denied())?;
        }
        Ok(Self {
            issuer,
            pins,
            seed,
            native_root_key_id: native_root_id,
            native_root_public_key: native_root,
            native_owner_binding_pin: binding,
        })
    }
    pub(crate) fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.issuer
    }
    /// Fixed public deployment pins only; not Current, Source or consent authority.
    /// No Identity adapter secret or binding/Owner signing seed is loaded here.
    pub(crate) fn native_scope(&self) -> oasis7_custody_core::provisioning::NativeStageScope<'_> {
        oasis7_custody_core::provisioning::NativeStageScope {
            http_origin: &self.issuer.native_audience_origin,
            native_root_key_id: &self.native_root_key_id,
            native_root_public_key: &self.native_root_public_key,
            owner_binding_pin: &self.native_owner_binding_pin,
        }
    }
    // Child typed producers must first compare their Core token's issuer and
    // lifetime to this configuration. Caller-supplied purpose/bytes never reach
    // an HTTP route and cannot select a different signing domain.
    fn validate_lifetime(
        &self,
        purpose: AccountMachinePurposeV2,
        issued: u64,
        expires: u64,
        now: u64,
    ) -> Result<()> {
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == purpose)
            .ok_or_else(denied)?;
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        self.issuer
            .verify_pin(pin, &floor, purpose, issued, expires, now)
            .map_err(|_| denied())
    }
    fn sign_bytes(&self, bytes: &[u8]) -> [u8; 64] {
        SigningKey::from_bytes(&self.seed).sign(bytes).to_bytes()
    }
}

#[path = "account_service_issuer_execution.rs"]
mod execution;
#[path = "account_service_issuer_native.rs"]
mod native;

#[path = "account_service_issuer_preflight.rs"]
mod preflight;

#[path = "account_service_issuer_execution_native.rs"]
mod execution_native;

#[path = "account_service_issuer_resource_final.rs"]
mod resource_final;

#[path = "account_service_issuer_execution_dispatch.rs"]
mod execution_dispatch;
