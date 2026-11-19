//! Hook beside module_execution_tests to reuse the genuine accepted GEN fixture.
use super::*;
use crate::world::incremental_factory_directory_orders::*;
use oasis7_world_state::incremental_world_rule_composite_family::bind_current_agent_orders_v1;
use oasis7_world_state::incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1;
// Observes original immutable value payloads, without replacing state/value roots.
struct IndexReadFence<'a> {
    inner: Reader<'a>,
    marker: String,
    reads: std::cell::Cell<usize>,
    deny: std::cell::Cell<bool>,
}
impl ValueNodeReader for IndexReadFence<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let bytes = ValueNodeReader::read_node(&self.inner, id, max, memory, class)?;
        if bytes
            .windows(self.marker.len())
            .any(|window| window == self.marker.as_bytes())
        {
            self.reads.set(self.reads.get() + 1);
            if self.deny.get() {
                return Err(ValueTreeError::Invalid(
                    "unchanged index payload must remain unread",
                ));
            }
        }
        Ok(bytes)
    }
}
impl MerkleNodeReader for IndexReadFence<'_> {
    type Node = <Reader<'static> as MerkleNodeReader>::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.inner.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.inner.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        MerkleNodeReader::read_node(&self.inner, id, max)
    }
}
fn order_work() -> FactoryOrderDirectoryWorkV1 {
    FactoryOrderDirectoryWorkV1 {
        ids:
            oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1 {
                max_ids: 32,
                max_id_bytes: 256,
                comparison_bytes_remaining: 16 << 20,
            },
        pending_entries_remaining: 10000,
        merge_comparison_bytes_remaining: 16 << 20,
        max_index_text_bytes: 256,
    }
}
fn order_read_work()
-> oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1 {
    oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1 {
        json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 100000,
            max_depth: 64,
            max_payload_bytes: 1 << 20,
        },
        max_retained_trees: 4096,
        max_tree_heap: 64 << 10,
    }
}
#[test]
fn actual_accepted_empty_orders_directory_uses_gen_scope_and_ordinary_source_binding() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = World::new();
    assert!(world.state.world_rules.is_none());
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut scan = StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100000,
    };
    let mut typed = order_read_work();
    let proof =
        qualify_world_rule_agent_orders_v1(&fixture.roots, &mut read, &mut scan, &mut typed)
            .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [61; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let guard = ctx.transaction.guard();
    let source = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    let mut work = order_work();
    let guard = ctx.transaction.guard();
    let accepted = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &source,
        &reader,
        &mut ctx,
        &mut scan,
        &mut typed,
        &mut work,
        &guard,
        &live,
    )
    .unwrap();
    scan.nodes_remaining = 0;
    scan.read_bytes_remaining = 0;
    scan.leaves_remaining = 0;
    let guard = ctx.transaction.guard();
    let current = open_current_factory_order_directory_v1(
        &accepted, &source, &reader, &mut ctx, &mut scan, &mut typed, &mut work, &guard, &live,
    )
    .unwrap();
    assert_eq!(current.iter_order_ids().count(), 0);
    assert_eq!(current.iter_producing_order_ids().count(), 0);
    assert!(std::ptr::eq(current.source(), &source));
    ctx.transaction
        .write(
            StateNamespaceV1::Scheduler,
            b"unrelated-order-directory-contribution",
            Some([8; 32]),
        )
        .unwrap();
    let guard = ctx.transaction.guard();
    current.validate_current(&mut ctx, &guard, &live).unwrap();
    let mut foreign = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [62; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let guard = foreign.guard();
    let result = current.validate_current(
        &mut WorldFieldOverlayContextV1 {
            transaction: &mut foreign,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut genesis::io(),
            },
            key_work: &mut genesis::keys(),
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        },
        &guard,
        &live,
    );
    assert!(matches!(
        result,
        Err(FactoryOrderDirectoryErrorV1::Execution(
            ExecutionError::StaleGuard
        ))
    ));
    let guard = foreign.guard();
    assert!(foreign.seal(&guard, &live, 1).is_err());
}

