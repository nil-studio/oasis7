//! Explicit retention capability and original material admission, separate from consensus keys.
use super::*;
use crate::protected_bft_v3::private_retention::*;
use crate::protected_bft_v3::private_retention_backend::*;
use crate::protected_bft_v3::private_retention_input::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Service-selected signing use, never supplied by the wire/request caller.
/// Material validity of an alternative does not make it a Local selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BftPrivateRetentionSigningPurposeV1 {
    LocalProposal,
    BlockVerification,
}

/// Independently configured private material verifier. Raw RPC refs or an
/// archive package are not this capability and do not grant a ReadGrant.
pub trait BftPrivateRetentionMaterialVerifierV1: Send + Sync {
    fn verify_signing_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &BftPrivateRetentionAdmissionInputV1,
        context: &crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        _purpose: BftPrivateRetentionSigningPurposeV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if context.signer().state.active_window.is_some() {
            return Err(invalid(
                "independent Active signing purpose verification unsupported",
            ));
        }
        // Preserve legacy Source material verification without silently enabling
        // any Active authority in a previously configured verifier.
        self.verify_ready_input_with_context(binding, entry, input, context, deadline)
    }

    fn verify_selection_input_with_context(
        &self,
        _binding: &BftSignerBindingV3,
        _entry: &BftPrivateRetentionEntryV1,
        _input: &BftPrivateRetentionAdmissionInputV1,
        _context: &crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        _deadline: Instant,
    ) -> Result<
        crate::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1,
        BftSignerErrorV3,
    > {
        Err(invalid(
            "independent original selection verification unsupported",
        ))
    }
    /// Validate the actual source's initialization against the original protected
    /// creation identity (or a separately authenticated pre-existing coverage
    /// seal). Directory emptiness/current public journal cannot establish it.
    fn verify_source_creation(
        &self,
        _binding: &BftSignerBindingV3,
        _state: &BftSignerStateV3,
        _creation: &crate::protected_bft_v3::private_retention_creation::ValidatedBftPrivateRetentionCreationV1,
        _deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(invalid(
            "private retention independent source creation verification unsupported",
        ))
    }
    /// Reproduce the exact canonical plan without publishing managed material.
    fn verify_reservation(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        entry: &BftPrivateRetentionEntryV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3>;
    fn verify_reservation_input(
        &self,
        _binding: &BftSignerBindingV3,
        _state: &BftSignerStateV3,
        _input: &BftPrivateRetentionAdmissionInputV1,
        _deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(invalid(
            "private retention actual original input verification unsupported",
        ))
    }
    fn verify_ready_input(
        &self,
        _binding: &BftSignerBindingV3,
        _state: &BftSignerStateV3,
        _entry: &BftPrivateRetentionEntryV1,
        _input: &BftPrivateRetentionAdmissionInputV1,
        _deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(invalid(
            "private retention actual atomic input publication verification unsupported",
        ))
    }
    /// The service supplies its exact original backend inventory, not local
    /// caller-selected metadata. Legacy verifiers cannot assert this boundary.
    fn verify_reservation_input_with_context(
        &self,
        _binding: &BftSignerBindingV3,
        _input: &BftPrivateRetentionAdmissionInputV1,
        _context: &crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        _deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(invalid(
            "private retention original inventory verification unsupported",
        ))
    }
    fn verify_ready_input_with_context(
        &self,
        _binding: &BftSignerBindingV3,
        _entry: &BftPrivateRetentionEntryV1,
        _input: &BftPrivateRetentionAdmissionInputV1,
        _context: &crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        _deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(invalid(
            "private retention original inventory readback verification unsupported",
        ))
    }
    /// Historical originals require their exact original parent and inventory;
    /// the current Ready-input method is not an authority to verify old heights.
    fn verify_seal_with_context(
        &self,
        _binding: &BftSignerBindingV3,
        _cut: &crate::protected_bft_v3::BftSuccessorWindowCutV1,
        _context: &crate::protected_bft_v3::BftPrivateRetentionSealVerificationContextV1<'_>,
        _deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(invalid(
            "independent historical original seal verification unsupported",
        ))
    }
    /// Independently reopen original Data/metadata, pins/WAL and all read/write
    /// dependencies; require exact plan/ref/owner/publication readback.
    fn verify_ready(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        entry: &BftPrivateRetentionEntryV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3>;
}

pub struct BftPrivateRetentionSupportV1 {
    pub(super) attestor: BftPrivateRetentionAttestorV1,
    verifier: Arc<dyn BftPrivateRetentionMaterialVerifierV1>,
    timeout: Duration,
    input_reader: Option<Arc<dyn BftPrivateRetentionAdmissionInputReaderV1>>,
}

impl BftPrivateRetentionSupportV1 {
    pub fn new(
        attestor: BftPrivateRetentionAttestorV1,
        verifier: Arc<dyn BftPrivateRetentionMaterialVerifierV1>,
        timeout: Duration,
    ) -> Result<Self, BftSignerErrorV3> {
        attestor.digest()?;
        if timeout.is_zero() || timeout > Duration::from_secs(120) {
            return Err(invalid("private retention attempt timeout bounds"));
        }
        Ok(Self {
            attestor,
            verifier,
            timeout,
            input_reader: None,
        })
    }

    pub fn with_admission_input_reader(
        mut self,
        reader: Arc<dyn BftPrivateRetentionAdmissionInputReaderV1>,
    ) -> Self {
        self.input_reader = Some(reader);
        self
    }

    pub(super) fn deadline(&self, start: Instant) -> Result<Instant, BftSignerErrorV3> {
        start
            .checked_add(self.timeout)
            .ok_or_else(|| invalid("private retention deadline overflow"))
    }

    pub(super) fn validate_config(
        &self,
        binding: &BftSignerBindingV3,
        provider: &str,
    ) -> Result<(), BftSignerErrorV3> {
        if self.attestor.provider_public_key != provider
            || self.attestor.source.binding_digest != binding.digest()?
            || self.attestor.source.handoff_digest != binding.handoff_record_id
            || self.attestor.source.material_source_digest != binding.material_identity
            || self.attestor.coverage_start_height != binding.initial_schedule().height()
        {
            return Err(invalid(
                "private retention external capability differs from original source",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_current(
        &self,
        current: &BftPrivateRetentionCurrentV1,
    ) -> Result<(), BftSignerErrorV3> {
        match (&current.signer.state.private_retention, &current.retention) {
            (None, None) if current.inputs.is_empty() => Ok(()),
            (Some(head), Some(ledger)) => {
                ledger.receipt.verify_map(&self.attestor, &ledger.map)?;
                super::super::private_retention_backend::validate_input_inventory(
                    ledger,
                    &current.inputs,
                )?;
                let body = &ledger.receipt.body;
                if head != &body.head
                    || current.signer.state.revision < body.after_revision
                    || (current.signer.state.revision == body.after_revision
                        && current.signer.state.state_digest != body.after_state_digest)
                    || current.signer.lease_generation < body.original_lease_generation
                {
                    return Err(invalid("private retention current State/receipt mismatch"));
                }
                Ok(())
            }
            _ => Err(invalid("private retention missing State/ledger")),
        }
    }
}

fn live(deadline: Instant) -> Result<(), BftSignerErrorV3> {
    if Instant::now() >= deadline {
        Err(invalid("private retention attempt expired"))
    } else {
        Ok(())
    }
}

impl BftSignerServiceV3 {
    pub(super) fn observe_private_retention(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: &BftSignerBackendSnapshotV3,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        let Some(support) = &self.private_retention_support else {
            if snapshot.state.private_retention.is_some() {
                return Err(invalid(
                    "private retention State requires explicit attestor capability",
                ));
            }
            return Ok(BftPrivateRetentionCurrentV1 {
                signer: snapshot.clone(),
                retention: None,
                inputs: Default::default(),
            });
        };
        let observed = runtime.backend.read_private_retention_current(
            snapshot.lease_generation,
            &snapshot.lease_token_digest,
        );
        let current = match observed {
            Ok(current) => current,
            Err(error) => {
                runtime.poisoned = true;
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        };
        let valid_inventory = if current
            .retention
            .as_ref()
            .is_some_and(|ledger| ledger.map.previous_seal_receipt_digest.is_some())
        {
            let sealed = runtime.backend.read_private_retention_sealed_current(
                snapshot.lease_generation,
                &snapshot.lease_token_digest,
            )?;
            sealed.current == current
                && sealed
                    .validate_inventory_with_verified_state(
                        &self.binding,
                        &support.attestor,
                        runtime.verified.as_ref(),
                    )
                    .is_ok()
        } else {
            support.validate_current(&current).is_ok()
        };
        if current.signer != *snapshot || !valid_inventory {
            runtime.poisoned = true;
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid(
                "private retention atomic current observation mismatch",
            ));
        }
        if let Some(ledger) = &current.retention {
            // Read the original operation under the current lease, but never
            // turn that historical lease/State into current signing authority.
            let result = runtime.backend.read_private_retention_operation(
                snapshot.lease_generation,
                &snapshot.lease_token_digest,
                &ledger.receipt.body.operation_digest,
            );
            let original = match result {
                Ok(original) => original,
                Err(error) => {
                    runtime.poisoned = true;
                    runtime.input_view_epoch.invalidate()?;
                    return Err(error);
                }
            };
            if original.inputs != current.inputs
                || original.retention != *ledger
                || original.signer.state.binding_digest != snapshot.state.binding_digest
                || original.signer.state.private_retention.as_ref()
                    != Some(&ledger.receipt.body.head)
                || original.signer.state.revision != ledger.receipt.body.after_revision
                || original.signer.state.state_digest != ledger.receipt.body.after_state_digest
                || original.signer.lease_generation != ledger.receipt.body.original_lease_generation
            {
                runtime.poisoned = true;
                runtime.input_view_epoch.invalidate()?;
                return Err(invalid(
                    "private retention original operation readback mismatch",
                ));
            }
            if let Err(error) = original.signer.state.validate_digest() {
                runtime.poisoned = true;
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        }
        Ok(current)
    }

    pub(super) fn require_private_ready(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: &BftSignerBackendSnapshotV3,
        height: u64,
        header_id: &str,
        purpose: BftPrivateRetentionSigningPurposeV1,
    ) -> Result<(), BftSignerErrorV3> {
        let Some(support) = &self.private_retention_support else {
            if snapshot.state.private_retention.is_some() {
                return Err(invalid(
                    "private retention signing unsupported without explicit capability",
                ));
            }
            return Ok(());
        };
        let current = self.observe_private_retention(runtime, snapshot)?;
        let ledger = current.retention.as_ref().ok_or_else(|| {
            invalid("private retention must be Reserved/Ready before fresh signing")
        })?;
        if ledger
            .map
            .entries
            .iter()
            .any(|entry| entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
        {
            return Err(invalid("private retention original publication unresolved"));
        }
        let mut matches = ledger
            .map
            .entries
            .iter()
            .filter(|entry| entry.key.height == height && entry.key.header_id == header_id);
        let entry = matches
            .next()
            .ok_or_else(|| invalid("private retention original candidate absent"))?;
        if matches.next().is_some() || !matches!(entry.fact, BftPrivateRetentionFactV1::Some { .. })
        {
            return Err(invalid(
                "private retention candidate source ambiguous or uncovered",
            ));
        }
        let deadline = runtime
            .successor_deadline
            .ok_or_else(|| invalid("private retention signing deadline unavailable"))?;
        let BftPrivateRetentionFactV1::Some {
            admission_digest, ..
        } = &entry.fact
        else {
            return Err(invalid("covered signing requires original Some input"));
        };
        let input = current
            .inputs
            .get(admission_digest)
            .ok_or_else(|| invalid("Ready original atomic input missing"))?;
        let sealed = if current
            .retention
            .as_ref()
            .is_some_and(|ledger| ledger.map.previous_seal_receipt_digest.is_some())
        {
            let sealed = runtime.backend.read_private_retention_sealed_current(
                snapshot.lease_generation,
                &snapshot.lease_token_digest,
            )?;
            if sealed.current != current {
                return Err(invalid(
                    "retention signing original sealed observation changed",
                ));
            }
            Some(sealed)
        } else {
            None
        };
        let context = match &sealed {
            Some(sealed) => crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1::from_backend_sealed_current_with_verified_state(&self.binding, &support.attestor, sealed, snapshot, runtime.verified.as_ref())?,
            None => crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1::from_backend_current_with_verified_state(&self.binding, &support.attestor, &current, snapshot, runtime.verified.as_ref())?,
        };
        support.verifier.verify_signing_input_with_context(
            &self.binding,
            entry,
            input,
            &context,
            purpose,
            deadline,
        )?;
        live(deadline)
    }

    pub(super) fn private_retention_command(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        if matches!(
            request.body.command,
            BftSignerCommandV3::SealPrivateRetention { .. }
        ) {
            return self.private_retention_seal_command(runtime, snapshot, request, request_digest);
        }
        let support = self
            .private_retention_support
            .as_ref()
            .ok_or_else(|| invalid("private retention capability unsupported"))?;
        let deadline = runtime
            .successor_deadline
            .ok_or_else(|| invalid("private retention deadline unavailable"))?;
        live(deadline)?;
        if snapshot.successor_archive_transfer.is_none() {
            return Err(invalid(
                "private retention map transfer requires explicit transfer capability",
            ));
        }
        let observed = self.observe_private_retention(runtime, &snapshot)?;
        let sealed = if observed
            .retention
            .as_ref()
            .is_some_and(|ledger| ledger.map.previous_seal_receipt_digest.is_some())
        {
            let sealed = runtime.backend.read_private_retention_sealed_current(
                snapshot.lease_generation,
                &snapshot.lease_token_digest,
            )?;
            if sealed.current != observed {
                return Err(invalid(
                    "retention context original sealed observation changed",
                ));
            }
            Some(sealed)
        } else {
            None
        };
        let context = match &sealed {
            Some(sealed) => crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1::from_backend_sealed_current_with_verified_state(&self.binding, &support.attestor, sealed, &snapshot, runtime.verified.as_ref())?,
            None => crate::protected_bft_v3::BftPrivateRetentionVerificationContextV1::from_backend_current_with_verified_state(&self.binding, &support.attestor, &observed, &snapshot, runtime.verified.as_ref())?,
        };
        let latest_anchor = sealed
            .as_ref()
            .map(|sealed| sealed.latest_seal())
            .transpose()?
            .flatten()
            .map(|seal| seal.anchor())
            .transpose()?;
        // Arc clones preserve the backend-owned originals while the immutable
        // pre-operation context stays borrowed through actual verification.
        let previous = observed.retention.clone();
        let mut inputs = observed.inputs.clone();
        // Keep the real receiving buffer permit alive through actual verification,
        // atomic State/map/receipt/input persistence and exact readback below.
        let mut input_read: Option<BftPrivateRetentionAdmissionInputReadV1> = None;
        if previous.is_none() {
            let creation = runtime.backend.read_private_retention_creation(
                snapshot.lease_generation,
                &snapshot.lease_token_digest,
            )?;
            creation.validate_for(&self.binding, &support.attestor, &snapshot)?;
            support.verifier.verify_source_creation(
                &self.binding,
                &snapshot.state,
                &creation,
                deadline,
            )?;
            live(deadline)?;
        }
        let mut map = previous.as_ref().map_or_else(
            || BftPrivateRetentionMapV1 {
                schema_version: 1,
                source: support.attestor.source.clone(),
                coverage_start_height: support.attestor.coverage_start_height,
                previous_seal_receipt_digest: None,
                entries: Vec::new(),
            },
            |ledger| ledger.map.clone(),
        );
        let mut selection_index = previous.as_ref().and_then(|p| p.selection_index.clone());
        let mut selection_operation = None;
        let (revision, digest, target, changed) = match &request.body.command {
            BftSignerCommandV3::ProvisionPrivateRetention {
                expected_revision,
                expected_state_digest,
            } => (
                *expected_revision,
                expected_state_digest,
                None,
                previous.is_none(),
            ),
            BftSignerCommandV3::GetPrivateRetention => (
                snapshot.state.revision,
                &snapshot.state.state_digest,
                None,
                false,
            ),
            BftSignerCommandV3::ReservePrivateRetention { .. } => {
                return Err(invalid(
                    "metadata-only Some reservation cannot retain the original input",
                ));
            }
            BftSignerCommandV3::ReservePrivateRetentionInput {
                expected_revision,
                expected_state_digest,
                reference,
            } => {
                reference.validate()?;
                if reference.source != support.attestor.source {
                    return Err(invalid("admission input original source differs"));
                }
                let changed = match map
                    .entries
                    .binary_search_by(|old| old.key.cmp(&reference.key))
                {
                    Ok(index) => {
                        if !matches!(&map.entries[index].fact, BftPrivateRetentionFactV1::Some { admission_digest, .. } if admission_digest == &reference.admission_digest)
                        {
                            return Err(invalid("admission original fact conflict"));
                        }
                        let original = inputs
                            .get(&reference.admission_digest)
                            .ok_or_else(|| invalid("original atomic input absent"))?;
                        reference.validate_input(original)?;
                        false
                    }
                    Err(index) => {
                        let reader = support.input_reader.as_ref().ok_or_else(|| {
                            invalid("authorized encrypted original input reader unavailable")
                        })?;
                        input_read = Some(reader.read_original(reference, deadline)?);
                        let read = input_read
                            .as_ref()
                            .ok_or_else(|| invalid("original input buffer hold absent"))?;
                        let input = read.input();
                        reference.validate_input(input)?;
                        support.verifier.verify_reservation_input_with_context(
                            &self.binding,
                            input,
                            &context,
                            deadline,
                        )?;
                        live(deadline)?;
                        map.entries.insert(
                            index,
                            BftPrivateRetentionEntryV1 {
                                key: reference.key.clone(),
                                fact: input.fact()?,
                                lifecycle: BftPrivateRetentionLifecycleV1::ReservedPending,
                            },
                        );
                        inputs.insert(
                            reference.admission_digest.clone(),
                            read.original_for_atomic_storage(),
                        );
                        true
                    }
                };
                (
                    *expected_revision,
                    expected_state_digest,
                    Some(reference.key.clone()),
                    changed,
                )
            }
            BftSignerCommandV3::ReadyPrivateRetention {
                expected_revision,
                expected_state_digest,
                key,
            }
            | BftSignerCommandV3::ReadyPrivateRetentionOriginalLocal {
                expected_revision,
                expected_state_digest,
                key,
            } => {
                let index = map
                    .entries
                    .binary_search_by(|entry| entry.key.cmp(key))
                    .map_err(|_| invalid("private retention original reservation absent"))?;
                let mut changed =
                    map.entries[index].lifecycle != BftPrivateRetentionLifecycleV1::Ready;
                if changed {
                    let BftPrivateRetentionFactV1::Some {
                        admission_digest, ..
                    } = &map.entries[index].fact
                    else {
                        return Err(invalid("Ready requires original Some input"));
                    };
                    let input = inputs
                        .get(admission_digest)
                        .ok_or_else(|| invalid("Ready original atomic input absent"))?;
                    support.verifier.verify_ready_input_with_context(
                        &self.binding,
                        &map.entries[index],
                        input,
                        &context,
                        deadline,
                    )?;
                    live(deadline)?;
                    map.entries[index].lifecycle = BftPrivateRetentionLifecycleV1::Ready;
                }
                if matches!(
                    request.body.command,
                    BftSignerCommandV3::ReadyPrivateRetentionOriginalLocal { .. }
                ) {
                    let entry = &map.entries[index];
                    let reference =
                        BftPrivateRetentionAdmissionInputRefV1::from_entry(&map.source, entry)?;
                    let input = inputs
                        .get(&reference.admission_digest)
                        .ok_or_else(|| invalid("selection original input absent"))?;
                    reference.validate_input(input)?;
                    let identity = support.verifier.verify_selection_input_with_context(
                        &self.binding,
                        entry,
                        input,
                        &context,
                        deadline,
                    )?;
                    if identity.kind() != crate::protected_bft_v3::BftPrivateRetentionSelectionKindV1::OriginalLocal { return Err(invalid("original-local Ready kind differs")); }
                    let typed_operation = crate::protected_bft_v3::BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
                        reference: reference.clone(), expected_revision: *expected_revision,
                        expected_state_digest: expected_state_digest.clone(),
                        expected_head_digest: previous.as_ref().ok_or_else(|| invalid("selection original head absent"))?.receipt.body.head.digest()?,
                    };
                    let operation = typed_operation.digest()?;
                    let (next_index, selected) =
                        crate::protected_bft_v3::private_retention_selection::select(
                            selection_index.as_ref(),
                            &map.source,
                            reference,
                            identity,
                            typed_operation,
                        )?;
                    selection_index = Some(next_index);
                    changed |= selected;
                    if selected {
                        selection_operation = Some(operation);
                    }
                }
                (
                    *expected_revision,
                    expected_state_digest,
                    Some(key.as_ref().clone()),
                    changed,
                )
            }
            BftSignerCommandV3::SelectPrivateRetention { selection } => {
                let previous_ledger = previous
                    .as_ref()
                    .ok_or_else(|| invalid("selection original ledger absent"))?;
                selection.reference.validate()?;
                if selection.reference.key != selection.key
                    || selection.reference.source != map.source
                {
                    return Err(invalid("selection request exact source/key differs"));
                }
                let operation = selection.digest()?;
                let existing = selection_index.as_ref().and_then(|i| {
                    i.selections.iter().find(|v| {
                        v.key.ledger_origin_digest == selection.key.ledger_origin_digest
                            && v.key.height == selection.key.height
                    })
                });
                let retry = existing.is_some_and(|v| {
                    v.reference == selection.reference && v.operation_digest == operation
                });
                if retry {
                    let selected =
                        existing.ok_or_else(|| invalid("selection retry original absent"))?;
                    let original = runtime.backend.read_private_retention_operation(
                        snapshot.lease_generation,
                        &snapshot.lease_token_digest,
                        &operation,
                    )?;
                    crate::protected_bft_v3::private_retention_selection::validate_original_selection(&self.binding, &support.attestor, previous_ledger, &original.signer.state, &original.retention, selected)?;
                    crate::protected_bft_v3::private_retention_selection::preflight_original_operation(&original.signer.state, &original.retention)?;
                    live(deadline)?;
                    return self.private_retention_reply_with_anchor(
                        runtime,
                        snapshot,
                        previous_ledger.clone(),
                        latest_anchor,
                        request,
                        request_digest,
                    );
                }
                if !retry
                    && (existing.is_some()
                        || selection.expected_choice.is_some()
                        || selection.expected_head_digest
                            != previous_ledger.receipt.body.head.digest()?)
                {
                    return Err(invalid("selection predecessor or first choice differs"));
                }
                let entry = map
                    .entries
                    .iter()
                    .find(|e| {
                        e.key == selection.key
                            && e.lifecycle == BftPrivateRetentionLifecycleV1::Ready
                    })
                    .ok_or_else(|| invalid("selection requires original Ready"))?;
                if BftPrivateRetentionAdmissionInputRefV1::from_entry(&map.source, entry)?
                    != selection.reference
                {
                    return Err(invalid("selection original fact differs"));
                }
                let input = inputs
                    .get(&selection.reference.admission_digest)
                    .ok_or_else(|| invalid("selection original Input absent"))?;
                selection.reference.validate_input(input)?;
                let identity = support.verifier.verify_selection_input_with_context(
                    &self.binding,
                    entry,
                    input,
                    &context,
                    deadline,
                )?;
                live(deadline)?;
                let (next_index, changed) =
                    crate::protected_bft_v3::private_retention_selection::select(
                        selection_index.as_ref(),
                        &map.source,
                        selection.reference.clone(),
                        identity,
                        crate::protected_bft_v3::BftPrivateRetentionSelectionOperationV1::Select {
                            request: selection.as_ref().clone(),
                        },
                    )?;
                selection_index = Some(next_index);
                if changed {
                    selection_operation = Some(operation);
                }
                (
                    selection.expected_revision,
                    &selection.expected_state_digest,
                    Some(selection.key.clone()),
                    changed,
                )
            }
            _ => return Err(invalid("private retention command kind")),
        };
        map.validate()?;
        let (current, ledger) = if changed {
            if revision != snapshot.state.revision || digest != &snapshot.state.state_digest {
                return Err(invalid("private retention expected checkpoint stale"));
            }
            let previous_receipt_digest = previous
                .as_ref()
                .map(|ledger| {
                    digest_hex("oasis7.bft.private-retention.v1.receipt", &ledger.receipt)
                })
                .transpose()?;
            let sequence = previous.as_ref().map_or(Ok(1), |ledger| {
                ledger
                    .receipt
                    .body
                    .head
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| invalid("private retention sequence overflow"))
            })?;
            let bytes = map.canonical_bytes()?;
            let mut head = BftPrivateRetentionHeadV1 {
                selection_commitment: selection_index
                    .as_ref()
                    .map(|i| i.commitment())
                    .transpose()?,
                schema_version: if selection_index.is_some() { 3 } else { 2 },
                source: map.source.clone(),
                capability_digest: support.attestor.digest()?,
                sequence,
                map_digest: map.digest()?,
                map_byte_hash: blake3::hash(&bytes).to_hex().to_string(),
                entry_count: u32::try_from(map.entries.len())
                    .map_err(|_| invalid("private retention count conversion"))?,
                previous_receipt_digest,
                retained_material_bytes: None,
            };
            head.retained_material_bytes = Some(retention_material_bound(
                &snapshot,
                previous.as_ref(),
                &map,
                &head,
                &inputs,
            )?);
            let next = snapshot.state.with_private_retention_head(head.clone())?;
            let body = BftPrivateRetentionReceiptBodyV1 {
                head,
                operation_digest: selection_operation.unwrap_or(digest_hex(
                    "oasis7.bft.private-retention.v1.operation",
                    &(
                        &target,
                        &map,
                        &previous
                            .as_ref()
                            .map(|ledger| &ledger.receipt.body.operation_digest),
                    ),
                )?),
                original_lease_generation: snapshot.lease_generation,
                before_revision: snapshot.state.revision,
                before_state_digest: snapshot.state.state_digest.clone(),
                after_revision: next.revision,
                after_state_digest: next.state_digest.clone(),
            };
            let receipt = BftPrivateRetentionReceiptV1 {
                signature_hex: sign_preimage(self.provider_key.as_ref(), RECEIPT_DOMAIN, &body)?,
                body,
            };
            let mut after = snapshot.clone();
            after.state = next;
            let transaction = BftPrivateRetentionCasV1 {
                before: snapshot,
                expected_retention: previous,
                after: BftPrivateRetentionSnapshotV1 {
                    inputs,
                    signer: after,
                    retention: BftPrivateRetentionLedgerV1 {
                        selection_index,
                        map,
                        receipt,
                    },
                },
            };
            transaction.validate_envelope_with_anchor(&support.attestor, latest_anchor.as_ref())?;
            if matches!(
                request.body.command,
                BftSignerCommandV3::SelectPrivateRetention { .. }
                    | BftSignerCommandV3::ReadyPrivateRetentionOriginalLocal { .. }
            ) {
                crate::protected_bft_v3::private_retention_selection::preflight_original_operation(
                    &transaction.after.signer.state,
                    &transaction.after.retention,
                )?;
            }
            let upper = self.private_retention_reply_bound_with_anchor(
                &transaction.after.signer,
                &transaction.after.retention,
                latest_anchor.as_ref(),
                request,
                request_digest,
            )?;
            let counter = transaction
                .after
                .signer
                .successor_archive_transfer
                .ok_or_else(|| invalid("private retention transfer capability missing"))?
                .reserved_response_bytes;
            super::super::archive_transfer::reserve_successor_archive_transfer_v1(
                &transaction.after.signer,
                transaction.after.signer.lease_generation,
                &transaction.after.signer.lease_token_digest,
                transaction.after.signer.state.revision,
                &transaction.after.signer.state.state_digest,
                counter,
                upper,
            )?;
            live(deadline)?;
            runtime.input_view_epoch.invalidate()?;
            let response = runtime
                .backend
                .compare_and_swap_private_retention(&transaction);
            let committed = match response {
                Ok(value) => value,
                Err(error) => {
                    runtime.poisoned = true;
                    runtime.input_view_epoch.invalidate()?;
                    return Err(invalid(format!(
                        "private retention CAS outcome unknown; provider poisoned: {error}"
                    )));
                }
            };
            if let Err(error) = transaction.validate_exact_readback(&committed) {
                runtime.poisoned = true;
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
            let verified =
                match self.validate_provider_state(&committed.signer.state, Some(deadline)) {
                    Ok(verified) => verified,
                    Err(error) => {
                        runtime.poisoned = true;
                        runtime.input_view_epoch.invalidate()?;
                        return Err(error);
                    }
                };
            runtime.verified = Some(verified);
            (committed.signer, committed.retention)
        } else {
            (
                snapshot,
                previous.ok_or_else(|| invalid("private retention original ledger absent"))?,
            )
        };
        live(deadline)?;
        let _original_input_hold = input_read;
        self.private_retention_reply_with_anchor(
            runtime,
            current,
            ledger,
            latest_anchor,
            request,
            request_digest,
        )
    }

    fn private_retention_reply_with_anchor(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: BftSignerBackendSnapshotV3,
        ledger: BftPrivateRetentionLedgerV1,
        latest_seal: Option<crate::protected_bft_v3::BftPrivateRetentionSealAnchorV1>,
        request: &SignedBftSignerRequestV3,
        digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let amount = self.private_retention_reply_bound_with_anchor(
            &snapshot,
            &ledger,
            latest_seal.as_ref(),
            request,
            digest,
        )?;
        let charged = self.reserve_archive_transfer(runtime, &snapshot, amount)?;
        let result = BftSignerReplyKindV3::PrivateRetention {
            ledger: Box::new(ledger),
            latest_seal: latest_seal.map(Box::new),
            reserved_response_bytes: charged
                .successor_archive_transfer
                .ok_or_else(|| invalid("private retention charged counter missing"))?
                .reserved_response_bytes,
        };
        self.state_reply(charged, request, digest, result)
    }

    fn private_retention_reply_preflight_with_anchor(
        &self,
        snapshot: &BftSignerBackendSnapshotV3,
        ledger: &BftPrivateRetentionLedgerV1,
        latest: Option<&crate::protected_bft_v3::BftPrivateRetentionSealAnchorV1>,
        request: &SignedBftSignerRequestV3,
        digest: &str,
    ) -> Result<(), BftSignerErrorV3> {
        let amount = self
            .private_retention_reply_bound_with_anchor(snapshot, ledger, latest, request, digest)?;
        let counter = snapshot
            .successor_archive_transfer
            .ok_or_else(|| invalid("retention transfer capability missing"))?
            .reserved_response_bytes;
        super::super::archive_transfer::reserve_successor_archive_transfer_v1(
            snapshot,
            snapshot.lease_generation,
            &snapshot.lease_token_digest,
            snapshot.state.revision,
            &snapshot.state.state_digest,
            counter,
            amount,
        )?;
        Ok(())
    }

    fn private_retention_reply_bound_with_anchor(
        &self,
        snapshot: &BftSignerBackendSnapshotV3,
        ledger: &BftPrivateRetentionLedgerV1,
        latest_seal: Option<&crate::protected_bft_v3::BftPrivateRetentionSealAnchorV1>,
        request: &SignedBftSignerRequestV3,
        digest: &str,
    ) -> Result<u64, BftSignerErrorV3> {
        let checkpoint = self.make_checkpoint(
            snapshot.state.clone(),
            snapshot.lease_generation,
            request.body.challenge.clone(),
            digest.into(),
        )?;
        let result = BftSignerReplyKindV3::PrivateRetention {
            ledger: Box::new(ledger.clone()),
            latest_seal: latest_seal.cloned().map(Box::new),
            reserved_response_bytes: u64::MAX,
        };
        let upper = SignedBftSignerReplyV3 {
            body: BftSignerReplyBodyV3 {
                schema_version: 3,
                binding_digest: self.binding.digest()?,
                request_digest: digest.into(),
                challenge: request.body.challenge.clone(),
                lease_generation: snapshot.lease_generation,
                lease_token: None,
                state_revision: snapshot.state.revision,
                state_digest: snapshot.state.state_digest.clone(),
                checkpoint: Some(Box::new(checkpoint)),
                result: result.clone(),
            },
            provider_signature_hex: "0".repeat(128),
        };
        u64::try_from(encode_frame(&upper)?.len())
            .map_err(|_| invalid("private retention response bound"))
    }
}

#[path = "private_retention_seal.rs"]
mod seal;
