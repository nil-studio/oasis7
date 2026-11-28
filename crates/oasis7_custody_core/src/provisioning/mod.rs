//! Minimal pure provisioning contract types retained for approval and native binding.
use crate::DecimalU64;
use crate::approval::{
    ApprovedActionV1, ApprovedClientBindingV1, ApprovedRoutingV1, ApprovedWorldV1, ControlBudgetV1,
    CustodyIntentV1, DecimalI64, PurposeBudgetV1,
};
use oasis7_agent_api::game::{GameTaskFenceV1, GoalContentV1};
use oasis7_client_api::world_service::resource_basis as wire;
use serde::{Deserialize, Serialize};

pub const OWNER_SETUP_RECONCILE_ATTEMPT_TRANSPORT_COST: u64 = 104;

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRoleKey {
    pub key_id: String,
    pub key_version: DecimalU64,
    pub public_key: String,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupStageV1 {
    CreateTask,
    PrepareGoal,
    PublishGoal,
    Delegation,
    Bind,
    FinalObservation,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcilePlanV1 {
    pub api_version: u8,
    pub reconcile_id: String,
    pub setup_id: String,
    pub provision_id: String,
    pub stage: OwnerSetupStageV1,
    pub setup_plan_digest: String,
    pub original_digest: String,
    pub correlation_digest: String,
    pub world_identity: ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub original_deadline: DecimalU64,
    pub deadline: DecimalU64,
    pub attempt_limit: u8,
    pub transport_limit: DecimalU64,
}

impl OwnerSetupReconcilePlanV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != 1
            || ![
                &self.reconcile_id,
                &self.setup_id,
                &self.provision_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
            ]
            .iter()
            .all(|s| id(s))
            || ![
                &self.setup_plan_digest,
                &self.original_digest,
                &self.correlation_digest,
            ]
            .iter()
            .all(|s| hex(s))
            || !(1..=3).contains(&self.attempt_limit)
            || self.deadline.0 == 0
            || self.deadline.0 > self.original_deadline.0
            || self.transport_limit.0
                != 8 + OWNER_SETUP_RECONCILE_ATTEMPT_TRANSPORT_COST * u64::from(self.attempt_limit)
            || self.transport_limit.0 > 400
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest("oasis7.custody.owner-setup-reconcile-plan.v1", self)
    }

    pub fn intent(&self) -> Result<CustodyIntentV1, String> {
        self.validate()?;
        Ok(CustodyIntentV1::OwnerSetupReconcile {
            plan: Box::new(self.clone()),
        })
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupPlanV2 {
    pub api_version: u8,
    pub setup_id: String,
    pub provision_id: String,
    pub participant_receipt_digest: String,
    pub world_identity: ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub bootstrap_config_version: DecimalU64,
    pub goal_content: GoalContentV1,
    pub create_operation_id: String,
    pub prepare_operation_id: String,
    pub prepare_nonce: String,
    pub publish_operation_id: String,
    pub delegation_operation_id: String,
    pub bind_operation_id: String,
    pub executor_id: String,
    pub executor_public_key: String,
    pub delegation_generation: DecimalU64,
    pub delegation_nonce: DecimalU64,
    #[cfg_attr(test, schemars(with = "schema::ResourceSelectionSchemaV1"))]
    pub selection: wire::ResourceBasisSelectionV1,
    pub deadline: DecimalU64,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub write_limit: DecimalU64,
}

impl core::fmt::Debug for OwnerSetupPlanV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupPlanV2(<private>)")
    }
}