pub(super) fn directory_order(
    id: &str,
    phase: oasis7_world_state::world_rules::AgentIndustrialPhaseV1,
) -> oasis7_world_state::world_rules::AgentIndustrialOrderV1 {
    use oasis7_world_state::world_rules::{
        AgentIndustrialOrderV1, AgentIndustrialPhaseV1 as Phase, AgentManufacturingQuoteV1,
    };
    let mut quote = AgentManufacturingQuoteV1 {
        policy_version: 1,
        world_binding: None,
        recipe_id: "recipe.assembler.agent_mk1".into(),
        owner_id: "owner".into(),
        owner_player_id: "player".into(),
        owner_source_id: "starter:owner".into(),
        factory_id: "assembler".into(),
        quoted_at: 0,
        expires_at: 100,
        quote_digest: String::new(),
        dependency_digest: "large-unread-quote".repeat(16384),
        delivery_location_id: "site".into(),
        inputs: std::collections::BTreeMap::new(),
        input_ledger: oasis7_agent_api::world_action::MaterialLedgerId::world(),
        electricity: 48,
        claim_cost: oasis7_world_state::AgentClaimCostQuote {
            reputation_tier: 0,
            claim_cap: 2,
            slot_index: 2,
            activation_fee_amount: 0,
            activation_fee_burn_amount: 0,
            activation_fee_treasury_amount: 0,
            claim_bond_amount: 0,
            upkeep_per_epoch: 0,
            release_cooldown_epochs: 0,
            grace_epochs: 0,
            idle_warning_epochs: 0,
            forced_idle_reclaim_epochs: 0,
            forced_reclaim_penalty_bps: 0,
        },
        upfront_liquid: 0,
        liquid_after_reservation: 0,
        electricity_after_reservation: 100,
        maintenance_per_tick: 1,
    };
    quote.quote_digest = crate::util::hash_json(&quote).unwrap();
    let delivered = phase == Phase::Delivered;
    let started = matches!(phase, Phase::Producing | Phase::Delivered);
    let value = AgentIndustrialOrderV1 {
        order_id: id.into(),
        owner_id: quote.owner_id.clone(),
        factory_id: quote.factory_id.clone(),
        target_agent_id: format!("industrial:{}", crate::util::hash_json(&id).unwrap()),
        policy_version: 1,
        inputs: std::collections::BTreeMap::new(),
        confirmed_at: 0,
        ready_at: 12,
        delivered_at: delivered.then_some(12),
        phase,
        quote,
        started_at: started.then_some(1),
        produced_at: delivered.then_some(12),
        blocked_at: None,
        completed_work_ticks: if delivered { 12 } else { 0 },
        last_progress_tick: None,
        terminal_reason: None,
        refund_pending: false,
        refunded_at: None,
    };
    value.validate().unwrap();
    value
}
#[test]
fn actual_order_directory_merges_changed_ids_keeps_delivered_and_filters_real_deletion_without_quotes()
 {
    use oasis7_world_state::world_rules::AgentIndustrialPhaseV1 as Phase;
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    let mut rules = oasis7_world_state::WorldRulesStateV1::default();
    rules
        .agent_orders
        .insert("a".into(), directory_order("a", Phase::Reserved));
    rules.agent_orders.insert(
        "removed".into(),
        directory_order("removed", Phase::Producing),
    );
    rules.agent_orders.insert(
        "z-delivered".into(),
        directory_order("z-delivered", Phase::Delivered),
    );
    world.state.world_rules = Some(rules);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = IndexReadFence {
        inner: Reader {
            cas: fixture.cas.clone(),
            memory: &memory,
        },
        marker: format!(
            "industrial:{}",
            crate::util::hash_json(&"z-delivered").unwrap()
        ),
        reads: std::cell::Cell::new(0),
        deny: std::cell::Cell::new(false),
    };
    let cancelled = std::cell::Cell::new(false);
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.inner.cas.error(),
        cancelled: || cancelled.get(),
    };
    let mut scan = StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100000,
    };
    let mut typed = order_read_work();
    let proof =
        qualify_world_rule_agent_orders_v1(&fixture.roots, &mut read, &mut scan, &mut typed)
            .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [63; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.inner.cas.error(),
        cancelled: || cancelled.get(),
    };
    let guard = ctx.transaction.guard();
    let baseline = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    let mut work = order_work();
    let guard = ctx.transaction.guard();
    let accepted = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &baseline,
        &reader,
        &mut ctx,
        &mut scan,
        &mut typed,
        &mut work,
        &guard,
        &live,
    )
    .unwrap();
    let cached_reads = reader.reads.get();
    assert!(
        cached_reads > 0,
        "GEN cache must read the real unchanged target index"
    );
    reader.deny.set(true);
    // Replace exactly the actual AgentOrders branch with original generated rows.
    // This is a storage test: manufacturing business eligibility remains separate.
    let mut changed = oasis7_world_state::WorldState::default();
    let mut rules = oasis7_world_state::WorldRulesStateV1::default();
    rules
        .agent_orders
        .insert("a".into(), directory_order("a", Phase::Producing));
    rules
        .agent_orders
        .insert("new".into(), directory_order("new", Phase::Reserved));
    rules.agent_orders.insert(
        "z-delivered".into(),
        directory_order("z-delivered", Phase::Delivered),
    );
    changed.world_rules = Some(rules);
    let mut cas = fixture.cas.clone();
    let mut wi = genesis::io();
    let mut wk = genesis::keys();
    let mut write_work =
        oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_json_depth: 64,
                max_pending_node_bytes: 16 << 20,
            },
        };
    let descriptor = oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS
        .iter()
        .find(|field| field.name == "world_rules")
        .unwrap();
    let digest = oasis7_world_state::incremental_semantic_key::hash_semantic_key_v1(
        &("agent_orders", ()),
        &memory,
        MemoryClass::Ordinary,
        &mut genesis::keys(),
        &mut || false,
    )
    .unwrap()
    .digest;
    let wanted = oasis7_world_state::incremental_world_field_key::WorldFieldKeyV1::new(
        *descriptor,
        oasis7_world_state::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
        Some(digest),
    )
    .unwrap();
    let mut changed_edges = 0;
    oasis7_world_state::incremental_world_rule_receipts::prepare_world_rule_receipts_genesis_v1(
        &changed,
        &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
            sink: &mut cas,
            memory: &memory,
            io: &mut wi,
            key_work: &mut wk,
            limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 16 << 20,
            },
            cancelled: || cancelled.get(),
        },
        &mut write_work,
        &mut |row| {
            if row.mutation.key.as_slice() == wanted.as_bytes() {
                ctx.transaction
                    .write(row.namespace, &row.mutation.key, row.mutation.after)
                    .unwrap();
                changed_edges += 1;
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(changed_edges, 1);
    let guard = ctx.transaction.guard();
    typed.max_tree_heap = 4 << 20;
    let source = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    typed.max_tree_heap = 16 << 10;
    let guard = ctx.transaction.guard();
    let current = open_current_factory_order_directory_v1(
        &accepted, &source, &reader, &mut ctx, &mut scan, &mut typed, &mut work, &guard, &live,
    )
    .unwrap();
    assert_eq!(
        current.iter_order_ids().collect::<Vec<_>>(),
        vec!["a", "new", "z-delivered"]
    );
    assert_eq!(
        current.iter_producing_order_ids().collect::<Vec<_>>(),
        vec!["a"]
    );
    let guard = ctx.transaction.guard();
    current.validate_current(&mut ctx, &guard, &live).unwrap();
    assert_eq!(
        reader.reads.get(),
        cached_reads,
        "ordinary must not read unchanged index payloads"
    );
    let delivered = current
        .iter_order_indices()
        .find(|(id, _)| *id == "z-delivered")
        .unwrap()
        .1;
    assert_eq!(delivered.phase(), Phase::Delivered);
    assert_eq!(delivered.target_agent_id(), reader.marker);
    assert_eq!(delivered.delivered_at(), Some(12));
    cancelled.set(true);
    let guard = ctx.transaction.guard();
    assert!(
        open_current_factory_order_directory_v1(
            &accepted, &source, &reader, &mut ctx, &mut scan, &mut typed, &mut work, &guard, &live
        )
        .is_err()
    );
    let guard = ctx.transaction.guard();
    // NLL ends the overlay borrow before the owned transaction is consumed.
    assert!(tx.seal(&guard, &live, 1).is_err());
}
