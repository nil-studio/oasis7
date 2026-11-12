//! NEW unhooked empty/nonempty real owner-GEN page regression, not WorldReady.
use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, Writer, io, source_admission};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_state::{StateDeltaV1, StateScanContext, StateScanError, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use std::collections::BTreeMap;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> DiplomacyIndexWorkV1 {
    DiplomacyIndexWorkV1 {
        cells_remaining: 100000,
        comparison_bytes_remaining: 1 << 26,
        max_rows: 32,
        max_members: 32,
        max_text_bytes: 4096,
        max_pending_rows: 32,
        pending_entries_remaining: 100000,
    }
}
fn record(creator: &str, status: EconomicContractStatus) -> EconomicContractState {
    EconomicContractState {
        contract_id: "embedded-contract-differs-from-each-outer".into(),
        creator_agent_id: creator.into(),
        counterparty_agent_id: "counterparty".into(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
        settlement_amount: 10,
        reputation_stake: 1,
        expires_at: 100,
        description: "original contract description".into(),
        status,
        accepted_at: None,
        settled_at: None,
        settlement_success: None,
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: None,
    }
}
#[test]
fn actual_contract_gen_metadata_and_creator_status_index_empty_and_nonempty() {
    let _source = source_admission();
    for contracts in [
        BTreeMap::new(),
        BTreeMap::from([
            (
                "outer-open".to_owned(),
                record("alice", EconomicContractStatus::Open),
            ),
            (
                "outer-accepted".into(),
                record("alice", EconomicContractStatus::Accepted),
            ),
            (
                "outer-settled".into(),
                record("alice", EconomicContractStatus::Settled),
            ),
            (
                "outer-other".into(),
                record("bob", EconomicContractStatus::Open),
            ),
        ]),
    ] {
        let memory = MemoryBudget::new(512 << 20, 64 << 20).unwrap();
        let pages = Pages::with_limit(&memory, 32768);
        let _phase = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
        let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::new();
        let mut writer = Writer(&pages);
        let mut wi = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 26,
        };
        let mut output = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut wi,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };
        for field in DiplomacyFieldV1::ALL {
            let row = crate::incremental_field_presence::write_field_metadata_v1(
                field.descriptor().unwrap(),
                &mut crate::incremental_field_presence::PresenceRecordWriteContext {
                    sink: &mut *output.sink,
                    memory: &memory,
                    io: &mut *output.io,
                    key_work: &mut *output.key_work,
                    limits: crate::incremental_field_presence::PresenceRecordLimitsV1 {
                        semantic_bytes: 1 << 20,
                        record_bytes: 2 << 20,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            crate::incremental_field_write::emit(
                row.field_key,
                row.value_root,
                &mut output,
                &mut |row| {
                    rows.push(row);
                    Ok(())
                },
            )
            .unwrap();
        }
        let mut cells = 100000;
        for (outer, contract) in &contracts {
            let body = genesis::body(
                DiplomacySourceV1::Contract(contract),
                &mut output,
                &mut cells,
                8 << 20,
            )
            .unwrap();
            let row = write_diplomacy_root_reference_v1(
                DiplomacyFieldV1::EconomicContracts,
                outer,
                &body,
                &mut output,
            )
            .unwrap();
            crate::incremental_field_write::emit(
                row.field_key,
                row.value_root,
                &mut output,
                &mut |row| {
                    rows.push(row);
                    Ok(())
                },
            )
            .unwrap();
        }
        drop(output);
        let mut roots = RootSetV1::empty("contract-creator-index".into(), [71; 32]).unwrap();
        let mut mutations: Vec<StateWriteV1> =
            rows.iter().map(|row| row.mutation.clone()).collect();
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            mutations,
            &pages,
            8 << 20,
        )
        .unwrap();
        for node in &delta.new_nodes {
            Writer(&pages)
                .put_node(
                    *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        *roots
            .state
            .iter_mut()
            .find(|r| r.namespace == StateNamespaceV1::Business)
            .unwrap() = delta.after.clone();
        let mut ri = io();
        let mut rk = SemanticKeyBudget {
            bytes_remaining: 1 << 26,
        };
        let mut read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut ri,
            },
            key_work: &mut rk,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let qualified = qualify_diplomacy_family_v1(
            &roots,
            &mut read,
            &mut scan(),
            &mut scan(),
            &mut 100000,
            1 << 20,
        )
        .unwrap();
        assert_eq!(qualified.record_counts()[3], contracts.len() as u64);
        let mut index = ContractCreatorIndexBuilderV1::new(&memory, MemoryClass::Ordinary).unwrap();
        let mut budget = work();
        let result = StateScanContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut scan(),
        }
        .visit_entries(
            roots.state_root(StateNamespaceV1::Business),
            |entry| {
                // Original outer recognizer authenticates and filters field Metadata. It
                // never forwards metadata to an Entry-only typed column reader.
                if let Some((field, _, child)) =
                    crate::incremental_diplomacy::family::outer(entry, &mut read)?
                {
                    if field == DiplomacyFieldV1::EconomicContracts {
                        let row = project_contract_row(entry, &child, &mut read, &mut budget)?;
                        index.put(row, &mut budget, || false)?;
                    }
                }
                Ok::<_, ValueTreeError>(())
            },
            || false,
        );
        assert!(
            result.is_ok(),
            "original GEN metadata/entry projection failed"
        );
        let index = index.finish().unwrap();
        assert_eq!(index.len(), contracts.len());
        let expected = contracts
            .values()
            .filter(|c| {
                c.creator_agent_id == "alice"
                    && matches!(
                        c.status,
                        EconomicContractStatus::Open | EconomicContractStatus::Accepted
                    )
            })
            .count();
        assert_eq!(
            index
                .active_for_creator("alice", &memory, MemoryClass::Ordinary, &mut budget, || {
                    false
                })
                .unwrap(),
            expected
        );
        if !contracts.is_empty() {
            let mut changed = index
                .copy_for_changes(&memory, MemoryClass::Ordinary, &mut budget, || false)
                .unwrap();
            let original = &index
                .data
                .rows
                .iter()
                .find(|row| row.outer.as_str() == "outer-open")
                .unwrap()
                .key;
            changed.remove(original, &mut budget, || false).unwrap();
            let changed = changed.finish().unwrap();
            assert_eq!(
                changed
                    .active_for_creator(
                        "alice",
                        &memory,
                        MemoryClass::Ordinary,
                        &mut budget,
                        || false
                    )
                    .unwrap(),
                1
            );
            assert_eq!(
                changed
                    .active_for_creator("bob", &memory, MemoryClass::Ordinary, &mut budget, || {
                        false
                    })
                    .unwrap(),
                1
            );
        }
    }
}
