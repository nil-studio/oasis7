//! Software-only atomic backend. This is not a hardware or disk rollback anchor.
use super::backend::*;
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::private_retention::BftPrivateRetentionAttestorV1;
use super::private_retention_backend::*;
use super::private_retention_creation::*;
use super::state::BftSignerStateV3;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
#[path = "private_retention_memory_validated_input.rs"]
mod validated_input;
use validated_input::ValidatedMemoryInputs;

#[cfg(test)]
#[path = "private_retention_memory_hash_probe.rs"]
pub(super) mod hash_probe;
#[cfg(not(test))]
type MemoryGuard<'a> = MutexGuard<'a, Memory>;
#[cfg(test)]
struct MemoryGuard<'a> {
    _probe: hash_probe::Scope,
    inner: MutexGuard<'a, Memory>,
}
#[cfg(test)]
impl std::ops::Deref for MemoryGuard<'_> {
    type Target = Memory;
    fn deref(&self) -> &Memory {
        &self.inner
    }
}
#[cfg(test)]
impl std::ops::DerefMut for MemoryGuard<'_> {
    fn deref_mut(&mut self) -> &mut Memory {
        &mut self.inner
    }
}

#[derive(Clone)]
pub struct BftPrivateRetentionMemoryBackendV1 {
    inner: Arc<Mutex<Memory>>,
    trusted: BftPrivateRetentionAttestorV1,
    clock_origin: Instant,
    clock_floor: i64,
}

pub(super) struct Memory {
    material_epoch: u64,
    validated_inputs: ValidatedMemoryInputs,
    #[cfg(test)]
    hash_probe: Arc<hash_probe::Counts>,
    _preparation: Option<Arc<FreshPreparation>>,
    creation: Option<BftPrivateRetentionCreationMarkerV1>,
    signer: BftSignerBackendSnapshotV3,
    /// Exact original operation snapshot, distinct from the current lease/state.
    /// A subsequent retention operation is permitted only after its predecessor
    /// is read back. Older receipts/maps belong to the immutable archive chain.
    original: Option<Arc<BftPrivateRetentionSnapshotV1>>,
    original_observed: bool,
    operations: std::collections::BTreeMap<String, Arc<BftPrivateRetentionSnapshotV1>>,
    history: super::private_retention_history::BftPrivateRetentionHistoryV1,
    seals: std::collections::BTreeMap<
        String,
        Arc<super::private_retention_seal::BftPrivateRetentionSealV1>,
    >,
    sealed_inputs: BftPrivateRetentionInputInventoryV1,
}

impl BftPrivateRetentionMemoryBackendV1 {
    /// Restores an uncovered software snapshot. This path never creates source
    /// provenance, even when public history/journal happen to be empty.
    pub fn restore_legacy(
        binding: &BftSignerBindingV3,
        snapshot: BftSignerBackendSnapshotV3,
        trusted: BftPrivateRetentionAttestorV1,
    ) -> Result<Self, BftSignerErrorV3> {
        trusted.digest()?;
        snapshot
            .state
            .validate(binding, &trusted.provider_public_key)?;
        if snapshot.state.private_retention.is_some()
            || snapshot.state.binding_digest != trusted.source.binding_digest
            || trusted.source.handoff_digest != binding.handoff_record_id
            || trusted.source.material_source_digest != binding.material_identity
            || !snapshot.lease_token_digest.is_empty()
        {
            return Err(invalid(
                "private retention memory provisioning identity/state",
            ));
        }
        super::archive_transfer::validate_successor_archive_budget(
            &snapshot.state,
            snapshot.successor_archive_transfer.as_ref(),
        )?;
        let validated_inputs = ValidatedMemoryInputs::empty(&trusted, 0)?;
        Ok(Self {
            clock_floor: snapshot.protected_time_floor_ms,
            clock_origin: Instant::now(),
            trusted,
            inner: Arc::new(Mutex::new(Memory {
                _preparation: None,
                material_epoch: 0,
                validated_inputs,
                creation: None,
                signer: snapshot,
                original: None,
                original_observed: true,
                operations: Default::default(),
                history: Default::default(),
                seals: Default::default(),
                sealed_inputs: Default::default(),
                #[cfg(test)]
                hash_probe: Default::default(),
            })),
        })
    }

