//! Original empty-coverage activation and authenticated covered-empty cold reopen.
//! The parent obtains checkpoint/ledger from one fresh authenticated lease observation.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionCreationMarkerV1,
    BftPrivateRetentionLedgerV1, BftSignerLeaseV3, ValidatedBftPrivateRetentionObservationV1,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

const ACTIVATION_DOMAIN: &str = "oasis7.formal.bft.private-source.activation.v1";
pub(in super::super) const MAX_ACTIVATION_BYTES: usize = 64 * 1024;

pub(crate) struct ActivateBftPrivateSourceV1<'a> {
    pub(crate) binding: &'a BftSignerBindingV3,
    /// Independently configured trust; never select this from the stored capsule.
    pub(crate) fixed_attestor: &'a BftPrivateRetentionAttestorV1,
    pub(crate) original_marker: &'a BftPrivateRetentionCreationMarkerV1,
    pub(crate) lease: &'a mut dyn BftSignerLeaseV3,
    pub(crate) deadline: Instant,
}

/// A move-only local opening after the original fresh observation completed.
/// This retains immutable initialization identity, not a current lease, reader
/// grant or permission to sign/admit a candidate. Node prepare obtains its own
/// fresh current observation before execution resumes.
pub(crate) struct PendingBftPrivateSourceActivationOpenV1 {
    prepared: Box<PreparedBftPrivateSourceOriginV1>,
    activation: ValidatedSourceActivationV1,
    binding: BftSignerBindingV3,
    deadline: Instant,
}
impl PendingBftPrivateSourceActivationOpenV1 {
    pub(crate) fn open(self) -> Result<Box<FormalBftHeaderNodeDriverV3>, String> {
        successor_archive::live(self.deadline)?;
        let Self {
            prepared,
            activation,
            binding,
            deadline,
        } = self;
        FormalBftHeaderNodeDriverV3::open_covered_source(
            super::super::node_execution::source_activation::CoveredSourceOpenV1 {
                records: prepared.records,
                store: prepared.store,
                inputs: prepared.inputs,
                origin: prepared.origin,
                carriers: prepared.carriers,
                binding: &binding,
                activation: &activation,
                deadline,
            },
        )
    }
}

/// Immutable original initialization bytes. Current challenge, current lease and
/// transfer counter deliberately do not enter the durable artifact's identity.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct SourceActivationRecordV1 {
    schema_version: u32,
    domain: String,
    marker: BftPrivateRetentionCreationMarkerV1,
    capability_digest: String,
    original_empty_coverage: BftPrivateRetentionLedgerV1,
}

impl SourceActivationRecordV1 {
    pub(in super::super) fn bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode original source activation")?;
        if bytes.len() > MAX_ACTIVATION_BYTES {
            return Err("source activation byte capacity".into());
        }
        Ok(bytes)
    }

    pub(super) fn digest(&self) -> Result<String, String> {
        Ok(blake3_hex(&self.bytes()?))
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_ACTIVATION_BYTES {
            return Err("source activation byte capacity".into());
        }
        let record: Self =
            serde_cbor::from_slice(bytes).map_err(|_| "decode original source activation")?;
        if record.bytes()? != bytes {
            return Err("noncanonical original source activation".into());
        }
        Ok(record)
    }

    pub(super) fn validate_original(
        &self,
        binding: &BftSignerBindingV3,
        fixed: &BftPrivateRetentionAttestorV1,
    ) -> Result<(), String> {
        let ledger = &self.original_empty_coverage;
        let body = &ledger.receipt.body;
        let initial = BftSignerStateV3::from_handoff(binding).map_err(|e| e.to_string())?;
        oasis7_distfs::protected_bft_v3::BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: fixed.provider_public_key.clone(),
            binding_digest: binding.digest().map_err(|e| e.to_string())?,
        }
        .validate_for(binding)
        .map_err(|e| e.to_string())?;
        if self.schema_version != 1
            || self.domain != ACTIVATION_DOMAIN
            || self.capability_digest != fixed.digest().map_err(|e| e.to_string())?
            || self.marker.digest().map_err(|e| e.to_string())? != fixed.creation_marker_digest
            || self.marker.source != fixed.source
            || fixed.source.binding_digest != binding.digest().map_err(|e| e.to_string())?
            || fixed.source.handoff_digest != binding.handoff_record_id
            || fixed.source.material_source_digest != binding.material_identity
            || self.marker.initial_height != binding.initial_schedule().height()
            || self.marker.initial_state_digest != initial.state_digest
            || self.marker.activation_digest != binding.activation_digest
            || body.before_revision != initial.revision
            || body.before_state_digest != initial.state_digest
            || body.after_revision
                != initial
                    .revision
                    .checked_add(1)
                    .ok_or("source activation revision overflow")?
            || body.head.sequence != 1
            || body.head.previous_receipt_digest.is_some()
            || ledger.map.previous_seal_receipt_digest.is_some()
            || !ledger.map.entries.is_empty()
            || ledger.map.coverage_start_height != binding.initial_schedule().height()
        {
            return Err("original source activation is not the first covered H transition".into());
        }
        ledger
            .receipt
            .verify_map(fixed, &ledger.map)
            .map_err(|e| e.to_string())?;
        // Check only the exact historical H->head transition. This expected
        // State is never exposed as a current authority or returned to callers.
        let mut after = initial;
        after.private_retention = Some(body.head.clone());
        after.revision = body.after_revision;
        after.state_digest = body.after_state_digest.clone();
        after
            .validate(binding, &fixed.provider_public_key)
            .map_err(|e| e.to_string())
    }
}

