use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
        TransitionVerifier,
    },
    incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn fresh_creates_ordered_edits_closed_delete_and_receiver() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("transition"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let before = build(
        &store,
        &memory,
        &produce(&store, &memory, &WorldState::default()),
    );
    let parent = ExecutionParent {
        roots: roots(&before),
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
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = fixed!(&before, &state, &values, &memory, &mut io, &mut keys);
    let family = qualify_diplomacy_family_v1(
        &parent.roots,
        &mut read,
        &mut scan(),
        &mut scan(),
        &mut 8192,
        165,
    )
    .unwrap();
    assert_eq!(family.record_counts(), [0; 4]);
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
    let mut overlay_io = logical();
    let mut overlay_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut overlay_io,
        },
        key_work: &mut overlay_keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut writer = store.value_session(physical());
    let mut write_io = logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: limits(),
        cancelled: || false,
    };
    let mut input = source();
    input.alliances.get_mut(KEY).unwrap().members.clear();
    input
        .wars
        .get_mut(KEY)
        .unwrap()
        .participant_outcomes
        .clear();
    let sources = [
        DiplomacySourceV1::Alliance(&input.alliances[KEY]),
        DiplomacySourceV1::War(&input.wars[KEY]),
        DiplomacySourceV1::Crisis(&input.crises[KEY]),
        DiplomacySourceV1::Contract(&input.economic_contracts[KEY]),
    ];
    let mut proofs = Vec::new();
    for source in sources {
        let guard = overlay.transaction.guard();
        proofs.push(
            create_qualified_diplomacy_empty_v1(
                DiplomacyEmptyCreateV1 {
                    id: KEY,
                    source,
                    family: &family,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
                &mut 128,
            )
            .unwrap(),
        );
    }
    let old = WarParticipantOutcome {
        agent_id: "old".into(),
        electricity_delta: i64::MIN,
        data_delta: i64::MAX,
        reputation_delta: 0,
    };
    let new = WarParticipantOutcome {
        agent_id: String::new(),
        electricity_delta: i64::MAX,
        data_delta: i64::MIN,
        reputation_delta: -1,
    };
    for (index, field, original, replacement) in [
        (
            0,
            DiplomacyFieldV1::Alliances,
            DiplomacyMemberV1::Alliance("same\0é"),
            DiplomacyMemberV1::Alliance(""),
        ),
        (
            1,
            DiplomacyFieldV1::Wars,
            DiplomacyMemberV1::Outcome(&old),
            DiplomacyMemberV1::Outcome(&new),
        ),
    ] {
        for (length, ordinal, expected, after) in [
            (0, 0, None, Some(original)),
            (1, 1, None, Some(original)),
            (2, 0, Some(original), Some(replacement)),
            (2, 1, Some(original), None),
        ] {
            let guard = overlay.transaction.guard();
            proofs[index] = change_qualified_diplomacy_member_v1(
                DiplomacyMemberChangeV1 {
                    field,
                    id: KEY,
                    qualification: &proofs[index],
                    expected_length: length,
                    ordinal,
                    expected,
                    after,
                    max_current_payload_bytes: 165,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
        }
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut point = fixed!(
            proofs[index].root(),
            &state,
            &values,
            &memory,
            &mut io,
            &mut keys
        );
        assert_eq!(
            read_qualified_diplomacy_length_v1(field, KEY, &proofs[index], &mut point).unwrap(),
            1
        );
        let column = if index == 0 {
            DiplomacyMemberColumnV1::AllianceMember
        } else {
            DiplomacyMemberColumnV1::OutcomeElectricityDelta
        };
        let mut text = Vec::new();
        let value = read_qualified_diplomacy_member_v1(
            DiplomacyMemberReadV1 {
                field,
                id: KEY,
                ordinal: 0,
                column,
                qualification: &proofs[index],
                max_payload_bytes: 165,
            },
            &mut point,
            &mut text,
        )
        .unwrap();
        assert_eq!(
            value,
            Some(if index == 0 {
                DiplomacyValueV1::Text { utf8_bytes: 0 }
            } else {
                DiplomacyValueV1::Signed(i64::MAX)
            })
        );
    }
    // Nonempty borrowed source collections must not replace authenticated members.
    let mut alliance_after = input.alliances[KEY].clone();
    alliance_after.alliance_id = "changed-inner-id".into();
    alliance_after.charter = "changed".into();
    alliance_after.members = vec!["unrelated-source".into(); 3];
    let mut war_after = input.wars[KEY].clone();
    war_after.active = true;
    war_after.intensity = 0;
    war_after.winner_alliance_id = None;
    war_after.participant_outcomes = vec![old.clone(); 3];
    let mut contract_after = input.economic_contracts[KEY].clone();
    contract_after.status = EconomicContractStatus::Open;
    contract_after.accepted_at = None;
    contract_after.settlement_notes = None;
    for (index, after) in [
        (0, DiplomacySourceV1::Alliance(&alliance_after)),
        (1, DiplomacySourceV1::War(&war_after)),
        (3, DiplomacySourceV1::Contract(&contract_after)),
    ] {
        let guard = overlay.transaction.guard();
        proofs[index] = change_qualified_diplomacy_scalars_v1(
            DiplomacyScalarsChangeV1 {
                id: KEY,
                qualification: &proofs[index],
                expected: sources[index],
                after,
                max_current_payload_bytes: 165,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        if index < 2 {
            let field = if index == 0 {
                DiplomacyFieldV1::Alliances
            } else {
                DiplomacyFieldV1::Wars
            };
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut point = fixed!(
                proofs[index].root(),
                &state,
                &values,
                &memory,
                &mut io,
                &mut keys
            );
            assert_eq!(
                read_qualified_diplomacy_length_v1(field, KEY, &proofs[index], &mut point).unwrap(),
                1
            );
        }
    }
    let guard = overlay.transaction.guard();
    delete_qualified_diplomacy_closed_v1(
        DiplomacyClosedDeleteV1 {
            id: KEY,
            qualification: &proofs[2],
            expected: sources[2],
            max_current_payload_bytes: 165,
        },
        &mut overlay,
        &guard,
        &live,
    )
    .unwrap();
    let guard = overlay.transaction.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    for node in &prepared.new_nodes {
        writer
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let after = prepared
        .record
        .after
        .state
        .iter()
        .find(|r| r.namespace == Ns::Business)
        .unwrap();
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = fixed!(after, &state, &values, &memory, &mut io, &mut keys);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    };
    let accepted = accept_diplomacy_transition_v1(
        &family,
        &verifier,
        &prepared.record,
        &mut ctx,
        &mut 100_000,
        165,
    )
    .unwrap();
    assert_eq!(accepted.record_counts(), [1, 1, 0, 1]);
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = fixed!(after, &state, &values, &memory, &mut io, &mut keys);
    assert_eq!(
        qualify_diplomacy_family_v1(
            &prepared.record.after,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165
        )
        .unwrap()
        .record_counts(),
        accepted.record_counts()
    );
}
