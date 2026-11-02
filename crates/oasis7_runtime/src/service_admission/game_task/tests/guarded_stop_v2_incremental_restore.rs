//! Actual GuardedStop → existing Runtime Authorization Value → qualified
//! snapshot cold restore. Not an incremental commit/remote finality claim.
use super::*;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_world_state::incremental_projection::{
    FieldDescriptor, FieldDestination, FieldOwner, FieldShape, GenesisFieldVisitor, SemanticKeyV1,
};
use serde::Serialize;
#[derive(Default)]
struct AuthorizationValue {
    values: Vec<serde_json::Value>,
    descriptor: Option<FieldDescriptor>,
}
impl GenesisFieldVisitor for AuthorizationValue {
    type Error = String;
    fn begin(&mut self, field: FieldDescriptor, _: Option<usize>) -> Result<(), String> {
        if field.owner == FieldOwner::Runtime && field.name == "capability_revocation_state" {
            assert_eq!(field.shape, FieldShape::Value);
            assert_eq!(
                field.destination,
                FieldDestination::State(StateNamespaceV1::Authorization)
            );
            assert!(self.descriptor.replace(field).is_none());
        }
        Ok(())
    }
    fn group<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        _: FieldDescriptor,
        _: &K,
        _: usize,
    ) -> Result<(), String> {
        Ok(())
    }
    fn entry<K: SemanticKeyV1 + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        _: &K,
        value: &V,
    ) -> Result<(), String> {
        if field.owner == FieldOwner::Runtime && field.name == "capability_revocation_state" {
            self.values
                .push(serde_json::to_value(value).map_err(|e| e.to_string())?);
        }
        Ok(())
    }
    fn bytes<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        _: FieldDescriptor,
        _: &K,
        _: &[u8],
    ) -> Result<(), String> {
        Ok(())
    }
    fn side<T: Serialize + ?Sized>(
        &mut self,
        _: FieldDescriptor,
        _: &T,
        _: Option<u64>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn process(&mut self, _: FieldDescriptor) -> Result<(), String> {
        Ok(())
    }
}
fn capture(world: &World) -> serde_json::Value {
    let mut sink = AuthorizationValue::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    assert!(sink.descriptor.is_some());
    assert_eq!(sink.values.len(), 1);
    sink.values.pop().unwrap()
}
struct NoArchivedNodes;
impl oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader
    for NoArchivedNodes
{
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
#[test]
fn guarded_stop_v2_incremental_authorization_value_and_cold_restore_preserve_record() {
    let (mut world, owner, initial) = setup();
    let before = capture(&world);
    assert!(
        before.get("task_stops_v2").is_none(),
        "old None stays omitted"
    );
    let original = guarded_stop(&world, &owner, &initial, "incremental-guarded-stop", 5000);
    let context = begin(&mut world);
    let record = world.apply_guarded_task_stop_v2(&original).unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    assert_eq!(record.committed_height, context.height);
    assert_eq!(
        record.committed_at_unix_ms,
        u64::try_from(context.committed_at_unix_ms).unwrap()
    );
    assert_eq!(
        record.after_control.task_status,
        GameTaskStatusV1::Suspended
    );
    let after = capture(&world);
    assert_ne!(before, after);
    let records = after["task_stops_v2"]["records"].as_object().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records.get(&record.operation_key().unwrap()),
        Some(&serde_json::to_value(&record).unwrap())
    );
    assert_eq!(
        after,
        serde_json::to_value(world.capability_revocation_state()).unwrap(),
        "the whole nested Stop index must remain inside the Authorization Value"
    );
    let snapshot = world.snapshot();
    let bytes = oasis7_world_state::codec::snapshot_cbor(&snapshot).unwrap();
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        snapshot.state.retained_history_root.clone().unwrap(),
        &bytes,
    );
    let restored = World::from_verified_formal_snapshot(
        snapshot.clone(),
        world.journal().clone(),
        parent,
        std::sync::Arc::new(NoArchivedNodes),
    )
    .unwrap();
    assert_eq!(
        restored.guarded_task_stop_v2(&original).unwrap(),
        Some(record)
    );
    assert_eq!(capture(&restored), after);
    assert_eq!(
        restored.snapshot().capability_authorization_root,
        snapshot.capability_authorization_root
    );
    assert_eq!(
        restored.capability_revocation_state().task_stops_v2,
        snapshot.capability_revocation_state.task_stops_v2
    );
    assert_eq!(
        current_control(&restored, "agent")
            .unwrap()
            .unwrap()
            .task_status,
        GameTaskStatusV1::Suspended
    );
}
