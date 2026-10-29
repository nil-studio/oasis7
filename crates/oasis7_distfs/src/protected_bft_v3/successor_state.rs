//! Protected successor transaction metadata. Hot H history remains intact.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::crypto::{BFT_SIGNER_MAX_FRAME_BYTES_V3, encode_bounded};
use super::state::{BftSignerStateV3, VerifiedBftSignerStateV3, validate_state};
use super::successor_window::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowStoredRequestV1 {
    pub original_public_state: BftSuccessorWindowArchiveRefV1,
    pub manifest: BftSuccessorWindowArchiveManifestV1,
    pub prepared_revision: u64,
    pub request: BftSuccessorWindowPersistedRequestV1,
}
impl BftSuccessorWindowStoredRequestV1 {
    pub fn request(&self) -> &BftSuccessorWindowPersistedRequestV1 {
        &self.request
    }
    pub fn validate(
        &self,
        binding: &BftSignerBindingV3,
        provider: &str,
    ) -> Result<(), BftSignerErrorV3> {
        self.request.validate(provider)?;
        let body = &self.request.intent.body;
        body.checkpoint.source.validate_for_binding(binding)?;
        self.original_public_state.validate()?;
        self.manifest.validate_for_checkpoint(&body.checkpoint)?;
        validate_whole_state_public_manifest(
            &self.manifest,
            &self.original_public_state,
            &body.expected_state_digest,
        )?;
        if self.prepared_revision
            != body
                .expected_revision
                .checked_add(2)
                .ok_or_else(|| invalid("successor prepared revision overflow"))?
            || self
                .request
                .receipt
                .as_ref()
                .is_some_and(|r| r.body.committed_revision <= self.prepared_revision)
            || self.request.status == BftSuccessorWindowStatusV1::Superseded
        {
            return Err(invalid(
                "successor stored original/material/prepared revision",
            ));
        }
        Ok(())
    }
}

