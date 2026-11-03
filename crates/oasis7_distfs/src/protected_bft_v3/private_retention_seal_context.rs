//! Explicit historical operation provenance for an independent seal verifier.
//! Original State/history/digests are never truncated into a synthetic current.
use super::backend::BftSignerBackendSnapshotV3;
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::private_retention::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionEntryV1, BftPrivateRetentionKeyV1,
    BftPrivateRetentionLifecycleV1,
};
use super::private_retention_backend::BftPrivateRetentionSnapshotV1;
use super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1;

/// Same-backend complete original history plus one current active lease sample.
/// This is not a migration grant, private ReadGrant, or a current input context.
pub struct BftPrivateRetentionSealVerificationContextV1<'a> {
    current: &'a BftPrivateRetentionSealedCurrentV1,
    trusted: &'a BftPrivateRetentionAttestorV1,
    binding: &'a BftSignerBindingV3,
}
/// The exact first Reserve and first Ready operation of one immutable input.
/// Replay uses `reservation.signer.state` verbatim; the previous receipt/input
/// inventory, not today's shared material, establishes its original budget.
pub struct BftPrivateRetentionHistoricalInputContextV1<'a> {
    live: &'a BftSignerBackendSnapshotV3,
    current: &'a BftPrivateRetentionSealedCurrentV1,
    trusted: &'a BftPrivateRetentionAttestorV1,
    reservation: &'a BftPrivateRetentionSnapshotV1,
    predecessor: Option<&'a BftPrivateRetentionSnapshotV1>,
    ready: &'a BftPrivateRetentionSnapshotV1,
    sequence_limit: Option<u64>,
}
impl<'a> BftPrivateRetentionSealVerificationContextV1<'a> {
    pub(crate) fn from_backend_sealed_current(
        binding: &'a BftSignerBindingV3,
        trusted: &'a BftPrivateRetentionAttestorV1,
        current: &'a BftPrivateRetentionSealedCurrentV1,
        expected_lease: &BftSignerBackendSnapshotV3,
    ) -> Result<Self, BftSignerErrorV3> {
        Self::from_backend_sealed_current_with_verified_state(
            binding,
            trusted,
            current,
            expected_lease,
            None,
        )
    }
    pub(crate) fn from_backend_sealed_current_with_verified_state(
        binding: &'a BftSignerBindingV3,
        trusted: &'a BftPrivateRetentionAttestorV1,
        current: &'a BftPrivateRetentionSealedCurrentV1,
        expected_lease: &BftSignerBackendSnapshotV3,
        verified: Option<&super::state::VerifiedBftSignerStateV3>,
    ) -> Result<Self, BftSignerErrorV3> {
        if &current.current.signer != expected_lease
            || expected_lease.lease_token_digest.is_empty()
            || expected_lease.lease_generation == 0
        {
            return Err(invalid("seal context current lease observation differs"));
        }
        super::private_retention_verification_context::validate_context_state(
            binding,
            trusted,
            expected_lease,
            verified,
        )?;
        current.validate_inventory_with_verified_state(binding, trusted, verified)?;
        Ok(Self {
            current,
            trusted,
            binding,
        })
    }
    pub fn binding(&self) -> &BftSignerBindingV3 {
        self.binding
    }
    pub fn attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        self.trusted
    }
    pub fn signer(&self) -> &BftSignerBackendSnapshotV3 {
        &self.current.current.signer
    }
    /// The protected owner may independently verify its own exact retained
    /// original without recursively reading its network service. This borrows
    /// one backend-owned allocation and creates no external ReadGrant.
    pub fn owned_original(
        &self,
        reference: &super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<
        &super::private_retention_input::BftPrivateRetentionAdmissionInputV1,
        BftSignerErrorV3,
    > {
        reference.validate()?;
        if reference.source != self.trusted.source {
            return Err(invalid("owned original source differs from fixed role"));
        }
        let entry = self
            .current
            .original_entry(&reference.key)?
            .ok_or_else(|| invalid("owned original protected key missing"))?;
        if entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready
            || super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1::from_entry(
                &reference.source,
                entry,
            )? != *reference
        {
            return Err(invalid("owned original key/ref/fact differs"));
        }
        let input = self
            .current
            .original_input(&reference.admission_digest)
            .ok_or_else(|| invalid("owned original protected input missing"))?;
        reference.validate_input(input)?;
        Ok(input.as_ref())
    }
    pub fn validate_cut(
        &self,
        cut: &super::successor_window::BftSuccessorWindowCutV1,
    ) -> Result<(), BftSignerErrorV3> {
        let old = self
            .current
            .current
            .retention
            .as_ref()
            .ok_or_else(|| invalid("seal original map missing"))?;
        super::private_retention_seal::BftPrivateRetentionSealBodyV1::from_original(
            old.clone(),
            cut.clone(),
        )?
        .validate_applied_cut(self.trusted, &self.current.current.signer.state)
    }
    pub fn current_entries(&self) -> &[BftPrivateRetentionEntryV1] {
        self.current
            .current
            .retention
            .as_ref()
            .map_or(&[], |ledger| ledger.map.entries.as_slice())
    }
    fn original_operations(
        &self,
        key: &BftPrivateRetentionKeyV1,
    ) -> Result<
        (
            &'a BftPrivateRetentionSnapshotV1,
            Option<&'a BftPrivateRetentionSnapshotV1>,
            Option<&'a BftPrivateRetentionSnapshotV1>,
        ),
        BftSignerErrorV3,
    > {
        let mut first = None;
        let mut ready = None;
        let mut original = None;
        // Full history has already proved unique contiguous receipt sequences.
        for operation in self.current.history.values() {
            if let Ok(index) = operation
                .retention
                .map
                .entries
                .binary_search_by(|entry| entry.key.cmp(key))
            {
                let entry = &operation.retention.map.entries[index];
                if original.is_some_and(|fact| fact != &entry.fact) {
                    return Err(invalid("seal history original input fact changed"));
                }
                original = Some(&entry.fact);
                if first.is_none_or(|old: &BftPrivateRetentionSnapshotV1| {
                    old.retention.receipt.body.head.sequence
                        > operation.retention.receipt.body.head.sequence
                }) {
                    first = Some(operation.as_ref());
                }
                if entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready
                    && ready.is_none_or(|old: &BftPrivateRetentionSnapshotV1| {
                        old.retention.receipt.body.head.sequence
                            > operation.retention.receipt.body.head.sequence
                    })
                {
                    ready = Some(operation.as_ref());
                }
            }
        }
        let reservation = first.ok_or_else(|| invalid("seal historical reservation missing"))?;
        let entry = &reservation.retention.map.entries[reservation
            .retention
            .map
            .entries
            .binary_search_by(|entry| entry.key.cmp(key))
            .map_err(|_| invalid("seal first reservation key missing"))?];
        if entry.lifecycle != BftPrivateRetentionLifecycleV1::ReservedPending {
            return Err(invalid(
                "seal history input lacks original Reserve transition",
            ));
        }
        let predecessor = reservation
            .retention
            .receipt
            .body
            .head
            .previous_receipt_digest
            .as_ref()
            .map(|id| {
                self.current
                    .history
                    .get(id)
                    .map(|op| op.as_ref())
                    .ok_or_else(|| invalid("seal reservation original predecessor missing"))
            })
            .transpose()?;
        if predecessor.is_some_and(|op| {
            op.retention
                .map
                .entries
                .binary_search_by(|entry| entry.key.cmp(key))
                .is_ok()
        }) {
            return Err(invalid("seal history first reservation was not new"));
        }
        Ok((reservation, predecessor, ready))
    }
    pub fn original_reservation(
        &self,
        key: &BftPrivateRetentionKeyV1,
    ) -> Result<BftPrivateRetentionReservationInputContextV1<'a>, BftSignerErrorV3> {
        let (reservation, predecessor, _) = self.original_operations(key)?;
        Ok(BftPrivateRetentionReservationInputContextV1 {
            live: &self.current.current.signer,
            current: self.current,
            trusted: self.trusted,
            reservation,
            predecessor,
        })
    }
    pub fn original_ready(
        &self,
        key: &BftPrivateRetentionKeyV1,
    ) -> Result<BftPrivateRetentionHistoricalInputContextV1<'a>, BftSignerErrorV3> {
        let (reservation, predecessor, ready) = self.original_operations(key)?;
        let ready = ready.ok_or_else(|| invalid("seal historical Ready operation missing"))?;
        if ready.retention.receipt.body.head.sequence
            <= reservation.retention.receipt.body.head.sequence
        {
            return Err(invalid("seal history Ready precedes original Reserve"));
        }
        Ok(BftPrivateRetentionHistoricalInputContextV1 {
            live: &self.current.current.signer,
            current: self.current,
            trusted: self.trusted,
            reservation,
            predecessor,
            ready,
            sequence_limit: None,
        })
    }
}
impl<'a> BftPrivateRetentionHistoricalInputContextV1<'a> {
    pub(crate) fn restrict_to_sequence(mut self, sequence: u64) -> Result<Self, BftSignerErrorV3> {
        if self.ready.retention.receipt.body.head.sequence > sequence {
            return Err(invalid("historical original Ready exceeds target sequence"));
        }
        self.sequence_limit = Some(sequence);
        Ok(self)
    }

