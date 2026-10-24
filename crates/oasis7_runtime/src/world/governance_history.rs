//! Revalidate governance against each original qualified parent, without replaying
//! unrelated simulation state. The host verifies the actual hash-linked chain.
use super::World;
use crate::{GovernanceEvent, VerifiedFormalRetainedHistoryParentV1, WorldEventBody};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

#[cfg(test)]
#[path = "governance_history_tests.rs"]
mod tests;

const MAX_EVENTS: u128 = 65_536;
const MAX_BYTES: usize = 64 * 1024 * 1024;
type Cursor = (u64, u64);

fn ordinal(cursor: Cursor) -> Result<u128, String> {
    if cursor.0 == u64::MAX || cursor.1 == 0 {
        return Err("governance history event cursor is invalid or saturated".into());
    }
    Ok(u128::from(cursor.0) * u128::from(u64::MAX) + u128::from(cursor.1 - 1))
}
fn advance(cursor: Cursor) -> Result<Cursor, String> {
    if cursor.1 == u64::MAX {
        Ok((
            cursor
                .0
                .checked_add(1)
                .ok_or("governance history cursor exhausted")?,
            1,
        ))
    } else {
        Ok((cursor.0, cursor.1 + 1))
    }
}
fn previous(cursor: Cursor) -> Result<Cursor, String> {
    if cursor.1 == 1 {
        Ok((
            cursor
                .0
                .checked_sub(1)
                .ok_or("governance history journal underflow")?,
            u64::MAX,
        ))
    } else {
        Ok((cursor.0, cursor.1 - 1))
    }
}

// Counting serialization avoids allocating a second potentially large event.
struct ByteBudget(usize);
impl Write for ByteBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("governance history byte capacity exceeded"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl World {
    fn exact_governance_history_parent(
        &self,
    ) -> Result<&VerifiedFormalRetainedHistoryParentV1, String> {
        let parent = self
            .formal_history_reader
            .as_ref()
            .and_then(|h| h.parent.as_ref())
            .ok_or("governance history requires qualified snapshot parents")?;
        let bytes = oasis7_world_state::codec::snapshot_cbor(&self.snapshot())
            .map_err(|e| e.to_string())?;
        if blake3::hash(&bytes).to_hex().as_str() != parent.after_snapshot_root()
            || self.state.retained_history_root.as_ref() != Some(parent.root())
        {
            return Err("governance history snapshot differs from qualified parent".into());
        }
        Ok(parent)
    }

    /// Validate the governance projection of one adjacent pair of qualified
    /// snapshots. This does not verify the host's canonical previous-hash chain.
    pub fn verify_formal_governance_successor(
        &self,
        next: &World,
        mut check_budget: impl FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        check_budget()?;
        let parent = self.exact_governance_history_parent()?;
        check_budget()?;
        let successor = next.exact_governance_history_parent()?;
        check_budget()?;
        let world = parent
            .effect_world()
            .ok_or("governance history world missing")?;
        if successor.effect_world() != Some(world) {
            return Err("governance history world domain mismatch".into());
        }
        let head = parent
            .head()
            .ok_or("governance history parent head missing")?;
        let next_head = successor
            .head()
            .ok_or("governance history successor head missing")?;
        if head.position.checked_add(1) != Some(next_head.position) {
            return Err("governance history commits are not adjacent".into());
        }
        if next.state.time < self.state.time {
            return Err("governance history logical time moved backward".into());
        }
        let start = (self.next_event_id_era, self.next_event_id);
        let end = (next.next_event_id_era, next.next_event_id);
        let count = ordinal(end)?
            .checked_sub(ordinal(start)?)
            .ok_or("governance history event cursor moved backward")?;
        if count > MAX_EVENTS {
            return Err("governance history event capacity exceeded".into());
        }
        // Walk only the bounded relevant hot suffix; older entries are outside
        // this commit. Missing events are fetched from the authenticated index.
        let mut hot = BTreeMap::new();
        let mut cursor = end;
        for event in next.journal.events.iter().rev().take(count as usize) {
            check_budget()?;
            cursor = previous(cursor)?;
            if event.id != cursor.1 {
                return Err("governance history hot journal is not contiguous".into());
            }
            hot.insert(cursor, event);
        }
        let mut projection = self.clone();
        projection.governance_replaying = true;
        let mut budget = ByteBudget(MAX_BYTES);
        let mut touched = BTreeSet::new();
        for record in self.state.governance_validator_admissions.values() {
            check_budget()?;
            touched.insert(record.node_id.clone());
        }
        let mut cursor = start;
        while cursor != end {
            check_budget()?;
            let cold;
            let event = if let Some(event) = hot.get(&cursor) {
                *event
            } else {
                cold = next
                    .read_retained_journal_event(cursor.0, cursor.1)
                    .map_err(|e| format!("governance history cold journal: {e:?}"))?
                    .ok_or("governance history event missing")?;
                &cold
            };
            if event.id != cursor.1
                || event.time < projection.state.time
                || event.time > next.state.time
            {
                return Err("governance history event cursor or time mismatch".into());
            }
            serde_cbor::to_writer(&mut budget, event)
                .map_err(|_| "governance history byte capacity exceeded".to_string())?;
            projection.state.time = event.time;
            if let WorldEventBody::Governance(governance) = &event.body
                && matches!(
                    governance,
                    GovernanceEvent::ValidatorAdmissionSubmitted { .. }
                        | GovernanceEvent::ValidatorAdmissionApproved { .. }
                        | GovernanceEvent::ValidatorAdmissionActivated { .. }
                        | GovernanceEvent::ValidatorAdmissionRevoked { .. }
                        | GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated { .. }
                )
            {
                projection.apply_governance_event(governance).map_err(|e| {
                    format!("governance history authorization or transition: {e:?}")
                })?;
            }
            cursor = advance(cursor)?;
            check_budget()?;
        }
        if projection.state.governance_validator_admissions
            != next.state.governance_validator_admissions
            || projection.state.governance_authorization_nonces
                != next.state.governance_authorization_nonces
            || projection.state.governance_main_token_controller_registry
                != next.state.governance_main_token_controller_registry
        {
            return Err("governance history snapshot projection mismatch".into());
        }
        for record in projection.state.governance_validator_admissions.values() {
            check_budget()?;
            touched.insert(record.node_id.clone());
        }
        for node in touched {
            check_budget()?;
            if projection.state.node_identity_bindings.get(&node)
                != next.state.node_identity_bindings.get(&node)
                || projection.state.node_main_token_account_bindings.get(&node)
                    != next.state.node_main_token_account_bindings.get(&node)
            {
                return Err(
                    "governance history admission identity/account mapping mismatch".into(),
                );
            }
        }
        check_budget()?;
        Ok(())
    }
}
