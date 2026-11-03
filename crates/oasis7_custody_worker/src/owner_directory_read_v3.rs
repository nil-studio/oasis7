//! Read-only paid Directory leaf. Admission records are committed before the
//! actual business view; neither historical references nor rows grant Current.
use super::*;
use crate::directory_admission_policy_v3::DirectoryAdmissionPolicyV3;
use oasis7_custody_core::{read_directory_admission_v3::*, read_directory_v3::*};
#[wasm_bindgen(module = "/platform/read_directory_admission_sql_v3.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = claimReadDirectoryV3)]
    fn claim_sql(
        state: &JsValue,
        execution_capacity: u32,
        read_capacity: u32,
        row_bound: f64,
        transition: &js_sys::Function,
    ) -> std::result::Result<String, JsValue>;
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadDirectoryReplyV3 {
    pub api_version: u8,
    pub claim_digest: String,
    pub view: ProvisionGateView,
}
fn reference(proof: &SignedReadDirectoryV3) -> NativeInspectReference {
    let r = &proof.claim.query.manager_reference;
    NativeInspectReference {
        identity_namespace: r.identity_namespace.clone(),
        identity_instance_epoch: r.identity_instance_epoch,
        account_id: r.account_id.clone(),
        session_ref: r.session_ref.clone(),
        credential_ref: r.credential_ref.clone(),
        credential_version: r.credential_version,
        security_version: r.security_version,
        policy_version: r.policy_version,
        expires_at: r.expires_at,
        idle_expires_at: r.idle_expires_at,
    }
}
impl ManagerAssociations {
    pub(super) fn read_directory_v3(
        &self,
        raw: &str,
        directory: &DirectoryConfig,
    ) -> Result<serde_json::Value> {
        let proof = SignedReadDirectoryV3::decode_wire(raw.as_bytes()).map_err(|_| denied())?;
        let policy = DirectoryAdmissionPolicyV3::load(&self.env)?;
        let config = policy.read.as_ref().ok_or_else(denied)?;
        let active = config.active.as_ref().ok_or_else(denied)?;
        let principal = directory.inspection_reference_principal(&reference(&proof))?;
        let name = digest(
            "oasis7.custody.manager-directory.v1",
            &(&directory.environment, &principal),
        )?;
        let expected = self
            .env
            .durable_object("MANAGER_ASSOCIATIONS")?
            .id_from_name(&name)?
            .to_string();
        // Independent receiver target check before ANY SQL/schema/inventory.
        if expected != self.object_id {
            return Err(denied());
        }
        let account = self
            .env
            .durable_object("ACCOUNTS")?
            .id_from_name(&proof.claim.query.custody_account_id)?
            .to_string();
        proof
            .verify(active, &self.object_id, &account, clock()?)
            .map_err(|_| denied())?;
        let issuer = &proof.claim.coordinator.claim.issuer;
        let scope = ReadDirectoryAdmissionScopeV3 {
            object_id: self.object_id.clone(),
            environment: directory.environment.clone(),
            manager_identity_namespace: principal.identity_namespace.clone(),
            manager_identity_instance_epoch: directory.epoch,
            manager_account_id: principal.native_account_id.clone(),
            issuer_namespace: issuer.namespace.clone(),
            issuer_service_id: issuer.service_id.clone(),
        };
        let mut first = None;
        let mut transition = |inventory: String| -> std::result::Result<Array, JsValue> {
            let mut action = || -> Result<Array> {
                let old = policy.verify(&self.env, &self.object_id, &inventory)?;
                let now = clock()?;
                let (next, handle) = ReadDirectoryRowsV3::consume(
                    old,
                    config,
                    scope.clone(),
                    proof.clone(),
                    &account,
                    now,
                    |id| {
                        self.env
                            .durable_object("ACCOUNTS")
                            .and_then(|ns| ns.id_from_name(id).map(|id| id.to_string()))
                            .map_err(|_| "Read Directory account rejected".into())
                    },
                )
                .map_err(|_| denied())?;
                let values = next.storage_transition(&proof).map_err(|_| denied())?;
                let out = Array::new();
                for value in values {
                    out.push(&value.into());
                }
                first = Some(handle);
                Ok(out)
            };
            action().map_err(|_| JsValue::from_str("Read Directory admission rejected"))
        };
        let closure = Closure::borrow_mut(&mut transition);
        let saved = claim_sql(
            &self.state,
            policy.execution_capacity,
            policy.read_capacity(),
            policy.row_bound as f64,
            closure.as_ref().unchecked_ref(),
        )
        .map_err(|_| denied())?;
        drop(closure);
        let first = first.ok_or_else(denied)?;
        // Commit has completed. Only now may ordinary business bytes/view be read.
        let stored: ReadDirectoryOnceV3 = serde_json::from_str(&saved).map_err(|_| denied())?;
        if stored.request != proof {
            return Err(denied());
        }
        let snapshot = self.snapshot_for(directory, &principal, false, true)?;
        let value = self.provision_gate_view(
            directory,
            &principal,
            &proof.claim.query.provision_id,
            &proof.claim.query.custody_account_id,
            snapshot,
        )?;
        let view: ProvisionGateView = serde_json::from_value(value).map_err(|_| denied())?;
        let now = clock()?;
        first
            .require_observed(view.observed_at.0, now)
            .map_err(|_| denied())?;
        proof
            .verify(active, &self.object_id, &account, now)
            .map_err(|_| denied())?;
        Ok(serde_json::to_value(ReadDirectoryReplyV3 {
            api_version: 3,
            claim_digest: proof.digest().map_err(|_| denied())?,
            view,
        })?)
    }
}
