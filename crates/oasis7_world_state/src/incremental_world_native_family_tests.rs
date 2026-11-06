use super::super::prepare_world_native_metadata_v1;
use super::super::prepare_world_native_record_v1;
use super::*;
use oasis7_formal_protocol::incremental_execution::ExecutionWriteV1;
#[path = "incremental_world_native_family_test_support.rs"]
mod support;
use crate::incremental_world_native_gameplay::{GameplayPolicyFieldV1, MetaProgressFieldV1};
use support::*;
fn meta(id: &str) -> crate::MetaProgressState {
    crate::MetaProgressState {
        agent_id: id.into(),
        track_points: std::collections::BTreeMap::from([("".into(), i64::MIN)]),
        total_points: i64::MAX,
        achievements: vec!["repeat".into(), "repeat".into()],
        unlocked_tiers: std::collections::BTreeMap::from([("empty".into(), vec![])]),
        last_granted_at: 0,
    }
}
struct OneMeta;
impl WorldNativeFieldSpecV1 for OneMeta {
    type Key = String;
    type Value = crate::MetaProgressState;
    const NAME: &'static str = "meta_progress";
    const COLUMNS: &'static str = "meta_progress.columns";
    const SHAPE: FieldShape = FieldShape::Map;
    const MAX_RECORDS: Option<u64> = Some(1);
}
struct NumericMeta;
impl WorldNativeFieldSpecV1 for NumericMeta {
    type Key = u64;
    type Value = crate::MetaProgressState;
    const NAME: &'static str = "meta_progress";
    const COLUMNS: &'static str = "meta_progress.columns";
    const SHAPE: FieldShape = FieldShape::Map;
}
#[test]
fn native_field_full_scope_requires_metadata_preserves_empty_map_and_honors_bound() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let absent = RootSetV1::empty("absent".into(), [73; 32]).unwrap();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    assert!(
        qualify_world_native_field_v1::<MetaProgressFieldV1>(
            &absent,
            &mut context(
                &memory,
                &pages,
                absent.state_root(StateNamespaceV1::Business),
                &mut quota,
                &mut keys
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
    let empty = rows::<MetaProgressFieldV1>(&memory, &pages, &[]);
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &empty);
    let proof = qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.records(), 0);
    let rows = rows::<MetaProgressFieldV1>(
        &memory,
        &pages,
        &[
            ("outer-a".into(), meta("inner-a")),
            ("outer-b".into(), meta("inner-b")),
        ],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &rows);
    assert!(
        qualify_world_native_field_v1::<OneMeta>(
            &roots,
            &mut context(
                &memory,
                &pages,
                roots.state_root(StateNamespaceV1::Business),
                &mut quota,
                &mut keys
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
    let proof = qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.records(), 2);
    assert_eq!(crate::incremental_world_native_governance::GovernanceFinalityKeyRotationsFieldV1::MAX_RECORDS,Some(256));
    assert!(
        qualify_world_native_field_v1::<MetaProgressFieldV1>(
            &roots,
            &mut context(
                &memory,
                &pages,
                roots.state_root(StateNamespaceV1::Authorization),
                &mut quota,
                &mut keys
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
}
#[test]
fn native_field_map_text_never_accepts_numeric_original_key() {
    // This test-only spec produces an otherwise valid envelope with the wrong
    // original key type. It is never part of the complete native registry.
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let rows = rows::<NumericMeta>(&memory, &pages, &[(99, meta("inner"))]);
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &rows);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    assert!(
        qualify_world_native_field_v1::<MetaProgressFieldV1>(
            &roots,
            &mut context(
                &memory,
                &pages,
                roots.state_root(StateNamespaceV1::Business),
                &mut quota,
                &mut keys
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
}

#[test]
fn native_value_field_requires_one_original_unit_record() {
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let only_meta = rows::<GameplayPolicyFieldV1>(&memory, &pages, &[]);
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &only_meta);
    assert!(
        qualify_world_native_field_v1::<GameplayPolicyFieldV1>(
            &roots,
            &mut context(
                &memory,
                &pages,
                roots.state_root(StateNamespaceV1::Business),
                &mut quota,
                &mut keys
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
    let rows = rows::<GameplayPolicyFieldV1>(
        &memory,
        &pages,
        &[((), crate::GameplayPolicyState::default())],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &rows);
    let proof = qualify_world_native_field_v1::<GameplayPolicyFieldV1>(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.records(), 1);
}

#[path = "incremental_world_native_family_transition_tests.rs"]
mod transition;

#[path = "incremental_world_native_family_recovery_tests.rs"]
mod recovery;
