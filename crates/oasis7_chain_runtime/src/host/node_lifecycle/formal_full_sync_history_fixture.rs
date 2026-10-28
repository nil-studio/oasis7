//! Signed-cut history fixture shared by full-sync route and lease tests.
use super::*;

#[derive(Clone)]
pub(super) struct FullSyncCutFixture {
    pub(super) world: World,
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) material: ClosedRecord,
    pub(super) proof: FormalQualifiedCommitProofV1,
}

pub(super) struct FullSyncResolver {
    pub(super) cuts: Mutex<Vec<FullSyncCutFixture>>,
    pub(super) advance_after_material: Mutex<Option<(Arc<PublishedReadState>, FullSyncCutFixture)>>,
    pub(super) transition_steps: Mutex<BTreeMap<String, Vec<FormalReadStepV1>>>,
}

impl FullSyncResolver {
    fn index_of(
        cuts: &[FullSyncCutFixture],
        candidate: &VerifiedFormalReadAnchorV1,
    ) -> Option<usize> {
        cuts.iter()
            .position(|cut| cut.anchor.checkpoint() == candidate.checkpoint())
    }

    pub(super) fn push(&self, cut: FullSyncCutFixture) {
        self.cuts.lock().unwrap().push(cut);
    }

    pub(super) fn advance_after_next_material(
        &self,
        published: Arc<PublishedReadState>,
        cut: FullSyncCutFixture,
    ) {
        *self.advance_after_material.lock().unwrap() = Some((published, cut));
    }
}

impl FormalLiveHistoryResolver for FullSyncResolver {
    fn origin_publication(
        &self,
        request: &FormalReadAnchorRefV1,
    ) -> Result<FormalReadPublication, FormalReadError> {
        let cuts = self.cuts.lock().map_err(|_| {
            FormalReadError::Internal("full-sync fixture history lock poisoned".into())
        })?;
        let cut = cuts
            .iter()
            .find(|cut| {
                cut.anchor.head().record().position == request.position
                    && cut.anchor.head().head().record_id == request.record_id
                    && cut.anchor.authority().authority_id().ok().as_deref()
                        == Some(request.active_authority_id.as_str())
            })
            .ok_or_else(|| {
                FormalReadError::HistoryUnavailable(
                    "HistoryUnavailable: full-sync fixture cut is unknown".into(),
                )
            })?;
        FormalReadPublication::new(cut.anchor.clone(), cut.world.clone())
    }

    fn fixed_publication(
        &self,
        _commit: &CommitRef,
    ) -> Result<FormalReadPublication, FormalReadError> {
        Err(FormalReadError::HistoryUnavailable(
            "HistoryUnavailable: the full-sync fixture has only its pinned cut".into(),
        ))
    }

    fn forward_publication(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        live: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadPublication, FormalReadError> {
        let cuts = self.cuts.lock().map_err(|_| {
            FormalReadError::Internal("full-sync fixture history lock poisoned".into())
        })?;
        let origin_index = Self::index_of(&cuts, origin).ok_or_else(|| {
            FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture origin is unknown".into(),
            )
        })?;
        let live_index = Self::index_of(&cuts, live).ok_or_else(|| {
            FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture live cut is unknown".into(),
            )
        })?;
        if max_steps == 0 || max_steps > MAX_FORMAL_HISTORY_STEPS || live_index < origin_index {
            return Err(FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture has no forward history".into(),
            ));
        }
        let origin_position = cuts[origin_index].anchor.head().record().position;
        let live_position = cuts[live_index].anchor.head().record().position;
        let next = cuts
            .iter()
            .enumerate()
            .skip(origin_index + 1)
            .take(live_index.saturating_sub(origin_index))
            .find(|(_, cut)| {
                let position = cut.anchor.head().record().position;
                (position > origin_position
                    || cut.anchor.checkpoint() != cuts[origin_index].anchor.checkpoint())
                    && position <= live_position
                    && position - origin_position <= max_steps as u64
            })
            .map(|(_, cut)| cut)
            .ok_or_else(|| {
                FormalReadError::HistoryUnavailable(
                    "HistoryUnavailable: full-sync fixture has no bounded next cut".into(),
                )
            })?;
        FormalReadPublication::new(next.anchor.clone(), next.world.clone())
    }

    fn history_page(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        target: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadHistoryV1, FormalReadError> {
        let cuts = self.cuts.lock().map_err(|_| {
            FormalReadError::Internal("full-sync fixture history lock poisoned".into())
        })?;
        let origin_index = Self::index_of(&cuts, origin).ok_or_else(|| {
            FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture origin is unknown".into(),
            )
        })?;
        let target_index = Self::index_of(&cuts, target).ok_or_else(|| {
            FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture target is unknown".into(),
            )
        })?;
        let start = cuts[origin_index].anchor.head().record().position;
        let end = cuts[target_index].anchor.head().record().position;
        if max_steps > MAX_FORMAL_HISTORY_STEPS
            || target_index < origin_index
            || end < start
            || end - start > max_steps as u64
        {
            return Err(FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture has no history page".into(),
            ));
        }
        let transitions = self.transition_steps.lock().map_err(|_| {
            FormalReadError::Internal("full-sync fixture transitions lock poisoned".into())
        })?;
        let mut steps = Vec::new();
        for cut in &cuts[origin_index + 1..=target_index] {
            if let Some(transition) = transitions.get(
                &cut.anchor
                    .checkpoint()
                    .digest()
                    .map_err(FormalReadError::from)?,
            ) {
                steps.extend(transition.iter().cloned());
            } else {
                steps.push(FormalReadStepV1::Commit(Box::new(cut.proof.clone())));
            }
        }
        Ok(FormalReadHistoryV1 {
            schema_version: 1,
            steps,
        })
    }

    fn full_sync_material(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
    ) -> Result<ClosedRecord, FormalReadError> {
        let cuts = self.cuts.lock().map_err(|_| {
            FormalReadError::Internal("full-sync fixture history lock poisoned".into())
        })?;
        let index = Self::index_of(&cuts, anchor).ok_or_else(|| {
            FormalReadError::HistoryUnavailable(
                "HistoryUnavailable: full-sync fixture cut differs".into(),
            )
        })?;
        let material = cuts[index].material.clone();
        drop(cuts);
        if let Some((published, next)) = self
            .advance_after_material
            .lock()
            .map_err(|_| {
                FormalReadError::Internal("full-sync fixture advance hook lock poisoned".into())
            })?
            .take()
        {
            self.push(next.clone());
            published
                .publish(next.world, next.anchor)
                .map_err(FormalReadError::Integrity)?;
        }
        Ok(material)
    }
}
