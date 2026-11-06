use super::*;
use crate::{
    WorldState,
    incremental_field_member::*,
    incremental_typed_record_test_support::{Pages, Writer, io},
    incremental_world_state_genesis::*,
};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateNodeWriteV1, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> WorldPrimitiveFamilyWorkV1 {
    WorldPrimitiveFamilyWorkV1 {
        records_remaining: 100000,
        max_record_bytes: 4 << 20,
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
fn descriptor(name: &str) -> FieldDescriptor {
    WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|f| f.name == name)
        .unwrap()
}
fn source() -> WorldState {
    let mut s = WorldState::default();
    s.time = 7;
    s.main_token_balances.insert(
        "outer-account".into(),
        crate::MainTokenAccountBalance {
            account_id: " original-inner-account ".into(),
            liquid_balance: u64::MAX,
            vested_balance: 7,
            restricted_starter_claim_balance: 11,
        },
    );
    s.node_asset_balances.insert(
        "outer-node".into(),
        crate::NodeAssetBalance {
            node_id: " original-inner-node ".into(),
            power_credit_balance: u64::MAX,
            total_minted_credits: 13,
            total_burned_credits: 17,
        },
    );
    s.reputation_scores
        .insert(" original/key ".into(), i64::MIN);
    s.module_artifact_owners
        .insert(" large-artifact ".into(), "original_sender/Ω".repeat(16384));
    s.module_artifact_owners
        .insert("small-artifact".into(), "original-small-sender".into());
    s.installed_module_targets
        .insert(" target self ".into(), ModuleInstallTarget::SelfAgent);
    s.installed_module_targets.insert(
        "target-location".into(),
        ModuleInstallTarget::LocationInfrastructure {
            location_id: " original-location ".into(),
        },
    );
    s
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
fn install(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut writer = Writer(pages);
    let mut rows = Vec::new();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let summary = prepare_world_state_genesis_v1(
        &source(),
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut quota,
            key_work: &mut keys,
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
    assert_eq!(summary.native_fields, 97);
    let mut roots = RootSetV1::empty("primitive29-real-gen97".into(), [6; 32]).unwrap();
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
        let delta = StateDeltaV1::build(root.clone(), writes, pages, 8 << 20).unwrap();
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
    keys: &'a mut crate::incremental_semantic_key::SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
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
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 32,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn member<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    memory: &MemoryBudget,
    pages: &Pages,
    name: &str,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
) -> MemberRecordV1 {
    write_member_record_v1(
        descriptor(name),
        WorldFieldRecordKindV1::Entry,
        key,
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
fn fixed<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    memory: &MemoryBudget,
    name: &str,
    key: &K,
) -> WorldFieldKeyV1 {
    let digest = crate::incremental_semantic_key::hash_semantic_key_v1(
        key,
        memory,
        MemoryClass::Ordinary,
        &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        || false,
    )
    .unwrap();
    WorldFieldKeyV1::new(
        descriptor(name),
        WorldFieldRecordKindV1::Entry,
        Some(digest.digest),
    )
    .unwrap()
}
fn ns(key: &WorldFieldKeyV1) -> StateNamespaceV1 {
    WorldFieldKeyV1::from_bytes(key.as_bytes())
        .unwrap()
        .namespace
}
#[test]
fn primitive29_real_gen97_and_general_scalar_map_text_installed_deltas() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let proof = qualify_world_primitive_fields_v1(
        &roots,
        &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.field_count(), 29);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    for row in [
        member(
            &memory,
            &pages,
            "main_token_balances",
            "outer-account",
            MemberPayloadRefV1::IdentityU64Triple {
                identity: " updated-inner-account ",
                amounts: [u64::MAX, 19, 23],
            },
        ),
        member(
            &memory,
            &pages,
            "node_asset_balances",
            "outer-node",
            MemberPayloadRefV1::IdentityU64Triple {
                identity: " updated-inner-node ",
                amounts: [29, 31, u64::MAX],
            },
        ),
        member(&memory, &pages, "time", &(), MemberPayloadRefV1::U64(42)),
        member(
            &memory,
            &pages,
            "reputation_scores",
            " original/key ",
            MemberPayloadRefV1::I64(i64::MAX),
        ),
        member(
            &memory,
            &pages,
            "module_artifact_owners",
            "small-artifact",
            MemberPayloadRefV1::Text("updated-original-sender"),
        ),
        member(
            &memory,
            &pages,
            "installed_module_targets",
            "new-installed",
            MemberPayloadRefV1::Unit,
        ),
        member(
            &memory,
            &pages,
            "installed_module_targets",
            " target self ",
            MemberPayloadRefV1::Text("original new location"),
        ),
    ] {
        tx.write(
            ns(&row.field_key),
            row.field_key.as_bytes(),
            Some(row.value_root),
        )
        .unwrap();
    }
    let removed = fixed(&memory, "installed_module_targets", "target-location");
    tx.write(ns(&removed), removed.as_bytes(), None).unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    persist(&memory, &pages, &prepared.new_nodes);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let after = accept_world_primitive_transition_v1(
        &proof,
        &verifier,
        &prepared.record,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut work(),
    )
    .unwrap();
    assert_eq!(after.field_count(), 29);
    after.validate_binding(&prepared.record.after).unwrap();
    use crate::incremental_world_balances::{
        WorldBalanceFieldV1, WorldBalanceReadV1, read_world_balance_v1,
    };
    let _output = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
    let mut identity = Vec::with_capacity(128);
    let account = read_world_balance_v1(
        WorldBalanceFieldV1::MainToken,
        "outer-account",
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        128,
        &mut identity,
    )
    .unwrap()
    .unwrap();
    assert_eq!(identity, b" updated-inner-account ");
    assert!(matches!(
        account,
        WorldBalanceReadV1::MainToken {
            liquid_balance: u64::MAX,
            vested_balance: 19,
            restricted_starter_claim_balance: 23,
            ..
        }
    ));
    identity.clear();
    let node = read_world_balance_v1(
        WorldBalanceFieldV1::NodeAsset,
        "outer-node",
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        128,
        &mut identity,
    )
    .unwrap()
    .unwrap();
    assert_eq!(identity, b" updated-inner-node ");
    assert!(matches!(
        node,
        WorldBalanceReadV1::NodeAsset {
            power_credit_balance: 29,
            total_minted_credits: 31,
            total_burned_credits: u64::MAX,
            ..
        }
    ));

    qualify_world_primitive_fields_v1(
        &prepared.record.after,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
}
#[test]
fn primitive29_scalar_delta_inherits_unchanged_large_original_string() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let proof = qualify_world_primitive_fields_v1(
        &roots,
        &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let key = fixed(&memory, "module_artifact_owners", " large-artifact ");
    let value = roots
        .state_root(ns(&key))
        .get(key.as_bytes(), &pages)
        .unwrap()
        .unwrap();
    pages.remove(&value);
    // Logical inheritance only; absence of this unchanged physical page still
    // prevents a complete physical endpoint from claiming WorldReady.
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    let row = member(&memory, &pages, "time", &(), MemberPayloadRefV1::U64(43));
    tx.write(
        ns(&row.field_key),
        row.field_key.as_bytes(),
        Some(row.value_root),
    )
    .unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    accept_world_primitive_transition_v1(
        &proof,
        &verifier,
        &prepared.record,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut WorldPrimitiveFamilyWorkV1 {
            records_remaining: 8,
            max_record_bytes: 4096,
        },
    )
    .unwrap();
}
#[test]
fn primitive29_receiving_rejects_metadata_scalar_deletion_bad_semantic_payload_and_kind() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let proof = qualify_world_primitive_fields_v1(
        &roots,
        &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    for case in 0..5 {
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits())
                .unwrap();
        match case {
            0 => {
                let key = WorldFieldKeyV1::new(
                    descriptor("time"),
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .unwrap();
                tx.write(ns(&key), key.as_bytes(), None).unwrap();
            }
            1 => {
                let key = fixed(&memory, "time", &());
                tx.write(ns(&key), key.as_bytes(), None).unwrap();
            }
            2 => {
                let row = member(
                    &memory,
                    &pages,
                    "reputation_scores",
                    &99u64,
                    MemberPayloadRefV1::I64(1),
                );
                tx.write(
                    ns(&row.field_key),
                    row.field_key.as_bytes(),
                    Some(row.value_root),
                )
                .unwrap();
            }
            3 => {
                let row = member(
                    &memory,
                    &pages,
                    "reputation_scores",
                    "bad-payload",
                    MemberPayloadRefV1::U64(1),
                );
                tx.write(
                    ns(&row.field_key),
                    row.field_key.as_bytes(),
                    Some(row.value_root),
                )
                .unwrap();
            }
            _ => {
                let row = member(
                    &memory,
                    &pages,
                    "reputation_scores",
                    "bad-kind",
                    MemberPayloadRefV1::I64(1),
                );
                let mut raw = row.field_key.as_bytes().to_vec();
                raw[b"oasis7.iws.world-field-key.v1\0".len() + 4] = 255;
                tx.write(ns(&row.field_key), &raw, Some(row.value_root))
                    .unwrap();
            }
        }
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [3; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        assert!(
            accept_world_primitive_transition_v1(
                &proof,
                &verifier,
                &prepared.record,
                &mut context(
                    &memory,
                    &pages,
                    &prepared.record.after,
                    &mut quota,
                    &mut keys
                ),
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
fn primitive29_recovery_proof_requires_explicit_ordinary_readmission() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let roots = install(&memory, &pages);
    let recovery = Recovery {
        pages: &pages,
        memory: &memory,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let proof = qualify_world_primitive_fields_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
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
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.field_count(), 29);
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    assert!(
        accept_world_primitive_transition_v1(
            &proof,
            &verifier,
            &prepared.record,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut quota,
                &mut keys
            ),
            &mut work()
        )
        .is_err()
    );
    let promoted =
        readmit_world_primitive_fields_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    assert_eq!(promoted.allocation_class(), MemoryClass::Ordinary);
    accept_world_primitive_transition_v1(
        &promoted,
        &verifier,
        &prepared.record,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut work(),
    )
    .unwrap();
    assert!(
        readmit_world_primitive_fields_ordinary_v1(&promoted, &roots, &memory, || false).is_err()
    );
    let foreign = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    assert!(
        readmit_world_primitive_fields_ordinary_v1(&proof, &roots, &foreign, || false).is_err()
    );
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(readmit_world_primitive_fields_ordinary_v1(&proof, &wrong, &memory, || false).is_err());
    let mut polls = 0;
    assert!(matches!(
        readmit_world_primitive_fields_ordinary_v1(&proof, &roots, &memory, || {
            polls += 1;
            polls > 1
        }),
        Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled))
    ));
    let exhausted = memory
        .reserve((480 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_world_primitive_fields_ordinary_v1(&proof, &roots, &memory, || false),
        Err(WorldStateOwnerError::Memory(_))
    ));
    drop(exhausted);
}
