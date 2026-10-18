//! Operator-private encrypted goals. No public CAS, ledger or activation authority.
//! The caller supplies a dedicated absolute directory and an external 32-byte key.
//! Only Unix permissions are supported; unavailable security guarantees fail closed.
use oasis7_agent_api::{
    Digest32,
    game::{DecimalU64, GoalContentV1, game_goal_version_digest},
};
use oasis7_client_api::world_service::WorldIdentity;
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

pub const MAX_PRIVATE_GOAL_FILE_BYTES: usize = 32 * 1024;
#[cfg(unix)]
const MAGIC: &[u8; 8] = b"O7GOAL01";

/// Private authenticated metadata, never a public intent payload. Expected values
/// on read must come from the caller's trusted owner/task authority boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateGoalBindingV1 {
    pub world: WorldIdentity,
    pub task_id: String,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_public_key: String,
    pub owner_generation: DecimalU64,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub goal_revision: DecimalU64,
}
impl PrivateGoalBindingV1 {
    fn validate(&self) -> Result<(), String> {
        let valid = |s: &str| !s.trim().is_empty() && s.len() <= 256;
        if !valid(&self.world.world_id)
            || !valid(&self.world.genesis_digest)
            || !valid(&self.task_id)
            || !valid(&self.owner_public_key)
            || !valid(&self.branch_id)
            || !valid(&self.agent_id)
            || !valid(&self.owner_binding)
            || self.owner_generation.0 == 0
            || self.goal_revision.0 == 0
            || self.owner_public_key.len() != 64
            || !self
                .owner_public_key
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("invalid private goal binding".into());
        }
        Ok(())
    }
}

/// A random locator, not a content-address or proof of permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateGoalRef(String);
impl PrivateGoalRef {
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("invalid private goal reference".into());
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Decrypted local data. The digest is private and must not enter public streams.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredGoalV1 {
    pub schema_revision: u32,
    pub binding: PrivateGoalBindingV1,
    pub content: GoalContentV1,
    pub goal_digest: Digest32,
}
#[cfg(unix)]
impl StoredGoalV1 {
    fn validate(&self) -> Result<(), String> {
        self.binding.validate()?;
        if self.schema_revision != 1
            || self.goal_digest
                != game_goal_version_digest(
                    &self.binding.task_id,
                    self.binding.goal_revision,
                    &self.content,
                )
                .map_err(str::to_owned)?
        {
            return Err("invalid stored private goal".into());
        }
        Ok(())
    }
}

/// Holds a capability directory handle and an erased-on-drop external key.
/// Keys are never persisted; reopening with the same key restores stored bodies.
pub struct PrivateGoalStore {
    #[cfg(unix)]
    directory: cap_std::fs::Dir,
    key: Zeroizing<[u8; 32]>,
}
impl PrivateGoalStore {
    pub fn open(root: &Path, key: [u8; 32]) -> Result<Self, String> {
        let key = Zeroizing::new(key);
        #[cfg(unix)]
        {
            let directory = unix::open_root(root)?;
            unix::initialize_key(&directory, &key)?;
            Ok(Self { directory, key })
        }
        #[cfg(not(unix))]
        {
            let _ = (root, key);
            Err("private goal storage requires Unix permission guarantees".into())
        }
    }
    /// Returns only after immutable publication and directory fsync succeed.
    pub fn put(
        &self,
        binding: &PrivateGoalBindingV1,
        content: &GoalContentV1,
    ) -> Result<PrivateGoalRef, String> {
        binding.validate()?;
        let goal_digest =
            game_goal_version_digest(&binding.task_id, binding.goal_revision, content)
                .map_err(str::to_owned)?;
        let goal = StoredGoalV1 {
            schema_revision: 1,
            binding: binding.clone(),
            content: content.clone(),
            goal_digest,
        };
        let plaintext =
            Zeroizing::new(serde_cbor::to_vec(&goal).map_err(|_| "private goal encoding failed")?);
        #[cfg(unix)]
        {
            unix::put(&self.directory, &self.key, binding, &plaintext)
        }
        #[cfg(not(unix))]
        {
            let _ = (plaintext, &self.key);
            Err("private goal storage unsupported".into())
        }
    }
    pub fn get(
        &self,
        reference: &PrivateGoalRef,
        expected: &PrivateGoalBindingV1,
    ) -> Result<StoredGoalV1, String> {
        expected.validate()?;
        #[cfg(unix)]
        {
            let plaintext = unix::get(&self.directory, &self.key, reference, expected)?;
            let goal: StoredGoalV1 =
                serde_cbor::from_slice(&plaintext).map_err(|_| "invalid private goal encoding")?;
            goal.validate()?;
            if &goal.binding != expected {
                return Err("private goal binding mismatch".into());
            }
            Ok(goal)
        }
        #[cfg(not(unix))]
        {
            let _ = (reference, &self.key);
            Err("private goal storage unsupported".into())
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
#[cfg(unix)]
mod unix;

pub mod prepared;
pub use prepared::{PreparedGoalError, PreparedGoalReceiptV1};

mod task_context;
pub use task_context::PrivateTaskContextError;

pub mod client_state;
