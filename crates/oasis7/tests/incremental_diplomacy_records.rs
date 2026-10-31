#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_agent_api::world_action::ResourceKind;
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
    AllianceState, CrisisState, CrisisStatus, EconomicContractFulfillmentKind,
    EconomicContractState, EconomicContractStatus, WarParticipantOutcome, WarState, WorldState,
    incremental_diplomacy::*, incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_000_000,
        ..fixture::physical()
    }
}
#[allow(dead_code)]
#[path = "support/incremental_diplomacy_fixture.rs"]
mod diplomacy_fixture;
use diplomacy_fixture::*;
#[path = "support/incremental_diplomacy_literal.rs"]
mod literal;
const KEY: &str = "outer\0é";
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        leaves_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("diplomacy".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Business)
        .unwrap() = root.clone();
    roots
}
macro_rules! fixed {
    ($root:expr,$state:expr,$values:expr,$memory:expr,$io:expr,$keys:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $state,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: MemoryClass::Ordinary,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}
fn source() -> WorldState {
    WorldState {
        alliances: [(
            KEY.into(),
            AllianceState {
                alliance_id: "internal-alliance".into(),
                members: vec!["same\0é".into(), "same\0é".into(), String::new()],
                charter: String::new(),
                formed_by_agent_id: "creator".into(),
                formed_at: u64::MAX,
            },
        )]
        .into(),
        wars: [(
            KEY.into(),
            WarState {
                war_id: "internal-war".into(),
                initiator_agent_id: "i".into(),
                aggressor_alliance_id: "a".into(),
                defender_alliance_id: "d".into(),
                objective: String::new(),
                intensity: u32::MAX,
                active: false,
                declared_mobilization_electricity_cost: i64::MIN,
                declared_mobilization_data_cost: i64::MAX,
                max_duration_ticks: u64::MAX,
                aggressor_score: -1,
                defender_score: 0,
                concluded_at: Some(0),
                winner_alliance_id: Some(String::new()),
                loser_alliance_id: None,
                settlement_summary: Some("summary".into()),
                participant_outcomes: vec![
                    WarParticipantOutcome {
                        agent_id: String::new(),
                        electricity_delta: i64::MIN,
                        data_delta: i64::MAX,
                        reputation_delta: 0,
                    },
                    WarParticipantOutcome {
                        agent_id: String::new(),
                        electricity_delta: i64::MIN,
                        data_delta: i64::MAX,
                        reputation_delta: 0,
                    },
                ],
                declared_at: 0,
            },
        )]
        .into(),
        crises: [(
            KEY.into(),
            CrisisState {
                crisis_id: "internal-crisis".into(),
                kind: String::new(),
                severity: u32::MAX,
                status: CrisisStatus::TimedOut,
                opened_at: 0,
                expires_at: u64::MAX,
                resolver_agent_id: Some(String::new()),
                strategy: None,
                success: Some(false),
                impact: i64::MIN,
                resolved_at: Some(0),
            },
        )]
        .into(),
        economic_contracts: [(
            KEY.into(),
            EconomicContractState {
                contract_id: "internal-contract".into(),
                creator_agent_id: "creator".into(),
                counterparty_agent_id: String::new(),
                fulfillment_kind: EconomicContractFulfillmentKind::Service,
                settlement_kind: ResourceKind::Data,
                settlement_amount: i64::MIN,
                reputation_stake: i64::MAX,
                expires_at: u64::MAX,
                description: String::new(),
                status: EconomicContractStatus::Expired,
                accepted_at: Some(0),
                settled_at: None,
                settlement_success: Some(false),
                transfer_amount: -1,
                tax_amount: 0,
                settlement_notes: Some(String::new()),
            },
        )]
        .into(),
        ..WorldState::default()
    }
}
#[test]
fn four_owner_genesis_root_only_cold_and_ordered_points() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("diplomacy");
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let original = source();
        let rows = produce(&store, &memory, &original);
        assert_eq!(rows.len(), 8);
        let root = build(&store, &memory, &rows);
        let canonical = roots(&root).canonical_bytes().unwrap();
        drop(original);
        drop(store);
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let recovered = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let root = recovered
            .state
            .iter()
            .find(|r| r.namespace == Ns::Business)
            .unwrap();
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = fixed!(root, &state, &values, &memory, &mut io, &mut keys);
        let family = qualify_diplomacy_family_v1(
            &recovered,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        );
        eprintln!(
            "cold {:?}: state={:?} values={:?} result={:?}",
            compression,
            state.remaining_io(),
            values.remaining_io(),
            family.as_ref().map(|q| q.record_counts())
        );
        let family = family.unwrap();
        assert_eq!(family.record_counts(), [1; 4]);
        for (field, column, expected, bytes) in [
            (
                DiplomacyFieldV1::Alliances,
                DiplomacyScalarV1::Alliance(AllianceScalarV1::AllianceId),
                Some(DiplomacyValueV1::Text { utf8_bytes: 17 }),
                b"internal-alliance".as_slice(),
            ),
            (
                DiplomacyFieldV1::Wars,
                DiplomacyScalarV1::War(WarScalarV1::WinnerAllianceId),
                Some(DiplomacyValueV1::Text { utf8_bytes: 0 }),
                b"".as_slice(),
            ),
            (
                DiplomacyFieldV1::Crises,
                DiplomacyScalarV1::Crisis(CrisisScalarV1::Success),
                Some(DiplomacyValueV1::Bool(false)),
                b"".as_slice(),
            ),
            (
                DiplomacyFieldV1::EconomicContracts,
                DiplomacyScalarV1::Contract(ContractScalarV1::Status),
                Some(DiplomacyValueV1::Text { utf8_bytes: 7 }),
                b"expired".as_slice(),
            ),
        ] {
            let proof =
                open_qualified_diplomacy_record_v1(field, KEY, &recovered, &family, &mut ctx)
                    .unwrap()
                    .unwrap();
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut point = fixed!(proof.root(), &state, &values, &memory, &mut io, &mut keys);
            literal::check(field, &mut point);
            let mut output = Vec::new();
            assert_eq!(
                read_qualified_diplomacy_scalar_v1(
                    KEY,
                    field,
                    column,
                    &proof,
                    &mut point,
                    165,
                    &mut output
                )
                .unwrap(),
                expected
            );
            assert_eq!(output, bytes);
            if matches!(field, DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars) {
                assert_eq!(
                    read_qualified_diplomacy_length_v1(field, KEY, &proof, &mut point).unwrap(),
                    if field == DiplomacyFieldV1::Alliances {
                        3
                    } else {
                        2
                    }
                );
                for index in 0..2 {
                    let column = if field == DiplomacyFieldV1::Alliances {
                        DiplomacyMemberColumnV1::AllianceMember
                    } else {
                        DiplomacyMemberColumnV1::OutcomeElectricityDelta
                    };
                    let mut output = Vec::new();
                    let actual = read_qualified_diplomacy_member_v1(
                        DiplomacyMemberReadV1 {
                            field,
                            id: KEY,
                            ordinal: index,
                            column,
                            qualification: &proof,
                            max_payload_bytes: 165,
                        },
                        &mut point,
                        &mut output,
                    )
                    .unwrap();
                    if field == DiplomacyFieldV1::Wars {
                        assert_eq!(actual, Some(DiplomacyValueV1::Signed(i64::MIN)));
                    } else {
                        assert_eq!(output, "same\0é".as_bytes());
                    }
                }
            }
        }
        eprintln!(
            "complete {:?}: state={:?} values={:?}",
            compression,
            state.remaining_io(),
            values.remaining_io()
        );
    }
}

#[path = "support/incremental_diplomacy_transitions.rs"]
mod transitions;

#[path = "support/incremental_diplomacy_bad_roots.rs"]
mod bad_roots;

#[path = "support/incremental_diplomacy_pages.rs"]
mod pages;

#[path = "support/incremental_diplomacy_transaction_faults.rs"]
mod transaction_faults;

#[path = "support/incremental_diplomacy_receiver.rs"]
mod receiver_faults;

#[path = "support/incremental_diplomacy_cold_delta.rs"]
mod cold_delta;

#[path = "support/incremental_diplomacy_point_scalars.rs"]
mod point_scalars;