    /// Only originals already present at first Reserve may reduce its price.
    /// Iteration repeats shared references across receipts but copies no payload;
    /// the consumer forms a digest set before calculating any byte discount.
    pub fn deduction_inputs(
        &self,
    ) -> impl Iterator<
        Item = &std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>,
    > {
        let sequence = self.reservation.retention.receipt.body.head.sequence;
        self.current
            .history
            .values()
            .filter(move |operation| operation.retention.receipt.body.head.sequence <= sequence)
            .flat_map(|operation| operation.inputs.values())
    }
    /// Current local files may be newer; this validates them without making
    /// their later shared references eligible for the original price.
    pub fn original_entry(
        &self,
        key: &BftPrivateRetentionKeyV1,
    ) -> Result<Option<&BftPrivateRetentionEntryV1>, BftSignerErrorV3> {
        if let Some(limit) = self.sequence_limit {
            return Ok(self
                .current
                .history
                .values()
                .filter(|op| op.retention.receipt.body.head.sequence <= limit)
                .filter_map(|op| {
                    op.retention
                        .map
                        .entries
                        .iter()
                        .find(|entry| &entry.key == key)
                        .map(|entry| (op.retention.receipt.body.head.sequence, entry))
                })
                .max_by_key(|(sequence, _)| *sequence)
                .map(|(_, entry)| entry));
        }
        self.current.original_entry(key)
    }
    pub fn original_input(
        &self,
        digest: &str,
    ) -> Option<&std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>>
    {
        if let Some(limit) = self.sequence_limit {
            return self
                .current
                .history
                .values()
                .filter(|op| op.retention.receipt.body.head.sequence <= limit)
                .find_map(|op| op.inputs.get(digest));
        }
        self.current.original_input(digest)
    }
    pub fn current_signer(&self) -> &BftSignerBackendSnapshotV3 {
        self.live
    }
    pub fn attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        self.trusted
    }
    pub fn reservation(&self) -> &'a BftPrivateRetentionSnapshotV1 {
        self.reservation
    }
    pub fn predecessor(&self) -> Option<&'a BftPrivateRetentionSnapshotV1> {
        self.predecessor
    }
    pub fn ready(&self) -> &'a BftPrivateRetentionSnapshotV1 {
        self.ready
    }
}