/// Only the fresh lease consumer in the parent module can construct this value;
/// a serialized capsule alone never authorizes managed source publication.
pub(in super::super) struct ValidatedSourceActivationV1 {
    record: SourceActivationRecordV1,
    fixed_attestor: BftPrivateRetentionAttestorV1,
}

impl ValidatedSourceActivationV1 {
    pub(super) fn from_observation(
        binding: &BftSignerBindingV3,
        fixed: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
        observation: &ValidatedBftPrivateRetentionObservationV1,
    ) -> Result<Self, String> {
        let checkpoint = observation.checkpoint();
        let record = SourceActivationRecordV1 {
            schema_version: 1,
            domain: ACTIVATION_DOMAIN.into(),
            marker: marker.clone(),
            capability_digest: fixed.digest().map_err(|e| e.to_string())?,
            original_empty_coverage: observation.ledger().clone(),
        };
        record.validate_original(binding, fixed)?;
        let initial = BftSignerStateV3::from_handoff(binding).map_err(|e| e.to_string())?;
        let body = &record.original_empty_coverage.receipt.body;
        checkpoint
            .state
            .validate(binding, &fixed.provider_public_key)
            .map_err(|e| e.to_string())?;
        let mut before = checkpoint.state.clone();
        before.private_retention = None;
        before.revision = initial.revision;
        before.state_digest = initial.state_digest.clone();
        if before != initial
            || checkpoint.provider_public_key != fixed.provider_public_key
            || checkpoint.state.private_retention.as_ref() != Some(&body.head)
            || checkpoint.state.revision != body.after_revision
            || checkpoint.state.state_digest != body.after_state_digest
            || checkpoint.lease_generation < body.original_lease_generation
        {
            return Err("source activation differs from current original empty coverage".into());
        }
        record.bytes()?;
        Ok(Self {
            record,
            fixed_attestor: fixed.clone(),
        })
    }

    /// Reopening consumes both the original immutable capsule and a new opaque
    /// current-lease observation. Historical receipt bytes never select the
    /// current lease, revision, clock or signing journal.
    pub(super) fn from_original_and_current(
        binding: &BftSignerBindingV3,
        fixed: &BftPrivateRetentionAttestorV1,
        bytes: &[u8],
        observation: &ValidatedBftPrivateRetentionObservationV1,
    ) -> Result<Self, String> {
        let record = SourceActivationRecordV1::decode(bytes)?;
        record.validate_original(binding, fixed)?;
        let checkpoint = observation.checkpoint();
        checkpoint
            .state
            .validate(binding, &fixed.provider_public_key)
            .map_err(|e| e.to_string())?;
        let original = &record.original_empty_coverage;
        let current = observation.ledger();
        if checkpoint.provider_public_key != fixed.provider_public_key
            || current.map.source != original.map.source
            || current.map.coverage_start_height != original.map.coverage_start_height
            || current.receipt.body.head.sequence < original.receipt.body.head.sequence
            || checkpoint.state.private_retention.as_ref() != Some(&current.receipt.body.head)
            || current.receipt.body.head.capability_digest != record.capability_digest
            || checkpoint.state.revision < original.receipt.body.after_revision
            || checkpoint.state.successor_window.is_some()
            || checkpoint.state.active_window.is_some()
        {
            return Err(
                "covered source cold open differs from authenticated original coverage".into(),
            );
        }

        Ok(Self {
            record,
            fixed_attestor: fixed.clone(),
        })
    }

