use super::*;
use crate::formal_read::{FormalReadHistoryPageV1, FormalReadReplayCursorV1};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalReadCheckpointV1, InitialActivationTrustV1, VerifiedFormalReadAnchorV1,
};
/// Authorized local pins; never filled from service responses.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalReadTrustConfigV1 {
    pub initial_trust: InitialActivationTrustV1,
    pub checkpoint: FormalReadCheckpointV1,
    pub checkpoint_digest: String,
}
impl FormalReadTrustConfigV1 {
    pub fn verified_anchor(&self) -> Result<VerifiedFormalReadAnchorV1, String> {
        VerifiedFormalReadAnchorV1::from_fixed_pin(
            &self.checkpoint,
            &self.checkpoint_digest,
            &self.initial_trust,
        )
        .map_err(|error| error.to_string())
    }
}
impl<P: FormalReadProjection> RemoteWorldServiceReader<P> {
    /// Rebuild portable evidence from the CURRENT external pin. No saved
    /// cursor or response chooses a new trust root. Failure leaves this cache
    /// unchanged, including when a concurrent observation wins the CAS.
    pub fn rebase_formal_cursor(
        &self,
        minimum: &CommitRef,
    ) -> Result<FormalReadCursorBundleV1<P>, WorldServiceClientError> {
        if self.config.scope_id != "public" {
            return Err(WorldServiceClientError::Assurance(
                "formal cursor rebase requires configured public scope".into(),
            ));
        }
        let cache = self.formal_read.as_ref().ok_or_else(|| {
            WorldServiceClientError::Assurance(
                "formal cursor requires configured proof trust".into(),
            )
        })?;
        let expected = cache
            .lock()
            .map_err(|_| WorldServiceClientError::Assurance("formal cache poisoned".into()))?
            .replay_cursor()
            .map_err(WorldServiceClientError::Assurance)?;
        let fresh = Self::new(self.config.clone())?;
        let observed = fresh.read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: self.config.expected_world.clone(),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })?;
        let next = observed
            .formal_verifier()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal rebase has no independently verified cache".into(),
                )
            })?
            .clone();
        let bundle = next
            .export_cursor()
            .map_err(WorldServiceClientError::Assurance)?;
        cache
            .lock()
            .map_err(|_| WorldServiceClientError::Assurance("formal cache poisoned".into()))?
            .adopt_rebased_cursor(next, &expected, minimum)
            .map_err(WorldServiceClientError::Assurance)?;
        Ok(bundle)
    }
    pub fn formal_cursor(&self) -> Result<FormalReadCursorBundleV1<P>, WorldServiceClientError> {
        self.formal_read
            .as_ref()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal cursor requires external proof configuration".into(),
                )
            })?
            .lock()
            .map_err(|_| {
                WorldServiceClientError::Assurance("formal verifier cache poisoned".into())
            })?
            .export_cursor()
            .map_err(WorldServiceClientError::Assurance)
    }
    pub fn restore_formal_cursor(
        &self,
        bundle: &FormalReadCursorBundleV1<P>,
    ) -> Result<CommitRef, WorldServiceClientError> {
        let configured = self.config.formal_read_trust.as_ref().ok_or_else(|| {
            WorldServiceClientError::Configuration(
                "formal cursor requires CURRENT external pinned configuration".into(),
            )
        })?;
        let reader = authority::sign_read_request(VIEW_PATH, (), &self.config.read_private_key_hex)
            .map_err(WorldServiceClientError::Configuration)?
            .subject_public_key;
        let next = FormalReadVerifier::<P>::restore_cursor(
            &configured.initial_trust,
            configured
                .verified_anchor()
                .map_err(WorldServiceClientError::Configuration)?,
            &configured.checkpoint_digest,
            bundle,
            &reader,
        )
        .map_err(WorldServiceClientError::Assurance)?;
        let cache = self
            .formal_read
            .as_ref()
            .ok_or_else(|| WorldServiceClientError::Assurance("formal verifier absent".into()))?;
        let mut current = cache.lock().map_err(|_| {
            WorldServiceClientError::Assurance("formal verifier cache poisoned".into())
        })?;
        let fresh = FormalReadVerifier::<P>::new(
            &configured.initial_trust,
            configured
                .verified_anchor()
                .map_err(WorldServiceClientError::Configuration)?,
        )
        .map_err(WorldServiceClientError::Assurance)?;
        if !current
            .is_fresh_cursor_target(&fresh)
            .map_err(WorldServiceClientError::Assurance)?
        {
            return Err(WorldServiceClientError::Assurance(
                "formal cursor restore can only seed a fresh CURRENT external-origin client".into(),
            ));
        }
        *current = next;
        Ok(bundle.commit.clone())
    }
    /// Prepare a new observation challenge and anchor BEFORE an actor/owner
    /// signs the request. Signed relay helpers never amend an existing proof.
    pub fn prepare_read_view_request(
        &self,
        mut request: ReadWorldViewRequest,
    ) -> Result<ReadWorldViewRequest, WorldServiceClientError> {
        if self.formal_read.is_some() {
            request.observation_nonce = None;
        }
        self.prepare_formal_request(request)
    }
    /// Rebuild a persistent page chain from the exact configured external V1
    /// pin. The caller supplies content-addressed bytes; neither the page
    /// hashes nor the final cursor are trusted until every page is replayed.
    pub fn resume_formal_history_pages<I>(
        &self,
        origin_digest: &str,
        pages: I,
        expected_final_cursor: &FormalReadReplayCursorV1,
    ) -> Result<(), WorldServiceClientError>
    where
        I: IntoIterator<Item = Result<(String, Vec<u8>), String>>,
    {
        let configured = self.config.formal_read_trust.as_ref().ok_or_else(|| {
            WorldServiceClientError::Configuration(
                "formal history recovery requires configured external pin".into(),
            )
        })?;
        if configured.checkpoint_digest != origin_digest {
            return Err(WorldServiceClientError::Assurance(
                "formal history recovery differs from exact external origin digest".into(),
            ));
        }
        let origin_anchor = configured
            .verified_anchor()
            .map_err(WorldServiceClientError::Configuration)?;
        let mut verifier = FormalReadVerifier::<P>::new(&configured.initial_trust, origin_anchor)
            .map_err(WorldServiceClientError::Assurance)?;
        for page in pages {
            let (expected_digest, bytes) = page.map_err(WorldServiceClientError::Assurance)?;
            if expected_digest.len() != 64
                || !expected_digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || blake3::hash(&bytes).to_hex().as_str() != expected_digest
            {
                return Err(WorldServiceClientError::Assurance(
                    "formal history page content digest mismatch".into(),
                ));
            }
            let page = FormalReadHistoryPageV1::decode_bytes(&bytes)
                .map_err(WorldServiceClientError::Assurance)?;
            verifier
                .replay_history_page(&page)
                .map_err(WorldServiceClientError::Assurance)?;
        }
        if verifier
            .replay_cursor()
            .map_err(WorldServiceClientError::Assurance)?
            != *expected_final_cursor
        {
            return Err(WorldServiceClientError::Assurance(
                "formal history manifest cursor differs from replayed external lineage".into(),
            ));
        }
        let cache = self.formal_read.as_ref().ok_or_else(|| {
            WorldServiceClientError::Assurance("formal read verification is not configured".into())
        })?;
        let mut current = cache.lock().map_err(|_| {
            WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
        })?;
        if current
            .replay_cursor()
            .map_err(WorldServiceClientError::Assurance)?
            != FormalReadVerifier::<P>::new(
                &configured.initial_trust,
                configured
                    .verified_anchor()
                    .map_err(WorldServiceClientError::Configuration)?,
            )
            .map_err(WorldServiceClientError::Assurance)?
            .replay_cursor()
            .map_err(WorldServiceClientError::Assurance)?
        {
            return Err(WorldServiceClientError::Assurance(
                "formal page recovery can only seed a fresh external-origin client".into(),
            ));
        }
        *current = verifier;
        Ok(())
    }

    /// Return the anchor produced by this client's configured formal proof
    /// verifier. It is never populated from a service checkpoint or cursor.
    pub fn formal_current_anchor(
        &self,
    ) -> Result<VerifiedFormalReadAnchorV1, WorldServiceClientError> {
        self.formal_read
            .as_ref()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal read verification is not configured".into(),
                )
            })?
            .lock()
            .map_err(|_| {
                WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
            })
            .map(|verifier| verifier.current_anchor().clone())
    }

    /// Return a consistency cursor for the currently verified proof lineage.
    /// This value is not an authority pin and must be checked by replay.
    pub fn formal_replay_cursor(
        &self,
    ) -> Result<FormalReadReplayCursorV1, WorldServiceClientError> {
        self.formal_read
            .as_ref()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal read verification is not configured".into(),
                )
            })?
            .lock()
            .map_err(|_| {
                WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
            })?
            .replay_cursor()
            .map_err(WorldServiceClientError::Assurance)
    }

    /// Resume from a privately constructed, independently proved view. A raw
    /// CommitRef or a service-provided checkpoint cannot seed this cache.
    pub fn resume_formal_view(
        &self,
        view: &VerifiedWorldView<P>,
    ) -> Result<(), WorldServiceClientError> {
        let configured = self.config.formal_read_trust.as_ref().ok_or_else(|| {
            WorldServiceClientError::Configuration(
                "formal resume requires configured external pin".into(),
            )
        })?;
        if !view.read_authority_matches(
            &self
                .read_authority_identity()
                .map_err(WorldServiceClientError::Assurance)?,
        ) {
            return Err(WorldServiceClientError::Assurance(
                "formal resume view belongs to another configured read authority".into(),
            ));
        }
        let verifier = view
            .formal_verifier()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal resume requires independently verified cache lineage".into(),
                )
            })?
            .clone();
        let base = FormalReadVerifier::<P>::new(
            &configured.initial_trust,
            configured
                .verified_anchor()
                .map_err(WorldServiceClientError::Configuration)?,
        )
        .map_err(WorldServiceClientError::Assurance)?;
        if verifier.origin() != base.origin() {
            return Err(WorldServiceClientError::Assurance(
                "formal resume has another fixed checkpoint origin".into(),
            ));
        }
        let cache = self.formal_read.as_ref().ok_or_else(|| {
            WorldServiceClientError::Assurance("formal read verification is not configured".into())
        })?;
        let mut current = cache.lock().map_err(|_| {
            WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
        })?;
        let old = current
            .anchor_ref()
            .map_err(WorldServiceClientError::Assurance)?;
        if !verifier
            .contains_verified_anchor(&old)
            .map_err(WorldServiceClientError::Assurance)?
        {
            return Err(WorldServiceClientError::Assurance(
                "HistoryUnavailable: formal resume cannot prove current exact head ancestry".into(),
            ));
        }
        *current = verifier;
        Ok(())
    }
    pub(super) fn check_signed_formal_anchor(
        &self,
        request: &ReadWorldViewRequest,
    ) -> Result<(), WorldServiceClientError> {
        if self.formal_read.is_some() {
            request.validate_formal_shape().map_err(|error| {
                WorldServiceClientError::Assurance(format!(
                    "actor/owner must prepare formal anchor and fresh nonce BEFORE signing: {error}"
                ))
            })?;
        }
        let prepared = self.prepare_formal_request(request.clone())?;
        if &prepared != request {
            return Err(WorldServiceClientError::Assurance(
                "actor must sign the current verified formal read anchor".into(),
            ));
        }
        Ok(())
    }
    /// The anchor is independently verified before this constructor. Neither
    /// service identity nor service response fields supply proof trust.
    pub fn new_formal(
        config: WorldServiceClientConfig,
        trust: InitialActivationTrustV1,
        anchor: VerifiedFormalReadAnchorV1,
    ) -> Result<Self, WorldServiceClientError> {
        if config.expected_world.world_id != trust.world.world_id
            || config.expected_world.genesis_digest != trust.world.genesis_digest
        {
            return Err(WorldServiceClientError::Configuration(
                "formal initial trust differs from configured world".into(),
            ));
        }
        let configured_pin = if let Some(configured) = &config.formal_read_trust {
            configured
                .verified_anchor()
                .map_err(WorldServiceClientError::Configuration)?;
            if configured.initial_trust != trust
                || anchor
                    .checkpoint()
                    .digest()
                    .map_err(|error| WorldServiceClientError::Configuration(error.to_string()))?
                    != configured.checkpoint_digest
            {
                return Err(WorldServiceClientError::Configuration("supplied formal anchor differs from exact configured fixed pin; resume requires verified lineage".into()));
            }
            configured.checkpoint_digest.clone()
        } else {
            anchor
                .checkpoint()
                .digest()
                .map_err(|error| WorldServiceClientError::Configuration(error.to_string()))?
        };
        let identity =
            authority::request_digest("formal-read-local-trust-v1", &(&trust, configured_pin))
                .map_err(WorldServiceClientError::Configuration)?;
        let verifier = FormalReadVerifier::<P>::new(&trust, anchor)
            .map_err(WorldServiceClientError::Configuration)?;
        let mut client = Self::new_base_with_evidence_policy(config, ReadEvidencePolicy::Ordinary)?;
        client.formal_read = Some(std::sync::Arc::new(std::sync::Mutex::new(verifier)));
        client.formal_trust_identity = Some(identity);
        Ok(client)
    }
    pub fn formal_checkpoint(&self) -> Result<FormalReadCheckpointV1, WorldServiceClientError> {
        self.formal_read
            .as_ref()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal read verification is not configured".into(),
                )
            })?
            .lock()
            .map_err(|_| {
                WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
            })?
            .checkpoint()
            .map_err(WorldServiceClientError::Assurance)
    }
    pub fn formal_anchor_ref(&self) -> Result<FormalReadAnchorRefV1, WorldServiceClientError> {
        self.formal_read
            .as_ref()
            .ok_or_else(|| {
                WorldServiceClientError::Assurance(
                    "formal read verification is not configured".into(),
                )
            })?
            .lock()
            .map_err(|_| {
                WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
            })?
            .anchor_ref()
            .map_err(WorldServiceClientError::Assurance)
    }
    pub(crate) fn prepare_formal_request(
        &self,
        mut request: ReadWorldViewRequest,
    ) -> Result<ReadWorldViewRequest, WorldServiceClientError> {
        if let Some(verifier) = &self.formal_read {
            if request.observation_nonce.is_none() {
                let mut nonce = [0u8; 32];
                getrandom::fill(&mut nonce).map_err(|_| {
                    WorldServiceClientError::Assurance(
                        "formal read observation randomness unavailable".into(),
                    )
                })?;
                request.observation_nonce = Some(hex::encode(nonce));
            }
            let verifier = verifier.lock().map_err(|_| {
                WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
            })?;
            let anchor = if let Some(fixed) = &request.fixed_commit {
                verifier.fixed_anchor_ref(fixed)
            } else {
                verifier.anchor_ref()
            }
            .map_err(WorldServiceClientError::Assurance)?;
            if request
                .formal_anchor
                .as_ref()
                .is_some_and(|supplied| supplied != &anchor)
            {
                return Err(WorldServiceClientError::Assurance(
                    "caller formal anchor differs from verified cache".into(),
                ));
            }
            request.formal_anchor = Some(anchor);
        } else if request.formal_anchor.is_some() {
            return Err(WorldServiceClientError::Assurance(
                "formal anchor requires configured external trust".into(),
            ));
        }
        Ok(request)
    }
    pub(super) fn finish_view(
        &self,
        response: ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        reader: &str,
    ) -> Result<VerifiedWorldView<P>, WorldServiceClientError> {
        let verified = if let Some(verifier) = &self.formal_read {
            let observation = verifier
                .lock()
                .map_err(|_| {
                    WorldServiceClientError::Assurance("formal verifier cache is poisoned".into())
                })?
                .verify_view(&response, request, reader)
                .map_err(WorldServiceClientError::Assurance)?;
            VerifiedWorldView::new_formal_verified(response, request, observation)
        } else {
            VerifiedWorldView::new_with_evidence_policy(response, request, self.evidence_policy)
        };
        verified.map_err(WorldServiceClientError::Assurance)
    }
    pub(crate) fn require_unproved_operation_profile(
        &self,
        profile: Option<&FormalReadProfileV1>,
    ) -> Result<(), WorldServiceClientError> {
        if self.formal_read.is_some() || profile.is_some() {
            return Err(WorldServiceClientError::Assurance(
                "formal Submit/Lookup/Changes proof contract is unsupported; client is read-only"
                    .into(),
            ));
        }
        Ok(())
    }
}
