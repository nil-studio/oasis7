use super::*;
use oasis7_formal_protocol::incremental_value::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::Arc,
};
#[derive(Default)]
struct Pages(BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        _: &MemoryBudget,
        _: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .0
            .get(id)
            .ok_or(ValueTreeError::Invalid("test page missing"))?;
        if value.len() > max {
            return Err(ValueTreeError::Invalid("test page bound"));
        }
        Ok(value.clone())
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 4 << 20,
        write_bytes_remaining: 4 << 20,
    }
}
fn tree(value: AgentScalarRefV1<'_>, memory: &MemoryBudget, pages: &mut Pages) -> [u8; 32] {
    let length = value.encoded_len().unwrap();
    let _charge = memory
        .reserve(length as usize, MemoryClass::Ordinary)
        .unwrap();
    let mut bytes = Vec::with_capacity(length as usize);
    value.write_to(&mut bytes).unwrap();
    write_value(
        &mut bytes.as_slice(),
        length,
        pages,
        memory,
        &mut io(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap()
}
struct Choppy {
    bytes: Vec<u8>,
    interrupted: bool,
}
impl Write for Choppy {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = bytes.len().min(1);
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn original_scalar_wire_streams_all_kinds_and_utf8_without_aggregation() {
    let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
    let _metadata = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
    let text = "α界🙂".repeat(700);
    let mut pages = Pages::default();
    use AgentFieldV1 as F;
    use AgentScalarRefV1 as V;
    use AgentSubtreeValueV1 as O;
    for (field, value, expected, output) in [
        (
            F::IntentSummary,
            V::Text(&text),
            O::Text {
                utf8_bytes: text.len() as u64,
            },
            text.as_str(),
        ),
        (F::SlotType, V::U8(5), O::U8(5), ""),
        (F::SlotCapacity, V::U16(u16::MAX), O::U16(u16::MAX), ""),
        (F::IntentSchema, V::U32(u32::MAX), O::U32(u32::MAX), ""),
        (F::IntentEventSeq, V::U64(u64::MAX), O::U64(u64::MAX), ""),
        (F::PositionX, V::I64(i64::MIN), O::I64(i64::MIN), ""),
        (F::SlotLocked, V::Bool(true), O::Bool(true), ""),
        (
            F::IntentTarget,
            V::OptionalText(None),
            O::OptionalText(None),
            "",
        ),
        (
            F::IntentTarget,
            V::OptionalText(Some("")),
            O::OptionalText(Some(0)),
            "",
        ),
        (
            F::ActivityOperationId,
            V::OptionalU64(None),
            O::OptionalU64(None),
            "",
        ),
        (
            F::ActivityOperationId,
            V::OptionalU64(Some(0)),
            O::OptionalU64(Some(0)),
            "",
        ),
    ] {
        let id = tree(value, &memory, &mut pages);
        let _output = memory.reserve(output.len(), MemoryClass::Ordinary).unwrap();
        let mut sink = Choppy {
            bytes: Vec::with_capacity(output.len()),
            interrupted: false,
        };
        let got = read_agent_scalar_tree_v1(
            field,
            &id,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            1 << 20,
            &mut sink,
            &mut || false,
        )
        .unwrap();
        assert_eq!(got, expected);
        assert_eq!(sink.bytes, output.as_bytes());
    }
    let id = tree(V::U8(6), &memory, &mut pages);
    assert!(matches!(
        read_agent_scalar_tree_v1(
            F::SlotType,
            &id,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io()
            },
            64,
            &mut io::sink(),
            &mut || false
        ),
        Err(ValueTreeError::Invalid(_))
    ));
}
#[test]
fn original_key_parsers_preserve_identity_and_reject_wrong_namespace_and_versions() {
    use crate::{incremental_agent_body::AgentBodyKeyV1, incremental_event_fields::MailboxKeyV1};
    use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as N;
    let id = " 原 agent ";
    let body = AgentBodyKeyV1::new(id).unwrap();
    assert_eq!(AgentBodyKeyV1::from_bytes(body.as_bytes()).unwrap(), id);
    assert!(
        matches!(parse_agent_subtree_key_v1(N::Business,body.as_bytes()).unwrap(),Some(AgentSubtreeKeyV1::Body {agent}) if agent==id)
    );
    assert!(parse_agent_subtree_key_v1(N::Authorization, body.as_bytes()).is_err());
    for ordinal in [None, Some(0), Some(u64::MAX)] {
        let key = MailboxKeyV1::new(id, ordinal).unwrap();
        assert_eq!(
            MailboxKeyV1::from_bytes(key.as_bytes()).unwrap(),
            (id, ordinal)
        );
        assert!(
            matches!(parse_agent_subtree_key_v1(N::Scheduler,key.as_bytes()).unwrap(),Some(AgentSubtreeKeyV1::Mailbox {agent,ordinal:n}) if agent==id && n==ordinal)
        );
    }
    for &field in AgentFieldV1::ALL {
        let ordinal = if field == AgentFieldV1::ResourceAmount {
            1
        } else if field.indexed() {
            7
        } else {
            0
        };
        let key = AgentFieldKeyV1::new(id, field, ordinal).unwrap();
        assert!(
            matches!(parse_agent_subtree_key_v1(field.namespace(),key.as_bytes()).unwrap(),Some(AgentSubtreeKeyV1::Field {field:f,ordinal:n,..}) if f==field && n==ordinal)
        );
    }
    let mut bad = body.as_bytes().to_vec();
    bad[crate::incremental_agent_body::AGENT_BODY_KEY_DOMAIN_V1.len() - 2] = b'9';
    assert!(parse_agent_subtree_key_v1(N::Business, &bad).is_err());
}

#[path = "incremental_agent_subtree_fixture.rs"]
mod fixture;
#[test]
fn complete_structure_keeps_per_agent_counts_and_does_not_certify_history() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _metadata = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let (mut pages, roots) = fixture::seed(&memory);
    let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
    assert_eq!(proof.agent_count(), 2);
    assert_eq!(proof.mailbox_reference_count(), 0);
    proof.validate_binding(&roots).unwrap();
    let state = fixture::State::new(&pages, &memory);
    let got = read_agent_subtree_scalar_raw_v1(
        &roots,
        AgentSubtreeScalarRequestV1 {
            agent: "agent β",
            field: AgentFieldV1::IntentEventSeq,
            ordinal: 0,
        },
        &mut crate::incremental_field_read::WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || state.take(),
            cancelled: || false,
        },
        128,
        &mut io::sink(),
    )
    .unwrap();
    assert_eq!(got, Some(AgentSubtreeValueV1::U64(u64::MAX)));
    let bad = fixture::false_cross_agent_counts(&roots, &mut pages, &memory);
    assert!(matches!(
        fixture::qualify(&bad, &pages, &memory),
        Err(ValueTreeError::Invalid(_))
    ));
}

#[test]
fn mailbox_reference_still_has_the_original_literal_bytes() {
    use crate::incremental_event_fields::{
        MAILBOX_EVENT_REFERENCE_BYTES, mailbox_event_id_v1, mailbox_event_ref_v1,
    };
    let original = [0x7f; 32];
    let mut expected = b"oasis7.iws.mailbox-event.v1\0".to_vec();
    expected.extend_from_slice(&original);
    let encoded = mailbox_event_ref_v1(original);
    assert_eq!(MAILBOX_EVENT_REFERENCE_BYTES, expected.len());
    assert_eq!(encoded.as_slice(), expected);
    assert_eq!(mailbox_event_id_v1(&encoded).unwrap(), original);
}

#[path = "incremental_agent_subtree_point_tests.rs"]
mod point_tests;
