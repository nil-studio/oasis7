//! Typed snapshot access over the complete byte-preserving protocol delta.
use super::Snapshot;
pub use oasis7_formal_protocol::snapshot_delta::{
    MAX_SNAPSHOT_CHAIN_BYTES, MAX_SNAPSHOT_DELTA_BYTES, MAX_SNAPSHOT_DELTA_DEPTH,
    SnapshotArtifactV1, SnapshotDeltaV1,
};
pub fn resolve_snapshot_artifact(
    reference: &str,
    reader: &impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Snapshot, String> {
    resolve_snapshot_artifact_with_depth(reference, reader).map(|(snapshot, _)| snapshot)
}
pub fn resolve_snapshot_artifact_with_depth(
    reference: &str,
    reader: &impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(Snapshot, usize), String> {
    let (bytes, depth) =
        oasis7_formal_protocol::snapshot_delta::resolve_snapshot_artifact_bytes_with_depth(
            reference, reader,
        )?;
    let snapshot = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    if serde_cbor::to_vec(&snapshot).map_err(|e| e.to_string())? != bytes {
        return Err("snapshot artifact typed canonical encoding mismatch".into());
    }
    Ok((snapshot, depth))
}
#[cfg(test)]
fn bytes(snapshot: &Snapshot) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(snapshot).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Action, World};
    #[test]
    fn complete_delta_nonempty_suffix_restores_allocators_pending_and_next_work() {
        let seed = World::new();
        let mut checkpoint = seed.snapshot();
        checkpoint.next_action_id = u64::MAX;
        checkpoint.action_id_era = 7;
        checkpoint.next_intent_id = u64::MAX;
        checkpoint.intent_id_era = 8;
        checkpoint.next_proposal_id = u64::MAX;
        checkpoint.proposal_id_era = 9;
        let mut live = World::from_snapshot(checkpoint.clone(), seed.journal().clone()).unwrap();
        live.submit_action(Action::RegisterAgent {
            agent_id: "delta-agent".into(),
            pos: crate::GeoPos::new(0, 0, 0),
        });
        live.step().unwrap();
        // A pending action's reserved ID and its consumption boundary are durable too.
        live.submit_action(Action::MoveAgent {
            agent_id: "delta-agent".into(),
            to: crate::GeoPos::new(1, 0, 0),
        });
        let target = live.snapshot();
        assert!(target.journal_len > checkpoint.journal_len);
        assert_ne!(checkpoint.action_id_era, target.action_id_era);
        let delta = SnapshotDeltaV1::between(&checkpoint, &target).unwrap();
        assert!(delta.replacements.contains_key("pending_actions"));
        assert!(delta.replacements.contains_key("next_action_id"));
        let decoded: SnapshotDeltaV1 =
            serde_cbor::from_slice(&serde_cbor::to_vec(&delta).unwrap()).unwrap();
        let recovered = decoded.apply(&checkpoint).unwrap();
        assert_eq!(bytes(&recovered).unwrap(), bytes(&target).unwrap());
        let mut restored = World::from_snapshot(recovered, live.journal().clone()).unwrap();
        for world in [&mut live, &mut restored] {
            world.step().unwrap();
            world.submit_action(Action::MoveAgent {
                agent_id: "delta-agent".into(),
                to: crate::GeoPos::new(2, 0, 0),
            });
            world.step().unwrap();
        }
        assert_eq!(
            bytes(&restored.snapshot()).unwrap(),
            bytes(&live.snapshot()).unwrap()
        );
        assert_eq!(restored.journal(), live.journal());
    }
    #[test]
    fn complete_delta_rejects_missing_control_fields_base_and_target_tampering() {
        let base = World::new().snapshot();
        let mut target = base.clone();
        target
            .module_tick_schedule
            .insert("pure.instance".into(), 42);
        target
            .module_artifact_bytes
            .insert("original-version".into(), vec![1, 2, 3]);
        let delta = SnapshotDeltaV1::between(&base, &target).unwrap();
        assert_eq!(delta.apply(&base).unwrap(), target);
        let mut bad = delta.clone();
        bad.fields.retain(|key| key != "next_intent_id");
        assert!(bad.apply(&base).is_err());
        let mut bad = delta.clone();
        bad.replacements.remove("module_tick_schedule");
        assert!(bad.apply(&base).is_err());
        let mut bad = delta.clone();
        bad.schema_version = 2;
        assert!(bad.apply(&base).is_err());
        assert!(delta.apply(&target).is_err());
        let cleared = SnapshotDeltaV1::between(&target, &base).unwrap();
        assert!(
            cleared
                .apply(&target)
                .unwrap()
                .module_tick_schedule
                .is_empty()
        );
    }
    #[test]
    fn complete_delta_preserves_confirmed_and_unknown_effects_without_redispatch() {
        use crate::runtime::{CapabilityGrant, EffectOrigin, PolicySet};
        let mut live = World::new();
        live.add_capability(CapabilityGrant::allow_all("delta-effects"));
        live.set_policy(PolicySet::allow_all());
        let mut checkpoint = live.snapshot();
        checkpoint.next_intent_id = u64::MAX;
        checkpoint.intent_id_era = 8;
        live = World::from_snapshot(checkpoint.clone(), live.journal().clone()).unwrap();
        live.emit_effect(
            "http.request",
            serde_json::json!({"input":"original"}),
            "delta-effects",
            EffectOrigin::System,
        )
        .unwrap();
        let confirmed = live.take_next_effect().unwrap();
        let receipt = live
            .receipt_for_effect(
                &confirmed,
                "ok".into(),
                serde_json::json!({"original_response":73}),
                Some(5),
            )
            .unwrap();
        live.ingest_receipt(receipt).unwrap();
        live.emit_effect(
            "http.request",
            serde_json::json!({"input":"unknown_delivery"}),
            "delta-effects",
            EffectOrigin::System,
        )
        .unwrap();
        let unknown = live.take_next_effect().unwrap();
        let target = live.snapshot();
        assert_eq!(target.intent_id_era, 9);
        let delta = SnapshotDeltaV1::between(&checkpoint, &target).unwrap();
        let mut restored =
            World::from_snapshot(delta.apply(&checkpoint).unwrap(), live.journal().clone())
                .unwrap();
        assert!(
            restored.take_next_effect().is_none(),
            "confirmed and unknown inflight effects must not redispatch"
        );
        assert!(
            restored
                .snapshot()
                .inflight_effects
                .values()
                .any(|effect| effect == &unknown)
        );
        assert_eq!(restored.snapshot(), target);
        for world in [&mut live, &mut restored] {
            world
                .emit_effect(
                    "http.request",
                    serde_json::json!({"input":"next_legal"}),
                    "delta-effects",
                    EffectOrigin::System,
                )
                .unwrap();
        }
        assert_eq!(live.take_next_effect(), restored.take_next_effect());
        assert_eq!(live.snapshot(), restored.snapshot());
    }
}