impl OwnerSetupPlanV2 {
    pub fn validate(&self) -> Result<(), String> {
        let ops = [
            &self.create_operation_id,
            &self.prepare_operation_id,
            &self.publish_operation_id,
            &self.delegation_operation_id,
            &self.bind_operation_id,
        ];
        if self.api_version != 2
            || ops.iter().any(|v| !id(v))
            || ops.iter().collect::<std::collections::BTreeSet<_>>().len() != 5
            || [
                &self.setup_id,
                &self.provision_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
                &self.executor_id,
            ]
            .iter()
            .any(|v| !id(v))
            || !hex(&self.participant_receipt_digest)
            || !hex(&self.prepare_nonce)
            || !hex(&self.executor_public_key)
            || !self.world_identity.valid()
            || self.bootstrap_config_version.0 == 0
            || self.delegation_generation.0 != 1
            || self.delegation_nonce.0 != 1
            || self.deadline.0 == 0
            || self.request_limit.0 != 6
            || !(584..=600).contains(&self.transport_limit.0)
            || self.write_limit.0 != 4
        {
            return Err(invalid());
        }
        self.goal_content.validate()?;
        self.selection.validate()?;
        Ok(())
    }

    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest("oasis7.custody.owner-setup-plan.v2", self)
    }

    pub fn intent(&self) -> Result<CustodyIntentV1, String> {
        self.validate()?;
        Ok(CustodyIntentV1::OwnerSetupV2 {
            plan: Box::new(self.clone()),
        })
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceTestSessionScopeV2 {
    pub session_id: String,
    pub native_account_id: String,
    pub world_identity: ApprovedWorldV1,
    pub task_fence: GameTaskFenceV1,
    pub executor_key_ref: String,
    pub client_binding: ApprovedClientBindingV1,
    pub client_pairing: crate::client_enrollment::ClientPairingBindingV1,
    pub runtime_ref: String,
    pub skill_ref: String,
    pub source_ref: String,
    pub termination_plan_digest: String,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPlanV2 {
    pub api_version: u8,
    pub request_id: String,
    pub enrollment_id: String,
    pub basis_digest: String,
    pub owner_setup_plan_digest: String,
    pub prepared_resource_basis_digest: String,
    #[cfg_attr(test, schemars(with = "schema::ResourceSelectionSchemaV1"))]
    pub selection: wire::ResourceBasisSelectionV1,
    pub actions: Vec<ApprovedActionV1>,
    pub allowed_resource_refs: Vec<String>,
    pub write_deadline: DecimalU64,
    pub read_until: DecimalU64,
    pub cleanup_until: DecimalU64,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub business_limit: DecimalU64,
    pub read_budget: PurposeBudgetV1,
    pub cleanup_budget: PurposeBudgetV1,
    pub control_budget: ControlBudgetV1,
}

fn action(s: &wire::ResourceBasisSelectionV1) -> ApprovedActionV1 {
    match s {
        wire::ResourceBasisSelectionV1::Transfer {
            from,
            to,
            material,
            amount,
            distance_km,
            priority,
            routing,
        } => ApprovedActionV1::Transfer {
            from: from.clone().into(),
            to: to.clone().into(),
            material: material.clone(),
            amount: DecimalI64(amount.0),
            distance_km: DecimalI64(distance_km.0),
            priority: *priority,
            routing: match routing {
                wire::ResourceRoutingV1::Direct => ApprovedRoutingV1::Direct,
                wire::ResourceRoutingV1::Routes { route_ids } => ApprovedRoutingV1::Routes {
                    route_ids: route_ids.clone(),
                },
            },
        },
        wire::ResourceBasisSelectionV1::Build {
            site,
            factory,
            factory_kind,
        } => ApprovedActionV1::Build {
            site: site.clone(),
            factory: factory.clone(),
            factory_kind: factory_kind.clone(),
        },
        wire::ResourceBasisSelectionV1::Schedule {
            factory,
            recipe,
            batches,
        } => ApprovedActionV1::Schedule {
            factory: factory.clone(),
            recipe: recipe.clone(),
            batches: *batches,
        },
    }
}

impl TestSessionPlanV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.selection.validate()?;
        let required = match &self.selection {
            wire::ResourceBasisSelectionV1::Transfer {
                from,
                to,
                material,
                routing,
                ..
            } => {
                let mut r = vec![from.reference(), to.reference(), material.clone()];
                if let wire::ResourceRoutingV1::Routes { route_ids } = routing {
                    r.extend(route_ids.clone());
                }
                r
            }
            wire::ResourceBasisSelectionV1::Build { site, factory, .. } => {
                vec![site.clone(), factory.clone()]
            }
            wire::ResourceBasisSelectionV1::Schedule {
                factory, recipe, ..
            } => vec![factory.clone(), recipe.clone()],
        };
        if required
            .iter()
            .any(|v| !self.allowed_resource_refs.contains(v))
            || self.api_version != 2
            || !id(&self.request_id)
            || !id(&self.enrollment_id)
            || [
                &self.basis_digest,
                &self.owner_setup_plan_digest,
                &self.prepared_resource_basis_digest,
            ]
            .iter()
            .any(|v| !hex(v))
            || self.actions.len() != 1
            || self.actions[0] != action(&self.selection)
            || self.allowed_resource_refs.is_empty()
            || self.allowed_resource_refs.len() > 256
            || self.allowed_resource_refs.iter().any(|v| !id(v))
            || self
                .allowed_resource_refs
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.allowed_resource_refs.len()
            || !(1..=100).contains(&self.request_limit.0)
            || !(1..=400).contains(&self.transport_limit.0)
            || !(1..=100).contains(&self.read_budget.requests.0)
            || !(1..=300).contains(&self.read_budget.transport.0)
            || self.read_budget.business.0 != 0
            || self.cleanup_budget.requests.0 != 2
            || self.cleanup_budget.transport.0 != 24
            || self.cleanup_budget.business.0 != 0
            || !(1..=80).contains(&self.control_budget.requests.0)
            || !(1..=240).contains(&self.control_budget.transport_per_hop.0)
            || !(1..=160).contains(&self.control_budget.identity_transport.0)
            || !(1..=2).contains(&self.control_budget.transport_hops)
            || self.read_until.0 < self.write_deadline.0
            || self.read_until.0 > self.write_deadline.0.saturating_add(1_800_000)
            || self.cleanup_until.0 < self.write_deadline.0
            || self.cleanup_until.0 > self.write_deadline.0.saturating_add(300_000)
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest("oasis7.custody.test-session-plan.v2", self)
    }

    pub fn validate_scope(&self, s: &ResourceTestSessionScopeV2, now: u64) -> Result<(), String> {
        self.validate()?;
        s.client_pairing.validate().map_err(|_| invalid())?;
        s.task_fence.validate()?;
        let f = &s.task_fence;
        let w = &s.world_identity;
        let client = match &s.client_binding {
            ApprovedClientBindingV1::NativeBrokerCustodyV1 { client_key } => {
                self.control_budget.transport_hops == 1
                    && client_key == &s.client_pairing.client_public_key
                    && s.client_pairing.connector_public_key.is_none()
            }
            ApprovedClientBindingV1::DotMcpCustodyV1 {
                client_key,
                connector_key,
            } => {
                self.control_budget.transport_hops == 2
                    && client_key == &s.client_pairing.client_public_key
                    && s.client_pairing.connector_public_key.as_ref() == Some(connector_key)
            }
        };
        if [
            &s.session_id,
            &s.native_account_id,
            &s.executor_key_ref,
            &s.runtime_ref,
            &s.skill_ref,
            &s.source_ref,
        ]
        .iter()
        .any(|v| !id(v))
            || !hex(&s.termination_plan_digest)
            || !w.valid()
            || f.world_id != w.world_id
            || f.genesis_digest != w.genesis_digest
            || f.branch_id != w.branch_id
            || f.reorg_epoch.0 != w.reorg_epoch.0
            || f.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active
            || !client
            || self.write_deadline.0 <= now
            || self.write_deadline.0 > now.saturating_add(1_800_000)
            || s.client_pairing.enrollment_id != self.enrollment_id
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn intent(
        &self,
        s: ResourceTestSessionScopeV2,
        now: u64,
    ) -> Result<CustodyIntentV1, String> {
        self.validate_scope(&s, now)?;
        Ok(CustodyIntentV1::TestSessionV2 {
            plan: Box::new(self.clone()),
            scope: Box::new(s),
        })
    }
}

fn invalid() -> String {
    "invalid resource v2 contract".into()
}

fn id(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn digest<T: Serialize>(domain: &str, v: &T) -> Result<String, String> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, v))
        .map_err(|e| e.to_string())
        .map(|v| crate::approval::bytes_digest(&v))
}