/// A nonrecursive copy is necessary to recover S after reservation CAS but
/// before its archive exists. It cannot carry another reservation recursively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowPriorStateV1 {
    schema_version: u32,
    minimum: Option<BftSuccessorWindowMinimumV1>,
    active_request_id: Option<String>,
    usage: BftSuccessorWindowArchiveUsageV1,
    #[serde(deserialize_with = "super::state::bounded_sequence")]
    requests: Vec<BftSuccessorWindowStoredRequestV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowReservationV1 {
    pub body: BftSuccessorWindowInstallIntentBodyV1,
    pub manifest: BftSuccessorWindowArchiveManifestV1,
    pub original_public_state: BftSuccessorWindowArchiveRefV1,
    pub previous: Option<BftSuccessorWindowPriorStateV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowStateV1 {
    pub schema_version: u32,
    pub minimum: Option<BftSuccessorWindowMinimumV1>,
    pub active_request_id: Option<String>,
    pub usage: BftSuccessorWindowArchiveUsageV1,
    #[serde(deserialize_with = "super::state::bounded_sequence")]
    pub requests: Vec<BftSuccessorWindowStoredRequestV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation: Option<BftSuccessorWindowReservationV1>,
}
impl BftSuccessorWindowStateV1 {
    pub(crate) fn empty() -> Self {
        Self {
            schema_version: 1,
            minimum: None,
            active_request_id: None,
            usage: Default::default(),
            requests: Vec::new(),
            reservation: None,
        }
    }
    pub fn request(&self, id: &str) -> Option<&BftSuccessorWindowStoredRequestV1> {
        self.requests
            .iter()
            .find(|r| r.request.intent.body.checkpoint.request_id == id)
    }
    pub(crate) fn prior(&self) -> Result<BftSuccessorWindowPriorStateV1, BftSignerErrorV3> {
        if self.reservation.is_some() {
            return Err(invalid("successor nested reservation"));
        }
        Ok(BftSuccessorWindowPriorStateV1 {
            schema_version: self.schema_version,
            minimum: self.minimum.clone(),
            active_request_id: self.active_request_id.clone(),
            usage: self.usage,
            requests: self.requests.clone(),
        })
    }
    pub(crate) fn blocks_writes(&self) -> bool {
        self.reservation.is_some()
            || self
                .active_request_id
                .as_ref()
                .and_then(|id| self.request(id))
                .is_some_and(|r| r.request.status != BftSuccessorWindowStatusV1::Adopted)
    }
    pub(crate) fn validate(
        &self,
        binding: &BftSignerBindingV3,
        provider: Option<&str>,
    ) -> Result<(), BftSignerErrorV3> {
        encode_bounded(self, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?;
        self.usage.validate()?;
        if self.schema_version != 1
            || self.requests.len() + usize::from(self.reservation.is_some()) > 64
        {
            return Err(invalid("successor protected request/metadata capacity"));
        }
        let mut ids = BTreeSet::new();
        let mut minimum = None;
        let mut usage = BftSuccessorWindowArchiveUsageV1::default();
        for (index, record) in self.requests.iter().enumerate() {
            let provider = provider.unwrap_or(&record.request.intent.provider_public_key);
            record.validate(binding, provider)?;
            let body = &record.request.intent.body.checkpoint;
            validate_manifest_predecessor(
                binding,
                &record.manifest,
                body,
                index.checked_sub(1).map(|i| &self.requests[i]),
            )?;
            if !ids.insert(body.request_id.as_str()) {
                return Err(invalid("successor original request ID duplicate"));
            }
            body.usage.validate_after(&usage)?;
            usage = body.usage;
            if index + 1 < self.requests.len()
                && record.request.status != BftSuccessorWindowStatusV1::Adopted
            {
                return Err(invalid("successor unfinished predecessor"));
            }
            if record.request.status != BftSuccessorWindowStatusV1::Prepared {
                let next = body.minimum()?;
                if let Some(previous) = &minimum {
                    next.validate_after(previous)?;
                }
                minimum = Some(next);
            }
        }
        if let Some(reservation) = &self.reservation {
            reservation.body.validate()?;
            reservation
                .body
                .checkpoint
                .source
                .validate_for_binding(binding)?;
            reservation
                .manifest
                .validate_for_checkpoint(&reservation.body.checkpoint)?;
            reservation.original_public_state.validate()?;
            validate_whole_state_public_manifest(
                &reservation.manifest,
                &reservation.original_public_state,
                &reservation.body.expected_state_digest,
            )?;
            validate_manifest_predecessor(
                binding,
                &reservation.manifest,
                &reservation.body.checkpoint,
                self.requests.last(),
            )?;
            reservation.body.checkpoint.usage.validate_after(&usage)?;
            if ids.contains(reservation.body.checkpoint.request_id.as_str())
                || reservation
                    .previous
                    .as_ref()
                    .map(|p| p.requests.as_slice())
                    .unwrap_or(&[])
                    != self.requests.as_slice()
                || self.active_request_id.as_deref()
                    != Some(reservation.body.checkpoint.request_id.as_str())
            {
                return Err(invalid("successor reservation original predecessor"));
            }
            usage = reservation.body.checkpoint.usage;
        } else if self.active_request_id.as_deref()
            != self
                .requests
                .last()
                .map(|r| r.request.intent.body.checkpoint.request_id.as_str())
        {
            return Err(invalid("successor active original request differs"));
        }
        if self.minimum != minimum || self.usage != usage {
            return Err(invalid("successor protected minimum/usage differs"));
        }
        Ok(())
    }
}
/// The V1 store has typed durable IO only for the complete original public
/// State. Extra manifest objects cannot be promised without a typed verifier.
pub(crate) fn validate_whole_state_public_manifest(
    manifest: &BftSuccessorWindowArchiveManifestV1,
    original_public_state: &BftSuccessorWindowArchiveRefV1,
    expected_state_digest: &str,
) -> Result<(), BftSignerErrorV3> {
    if manifest.kind != BftSuccessorWindowArchiveKindV1::Public
        || manifest.entries.len() != 1
        || manifest.entries[0].object != *original_public_state
        || manifest.entries[0].identity_digest != expected_state_digest
    {
        return Err(invalid(
            "successor whole-State public archive must contain exactly the original state identity/object",
        ));
    }
    Ok(())
}
fn validate_manifest_predecessor(
    binding: &BftSignerBindingV3,
    manifest: &BftSuccessorWindowArchiveManifestV1,
    checkpoint: &BftSuccessorWindowCheckpointBodyV1,
    previous: Option<&BftSuccessorWindowStoredRequestV1>,
) -> Result<(), BftSignerErrorV3> {
    let (first_height, root) = match previous {
        Some(record) => (
            record
                .request
                .intent
                .body
                .checkpoint
                .cut
                .height
                .checked_add(1)
                .ok_or_else(|| invalid("successor archive height overflow"))?,
            Some(record.manifest.digest()?),
        ),
        None => (
            binding
                .handoff_position
                .checked_add(1)
                .ok_or_else(|| invalid("successor archive H overflow"))?,
            None,
        ),
    };
    if manifest.kind != BftSuccessorWindowArchiveKindV1::Public
        || manifest.segment_sequence != checkpoint.sequence
        || manifest.first_height != first_height
        || manifest.predecessor_root != root
    {
        return Err(invalid(
            "successor public archive predecessor/range discontinuity",
        ));
    }
    Ok(())
}
impl BftSuccessorWindowPriorStateV1 {
    fn restore(&self) -> BftSuccessorWindowStateV1 {
        BftSuccessorWindowStateV1 {
            schema_version: self.schema_version,
            minimum: self.minimum.clone(),
            active_request_id: self.active_request_id.clone(),
            usage: self.usage,
            requests: self.requests.clone(),
            reservation: None,
        }
    }
}
impl BftSuccessorWindowReservationV1 {
    pub(crate) fn original_state(
        &self,
        binding: &BftSignerBindingV3,
        current: &BftSignerStateV3,
        provider: &str,
    ) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
        let mut original = current.clone();
        original.successor_window = self
            .previous
            .as_ref()
            .map(BftSuccessorWindowPriorStateV1::restore);
        original.revision = self.body.expected_revision;
        original.state_digest = self.body.expected_state_digest.clone();
        let verified = validate_state(binding, Some(provider), &original)?;
        let bytes = encode_bounded(&original, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
        self.original_public_state.validate_bytes(&bytes)?;
        Ok(verified)
    }
}

/// Generic protocol mutations never alter the extension. Specialized CAS must
/// preserve every original hot/proof/WAL field and increment exactly once.
pub(crate) fn validate_successor_only_transition(
    binding: &BftSignerBindingV3,
    previous: &VerifiedBftSignerStateV3,
    next: &BftSignerStateV3,
    provider: &str,
) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
    if previous.state.revision.checked_add(1) != Some(next.revision) {
        return Err(invalid("successor CAS skipped revision"));
    }
    let mut unchanged = next.clone();
    unchanged.successor_window = previous.state.successor_window.clone();
    unchanged.revision = previous.state.revision;
    unchanged.state_digest = previous.state.state_digest.clone();
    if unchanged != previous.state || next.successor_window == previous.state.successor_window {
        return Err(invalid("successor CAS changed original hot state"));
    }
    let new = next
        .successor_window
        .as_ref()
        .ok_or_else(|| invalid("successor extension removed"))?;
    new.validate(binding, Some(provider))?;
    let old = previous
        .state
        .successor_window
        .clone()
        .unwrap_or_else(BftSuccessorWindowStateV1::empty);
    if let Some(reserved) = &new.reservation {
        if old.blocks_writes()
            || reserved
                .previous
                .as_ref()
                .map(BftSuccessorWindowPriorStateV1::restore)
                != previous.state.successor_window
            || new.requests != old.requests
            || new.minimum != old.minimum
        {
            return Err(invalid("successor reserve changed predecessor"));
        }
        let original = reserved.original_state(binding, next, provider)?;
        if original.state != previous.state {
            return Err(invalid(
                "successor reservation reconstructed wrong original",
            ));
        }
    } else if let Some(reserved) = &old.reservation {
        let last = new
            .requests
            .last()
            .ok_or_else(|| invalid("successor missing prepared original"))?;
        if new.requests.len() != old.requests.len() + 1
            || !new.requests.starts_with(&old.requests)
            || last.request.status != BftSuccessorWindowStatusV1::Prepared
            || last.request.intent.body != reserved.body
            || last.original_public_state != reserved.original_public_state
            || last.manifest != reserved.manifest
            || last.prepared_revision != next.revision
            || new.minimum != old.minimum
            || new.usage != old.usage
        {
            return Err(invalid("successor material preparation changed original"));
        }
    } else {
        let id = old
            .active_request_id
            .as_ref()
            .ok_or_else(|| invalid("successor missing original active request"))?;
        let a = old
            .request(id)
            .ok_or_else(|| invalid("successor missing prior original request"))?;
        let b = new
            .request(id)
            .ok_or_else(|| invalid("successor original request removed"))?;
        let mut expected = old.clone();
        let index = expected
            .requests
            .iter()
            .position(|r| r.request.intent.body.checkpoint.request_id == *id)
            .ok_or_else(|| invalid("successor request index"))?;
        expected.requests[index] = b.clone();
        expected.minimum = new.minimum.clone();
        if expected != *new
            || a.original_public_state != b.original_public_state
            || a.manifest != b.manifest
            || a.prepared_revision != b.prepared_revision
            || a.request.intent != b.request.intent
        {
            return Err(invalid("successor transaction rewrote original request"));
        }
        match (a.request.status, b.request.status) {
            (
                BftSuccessorWindowStatusV1::Prepared,
                BftSuccessorWindowStatusV1::InstalledPending,
            ) => {
                let receipt = b
                    .request
                    .receipt
                    .as_ref()
                    .ok_or_else(|| invalid("successor receipt missing"))?;
                if receipt.body.committed_revision != next.revision {
                    return Err(invalid("successor receipt differs from actual CAS"));
                }
            }
            (BftSuccessorWindowStatusV1::InstalledPending, BftSuccessorWindowStatusV1::Adopted) => {
                if a.request.receipt != b.request.receipt || old.minimum != new.minimum {
                    return Err(invalid("successor adoption rewrote minimum/receipt"));
                }
            }
            _ => return Err(invalid("successor illegal durable transaction edge")),
        }
    }
    validate_state(binding, Some(provider), next)
}
