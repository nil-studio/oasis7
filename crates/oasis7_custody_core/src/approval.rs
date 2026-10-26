//! Typed native approval contracts. All timestamps are Unix milliseconds.
use crate::DecimalU64;
use oasis7_agent_api::{
    game::GameTaskFenceV1,
    world_action::{MaterialLedgerId, MaterialTransitPriority},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalContextV1 {
    pub environment: String,
    pub custody_account_id: String,
    pub audience: String,
    pub policy_version: DecimalU64,
    pub security_version: DecimalU64,
    pub service_epoch: DecimalU64,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CustodyIntentV1 {
    Provision {
        provision_id: String,
        purpose: String,
        plan_digest: String,
        bootstrap_config_version: DecimalU64,
        world_identity: ApprovedWorldV1,
        identity_route: String,
        world_route: String,
        native_identity_namespace: String,
        register_request_id: String,
        login_request_id: String,
        deadline: DecimalU64,
        transport_limit: DecimalU64,
    },
    ManagedLogin {
        native_account_id: String,
        login_key_ref: String,
        native_identity_namespace: String,
        blocked_operation_ref: String,
        deadline: DecimalU64,
    },
    TestSession {
        session_id: String,
        native_account_id: String,
        world_identity: Box<ApprovedWorldV1>,
        task_fence: Box<GameTaskFenceV1>,
        executor_key_ref: String,
        client_binding: ApprovedClientBindingV1,
        runtime_ref: String,
        skill_ref: String,
        source_ref: String,
        actions: Vec<ApprovedActionV1>,
        allowed_resource_refs: Vec<String>,
        read_budget: PurposeBudgetV1,
        cleanup_budget: PurposeBudgetV1,
        control_budget: ControlBudgetV1,
        write_deadline: DecimalU64,
        read_until: DecimalU64,
        cleanup_until: DecimalU64,
        request_limit: DecimalU64,
        transport_limit: DecimalU64,
        business_limit: DecimalU64,
        termination_plan_digest: String,
    },
}

/// Full world pins and task fence are signed, not supplied as opaque client claims.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedWorldV1 {
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub service_public_key: String,
}
impl ApprovedWorldV1 {
    pub(crate) fn valid(&self) -> bool {
        [&self.world_id, &self.genesis_digest, &self.branch_id]
            .iter()
            .all(|v| bounded(v))
            && hex_digest(&self.service_public_key)
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "profile", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ApprovedClientBindingV1 {
    NativeBrokerCustodyV1 {
        client_key: String,
    },
    DotMcpCustodyV1 {
        client_key: String,
        connector_key: String,
    },
}
impl ApprovedClientBindingV1 {
    fn valid(&self, hops: u8) -> bool {
        match self {
            Self::NativeBrokerCustodyV1 { client_key } => hex_digest(client_key) && hops == 1,
            Self::DotMcpCustodyV1 {
                client_key,
                connector_key,
            } => {
                hex_digest(client_key)
                    && hex_digest(connector_key)
                    && client_key != connector_key
                    && hops == 2
            }
        }
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurposeBudgetV1 {
    pub requests: DecimalU64,
    pub transport: DecimalU64,
    pub business: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlBudgetV1 {
    pub requests: DecimalU64,
    pub transport_per_hop: DecimalU64,
    pub identity_transport: DecimalU64,
    pub transport_hops: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DecimalI64(pub i64);
impl From<DecimalI64> for String {
    fn from(value: DecimalI64) -> Self {
        value.0.to_string()
    }
}
impl TryFrom<String> for DecimalI64 {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let parsed = value
            .parse::<i64>()
            .map_err(|_| "invalid signed decimal".to_owned())?;
        if parsed.to_string() != value {
            return Err("noncanonical signed decimal".into());
        }
        Ok(Self(parsed))
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApprovedActionV1 {
    Move {
        position_cm: [DecimalI64; 3],
    },
    Transfer {
        from: MaterialLedgerId,
        to: MaterialLedgerId,
        material: String,
        amount: DecimalI64,
        distance_km: DecimalI64,
        priority: MaterialTransitPriority,
        routing: ApprovedRoutingV1,
    },
    Build {
        site: String,
        factory: String,
        factory_kind: String,
    },
    Schedule {
        factory: String,
        recipe: String,
        batches: u32,
    },
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApprovedRoutingV1 {
    Direct,
    Routes { route_ids: Vec<String> },
}
fn bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ApprovedActionV1 {
    fn valid(&self, resources: &[String]) -> bool {
        let resource = |v: &str| bounded(v) && resources.iter().any(|r| r == v);
        match self {
            Self::Move { .. } => true,
            Self::Transfer {
                from,
                to,
                material,
                amount,
                distance_km,
                routing,
                ..
            } => {
                from != to
                    && resource(&from.to_string())
                    && resource(&to.to_string())
                    && resource(material)
                    && amount.0 > 0
                    && distance_km.0 >= 0
                    && match routing {
                        ApprovedRoutingV1::Direct => true,
                        ApprovedRoutingV1::Routes { route_ids } => {
                            !route_ids.is_empty()
                                && route_ids.len() <= 16
                                && route_ids.iter().enumerate().all(|(index, route)| {
                                    resource(route) && !route_ids[..index].contains(route)
                                })
                        }
                    }
            }
            Self::Build {
                site,
                factory,
                factory_kind,
            } => {
                resource(site)
                    && resource(factory)
                    && oasis7_agent_api::world_quote::builtin_factory_spec(factory_kind, factory)
                        .is_some()
            }
            Self::Schedule {
                factory,
                recipe,
                batches,
            } => {
                resource(factory)
                    && resource(recipe)
                    && *batches > 0
                    && oasis7_agent_api::world_quote::builtin_recipe_plan(recipe, *batches)
                        .is_some()
            }
        }
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalOptionsRequestV1 {
    pub approval_request_id: String,
    pub context: ApprovalContextV1,
    pub intent: CustodyIntentV1,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPhaseV1 {
    Pending,
    Completed,
    Expired,
    Exhausted,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalOptionsResponseV1 {
    pub api_version: u8,
    pub phase: ApprovalPhaseV1,
    pub approval_id: String,
    pub ceremony_id: String,
    pub operation_id: String,
    pub options: Option<serde_json::Value>,
    pub intent_digest: String,
    pub audience: String,
    pub ceremony_expires_at: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalCompleteResponseV1 {
    pub api_version: u8,
    pub phase: ApprovalPhaseV1,
    pub approval: SignedCustodyApprovalV1,
    pub proof_bytes: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalCompleteRequestV1 {
    pub approval_id: String,
    pub ceremony_id: String,
    pub operation_id: String,
    pub intent_digest: String,
    /// Standard WebAuthn assertion, parsed exclusively by the native verifier.
    pub response: serde_json::Value,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodySessionStatusV1 {
    pub api_version: u8,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub account_id: String,
    pub session_ref: String,
    pub credential_ref: String,
    pub credential_version: DecimalU64,
    pub security_version: DecimalU64,
    pub policy_version: DecimalU64,
    pub server_time: DecimalU64,
    pub expires_at: DecimalU64,
    pub idle_expires_at: DecimalU64,
}

fn encode<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, String> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|e| e.to_string())
}
pub fn bytes_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
pub fn complete_body_digest<T: Serialize>(value: &T) -> Result<String, String> {
    encode("oasis7.custody.complete-body.v1", value).map(|bytes| bytes_digest(&bytes))
}
impl ApprovalOptionsRequestV1 {
    pub fn intent_digest(&self) -> Result<String, String> {
        encode("oasis7.custody.intent.v1", self).map(|v| bytes_digest(&v))
    }
    pub fn validate(&self, now: u64) -> Result<(), String> {
        let required = [
            &self.approval_request_id,
            &self.context.environment,
            &self.context.custody_account_id,
            &self.context.audience,
        ];
        if required.iter().any(|v| v.is_empty() || v.len() > 256)
            || self.context.policy_version.0 == 0
            || (self.context.security_version.0 == 0
                && !matches!(&self.intent, CustodyIntentV1::Provision { .. }))
            || self.context.service_epoch.0 == 0
        {
            return Err("invalid approval context".into());
        }
        let valid = match &self.intent {
            CustodyIntentV1::Provision {
                provision_id,
                purpose,
                plan_digest,
                bootstrap_config_version,
                world_identity,
                identity_route,
                world_route,
                native_identity_namespace,
                register_request_id,
                login_request_id,
                deadline,
                transport_limit,
            } => {
                world_identity.valid()
                    && bounded(purpose)
                    && hex_digest(plan_digest)
                    && [
                        provision_id,
                        identity_route,
                        world_route,
                        native_identity_namespace,
                        register_request_id,
                        login_request_id,
                    ]
                    .iter()
                    .all(|v| !v.is_empty() && v.len() <= 256)
                    && bootstrap_config_version.0 > 0
                    && deadline.0 > now
                    && deadline.0 <= now.saturating_add(1_800_000)
                    && (1..=24).contains(&transport_limit.0)
                    && register_request_id != login_request_id
            }
            CustodyIntentV1::ManagedLogin {
                native_account_id,
                login_key_ref,
                native_identity_namespace,
                blocked_operation_ref,
                deadline,
            } => {
                [
                    native_account_id,
                    login_key_ref,
                    native_identity_namespace,
                    blocked_operation_ref,
                ]
                .iter()
                .all(|v| !v.is_empty() && v.len() <= 256)
                    && deadline.0 > now
                    && deadline.0 <= now.saturating_add(300_000)
            }
            CustodyIntentV1::TestSession {
                session_id,
                native_account_id,
                world_identity,
                task_fence,
                executor_key_ref,
                client_binding,
                runtime_ref,
                skill_ref,
                source_ref,
                allowed_resource_refs,
                read_budget,
                cleanup_budget,
                control_budget,
                actions,
                write_deadline,
                read_until,
                cleanup_until,
                request_limit,
                transport_limit,
                termination_plan_digest,
                ..
            } => {
                [
                    session_id,
                    native_account_id,
                    executor_key_ref,
                    runtime_ref,
                    skill_ref,
                    source_ref,
                    termination_plan_digest,
                ]
                .iter()
                .all(|v| !v.is_empty() && v.len() <= 256)
                    && !actions.is_empty()
                    && actions.len() <= 100
                    && !allowed_resource_refs.is_empty()
                    && allowed_resource_refs.len() <= 256
                    && allowed_resource_refs.iter().all(|v| bounded(v))
                    && world_identity.valid()
                    && task_fence.validate().is_ok()
                    && task_fence.task_status == oasis7_agent_api::game::GameTaskStatusV1::Active
                    && client_binding.valid(control_budget.transport_hops)
                    && task_fence.world_id == world_identity.world_id
                    && task_fence.genesis_digest == world_identity.genesis_digest
                    && task_fence.branch_id == world_identity.branch_id
                    && task_fence.reorg_epoch == world_identity.reorg_epoch
                    && actions.iter().all(|a| a.valid(allowed_resource_refs))
                    && hex_digest(termination_plan_digest)
                    && (1..=100).contains(&read_budget.requests.0)
                    && (1..=300).contains(&read_budget.transport.0)
                    && read_budget.business.0 == 0
                    && (1..=2).contains(&cleanup_budget.requests.0)
                    && (1..=24).contains(&cleanup_budget.transport.0)
                    && cleanup_budget.business.0 == 0
                    && (1..=80).contains(&control_budget.requests.0)
                    && (1..=240).contains(&control_budget.transport_per_hop.0)
                    && (1..=160).contains(&control_budget.identity_transport.0)
                    && (1..=2).contains(&control_budget.transport_hops)
                    && write_deadline.0 > now
                    && write_deadline.0 <= now.saturating_add(1_800_000)
                    && read_until.0 >= write_deadline.0
                    && read_until.0 <= write_deadline.0.saturating_add(1_800_000)
                    && cleanup_until.0 >= write_deadline.0
                    && cleanup_until.0 <= write_deadline.0.saturating_add(300_000)
                    && (1..=100).contains(&request_limit.0)
                    && (1..=400).contains(&transport_limit.0)
            }
        };
        if valid {
            Ok(())
        } else {
            Err("invalid typed approval intent".into())
        }
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyApprovalProofV1 {
    pub version: u8,
    pub key_id: String,
    pub identity_namespace: String,
    pub approval_id: String,
    pub approval_request_id: String,
    pub context: ApprovalContextV1,
    pub intent_digest: String,
    pub manager_account_id: String,
    pub manager_session_ref: String,
    pub origin_credential_id: String,
    pub origin_credential_version: DecimalU64,
    pub passkey_credential_id: String,
    pub passkey_credential_version: DecimalU64,
    pub manager_policy_version: DecimalU64,
    pub manager_security_version: DecimalU64,
    pub identity_instance_epoch: DecimalU64,
    pub nonce: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl CustodyApprovalProofV1 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        encode("oasis7.custody.approval.v1", self)
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCustodyApprovalV1 {
    pub proof: CustodyApprovalProofV1,
    pub signature: String,
}
impl SignedCustodyApprovalV1 {
    pub fn digest(&self) -> Result<String, String> {
        encode("oasis7.custody.signed-approval.v1", self).map(|v| bytes_digest(&v))
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPurposeV1 {
    ConsumeApproval,
    InspectOrigin,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalVerifyRequestV1 {
    pub approval: SignedCustodyApprovalV1,
    pub proof_digest: String,
    pub expected_audience: String,
    pub expected_intent_digest: String,
    pub purpose: VerificationPurposeV1,
    pub verify_nonce: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofStateV1 {
    Active,
    Expired,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginStateV1 {
    Active,
    Revoked,
    Unavailable,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalVerificationV1 {
    pub version: u8,
    pub key_id: String,
    pub identity_namespace: String,
    pub purpose: VerificationPurposeV1,
    pub proof_digest: String,
    pub approval_request_id: String,
    pub audience: String,
    pub intent_digest: String,
    pub verify_nonce: String,
    pub manager_security_version: DecimalU64,
    pub manager_policy_version: DecimalU64,
    pub origin_credential_version: DecimalU64,
    pub identity_instance_epoch: DecimalU64,
    pub credential_version: DecimalU64,
    pub observed_at: DecimalU64,
    pub proof_expires_at: DecimalU64,
    pub source_session_expires_at: DecimalU64,
    pub source_session_idle_expires_at: DecimalU64,
    pub proof_state: ProofStateV1,
    pub origin_state: OriginStateV1,
}
impl ApprovalVerificationV1 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        encode("oasis7.custody.approval-verification.v1", self)
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedApprovalVerificationV1 {
    pub verification: ApprovalVerificationV1,
    pub signature: String,
}

#[cfg(test)]
impl schemars::JsonSchema for DecimalI64 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DecimalI64".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let positive = oasis7_agent_api::schema::bounded_decimal_pattern("9223372036854775807");
        let negative = oasis7_agent_api::schema::bounded_decimal_pattern("9223372036854775808");
        schemars::json_schema!({"type":"string","maxLength":20,
            "pattern":format!("^(?:{positive}|-(?!0$){negative})$")})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> ApprovalContextV1 {
        ApprovalContextV1 {
            environment: "test".into(),
            custody_account_id: "custody-one".into(),
            audience: "account-authority:test".into(),
            policy_version: DecimalU64(1),
            security_version: DecimalU64(2),
            service_epoch: DecimalU64(3),
        }
    }
    #[test]
    fn signing_domains_and_versions_have_frozen_canonical_vectors() {
        let proof = CustodyApprovalProofV1 {
            version: 1,
            key_id: "key-one".into(),
            identity_namespace: "identity:test".into(),
            approval_id: "approval-one".into(),
            approval_request_id: "request-one".into(),
            context: context(),
            intent_digest: "ab".repeat(32),
            manager_account_id: "manager-one".into(),
            manager_session_ref: "session-one".into(),
            origin_credential_id: "origin-one".into(),
            origin_credential_version: DecimalU64(u64::MAX),
            passkey_credential_id: "passkey-one".into(),
            passkey_credential_version: DecimalU64(9_007_199_254_740_993),
            manager_policy_version: DecimalU64(4),
            manager_security_version: DecimalU64(5),
            identity_instance_epoch: DecimalU64(6),
            nonce: "nonce-one".into(),
            issued_at: DecimalU64(1000),
            expires_at: DecimalU64(31000),
        };
        assert_eq!(
            bytes_digest(&proof.signing_bytes().unwrap()),
            "b81d7a58f2384131a00ef3784b8d8889911d8013d6e8083dedf3f6314bb0b147"
        );
        let verification = ApprovalVerificationV1 {
            version: 1,
            key_id: "key-one".into(),
            identity_namespace: "identity:test".into(),
            purpose: VerificationPurposeV1::InspectOrigin,
            proof_digest: "cd".repeat(32),
            approval_request_id: "request-one".into(),
            audience: "account-authority:test".into(),
            intent_digest: "ab".repeat(32),
            verify_nonce: "verify-one".into(),
            manager_security_version: DecimalU64(5),
            manager_policy_version: DecimalU64(4),
            origin_credential_version: DecimalU64(u64::MAX),
            identity_instance_epoch: DecimalU64(6),
            credential_version: DecimalU64(9_007_199_254_740_993),
            observed_at: DecimalU64(32000),
            proof_expires_at: DecimalU64(31000),
            source_session_expires_at: DecimalU64(100000),
            source_session_idle_expires_at: DecimalU64(90000),
            proof_state: ProofStateV1::Expired,
            origin_state: OriginStateV1::Active,
        };
        assert_eq!(
            bytes_digest(&verification.signing_bytes().unwrap()),
            "c8ba0543d914413109f54b92aee6012aa4e6a1dbdeaf273fdd77c607afb7e046"
        );
        let mut changed = proof.clone();
        changed.key_id = "other".into();
        assert_ne!(
            proof.signing_bytes().unwrap(),
            changed.signing_bytes().unwrap()
        );
        changed = proof.clone();
        changed.context.service_epoch = DecimalU64(4);
        assert_ne!(
            proof.signing_bytes().unwrap(),
            changed.signing_bytes().unwrap()
        );
        let mut consumed = verification.clone();
        consumed.purpose = VerificationPurposeV1::ConsumeApproval;
        assert_ne!(
            verification.signing_bytes().unwrap(),
            consumed.signing_bytes().unwrap()
        );
    }
    #[test]
    fn full_test_plan_is_closed_and_every_purpose_budget_changes_its_digest() {
        let fence = serde_json::from_value(serde_json::json!({"world_id":"world-one","genesis_digest":"genesis-one","branch_id":"branch-one",
            "reorg_epoch":"1","agent_id":"agent-one","owner_binding":"owner-one","owner_generation":"1",
            "owner_public_key":"aa".repeat(32),"task_id":"task-one","task_revision":"1","task_status":"active",
            "latest_goal_revision":"0","active_goal_revision":"0","active_goal_reference":null,"executor_epoch":"1",
            "executor_binding":{"executor_id":"executor-one","executor_public_key":"bb".repeat(32),"delegation_generation":"1","executor_epoch":"1"}})).unwrap();
        let request = ApprovalOptionsRequestV1 {
            approval_request_id: "request".into(),
            context: context(),
            intent: CustodyIntentV1::TestSession {
                session_id: "test-one".into(),
                native_account_id: "native-one".into(),
                world_identity: Box::new(ApprovedWorldV1 {
                    world_id: "world-one".into(),
                    genesis_digest: "genesis-one".into(),
                    branch_id: "branch-one".into(),
                    reorg_epoch: DecimalU64(1),
                    service_public_key: "cc".repeat(32),
                }),
                task_fence: fence,
                executor_key_ref: "executor-key".into(),
                client_binding: ApprovedClientBindingV1::NativeBrokerCustodyV1 {
                    client_key: "dd".repeat(32),
                },
                runtime_ref: "codex".into(),
                skill_ref: "skill-sha".into(),
                source_ref: "source-sha".into(),
                actions: vec![ApprovedActionV1::Move {
                    position_cm: [DecimalI64(1), DecimalI64(-2), DecimalI64(i64::MAX)],
                }],
                allowed_resource_refs: vec!["agent:agent-one".into()],
                read_budget: PurposeBudgetV1 {
                    requests: DecimalU64(100),
                    transport: DecimalU64(300),
                    business: DecimalU64(0),
                },
                cleanup_budget: PurposeBudgetV1 {
                    requests: DecimalU64(2),
                    transport: DecimalU64(24),
                    business: DecimalU64(0),
                },
                control_budget: ControlBudgetV1 {
                    requests: DecimalU64(80),
                    transport_per_hop: DecimalU64(240),
                    identity_transport: DecimalU64(160),
                    transport_hops: 1,
                },
                write_deadline: DecimalU64(301000),
                read_until: DecimalU64(601000),
                cleanup_until: DecimalU64(601000),
                request_limit: DecimalU64(100),
                transport_limit: DecimalU64(400),
                business_limit: DecimalU64(u64::MAX),
                termination_plan_digest: "ee".repeat(32),
            },
        };
        request.validate(1000).unwrap();
        let original = request.intent_digest().unwrap();
        let value = serde_json::to_value(&request).unwrap();
        for (object, field) in [
            ("read_budget", "requests"),
            ("cleanup_budget", "transport"),
            ("control_budget", "requests"),
        ] {
            let mut changed = value.clone();
            changed["intent"][object][field] = "1".into();
            let changed: ApprovalOptionsRequestV1 = serde_json::from_value(changed).unwrap();
            assert_ne!(original, changed.intent_digest().unwrap());
        }
        let mut unknown = value.clone();
        unknown["intent"]["arbitrary_raw_action"] = true.into();
        assert!(serde_json::from_value::<ApprovalOptionsRequestV1>(unknown).is_err());
        let mut missing = value;
        missing["intent"]
            .as_object_mut()
            .unwrap()
            .remove("read_budget");
        assert!(serde_json::from_value::<ApprovalOptionsRequestV1>(missing).is_err());
        for invalid in ["01", "+1", "-0", "9223372036854775808"] {
            assert!(serde_json::from_value::<DecimalI64>(invalid.into()).is_err());
        }
    }
}