#[cfg(test)]
pub(crate) mod schema {
    //! Test-only schema projection of SDK wire. No authority or runtime conversion.
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(JsonSchema, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ResourceLedgerSchemaV1 {
        World,
        Agent { agent_id: String },
        Site { site_id: String },
        Factory { factory_id: String },
    }

    #[derive(JsonSchema, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ResourceRoutingSchemaV1 {
        Direct,
        Routes { route_ids: Vec<String> },
    }

    #[derive(JsonSchema, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ResourceSelectionSchemaV1 {
        Transfer {
            from: ResourceLedgerSchemaV1,
            to: ResourceLedgerSchemaV1,
            material: String,
            #[schemars(regex(pattern = "^(0|[1-9][0-9]*|-[1-9][0-9]*)$"))]
            amount: String,
            #[schemars(regex(pattern = "^(0|[1-9][0-9]*|-[1-9][0-9]*)$"))]
            distance_km: String,
            priority: oasis7_agent_api::world_action::MaterialTransitPriority,
            routing: ResourceRoutingSchemaV1,
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

    #[test]
    fn resource_v2_schema_projection_preserves_actual_sdk_selection_wire() {
        use oasis7_client_api::world_service::resource_basis as w;
        for selection in [
            w::ResourceBasisSelectionV1::Transfer {
                from: w::ResourceLedgerV1::Agent {
                    agent_id: "a".into(),
                },
                to: w::ResourceLedgerV1::Site {
                    site_id: "s".into(),
                },
                material: "ore".into(),
                amount: w::ResourceDecimalI64V1(9_007_199_254_740_993),
                distance_km: w::ResourceDecimalI64V1(0),
                priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
                routing: w::ResourceRoutingV1::Routes {
                    route_ids: vec!["route".into()],
                },
            },
            w::ResourceBasisSelectionV1::Build {
                site: "s".into(),
                factory: "f".into(),
                factory_kind: "factory.smelter.mk1".into(),
            },
            w::ResourceBasisSelectionV1::Schedule {
                factory: "f".into(),
                recipe: "recipe.smelter.iron_ingot".into(),
                batches: 1,
            },
        ] {
            let bytes = serde_json::to_vec(&selection).unwrap();
            let projection: ResourceSelectionSchemaV1 = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(serde_json::to_vec(&projection).unwrap(), bytes);
        }
    }
}
