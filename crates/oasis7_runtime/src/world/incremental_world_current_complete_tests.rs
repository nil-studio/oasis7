//! Full current-state acceptance over the actual full Runtime GEN fixture.
use super::super::raw_complete::{Binder, Scratch, raw_work};
use super::*;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::{
        CognitionCurrentCompleteWorkV3, CognitionCurrentPresenceV3,
    },
    incremental_factory_records as factory, incremental_industry_jobs as industry,
    incremental_inventory as inventory, incremental_logistics_records as logistics,
    incremental_module_marketplace as market, incremental_owner_closure as primitive,
    incremental_world_collection_controls as controls,
    incremental_world_current_complete::*,
    incremental_world_native as native,
    incremental_world_native_complete::WorldNativeCompleteWorkV1,
    incremental_world_rules_family as rules,
};
use std::cell::Cell;
fn rules_work() -> rules::WorldRulesFamilyWorkV1 {
    rules::WorldRulesFamilyWorkV1 {
        outer_records_remaining: 1_000_000,
        legacy_cells_remaining: 1_000_000,
        max_semantic_bytes: 1 << 20,
        max_record_bytes: 8 << 20,
        composite: rules::WorldRulesTypedReadWorkV1 {
            max_retained_trees: 100000,
            max_tree_heap: 1 << 20,
            json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_depth: 64,
                max_payload_bytes: 4 << 20,
            },
        },
        remainder: oasis7_world_state::incremental_world_rule_effects::WorldRuleRemainderWorkV1 {
            cells_remaining: 1_000_000,
            max_semantic_bytes: 1 << 20,
            max_payload_bytes: 4 << 20,
        },
    }
}
fn native_work() -> WorldNativeCompleteWorkV1 {
    WorldNativeCompleteWorkV1 {
        legacy_cells_remaining: 1_000_000,
        max_payload_bytes: 8 << 20,
        primitive: primitive::WorldPrimitiveFamilyWorkV1 {
            records_remaining: 1_000_000,
            max_record_bytes: 8 << 20,
        },
        native: native::WorldNativeQualificationWorkV1 {
            records_remaining: 1_000_000,
            trees_remaining: 1_000_000,
            max_key_heap: 1 << 20,
            max_pending_records: 8,
            typed: oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
                json: json_work(),
                max_retained_trees: 100_000,
                max_tree_heap: 1 << 20,
            },
        },
        inventory: inventory::InventoryFamilyWorkV1 {
            records_remaining: 1_000_000,
            index_steps_remaining: 1_000_000,
            max_semantic_bytes: 1 << 20,
        },
        factory: factory::FactoryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        industry: industry::IndustryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        logistics: logistics::LogisticsWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        market: market::ModuleMarketplaceFamilyWorkV1 {
            outer_rows_remaining: 1_000_000,
            changed_cells_remaining: 1_000_000,
            membership_cells_remaining: 1_000_000,
            max_record_bytes: 8 << 20,
        },
        rules: rules_work(),
        controls: controls::WorldCollectionControlsWorkV1 {
            records_remaining: 1_000_000,
            index_steps_remaining: 1_000_000,
            max_record_bytes: 8 << 20,
        },
    }
}
fn json_work() -> oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
    oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}
