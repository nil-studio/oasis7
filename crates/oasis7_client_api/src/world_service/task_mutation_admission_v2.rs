//! Queue acceptance only. This type has no canonical result, receipt or Current.
use super::*;
use oasis7_agent_api::game::DecimalU64;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskMutationKindV2 {
    GuardedStop,
    RevokeAndClose,
}
impl TaskMutationKindV2 {
    pub fn path(self) -> &'static str {
        match self {
            Self::GuardedStop => task_stop_v2::TASK_STOP_SUBMIT_V2,
            Self::RevokeAndClose => task_cleanup_v2::TASK_CLEANUP_SUBMIT_V2,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskMutationQueuedOutcomeV2 {
    QueuedVolatile,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMutationAdmissionV2 {
    pub api_revision: u8,
    pub kind: TaskMutationKindV2,
    pub world: WorldIdentity,
    pub original_digest: String,
    pub action_id: DecimalU64,
    pub admitted_at_ms: DecimalU64,
    pub outcome: TaskMutationQueuedOutcomeV2,
}
/// Deterministic queue ID; it is neither a World nonce nor a permanent HWM.
pub fn mutation_action_id_v2(
    kind: TaskMutationKindV2,
    world: &WorldIdentity,
    original_digest: &str,
) -> Result<u64, String> {
    world.validate().map_err(|e| e.to_string())?;
    if original_digest.len() != 64
        || !original_digest
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err("invalid mutation original digest".into());
    }
    let digest = request_digest(
        "oasis7.world.task-mutation-action/v2",
        &(kind, world, original_digest),
    )?;
    let hex_digest = digest
        .strip_prefix("blake3:")
        .ok_or("mutation action digest algorithm mismatch")?;
    if hex_digest.len() != 64
        || !hex_digest
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err("invalid mutation action digest".into());
    }
    let bytes = hex::decode(hex_digest).map_err(|e| e.to_string())?;
    Ok(u64::from_be_bytes(bytes[..8].try_into().map_err(|_| "action digest width")?).max(1))
}
impl TaskMutationAdmissionV2 {
    pub fn validate(
        &self,
        kind: TaskMutationKindV2,
        world: &WorldIdentity,
        original_digest: &str,
    ) -> Result<(), String> {
        if self.api_revision != 2
            || self.kind != kind
            || &self.world != world
            || self.original_digest != original_digest
            || self.action_id.0 != mutation_action_id_v2(kind, world, original_digest)?
            || self.admitted_at_ms.0 == 0
        {
            return Err("task mutation queued ACK mismatch".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod action_id_tests {
    use super::*;
    #[test]
    fn task_mutation_action_id_maps_actual_prefixed_digest_without_rehashing() {
        let world = WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        };
        let original = "ab".repeat(32);
        for kind in [
            TaskMutationKindV2::GuardedStop,
            TaskMutationKindV2::RevokeAndClose,
        ] {
            let actual = request_digest(
                "oasis7.world.task-mutation-action/v2",
                &(kind, &world, &original),
            )
            .unwrap();
            assert!(actual.starts_with("blake3:"));
            let raw = hex::decode(actual.strip_prefix("blake3:").unwrap()).unwrap();
            let expected = u64::from_be_bytes(raw[..8].try_into().unwrap()).max(1);
            assert_eq!(mutation_action_id_v2(kind, &world, &original), Ok(expected));
        }
    }
}
