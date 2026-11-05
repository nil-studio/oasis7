//! Explicit Native profile gate. No deployment configuration can select the fixture variant.
use super::*;
#[cfg(test)]
use oasis7_custody_core::provisioning::{TerminationPlanV3, TerminationTransportPolicyV3};
#[derive(Clone, Copy)]
pub(super) enum TerminationApprovalProfile {
    Production,
    #[cfg(test)]
    DevelopmentOnlyFixture,
}
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TerminationApprovalSummaryV3 {
    api_version: u8,
    kind: &'static str,
    subject_session_ref: String,
    custody_account_ref: String,
    world_id: String,
    world_branch: String,
    world_reorg_epoch: DecimalU64,
    task_id: String,
    irreversible: bool,
    owner_key_ref: String,
    executor_key_ref: String,
    signer_public_key: String,
    signer_generation: DecimalU64,
    nonce_policy: &'static str,
    requires_all_allocated_unknowns_inventory: bool,
    effects: [&'static str; 3],
    world_effect_limit: DecimalU64,
    native_subject: oasis7_custody_core::provisioning::NativeSubjectRefV3,
    transport_limit: DecimalU64,
    cleanup_until: DecimalU64,
    automatic_lookup_per_original: u8,
    fresh_recovery_lookup_per_original: u8,
    unknown_can_keep_session_open: bool,
    owner_signing_recovery_policy: &'static str,
    may_reopen_owner_seed_for_exact_recomputation: bool,
    max_recomputations_per_effect: DecimalU64,
    development_only: bool,
}
pub(super) fn enforce_profile(
    request: &ApprovalOptionsRequestV1,
    profile: TerminationApprovalProfile,
    now: u64,
) -> Result<Option<TerminationApprovalSummaryV3>, StoreError> {
    let CustodyIntentV1::TerminationPlanV3 { plan } = &request.intent else {
        return Ok(None);
    };
    request
        .validate(now)
        .map_err(|_| reject("invalid_termination_approval"))?;
    #[cfg(not(test))]
    let _ = plan;
    // All currently implemented policies are development-only. Production is
    // fail-closed even in a test build or when an unrelated feature flag is true.
    match profile {
        TerminationApprovalProfile::Production => Err(reject("unsupported_termination_policy")),
        #[cfg(test)]
        TerminationApprovalProfile::DevelopmentOnlyFixture => Ok(Some(summary(plan))),
    }
}
#[cfg(test)]
fn summary(plan: &TerminationPlanV3) -> TerminationApprovalSummaryV3 {
    let development_only = matches!(
        &plan.transport_policy,
        TerminationTransportPolicyV3::DevelopmentMeasuredGraphV1 { .. }
    );
    TerminationApprovalSummaryV3 {
        api_version: 3,
        kind: "termination_plan_v3",
        subject_session_ref: plan.subject_session_id.clone(),
        custody_account_ref: plan.custody_account_id.clone(),
        world_id: plan.world.world_id.clone(),
        world_branch: plan.world.branch_id.clone(),
        world_reorg_epoch: plan.world.reorg_epoch,
        task_id: plan.initial_fence.task_id.clone(),
        owner_key_ref: plan.owner_key_ref.clone(),
        executor_key_ref: plan.executor_key_ref.clone(),
        signer_public_key: plan.original_delegation.delegate_public_key.clone(),
        signer_generation: plan.original_delegation.generation,
        nonce_policy: "account_allocated_high_water_v1",
        requires_all_allocated_unknowns_inventory: true,
        irreversible: true,
        effects: [
            "guarded_stop_v2",
            "revoke_exact_signer",
            "permanent_world_closed_v3_and_detached",
        ],
        world_effect_limit: plan.request_limit,
        native_subject: plan.native_subject.clone(),
        transport_limit: plan.transport_limit,
        cleanup_until: DecimalU64(plan.cleanup_until.get()),
        automatic_lookup_per_original: 1,
        fresh_recovery_lookup_per_original: 1,
        unknown_can_keep_session_open: true,
        owner_signing_recovery_policy: "exact_original_ed25519_recovery_v1",
        may_reopen_owner_seed_for_exact_recomputation: true,
        max_recomputations_per_effect: DecimalU64(
            plan.owner_signing_recovery_policy
                .max_recomputations_per_effect(),
        ),
        development_only,
    }
}
pub(super) fn response(
    record: &ApprovalRecord,
    now: u64,
    profile: TerminationApprovalProfile,
) -> Result<Value, StoreError> {
    let summary = enforce_profile(&record.request, profile, now)?;
    let mut result = super::record_response(record, now)?;
    if let Some(summary) = summary {
        result["termination_summary"] =
            serde_json::to_value(summary).map_err(|_| StoreError::Corrupt)?;
    }
    Ok(result)
}
#[cfg(test)]
impl IdentityService {
    /// Test-only internal profile entry, with the same session cookie and CSRF
    /// checks as the browser router; it is never mounted on an HTTP route.
    pub(super) fn custody_browser_termination_v3_fixture(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        payload: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        if !self.clock.observe(now) {
            return Err(reject("clock_unhealthy"));
        }
        let sid = super::super::service::cookie_value(headers, "__Host-oasis7_sid")
            .ok_or_else(|| reject("session_expired"))?;
        let csrf = super::digest(&self.config.session_key, "csrf", sid);
        self.validate_json_write(headers, &csrf)?;
        match (method, path) {
            ("POST", "/custody/approvals/options") => self.custody_options_profile(
                Some(sid),
                payload,
                source,
                now,
                TerminationApprovalProfile::DevelopmentOnlyFixture,
            ),
            ("POST", "/custody/approvals/complete") => self.custody_complete_profile(
                Some(sid),
                payload,
                source,
                now,
                TerminationApprovalProfile::DevelopmentOnlyFixture,
            ),
            _ => Err(reject("unsupported_fixture_route")),
        }
    }
    /// This route retains the real service-auth/strict proof/current checks.
    pub(super) fn custody_verify_termination_v3_fixture(
        &mut self,
        method: &str,
        headers: &str,
        payload: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        if !self.clock.observe(now) {
            return Err(reject("clock_unhealthy"));
        }
        self.custody_verify_route_profile(
            method,
            headers,
            payload,
            source,
            now,
            TerminationApprovalProfile::DevelopmentOnlyFixture,
        )
    }
}
