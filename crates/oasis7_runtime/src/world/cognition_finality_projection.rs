use super::World;
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};

impl World {
    /// Promote the Runtime cognition wire binding after a finalized
    /// capability authority is durably installed. Governance records use
    /// `finalized`, while cognition requests use `verified`; the promotion is
    /// valid only for the same world, branch, epoch, and finality hash. It is
    /// deliberately refused while a wake or continuation is live so an
    /// in-flight parent cannot be rebased by a status-only update.
    pub fn promote_cognition_runtime_finality(&mut self) -> Result<bool, WorldError> {
        let mut transaction = self.clone();
        let promoted = transaction.promote_cognition_runtime_finality_inner()?;
        if promoted {
            transaction.persist_runtime_transaction_if_configured()?;
            *self = transaction;
        }
        Ok(promoted)
    }

    fn promote_cognition_runtime_finality_inner(&mut self) -> Result<bool, WorldError> {
        let binding = self.current_cognition_runtime_binding()?;
        if binding.finality_status == "verified" {
            return Ok(false);
        }
        if binding.finality_status != "pending" {
            return Err(cognition_validation_error("runtime_binding_conflict"));
        }

        let authorities = &self.capability_revocation_state().authority_records;
        let Some(first) = authorities.values().next() else {
            return Err(cognition_validation_error("runtime_authority_required"));
        };
        let consistent = authorities.values().all(|record| {
            record.world_id == first.world_id
                && record.branch_id == first.branch_id
                && record.finality_epoch == first.finality_epoch
                && record.finality_block_hash == first.finality_block_hash
                && record.finality_status == first.finality_status
        });
        if !consistent || first.finality_status != "finalized" {
            return Err(cognition_validation_error("recovery_pending"));
        }
        let binding_finality_hash = binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string);
        if first.world_id != binding.world_id
            || first.branch_id != binding.branch_id
            || first.finality_epoch != binding.finality_epoch
            || binding_finality_hash.as_deref() != Some(first.finality_block_hash.as_str())
        {
            return Err(cognition_validation_error("reorg_invalidated"));
        }
        if !self.active_cognition_continuations()?.is_empty()
            || !self.cognition_in_flight_wakes()?.is_empty()
        {
            return Err(cognition_validation_error("runtime_binding_conflict"));
        }

        let mut projection = self
            .cognition
            .as_object()
            .cloned()
            .ok_or_else(|| cognition_validation_error("runtime_binding_missing"))?;
        let runtime_binding = projection
            .get_mut("runtime_binding")
            .and_then(JsonValue::as_object_mut)
            .ok_or_else(|| cognition_validation_error("runtime_binding_missing"))?;
        runtime_binding.insert("finality_status".to_string(), json!("verified"));
        self.cognition = JsonValue::Object(projection);
        self.current_cognition_runtime_binding()?;
        Ok(true)
    }
}

fn cognition_validation_error(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition validation failed: {code}"),
    }
}
