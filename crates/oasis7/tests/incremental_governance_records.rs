#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_governance_records::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, physical};
fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    io.reads_remaining = 2_000_000;
    io
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn source() -> WorldState {
    let mut world = WorldState::default();
    world.governance_identity_profiles.insert(" outer identity ".into(),serde_json::from_value(serde_json::json!({"agent_id":" different inner agent ","stake_locked":u64::MAX,"warmup_until_tick":0,"status":"expelled","slash_count":u32::MAX,"updated_at":u64::MAX})).unwrap());
    world.governance_proposals.insert(" outer proposal ".into(),serde_json::from_value(serde_json::json!({"proposal_key":"different embedded proposal","proposer_agent_id":" raw proposer ","title":"","description":" raw description ","options":["z","","z"],"voting_window_ticks":u64::MAX,"quorum_weight":0,"pass_threshold_bps":u16::MAX,"opened_at":0,"closes_at":u64::MAX,"status":"passed","finalized_at":null,"winning_option":"","winning_weight":u64::MAX,"total_weight_at_finalize":0,"snapshot_at_tick":0,"vote_weight_snapshot":{" raw snapshot key ":{"agent_id":"different snapshot inner","reputation_score":i64::MIN,"stake_locked":u64::MAX,"status":"frozen","vote_weight_cap":u32::MAX}}})).unwrap());
    world.governance_votes.insert("".into(),serde_json::from_value(serde_json::json!({"proposal_key":"embedded empty-key proposal","votes_by_agent":{" raw ballot key ":{"option":" original vote ","weight":u32::MAX,"voted_at":u64::MAX}},"tallies":{"":u64::MAX," original vote ":0},"total_weight":0,"last_updated_at":u64::MAX})).unwrap());
    world
}
fn work() -> GovernanceWorkBudgetV1 {
    GovernanceWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        },
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> RootSetV1 {
    seed_with_physical(store, memory, source, physical_scope())
}
fn seed_with_physical(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    source: &WorldState,
    scope: PageIoBudget,
) -> RootSetV1 {
    let mut values = store.value_session(scope);
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(32);
    let mut remaining = 100_000;
    let count = prepare_governance_records_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut remaining,
        &mut work(),
        &mut |row| {
            assert!(rows.len() < 32);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let expected = source.governance_identity_profiles.len()
        + source.governance_proposals.len()
        + source.governance_votes.len();
    assert_eq!(count, expected as u64);
    assert_eq!(rows.len(), 6 + expected);
    let mut roots = RootSetV1::empty("governance-source".into(), [0x41; 32]).unwrap();
    for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == ns)
            .map(|r| StateWriteV1 {
                key: r.mutation.key.clone(),
                before: None,
                after: r.mutation.after,
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &state, 8 << 20).unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        let index = roots.state.iter().position(|r| r.namespace == ns).unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots
}

fn qualify(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) -> ReservedValue<GovernanceFamilyQualificationV1> {
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    let root = &roots.state[0];
    qualify_governance_family_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut work().scan,
        &mut work().scan,
        &mut 1000,
        1 << 20,
    )
    .unwrap()
}

