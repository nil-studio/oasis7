#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
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
struct Missing<'a, R> {
    inner: &'a R,
    denied: [u8; 32],
    seen: std::cell::Cell<bool>,
}
impl<R: oasis7_formal_protocol::incremental_value::ValueNodeReader>
    oasis7_formal_protocol::incremental_value::ValueNodeReader for Missing<'_, R>
{
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if *id == self.denied {
            self.seen.set(true);
            return Err(ValueTreeError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "injected missing inner member data page",
            )));
        }
        self.inner.read_node(id, max, memory, class)
    }
}
#[path = "support/incremental_world_rule_supply_semantic_oracle.rs"]
mod semantic_oracle;
use semantic_oracle::{Path, semantic};
fn literal_rows(auth: bool) -> Vec<(Path<'static>, MemberPayloadCodecV1, Vec<u8>)> {
    use MemberPayloadCodecV1 as C;
    use Path::*;
    if auth {
        vec![
            (Record, C::Unit, vec![]),
            (Field("id"), C::Text, b"inner-auth".to_vec()),
            (Field("provider_id"), C::Text, b"provider".to_vec()),
            (Field("crisis_id"), C::Text, b"crisis".to_vec()),
            (Field("material"), C::Text, vec![]),
            (Field("review_proposal"), C::Text, vec![]),
            (
                Field("policy_version"),
                C::U64,
                u64::from(u32::MAX).to_be_bytes().to_vec(),
            ),
            (Field("starts_at"), C::U64, 0u64.to_be_bytes().to_vec()),
            (Field("expires_at"), C::U64, 1u64.to_be_bytes().to_vec()),
            (
                Field("remaining_escrow"),
                C::I64,
                i64::MIN.to_be_bytes().to_vec(),
            ),
            (Field("revision"), C::U64, 0u64.to_be_bytes().to_vec()),
            (Field("active"), C::Bool, vec![0]),
            (Crisis("crisis_id"), C::Text, b"evidence".to_vec()),
            (Crisis("kind"), C::Text, vec![]),
            (
                Crisis("severity"),
                C::U64,
                u64::from(u32::MAX).to_be_bytes().to_vec(),
            ),
            (Crisis("status"), C::Text, b"timed_out".to_vec()),
            (Crisis("opened_at"), C::U64, 0u64.to_be_bytes().to_vec()),
            (Crisis("expires_at"), C::U64, 1u64.to_be_bytes().to_vec()),
            (Flag("resolver_agent_id"), C::Bool, vec![1]),
            (Value("resolver_agent_id"), C::Text, vec![]),
            (Flag("strategy"), C::Bool, vec![0]),
            (Flag("success"), C::Bool, vec![1]),
            (Value("success"), C::Bool, vec![0]),
            (Crisis("impact"), C::I64, i64::MAX.to_be_bytes().to_vec()),
            (Flag("resolved_at"), C::Bool, vec![1]),
            (Value("resolved_at"), C::U64, 0u64.to_be_bytes().to_vec()),
            (Meta("beneficiaries"), C::Unit, vec![]),
            (Meta("requests"), C::Unit, vec![]),
            (Member("beneficiaries", OLD), C::Unit, vec![]),
            (
                Member("requests", OLD),
                C::I64,
                i64::MIN.to_be_bytes().to_vec(),
            ),
        ]
    } else {
        vec![
            (Record, C::Unit, vec![]),
            (Field("id"), C::Text, b"inner-batch".to_vec()),
            (Field("authorization_id"), C::Text, b"inner-auth".to_vec()),
            (Field("frozen_at"), C::U64, 0u64.to_be_bytes().to_vec()),
            (
                Field("authorization_revision"),
                C::U64,
                0u64.to_be_bytes().to_vec(),
            ),
            (Field("frozen_escrow"), C::I64, 0i64.to_be_bytes().to_vec()),
            (Field("allocated"), C::Bool, vec![0]),
            (Meta("frozen_lineages"), C::Unit, vec![]),
            (Meta("demands"), C::Unit, vec![]),
            (Meta("allocation"), C::Unit, vec![]),
            (
                Member("demands", OLD),
                C::I64,
                (-9i64).to_be_bytes().to_vec(),
            ),
            (
                Member("allocation", OLD),
                C::I64,
                0i64.to_be_bytes().to_vec(),
            ),
            (Member("frozen_lineages", OLD), C::Unit, vec![]),
            (
                Frozen(OLD, "target"),
                C::I64,
                (-7i64).to_be_bytes().to_vec(),
            ),
            (
                Frozen(OLD, "fulfilled"),
                C::I64,
                5i64.to_be_bytes().to_vec(),
            ),
            (Frozen(OLD, "reserved"), C::I64, 3i64.to_be_bytes().to_vec()),
        ]
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
    let mut roots = RootSetV1::empty("supply-literal".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
#[test]
fn borrowed_supply_genesis_preserves_all_literal_fields_and_cold_root_only_closure() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("literal");
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let canonical = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let mut auth = authorization();
            auth.policy_version = u32::MAX;
            auth.remaining_escrow = i64::MIN;
            auth.crisis_evidence.severity = u32::MAX;
            auth.crisis_evidence.impact = i64::MAX;
            auth.crisis_evidence.resolver_agent_id = Some(String::new());
            let source = WorldState {
                world_rules: Some(WorldRulesStateV1 {
                    supply_authorizations: [(KEY.into(), auth)].into(),
                    supply_batches: [(KEY.into(), batch())].into(),
                    ..WorldRulesStateV1::default()
                }),
                ..WorldState::default()
            };
            let mut rows = produce(&store, &memory, &source);
            let mut writer = store.value_session(physical());
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    payload_bytes: 165,
                    ..limits()
                },
                cancelled: || false,
            };
            assert_eq!(
                prepare_supply_root_genesis_v1(
                    &source,
                    &mut write,
                    &mut 128,
                    4 << 20,
                    &mut |row| {
                        rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
                        Ok(())
                    }
                )
                .unwrap(),
                SupplyRootGenesisCountsV1 {
                    authorizations: 1,
                    batches: 1
                }
            );
            assert_eq!(rows.len(), 4);
            roots(&build(&store, &memory, &rows))
                .canonical_bytes()
                .unwrap()
        };
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let auth_root = roots
            .state
            .iter()
            .find(|r| r.namespace == Ns::Authorization)
            .unwrap();
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = WorldFieldReadContextV1 {
            root: auth_root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family =
            qualify_supply_family_v1(&roots, &mut read, &mut scan(), &mut scan(), &mut 4096, 165)
                .unwrap();
        assert_eq!(family.record_counts(), [1, 1]);
        eprintln!(
            "whole family {compression:?}: state={:?}, values={:?}",
            state.remaining_io().unwrap(),
            values.remaining_io().unwrap()
        );
        for (owner, auth) in [
            (SupplyRootKeyV1::Authorization(KEY), true),
            (SupplyRootKeyV1::Batch(KEY), false),
        ] {
            let proof = open_qualified_supply_record_v1(owner, &roots, &family, &mut read)
                .unwrap()
                .unwrap();
            scalar_oracle::assert_fields(
                &store,
                &memory,
                owner,
                &proof,
                MemoryClass::Recovery,
                true,
            );
            let expected = literal_rows(auth);
            assert_eq!(proof.root().leaf_count, expected.len() as u64);
            for (path, codec, expected) in expected {
                let semantic = semantic(path);
                let key = WorldFieldKeyV1::new(
                    descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    Some(*blake3::hash(&semantic).as_bytes()),
                )
                .unwrap();
                // Each independent literal query has its own finite physical session;
                // the family bootstrap above retains one continuous scope budget.
                let point_state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let root = proof
                    .root()
                    .get(key.as_bytes(), &point_state)
                    .unwrap()
                    .unwrap();
                let mut actual = Vec::new();
                let mut primitive_io = logical();
                let mut primitive = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut primitive_io,
                };
                read_member_record_profile_v1(
                    &root,
                    &key,
                    &mut primitive,
                    MemberReadProfileV1 {
                        codec,
                        semantic_bytes: semantic.len() as u64,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 165,
                            record_bytes: 8192,
                        },
                    },
                    &mut std::io::sink(),
                    &mut actual,
                    &mut || false,
                )
                .unwrap();
                assert_eq!(actual, expected);
            }
            if auth {
                let raw = semantic(Path::Field("material"));
                let key = WorldFieldKeyV1::new(
                    descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    Some(*blake3::hash(&raw).as_bytes()),
                )
                .unwrap();
                let point_state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let value = proof
                    .root()
                    .get(key.as_bytes(), &point_state)
                    .unwrap()
                    .unwrap();
                let mut probe_io = logical();
                let mut primitive = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut probe_io,
                };
                let data = primitive.load_root(&value).unwrap().0.id;
                let missing = Missing {
                    inner: &values,
                    denied: data,
                    seen: std::cell::Cell::new(false),
                };
                let mut bad_io = logical();
                let mut bad_keys = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                let bad_state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let mut bad = WorldFieldReadContextV1 {
                    root: auth_root,
                    state: &bad_state,
                    values: ValueReadContext {
                        source: &missing,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut bad_io,
                    },
                    key_work: &mut bad_keys,
                    semantic_bytes: 1024,
                    record_bytes: 8192,
                    take_state_error: || bad_state.take_read_error().map(state_error),
                    cancelled: || false,
                };
                let error = qualify_supply_family_v1(
                    &roots,
                    &mut bad,
                    &mut scan(),
                    &mut scan(),
                    &mut 4096,
                    165,
                )
                .unwrap_err();
                assert!(missing.seen.get());
                assert!(
                    matches!(error,ValueTreeError::Io(e) if e.kind()==std::io::ErrorKind::NotFound)
                );
            }
            if auth {
                let semantic = semantic(Path::Value("strategy"));
                let key = WorldFieldKeyV1::new(
                    descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    Some(*blake3::hash(&semantic).as_bytes()),
                )
                .unwrap();
                let point_state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                assert_eq!(
                    proof.root().get(key.as_bytes(), &point_state).unwrap(),
                    None
                );
            }
        }
    }
}

#[path = "support/incremental_world_rule_supply_scalar_oracle.rs"]
mod scalar_oracle;
