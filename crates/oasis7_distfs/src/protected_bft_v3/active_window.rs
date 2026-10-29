//! Verified replay bases retain H authority and authenticated historical context.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::state::{BftDriverAppliedReceiptV3, BftSignerStateV3, VerifiedBftSignerStateV3};
use super::successor_state::BftSuccessorWindowStoredRequestV1;
use super::successor_window::*;
use oasis7_formal_protocol::bft::FormalBftCertifiedHeaderV3;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerActiveWindowV1 {
    pub schema_version: u32,
    pub checkpoint_digest: String,
}
/// No Deserialize: only the complete original archive verification path creates it.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedBftActiveWindowContextV1 {
    checkpoint: VerifiedBftSuccessorWindowV1,
    history: Vec<FormalBftCertifiedHeaderV3>,
    applied_installs: Vec<BftDriverAppliedReceiptV3>,
}
impl VerifiedBftActiveWindowContextV1 {
    pub(crate) fn checkpoint(&self) -> &VerifiedBftSuccessorWindowV1 {
        &self.checkpoint
    }
    pub(crate) fn validate_anchor(&self, state: &BftSignerStateV3) -> Result<(), BftSignerErrorV3> {
        let anchor = state
            .active_window
            .as_ref()
            .ok_or_else(|| invalid("active window anchor missing"))?;
        if anchor.schema_version != 1
            || anchor.checkpoint_digest != self.checkpoint.digest()
            || !state.successor_window.as_ref().is_some_and(|w| {
                w.requests
                    .iter()
                    .rev()
                    .find(|r| r.request.status == BftSuccessorWindowStatusV1::Adopted)
                    .is_some_and(|r| {
                        r.request.intent.body.checkpoint.digest().ok().as_deref()
                            == Some(self.checkpoint.digest())
                            && r.request
                                .adopted_evidence
                                .as_ref()
                                .is_some_and(|e| e.active_segment_origin.is_some())
                    })
            })
        {
            return Err(invalid(
                "active window differs from authenticated adopted checkpoint",
            ));
        }
        Ok(())
    }
    fn historical_state(
        &self,
        state: &BftSignerStateV3,
    ) -> Result<BftSignerStateV3, BftSignerErrorV3> {
        self.validate_anchor(state)?;
        let mut full = state.clone();
        full.history = self.history.clone();
        full.history.extend(state.history.iter().cloned());
        full.applied_installs = self.applied_installs.clone();
        full.applied_installs
            .extend(state.applied_installs.iter().cloned());
        if full.history.len() > 64 * 64 {
            return Err(invalid("successor historical context capacity"));
        }
        Ok(full)
    }
    pub(crate) fn validate_governance_records(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        provider: Option<&str>,
    ) -> Result<(), BftSignerErrorV3> {
        let historical = self.historical_state(state)?;
        super::membership::validate_records(binding, &historical, provider)?;
        super::rotation::validate_archive(binding, &historical)?;
        super::bootstrap_rotation::validate_archive(binding, &historical)
    }
    pub(crate) fn membership_contexts(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
    ) -> Result<Vec<super::install::BftTransitionInstallIntentBodyV3>, BftSignerErrorV3> {
        super::membership::contexts(binding, &self.historical_state(state)?)
    }
    pub(crate) fn has_applied_receipt(&self, receipt: &BftDriverAppliedReceiptV3) -> bool {
        self.applied_installs.iter().any(|r| r == receipt)
    }
}
/// Sequentially authenticates every original S and exact predecessor. Carries
/// historical certificate/driver evidence only; old journals stay in archives.
pub(crate) struct BftSuccessorArchiveChainVerifierV1 {
    records: Vec<BftSuccessorWindowStoredRequestV1>,
    context: Option<Arc<VerifiedBftActiveWindowContextV1>>,
}
impl BftSuccessorArchiveChainVerifierV1 {
    pub(crate) fn new() -> Self {
        Self {
            records: Vec::new(),
            context: None,
        }
    }
    pub(crate) fn context(&self) -> Option<&Arc<VerifiedBftActiveWindowContextV1>> {
        self.context.as_ref()
    }
    pub(crate) fn verify_next(
        &mut self,
        binding: &BftSignerBindingV3,
        provider: &str,
        record: &BftSuccessorWindowStoredRequestV1,
        original: &BftSignerStateV3,
    ) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
        record.validate(binding, provider)?;
        super::successor_state::validate_manifest_predecessor(
            binding,
            &record.manifest,
            &record.request.intent.body.checkpoint,
            self.records.last(),
        )?;
        let expected = &record.request.intent.body;
        let previous_minimum = self
            .context
            .as_ref()
            .map(|c| c.checkpoint.body().minimum())
            .transpose()?;
        if original.revision != expected.expected_revision
            || original.state_digest != expected.expected_state_digest
            || original.successor_window.as_ref().is_some_and(|w| {
                w.reservation.is_some()
                    || w.requests != self.records
                    || w.minimum != previous_minimum
            })
            || (!self.records.is_empty() && original.successor_window.is_none())
            || self
                .records
                .last()
                .is_some_and(|r| r.request.status != BftSuccessorWindowStatusV1::Adopted)
        {
            return Err(invalid(
                "successor original State differs from authenticated predecessor",
            ));
        }
        let verified = super::state::validate_state_with_active_context(
            binding,
            Some(provider),
            original,
            self.context.clone(),
        )?;
        record
            .original_public_state
            .validate_bytes(&super::crypto::encode_bounded(
                original,
                super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
            )?)?;
        super::successor_public_archive::validate_actual_public_manifest(
            &record.manifest,
            original,
        )?;
        let token = expected.checkpoint.bind_verified_state(
            binding,
            &verified,
            self.context.as_ref().map(|c| c.checkpoint()),
        )?;
        if record.request.status == BftSuccessorWindowStatusV1::Adopted {
            let historical = match &verified.active_context {
                Some(c) => c.historical_state(original)?,
                None => original.clone(),
            };
            self.context = Some(Arc::new(VerifiedBftActiveWindowContextV1 {
                checkpoint: token,
                history: historical.history,
                applied_installs: historical.applied_installs,
            }));
        }
        self.records.push(record.clone());
        Ok(verified)
    }
}

