//! Trusted deployment policy for management routing. No browser selects its principal.
use oasis7_custody_core::{
    DecimalU64,
    approval::{ApprovedWorldV1, CustodySessionStatusV1, bytes_digest},
    provisioning::*,
};
use serde::Serialize;
use worker::{Env, Error, Result};

pub(crate) fn denied() -> Error {
    Error::RustError("custody management unavailable or rejected".into())
}
pub(crate) fn clock() -> Result<u64> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(denied());
    }
    Ok(n as u64)
}
pub(crate) fn digest<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|_| denied())?;
    Ok(bytes_digest(&raw))
}
pub(crate) struct DirectoryConfig {
    pub namespace: String,
    pub epoch: DecimalU64,
    pub environment: String,
    pub managers: Vec<BootstrapManager>,
    pub routes: ProvisionRoutes,
    pub world: ApprovedWorldV1,
    pub version: DecimalU64,
}
impl DirectoryConfig {
    pub fn from_env(env: &Env) -> Result<Option<Self>> {
        const KEYS: [&str; 7] = [
            "CUSTODY_NATIVE_NAMESPACE",
            "CUSTODY_NATIVE_INSTANCE_EPOCH",
            "CUSTODY_NATIVE_ENVIRONMENT",
            "CUSTODY_BOOTSTRAP_MANAGERS",
            "CUSTODY_PROVISION_ROUTES",
            "CUSTODY_PROVISION_WORLD_IDENTITY",
            "CUSTODY_POLICY_VERSION",
        ];
        // Native Identity pins alone do not enable management/provisioning.
        if !KEYS[3..].iter().any(|key| env.var(key).is_ok()) {
            return Ok(None);
        }
        let v = KEYS
            .iter()
            .map(|key| env.var(key).map(|v| v.to_string()).map_err(|_| denied()))
            .collect::<Result<Vec<_>>>()?;
        let epoch: DecimalU64 =
            serde_json::from_value(v[1].clone().into()).map_err(|_| denied())?;
        let version: DecimalU64 =
            serde_json::from_value(v[6].clone().into()).map_err(|_| denied())?;
        let managers: Vec<BootstrapManager> = serde_json::from_str(&v[3]).map_err(|_| denied())?;
        let routes: ProvisionRoutes = serde_json::from_str(&v[4]).map_err(|_| denied())?;
        let world: ApprovedWorldV1 = serde_json::from_str(&v[5]).map_err(|_| denied())?;
        let public_key: [u8; 32] = hex::decode(&world.service_public_key)
            .map_err(|_| denied())?
            .try_into()
            .map_err(|_| denied())?;
        if v[0].is_empty()
            || v[0].len() > 256
            || epoch.0 == 0
            || version.0 == 0
            || managers.len() > MAX_MANAGER_ASSOCIATIONS
            || routes.environment != v[2]
            || [&world.world_id, &world.genesis_digest, &world.branch_id]
                .iter()
                .any(|v| v.len() > 256)
            || routes.world_identity != world.world_id
            || hex::encode(public_key) != world.service_public_key
            || world.world_id.is_empty()
            || world.genesis_digest.is_empty()
            || world.branch_id.is_empty()
            || managers
                .iter()
                .any(|m| m.principal.identity_namespace != v[0])
        {
            return Err(denied());
        }
        Ok(Some(Self {
            namespace: v[0].clone(),
            epoch,
            environment: v[2].clone(),
            managers,
            routes,
            world,
            version,
        }))
    }
    pub fn principal(&self, status: &CustodySessionStatusV1) -> Result<ManagerPrincipal> {
        let now = clock()?;
        if status.api_version != 1
            || status.identity_namespace != self.namespace
            || status.identity_instance_epoch != self.epoch
            || status.account_id.is_empty()
            || status.session_ref.is_empty()
            || status.credential_ref.is_empty()
            || status.credential_version.0 == 0
            || status.security_version.0 == 0
            || status.policy_version.0 == 0
            || status.server_time.0 > now
            || now.saturating_sub(status.server_time.0) > 5000
            || now >= status.expires_at.0.min(status.idle_expires_at.0)
        {
            return Err(denied());
        }
        Ok(ManagerPrincipal {
            identity_namespace: self.namespace.clone(),
            native_account_id: status.account_id.clone(),
        })
    }
    pub fn policy(&self) -> ProvisionPolicy<'_> {
        ProvisionPolicy {
            routes: &self.routes,
            managers: &self.managers,
            version: self.version.0,
        }
    }
}