/// Exact first Reserve price provenance; deliberately provides no Ready proof.
pub struct BftPrivateRetentionReservationInputContextV1<'a> {
    live: &'a BftSignerBackendSnapshotV3,
    current: &'a BftPrivateRetentionSealedCurrentV1,
    trusted: &'a BftPrivateRetentionAttestorV1,
    reservation: &'a BftPrivateRetentionSnapshotV1,
    predecessor: Option<&'a BftPrivateRetentionSnapshotV1>,
}
impl<'a> BftPrivateRetentionReservationInputContextV1<'a> {
    pub fn reservation(&self) -> &'a BftPrivateRetentionSnapshotV1 {
        self.reservation
    }
    pub fn predecessor(&self) -> Option<&'a BftPrivateRetentionSnapshotV1> {
        self.predecessor
    }
    pub fn current_signer(&self) -> &BftSignerBackendSnapshotV3 {
        self.live
    }
    pub fn attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        self.trusted
    }
    pub fn deduction_inputs(
        &self,
    ) -> impl Iterator<
        Item = &std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>,
    > {
        let sequence = self.reservation.retention.receipt.body.head.sequence;
        self.current
            .history
            .values()
            .filter(move |op| op.retention.receipt.body.head.sequence <= sequence)
            .flat_map(|op| op.inputs.values())
    }
    pub fn original_entry(
        &self,
        key: &BftPrivateRetentionKeyV1,
    ) -> Result<Option<&BftPrivateRetentionEntryV1>, BftSignerErrorV3> {
        self.current.original_entry(key)
    }
    pub fn original_input(
        &self,
        digest: &str,
    ) -> Option<&std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>>
    {
        self.current.original_input(digest)
    }
}