/// A ready active base authenticated by the complete original archive chain.
/// Not serializable and never constructed from a checkpoint summary alone.
#[derive(Debug, Clone)]
pub struct VerifiedBftSignerActiveWindowV1 {
    provider_public_key: String,
    active_segment_origin: BftSuccessorWindowArchiveRefV1,
    original: BftSuccessorWindowStoredRequestV1,
    schedule: oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3,
    minimum: BftSuccessorWindowMinimumV1,
}
impl VerifiedBftSignerActiveWindowV1 {
    pub fn active_segment_origin(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.active_segment_origin
    }
    pub fn provider_public_key(&self) -> &str {
        &self.provider_public_key
    }
    pub fn binding_digest(&self) -> &str {
        &self
            .original
            .request
            .intent
            .body
            .checkpoint
            .source
            .binding_digest
    }

    pub fn original_request(&self) -> &BftSuccessorWindowStoredRequestV1 {
        &self.original
    }
    pub fn base_schedule(&self) -> &oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3 {
        &self.schedule
    }
    pub fn minimum(&self) -> &BftSuccessorWindowMinimumV1 {
        &self.minimum
    }
    pub(crate) fn from_verified(
        state: &VerifiedBftSignerStateV3,
        provider_public_key: &str,
    ) -> Result<Option<Self>, BftSignerErrorV3> {
        if state
            .state
            .successor_window
            .as_ref()
            .is_some_and(|w| w.blocks_writes())
        {
            return Err(invalid(
                "active window unavailable until protected installation resolves",
            ));
        }
        let Some(context) = &state.active_context else {
            return Ok(None);
        };
        context.validate_anchor(&state.state)?;
        let window = state
            .state
            .successor_window
            .as_ref()
            .ok_or_else(|| invalid("active window missing protected records"))?;
        if window.blocks_writes() {
            return Err(invalid(
                "active window unavailable until protected installation resolves",
            ));
        }
        let record = window
            .requests
            .last()
            .ok_or_else(|| invalid("active original request missing"))?;
        if record.request.status != BftSuccessorWindowStatusV1::Adopted
            || record.request.intent.body.checkpoint.digest()? != context.checkpoint.digest()
        {
            return Err(invalid(
                "active original request differs from verified base",
            ));
        }
        Ok(Some(Self {
            provider_public_key: provider_public_key.into(),
            active_segment_origin: record
                .request
                .adopted_evidence
                .as_ref()
                .and_then(|e| e.active_segment_origin.clone())
                .ok_or_else(|| invalid("active window Origin commitment missing"))?,
            original: record.clone(),
            schedule: context.checkpoint.schedule().clone(),
            minimum: context.checkpoint.body().minimum()?,
        }))
    }
}
