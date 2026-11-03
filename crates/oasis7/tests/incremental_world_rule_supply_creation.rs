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
    WorldState, incremental_field_member::*, incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_world_rule_effects::*,
    incremental_world_rule_supply_root::*, world_rules::WorldRulesStateV1,
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
#[derive(Clone, Copy, Debug)]
enum Kind {
    Beneficiary,
    Request,
    Demand,
    Allocation,
    Frozen,
}
impl Kind {
    fn auth(self) -> bool {
        matches!(self, Self::Beneficiary | Self::Request)
    }
    fn collection(self) -> &'static str {
        match self {
            Self::Beneficiary => "beneficiaries",
            Self::Request => "requests",
            Self::Demand => "demands",
            Self::Allocation => "allocation",
            Self::Frozen => "frozen_lineages",
        }
    }
}
fn text(s: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(s.len() as u64).to_be_bytes());
    b.extend(s.as_bytes());
    b
}
fn pair(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let mut out = vec![5];
    out.extend(a);
    out.extend(b);
    out
}
fn orphan_key(kind: Kind, column: Option<&str>) -> WorldFieldKeyV1 {
    let root = pair(
        text(if kind.auth() {
            "supply_authorizations"
        } else {
            "supply_batches"
        }),
        text(KEY),
    );
    let member = pair(pair(root, text(kind.collection())), text(OLD));
    let body = column.map_or_else(|| member.clone(), |name| pair(member.clone(), text(name)));
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    semantic.extend(body);
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap()
}
fn audit(kind: Kind) {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let _fixture_admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("supply-create-audit"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let source = WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    };
    let mut rows = produce(&store, &memory, &source);
    let parent_len = rows.len();
    // Immutable 0db3912 retains the full lawful old GEN -> strip -> successful
    // orphan adoption red. Here generic public framing recreates exactly that
    // stripped map, without publishing obsolete flat execution just for a seed.
    {
        let mut write = MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        };
        macro_rules! row {
            ($semantic:expr,$payload:expr,$column:expr) => {{
                let row = write_member_record_v1(
                    descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    $semantic,
                    $payload,
                    &mut write,
                )
                .unwrap();
                assert_eq!(row.field_key, orphan_key(kind, $column));
                rows.push((row.field_key.as_bytes().to_vec(), row.value_root));
            }};
        }
        match kind {
            Kind::Beneficiary => row!(
                &((("supply_authorizations", KEY), "beneficiaries"), OLD),
                MemberPayloadRefV1::Unit,
                None
            ),
            Kind::Request => row!(
                &((("supply_authorizations", KEY), "requests"), OLD),
                MemberPayloadRefV1::I64(i64::MIN),
                None
            ),
            Kind::Demand => row!(
                &((("supply_batches", KEY), "demands"), OLD),
                MemberPayloadRefV1::I64(-9),
                None
            ),
            Kind::Allocation => row!(
                &((("supply_batches", KEY), "allocation"), OLD),
                MemberPayloadRefV1::I64(0),
                None
            ),
            Kind::Frozen => {
                row!(
                    &((("supply_batches", KEY), "frozen_lineages"), OLD),
                    MemberPayloadRefV1::Unit,
                    None
                );
                for (name, value) in [("target", -7), ("fulfilled", 5), ("reserved", 3)] {
                    row!(
                        &(((("supply_batches", KEY), "frozen_lineages"), OLD), name),
                        MemberPayloadRefV1::I64(value),
                        Some(name)
                    );
                }
            }
        }
    }
    assert_eq!(
        rows.len() - parent_len,
        if matches!(kind, Kind::Frozen) { 4 } else { 1 }
    );
    let root = build(&store, &memory, &rows);
    let mut roots = RootSetV1::empty("supply-create-audit".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root;
    let parent = ExecutionParent {
        roots,
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 160,
            max_writes: 96,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut read = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut logical(),
        },
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut logical(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        limits: limits(),
        cancelled: || false,
    };
    let guard = read.transaction.guard();
    assert!(
        change_world_rule_effect_overlay_v1(
            WorldRuleEffectChangeV1 {
                key: "p",
                expected: WorldRuleEffectExpectedV1::Value("original"),
                after: Some("prefix"),
                max_current_payload_bytes: 128
            },
            &mut read,
            &mut write,
            &guard,
            &live
        )
        .unwrap()
    );
    let error = read
        .transaction
        .contribute(
            |_| -> Result<_, oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1> {
                let root = parent
                    .roots
                    .state
                    .iter()
                    .find(|r| r.namespace == Ns::Authorization)
                    .unwrap();
                let mut fixed = WorldFieldReadContextV1 {
                    root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 8192,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || false,
                };
                let mut outer = StateScanBudget {
                    nodes_remaining: 100000,
                    leaves_remaining: 100000,
                    read_bytes_remaining: 256 << 20,
                };
                let mut inner = StateScanBudget {
                    nodes_remaining: 100000,
                    leaves_remaining: 100000,
                    read_bytes_remaining: 256 << 20,
                };
                Ok(qualify_supply_family_v1(
                    &parent.roots,
                    &mut fixed,
                    &mut outer,
                    &mut inner,
                    &mut 1024,
                    165,
                )?)
            },
        )
        .unwrap_err();
    // A quota/codec failure must not cover a qualifier regression. No qualified
    // parent may escape to the normal empty creator for this persisted input.
    assert!(
        matches!(
            error,
            oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                ValueTreeError::Invalid("supply flat orphan descendant")
            )
        ),
        "{kind:?}: {error:?}"
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
    ));
}
#[test]
fn empty_authorization_must_not_revive_beneficiary() {
    audit(Kind::Beneficiary);
}
#[test]
fn empty_authorization_must_not_revive_request() {
    audit(Kind::Request);
}
#[test]
fn empty_batch_must_not_revive_demand() {
    audit(Kind::Demand);
}
#[test]
fn empty_batch_must_not_revive_allocation() {
    audit(Kind::Allocation);
}
#[test]
fn empty_batch_must_not_revive_frozen_lineage() {
    audit(Kind::Frozen);
}