    pub(in super::super) fn fixed_attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        &self.fixed_attestor
    }
    pub(in super::super) fn bytes(&self) -> Result<Vec<u8>, String> {
        self.record.bytes()
    }
    pub(in super::super) fn reference(&self) -> Result<String, String> {
        self.record.digest()
    }
    pub(in super::super) fn marker(&self) -> &BftPrivateRetentionCreationMarkerV1 {
        &self.record.marker
    }
}

/// Independently configured current role must still be the role fixed by the
/// immutable original activation capsule. No new Client role rebinds the source.
pub(in super::super) fn validate_input_context_role(
    bytes: &[u8],
    binding: &BftSignerBindingV3,
    context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
) -> Result<(), String> {
    validate_input_role(bytes, binding, context.attestor(), &context.signer().state)
}

pub(in super::super) fn validate_input_role(
    bytes: &[u8],
    binding: &BftSignerBindingV3,
    role: &BftPrivateRetentionAttestorV1,
    state: &BftSignerStateV3,
) -> Result<(), String> {
    let record = SourceActivationRecordV1::decode(bytes)?;
    record.validate_original(binding, role)?;
    let head = state
        .private_retention
        .as_ref()
        .ok_or("source original current head missing")?;
    if head.capability_digest != record.capability_digest || head.source != record.marker.source {
        return Err("source original capsule differs from current protected role".into());
    }
    Ok(())
}

/// Canonical local carrier checks only. This does not authorize current state;
/// cold writable admission separately consumes fixed trust and an opaque live
/// lease observation.
pub(in super::super) fn validate_stored_owner(
    bytes: &[u8],
    ledger: &str,
    material: &str,
    instance: &str,
    height: u64,
) -> Result<(), String> {
    let record = SourceActivationRecordV1::decode(bytes)?;
    let marker = &record.marker;
    marker.digest().map_err(|e| e.to_string())?;
    if marker.ledger_origin_digest != ledger
        || marker.source.material_source_digest != material
        || marker.creation_identity != instance
        || height.checked_add(1) != Some(marker.initial_height)
    {
        return Err("stored source activation differs from private original owner".into());
    }
    Ok(())
}

impl PreparedBftPrivateSourceOriginV1 {
    pub(crate) fn activate(
        self: Box<Self>,
        request: ActivateBftPrivateSourceV1<'_>,
    ) -> Result<Box<FormalBftHeaderNodeDriverV3>, String> {
        self.authenticate_activation(request)?.open()
    }

    pub(crate) fn authenticate_activation(
        self: Box<Self>,
        request: ActivateBftPrivateSourceV1<'_>,
    ) -> Result<PendingBftPrivateSourceActivationOpenV1, String> {
        successor_archive::live(request.deadline)?;
        self.verify_unchanged()?;
        let observation = request
            .lease
            .observe_private_retention(request.deadline)
            .map_err(|e| e.to_string())?;
        let activation = ValidatedSourceActivationV1::from_observation(
            request.binding,
            request.fixed_attestor,
            request.original_marker,
            &observation,
        )?;
        if activation.marker().creation_identity != self.creation_identity
            || activation.marker().ledger_origin_digest != self.origin.digest()?
        {
            return Err("source activation differs from prepared original Origin".into());
        }
        Ok(PendingBftPrivateSourceActivationOpenV1 {
            prepared: self,
            activation,
            binding: request.binding.clone(),
            deadline: request.deadline,
        })
    }

