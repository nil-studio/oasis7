//! The supervisor must retain the sealed candidate through acknowledgement.
use super::*;

pub(crate) struct VerifiedRetainedCandidate<'a> {
    restored: &'a RestoredFormalFullSyncWorldV1,
    request_digest: String,
    exchange: Arc<dyn oasis7_game_client::formal_sync_exchange::FullSyncExchange>,
}

impl FormalFullSyncSession {
    pub(super) fn prepare_installation(
        &mut self,
        restored: &RestoredFormalFullSyncWorldV1,
    ) -> Result<(), String> {
        if let Some(exchange) = &self.shared_exchange {
            exchange
                .begin_installation(
                    &serde_json::to_value(&restored.cut).map_err(|e| e.to_string())?,
                    &restored.retained_cas_hashes,
                    &restored.manifest_reference,
                )
                .map_err(|e| e.to_string())?;
            self.check_operation_active()?;
        }
        Ok(())
    }

    pub(crate) fn is_single_operation(&self) -> bool {
        self.shared_exchange.is_some()
    }

    /// Pure validation before taking the short status/stop serialization gate.
    pub(crate) fn verify_retained_candidate<'a>(
        &mut self,
        restored: &'a RestoredFormalFullSyncWorldV1,
    ) -> Result<VerifiedRetainedCandidate<'a>, String> {
        self.check_operation_active()?;
        if self.status.loaded_cut.as_ref() != Some(&restored.cut) {
            return Err("retaining consumer differs from selected candidate".into());
        }
        let bytes = oasis7_world_state::codec::snapshot_cbor(&restored.world.snapshot())
            .map_err(|e| e.to_string())?;
        if blake3::hash(&bytes).to_hex().as_str() != restored.cut.after_state_root {
            return Err("retaining consumer World differs from selected state root".into());
        }
        cas_lifecycle::verify_loaded_world_pins(&self.cas, &restored.retained_cas_hashes)?;
        self.check_operation_active()?;
        let request_digest = self
            .shared_exchange
            .as_ref()
            .ok_or("not a shared operation")?
            .original_request_digest()
            .map_err(|e| e.to_string())?;
        Ok(VerifiedRetainedCandidate {
            restored,
            request_digest,
            exchange: Arc::clone(
                self.shared_exchange
                    .as_ref()
                    .ok_or("not a shared operation")?,
            ),
        })
    }

    /// Called with this same candidate still retained and the real owner/stop
    /// fence held. Receipt persistence is not restart readiness authority.
    pub(crate) fn acknowledge_retained_candidate(
        &mut self,
        verified: VerifiedRetainedCandidate<'_>,
    ) -> Result<(), String> {
        let restored = verified.restored;
        let exchange = self
            .shared_exchange
            .as_ref()
            .ok_or("not a shared operation")?;
        if !Arc::ptr_eq(exchange, &verified.exchange)
            || exchange
                .original_request_digest()
                .map_err(|e| e.to_string())?
                != verified.request_digest
            || self.status.loaded_cut.as_ref() != Some(&restored.cut)
        {
            return Err("verified retaining consumer belongs to another operation".into());
        }
        {
            exchange
                .complete_installation(
                    &serde_json::to_value(&restored.cut).map_err(|e| e.to_string())?,
                    &restored.retained_cas_hashes,
                    &restored.manifest_reference,
                )
                .map_err(|e| e.to_string())?;
        }
        self.status.world_loaded = true;
        self.status.ready = true;
        Ok(())
    }
}