    /// Consumes the fresh single-use namespace, storing its original marker and
    /// H State together before returning a backend to any managed producer.
    pub fn create_fresh(
        namespace: BftPrivateRetentionFreshNamespaceV1,
        trusted: BftPrivateRetentionAttestorV1,
        transfer: Option<super::archive_transfer::BftSuccessorArchiveTransferLedgerV1>,
        floor: i64,
    ) -> Result<Self, BftSignerErrorV3> {
        trusted.digest()?;
        if floor < 0
            || namespace.marker.digest()? != trusted.creation_marker_digest
            || namespace.marker.source != trusted.source
            || namespace.marker.initial_height != trusted.coverage_start_height
        {
            return Err(invalid(
                "private retention fresh namespace external capability mismatch",
            ));
        }
        let preparation = namespace.preparation.clone();
        let mut record = preparation
            .state
            .lock()
            .map_err(|_| invalid("private retention source preparation poisoned"))?;
        let role_digest = trusted.digest()?;
        if record.backend_created {
            if record.role_digest.as_ref() != Some(&role_digest)
                || record.transfer != transfer
                || record.clock.is_none_or(|(_, original)| original != floor)
            {
                return Err(invalid(
                    "private retention original namespace provisioning changed",
                ));
            }
            let inner = record.backend.upgrade().ok_or_else(|| {
                invalid("private retention original namespace unavailable; cannot recreate")
            })?;
            let (clock_origin, clock_floor) = record
                .clock
                .ok_or_else(|| invalid("private retention original namespace clock unavailable"))?;
            return Ok(Self {
                inner,
                trusted,
                clock_origin,
                clock_floor,
            });
        }
        let snapshot = BftSignerBackendSnapshotV3 {
            state: namespace.initial_state,
            successor_archive_transfer: transfer,
            lease_generation: 1,
            lease_token_digest: String::new(),
            protected_time_floor_ms: floor,
        };
        super::archive_transfer::validate_successor_archive_budget(
            &snapshot.state,
            snapshot.successor_archive_transfer.as_ref(),
        )?;
        let clock_origin = Instant::now();
        let validated_inputs = ValidatedMemoryInputs::empty(&trusted, 0)?;
        let inner = Arc::new(Mutex::new(Memory {
            material_epoch: 0,
            validated_inputs,
            _preparation: Some(preparation.clone()),
            creation: Some(namespace.marker),
            signer: snapshot,
            original: None,
            original_observed: true,
            operations: Default::default(),
            history: Default::default(),
            seals: Default::default(),
            sealed_inputs: Default::default(),
            #[cfg(test)]
            hash_probe: Default::default(),
        }));
        record.backend_created = true;
        record.backend = Arc::downgrade(&inner);
        record.role_digest = Some(role_digest);
        record.clock = Some((clock_origin, floor));
        record.transfer = transfer;
        drop(record);
        Ok(Self {
            inner,
            trusted,
            clock_origin,
            clock_floor: floor,
        })
    }

