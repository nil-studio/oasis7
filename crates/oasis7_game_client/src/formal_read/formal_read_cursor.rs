//! Bounded historical evidence. Saved frames confer no fresh observation and
//! are always replay-verified from the currently configured external root.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct CursorJournal<P: FormalReadProjection> {
    origin: String,
    frames: Vec<FormalReadCursorFrameV1<P>>,
    bytes: usize,
    unavailable: bool,
}
impl<P: FormalReadProjection> CursorJournal<P> {
    pub(super) fn invalidate(&mut self) {
        self.unavailable = true;
        self.frames.clear();
        self.bytes = 0;
    }
    pub(super) fn new(anchor: &VerifiedFormalReadAnchorV1) -> Result<Self, String> {
        Ok(Self {
            origin: anchor
                .checkpoint()
                .digest()
                .map_err(|error| error.to_string())?,
            frames: Vec::new(),
            bytes: 0,
            unavailable: false,
        })
    }
    pub(super) fn record(
        &mut self,
        request: &ReadWorldViewRequest,
        response: &ReadWorldViewResponse<P>,
    ) -> Result<(), String> {
        if self.unavailable || request.scope_id != "public" || request.fixed_commit.is_some() {
            return Ok(());
        }
        if self
            .frames
            .iter()
            .any(|frame| frame.request.observation_nonce == request.observation_nonce)
        {
            self.invalidate();
            return Ok(());
        }
        if self
            .frames
            .last()
            .is_some_and(|last| last.response.version == response.version)
            && FormalReadHistoryV1::decode_hex(
                &response
                    .formal_proof
                    .as_ref()
                    .ok_or("formal cursor proof absent")?
                    .history_cbor_hex,
            )?
            .steps
            .is_empty()
        {
            return Ok(());
        }
        let frame = FormalReadCursorFrameV1 {
            request: request.clone(),
            response: response.clone(),
        };
        let bytes = serde_json::to_vec(&frame)
            .map_err(|error| error.to_string())?
            .len();
        let next_bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or("formal cursor byte overflow")?;
        if self.frames.len() == MAX_FORMAL_CURSOR_FRAMES || next_bytes > MAX_FORMAL_CURSOR_BYTES {
            self.unavailable = true;
            // Retain the last complete bounded journal for diagnostics. It does
            // not authenticate the new active cut, so export must fail closed.
            return Ok(()); // Live verification continues; portable export fails closed.
        }
        self.bytes = next_bytes;
        self.frames.push(frame);
        Ok(())
    }
}

