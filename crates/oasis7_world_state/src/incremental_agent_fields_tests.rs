use super::*;
use crate::{
    AgentCell, AgentState, DomainEvent,
    agent_cell::{AgentActivityV1, AgentIntentV2},
    agent_state::{CargoEntityEntry, CargoEntityKind},
    incremental_agent_body::AgentBodyValueV1,
};
use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
use std::collections::{BTreeMap, BTreeSet};
fn cell() -> AgentCell {
    let mut cell = AgentCell::new(AgentState::new("a", GeoPos::new(-1, 2, i64::MAX)), 7);
    cell.state
        .resources
        .amounts
        .insert(ResourceKind::Electricity, 0);
    cell.state.resources.amounts.insert(ResourceKind::Data, -7);
    cell.state.body_state.cargo_entries = vec![
        CargoEntityEntry {
            entity_id: "duplicate".into(),
            entity_kind: CargoEntityKind::Mineral,
            quantity: 2,
            size_per_unit: 3,
        },
        CargoEntityEntry {
            entity_id: "duplicate".into(),
            entity_kind: CargoEntityKind::ModuleItem,
            quantity: 4,
            size_per_unit: 5,
        },
    ];
    cell.activity = Some(AgentActivityV1::blocked(
        "work", 9, "target", "reason", "detail", 5,
    ));
    cell.intent = Some(AgentIntentV2 {
        schema_version: 2,
        agent_id: "a".into(),
        intent_id: "intent".into(),
        kind: "kind".into(),
        summary: "large summary".into(),
        target_id: None,
        effect_intent_id: Some("effect".into()),
        intent_tick: Some(3),
        world_id: Some("world".into()),
        reorg_epoch: Some(4),
        authority_scope: None,
        status: "accepted".into(),
        source: "source".into(),
        logical_time: 1,
        event_seq: 2,
        updated_at: 3,
        receipt_ref: Some("world-event:7".into()),
        reason_code: None,
        reason_summary: None,
        replaced_by: None,
        actor_id: "actor".into(),
        request_digest: "request".into(),
    });
    let event = DomainEvent::BodyAttributesUpdated {
        agent_id: "a".into(),
        view: cell.state.body_view.clone(),
        reason: "same".into(),
    };
    cell.mailbox.extend([event.clone(), event]);
    cell
}
#[derive(Default)]
struct Capture {
    values: BTreeMap<(u16, u64), Vec<u8>>,
    summary: usize,
    body: Vec<u8>,
    frontier: u64,
    mail: Vec<(u64, usize)>,
    fail: Option<AgentFieldV1>,
}
impl AgentCellFieldVisitor for Capture {
    type Error = &'static str;
    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        value: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error> {
        if self.fail == Some(field) {
            return Err("injected scalar failure");
        }
        if field == AgentFieldV1::IntentSummary {
            let AgentScalarRefV1::Text(value) = value else {
                panic!()
            };
            self.summary = value.as_ptr() as usize;
        }
        let mut bytes = vec![];
        value.write_to(&mut bytes).unwrap();
        assert!(self.values.insert((field as u16, ordinal), bytes).is_none());
        Ok(())
    }
    fn body(&mut self, value: &AgentBodyValueV1) -> Result<(), Self::Error> {
        self.body = value.canonical_bytes().to_vec();
        Ok(())
    }
    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error> {
        self.frontier = count;
        Ok(())
    }
    fn mailbox(&mut self, ordinal: u64, event: &DomainEvent) -> Result<(), Self::Error> {
        self.mail.push((ordinal, event as *const _ as usize));
        Ok(())
    }
}
#[test]
fn exact_scalar_and_key_vectors_preserve_types_and_borrow_text() {
    use AgentFieldV1 as F;
    use AgentScalarRefV1 as V;
    let values = [
        (F::SlotType, V::U8(5)),
        (F::SlotCapacity, V::U16(u16::MAX)),
        (F::IntentSchema, V::U32(u32::MAX)),
        (F::IntentUpdated, V::U64(u64::MAX)),
        (F::PositionX, V::I64(i64::MIN)),
        (F::SlotLocked, V::Bool(true)),
        (F::Identity, V::Text("雪\0")),
        (F::IntentTarget, V::OptionalText(None)),
        (F::IntentReceipt, V::OptionalText(Some(""))),
        (F::IntentTick, V::OptionalU64(Some(u64::MAX))),
    ];
    for (field, value) in values {
        let mut bytes = vec![];
        value.write_to(&mut bytes).unwrap();
        assert_eq!(bytes.len() as u64, value.encoded_len().unwrap());
        assert_eq!(V::from_canonical_bytes(&bytes, field).unwrap(), value);
        assert!(V::from_canonical_bytes(&bytes[..bytes.len() - 1], field).is_err());
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(V::from_canonical_bytes(&bad, field).is_err());
    }
    let mut expected = b"oasis7.iws.agent-scalar.v1\0".to_vec();
    expected.push(5);
    expected.extend(i64::MIN.to_be_bytes());
    let mut actual = vec![];
    V::I64(i64::MIN).write_to(&mut actual).unwrap();
    assert_eq!(actual, expected);
    let key = AgentFieldKeyV1::new("a", F::IntentStatus, 0).unwrap();
    let mut raw = b"oasis7.iws.agent-id.v1\0".to_vec();
    raw.extend(1u64.to_be_bytes());
    raw.push(b'a');
    let mut expected = b"oasis7.iws.agent-field-key.v1\0".to_vec();
    expected.extend(blake3::hash(&raw).as_bytes());
    expected.extend(43u16.to_be_bytes());
    expected.extend(0u64.to_be_bytes());
    assert_eq!(key.as_bytes(), expected);
    assert_eq!(
        AgentFieldKeyV1::from_bytes(key.as_bytes()).unwrap().1,
        F::IntentStatus
    );
    let mut bytes = vec![];
    V::Text("borrowed").write_to(&mut bytes).unwrap();
    let V::Text(text) = V::from_canonical_bytes(&bytes, F::IntentSummary).unwrap() else {
        panic!()
    };
    assert_eq!(text.as_ptr(), bytes[bytes.len() - 8..].as_ptr());
}
#[test]
fn exhaustive_agent_subfields_preserve_resources_duplicates_intent_and_mailbox_order() {
    let cell = cell();
    let before = serde_json::to_vec(&cell).unwrap();
    let mut sink = Capture::default();
    visit_agent_cell("a", &cell, &mut sink).unwrap();
    assert_eq!(
        sink.summary,
        cell.intent.as_ref().unwrap().summary.as_ptr() as usize
    );
    assert_eq!(sink.frontier, 2);
    for (index, (_, ptr)) in sink.mail.iter().enumerate() {
        assert_eq!(*ptr, &cell.mailbox[index] as *const _ as usize);
    }
    assert_eq!(
        sink.mail.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(
        sink.values
            .contains_key(&(AgentFieldV1::ResourceAmount as u16, 0))
    );
    assert!(
        sink.values
            .contains_key(&(AgentFieldV1::ResourceAmount as u16, 1))
    );
    assert_eq!(
        AgentScalarRefV1::from_canonical_bytes(
            &sink.values[&(AgentFieldV1::CargoQuantity as u16, 1)],
            AgentFieldV1::CargoQuantity
        )
        .unwrap(),
        AgentScalarRefV1::I64(4)
    );
    assert_eq!(
        AgentBodyValueV1::from_canonical_bytes(&sink.body)
            .unwrap()
            .last_active,
        7
    );
    assert_eq!(serde_json::to_vec(&cell).unwrap(), before);
    let names: BTreeSet<_> = AgentFieldV1::ALL.iter().map(|f| f.name()).collect();
    assert_eq!(names.len(), 53);
    assert_eq!(AgentFieldV1::ActivityStatus.namespace(), Ns::Scheduler);
    assert_eq!(AgentFieldV1::IntentStatus.namespace(), Ns::Authorization);
}
#[test]
fn unknown_tags_bad_types_ordinals_and_identity_fail_without_partial_success() {
    use AgentFieldV1 as F;
    use AgentScalarRefV1 as V;
    assert!(AgentFieldV1::from_tag(54).is_err());
    assert!(AgentFieldKeyV1::new("a", F::Identity, 1).is_err());
    assert!(AgentFieldKeyV1::new("a", F::ResourceAmount, 2).is_err());
    let mut bad = vec![];
    V::U8(9).write_to(&mut bad).unwrap();
    assert!(V::from_canonical_bytes(&bad, F::SlotType).is_err());
    assert!(V::from_canonical_bytes(&bad, F::PositionX).is_err());
    let mut cell = cell();
    cell.state.agent_id = "other".into();
    let mut sink = Capture::default();
    assert!(visit_agent_cell("a", &cell, &mut sink).is_err());
    assert!(sink.values.is_empty());
    let cell = super::tests::cell();
    let before = serde_json::to_vec(&cell).unwrap();
    let mut sink = Capture {
        fail: Some(F::IntentSummary),
        ..Capture::default()
    };
    assert!(visit_agent_cell("a", &cell, &mut sink).is_err());
    assert!(sink.mail.is_empty());
    assert_eq!(serde_json::to_vec(&cell).unwrap(), before);
}