    fn lock(&self) -> Result<MemoryGuard<'_>, BftSignerErrorV3> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| invalid("private retention memory poisoned"))?;
        #[cfg(test)]
        {
            let probe = hash_probe::Scope::enter(inner.hash_probe.clone());
            Ok(MemoryGuard {
                _probe: probe,
                inner,
            })
        }
        #[cfg(not(test))]
        {
            Ok(inner)
        }
    }

    fn lease(
        snapshot: &BftSignerBackendSnapshotV3,
        generation: u64,
        token: &str,
    ) -> Result<(), BftSignerErrorV3> {
        if token.is_empty()
            || snapshot.lease_generation != generation
            || snapshot.lease_token_digest != token
        {
            return Err(invalid("private retention memory lease fenced"));
        }
        Ok(())
    }

    fn seal_frozen(memory: &Memory) -> bool {
        memory.original.as_ref().is_some_and(|original| {
            original.signer.state == memory.signer.state
                && original
                    .retention
                    .map
                    .previous_seal_receipt_digest
                    .as_ref()
                    .and_then(|id| memory.seals.get(id))
                    .is_some_and(|seal| {
                        seal.body.original.receipt.body.head.sequence.checked_add(1)
                            == Some(original.retention.receipt.body.head.sequence)
                    })
        })
    }
    fn validate_sealed_next(
        memory: &Memory,
        next: &BftSignerStateV3,
    ) -> Result<(), BftSignerErrorV3> {
        if !Self::seal_frozen(memory) {
            return Ok(());
        }
        let reservation = next
            .successor_window
            .as_ref()
            .and_then(|window| window.reservation.as_ref())
            .ok_or_else(|| invalid("sealed source permits only original successor reservation"))?;
        let mut unchanged = next.clone();
        unchanged.successor_window = memory.signer.state.successor_window.clone();
        unchanged.revision = memory.signer.state.revision;
        unchanged.state_digest = memory.signer.state.state_digest.clone();
        if unchanged != memory.signer.state
            || reservation.body.expected_revision != memory.signer.state.revision
            || reservation.body.expected_state_digest != memory.signer.state.state_digest
        {
            return Err(invalid(
                "sealed source successor did not consume exact original post-seal State",
            ));
        }
        Ok(())
    }

    fn validate_current(&self, memory: &Memory) -> Result<(), BftSignerErrorV3> {
        if memory.signer.state.private_retention.as_ref()
            != memory
                .original
                .as_ref()
                .map(|original| &original.retention.receipt.body.head)
        {
            return Err(invalid("private retention memory State/ledger mismatch"));
        }
        if memory.signer.state.binding_digest != self.trusted.source.binding_digest {
            return Err(invalid("private retention memory current binding differs"));
        }
        if let Some(head) = &memory.signer.state.private_retention
            && (head.source != self.trusted.source
                || head.capability_digest != self.trusted.digest()?)
        {
            return Err(invalid("private retention memory current role differs"));
        }
        memory
            .validated_inputs
            .verify_current(memory, &self.trusted)?;
        super::archive_transfer::validate_successor_archive_budget(
            &memory.signer.state,
            memory.signer.successor_archive_transfer.as_ref(),
        )?;
        Ok(())
    }
}

