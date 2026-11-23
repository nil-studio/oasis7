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
fn typed_point_options_and_every_closed_enum_label_round_trip() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("point"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let before = build(&store, &memory, &produce(&store, &memory, &source()));
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
    assert_eq!(family.record_counts(), [1; 4]);
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
    let mut proofs = Vec::new();
    for field in DiplomacyFieldV1::ALL {
        proofs.push(
            open_qualified_diplomacy_record_v1(field, KEY, &parent.roots, &family, &mut read)
                .unwrap()
                .unwrap(),
        );
    }

    use DiplomacyScalarV1 as Col;
    use DiplomacyScalarValueV1 as V;
    let changes = [
        (
            0,
            Col::Alliance(AllianceScalarV1::Charter),
            Some(V::Text("")),
            Some(V::Text("changed")),
        ),
        (
            1,
            Col::War(WarScalarV1::Intensity),
            Some(V::U32(u32::MAX)),
            Some(V::U32(0)),
        ),
        (1, Col::War(WarScalarV1::ConcludedAt), Some(V::U64(0)), None),
        (1, Col::War(WarScalarV1::ConcludedAt), None, Some(V::U64(0))),
        (
            1,
            Col::War(WarScalarV1::WinnerAllianceId),
            Some(V::Text("")),
            None,
        ),
        (
            1,
            Col::War(WarScalarV1::WinnerAllianceId),
            None,
            Some(V::Text("")),
        ),
        (
            2,
            Col::Crisis(CrisisScalarV1::Success),
            Some(V::Bool(false)),
            None,
        ),
        (
            2,
            Col::Crisis(CrisisScalarV1::Success),
            None,
            Some(V::Bool(false)),
        ),
        (
            2,
            Col::Crisis(CrisisScalarV1::Success),
            Some(V::Bool(false)),
            Some(V::Bool(true)),
        ),
        (
            2,
            Col::Crisis(CrisisScalarV1::Status),
            Some(V::CrisisStatus(CrisisStatus::TimedOut)),
            Some(V::CrisisStatus(CrisisStatus::Active)),
        ),
        (
            2,
            Col::Crisis(CrisisScalarV1::Status),
            Some(V::CrisisStatus(CrisisStatus::Active)),
            Some(V::CrisisStatus(CrisisStatus::Resolved)),
        ),
        (
            2,
            Col::Crisis(CrisisScalarV1::Status),
            Some(V::CrisisStatus(CrisisStatus::Resolved)),
            Some(V::CrisisStatus(CrisisStatus::TimedOut)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::Status),
            Some(V::ContractStatus(EconomicContractStatus::Expired)),
            Some(V::ContractStatus(EconomicContractStatus::Open)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::Status),
            Some(V::ContractStatus(EconomicContractStatus::Open)),
            Some(V::ContractStatus(EconomicContractStatus::Accepted)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::Status),
            Some(V::ContractStatus(EconomicContractStatus::Accepted)),
            Some(V::ContractStatus(EconomicContractStatus::Settled)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::Status),
            Some(V::ContractStatus(EconomicContractStatus::Settled)),
            Some(V::ContractStatus(EconomicContractStatus::Expired)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::FulfillmentKind),
            Some(V::FulfillmentKind(EconomicContractFulfillmentKind::Service)),
            Some(V::FulfillmentKind(
                EconomicContractFulfillmentKind::AtomicExchange,
            )),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::FulfillmentKind),
            Some(V::FulfillmentKind(
                EconomicContractFulfillmentKind::AtomicExchange,
            )),
            Some(V::FulfillmentKind(EconomicContractFulfillmentKind::Service)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::SettlementKind),
            Some(V::Resource(ResourceKind::Data)),
            Some(V::Resource(ResourceKind::Electricity)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::SettlementKind),
            Some(V::Resource(ResourceKind::Electricity)),
            Some(V::Resource(ResourceKind::Data)),
        ),
        (
            3,
            Col::Contract(ContractScalarV1::SettlementNotes),
            Some(V::Text("")),
            None,
        ),
        (
            3,
            Col::Contract(ContractScalarV1::SettlementNotes),
            None,
            Some(V::Text("")),
        ),
    ];
    for (index, column, expected, after) in changes {
        let field = DiplomacyFieldV1::ALL[index];
        let guard = overlay.transaction.guard();
        proofs[index] = change_qualified_diplomacy_scalar_v1(
            DiplomacyScalarChangeV1 {
                field,
                id: KEY,
                column,
                qualification: &proofs[index],
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
        let mut text = Vec::new();
        let value = read_qualified_diplomacy_scalar_v1(
            KEY,
            field,
            column,
            &proofs[index],
            &mut point,
            165,
            &mut text,
        )
        .unwrap();
        let (wanted, bytes) = match after {
            None => (None, ""),
            Some(V::Text(v)) => (
                Some(DiplomacyValueV1::Text {
                    utf8_bytes: v.len() as u64,
                }),
                v,
            ),
            Some(V::U64(v)) => (Some(DiplomacyValueV1::Unsigned(v)), ""),
            Some(V::U32(v)) => (Some(DiplomacyValueV1::Unsigned(u64::from(v))), ""),
            Some(V::Bool(v)) => (Some(DiplomacyValueV1::Bool(v)), ""),
            Some(V::CrisisStatus(v)) => {
                let text = match v {
                    CrisisStatus::Active => "active",
                    CrisisStatus::Resolved => "resolved",
                    CrisisStatus::TimedOut => "timed_out",
                };
                (
                    Some(DiplomacyValueV1::Text {
                        utf8_bytes: text.len() as u64,
                    }),
                    text,
                )
            }
            Some(V::ContractStatus(v)) => {
                let text = match v {
                    EconomicContractStatus::Open => "open",
                    EconomicContractStatus::Accepted => "accepted",
                    EconomicContractStatus::Settled => "settled",
                    EconomicContractStatus::Expired => "expired",
                };
                (
                    Some(DiplomacyValueV1::Text {
                        utf8_bytes: text.len() as u64,
                    }),
                    text,
                )
            }
            Some(V::FulfillmentKind(v)) => {
                let text = match v {
                    EconomicContractFulfillmentKind::AtomicExchange => "atomic_exchange",
                    EconomicContractFulfillmentKind::Service => "service",
                };
                (
                    Some(DiplomacyValueV1::Text {
                        utf8_bytes: text.len() as u64,
                    }),
                    text,
                )
            }
            Some(V::Resource(v)) => {
                let text = match v {
                    ResourceKind::Electricity => "electricity",
                    ResourceKind::Data => "data",
                };
                (
                    Some(DiplomacyValueV1::Text {
                        utf8_bytes: text.len() as u64,
                    }),
                    text,
                )
            }
            Some(V::I64(v)) => (Some(DiplomacyValueV1::Signed(v)), ""),
        };
        assert_eq!(value, wanted);
        assert_eq!(text, bytes.as_bytes());
    }
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
    // A separate bounded receiver read session checks the final net transition.
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
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
    assert_eq!(
        accept_diplomacy_transition_v1(
            &family,
            &verifier,
            &prepared.record,
            &mut ctx,
            &mut 100_000,
            165
        )
        .unwrap()
        .record_counts(),
        [1; 4]
    );
}
