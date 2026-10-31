#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_agent_api::world_action::{
    OrganizationEstateRightV1 as Right, OrganizationEstateScopeV1 as Scope,
};
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
    WorldState,
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_effects::*,
    incremental_world_rule_organization::root::*,
    incremental_world_rule_organization::*,
    world_rules::{
        OrganizationCaseV1, OrganizationDispositionPhaseV1 as Disposition,
        OrganizationEstateDepositV1, OrganizationGrantV1, OrganizationProtectionPhaseV1 as Phase,
        OrganizationRecoveryRequestV1, WorldRulesStateV1,
    },
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
// The rich owner fixture has55 original leaves and at most19 fixed query paths.
// Choose finite fixture operations up front; retain the existing byte caps.
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
const CASE: &str = " outer-case\0\u{e9} ";
const ESTATE: &str = "estate-key\0\u{e9}";
const GRANT: &str = "grant\0outer";
const REQUEST: &str = "request\0outer";
fn scope(object_id: &str, right: Right) -> Scope {
    Scope {
        object_id: object_id.into(),
        right,
    }
}
fn grant() -> OrganizationGrantV1 {
    OrganizationGrantV1 {
        id: "grant-inner".into(),
        delegate_id: "delegate".into(),
        scopes: [
            scope("", Right::Preserve),
            scope("return\0\u{e9}", Right::ReturnTrust),
            scope("residual", Right::DistributeResidual),
            scope("finalize", Right::FinalizeCase),
            scope("advance", Right::AdvanceCase),
        ]
        .into(),
        revision: 0,
        expires_at: u64::MAX,
        source_proposal: String::new(),
        replaces: Some(String::new()),
    }
}
fn recovery() -> OrganizationRecoveryRequestV1 {
    OrganizationRecoveryRequestV1 {
        requester_id: "requester-inner".into(),
        appeal: true,
        approved: Some(false),
        resolved_by: Some(String::new()),
    }
}
fn case() -> OrganizationCaseV1 {
    OrganizationCaseV1 {
        id: "case-inner".into(),
        organization_id: "org\0\u{e9}".into(),
        founder_id: String::new(),
        members: [String::new(), "member\0\u{e9}".into()].into(),
        policy_version: u32::MAX,
        revision: u64::MAX,
        phase: Phase::Protected,
        disposition: Disposition::Obligations,
        notified_at: None,
        protected_at: Some(0),
        grants: [(GRANT.into(), grant())].into(),
        requests: [(REQUEST.into(), recovery())].into(),
        stage_receipts: vec![String::new(), "repeat".into(), "repeat".into()],
    }
}
fn estate() -> OrganizationEstateDepositV1 {
    OrganizationEstateDepositV1 {
        id: "estate-inner".into(),
        organization_id: "different-org".into(),
        contributor_id: "contributor".into(),
        material: String::new(),
        amount: i64::MIN,
        held_in_trust: true,
        disposed: false,
        effect_receipt: Some(String::new()),
    }
}
fn source() -> WorldState {
    WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            organization_cases: [(CASE.into(), case())].into(),
            organization_estate: [(ESTATE.into(), estate())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    }
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Object<'a> {
    Case(&'a str),
    Estate(&'a str),
    Grant(&'a str, &'a str),
    Recovery(&'a str, &'a str),
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Part<'a> {
    Record,
    Field(&'a str),
    Flag(&'a str),
    Option(&'a str),
    Collection(&'a str),
    Member(&'a str, &'a str),
    Scope(&'a str, &'a str),
    Ordinal(u64),
    Length,
}
fn text(v: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(v.len() as u64).to_be_bytes());
    b.extend_from_slice(v.as_bytes());
    b
}
fn pair(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let mut out = vec![5];
    out.extend(a);
    out.extend(b);
    out
}
fn literal_outer_key(object: Object<'_>) -> WorldFieldKeyV1 {
    let (family, original) = match object {
        Object::Case(key) | Object::Grant(key, _) | Object::Recovery(key, _) => {
            ("organization_cases", key)
        }
        Object::Estate(key) => ("organization_estate", key),
    };
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend(pair(text(family), text(original)));
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&bytes).as_bytes()),
    )
    .unwrap()
}
fn semantic(object: Object<'_>, part: Part<'_>) -> Vec<u8> {
    let root = match object {
        Object::Case(_) | Object::Estate(_) => vec![0],
        Object::Grant(_, key) => pair(pair(vec![0], text("grants")), text(key)),
        Object::Recovery(_, key) => pair(pair(vec![0], text("requests")), text(key)),
    };
    let body = match part {
        Part::Record => root,
        Part::Field(name) => pair(root, text(name)),
        Part::Flag(name) => pair(pair(root, text(name)), vec![0]),
        Part::Option(name) => pair(pair(root, text(name)), text("value")),
        Part::Collection(name) => pair(pair(root, text(name)), vec![0]),
        Part::Member(name, key) => pair(pair(root, text(name)), text(key)),
        Part::Scope(object, right) => {
            pair(pair(root, text("scopes")), pair(text(object), text(right)))
        }
        Part::Length => pair(pair(root, text("stage_receipts")), text("length")),
        Part::Ordinal(index) => {
            let mut n = vec![2];
            n.extend(index.to_be_bytes());
            pair(pair(root, text("stage_receipts")), n)
        }
    };
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend(body);
    bytes
}
fn key(object: Object<'_>, part: Part<'_>) -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(object, part)).as_bytes()),
    )
    .unwrap()
}
fn frame(object: Object<'_>, part: Part<'_>, codec: u8, payload: &[u8]) -> Vec<u8> {
    let key = key(object, part);
    let semantic = semantic(object, part);
    let mut b = b"oasis7.iws.field-member.v1\0".to_vec();
    b.extend_from_slice(&(key.as_bytes().len() as u16).to_be_bytes());
    b.push(codec);
    b.extend_from_slice(&(semantic.len() as u64).to_be_bytes());
    b.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    b.extend_from_slice(key.as_bytes());
    b.extend(semantic);
    b.extend(payload);
    b
}
type Golden = (Object<'static>, Part<'static>, u8, Vec<u8>);
fn golden() -> Vec<Golden> {
    let c = Object::Case(CASE);
    let e = Object::Estate(ESTATE);
    let g = Object::Grant(CASE, GRANT);
    let r = Object::Recovery(CASE, REQUEST);
    let mut v = vec![
        (c, Part::Field("id"), 5, b"case-inner".to_vec()),
        (
            c,
            Part::Field("organization_id"),
            5,
            "org\0\u{e9}".as_bytes().to_vec(),
        ),
        (c, Part::Field("founder_id"), 5, vec![]),
        (
            c,
            Part::Field("policy_version"),
            2,
            u64::from(u32::MAX).to_be_bytes().to_vec(),
        ),
        (
            c,
            Part::Field("revision"),
            2,
            u64::MAX.to_be_bytes().to_vec(),
        ),
        (c, Part::Field("phase"), 5, b"protected".to_vec()),
        (c, Part::Field("disposition"), 5, b"obligations".to_vec()),
        (c, Part::Flag("notified_at"), 4, vec![0]),
        (c, Part::Flag("protected_at"), 4, vec![1]),
        (
            c,
            Part::Option("protected_at"),
            2,
            0u64.to_be_bytes().to_vec(),
        ),
        (c, Part::Length, 2, 3u64.to_be_bytes().to_vec()),
        (e, Part::Field("id"), 5, b"estate-inner".to_vec()),
        (
            e,
            Part::Field("organization_id"),
            5,
            b"different-org".to_vec(),
        ),
        (e, Part::Field("contributor_id"), 5, b"contributor".to_vec()),
        (e, Part::Field("material"), 5, vec![]),
        (e, Part::Field("amount"), 3, i64::MIN.to_be_bytes().to_vec()),
        (e, Part::Field("held_in_trust"), 4, vec![1]),
        (e, Part::Field("disposed"), 4, vec![0]),
        (e, Part::Flag("effect_receipt"), 4, vec![1]),
        (e, Part::Option("effect_receipt"), 5, vec![]),
        (g, Part::Field("id"), 5, b"grant-inner".to_vec()),
        (g, Part::Field("delegate_id"), 5, b"delegate".to_vec()),
        (g, Part::Field("revision"), 2, 0u64.to_be_bytes().to_vec()),
        (
            g,
            Part::Field("expires_at"),
            2,
            u64::MAX.to_be_bytes().to_vec(),
        ),
        (g, Part::Field("source_proposal"), 5, vec![]),
        (g, Part::Flag("replaces"), 4, vec![1]),
        (g, Part::Option("replaces"), 5, vec![]),
        (
            r,
            Part::Field("requester_id"),
            5,
            b"requester-inner".to_vec(),
        ),
        (r, Part::Field("appeal"), 4, vec![1]),
        (r, Part::Flag("approved"), 4, vec![1]),
        (r, Part::Option("approved"), 4, vec![0]),
        (r, Part::Flag("resolved_by"), 4, vec![1]),
        (r, Part::Option("resolved_by"), 5, vec![]),
        (c, Part::Member("members", ""), 1, vec![]),
        (c, Part::Member("members", "member\0\u{e9}"), 1, vec![]),
        (c, Part::Ordinal(0), 5, vec![]),
        (c, Part::Ordinal(1), 5, b"repeat".to_vec()),
        (c, Part::Ordinal(2), 5, b"repeat".to_vec()),
    ];
    for (object, right) in [
        ("", "preserve"),
        ("return\0\u{e9}", "return_trust"),
        ("residual", "distribute_residual"),
        ("finalize", "finalize_case"),
        ("advance", "advance_case"),
    ] {
        v.push((g, Part::Scope(object, right), 1, vec![]));
    }
    assert_eq!(v.len(), 43);
    v
}
// Administrative markers are separate from the43 original-content oracles.
// Qualified-root routing may evolve these; it must not weaken original fields.
fn marker_frames() -> Vec<Golden> {
    let c = Object::Case(CASE);
    let g = Object::Grant(CASE, GRANT);
    vec![
        (c, Part::Record, 1, vec![]),
        (Object::Estate(ESTATE), Part::Record, 1, vec![]),
        (g, Part::Record, 1, vec![]),
        (Object::Recovery(CASE, REQUEST), Part::Record, 1, vec![]),
        (c, Part::Collection("members"), 1, vec![]),
        (c, Part::Collection("grants"), 1, vec![]),
        (c, Part::Collection("requests"), 1, vec![]),
        (c, Part::Collection("stage_receipts"), 1, vec![]),
        (g, Part::Collection("scopes"), 1, vec![]),
    ]
}

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        leaves_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("organization-relative-oracle".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
fn produce_all(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> Rows {
    let mut rows = produce(store, memory, source);
    let mut remaining = 128;
    let mut bounds = limits();
    bounds.payload_bytes = 165;
    let counts = prepare_qualified_organization_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: bounds,
            cancelled: || false,
        },
        &mut remaining,
        4 << 20,
        &mut |row| {
            assert_eq!(row.namespace, Ns::Authorization);
            assert_eq!(row.mutation.before, None);
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        (counts.cases, counts.estate, counts.grants, counts.recovery),
        (1, 1, 1, 1)
    );
    assert_eq!(remaining, 74, "52 relative cells and two outer references");
    assert_eq!(rows.len(), 5);
    rows
}
#[test]
fn original_owner_fields_and_dynamic_identities_have_literal_cold_records() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let root = {
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            build(&store, &memory, &produce_all(&store, &memory, &source()))
                .canonical_bytes()
                .unwrap()
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _full = (class == MemoryClass::Recovery).then(|| {
                memory
                    .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap()
            });
            let root = StateRootV1::from_canonical_bytes(&root).unwrap();
            let roots = roots(&root);
            let state = store.state_session(physical(), class).unwrap();
            let values = store.value_session(physical());
            let mut io = logical();
            let mut budget = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut io,
                },
                key_work: &mut budget,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let family_result = qualify_organization_family_v1(
                &roots,
                &mut read,
                &mut scan(),
                &mut scan(),
                &mut 1024,
                165,
            );
            if family_result.is_err() {
                eprintln!(
                    "family probe {compression:?}/{class:?}: state={:?} values={:?} logical_nodes={} logical_read={}",
                    state.remaining_io().unwrap(),
                    values.remaining_io().unwrap(),
                    read.values.io.nodes_remaining,
                    read.values.io.read_bytes_remaining
                );
            }
            let family = family_result.unwrap();
            eprintln!(
                "qualified rich family {compression:?}/{class:?}: state={:?}",
                state.remaining_io().unwrap()
            );
            for (owner, estate) in [
                (OrganizationRootKeyV1::Case(CASE), false),
                (OrganizationRootKeyV1::Estate(ESTATE), true),
            ] {
                let proof =
                    open_qualified_organization_record_v1(owner, &roots, &family, &mut read)
                        .unwrap()
                        .unwrap();
                // These52 byte-level oracles are independently encoded original
                // fields/content; only the new relative root addressing changes.
                for (object, part, codec, payload) in golden().into_iter().chain(marker_frames()) {
                    if matches!(object, Object::Estate(_)) != estate {
                        continue;
                    }
                    // Independent literal queries have their own unchanged
                    // physical allowance; bootstrap above is one whole scope.
                    let oracle_state = store.state_session(physical(), class).unwrap();
                    assert!(
                        root.get(literal_outer_key(object).as_bytes(), &oracle_state)
                            .unwrap()
                            .is_some(),
                        "original outer ID must remain distinct from inner id"
                    );
                    let target = key(object, part);
                    let value = proof
                        .root()
                        .get(target.as_bytes(), &oracle_state)
                        .unwrap()
                        .unwrap();
                    let _scratch = memory.reserve(8192, class).unwrap();
                    let loaded = read.values.load_root(&value).unwrap();
                    let mut bytes = Vec::with_capacity(8192);
                    read.values
                        .read_range(&loaded, 0, loaded.0.logical_len, &mut bytes, || false)
                        .unwrap();
                    assert_eq!(bytes, frame(object, part, codec, &payload));
                }
                typed_fields::exercise(owner, &proof, &store, &memory, class);
                if !estate {
                    let mut point = WorldFieldReadContextV1 {
                        root: proof.root(),
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut *read.values.io,
                        },
                        key_work: &mut *read.key_work,
                        semantic_bytes: 1024,
                        record_bytes: 8192,
                        take_state_error: || state.take_read_error().map(state_error),
                        cancelled: || false,
                    };
                    for scope in grant().scopes {
                        assert_eq!(
                            read_qualified_organization_set_v1(
                                OrganizationSetRequestV1 {
                                    case: CASE,
                                    key: OrganizationRelativeSetKeyV1::Scope {
                                        grant: GRANT,
                                        scope: &scope
                                    },
                                    qualification: &proof,
                                },
                                &mut point
                            )
                            .unwrap(),
                            OrganizationSetReadV1::Present
                        );
                    }
                    for (ordinal, expected) in [(0, ""), (1, "repeat"), (2, "repeat")] {
                        let _scratch = memory.reserve(256, class).unwrap();
                        let mut bytes = Vec::with_capacity(256);
                        assert_eq!(
                            read_qualified_organization_receipt_v1(
                                CASE,
                                &proof,
                                ordinal,
                                Some(3),
                                &mut point,
                                128,
                                &mut bytes
                            )
                            .unwrap(),
                            WorldRuleOrganizationReceiptValueV1::Value {
                                length: 3,
                                utf8_bytes: expected.len() as u64
                            }
                        );
                        assert_eq!(bytes, expected.as_bytes());
                    }
                }
            }
        }
    }
}

#[path = "support/incremental_world_rule_organization_relative_bad_roots.rs"]
mod relative_bad_roots;
#[test]
fn nested_markers_and_required_columns_are_mandatory_in_complete_raw_scan() {
    relative_bad_roots::exercise();
}

#[path = "support/incremental_world_rule_organization_relative_transitions.rs"]
mod relative_transitions;
#[test]
fn qualified_scope_changes_and_closed_deletes_have_independent_receiver_and_cold_successors() {
    relative_transitions::exercise();
}

#[path = "support/incremental_world_rule_organization_relative_failures.rs"]
mod relative_failures;
#[test]
fn qualified_point_failures_and_final_noop_cancellation_poison_successful_prefix() {
    relative_failures::exercise();
}

#[path = "support/incremental_world_rule_organization_relative_genesis_failures.rs"]
mod relative_genesis_failures;
#[test]
fn qualified_genesis_failures_discard_the_combined_prefix() {
    relative_genesis_failures::exercise();
}

#[path = "support/incremental_world_rule_organization_relative_fields.rs"]
mod typed_fields;
