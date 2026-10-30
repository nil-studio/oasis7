//! Bounded collection followed by oldest-to-newest actual replay.
use super::*;
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(in super::super) fn verify_original_closure_with_origin(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        deadline: Instant,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        let bytes = self.load(&original.original_public_state, MAX_CLOSURE, deadline)?;
        let state: BftSignerStateV3 = serde_cbor::from_slice(&bytes).map_err(err)?;
        let body = &original.request.intent.body;
        if encode_protocol(&state, MAX_CLOSURE)? != bytes
            || state.revision != body.expected_revision
            || state.state_digest != body.expected_state_digest
        {
            return Err("successor immutable original S differs from protected request".into());
        }
        super::successor_install::preflight_pointer(original)?;
        let mut base = self.verify_closure_with_origin(
            &body.checkpoint,
            &state,
            &original.original_public_state,
            deadline,
        )?;
        base.original = Some(original.clone());
        Ok(base)
    }
    pub(super) fn verify_closure_with_origin(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        // Collect canonical original States before invoking any actual World
        // replay. No predecessor verification call is nested on the native stack.
        let mut chain = Vec::new();
        let mut state = public_state.clone();
        let mut seen = BTreeSet::new();
        let mut bytes_read = 0usize;
        while let Some(active) = &state.active_window {
            live(deadline)?;
            if chain.len() >= 64 || !seen.insert(active.checkpoint_digest.clone()) {
                return Err("private archive predecessor chain capacity/cycle".into());
            }
            let record = state
                .successor_window
                .as_ref()
                .and_then(|window| window.requests.last())
                .ok_or("private active predecessor record absent")?
                .clone();
            let body = &record.request.intent.body;
            if record.request.status
                != oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::Adopted
                || body.checkpoint.digest().map_err(err)? != active.checkpoint_digest
            {
                return Err("private active predecessor identity differs".into());
            }
            let bytes = self.load(&record.original_public_state, MAX_CLOSURE, deadline)?;
            bytes_read = bytes_read
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_CLOSURE)
                .ok_or("private predecessor original State operation capacity")?;
            let original: BftSignerStateV3 = serde_cbor::from_slice(&bytes).map_err(err)?;
            if encode_protocol(&original, MAX_CLOSURE)? != bytes
                || original.revision != body.expected_revision
                || original.state_digest != body.expected_state_digest
            {
                return Err("private predecessor original State differs".into());
            }
            chain.push((record, original.clone()));
            state = original;
        }
        let mut predecessor = None;
        for (record, state) in chain.into_iter().rev() {
            let mut verified = self.verify_one_closure(
                &record.request.intent.body.checkpoint,
                &state,
                &record.original_public_state,
                deadline,
                predecessor.as_ref(),
            )?;
            verified.original = Some(record);
            predecessor = Some(verified);
        }
        self.verify_one_closure(
            checkpoint,
            public_state,
            expected_public_ref,
            deadline,
            predecessor.as_ref(),
        )
    }
}
