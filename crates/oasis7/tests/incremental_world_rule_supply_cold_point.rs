#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    CrisisState, CrisisStatus, WorldState,
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_effects::*,
    incremental_world_rule_supply_root::*,
    world_rules::{SupplyAuthorizationV1, SupplyBatchV1, SupplyLineageV1, WorldRulesStateV1},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        ..fixture::physical()
    }
}
#[allow(dead_code)]
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
const KEY: &str = "outer-record\0é";
const OLD: &str = "old-member\0é";
#[path = "support/incremental_world_rule_supply_fixture.rs"]
mod supply_fixture;
use supply_fixture::{authorization, batch};
const TEXT_BYTES: usize = 32 << 20;
fn bounds() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        payload_bytes: TEXT_BYTES as u64,
        record_bytes: (TEXT_BYTES + 8192) as u64,
        ..limits()
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 100000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("supply-cold-point".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
struct HashOutput {
    hasher: blake3::Hasher,
    bytes: u64,
}
impl std::io::Write for HashOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn cold_supply_scalar_streams_larger_than_pool_and_member_point_writes_skip_it() {
    let directory = Directory::new();
    let path = directory.0.join("large-field");
    let (canonical, expected_hash) = {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _source = memory.reserve(40 << 20, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Zstd).unwrap();
        let mut auth = authorization();
        auth.id = "x".repeat(TEXT_BYTES);
        let expected = *blake3::hash(auth.id.as_bytes()).as_bytes();
        let source = WorldState {
            world_rules: Some(WorldRulesStateV1 {
                supply_authorizations: [(KEY.into(), auth)].into(),
                supply_batches: [(KEY.into(), batch())].into(),
                ..WorldRulesStateV1::default()
            }),
            ..WorldState::default()
        };
        let mut rows = produce(&store, &memory, &source);
        prepare_supply_root_genesis_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut store.value_session(physical()),
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: bounds(),
                cancelled: || false,
            },
            &mut 128,
            4 << 20,
            &mut |row| {
                rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
                Ok(())
            },
        )
        .unwrap();
        (
            roots(&build(&store, &memory, &rows))
                .canonical_bytes()
                .unwrap(),
            expected,
        )
    };
    // Source World and its 32MiB id have been dropped. Fill all Ordinary capacity;
    // root-only recovery and scalar streaming must fit the dedicated 4MiB reserve.
    // Zstd ordinary writes have an existing 16MiB encoder workspace.
    // 24MiB total admits that window and typed work, and remains below 32MiB text.
    let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
    let _outputs = memory.reserve(16 << 10, MemoryClass::Recovery).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        memory.clone(),
        PageCompression::Zstd,
        MemoryClass::Recovery,
    )
    .unwrap();
    let ordinary = memory
        .reserve((20 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut fixed = WorldFieldReadContextV1 {
        root,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: bounds().record_bytes,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let family = qualify_supply_family_v1(
        &roots,
        &mut fixed,
        &mut scan(),
        &mut scan(),
        &mut 4096,
        TEXT_BYTES as u64,
    )
    .unwrap();
    assert_eq!(family.record_counts(), [1, 1]);
    let proof = open_qualified_supply_record_v1(
        SupplyRootKeyV1::Authorization(KEY),
        &roots,
        &family,
        &mut fixed,
    )
    .unwrap()
    .unwrap();
    let mut scalar_io = logical();
    let mut scalar_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut scalar = WorldFieldReadContextV1 {
        root: proof.root(),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut scalar_io,
        },
        key_work: &mut scalar_keys,
        semantic_bytes: 1024,
        record_bytes: bounds().record_bytes,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut output = HashOutput {
        hasher: blake3::Hasher::new(),
        bytes: 0,
    };
    assert_eq!(
        read_qualified_supply_scalar_v1(
            SupplyScalarRequestV1 {
                owner: SupplyRootKeyV1::Authorization(KEY),
                field: SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::Id),
                qualification: &proof
            },
            &mut scalar,
            TEXT_BYTES as u64,
            &mut output
        )
        .unwrap(),
        Some(SupplyScalarValueV1::Text {
            utf8_bytes: TEXT_BYTES as u64
        })
    );
    assert_eq!(output.bytes, TEXT_BYTES as u64);
    assert_eq!(output.hasher.finalize().as_bytes(), &expected_hash);
    drop(ordinary);
    let point_state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut point_io = logical();
    point_io.read_bytes_remaining = 1 << 20;
    let mut point_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut point = WorldFieldReadContextV1 {
        root: proof.root(),
        state: &point_state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut point_io,
        },
        key_work: &mut point_keys,
        semantic_bytes: 1024,
        record_bytes: bounds().record_bytes,
        take_state_error: || point_state.take_read_error().map(state_error),
        cancelled: || false,
    };
    assert_eq!(
        read_qualified_supply_member_v1(
            SupplyMemberRequestV1 {
                owner: SupplyRootKeyV1::Authorization(KEY),
                member: SupplyMemberKeyV1::Request(OLD),
                qualification: &proof
            },
            &mut point
        )
        .unwrap(),
        Some(SupplyMemberValueV1::I64(i64::MIN))
    );
    assert!(matches!(
        read_qualified_supply_scalar_v1(
            SupplyScalarRequestV1 {
                owner: SupplyRootKeyV1::Authorization(KEY),
                field: SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::Id),
                qualification: &proof
            },
            &mut point,
            16,
            &mut std::io::sink()
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let parent = ExecutionParent {
        roots,
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [6; 32],
        &point_state,
        &memory,
        TransactionLimits {
            max_reads: 160,
            max_writes: 96,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut change_io = logical();
    change_io.read_bytes_remaining = 1 << 20;
    let mut change_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut context = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut change_io,
        },
        key_work: &mut change_keys,
        semantic_bytes: 1024,
        record_bytes: bounds().record_bytes,
        take_state_error: || point_state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let guard = context.transaction.guard();
    let changed = change_qualified_supply_member_v1(
        SupplyMemberChangeV1 {
            request: SupplyMemberRequestV1 {
                owner: SupplyRootKeyV1::Authorization(KEY),
                member: SupplyMemberKeyV1::Request(OLD),
                qualification: &proof,
            },
            expected: Some(SupplyMemberValueV1::I64(i64::MIN)),
            after: Some(SupplyMemberValueV1::I64(i64::MAX)),
            max_pending_node_bytes: 4 << 20,
        },
        &mut context,
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: bounds(),
            cancelled: || false,
        },
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    semantic.extend([5, 0, 1]);
    semantic.extend(2u64.to_be_bytes());
    semantic.extend(b"id");
    let key = WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap();
    assert_eq!(
        proof.root().get(key.as_bytes(), &point_state).unwrap(),
        changed.root().get(key.as_bytes(), &point_state).unwrap()
    );
    let guard = tx.guard();
    assert_eq!(tx.seal(&guard, &live, 1).unwrap().record.writes.len(), 1);
}