fn whole_work() -> WorldCurrentStateWorkV1 {
    WorldCurrentStateWorkV1 {
        key_scan: scan(),
        native_outer_scan: scan(),
        native_inner_scan: scan(),
        key_changes_remaining: 1_000_000,
        native: native_work(),
        runtime: read_work(),
        raw: raw_work(),
        cognition: CognitionCurrentCompleteWorkV3 {
            scan: scan(),
            json: oasis7_world_state::incremental_json_records::JsonChangeWorkV1 {
                json: json(),
                paths_remaining: 1_000_000,
                max_pending_pairs: 1024,
            },
            outer_cells_remaining: 1_000_000,
        },
    }
}
struct Counted<'a> {
    reader: &'a Reader<'a>,
    reads: Cell<usize>,
}
impl MerkleNodeReader for Counted<'_> {
    type Node = <Reader<'static> as MerkleNodeReader>::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.reader.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.reader.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        self.reads.set(self.reads.get() + 1);
        MerkleNodeReader::read_node(self.reader, id, max)
    }
}
pub(super) fn exercise(
    memory: &MemoryBudget,
    pages: &mut Pages,
    roots: &RootSetV1,
    originals: &BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
) {
    let events = BTreeMap::new();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 30,
    };
    let mut budget = whole_work();
    let proof = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let scratch = Scratch::new(memory, MemoryClass::Ordinary);
        let mut objects = Originals {
            values: originals,
            memory,
            class: MemoryClass::Ordinary,
            calls: 0,
        };
        let mut binder = Binder {
            events: &events,
            calls: 0,
        };
        let proof = qualify_world_current_state_v1(
            roots,
            CognitionCurrentPresenceV3::Enabled,
            &mut context(roots, &reader, &mut quota, &mut keys),
            &mut budget,
            &scratch,
            &mut objects,
            &mut binder,
        )
        .unwrap();
        assert_eq!(objects.calls, 2);
        assert_eq!(binder.calls, 0);
        assert_eq!(proof.native().field_count(), 97);
        assert_eq!(
            proof.cognition().presence(),
            CognitionCurrentPresenceV3::Enabled
        );
        proof.validate_binding(roots).unwrap();
        proof
    };
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [81; 32],
        authority: AuthorityBinding {
            token: [82; 32],
            epoch: 4,
        },
    };
    let field = oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|f| f.name == "time")
        .unwrap();
    let control=oasis7_world_state::incremental_runtime_controls::RuntimeControlFieldV1::NextGovernanceIdentityPenaltyId.descriptor();
    let mut write_quota = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let mut writer = MemberRecordWriteContext {
        sink: pages,
        memory,
        io: &mut write_quota,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let clock = oasis7_world_state::incremental_field_member::write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        oasis7_world_state::incremental_field_member::MemberPayloadRefV1::U64(1),
        &mut writer,
    )
    .unwrap();
    let penalty = oasis7_world_state::incremental_field_member::write_member_record_v1(
        control,
        WorldFieldRecordKindV1::Entry,
        &(),
        oasis7_world_state::incremental_field_member::MemberPayloadRefV1::U64(17),
        &mut writer,
    )
    .unwrap();
    drop(writer);
    use oasis7_world_state::incremental_runtime_allocators_codec::{
        RuntimeAllocatorV1, RuntimeSequenceV1,
    };
    let old = proof
        .raw()
        .allocators()
        .sequence(RuntimeAllocatorV1::Action);
    let (bytes, len) = RuntimeAllocatorV1::Action.canonical_bytes(RuntimeSequenceV1 {
        next: old.next.checked_add(1).unwrap(),
        era: old.era,
    });
    let action = write_value(
        &mut &bytes[..len],
        len as u64,
        pages,
        memory,
        &mut io(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap();
    let sealed = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [83; 32],
            &reader,
            memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        tx.write(
            StateNamespaceV1::Allocator,
            clock.field_key.as_bytes(),
            Some(clock.value_root),
        )
        .unwrap();
        tx.write(
            StateNamespaceV1::Allocator,
            penalty.field_key.as_bytes(),
            Some(penalty.value_root),
        )
        .unwrap();
        let (key, n) = RuntimeAllocatorV1::Action.key();
        tx.write(StateNamespaceV1::Allocator, &key[..n], Some(action))
            .unwrap();
        let guard = tx.guard();
        tx.seal(&guard, &parent.fence().unwrap(), 1).unwrap()
    };
    persist(memory, pages, sealed.new_nodes.clone());
    let after = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let counted = Counted {
            reader: &reader,
            reads: Cell::new(0),
        };
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [83; 32],
            source: &counted,
            memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 16 << 20,
        };
        let receipt = verifier.verify(&sealed.record).unwrap();
        drop(receipt);
        let once = counted.reads.replace(0);
        assert!(once > 0);
        let scratch = Scratch::new(memory, MemoryClass::Ordinary);
        let mut objects = Originals {
            values: originals,
            memory,
            class: MemoryClass::Ordinary,
            calls: 0,
        };
        let mut binder = Binder {
            events: &events,
            calls: 0,
        };
        let after = accept_world_current_state_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            CognitionCurrentPresenceV3::Enabled,
            &mut context(&sealed.record.after, &reader, &mut quota, &mut keys),
            &mut budget,
            &scratch,
            &mut objects,
            &mut binder,
        )
        .unwrap();
        after.validate_binding(&sealed.record.after).unwrap();
        assert_eq!(objects.calls, 0, "unchanged original Actions inherited");
        assert_eq!(
            counted.reads.get(),
            once,
            "whole conjunction verifies generic integrity exactly once"
        );
        assert_eq!(binder.calls, 0);
        after
    };
    let recovered = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Recovery,
        };
        let scratch = Scratch::new(memory, MemoryClass::Recovery);
        let mut objects = Originals {
            values: originals,
            memory,
            class: MemoryClass::Recovery,
            calls: 0,
        };
        let mut binder = Binder {
            events: &events,
            calls: 0,
        };
        let recovered = qualify_world_current_state_v1(
            &sealed.record.after,
            CognitionCurrentPresenceV3::Enabled,
            &mut context(&sealed.record.after, &reader, &mut quota, &mut keys),
            &mut budget,
            &scratch,
            &mut objects,
            &mut binder,
        )
        .unwrap();
        assert_eq!(objects.calls, 2);
        assert_eq!(recovered.allocation_class(), MemoryClass::Recovery);
        recovered
    };
    let used = memory.used();
    let reader = Reader {
        pages,
        memory,
        class: MemoryClass::Ordinary,
    };
    let admitted = readmit_world_current_state_ordinary_v1(
        &recovered,
        &sealed.record.after,
        &mut context(&sealed.record.after, &reader, &mut quota, &mut keys),
        &mut budget,
    )
    .unwrap();
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    admitted.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(admitted.native().field_count(), 97);
    assert_eq!(
        admitted.cognition().presence(),
        CognitionCurrentPresenceV3::Enabled
    );
    drop(admitted);
    assert_eq!(memory.used(), used);
    drop(after);
}