impl<P: FormalReadProjection> FormalReadVerifier<P> {
    pub fn adopt_rebased_cursor(
        &mut self,
        mut next: Self,
        expected: &FormalReadReplayCursorV1,
        minimum: &CommitRef,
    ) -> Result<(), String> {
        if &self.replay_cursor()? != expected {
            return Err("HistoryUnavailable: cursor rebase lost cache CAS".into());
        }
        if next.origin != self.origin || !next.contains_verified_anchor(&self.anchor_ref()?)? {
            return Err(
                "HistoryUnavailable: cursor rebase cannot prove exact current ancestry".into(),
            );
        }
        let target = next.export_cursor()?.commit;
        let request = ReadWorldViewRequest {
            observation_nonce: Some("00".repeat(32)),
            formal_anchor: Some(self.anchor_ref()?),
            contract_version: 1,
            world: minimum.world.clone(),
            scope_id: "public".into(),
            min_commit: Some(minimum.clone()),
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        self.verify_minimum(&request, &target)?;
        let old_minimum = self
            .history
            .get(&minimum.position)
            .ok_or("HistoryUnavailable: rebase minimum ancestry absent")?;
        let minimum_anchor = FormalReadAnchorRefV1 {
            position: minimum.position,
            record_id: old_minimum.head().head().record_id,
            active_authority_id: old_minimum
                .authority()
                .authority_id()
                .map_err(|e| e.to_string())?,
        };
        if !next.contains_verified_anchor(&minimum_anchor)? {
            return Err(
                "HistoryUnavailable: cursor rebase omits authenticated minimum ancestry".into(),
            );
        }
        // This binding comes from the previous independently verified public
        // projection, and its exact canonical ancestry was reverified above.
        next.proved_commits
            .insert(minimum.position, minimum.clone());
        *self = next;
        Ok(())
    }
    pub fn is_fresh_cursor_target(&self, external: &Self) -> Result<bool, String> {
        Ok(self.replay_cursor()? == external.replay_cursor()?
            && self.proved_commits.is_empty()
            && self.qualified_commits.is_empty()
            && self.cursor.frames.is_empty()
            && !self.cursor.unavailable)
    }
    pub fn export_cursor(&self) -> Result<FormalReadCursorBundleV1<P>, String> {
        if self.cursor.unavailable {
            return Err("HistoryUnavailable: formal cursor evidence budget exhausted; configure an independently pinned later root".into());
        }
        if self.cursor.origin
            != self
                .origin_checkpoint
                .digest()
                .map_err(|error| error.to_string())?
            || self
                .cursor
                .frames
                .first()
                .is_none_or(|frame| frame.request.formal_anchor.as_ref() != Some(&self.origin))
        {
            return Err(
                "HistoryUnavailable: cursor frames do not start at the exact external origin"
                    .into(),
            );
        }
        let commit = self
            .cursor
            .frames
            .last()
            .ok_or("HistoryUnavailable: no independently verified public cursor frames")?
            .response
            .version
            .commit
            .clone();
        if commit.execution_block_hash != self.current.head().head().record_id
            || commit.position != self.current.head().record().position
        {
            return Err(
                "HistoryUnavailable: cursor has no proved current public projection".into(),
            );
        }
        let bundle = FormalReadCursorBundleV1 {
            schema_version: 1,
            external_checkpoint_digest: self.cursor.origin.clone(),
            commit,
            frames: self.cursor.frames.clone(),
        };
        bundle
            .validate_structure()
            .map_err(|error| format!("HistoryUnavailable: {error}"))?;
        Ok(bundle)
    }
    pub fn restore_cursor(
        trust: &InitialActivationTrustV1,
        external_anchor: VerifiedFormalReadAnchorV1,
        external_digest: &str,
        bundle: &FormalReadCursorBundleV1<P>,
        reader_public_key: &str,
    ) -> Result<Self, String> {
        bundle
            .validate_structure()
            .map_err(|error| error.to_string())?;
        if external_anchor
            .checkpoint()
            .digest()
            .map_err(|error| error.to_string())?
            != external_digest
            || bundle.external_checkpoint_digest != external_digest
        {
            return Err("formal cursor differs from CURRENT externally pinned root".into());
        }
        let mut next = Self::new(trust, external_anchor)?;
        let mut nonces = std::collections::BTreeSet::new();
        for frame in &bundle.frames {
            let nonce = frame
                .request
                .observation_nonce
                .as_ref()
                .ok_or("formal cursor nonce absent")?;
            if !nonces.insert(nonce) {
                return Err("formal cursor repeats an observation frame nonce".into());
            }
            let count = next.cursor.frames.len();
            next.verify_view(&frame.response, &frame.request, reader_public_key)?;
            if next.cursor.frames.len() != count + 1 {
                return Err("formal cursor repeats or exceeds bounded evidence".into());
            }
        }
        if next.export_cursor()?.commit != bundle.commit {
            return Err("formal cursor final binding differs from replay-verified frames".into());
        }
        Ok(next)
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    type FormalReadVerifier = super::super::FormalReadVerifier<
        oasis7_client_api::world_read_observation::WorldReadObservationV1,
    >;

    #[test]
    fn cursor_budget_exhaustion_retains_prior_frames_and_refuses_new_cut_export() {
        let (world, trust, anchor, _) = super::super::tests::fixtures::protocol_initial_fixture();
        let (mut request, mut reply) = super::super::tests::response(
            &world,
            &anchor,
            FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![],
            },
        );
        let mut verifier = FormalReadVerifier::new(&trust, anchor).unwrap();
        for index in 0..MAX_FORMAL_CURSOR_FRAMES {
            request.observation_nonce = Some(format!("{index:064x}"));
            reply.version.projection_revision = format!("budget-{index}");
            verifier.cursor.record(&request, &reply).unwrap();
        }
        let old = verifier.cursor.frames.clone();
        request.observation_nonce = Some("ff".repeat(32));
        reply.version.projection_revision = "new-cut".into();
        verifier.cursor.record(&request, &reply).unwrap();
        assert!(
            verifier
                .export_cursor()
                .unwrap_err()
                .contains("budget exhausted")
        );
        assert_eq!(
            serde_json::to_value(&verifier.cursor.frames).unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        assert_eq!(verifier.cursor.frames.len(), MAX_FORMAL_CURSOR_FRAMES);

        let mut byte_journal = CursorJournal::new(&verifier.current).unwrap();
        byte_journal.record(&request, &reply).unwrap();
        let old = byte_journal.frames.clone();
        byte_journal.bytes = MAX_FORMAL_CURSOR_BYTES;
        request.observation_nonce = Some("ee".repeat(32));
        reply.version.projection_revision = "another-cut".into();
        byte_journal.record(&request, &reply).unwrap();
        assert!(byte_journal.unavailable);
        assert_eq!(
            serde_json::to_value(&byte_journal.frames).unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        assert_eq!(byte_journal.bytes, MAX_FORMAL_CURSOR_BYTES);
    }
}
