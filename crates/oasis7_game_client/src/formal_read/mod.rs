//! Independent light proof consumer. Service signatures authenticate transport;
//! external anchors, authority proof and projection inclusion authenticate reads.
#[path = "formal_read_cursor.rs"]
mod cursor;
#[path = "formal_read_history.rs"]
mod history;
#[cfg(test)]
#[path = "formal_read_tests.rs"]
mod tests;
use crate::authority;
pub use history::*;
use oasis7_client_api::formal_projection::{ProjectionAudienceV1, ProjectionInclusionV1};
use oasis7_client_api::world_service::*;
use oasis7_formal_protocol::controlled_authority::formal::*;
use std::collections::BTreeMap;

/// Projection schema validation belongs to its owning client/domain adapter.
/// Proof metadata and inclusion are always checked against serialized `Self`,
/// never values supplied by this callback.
pub trait FormalReadProjection:
    serde::Serialize + serde::de::DeserializeOwned + Clone + std::fmt::Debug
{
    fn read_metadata(
        &self,
    ) -> Result<oasis7_client_api::world_service_read::WorldServiceProjectionMetadataV1, String>
    {
        serde_json::from_value(serde_json::to_value(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    fn validate_contents(
        response: &ReadWorldViewResponse<Self>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String>;
}

#[derive(serde::Deserialize)]
struct ProjectionMetadata {
    runtime_binding: Option<serde_json::Value>,
    formal_commit_header: Option<FormalProjectionCommitHeaderV1>,
}

/// Private-constructed binding of the independently proved exact response.
#[derive(Debug, Clone)]
pub struct VerifiedFormalProjectionV1<P: FormalReadProjection> {
    commit: VerifiedFormalCommitV1,
    read_anchor: VerifiedFormalReadAnchorV1,
    binding_digest: String,
    reader_public_key: String,
    history_page: Option<FormalReadHistoryPageV1>,
    verifier: Box<FormalReadVerifier<P>>,
}
impl<P: FormalReadProjection> VerifiedFormalProjectionV1<P> {
    pub fn commit(&self) -> &VerifiedFormalCommitV1 {
        &self.commit
    }
    pub fn read_anchor(&self) -> &VerifiedFormalReadAnchorV1 {
        &self.read_anchor
    }
    pub fn verifier(&self) -> &FormalReadVerifier<P> {
        &self.verifier
    }
    pub fn history_page(&self) -> Option<&FormalReadHistoryPageV1> {
        self.history_page.as_ref()
    }
    pub fn matches(
        &self,
        response: &ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        if self.binding_digest
            != authority::request_digest(
                "formal-read-verified-response-v1",
                &(request, response, &self.reader_public_key),
            )?
        {
            return Err("formal verified observation differs from exact response/request".into());
        }
        Ok(())
    }
    fn new(
        commit: VerifiedFormalCommitV1,
        read_anchor: VerifiedFormalReadAnchorV1,
        response: &ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        reader_public_key: &str,
        history_page: Option<FormalReadHistoryPageV1>,
        verifier: FormalReadVerifier<P>,
    ) -> Result<Self, String> {
        Ok(Self {
            commit,
            read_anchor,
            binding_digest: authority::request_digest(
                "formal-read-verified-response-v1",
                &(request, response, reader_public_key),
            )?,
            reader_public_key: reader_public_key.to_owned(),
            history_page,
            verifier: Box::new(verifier),
        })
    }
}
const MAX_CACHED_HEADS: usize = 128;
const MAX_VERIFIED_AUTHORITIES: usize = 1024;
const MAX_VERIFIED_RECOVERY_CONTEXTS: usize = MAX_VERIFIED_AUTHORITIES;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct QualifiedCommitKey {
    position: u64,
    record_id: String,
    proof_digest: String,
}
#[derive(Debug, Clone)]
struct VerifiedHistoricalQualificationV1 {
    proof: FormalQualifiedCommitProofV1,
    commit: VerifiedFormalCommitV1,
    anchor: VerifiedFormalReadAnchorV1,
    recovery: Option<VerifiedRecovery>,
    retained_context: Option<VerifiedActiveRecoveryContextV2>,
    origin: FormalReadAnchorRefV1,
}
#[derive(Debug, Clone)]
pub struct FormalReadVerifier<P: FormalReadProjection> {
    current: VerifiedFormalReadAnchorV1,
    pending: Option<VerifiedRecovery>,
    pending_retained_context: Option<VerifiedActiveRecoveryContextV2>,
    last_recovery: Option<VerifiedRecovery>,
    history: BTreeMap<u64, VerifiedFormalReadAnchorV1>,
    proved_commits: BTreeMap<u64, CommitRef>,
    qualified_commits: BTreeMap<QualifiedCommitKey, VerifiedHistoricalQualificationV1>,
    verified_authorities: BTreeMap<String, VerifiedAuthorityV1>,
    /// Opaque recovery permissions and their exact signed predecessor cuts,
    /// retained from canonical replay independently of the 128-head cache.
    verified_recovery_contexts: Vec<(VerifiedRecovery, VerifiedFormalCommitV1)>,
    origin: FormalReadAnchorRefV1,
    cursor: cursor::CursorJournal<P>,
    origin_checkpoint: FormalReadCheckpointV1,
    origin_qualification: Option<FormalReadOriginQualificationV2>,
    replay_history: Option<Vec<FormalReadStepV1>>,
    requires_v2_checkpoint: bool,
}
impl<P: FormalReadProjection> FormalReadVerifier<P> {
    pub fn new(
        trust: &InitialActivationTrustV1,
        anchor: VerifiedFormalReadAnchorV1,
    ) -> Result<Self, String> {
        let initial = verify_initial_activation(anchor.initial_certificate(), trust)
            .map_err(|error| error.to_string())?;
        if anchor.authority().activation_ref() != initial.activation_ref()
            || anchor.authority().policy() != initial.policy()
            || anchor.authority().definition().world != initial.definition().world
        {
            return Err("formal read anchor differs from external initial trust".into());
        }
        let mut history = BTreeMap::new();
        let cursor = cursor::CursorJournal::new(&anchor)?;
        history.insert(anchor.head().record().position, anchor.clone());
        let mut verified_authorities = BTreeMap::new();
        let origin_authority_id = anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?;
        verified_authorities.insert(origin_authority_id, anchor.authority().clone());
        let origin_checkpoint = anchor.checkpoint();
        let origin = FormalReadAnchorRefV1 {
            position: anchor.head().record().position,
            record_id: anchor.head().head().record_id,
            active_authority_id: anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        };
        Ok(Self {
            current: anchor,
            pending: None,
            pending_retained_context: None,
            last_recovery: None,
            history,
            proved_commits: BTreeMap::new(),
            qualified_commits: BTreeMap::new(),
            verified_authorities,
            verified_recovery_contexts: Vec::new(),
            origin,
            cursor,
            origin_checkpoint,
            origin_qualification: None,
            replay_history: Some(Vec::new()),
            requires_v2_checkpoint: false,
        })
    }
    pub fn anchor_ref(&self) -> Result<FormalReadAnchorRefV1, String> {
        Ok(FormalReadAnchorRefV1 {
            position: self.current.head().record().position,
            record_id: self.current.head().head().record_id,
            active_authority_id: self
                .current
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        })
    }
    pub fn current_anchor(&self) -> &VerifiedFormalReadAnchorV1 {
        &self.current
    }
    pub fn replay_cursor(&self) -> Result<FormalReadReplayCursorV1, String> {
        let checkpoint_digest = self
            .current
            .checkpoint()
            .digest()
            .map_err(|error| error.to_string())?;
        Ok(FormalReadReplayCursorV1 {
            schema_version: 1,
            anchor: self.anchor_ref()?,
            checkpoint_digest,
            last_recovery_digest: self
                .last_recovery
                .as_ref()
                .map(|recovery| recovery.certificate_digest().to_owned()),
            pending_recovery_digest: self
                .pending
                .as_ref()
                .map(|recovery| recovery.certificate_digest().to_owned()),
            requires_v2_checkpoint: self.requires_v2_checkpoint,
        })
    }
    pub fn replay_history_page(&mut self, page: &FormalReadHistoryPageV1) -> Result<(), String> {
        page.validate()?;
        if self.replay_cursor()? != page.start {
            return Err(
                "HistoryUnavailable: formal proof page does not continue exact cursor".into(),
            );
        }
        let mut next = self.clone();
        next.replay_history_steps(&page.history.steps)?;
        if !page.history.steps.is_empty() {
            next.cursor.invalidate();
        }
        if next.replay_cursor()? != page.end {
            return Err(
                "HistoryUnavailable: formal proof page end cursor differs from replay".into(),
            );
        }
        *self = next;
        Ok(())
    }
    pub fn checkpoint(&self) -> Result<FormalReadCheckpointV1, String> {
        if self.pending.is_some() || self.requires_v2_checkpoint {
            return Err("formal read state requires a replayed V2 checkpoint".into());
        }
        Ok(self.current.checkpoint())
    }
    pub fn checkpoint_v2(&self, origin_digest: &str) -> Result<FormalReadCheckpointV2, String> {
        self.checkpoint_v2_with_origin_qualification(
            origin_digest,
            self.origin_qualification.clone(),
        )
    }
    pub fn checkpoint_v2_with_origin_qualification(
        &self,
        origin_digest: &str,
        origin_qualification: Option<FormalReadOriginQualificationV2>,
    ) -> Result<FormalReadCheckpointV2, String> {
        if self
            .origin_checkpoint
            .digest()
            .map_err(|error| error.to_string())?
            != origin_digest
        {
            return Err("formal V2 checkpoint origin is not the configured V1 pin".into());
        }
        let steps = self.replay_history.as_ref().ok_or_else(|| {
            "HistoryUnavailable: V2 replay history exceeded retention bound".to_owned()
        })?;
        let checkpoint = FormalReadCheckpointV2 {
            schema_version: 2,
            origin: self.origin_checkpoint.clone(),
            origin_digest: origin_digest.to_owned(),
            origin_qualification,
            history: FormalReadHistoryV1 {
                schema_version: 1,
                steps: steps.clone(),
            },
            current: self.current.checkpoint(),
        };
        checkpoint.encode_hex()?;
        Ok(checkpoint)
    }
    pub fn from_fixed_checkpoint_v2(
        trust: &InitialActivationTrustV1,
        checkpoint: &FormalReadCheckpointV2,
        pinned_digest: &str,
        origin_digest: &str,
    ) -> Result<Self, String> {
        if checkpoint.digest()? != pinned_digest || checkpoint.origin_digest != origin_digest {
            return Err("externally pinned formal V2 checkpoint mismatch".into());
        }
        let mut verifier = if let Some(qualification) = &checkpoint.origin_qualification {
            // The outer V2 digest is an external pin over this complete base
            // and replay packet. The embedded V1 base is never trusted alone.
            let base = VerifiedFormalReadAnchorV1::from_fixed_pin(
                &qualification.base,
                &qualification.base_digest,
                trust,
            )
            .map_err(|error| error.to_string())?;
            let mut origin_history = Self::new(trust, base.clone())?;
            origin_history.replay_history_steps(&qualification.history.steps)?;
            if origin_history.current.checkpoint() != checkpoint.origin {
                return Err(
                    "formal V2 origin qualification replay differs from pinned origin".into(),
                );
            }
            let origin_record_id = origin_history.current.head().head().record_id.clone();
            let key = QualifiedCommitKey {
                position: qualification.origin_record.position,
                record_id: origin_record_id,
                proof_digest: qualification.qualified_proof_digest.clone(),
            };
            if !origin_history.qualified_commits.contains_key(&key) {
                if qualification.qualified_proof.decision.branch != FormalCompletionBranchV1::Normal
                {
                    return Err(
                        "HistoryUnavailable: non-Normal origin proof was not replayed".into(),
                    );
                }
                let authority = origin_history
                    .verified_authorities
                    .get(&qualification.origin_record.original_authority_id)
                    .ok_or_else(|| {
                        "HistoryUnavailable: origin Normal authority is not verified".to_owned()
                    })?;
                // A same-cut recovery can change the active authority while
                // leaving the externally pinned initial record untouched. In
                // that case the origin proof is qualified directly by the
                // exact fixed V1 base, including profile-v1 genesis records;
                // it is not being promoted from an untrusted current-head
                // projection. For any later origin, keep the stricter
                // profile-v2 historical qualification path below.
                let (anchor, commit) = if base.head().record().position == 1
                    && base.authority().definition().epoch == 1
                    && qualification.origin_record == *base.head().record()
                    && qualification.finalizing_authority_id
                        == base.head().finalizing_authority_id()
                    && origin_history.current.head().record() == base.head().record()
                {
                    let commit = verify_formal_qualified(
                        &qualification.qualified_proof,
                        base.authority(),
                        None,
                        None,
                    )
                    .map_err(|error| error.to_string())?;
                    if commit.record() != base.head().record()
                        || commit.finalizing_authority_id() != base.head().finalizing_authority_id()
                    {
                        return Err("formal V2 initial origin proof differs from fixed base".into());
                    }
                    (base.clone(), commit)
                } else {
                    let anchor = VerifiedFormalReadAnchorV1::from_verified_historical_normal_head(
                        &origin_history.current,
                        authority,
                        &qualification.qualified_proof,
                    )
                    .map_err(|error| error.to_string())?;
                    let commit = anchor.head().clone();
                    (anchor, commit)
                };
                origin_history.qualified_commits.insert(
                    key.clone(),
                    VerifiedHistoricalQualificationV1 {
                        proof: qualification.qualified_proof.clone(),
                        commit,
                        anchor,
                        recovery: None,
                        retained_context: None,
                        origin: origin_history.origin.clone(),
                    },
                );
            }
            let qualified = origin_history
                .qualified_commits
                .get(&key)
                .expect("origin qualification was replayed or seeded");
            if qualification.origin_record != *origin_history.current.head().record()
                || qualification.finalizing_authority_id
                    != origin_history.current.head().finalizing_authority_id()
                || qualified.commit.record() != &qualification.origin_record
                || qualified.commit.finalizing_authority_id()
                    != qualification.finalizing_authority_id
                || qualified.proof != qualification.qualified_proof
            {
                return Err("formal V2 origin qualification body/finalizer mismatch".into());
            }
            let mut verifier = Self::new(trust, origin_history.current.clone())?;
            verifier.pending = origin_history.pending.clone();
            verifier.pending_retained_context = origin_history.pending_retained_context.clone();
            verifier.last_recovery = origin_history.last_recovery.clone();
            verifier.history = origin_history.history.clone();
            verifier.proved_commits = origin_history.proved_commits.clone();
            verifier.qualified_commits = origin_history.qualified_commits.clone();
            let verifier_origin = verifier.origin.clone();
            for qualified in verifier.qualified_commits.values_mut() {
                qualified.origin = verifier_origin.clone();
            }
            verifier.verified_authorities = origin_history.verified_authorities.clone();
            verifier.verified_recovery_contexts = origin_history.verified_recovery_contexts.clone();
            verifier.requires_v2_checkpoint = origin_history.requires_v2_checkpoint;
            verifier.origin_checkpoint = checkpoint.origin.clone();
            verifier.origin_qualification = Some(qualification.clone());
            verifier
        } else {
            let origin = VerifiedFormalReadAnchorV1::from_fixed_pin(
                &checkpoint.origin,
                origin_digest,
                trust,
            )
            .map_err(|error| error.to_string())?;
            Self::new(trust, origin)?
        };
        verifier.replay_history_steps(&checkpoint.history.steps)?;
        if verifier.current.checkpoint() != checkpoint.current {
            return Err("formal V2 replay differs from pinned current head".into());
        }
        Ok(verifier)
    }
    /// Fixed historical reads use only a locally retained verified head. They
    /// never rewind the current cache or silently adopt a response checkpoint.
    pub fn fixed_anchor_ref(&self, commit: &CommitRef) -> Result<FormalReadAnchorRefV1, String> {
        let anchor = self
            .history
            .get(&commit.position)
            .filter(|anchor| anchor.head().head().record_id == commit.execution_block_hash)
            .ok_or_else(|| {
                "HistoryUnavailable: fixed formal head is outside verified cache".to_owned()
            })?;
        Ok(FormalReadAnchorRefV1 {
            position: commit.position,
            record_id: commit.execution_block_hash.clone(),
            active_authority_id: anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        })
    }
    /// Verify an old view through a current-root inclusion fact in an isolated
    /// verifier. The ordinary current cache is never replaced or rewound.
    pub fn verify_committed_historical_view(
        &self,
        historical: &VerifiedHistoricalCutInclusionV2,
        response: &ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        reader_public_key: &str,
    ) -> Result<VerifiedFormalProjectionV1<P>, String> {
        if self.pending.is_some()
            || request.scope_id != "public"
            || request.fixed_commit.is_some()
            || request.min_commit.is_some()
        {
            return Err("historical delegation requires completed current/public read".into());
        }
        if historical.delegating_head() != &self.current.head().head() {
            return Err("historical cut inclusion is not under the exact current head".into());
        }
        let cut = historical.cut();
        let record = &cut.parent_record;
        let record_id = record.record_id().map_err(|error| error.to_string())?;
        let proof_digest = formal_qualified_proof_digest(&cut.parent_qualified)
            .map_err(|error| error.to_string())?;
        let key = QualifiedCommitKey {
            position: record.position,
            record_id: record_id.clone(),
            proof_digest,
        };
        let anchor = if let Some(qualification) = self.qualified_commits.get(&key) {
            if qualification.proof != cut.parent_qualified
                || qualification.commit.record() != record
                || qualification.commit.head().record_id != record_id
                || qualification.anchor.head().record() != qualification.commit.record()
                || qualification.anchor.head().finalizing_authority_id()
                    != qualification.commit.finalizing_authority_id()
                || qualification.origin != self.origin
                || qualification.anchor.head().record().world != self.current.head().record().world
                || qualification.anchor.head().record().position
                    >= self.current.head().record().position
                || (qualification.proof.decision.branch != FormalCompletionBranchV1::Normal
                    && qualification.recovery.is_none())
                || qualification.recovery.as_ref().is_some_and(|recovery| {
                    recovery.body().schema_version == 2 && qualification.retained_context.is_none()
                })
            {
                return Err(
                    "historical qualification differs from retained verified lineage".into(),
                );
            }
            qualification.anchor.clone()
        } else if cut.parent_qualified.decision.branch == FormalCompletionBranchV1::Normal {
            let authority = self
                .verified_authorities
                .get(&record.original_authority_id)
                .ok_or_else(|| {
                    "HistoryUnavailable: historical Normal authority is outside verified lineage"
                        .to_owned()
                })?;
            let committed = historical
                .qualify_normal_with_verified_authority(&self.current, authority)
                .map_err(|error| error.to_string())?;
            VerifiedFormalReadAnchorV1::from_committed_historical_cut_with_verified_authority(
                &self.current,
                &committed,
            )
            .map_err(|error| error.to_string())?
        } else {
            let contexts: Vec<_> = self
                .verified_recovery_contexts
                .iter()
                .map(|(recovery, predecessor)| (recovery, predecessor))
                .collect();
            let committed = historical
                .qualify_with_verified_recovery_contexts(&self.current, &contexts)
                .map_err(|error| error.to_string())?;
            VerifiedFormalReadAnchorV1::from_committed_historical_recovery_cut(
                &self.current,
                &committed,
            )
            .map_err(|error| error.to_string())?
        };
        let expected_old_anchor = FormalReadAnchorRefV1 {
            position: record.position,
            record_id,
            active_authority_id: anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        };
        if request.formal_anchor.as_ref() != Some(&expected_old_anchor)
            || request.world.world_id != record.world.world_id
            || request.world.genesis_digest != record.world.genesis_digest
        {
            return Err("historical request differs from locally qualified public anchor".into());
        }
        let sidecar = response
            .formal_proof
            .as_ref()
            .ok_or("historical projection proof missing")?;
        if response.version.commit.position != anchor.head().record().position
            || response.version.commit.execution_block_hash != anchor.head().head().record_id
            || !FormalReadHistoryV1::decode_hex(&sidecar.history_cbor_hex)?
                .steps
                .is_empty()
        {
            return Err("historical projection differs from exact delegated cut".into());
        }
        let mut isolated = self.clone();
        isolated.current = anchor.clone();
        isolated.history.clear();
        isolated
            .history
            .insert(anchor.head().record().position, anchor);
        isolated.proved_commits.clear();
        isolated.qualified_commits.clear();
        isolated.verify_view(response, request, reader_public_key)
    }

    pub fn verify_view(
        &mut self,
        response: &ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        reader_public_key: &str,
    ) -> Result<VerifiedFormalProjectionV1<P>, String> {
        if response.execution_evidence_scope.is_some()
            || response.formal_profile.as_ref() != Some(&FormalReadProfileV1::controlled())
        {
            return Err(
                "formal read requires exact advertised formal profile and no engineering scope"
                    .into(),
            );
        }
        let sidecar = response
            .formal_proof
            .as_ref()
            .ok_or_else(|| "formal profile has no independent proof sidecar".to_owned())?;
        sidecar.validate().map_err(|error| error.to_string())?;
        if sidecar.profile != FormalReadProfileV1::controlled()
            || request.formal_anchor.as_ref() != Some(&sidecar.anchor)
        {
            return Err("formal proof does not start at caller's verified anchor".into());
        }
        request
            .validate_formal_shape()
            .map_err(|error| error.to_string())?;
        let mut shape_request = request.clone();
        shape_request.min_commit = None;
        response
            .validate(&shape_request)
            .map_err(|error| error.to_string())?;
        P::validate_contents(response, &shape_request)?;
        let mut next = self.clone();
        if let Some(fixed) = &request.fixed_commit {
            if sidecar.anchor != self.fixed_anchor_ref(fixed)? {
                return Err("HistoryUnavailable: fixed formal anchor mismatch".into());
            }
            let proof = FormalReadHistoryV1::decode_hex(&sidecar.history_cbor_hex)?;
            if !proof.steps.is_empty() {
                return Err("fixed read cannot replace cached history".into());
            }
            let anchor = self
                .history
                .get(&fixed.position)
                .expect("verified cached fixed head");
            verify_projection(response, request, reader_public_key, anchor)?;
            self.verify_minimum(request, &response.version.commit)?;
            return VerifiedFormalProjectionV1::<P>::new(
                anchor.head().clone(),
                anchor.clone(),
                response,
                request,
                reader_public_key,
                None,
                self.clone(),
            );
        }
        if sidecar.anchor != self.anchor_ref()? {
            return Err("formal response is stale relative to local verified cache".into());
        }
        let history = FormalReadHistoryV1::decode_hex(&sidecar.history_cbor_hex)?;
        let start_cursor = next.replay_cursor()?;
        next.replay_history_steps(&history.steps)?;
        verify_projection(response, request, reader_public_key, &next.current)?;
        next.proved_commits.insert(
            response.version.commit.position,
            response.version.commit.clone(),
        );
        next.verify_minimum(request, &response.version.commit)?;
        next.cursor.record(request, response)?;
        let commit = next.current.head().clone();
        let history_page = if history.steps.is_empty() {
            None
        } else {
            let page = FormalReadHistoryPageV1 {
                schema_version: 1,
                start: start_cursor,
                history,
                end: next.replay_cursor()?,
            };
            page.validate()?;
            Some(page)
        };
        let observation = VerifiedFormalProjectionV1::<P>::new(
            commit,
            next.current.clone(),
            response,
            request,
            reader_public_key,
            history_page,
            next.clone(),
        )?;
        *self = next;
        Ok(observation)
    }
    fn replay_history_steps(&mut self, steps: &[FormalReadStepV1]) -> Result<(), String> {
        if steps.len() > MAX_FORMAL_HISTORY_STEPS {
            return Err("formal history step bound".into());
        }
        for step in steps {
            match step {
                FormalReadStepV1::DurableSetActivation(step) => {
                    if self.pending.is_some() || self.pending_retained_context.is_some() {
                        return Err("healthy durable set cannot replace unresolved recovery".into());
                    }
                    let predecessor_position = step.transition.body.predecessor.position;
                    let predecessor = self.history.get(&predecessor_position).ok_or_else(|| {
                        "HistoryUnavailable: healthy durable set exact predecessor unavailable"
                            .to_owned()
                    })?;
                    let transition = verify_normal_durable_set_transition(
                        &step.transition,
                        self.current.authority(),
                        predecessor.head(),
                        self.current.head(),
                    )
                    .map_err(|error| error.to_string())?;
                    let activation = verify_durable_set_activation(
                        &transition,
                        [&step.old_seals[0], &step.old_seals[1]],
                        [&step.new_installs[0], &step.new_installs[1]],
                    )
                    .map_err(|error| error.to_string())?;
                    self.remember_authority(activation.successor())?;
                    self.current = self
                        .current
                        .with_durable_set_activation(&activation)
                        .map_err(|error| error.to_string())?;
                    // A V1 snapshot alone cannot establish the dual install
                    // lineage. Persist/reopen through the authenticated replay.
                    self.requires_v2_checkpoint = true;
                    self.last_recovery = None;
                    self.history
                        .insert(self.current.head().record().position, self.current.clone());
                }
                FormalReadStepV1::Recovery(step) => {
                    let step_digest = recovery_certificate_digest(&step.certificate)
                        .map_err(|error| error.to_string())?;
                    if self
                        .last_recovery
                        .as_ref()
                        .is_some_and(|known| known.certificate_digest() == step_digest)
                    {
                        continue;
                    }
                    if self.verified_recovery_contexts.len() >= MAX_VERIFIED_RECOVERY_CONTEXTS {
                        return Err(
                            "HistoryUnavailable: verified recovery lineage bound reached".into(),
                        );
                    }
                    let predecessor = self.current.head().clone();
                    let verified = history::verify_recovery_step(
                        step,
                        &self.current,
                        self.last_recovery.as_ref(),
                    )?;
                    let recovery = verified.recovery;
                    self.remember_authority(recovery.original_authority())?;
                    self.remember_authority(recovery.predecessor())?;
                    self.remember_authority(recovery.successor())?;
                    if let Some(previous) = self.last_recovery.as_ref()
                        && previous.certificate_digest() != recovery.certificate_digest()
                        && recovery.body().schema_version != 2
                    {
                        return Err("schema-1 recovery cannot replace same-cut permission".into());
                    }
                    if recovery.body().schema_version == 2 {
                        self.requires_v2_checkpoint = true;
                        let previous = self.last_recovery.as_ref().ok_or_else(|| {
                            "HistoryUnavailable: retained recovery lost prior permission".to_owned()
                        })?;
                        let context = verified.retained_context.as_ref().ok_or_else(|| {
                            "retained recovery has no verified context".to_owned()
                        })?;
                        self.current = self
                            .current
                            .with_verified_retained_recovery(previous, context, &recovery)
                            .map_err(|error| error.to_string())?;
                    } else if recovery.body().tail == RecoveryTailV1::Empty {
                        self.current = self
                            .current
                            .with_empty_recovery(&recovery)
                            .map_err(|error| error.to_string())?;
                    }
                    self.pending =
                        (recovery.body().tail != RecoveryTailV1::Empty).then(|| recovery.clone());
                    self.pending_retained_context = if self.pending.is_some() {
                        verified.retained_context
                    } else {
                        None
                    };
                    self.verified_recovery_contexts
                        .push((recovery.clone(), predecessor));
                    self.last_recovery = Some(recovery);
                }
                FormalReadStepV1::Commit(proof) => {
                    let mut qualification_recovery = None;
                    let mut qualification_context = None;
                    let commit = if let Some(recovery) = self.pending.take() {
                        let retained_context = self.pending_retained_context.take();
                        let is_old_normal =
                            proof.decision.branch == FormalCompletionBranchV1::Normal;
                        if is_old_normal
                            && !matches!(&recovery.body().tail,RecoveryTailV1::Prepared {record_id,..}|RecoveryTailV1::Decision {record_id,..} if record_id==&proof.decision.record.record_id().map_err(|error|error.to_string())?)
                        {
                            return Err("formal recovery cannot replace frozen replay with old Normal envelope".into());
                        }
                        let commit = verify_formal_qualified(
                            proof,
                            recovery.original_authority(),
                            Some(self.current.head()),
                            if is_old_normal { None } else { Some(&recovery) },
                        )
                        .map_err(|error| error.to_string())?;
                        qualification_recovery = Some(recovery.clone());
                        qualification_context = retained_context.clone();
                        self.current = if let Some(context) = retained_context.as_ref() {
                            self.current.after_verified_retained_recovery_commit(
                                context,
                                &recovery,
                                commit.clone(),
                            )
                        } else {
                            self.current
                                .after_verified_recovery_commit(&recovery, commit.clone())
                        }
                        .map_err(|error| error.to_string())?;
                        commit
                    } else {
                        if proof.decision.branch != FormalCompletionBranchV1::Normal {
                            return Err(
                                "formal completion is missing verified recovery history".into()
                            );
                        }
                        let commit = verify_formal_qualified(
                            proof,
                            self.current.authority(),
                            Some(self.current.head()),
                            None,
                        )
                        .map_err(|error| error.to_string())?;
                        self.current = self
                            .current
                            .after_verified_commit(self.current.authority().clone(), commit.clone())
                            .map_err(|error| error.to_string())?;
                        commit
                    };
                    let proof_digest =
                        formal_qualified_proof_digest(proof).map_err(|error| error.to_string())?;
                    let key = QualifiedCommitKey {
                        position: commit.record().position,
                        record_id: commit.head().record_id,
                        proof_digest,
                    };
                    self.qualified_commits.insert(
                        key,
                        VerifiedHistoricalQualificationV1 {
                            proof: (**proof).clone(),
                            commit: commit.clone(),
                            anchor: self.current.clone(),
                            recovery: qualification_recovery,
                            retained_context: qualification_context,
                            origin: self.origin.clone(),
                        },
                    );
                    while self.qualified_commits.len() > MAX_CACHED_HEADS {
                        self.qualified_commits.pop_first();
                    }
                    self.last_recovery = None;
                    self.pending_retained_context = None;
                    self.history
                        .insert(self.current.head().record().position, self.current.clone());
                    while self.history.len() > MAX_CACHED_HEADS {
                        if let Some((position, _)) = self.history.pop_first() {
                            self.proved_commits.remove(&position);
                            self.qualified_commits
                                .retain(|key, _| key.position != position);
                        }
                    }
                }
            }
        }
        let replay_history_exceeds_bound = self.replay_history.as_ref().is_some_and(|history| {
            history.len().saturating_add(steps.len()) > MAX_FORMAL_HISTORY_STEPS
        });
        if replay_history_exceeds_bound {
            self.replay_history = None;
        } else if let Some(history) = self.replay_history.as_mut() {
            history.extend_from_slice(steps);
        }
        Ok(())
    }
    pub fn origin(&self) -> &FormalReadAnchorRefV1 {
        &self.origin
    }
    pub fn contains_verified_anchor(&self, anchor: &FormalReadAnchorRefV1) -> Result<bool, String> {
        if anchor == &self.origin || anchor == &self.anchor_ref()? {
            return Ok(true);
        }
        let Some(cached) = self.history.get(&anchor.position) else {
            return Ok(false);
        };
        Ok(cached.head().head().record_id == anchor.record_id
            && cached
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?
                == anchor.active_authority_id)
    }
    fn remember_authority(&mut self, authority: &VerifiedAuthorityV1) -> Result<(), String> {
        let id = authority
            .authority_id()
            .map_err(|error| error.to_string())?;
        let context = authority
            .context_digest()
            .map_err(|error| error.to_string())?;
        if let Some(existing) = self.verified_authorities.get(&id) {
            if existing
                .context_digest()
                .map_err(|error| error.to_string())?
                != context
            {
                return Err(
                    "HistoryUnavailable: authority ID has conflicting verified context".into(),
                );
            }
            return Ok(());
        }
        if self.verified_authorities.len() >= MAX_VERIFIED_AUTHORITIES {
            return Err("HistoryUnavailable: verified authority lineage bound reached".into());
        }
        self.verified_authorities.insert(id, authority.clone());
        Ok(())
    }
    fn verify_minimum(
        &self,
        request: &ReadWorldViewRequest,
        target: &CommitRef,
    ) -> Result<(), String> {
        let Some(minimum) = &request.min_commit else {
            return Ok(());
        };
        if self.proved_commits.get(&minimum.position) != Some(minimum) {
            return Err(
                "HistoryUnavailable: minimum binding is not a previously proved projection".into(),
            );
        }
        let anchor = self.history.get(&minimum.position).ok_or_else(|| {
            "HistoryUnavailable: minimum head is outside verified ancestry".to_owned()
        })?;
        if anchor.head().head().record_id != minimum.execution_block_hash
            || target.world != minimum.world
            || target.position < minimum.position
        {
            return Err("formal target does not descend from authenticated minimum".into());
        }
        Ok(())
    }
}
fn verify_projection<P: FormalReadProjection>(
    response: &ReadWorldViewResponse<P>,
    request: &ReadWorldViewRequest,
    reader_public_key: &str,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    let record = anchor.head().record();
    let commit = &response.version.commit;
    if response.version.projection_revision
        != authority::request_digest("projection", &response.view)?
    {
        return Err("formal projection revision differs from authenticated view bytes".into());
    }
    let metadata: ProjectionMetadata = serde_json::from_value(
        serde_json::to_value(&response.view).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("formal projection metadata invalid: {error}"))?;
    if metadata.runtime_binding.is_none() {
        return Err("formal projection requires committed RuntimeBinding".into());
    }
    let header = metadata
        .formal_commit_header
        .as_ref()
        .ok_or_else(|| "formal projection requires committed capability counters".to_owned())?;
    if response.continuation.commit != *commit
        || response.continuation.scope_id != request.scope_id
        || response.continuation.stream_id
            != authority::request_digest("formal-events-v1", &(&commit.world, &request.scope_id))?
        || response.continuation.era != header.event_id_era
        || response.continuation.sequence != header.journal_high_watermark
        || header.journal_retention_floor > header.journal_high_watermark.saturating_add(1)
    {
        return Err("formal continuation differs from authenticated journal header".into());
    }
    if header.authority_generation != commit.binding.authority_generation
        || header.permission_generation != commit.binding.permission_generation
    {
        return Err("formal CommitRef capability generations differ from projection header".into());
    }
    if commit.world.world_id != record.world.world_id
        || commit.world.genesis_digest != record.world.genesis_digest
        || commit.position != record.position
        || commit.execution_block_hash != record.record_id().map_err(|error| error.to_string())?
        || commit.state_root_ref != record.after_state_root
        || commit.binding.governing_manifest_ref
            != format!("blake3:{}", record.governing_manifest_digest)
    {
        return Err("formal read CommitRef differs from canonical committed record".into());
    }
    let audience = if request.scope_id == "public" {
        ProjectionAudienceV1::public()
    } else if let Some(agent) = request.scope_id.strip_prefix("agent:") {
        ProjectionAudienceV1::agent(agent, reader_public_key.to_owned())?
    } else {
        return Err("unsupported formal projection audience".into());
    };
    let wire = &response
        .formal_proof
        .as_ref()
        .expect("checked formal sidecar")
        .projection;
    let inclusion = ProjectionInclusionV1 {
        schema_version: wire.schema_version,
        audience: ProjectionAudienceV1 {
            scope_id: wire.audience.scope_id.clone(),
            reader_public_key: wire.audience.reader_public_key.clone(),
        },
        leaf_index: wire.leaf_index,
        leaf_count: wire.leaf_count,
        siblings: wire.siblings.clone(),
    };
    inclusion.verify(&audience, &response.view, &record.projection_root)
}
