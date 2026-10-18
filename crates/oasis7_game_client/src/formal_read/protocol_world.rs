//! Synthetic read-only protocol data; real World adapter coverage belongs to Service.
use super::super::WorldServiceProjection;
use oasis7_client_api::formal_projection::*;
#[derive(Clone)]
pub(in crate::formal_read) struct ProtocolWorld {
    pub(in crate::formal_read) projection: WorldServiceProjection,
}
impl ProtocolWorld {
    pub(in crate::formal_read) fn new() -> Self {
        let projection = serde_json::from_value(serde_json::json!({
            "state":{"time":0,"agents":{},"resources":{},"materials":{},"material_ledgers":{"world":{}}},
            "runtime_binding":{"world_id":"formal-fixture-world","branch_id":"main","finality_epoch":0,
                "finality_block_hash":null,"finality_status":"pending","base_tick":0,
                "base_world_hash":format!("blake3:{}", "90".repeat(32)), "reorg_epoch":0,
                "runtime_manifest_hash":format!("blake3:{}", "91".repeat(32))},
            "formal_commit_header":{"authority_generation":0,"permission_generation":0,"event_id_era":1,
                "journal_retention_floor":0,"journal_high_watermark":0},
            "agent_context":null,"scheduler_wakes":[],"continuations":[],"cognition_leases":[],
            "continuation_contexts":{},"events":[]
        })).unwrap();
        Self { projection }
    }
    pub(in crate::formal_read) fn step(&mut self) -> Result<(), String> {
        self.projection.state.time += 1;
        let bytes = serde_json::to_vec(&self.projection).unwrap();
        let binding = self.projection.runtime_binding.as_mut().unwrap();
        binding.base_tick = self.projection.state.time;
        binding.base_world_hash = serde_json::from_value(serde_json::json!(format!(
            "blake3:{}",
            blake3::hash(&bytes)
        )))
        .unwrap();
        Ok(())
    }
    pub(in crate::formal_read) fn snapshot(&self) -> &WorldServiceProjection {
        &self.projection
    }
    pub(in crate::formal_read) fn journal(&self) -> &[serde_json::Value; 0] {
        &[]
    }
    pub(in crate::formal_read) fn current_cognition_runtime_binding(
        &self,
    ) -> Result<oasis7_agent_api::RuntimeBindingV1, String> {
        Ok(self.projection.runtime_binding.clone().unwrap())
    }
}
pub(in crate::formal_read) fn commitments_for_world(
    world: &ProtocolWorld,
) -> Result<ProjectionCommitmentsV1, String> {
    let audience = ProjectionAudienceV1::public();
    ProjectionCommitmentsV1::from_leaves(vec![(
        audience.clone(),
        projection_leaf(&audience, &world.projection)?,
    )])
}
