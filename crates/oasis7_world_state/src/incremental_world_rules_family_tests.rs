use super::*;
use crate::{
    AgentClaimCostQuote, MaterialLedgerId, WorldState,
    incremental_field_member::*,
    incremental_typed_record_test_support::{Pages, Writer, io},
    incremental_world_state_genesis::*,
    world_rules::*,
};
use oasis7_agent_api::world_action::WorldRuleCommandV1;
use oasis7_formal_protocol::{
    incremental_state::{StateDeltaV1, StateNodeWriteV1, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use std::collections::BTreeMap;
#[path = "incremental_world_rule_continuity_test_data.rs"]
mod continuity_data;
#[path = "incremental_world_rule_industrial_test_data.rs"]
mod industrial_data;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
fn work() -> WorldRulesFamilyWorkV1 {
    WorldRulesFamilyWorkV1 {
        outer_records_remaining: 1_000_000,
        legacy_cells_remaining: 1_000_000,
        max_semantic_bytes: 1 << 20,
        max_record_bytes: 8 << 20,
        composite: WorldRulesTypedReadWorkV1 {
            max_retained_trees: 100000,
            max_tree_heap: 1 << 20,
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_depth: 64,
                max_payload_bytes: 4 << 20,
            },
        },
        remainder: WorldRuleRemainderWorkV1 {
            cells_remaining: 1_000_000,
            max_semantic_bytes: 1 << 20,
            max_payload_bytes: 4 << 20,
        },
    }
}
fn genesis_work() -> WorldStateGenesisWorkV1 {
    WorldStateGenesisWorkV1 {
        records_remaining: 1000000,
        factory: FactoryWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        factory_authority: FactoryAuthorityWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        material: MaterialWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        logistics: LogisticsWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        industry: IndustryWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        agent_authority: AgentAuthorityWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        governance: GovernanceWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        fixed_economy: FixedEconomyWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        main_token_policy: MainTokenPolicyWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        rewards: RewardRecordsWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        rule_composites: WorldRuleCompositeWriteWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            json: crate::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1000000,
                json_nodes_remaining: 100000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        },
        native: TypedRecordWriteWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            json: crate::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1000000,
                json_nodes_remaining: 100000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        },
        rules_max_pending_node_bytes: 8 << 20,
        diplomacy_max_pending_node_bytes: 8 << 20,
    }
}
fn claim_quote() -> AgentClaimCostQuote {
    AgentClaimCostQuote {
        reputation_tier: u8::MAX,
        claim_cap: u8::MAX,
        slot_index: u8::MAX,
        activation_fee_amount: u64::MAX,
        activation_fee_burn_amount: u64::MAX,
        activation_fee_treasury_amount: u64::MAX,
        claim_bond_amount: u64::MAX,
        upkeep_per_epoch: u64::MAX,
        release_cooldown_epochs: u64::MAX,
        grace_epochs: u64::MAX,
        idle_warning_epochs: u64::MAX,
        forced_idle_reclaim_epochs: u64::MAX,
        forced_reclaim_penalty_bps: u16::MAX,
    }
}
fn manufacturing_quote() -> AgentManufacturingQuoteV1 {
    AgentManufacturingQuoteV1 {
        policy_version: u32::MAX,
        world_binding: Some("original:world_binding".into()),
        recipe_id: "original:recipe_id".into(),
        owner_id: "original:owner_id".into(),
        owner_player_id: "original:owner_player_id".into(),
        owner_source_id: "original:owner_source_id".into(),
        factory_id: "original:factory_id".into(),
        quoted_at: u64::MAX,
        expires_at: u64::MAX,
        quote_digest: "original:quote_digest".into(),
        dependency_digest: "original:dependency_digest".into(),
        delivery_location_id: "original:delivery_location_id".into(),
        inputs: BTreeMap::from([("_length".into(), i64::MIN)]),
        input_ledger: MaterialLedgerId::World,
        electricity: i64::MIN,
        claim_cost: claim_quote(),
        upfront_liquid: u64::MAX,
        liquid_after_reservation: u64::MAX,
        electricity_after_reservation: i64::MIN,
        maintenance_per_tick: i64::MIN,
    }
}
fn industrial_order() -> AgentIndustrialOrderV1 {
    AgentIndustrialOrderV1 {
        order_id: "original:order_id".into(),
        owner_id: "original:owner_id".into(),
        factory_id: "original:factory_id".into(),
        target_agent_id: "original:target_agent_id".into(),
        policy_version: u32::MAX,
        inputs: BTreeMap::from([("_length".into(), i64::MIN)]),
        confirmed_at: u64::MAX,
        ready_at: u64::MAX,
        delivered_at: Some(u64::MAX),
        phase: industrial_phase(),
        quote: manufacturing_quote(),
        started_at: Some(u64::MAX),
        produced_at: Some(u64::MAX),
        blocked_at: Some(u64::MAX),
        completed_work_ticks: u32::MAX,
        last_progress_tick: Some(u64::MAX),
        terminal_reason: Some("original:terminal_reason".into()),
        refund_pending: true,
        refunded_at: Some(u64::MAX),
    }
}
fn industrial_phase() -> AgentIndustrialPhaseV1 {
    AgentIndustrialPhaseV1::Reserved
}
fn nonempty() -> WorldState {
    let mut source = WorldState::default();
    let mut r = WorldRulesStateV1::default();
    r.requests.insert(
        "outer-request".into(),
        WorldRuleReceiptV1 {
            request_id: "inner-request".into(),
            actor_id: "original-actor".into(),
            command: WorldRuleCommandV1::OpenOrganizationProtection {
                organization_id: " original-org ".into(),
            },
            committed_at: u64::MAX,
            result_id: "original-result".into(),
        },
    );
    r.agent_orders
        .insert("outer-order".into(), industrial_order());
    r.organization_cases.insert(
        "outer-case".into(),
        OrganizationCaseV1 {
            id: "inner-case".into(),
            organization_id: "original-org".into(),
            founder_id: "founder".into(),
            members: ["member".into()].into(),
            policy_version: u32::MAX,
            revision: u64::MAX,
            phase: OrganizationProtectionPhaseV1::Active,
            disposition: OrganizationDispositionPhaseV1::None,
            notified_at: None,
            protected_at: Some(0),
            grants: Default::default(),
            requests: Default::default(),
            stage_receipts: vec!["original-stage".into()],
        },
    );
    r.organization_estate.insert(
        "outer-estate".into(),
        OrganizationEstateDepositV1 {
            id: "inner-estate".into(),
            organization_id: "original-org".into(),
            contributor_id: "contributor".into(),
            material: "ore".into(),
            amount: i64::MIN,
            held_in_trust: true,
            disposed: false,
            effect_receipt: Some(String::new()),
        },
    );
    r.supply_authorizations.insert(
        "outer-auth".into(),
        SupplyAuthorizationV1 {
            id: "inner-auth".into(),
            provider_id: "provider".into(),
            crisis_id: "original-crisis".into(),
            crisis_evidence: crate::CrisisState {
                crisis_id: "nested-distinct-crisis".into(),
                kind: "original-kind".into(),
                severity: u32::MAX,
                status: crate::CrisisStatus::Active,
                opened_at: 0,
                expires_at: u64::MAX,
                resolver_agent_id: None,
                strategy: Some(String::new()),
                success: Some(false),
                impact: i64::MIN,
                resolved_at: None,
            },
            material: "ore".into(),
            beneficiaries: ["beneficiary".into()].into(),
            policy_version: u32::MAX,
            starts_at: 0,
            expires_at: u64::MAX,
            review_proposal: "original-proposal".into(),
            remaining_escrow: i64::MAX,
            requests: [("_length".into(), i64::MIN)].into(),
            revision: u64::MAX,
            active: true,
        },
    );
    r.supply_batches.insert(
        "outer-batch".into(),
        SupplyBatchV1 {
            id: "inner-batch".into(),
            authorization_id: "inner-auth".into(),
            frozen_at: u64::MAX,
            authorization_revision: u64::MAX,
            frozen_escrow: i64::MIN,
            frozen_lineages: Default::default(),
            demands: [("beneficiary".into(), -1)].into(),
            allocation: [("beneficiary".into(), 1)].into(),
            allocated: true,
        },
    );
    r.governance_effects
        .insert("original-effect-key".into(), "original-result".into());
    r.supply_lineages.insert(
        "original-lineage-key".into(),
        SupplyLineageV1 {
            target: i64::MIN,
            fulfilled: i64::MAX,
            reserved: 0,
        },
    );
    r.supply_shipments.insert(
        u64::MAX,
        SupplyShipmentV1 {
            authorization_id: "inner-auth".into(),
            batch_id: "inner-batch".into(),
            beneficiary_id: "beneficiary".into(),
            lineage_id: "different-inner-lineage-id".into(),
            input_amount: i64::MIN,
            delivered_amount: i64::MAX,
            settled: true,
        },
    );
    r.organization_continuity = continuity_data::fixture_OrganizationContinuityStateV1();
    r.emergency_supply = continuity_data::fixture_EmergencySupplyStateV1();
    r.industrial_work = industrial_data::industrial();
    r.substitution = industrial_data::substitution();
    source.world_rules = Some(r);
    source
}
fn persist(memory: &MemoryBudget, pages: &Pages, nodes: &[StateNodeWriteV1]) {
    for node in nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
fn install(memory: &MemoryBudget, pages: &Pages, source: &WorldState) -> RootSetV1 {
    let mut rows = Vec::new();
    let summary = prepare_world_state_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 128 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
        &mut genesis_work(),
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(summary.native_fields, 98);
    let mut roots = RootSetV1::empty("world-rules13-real-gen97".into(), [4; 32]).unwrap();
    for root in &mut roots.state {
        let mut writes: Vec<StateWriteV1> = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        if writes.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(root.clone(), writes, pages, 16 << 20).unwrap();
        persist(memory, pages, &delta.new_nodes);
        *root = delta.after;
    }
    roots
}
fn context<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    roots: &'a RootSetV1,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 8 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn apply(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
    mut writes: Vec<StateWriteV1>,
) -> RootSetV1 {
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let ns = StateNamespaceV1::Authorization;
    let delta = StateDeltaV1::build(roots.state_root(ns).clone(), writes, pages, 16 << 20).unwrap();
    persist(memory, pages, &delta.new_nodes);
    let mut after = roots.clone();
    *after.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    after
}
fn fixed<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    memory: &MemoryBudget,
    semantic: &K,
) -> WorldFieldKeyV1 {
    let field = crate::incremental_world_rule_parent::descriptor().unwrap();
    let digest = crate::incremental_semantic_key::hash_semantic_key_v1(
        semantic,
        memory,
        MemoryClass::Ordinary,
        &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        || false,
    )
    .unwrap();
    WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(digest.digest)).unwrap()
}
fn member<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    memory: &MemoryBudget,
    pages: &Pages,
    semantic: &K,
    payload: MemberPayloadRefV1<'_>,
) -> MemberRecordV1 {
    write_member_record_v1(
        crate::incremental_world_rule_parent::descriptor().unwrap(),
        WorldFieldRecordKindV1::Entry,
        semantic,
        payload,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap()
}
#[test]
fn world_rules13_full_gen_none_empty_and_all_nonempty_original_children() {
    let memory = MemoryBudget::new(1 << 30, 64 << 20).unwrap();
    let _fixture = memory.reserve(64 << 20, MemoryClass::Ordinary).unwrap();
    let mut empty = WorldState::default();
    empty.world_rules = Some(WorldRulesStateV1::default());
    for source in [WorldState::default(), empty, nonempty()] {
        let pages = Pages::with_limit(&memory, 262144);
        let roots = install(&memory, &pages, &source);
        let proof = qualify_world_rules_v1(
            &roots,
            &mut context(
                &memory,
                &pages,
                &roots,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 256 << 20,
                },
            ),
            &mut scan(),
            &mut scan(),
            &mut work(),
        )
        .unwrap();
        assert_eq!(proof.child_count(), 13);
        assert_eq!(proof.rules_present(), source.world_rules.is_some());
        proof.validate_binding(&roots).unwrap();
        assert_eq!(proof.allocation_class(), MemoryClass::Ordinary);
    }
}
#[test]
fn world_rules13_cold_rejects_unknown_wrongshape_missing_branch_parent_and_orphan() {
    let memory = MemoryBudget::new(1 << 30, 64 << 20).unwrap();
    let _fixture = memory.reserve(64 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 262144);
    let mut source = WorldState::default();
    source.world_rules = Some(WorldRulesStateV1::default());
    let roots = install(&memory, &pages, &source);
    let none_roots = install(&memory, &pages, &WorldState::default());
    for case in 0..5 {
        let mut writes = Vec::new();
        match case {
            0 | 1 => {
                let sem = if case == 0 {
                    ("unknown-child", "original")
                } else {
                    ("requests", "wrong-Text-instead-Unit")
                };
                let row = member(&memory, &pages, &sem, MemberPayloadRefV1::Unit);
                writes.push(StateWriteV1 {
                    key: row.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(row.value_root),
                });
            }
            2 => {
                let key = fixed(&memory, &("industrial_work", ()));
                let old = roots
                    .state_root(StateNamespaceV1::Authorization)
                    .get(key.as_bytes(), &pages)
                    .unwrap()
                    .unwrap();
                writes.push(StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: Some(old),
                    after: None,
                });
            }
            3 => {
                let key = WorldFieldKeyV1::new(
                    crate::incremental_world_rule_parent::descriptor().unwrap(),
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .unwrap();
                let old = roots
                    .state_root(StateNamespaceV1::Authorization)
                    .get(key.as_bytes(), &pages)
                    .unwrap()
                    .unwrap();
                writes.push(StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: Some(old),
                    after: None,
                });
            }
            _ => {
                // Start with an actual None-parent GEN, then add a complete
                // lineage. No unrelated common6 branches mask this orphan.
                for (column, payload) in [
                    (None, MemberPayloadRefV1::Unit),
                    (Some("target"), MemberPayloadRefV1::I64(7)),
                    (Some("fulfilled"), MemberPayloadRefV1::I64(0)),
                    (Some("reserved"), MemberPayloadRefV1::I64(0)),
                ] {
                    let row = match column {
                        None => member(&memory, &pages, &("supply_lineages", "orphan"), payload),
                        Some(column) => member(
                            &memory,
                            &pages,
                            &(("supply_lineages", "orphan"), column),
                            payload,
                        ),
                    };
                    writes.push(StateWriteV1 {
                        key: row.field_key.as_bytes().to_vec(),
                        before: None,
                        after: Some(row.value_root),
                    });
                }
            }
        }
        let before = if case == 4 { &none_roots } else { &roots };
        let after = apply(&memory, &pages, before, writes);
        assert!(
            qualify_world_rules_v1(
                &after,
                &mut context(
                    &memory,
                    &pages,
                    &after,
                    &mut io(),
                    &mut SemanticKeyBudget {
                        bytes_remaining: 256 << 20
                    }
                ),
                &mut scan(),
                &mut scan(),
                &mut work()
            )
            .is_err(),
            "case={case}"
        );
    }
}

