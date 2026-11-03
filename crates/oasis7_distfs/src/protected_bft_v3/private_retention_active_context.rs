//! Exact first-C0 historical ownership and authenticated admission prefixes.
//! These borrowed capabilities are not wire values or external read grants.
use super::*;
use crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionSnapshotV1 as Operation;
use crate::protected_bft_v3::private_retention_input::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input,
};
use crate::protected_bft_v3::private_retention_seal_context::BftPrivateRetentionHistoricalInputContextV1 as Historical;
use crate::protected_bft_v3::state::BftSignerStateV3 as State;
use crate::protected_bft_v3::successor_state::BftSuccessorWindowStoredRequestV1 as Request;
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3 as Schedule, FormalBftCertifiedHeaderV3 as Certificate,
};

pub struct BftPrivateRetentionHistoricalOwnedC0V1<'a> {
    context: &'a BftPrivateRetentionVerificationContextV1<'a>,
    binding: &'a BftSignerBindingV3,
    original: super::super::active_window::VerifiedBftSignerActiveWindowV1,
    state: Box<State>,
    operation: &'a Operation,
}
pub struct BftPrivateRetentionAuthenticatedAdmissionParentV1<'a> {
    live: &'a BftSignerBackendSnapshotV3,
    state: &'a State,
    schedule: Schedule,
    before: Option<&'a BftPrivateRetentionInputInventoryV1>,
    historical: Option<Historical<'a>>,
    reservation: Option<
        super::super::private_retention_seal_context::BftPrivateRetentionReservationInputContextV1<
            'a,
        >,
    >,
}
impl<'a> BftPrivateRetentionVerificationContextV1<'a> {
    pub fn authenticated_original_c0<'b>(
        &'b self,
        binding: &'b BftSignerBindingV3,
        bytes: &[u8],
    ) -> Result<BftPrivateRetentionHistoricalOwnedC0V1<'b>, BftSignerErrorV3> {
        validate_context_state(binding, self.trusted, &self.current.signer, self.verified)?;
        let active = self.active_parent()?;
        let original = active.original_request();
        original.validate(binding, &self.trusted.provider_public_key)?;
        original.original_public_state.validate_bytes(bytes)?;
        let state: State = super::super::crypto::decode_bounded(
            bytes,
            super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?;
        if super::super::crypto::encode_bounded(
            &state,
            super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )? != bytes
            || state.active_window.is_some()
            || state.successor_window.is_some()
            || state.revision != original.request.intent.body.expected_revision
            || state.state_digest != original.request.intent.body.expected_state_digest
        {
            return Err(invalid(
                "historical C0 requires exact canonical first Source State",
            ));
        }
        state.validate(binding, &self.trusted.provider_public_key)?;
        let sealed = self
            .sealed
            .ok_or_else(|| invalid("historical C0 complete history unavailable"))?;
        sealed.validate_inventory_with_verified_state(binding, self.trusted, self.verified)?;
        let head = state
            .private_retention
            .as_ref()
            .ok_or_else(|| invalid("historical C0 coverage missing"))?;
        let mut matches = sealed
            .history
            .values()
            .filter(|op| &op.retention.receipt.body.head == head);
        let operation = matches
            .next()
            .ok_or_else(|| invalid("historical C0 original head missing"))?;
        if matches.next().is_some()
            || operation.signer.state.revision > state.revision
            || (operation.signer.state.revision == state.revision
                && operation.signer.state != state)
        {
            return Err(invalid("historical C0 original operation differs"));
        }
        Ok(BftPrivateRetentionHistoricalOwnedC0V1 {
            context: self,
            binding,
            original: active,
            state: Box::new(state),
            operation,
        })
    }
    pub fn authenticated_current_admission_parent<'b>(
        &'b self,
        binding: &'b BftSignerBindingV3,
    ) -> Result<BftPrivateRetentionAuthenticatedAdmissionParentV1<'b>, BftSignerErrorV3> {
        validate_context_state(binding, self.trusted, &self.current.signer, self.verified)?;
        self.active_parent()?;
        Ok(BftPrivateRetentionAuthenticatedAdmissionParentV1 {
            live: &self.current.signer,
            state: &self.current.signer.state,
            schedule: self.current_schedule()?.clone(),
            before: Some(&self.current.inputs),
            historical: None,
            reservation: None,
        })
    }
    pub fn authenticated_original_admission_parent<'b>(
        &'b self,
        binding: &'b BftSignerBindingV3,
        key: &super::super::private_retention::BftPrivateRetentionKeyV1,
    ) -> Result<BftPrivateRetentionAuthenticatedAdmissionParentV1<'b>, BftSignerErrorV3> {
        self.active_parent()?;
        let historical = self
            .original_replay_context(binding)?
            .original_reservation(key)?;
        let state = &historical.reservation().signer.state;
        let verified = self
            .verified
            .ok_or_else(|| invalid("historical prefix verified context missing"))?;
        let original = super::super::state::validate_state_with_active_context(
            binding,
            Some(&self.trusted.provider_public_key),
            state,
            verified.active_context.clone(),
        )?;
        if state.active_window != self.current.signer.state.active_window
            || state.active_window.is_none()
        {
            return Err(invalid(
                "historical admission prefix is not this Active window",
            ));
        }
        let before = historical.predecessor().map(|op| &op.inputs);
        Ok(BftPrivateRetentionAuthenticatedAdmissionParentV1 {
            live: &self.current.signer,
            state,
            schedule: original.schedule,
            before,
            historical: None,
            reservation: Some(historical),
        })
    }
}
impl<'a> BftPrivateRetentionHistoricalOwnedC0V1<'a> {
    pub fn binding(&self) -> &BftSignerBindingV3 {
        self.binding
    }
    pub fn attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        self.context.trusted
    }
    pub fn live_signer(&self) -> &BftSignerBackendSnapshotV3 {
        &self.context.current.signer
    }
    pub fn original_state(&self) -> &State {
        &self.state
    }
    pub fn original_request(&self) -> &Request {
        self.original.original_request()
    }
    pub fn ledger(&self) -> &BftPrivateRetentionLedgerV1 {
        &self.operation.retention
    }
    pub fn original_ready(
        &self,
        key: &super::super::private_retention::BftPrivateRetentionKeyV1,
    ) -> Result<Historical<'a>, BftSignerErrorV3> {
        let historical = self.context.original_ready(self.binding, key)?;
        if historical.ready().retention.receipt.body.head.sequence
            > self.operation.retention.receipt.body.head.sequence
        {
            return Err(invalid("historical C0 cannot authorize later original"));
        }
        historical.restrict_to_sequence(self.operation.retention.receipt.body.head.sequence)
    }
    pub fn owned_original(&self, reference: &InputRef) -> Result<&Input, BftSignerErrorV3> {
        let historical = self.original_ready(&reference.key)?;
        let input = historical
            .ready()
            .inputs
            .get(&reference.admission_digest)
            .ok_or_else(|| invalid("historical C0 original input missing"))?;
        reference.validate_input(input)?;
        Ok(input)
    }
}
impl BftPrivateRetentionAuthenticatedAdmissionParentV1<'_> {
    pub fn live_signer(&self) -> &BftSignerBackendSnapshotV3 {
        self.live
    }
    pub fn state(&self) -> &State {
        self.state
    }
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }
    pub fn history(&self) -> &[Certificate] {
        &self.state.history
    }
    pub fn before_inventory(&self) -> Option<&BftPrivateRetentionInputInventoryV1> {
        self.before
    }
    pub fn reservation_input(
        &self,
    ) -> Option<
        &super::super::private_retention_seal_context::BftPrivateRetentionReservationInputContextV1<
            '_,
        >,
    > {
        self.reservation.as_ref()
    }
    pub fn historical_input(&self) -> Option<&Historical<'_>> {
        self.historical.as_ref()
    }
}