    /// Only an existing covered private Origin can enter this path. Missing or
    /// replaced capsules never become an empty legacy initialization.
    pub(crate) fn reopen_covered_empty(
        request: FreshBftPrivateSourceInputsV1,
        activation_request: ActivateBftPrivateSourceV1<'_>,
    ) -> Result<Box<FormalBftHeaderNodeDriverV3>, String> {
        successor_archive::live(activation_request.deadline)?;
        let records = canonical_storage_path(&request.records)?;
        let origin_bytes = MAX_H_BYTES;
        let ledger = storage::Ledger::open(&records, false, origin_bytes)?;
        ledger.require_completed()?;

        let origin = ledger.origin()?.clone();
        let carriers = Arc::new(super::super::successor_carriers::InstallationRootV1::new(
            records.clone(),
        )?);
        carriers.verify_origin_carriers(&origin, activation_request.deadline)?;
        if origin.source_creation_identity()
            != Some(
                activation_request
                    .original_marker
                    .creation_identity
                    .as_str(),
            )
            || origin.digest()? != activation_request.original_marker.ledger_origin_digest
        {
            return Err("covered-empty cold original creation differs".into());
        }
        let bytes = super::super::node_execution::source_activation::read_original_activation(
            &records,
            request.store.clone(),
            request.source.clone(),
            &activation_request.original_marker.creation_identity,
        )?;
        let observation = activation_request
            .lease
            .observe_private_retention(activation_request.deadline)
            .map_err(|e| e.to_string())?;
        if ledger
            .records()
            .map(|r| r.input().certificate)
            .ne(observation.checkpoint().state.history.iter().cloned())
        {
            return Err("covered source cold history differs from current protected parent".into());
        }
        let validated = match bytes {
            Some(bytes) => ValidatedSourceActivationV1::from_original_and_current(
                activation_request.binding,
                activation_request.fixed_attestor,
                &bytes,
                &observation,
            )?,
            None => ValidatedSourceActivationV1::from_observation(
                activation_request.binding,
                activation_request.fixed_attestor,
                activation_request.original_marker,
                &observation,
            )?,
        };
        if validated.marker() != activation_request.original_marker {
            return Err("covered cold marker differs from original fixed source".into());
        }
        drop(ledger);
        FormalBftHeaderNodeDriverV3::open_covered_source(
            super::super::node_execution::source_activation::CoveredSourceOpenV1 {
                records,
                store: request.store,
                inputs: Inputs {
                    anchor: request.anchor,
                    target: request.target,
                    material: request.material,
                    source: request.source,
                },
                origin,
                carriers,
                binding: activation_request.binding,
                activation: &validated,
                deadline: activation_request.deadline,
            },
        )
    }
}

#[cfg(test)]
thread_local! { static INTERRUPT_BEFORE_CAPSULE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(in super::super) fn before_capsule_for_test() -> Result<(), String> {
    if INTERRUPT_BEFORE_CAPSULE.with(|flag| flag.get()) {
        Err("injected original source activation before capsule".into())
    } else {
        Ok(())
    }
}
#[cfg(test)]
impl PreparedBftPrivateSourceOriginV1 {
    pub(crate) fn activate_before_capsule_for_test(
        self: Box<Self>,
        request: ActivateBftPrivateSourceV1<'_>,
    ) -> Result<Box<FormalBftHeaderNodeDriverV3>, String> {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                INTERRUPT_BEFORE_CAPSULE.with(|flag| flag.set(false));
            }
        }
        INTERRUPT_BEFORE_CAPSULE.with(|flag| flag.set(true));
        let _reset = Reset;
        self.activate(request)
    }
}

pub(in super::super) fn validate_archive_capsule(
    bytes: &[u8],
    binding: &BftSignerBindingV3,
    role: &BftPrivateRetentionAttestorV1,
    origin: &str,
    material: &str,
    instance: &str,
) -> Result<BftPrivateRetentionLedgerV1, String> {
    let record = SourceActivationRecordV1::decode(bytes)?;
    record.validate_original(binding, role)?;
    if record.marker.ledger_origin_digest != origin
        || record.marker.creation_identity != instance
        || record.marker.source.material_source_digest != material
    {
        return Err("archived source capsule original owner differs".into());
    }
    Ok(record.original_empty_coverage)
}
