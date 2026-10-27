//! Fixed service authority for native binding statements. Never a per-account role key.
use crate::directory_config::{DirectoryConfig, denied};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::native_binding::{
    NativeBindingPinV1, OwnerNativeBindingStatementV1, SignedOwnerNativeBindingStatementV1,
};
use worker::{Env, Result};
use zeroize::Zeroizing;

pub(super) struct NativeConfig {
    pub http_origin: String,
    pub native_root_key_id: String,
    pub native_root_public_key: String,
    pub owner_pin: NativeBindingPinV1,
    // No Debug, Clone or wire serialization; provisioning operational keys is out of scope.
    seed: Zeroizing<[u8; 32]>,
}
impl NativeConfig {
    pub fn load(env: &Env) -> Result<Self> {
        let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
        if value("CUSTODY_NATIVE_PROVISION_ENABLED")? != "true" {
            return Err(denied());
        }
        let http_origin = value("CUSTODY_NATIVE_ORIGIN")?;
        let url = worker::Url::parse(&http_origin).map_err(|_| denied())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.origin().ascii_serialization() != http_origin
        {
            return Err(denied());
        }
        let native_root_key_id = value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?;
        let native_root_public_key = value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?;
        let owner_pin: NativeBindingPinV1 =
            serde_json::from_str(&value("CUSTODY_NATIVE_BINDING_OWNER_PIN")?)
                .map_err(|_| denied())?;
        owner_pin.validate().map_err(|_| denied())?;
        let directory = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
        let epoch: oasis7_custody_core::DecimalU64 =
            serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| denied())?;
        if owner_pin.environment != directory.environment || owner_pin.service_epoch != epoch {
            return Err(denied());
        }
        let seed_text = Zeroizing::new(
            env.secret("CUSTODY_NATIVE_BINDING_SIGNING_SEED")
                .map_err(|_| denied())?
                .to_string(),
        );
        let mut seed = Zeroizing::new([0u8; 32]);
        if seed_text.len() != 64 {
            return Err(denied());
        }
        hex::decode_to_slice(seed_text.as_bytes(), seed.as_mut_slice()).map_err(|_| denied())?;
        if hex::encode(seed.as_slice()) != seed_text.as_str()
            || hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes())
                != owner_pin.public_key
            || owner_pin.public_key == native_root_public_key
            || owner_pin.key_id == native_root_key_id
        {
            return Err(denied());
        }
        let native_root: [u8; 32] = hex::decode(&native_root_public_key)
            .map_err(|_| denied())?
            .try_into()
            .map_err(|_| denied())?;
        if hex::encode(native_root) != native_root_public_key
            || native_root_key_id.is_empty()
            || native_root_key_id.len() > 256
        {
            return Err(denied());
        }
        Ok(Self {
            http_origin,
            native_root_key_id,
            native_root_public_key,
            owner_pin,
            seed,
        })
    }
    /// The Account must register and verify this exact statement before its signature is returned.
    /// This private helper accepts no HTTP bytes and grants no dispatch rights by itself.
    pub fn sign(
        &self,
        statement: OwnerNativeBindingStatementV1,
        now: u64,
    ) -> Result<SignedOwnerNativeBindingStatementV1> {
        self.owner_pin
            .validate_statement(&statement, now)
            .map_err(|_| denied())?;
        let bytes = statement.signing_bytes().map_err(|_| denied())?;
        let signature =
            URL_SAFE_NO_PAD.encode(SigningKey::from_bytes(&self.seed).sign(&bytes).to_bytes());
        Ok(SignedOwnerNativeBindingStatementV1 {
            statement,
            signature,
        })
    }
}
