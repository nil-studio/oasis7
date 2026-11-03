//! New paid Directory reference path; legacy fresh-status paths are unchanged.
use super::*;
use oasis7_custody_core::execution_pending_current_v2::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingDirectoryReplyV2 {
    pub api_version: u8,
    pub claim_digest: String,
    pub view: ProvisionGateView,
}
impl ManagerAssociations {
    pub(super) fn pending_execution_directory(
        &self,
        raw: &str,
        config: &DirectoryConfig,
    ) -> Result<serde_json::Value> {
        let proof: SignedExecutionPendingCurrentLeafV2 =
            serde_json::from_str(raw).map_err(|_| denied())?;
        let (actual, principal, _, _, _) =
            crate::pending_current_actor_once_v2::verify_reference(&self.env, &proof, clock()?)
                .map_err(|_| denied())?;
        if actual.environment != config.environment
            || actual.namespace != config.namespace
            || actual.epoch != config.epoch
            || actual.version != config.version
        {
            return Err(denied());
        }
        // Read the existing ordinary ledger for installation-scope validation only.
        // It is never created/repaired by this profile and does not emit a view yet.
        self.snapshot_for(config, &principal, false, true)?;
        let first = crate::pending_current_actor_once_v2::verify_and_claim(
            &self.state,
            &self.env,
            &self.object_id,
            &proof,
        )
        .map_err(|_| denied())?;
        let snapshot = self.snapshot_for(config, &principal, false, true)?;
        let value = self.provision_gate_view(
            config,
            &principal,
            &proof.claim.scope.provision_id,
            &proof.claim.scope.context.identity.account_id,
            snapshot,
        )?;
        let view: ProvisionGateView = serde_json::from_value(value).map_err(|_| denied())?;
        first
            .require_observed(view.observed_at.0, clock()?)
            .map_err(|_| denied())?;
        Ok(serde_json::to_value(PendingDirectoryReplyV2 {
            api_version: 2,
            claim_digest: proof.digest().map_err(|_| denied())?,
            view,
        })?)
    }
}
/// This signature accepts the first factory's exact signed proof, never a path
/// or arbitrary current-status/caller routing bundle.
pub(crate) async fn call_pending_directory(
    env: &Env,
    proof: &SignedExecutionPendingCurrentLeafV2,
) -> Result<String> {
    let (_, _, name, _, _) =
        crate::pending_current_actor_once_v2::verify_reference(env, proof, clock()?)
            .map_err(|_| denied())?;
    let raw = serde_json::to_string(proof)?;
    if raw.len() > 32768 {
        return Err(denied());
    }
    let stub = env
        .durable_object("MANAGER_ASSOCIATIONS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init(
        &format!("https://directory.internal{EXECUTION_PENDING_DIRECTORY_PATH_V2}"),
        &init,
    )?;
    request
        .headers_mut()?
        .set("Content-Type", "application/json")?;
    let response = stub.fetch_with_request(request).await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    read_response(response).await
}
