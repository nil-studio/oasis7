//! Pure authority binding of the raw roots retained by durable receipts.
use super::cognition::world_state_binding_digest_v1;
use super::cognition_recovery::{
    RuntimeCognitionBaseBindingV1, WorldCommitRecordV1, cognition_digest_v1,
};

impl WorldCommitRecordV1 {
    /// Derive the bound request identity from the recorded raw parent roots.
    /// This does not authenticate a receipt or alter its persisted digest;
    /// consumers must also verify its signature, request and receipt lineage.
    pub fn normalized_base_binding(&self) -> RuntimeCognitionBaseBindingV1 {
        RuntimeCognitionBaseBindingV1 {
            world_id: self.world_id.clone(),
            branch_id: self.branch_id.clone(),
            finality_epoch: self.finality_epoch,
            finality_block_hash: self.finality_block_hash.clone(),
            finality_status: self.finality_status.clone(),
            base_tick: self.parent_tick,
            base_world_hash: world_state_binding_digest_v1(
                &self.world_id,
                &self.branch_id,
                self.finality_epoch,
                self.finality_block_hash.as_deref(),
                &self.finality_status,
                self.parent_tick,
                &self.parent_world_hash,
                self.reorg_epoch,
                &self.runtime_manifest_hash,
            ),
            reorg_epoch: self.reorg_epoch,
            runtime_manifest_hash: cognition_digest_v1(
                "oasis7.runtime.manifest.v1",
                &self.runtime_manifest_hash,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn marker() -> WorldCommitRecordV1 {
        serde_json::from_value(json!({
            "schema_version":"world-commit.v1", "commit_id":"commit-a",
            "envelope_idempotency_key":"key-a", "envelope_digest":"envelope-a",
            "world_id":"world-a", "branch_id":"main", "finality_epoch":2,
            "finality_block_hash":"block-a", "finality_status":"verified",
            "finality_binding_digest":"finality-a", "runtime_manifest_hash":"raw-manifest",
            "action_id":"action:7", "parent_tick":11, "parent_world_hash":"raw-state",
            "staged_event_root":"event-a", "staged_state_root":"state-a",
            "receipt_id":"receipt-a", "receipt_digest":"receipt-digest-a",
            "reorg_epoch":3, "cognition_journal_seq":4, "status":"committed"
        }))
        .unwrap()
    }

    #[test]
    fn receipt_normalizes_raw_roots_without_mutating_marker() {
        let marker = marker();
        let original = marker.clone();
        let binding = marker.normalized_base_binding();
        assert_eq!(
            binding.base_world_hash,
            world_state_binding_digest_v1(
                "world-a",
                "main",
                2,
                Some("block-a"),
                "verified",
                11,
                "raw-state",
                3,
                "raw-manifest",
            )
        );
        assert_eq!(
            binding.runtime_manifest_hash,
            cognition_digest_v1("oasis7.runtime.manifest.v1", &"raw-manifest")
        );
        assert_ne!(binding.base_world_hash, marker.parent_world_hash);
        assert_ne!(binding.runtime_manifest_hash, marker.runtime_manifest_hash);
        assert_eq!(marker, original);
    }

    #[test]
    fn receipt_binding_rejects_every_parent_authority_axis_change() {
        let marker = marker();
        let expected = marker.normalized_base_binding();
        for (field, value) in [
            ("world_id", json!("world-b")),
            ("branch_id", json!("fork")),
            ("finality_epoch", json!(4)),
            ("finality_block_hash", json!("block-b")),
            ("finality_block_hash", json!(null)),
            ("finality_status", json!("pending")),
            ("parent_tick", json!(12)),
            ("reorg_epoch", json!(4)),
            ("parent_world_hash", json!("different-state")),
            ("runtime_manifest_hash", json!("different-manifest")),
        ] {
            let mut tampered = serde_json::to_value(&marker).unwrap();
            tampered[field] = value;
            let tampered: WorldCommitRecordV1 = serde_json::from_value(tampered).unwrap();
            assert_ne!(tampered.normalized_base_binding(), expected, "{field}");
        }
    }
}
