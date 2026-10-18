//! Formal effect birth and receipt scope come only from an attached qualified
//! parent. Legacy Worlds never acquire a fabricated world/chain/genesis scope.
use super::World;
use crate::{EffectIntent, EffectReceipt, WorldError};
use oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1;
use serde_json::Value;

impl World {
    pub(super) fn effect_world_scope(&self) -> Option<&FormalWorldIdentityV1> {
        self.formal_history_reader
            .as_ref()?
            .parent
            .as_ref()?
            .effect_world()
    }
    pub(super) fn effect_birth_era_for(&self, era: u64) -> Option<u64> {
        self.effect_world_scope().map(|_| era)
    }
    /// Construct an unsigned receipt matching the committed effect identity.
    /// Ingestion separately checks that the intent is still outstanding.
    pub fn receipt_for_effect(
        &self,
        intent: &EffectIntent,
        status: String,
        payload: Value,
        cost_cents: Option<u64>,
    ) -> Result<EffectReceipt, WorldError> {
        let (world, intent_payload_digest) = if intent.birth_era.is_some() {
            (
                Some(
                    self.effect_world_scope()
                        .ok_or_else(|| {
                            invalid("formal effect receipt lacks qualified world scope")
                        })?
                        .clone(),
                ),
                Some(intent.payload_digest()?),
            )
        } else {
            (None, None)
        };
        Ok(EffectReceipt {
            birth_era: intent.birth_era,
            world,
            intent_payload_digest,
            intent_id: intent.intent_id.clone(),
            status,
            payload,
            cost_cents,
            signature: None,
        })
    }
    pub(super) fn effect_receipt_key(&self, receipt: &EffectReceipt) -> Result<String, WorldError> {
        let mut matches = self
            .pending_effects
            .iter()
            .chain(self.inflight_effects.values())
            .filter(|intent| {
                intent.intent_id == receipt.intent_id && intent.birth_era == receipt.birth_era
            });
        let intent = matches
            .next()
            .ok_or_else(|| WorldError::ReceiptUnknownIntent {
                intent_id: receipt.intent_id.clone(),
            })?;
        if matches.any(|other| other != intent) {
            return Err(invalid("ambiguous original effect identity"));
        }
        match intent.birth_era {
            None if receipt.world.is_none() && receipt.intent_payload_digest.is_none() => {}
            Some(_)
                if receipt.world.as_ref() == self.effect_world_scope()
                    && receipt.world.is_some()
                    && receipt.intent_payload_digest.as_deref()
                        == Some(intent.payload_digest()?.as_str()) => {}
            _ => return Err(invalid("effect receipt world/birth/payload binding")),
        }
        Ok(intent.identity_key())
    }
}
fn invalid(reason: &str) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "effect_birth_tests.rs"]
mod tests;