impl BftSignerBackendV3 for BftPrivateRetentionMemoryBackendV1 {
    fn read_private_retention_creation(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<ValidatedBftPrivateRetentionCreationV1, BftSignerErrorV3> {
        let memory = self.lock()?;
        Self::lease(&memory.signer, generation, token)?;
        let marker = memory
            .creation
            .as_ref()
            .ok_or_else(|| invalid("legacy source is uncovered; original creation unavailable"))?;
        ValidatedBftPrivateRetentionCreationV1::from_fresh_backend(
            marker,
            &self.trusted,
            &memory.signer,
        )
    }
    fn load_private_retention_input(
        &mut self,
        binding: &str,
        reference: &super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<
        std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>,
        BftSignerErrorV3,
    > {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        if memory.signer.state.binding_digest != binding
            || reference.source.binding_digest != binding
        {
            return Err(invalid("atomic input source binding mismatch"));
        }
        let input = memory
            .original
            .as_ref()
            .and_then(|original| original.inputs.get(&reference.admission_digest))
            .or_else(|| memory.sealed_inputs.get(&reference.admission_digest))
            .ok_or_else(|| invalid("original atomic input unavailable"))?;
        memory.validated_inputs.verify_reference(
            reference,
            input,
            &self.trusted,
            memory.material_epoch,
        )?;
        Ok(input.clone())
    }
    fn load_private_retention_current(
        &mut self,
        binding: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        if memory.signer.state.binding_digest != binding {
            return Err(invalid("private retention memory binding"));
        }
        Ok(BftPrivateRetentionCurrentV1 {
            signer: memory.signer.clone(),
            inputs: memory
                .original
                .as_ref()
                .map(|original| original.inputs.clone())
                .unwrap_or_default(),
            retention: memory
                .original
                .as_ref()
                .map(|original| original.retention.clone()),
        })
    }

    fn read_private_retention_current(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        Self::lease(&memory.signer, generation, token)?;
        Ok(BftPrivateRetentionCurrentV1 {
            signer: memory.signer.clone(),
            inputs: memory
                .original
                .as_ref()
                .map(|original| original.inputs.clone())
                .unwrap_or_default(),
            retention: memory
                .original
                .as_ref()
                .map(|original| original.retention.clone()),
        })
    }

    fn load_private_retention_sealed_current(
        &mut self,
        binding: &str,
    ) -> Result<
        super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1,
        BftSignerErrorV3,
    > {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        if memory.signer.state.binding_digest != binding {
            return Err(invalid("private retention sealed load binding mismatch"));
        }
        Ok(
            super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1 {
                current: BftPrivateRetentionCurrentV1 {
                    signer: memory.signer.clone(),
                    retention: memory.original.as_ref().map(|op| op.retention.clone()),
                    inputs: memory
                        .original
                        .as_ref()
                        .map(|op| op.inputs.clone())
                        .unwrap_or_default(),
                },
                seals: memory.seals.clone(),
                sealed_inputs: memory.sealed_inputs.clone(),
                history: memory.history.clone(),
            },
        )
    }

    fn read_private_retention_sealed_current(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<
        super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1,
        BftSignerErrorV3,
    > {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        Self::lease(&memory.signer, generation, token)?;
        Ok(
            super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1 {
                current: BftPrivateRetentionCurrentV1 {
                    signer: memory.signer.clone(),
                    retention: memory.original.as_ref().map(|op| op.retention.clone()),
                    inputs: memory
                        .original
                        .as_ref()
                        .map(|op| op.inputs.clone())
                        .unwrap_or_default(),
                },
                seals: memory.seals.clone(),
                sealed_inputs: memory.sealed_inputs.clone(),
                history: memory.history.clone(),
            },
        )
    }

    fn compare_and_swap_private_retention_seal(
        &mut self,
        request: &super::private_retention_seal::BftPrivateRetentionSealCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        request.validate_envelope(&self.trusted)?;
        request.validate_exact_inventory()?;
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        Self::lease(
            &memory.signer,
            request.before.signer.lease_generation,
            &request.before.signer.lease_token_digest,
        )?;
        if memory.signer != request.before.signer
            || memory.original.as_ref().map(|op| &op.retention) != request.before.retention.as_ref()
            || memory.original.as_ref().map(|op| &op.inputs) != Some(&request.before.inputs)
            || memory.seals != request.previous_seals
            || memory.sealed_inputs != request.previous_inputs
            || !memory.original_observed
        {
            return Err(invalid("retention seal actual atomic before image differs"));
        }
        // The software backend preserves the very same immutable allocations.
        for (id, old) in &memory.sealed_inputs {
            if request
                .retained_inputs
                .get(id)
                .is_none_or(|new| !Arc::ptr_eq(old, new))
            {
                return Err(invalid("retention seal copied actual historical input"));
            }
        }
        if let Some(old) = &memory.original {
            for (id, input) in &old.inputs {
                if request
                    .retained_inputs
                    .get(id)
                    .is_none_or(|new| !Arc::ptr_eq(input, new))
                {
                    return Err(invalid("retention seal copied actual current input"));
                }
            }
        }
        for (id, old) in &memory.seals {
            if request
                .retained_seals
                .get(id)
                .is_none_or(|new| !Arc::ptr_eq(old, new))
            {
                return Err(invalid("retention seal copied actual historical seal"));
            }
        }
        let original = Arc::new(request.after.clone());
        let receipt_id = super::crypto::digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &original.retention.receipt,
        )?;
        let operation_id = original.retention.receipt.body.operation_digest.clone();
        if memory.operations.contains_key(&operation_id) || memory.history.contains_key(&receipt_id)
        {
            return Err(invalid("retention seal original operation identity reused"));
        }
        let (material_epoch, validated_inputs) = memory.validated_inputs.prepare_publication(
            &self.trusted,
            memory.material_epoch,
            &original,
            &request.retained_inputs,
        )?;
        memory.operations.insert(operation_id, original.clone());
        memory.history.insert(receipt_id, original.clone());
        memory.seals = request.retained_seals.clone();
        memory.sealed_inputs = request.retained_inputs.clone();
        memory.signer = request.after.signer.clone();
        memory.original = Some(original);
        memory.material_epoch = material_epoch;
        memory.validated_inputs = validated_inputs;
        memory.original_observed = true;
        Ok(request.after.clone())
    }

    fn load_provisioned(
        &mut self,
        binding: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        if memory.signer.state.binding_digest != binding {
            return Err(invalid("private retention memory binding"));
        }
        Ok(memory.signer.clone())
    }

    fn acquire_lease(
        &mut self,
        binding: &str,
        expected: u64,
        token: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        if memory.signer.state.binding_digest != binding || token.is_empty() {
            return Err(invalid("private retention memory acquire identity"));
        }
        if memory.signer.lease_generation != expected
            || !memory.signer.lease_token_digest.is_empty()
        {
            return Ok(BftSignerAcquireOutcomeV3::Conflict(memory.signer.clone()));
        }
        memory.signer.lease_generation = expected
            .checked_add(1)
            .ok_or_else(|| invalid("private retention memory generation overflow"))?;
        memory.signer.lease_token_digest = token.to_string();
        memory.original_observed = memory.original.is_none();
        Ok(BftSignerAcquireOutcomeV3::Acquired(memory.signer.clone()))
    }

    fn release_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        if memory.signer.lease_generation == generation
            && memory.signer.lease_token_digest == token
            && !token.is_empty()
        {
            memory.signer.lease_generation = generation
                .checked_add(1)
                .ok_or_else(|| invalid("private retention memory generation overflow"))?;
            memory.signer.lease_token_digest.clear();
        }
        Ok(memory.signer.clone())
    }

    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let memory = self.lock()?;
        self.validate_current(&memory)?;
        Self::lease(&memory.signer, generation, token)?;
        Ok(memory.signer.clone())
    }