#[test]
fn original_governance_three_families_cold_recover_all_original_member_shapes() {
    use GovernanceBallotColumnV1 as B;
    use GovernanceCellV1 as C;
    use GovernanceCollectionV1 as G;
    use GovernanceColumnV1 as F;
    use GovernanceRecordKindV1 as K;
    use GovernanceSnapshotColumnV1 as S;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("governance-pages");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source())
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let _caller = memory.reserve(16 << 10, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let family = qualify(&store, &memory, &roots, class);
            assert_eq!(family.record_counts(), [1, 1, 1]);
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            let mut context = WorldFieldReadContextV1 {
                root: roots
                    .state
                    .iter()
                    .find(|r| r.namespace == StateNamespaceV1::Authorization)
                    .unwrap(),
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 4096,
                record_bytes: 2 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            for (kind, id, cell, expected, text) in [
                (
                    K::Identity,
                    " outer identity ",
                    C::Column(F::AgentId),
                    GovernanceValueV1::Text {
                        utf8_bytes: " different inner agent ".len() as u64,
                    },
                    " different inner agent ",
                ),
                (
                    K::Identity,
                    " outer identity ",
                    C::Column(F::SlashCount),
                    GovernanceValueV1::Unsigned(u32::MAX as u64),
                    "",
                ),
                (
                    K::Identity,
                    " outer identity ",
                    C::Column(F::IdentityStatus),
                    GovernanceValueV1::Unsigned(2),
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Column(F::PassThresholdBps),
                    GovernanceValueV1::Unsigned(u16::MAX as u64),
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Column(F::FinalizedAt),
                    GovernanceValueV1::Unit,
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Column(F::WinningOption),
                    GovernanceValueV1::Text { utf8_bytes: 0 },
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Count(G::Options),
                    GovernanceValueV1::Unsigned(3),
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Option(0),
                    GovernanceValueV1::Text { utf8_bytes: 1 },
                    "z",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Option(1),
                    GovernanceValueV1::Text { utf8_bytes: 0 },
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Option(2),
                    GovernanceValueV1::Text { utf8_bytes: 1 },
                    "z",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Snapshot(" raw snapshot key ", S::AgentId),
                    GovernanceValueV1::Text {
                        utf8_bytes: "different snapshot inner".len() as u64,
                    },
                    "different snapshot inner",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Snapshot(" raw snapshot key ", S::ReputationScore),
                    GovernanceValueV1::Signed(i64::MIN),
                    "",
                ),
                (
                    K::Proposal,
                    " outer proposal ",
                    C::Snapshot(" raw snapshot key ", S::VoteWeightCap),
                    GovernanceValueV1::Unsigned(u32::MAX as u64),
                    "",
                ),
                (
                    K::Vote,
                    "",
                    C::Ballot(" raw ballot key ", B::Option),
                    GovernanceValueV1::Text {
                        utf8_bytes: " original vote ".len() as u64,
                    },
                    " original vote ",
                ),
                (
                    K::Vote,
                    "",
                    C::Ballot(" raw ballot key ", B::Weight),
                    GovernanceValueV1::Unsigned(u32::MAX as u64),
                    "",
                ),
                (
                    K::Vote,
                    "",
                    C::Tally(""),
                    GovernanceValueV1::Unsigned(u64::MAX),
                    "",
                ),
            ] {
                let key = GovernanceRecordKeyV1 { kind, id };
                let proof = open_qualified_governance_record_v1(key, &roots, &family, &mut context)
                    .unwrap()
                    .unwrap();
                let _output = memory.reserve(text.len() + 1024, class).unwrap();
                let mut output = Vec::with_capacity(text.len());
                let value = read_qualified_governance_cell_v1(
                    GovernanceCellRequestV1 {
                        key,
                        cell,
                        qualification: &proof,
                    },
                    &mut context,
                    1 << 20,
                    &mut output,
                )
                .unwrap();
                assert_eq!(value, expected);
                assert_eq!(output, text.as_bytes());
            }
        }
    }
}
fn state_error(e: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match e {
        E::Memory(e) => ValueTreeError::Memory(e),
        E::ResourceLimited(e) => ValueTreeError::ResourceLimited(e),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(e) => ValueTreeError::Io(e),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "governance missing physical page",
        )),
        E::Corrupt(e) => ValueTreeError::Invalid(e),
        E::UnsupportedPlatform => ValueTreeError::Invalid("governance platform"),
    }
}

#[path = "support/incremental_governance_overlay.rs"]
mod overlay;

#[path = "support/incremental_governance_points.rs"]
mod points;

#[path = "support/incremental_governance_pages.rs"]
mod pages;

#[path = "support/incremental_governance_malformed.rs"]
mod malformed;

#[path = "support/incremental_governance_carriers.rs"]
mod carriers;

#[path = "support/incremental_governance_put.rs"]
mod put;

#[path = "support/incremental_governance_missing.rs"]
mod missing;

#[path = "support/incremental_governance_scaling.rs"]
mod scaling;

#[path = "support/incremental_governance_empty.rs"]
mod empty;
