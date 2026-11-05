//! Independently consumed original inventory from one same-backend current
//! read. Never deserialize this type or accept its source as caller/wire input.
use super::backend::BftSignerBackendSnapshotV3;
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::private_retention::BftPrivateRetentionAttestorV1;
use super::private_retention_backend::{
    BftPrivateRetentionCurrentV1, BftPrivateRetentionInputInventoryV1, BftPrivateRetentionLedgerV1,
    validate_input_inventory,
};

/// This proves only exact current protected inventory provenance. It grants no
/// private ReadGrant, World authority, source creation or migration authority.
pub struct BftPrivateRetentionVerificationContextV1<'a> {
    current: &'a BftPrivateRetentionCurrentV1,
    verified: Option<&'a super::state::VerifiedBftSignerStateV3>,
    trusted: &'a BftPrivateRetentionAttestorV1,
    sealed: Option<&'a super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1>,
}
impl<'a> BftPrivateRetentionVerificationContextV1<'a> {
    /// Only the service's configured original backend read may enter here.
    /// Generic request DTOs and local private directory scans are not sources.
    pub(crate) fn from_backend_current_with_verified_state(
        binding: &BftSignerBindingV3,
        trusted: &'a BftPrivateRetentionAttestorV1,
        current: &'a BftPrivateRetentionCurrentV1,
        expected_lease: &BftSignerBackendSnapshotV3,
        verified: Option<&'a super::state::VerifiedBftSignerStateV3>,
    ) -> Result<Self, BftSignerErrorV3> {
        trusted.digest()?;
        if &current.signer != expected_lease
            || expected_lease.lease_token_digest.is_empty()
            || expected_lease.lease_generation == 0
        {
            return Err(invalid(
                "original verification context differs from same-read current lease snapshot",
            ));
        }

        validate_context_state(binding, trusted, expected_lease, verified)?;
        match (&current.signer.state.private_retention, &current.retention) {
            (None, None) if current.inputs.is_empty() => {}
            (Some(head), Some(original)) => {
                original.receipt.verify_map(trusted, &original.map)?;
                validate_input_inventory(original, &current.inputs)?;
                if head != &original.receipt.body.head
                    || current.signer.state.revision < original.receipt.body.after_revision
                    || current.signer.lease_generation
                        < original.receipt.body.original_lease_generation
                    || (current.signer.state.revision == original.receipt.body.after_revision
                        && current.signer.state.state_digest
                            != original.receipt.body.after_state_digest)
                {
                    return Err(invalid(
                        "original verification context current State/receipt differs",
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    "original verification context lacks exact map/input inventory",
                ));
            }
        }
        Ok(Self {
            current,
            verified,
            trusted,
            sealed: None,
        })
    }
    pub(crate) fn from_backend_sealed_current_with_verified_state(
        binding: &BftSignerBindingV3,
        trusted: &'a BftPrivateRetentionAttestorV1,
        sealed: &'a super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1,
        expected_lease: &BftSignerBackendSnapshotV3,
        verified: Option<&'a super::state::VerifiedBftSignerStateV3>,
    ) -> Result<Self, BftSignerErrorV3> {
        if &sealed.current.signer != expected_lease
            || expected_lease.lease_token_digest.is_empty()
            || expected_lease.lease_generation == 0
        {
            return Err(invalid(
                "sealed verification context same-read lease differs",
            ));
        }
        validate_context_state(binding, trusted, expected_lease, verified)?;
        sealed.validate_inventory_with_verified_state(binding, trusted, verified)?;
        Ok(Self {
            current: &sealed.current,
            verified,
            trusted,
            sealed: Some(sealed),
        })
    }
    /// Reborrow authenticated complete original history under this exact live
    /// backend sample. This creates no reader, external grant, or new inventory.
    pub fn original_replay_context<'b>(
        &'b self,
        binding: &'b BftSignerBindingV3,
    ) -> Result<
        super::private_retention_seal_context::BftPrivateRetentionSealVerificationContextV1<'b>,
        BftSignerErrorV3,
    > {
        let sealed = self.sealed.ok_or_else(|| {
            invalid("original replay requires complete same-backend sealed history")
        })?;
        super::private_retention_seal_context::BftPrivateRetentionSealVerificationContextV1::from_backend_sealed_current_with_verified_state(
            binding, self.trusted, sealed, &self.current.signer, self.verified,
        )
    }
    pub fn original_ready<'b>(
        &'b self,
        binding: &'b BftSignerBindingV3,
        key: &super::private_retention::BftPrivateRetentionKeyV1,
    ) -> Result<
        super::private_retention_seal_context::BftPrivateRetentionHistoricalInputContextV1<'b>,
        BftSignerErrorV3,
    > {
        self.original_replay_context(binding)?.original_ready(key)
    }
    /// Original Active ancestry authenticated by the exact service State. This
    /// is not a complete current admission owner or a private ReadGrant.
    pub fn active_parent(
        &self,
    ) -> Result<super::active_window::VerifiedBftSignerActiveWindowV1, BftSignerErrorV3> {
        let verified = self
            .verified
            .ok_or_else(|| invalid("retention context lacks authenticated current Active State"))?;
        if verified.state != self.current.signer.state {
            return Err(invalid(
                "retention Active State changed from original snapshot",
            ));
        }
        super::active_window::VerifiedBftSignerActiveWindowV1::from_verified(
            verified,
            &self.trusted.provider_public_key,
        )?
        .ok_or_else(|| invalid("retention context has no authenticated Active parent"))
    }
    /// Reconstruct historical Installed only from the exact original public
    /// archive object authenticated by this current opaque Active request.
    /// This is historical readback, never current signing/install authority.
    pub fn reconstruct_installed_original(
        &self,
        binding: &BftSignerBindingV3,
        original: &super::successor_state::BftSuccessorWindowStoredRequestV1,
        original_bytes: &[u8],
    ) -> Result<
        Box<super::active_window_reconstruction::ValidatedBftSuccessorInstalledStateV1>,
        BftSignerErrorV3,
    > {
        validate_context_state(binding, self.trusted, &self.current.signer, self.verified)?;
        let active = self.active_parent()?;
        if active.original_request() != original
            || active.provider_public_key() != self.trusted.provider_public_key
            || active.binding_digest() != binding.digest()?
        {
            return Err(invalid(
                "historical original differs from exact current Active request",
            ));
        }
        original.validate(binding, &self.trusted.provider_public_key)?;
        original
            .original_public_state
            .validate_bytes(original_bytes)?;
        let state: super::state::BftSignerStateV3 = super::crypto::decode_bounded(
            original_bytes,
            super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?;
        if super::crypto::encode_bounded(&state, super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3)?
            != original_bytes
        {
            return Err(invalid("historical original public State is not canonical"));
        }
        // This bridge intentionally supports only first Source C0. A previous
        // Active original needs its own complete authenticated archive context,
        // never a synthetic None context or a truncated current State.
        if state.active_window.is_some() || state.successor_window.is_some() {
            return Err(invalid(
                "historical Source original requires independent predecessor context",
            ));
        }
        let body = &original.request.intent.body;
        if state.revision != body.expected_revision
            || state.state_digest != body.expected_state_digest
            || state.binding_digest != binding.digest()?
        {
            return Err(invalid(
                "historical original public State differs from signed request",
            ));
        }
        state.validate(binding, &self.trusted.provider_public_key)?;
        let authenticated = super::archive_query::ValidatedBftSuccessorOriginalStateV1 {
            state,
            provider_public_key: self.trusted.provider_public_key.clone(),
            active_context: None,
        };
        Ok(Box::new(authenticated.reconstruct_installed_pending(
            binding,
            &self.trusted.provider_public_key,
            original,
        )?))
    }
    pub fn current_schedule(
        &self,
    ) -> Result<&oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3, BftSignerErrorV3> {
        let verified = self
            .verified
            .ok_or_else(|| invalid("retention context lacks authenticated current schedule"))?;
        if verified.state != self.current.signer.state {
            return Err(invalid(
                "retention schedule State changed from original snapshot",
            ));
        }
        Ok(&verified.schedule)
    }
    /// Full protected original union validates local slots and also authorizes
    /// current deduplication after a floor advance. No confidential copy occurs.
    pub fn original_inputs(
        &self,
    ) -> impl Iterator<
        Item = &std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>,
    > {
        self.current.inputs.values().chain(
            self.sealed
                .into_iter()
                .flat_map(|sealed| sealed.sealed_inputs.iter())
                .filter(|(id, _)| !self.current.inputs.contains_key(*id))
                .map(|(_, input)| input),
        )
    }
    pub fn original_entry(
        &self,
        key: &super::private_retention::BftPrivateRetentionKeyV1,
    ) -> Result<Option<&super::private_retention::BftPrivateRetentionEntryV1>, BftSignerErrorV3>
    {
        if let Some(sealed) = self.sealed {
            return sealed.original_entry(key);
        }
        Ok(self.current.retention.as_ref().and_then(|ledger| {
            ledger
                .map
                .entries
                .binary_search_by(|entry| entry.key.cmp(key))
                .ok()
                .map(|i| &ledger.map.entries[i])
        }))
    }
    pub fn original_input(
        &self,
        digest: &str,
    ) -> Option<&std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>>
    {
        self.current.inputs.get(digest).or_else(|| {
            self.sealed
                .and_then(|sealed| sealed.sealed_inputs.get(digest))
        })
    }

    pub fn attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        self.trusted
    }
    pub fn signer(&self) -> &BftSignerBackendSnapshotV3 {
        &self.current.signer
    }
    pub fn ledger(&self) -> Option<&BftPrivateRetentionLedgerV1> {
        self.current.retention.as_ref()
    }
    /// Immutable borrowed original payloads: budgeting should inspect refs and
    /// lengths here, without copying the bounded confidential object inventory.
    pub fn inputs(&self) -> &BftPrivateRetentionInputInventoryV1 {
        &self.current.inputs
    }
}