    fn protected_time_ms(&mut self, generation: u64, token: &str) -> Result<i64, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        Self::lease(&memory.signer, generation, token)?;
        let elapsed = i64::try_from(self.clock_origin.elapsed().as_millis())
            .map_err(|_| invalid("private retention memory clock overflow"))?;
        let now = self
            .clock_floor
            .checked_add(elapsed)
            .ok_or_else(|| invalid("private retention memory clock overflow"))?;
        memory.signer.protected_time_floor_ms = memory.signer.protected_time_floor_ms.max(now);
        Ok(memory.signer.protected_time_floor_ms)
    }

    fn reserve_successor_archive_transfer(
        &mut self,
        generation: u64,
        token: &str,
        revision: u64,
        digest: &str,
        expected: u64,
        amount: u64,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        let next = super::archive_transfer::reserve_successor_archive_transfer_v1(
            &memory.signer,
            generation,
            token,
            revision,
            digest,
            expected,
            amount,
        )?;
        memory.signer.successor_archive_transfer = Some(next);
        Ok(memory.signer.clone())
    }

    fn compare_and_swap(
        &mut self,
        generation: u64,
        token: &str,
        revision: u64,
        digest: &str,
        minimum_time: Option<i64>,
        next: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        Self::lease(&memory.signer, generation, token)?;
        next.validate_digest()?;
        Self::validate_sealed_next(&memory, &next)?;
        if memory.signer.state.revision != revision
            || memory.signer.state.state_digest != digest
            || next.revision
                != revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("private retention memory revision overflow"))?
            || next.private_retention != memory.signer.state.private_retention
            || next.binding_digest != memory.signer.state.binding_digest
            || (memory.creation.is_some() && memory.signer.state.private_retention.is_none())
            || !memory.original_observed
        {
            return Err(invalid("private retention memory generic CAS mismatch"));
        }
        super::archive_transfer::validate_successor_archive_budget(
            &next,
            memory.signer.successor_archive_transfer.as_ref(),
        )?;
        memory.signer.state = next;
        if let Some(time) = minimum_time {
            memory.signer.protected_time_floor_ms = memory.signer.protected_time_floor_ms.max(time);
        }
        Ok(memory.signer.clone())
    }

    fn compare_and_swap_private_retention(
        &mut self,
        request: &BftPrivateRetentionCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        let anchor = memory
            .original
            .as_ref()
            .and_then(|op| op.retention.map.previous_seal_receipt_digest.as_ref())
            .map(|id| {
                memory
                    .seals
                    .get(id)
                    .ok_or_else(|| invalid("retention actual original anchor missing"))
            })
            .transpose()?
            .map(|seal| seal.anchor())
            .transpose()?;
        request.validate_envelope_with_anchor(&self.trusted, anchor.as_ref())?;
        if Self::seal_frozen(&memory) {
            return Err(invalid(
                "sealed source blocks ordinary retention before original successor reservation",
            ));
        }
        Self::lease(
            &memory.signer,
            request.before.lease_generation,
            &request.before.lease_token_digest,
        )?;
        if memory.signer != request.before
            || memory.original.as_ref().map(|original| &original.retention)
                != request.expected_retention.as_ref()
            || !memory.original_observed
        {
            return Err(invalid("private retention memory atomic CAS mismatch"));
        }
        if let Some(original) = &memory.original {
            for (digest, input) in &original.inputs {
                if request.after.inputs.get(digest) != Some(input) {
                    return Err(invalid("private retention changed original atomic input"));
                }
            }
        }
        if request.expected_retention.is_none() {
            let marker = memory
                .creation
                .as_ref()
                .ok_or_else(|| invalid("legacy source cannot synthesize first retention head"))?;
            ValidatedBftPrivateRetentionCreationV1::from_fresh_backend(
                marker,
                &self.trusted,
                &memory.signer,
            )?;
        }
        let original = Arc::new(request.after.clone());
        let receipt_id = super::crypto::digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &original.retention.receipt,
        )?;
        let operation_id = original.retention.receipt.body.operation_digest.clone();
        if memory.operations.contains_key(&operation_id) || memory.history.contains_key(&receipt_id)
        {
            return Err(invalid("retention original operation identity reused"));
        }
        let (material_epoch, validated_inputs) = memory.validated_inputs.prepare_publication(
            &self.trusted,
            memory.material_epoch,
            &original,
            &memory.sealed_inputs,
        )?;
        memory.signer = request.after.signer.clone();
        memory.operations.insert(operation_id, original.clone());
        memory.history.insert(receipt_id, original.clone());
        memory.original = Some(original);
        memory.material_epoch = material_epoch;
        memory.validated_inputs = validated_inputs;
        // The returned atomic snapshot is the exact original readback. A caller
        // which loses this result must retrieve this original operation first.
        memory.original_observed = true;
        Ok(request.after.clone())
    }

    fn read_private_retention_operation(
        &mut self,
        generation: u64,
        token: &str,
        operation: &str,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        let mut memory = self.lock()?;
        self.validate_current(&memory)?;
        Self::lease(&memory.signer, generation, token)?;
        let original = memory
            .operations
            .get(operation)
            .ok_or_else(|| invalid("private retention original operation unavailable"))?
            .clone();
        if memory
            .original
            .as_ref()
            .is_some_and(|latest| latest.retention.receipt.body.operation_digest == operation)
        {
            memory.original_observed = true;
        }
        Ok((*original).clone())
    }
}

#[cfg(test)]
#[path = "private_retention_memory_tests.rs"]
mod tests;
