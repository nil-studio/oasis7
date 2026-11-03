//! Bounded collection followed by oldest-to-newest actual replay.
use super::*;
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(in super::super) fn verify_original_closure_with_origin(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        deadline: Instant,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        self.verify_original_closure_with_origin_access(
            original,
            deadline,
            &ProtectedInputAccessV1::External,
        )
    }
    pub(super) fn verify_original_closure_with_origin_access(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        self.verify_original_closure_boxed_access(original, deadline, access)
            .map(|base| *base)
    }
    /// Default-stack provider callers retain only pointers while World replays.
    /// The State decoder and later original attachment are separate phases.
    #[inline(never)]
    pub(super) fn verify_original_closure_boxed_access(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<Box<VerifiedPrivateSuccessorBaseV1>, String> {
        let state = self.load_original_replay_state(original, deadline)?;
        super::successor_install::preflight_pointer(original)?;
        let base = self.replay_original_into_box(original, &state, deadline, access)?;
        Ok(Self::attach_original_after_replay(base, original))
    }
    #[inline(never)]
    fn load_original_replay_state(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        deadline: Instant,
    ) -> Result<Box<BftSignerStateV3>, String> {
        let bytes = self.load(&original.original_public_state, MAX_CLOSURE, deadline)?;
        let state: Box<BftSignerStateV3> = serde_cbor::from_slice(&bytes).map_err(err)?;
        let body = &original.request.intent.body;
        if encode_protocol(state.as_ref(), MAX_CLOSURE)? != bytes
            || state.revision != body.expected_revision
            || state.state_digest != body.expected_state_digest
        {
            return Err("successor immutable original S differs from protected request".into());
        }
        Ok(state)
    }
    #[inline(never)]
    fn replay_original_into_box(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        state: &BftSignerStateV3,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<Box<VerifiedPrivateSuccessorBaseV1>, String> {
        self.verify_closure_boxed_with_origin_access(
            &original.request.intent.body.checkpoint,
            state,
            &original.original_public_state,
            deadline,
            access,
        )
    }
    #[inline(never)]
    fn boxed_replay_state(state: &BftSignerStateV3) -> Box<BftSignerStateV3> {
        Box::new(state.clone())
    }

    #[inline(never)]
    fn attach_original_after_replay(
        mut base: Box<VerifiedPrivateSuccessorBaseV1>,
        original: &BftSuccessorWindowStoredRequestV1,
    ) -> Box<VerifiedPrivateSuccessorBaseV1> {
        base.original = Some(original.clone());
        base
    }
    pub(super) fn verify_closure_with_origin(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        self.verify_closure_with_origin_access(
            checkpoint,
            public_state,
            expected_public_ref,
            deadline,
            &ProtectedInputAccessV1::External,
        )
    }
    pub(super) fn verify_closure_with_origin_access(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        self.verify_closure_boxed_with_origin_access(
            checkpoint,
            public_state,
            expected_public_ref,
            deadline,
            access,
        )
        .map(|base| *base)
    }

    #[inline(never)]
    fn verify_closure_boxed_with_origin_access(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<Box<VerifiedPrivateSuccessorBaseV1>, String> {
        // Collect canonical original States before invoking any actual World
        // replay. No predecessor verification call is nested on the native stack.
        let mut chain = Vec::new();
        let mut state = Self::boxed_replay_state(public_state);
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
            let original: Box<BftSignerStateV3> = serde_cbor::from_slice(&bytes).map_err(err)?;
            if encode_protocol(original.as_ref(), MAX_CLOSURE)? != bytes
                || original.revision != body.expected_revision
                || original.state_digest != body.expected_state_digest
            {
                return Err("private predecessor original State differs".into());
            }
            state = original.clone();
            chain.push((record, original));
        }
        let mut predecessor = None;
        for (record, state) in chain.into_iter().rev() {
            let mut verified = self.verify_one_closure_with_access(
                &record.request.intent.body.checkpoint,
                &state,
                &record.original_public_state,
                deadline,
                predecessor.as_deref(),
                access,
            )?;
            verified.original = Some(record);
            predecessor = Some(verified);
        }
        self.verify_one_closure_with_access(
            checkpoint,
            public_state,
            expected_public_ref,
            deadline,
            predecessor.as_deref(),
            access,
        )
    }
}