/// The original archive capability must belong to this exact already-validated
/// service State. An Active token or arbitrary State DTO is insufficient.
pub(crate) fn validate_context_state(
    binding: &BftSignerBindingV3,
    trusted: &BftPrivateRetentionAttestorV1,
    expected: &BftSignerBackendSnapshotV3,
    verified: Option<&super::state::VerifiedBftSignerStateV3>,
) -> Result<(), BftSignerErrorV3> {
    trusted.digest()?;
    if trusted.source.binding_digest != binding.digest()?
        || trusted.source.handoff_digest != binding.handoff_record_id
        || trusted.source.material_source_digest != binding.material_identity
    {
        return Err(invalid(
            "retention context fixed source differs from binding",
        ));
    }
    let context = match verified {
        Some(verified) => {
            if verified.state != expected.state {
                return Err(invalid(
                    "retention context verified State differs from current snapshot",
                ));
            }
            verified.active_context.clone()
        }
        None => None,
    };
    super::state::validate_state_with_active_context(
        binding,
        Some(&trusted.provider_public_key),
        &expected.state,
        context,
    )?;
    Ok(())
}

#[path = "private_retention_active_context.rs"]
mod active_context;
pub use active_context::{
    BftPrivateRetentionAuthenticatedAdmissionParentV1, BftPrivateRetentionHistoricalOwnedC0V1,
};