use std::sync::Arc;
struct Recovery<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("phase recovery source"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, MemoryClass::Recovery)
            .map(|n| Some(Node(n)))
            .map_err(|e| e.to_string())
    }
}

#[test]
fn world_rules13_recovery_retains_all_children_and_re_admits_only_same_roots_pool() {
    let memory = MemoryBudget::new(1 << 30, 64 << 20).unwrap();
    let _fixture = memory.reserve(64 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 262144);
    let mut source = WorldState::default();
    source.world_rules = Some(WorldRulesStateV1::default());
    let roots = install(&memory, &pages, &source);
    let recovery = Recovery {
        pages: &pages,
        memory: &memory,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 256 << 20,
    };
    let proof = qualify_world_rules_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 8 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.child_count(), 13);
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    assert!(proof.rules_present());
    let promoted = readmit_world_rules_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    assert_eq!(promoted.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(promoted.child_count(), 13);
    promoted.validate_binding(&roots).unwrap();
    assert!(readmit_world_rules_ordinary_v1(&promoted, &roots, &memory, || false).is_err());
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(readmit_world_rules_ordinary_v1(&proof, &wrong, &memory, || false).is_err());
    let foreign = MemoryBudget::new(1 << 30, 64 << 20).unwrap();
    assert!(readmit_world_rules_ordinary_v1(&proof, &roots, &foreign, || false).is_err());
    assert!(matches!(
        readmit_world_rules_ordinary_v1(&proof, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    let mut polls = 0;
    assert!(matches!(
        readmit_world_rules_ordinary_v1(&proof, &roots, &memory, || {
            polls += 1;
            polls > 4
        }),
        Err(ValueTreeError::Cancelled)
    ));
    let held = memory
        .reserve((960 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_world_rules_ordinary_v1(&proof, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(held);
}
