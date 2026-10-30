//! Storage accounting only. Neither an accounting snapshot nor a retained
//! promise can authorize a World, a signing schedule, or a historical cut.
use super::*;
#[derive(Clone, PartialEq, Eq)]
pub(in super::super) struct PublicationRecordBindingV1 {
    pub(in super::super) reservation_ref: String,
    pub(in super::super) header_id: String,
    pub(in super::super) actions_digest: String,
}
pub(in super::super) struct PublicationRecordIdentityV1<'a> {
    pub(in super::super) binding: &'a PublicationRecordBindingV1,
    pub(in super::super) origin: &'a str,
    pub(in super::super) height: u64,
    pub(in super::super) snapshot: &'a str,
    pub(in super::super) journal: &'a str,
    pub(in super::super) snapshot_bytes: usize,
    pub(in super::super) journal_bytes: usize,
}
pub(in super::super) trait RetainedPublicationClaimV1 {
    fn held_bytes(&self) -> usize;
    fn validate_record(&self, actual: PublicationRecordIdentityV1<'_>) -> Result<(), String>;
}
/// Budget-only original private Data evidence, not execution authority.
#[derive(Clone)]
pub(in super::super) struct PublicationPromiseV1 {
    pub(in super::super) binding: PublicationRecordBindingV1,
    pub(in super::super) origin: String,
    pub(in super::super) height: u64,
    pub(in super::super) snapshot: String,
    pub(in super::super) journal: String,
    pub(in super::super) snapshot_bytes: usize,
    pub(in super::super) journal_bytes: usize,
    pub(in super::super) promised: usize,
}
pub(in super::super) type PublicationPromiseInventoryV1 = BTreeMap<String, PublicationPromiseV1>;
impl RetainedPublicationClaimV1 for PublicationPromiseV1 {
    fn held_bytes(&self) -> usize {
        self.promised
    }
    fn validate_record(&self, actual: PublicationRecordIdentityV1<'_>) -> Result<(), String> {
        let PublicationRecordIdentityV1 {
            binding,
            origin,
            height,
            snapshot,
            journal,
            snapshot_bytes,
            journal_bytes,
        } = actual;
        if self.binding != *binding
            || self.origin != origin
            || self.height != height
            || self.snapshot != snapshot
            || self.journal != journal
            || self.snapshot_bytes != snapshot_bytes
            || self.journal_bytes != journal_bytes
        {
            return Err("publication promise differs from original private Data".into());
        }
        Ok(())
    }
}
#[derive(Clone)]
struct Consumption {
    binding: PublicationRecordBindingV1,
    height: u64,
    snapshot: String,
    journal: String,
    bytes: usize,
}
#[derive(Clone)]
pub(in super::super) struct LedgerPublicationAccountingV1 {
    origin: String,
    records: Vec<String>,
    actual: Capacity,
    proof_bytes: usize,
    consumed: BTreeMap<String, Consumption>,
}
impl LedgerPublicationAccountingV1 {
    pub(in super::super) fn consumed_for(
        &self,
        origin: &str,
        height: u64,
        binding: &PublicationRecordBindingV1,
        snapshot: &str,
        journal: &str,
    ) -> Result<usize, String> {
        if self.origin != origin {
            return Err("publication accounting owner differs".into());
        }
        let Some(value) = self.consumed.get(&binding.reservation_ref) else {
            return Ok(0);
        };
        if value.binding != *binding
            || value.height != height
            || value.snapshot != snapshot
            || value.journal != journal
        {
            return Err("publication accounting original record differs".into());
        }
        Ok(value.bytes)
    }
    pub(in super::super) fn total_consumed(&self) -> Result<usize, String> {
        self.consumed.values().try_fold(0usize, |sum, entry| {
            sum.checked_add(entry.bytes)
                .ok_or("publication accounting consumption overflow".into())
        })
    }
}
impl<F: Format> GenericLedger<F> {
    #[cfg(test)]
    pub(in super::super) fn publication_usage_for_test(&self) -> (usize, usize, usize) {
        (
            self.capacity.bytes,
            self.external_material_bytes,
            self.external_physical_bytes,
        )
    }
    /// Rebuild logical charges using the exact same object/height dedup rules.
    /// A fixed pre-pin QC pays only its existing record/metadata/proof; missing
    /// objects remain funded by its original future promise until recovered.
    pub(in super::super) fn publication_accounting(
        &self,
        store: &LocalCasStore,
    ) -> Result<LedgerPublicationAccountingV1, String> {
        self.verify_carriers()?;
        let mut actual = Capacity::default();
        actual.charge(self.origin_bytes)?;
        let mut consumed = BTreeMap::new();
        let mut records = Vec::new();
        let mut prefix = Vec::new();
        let mut proof_bytes = 0usize;
        let shards = store
            .list_pin_scope_shards(F::SCOPE)
            .map_err(|_| "inspect publication accounting pins")?;
        let mut index_nodes = BTreeSet::new();
        let mut index_bytes = 0usize;
        for slot in self.slots.values() {
            let record = &slot.record;
            let before = actual.bytes;
            actual.record::<F::Origin, F::Record>(record)?;
            if let Some(reference) = record.publication_reservation_ref() {
                let bytes = common_storage::read_cas_bounded(
                    store,
                    reference,
                    MAX_PUBLICATION_METADATA_BYTES,
                )?;
                actual.object(reference, bytes.len())?;
            }
            if !slot.reserved || shards.contains(&self.shard(record.height())?) {
                self.verify_pins(record, store)?;
                for (reference, limit) in [
                    (
                        record.snapshot_ref(),
                        common_storage::MAX_BFT_SNAPSHOT_BYTES,
                    ),
                    (record.journal_ref(), common_storage::MAX_BFT_JOURNAL_BYTES),
                ] {
                    let bytes = common_storage::read_cas_bounded(
                        store,
                        reference,
                        actual.read_bound(reference, limit)?,
                    )?;
                    actual.object(reference, bytes.len())?;
                }
                for reference in record.business_index_refs() {
                    let bytes = common_storage::read_cas_bounded(
                        store,
                        reference,
                        formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    )?;
                    if index_nodes.insert(reference.clone()) {
                        index_bytes = index_bytes
                            .checked_add(bytes.len())
                            .filter(|n| *n <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
                            .ok_or("publication accounting index byte capacity")?;
                        if index_nodes.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
                            return Err("publication accounting index count capacity".into());
                        }
                    }
                    actual.object(reference, bytes.len())?;
                }
            }
            prefix.push(record.clone());
            if let Some(bytes) = F::publication_proof_bytes(&prefix)? {
                actual.charge(bytes.saturating_sub(proof_bytes))?;
                proof_bytes = proof_bytes.max(bytes);
            }
            if let Some(binding) = record.publication_binding()? {
                let amount = actual
                    .bytes
                    .checked_sub(before)
                    .ok_or("publication accounting regressed")?;
                if consumed
                    .insert(
                        binding.reservation_ref.clone(),
                        Consumption {
                            binding,
                            height: record.height(),
                            snapshot: record.snapshot_ref().into(),
                            journal: record.journal_ref().into(),
                            bytes: amount,
                        },
                    )
                    .is_some()
                {
                    return Err("publication promise used by multiple records".into());
                }
            }
            records.push(record.digest()?);
        }
        Ok(LedgerPublicationAccountingV1 {
            origin: self.origin_digest()?,
            records,
            actual,
            proof_bytes,
            consumed,
        })
    }
    pub(in super::super) fn retained_recovery_claim(
        &self,
        record: &F::Record,
        store: &LocalCasStore,
    ) -> Result<Option<PublicationPromiseV1>, String> {
        let Some(binding) = record.publication_binding()? else {
            return Ok(None);
        };
        let Some(mut promise) = self
            .publication_promises
            .get(&binding.reservation_ref)
            .cloned()
        else {
            // A standalone strict verifier has no private Data budget. It may
            // verify existing material, but cannot consume a future promise.
            return Ok(None);
        };
        let accounting = self.publication_accounting(store)?;
        let consumed = accounting.consumed_for(
            record.origin_digest(),
            record.height(),
            &binding,
            record.snapshot_ref(),
            record.journal_ref(),
        )?;
        promise.promised = promise
            .promised
            .checked_sub(consumed)
            .ok_or("publication recovery promise overflow")?;
        Ok(Some(promise))
    }
    /// Caller supplies freshly verified private Data usage, split into physical
    /// bytes and immutable future promises. This grants no execution authority.
    pub(in super::super) fn reserve_private_material(
        &mut self,
        full_usage: usize,
        promises: PublicationPromiseInventoryV1,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        let future_promises = promises.values().try_fold(0usize, |sum, promise| {
            sum.checked_add(promise.promised)
                .ok_or("publication promise inventory overflow")
        })?;
        let physical = full_usage
            .checked_sub(future_promises)
            .ok_or("private usage promise split underflow")?;
        let accounting = self.publication_accounting(store)?;
        // Every consumed ref must be the same original Data promise. Summing
        // all held bytes first would let an unrelated promise fund a foreign QC.
        let mut consumed = 0usize;
        for (reference, used) in &accounting.consumed {
            let promise = promises
                .get(reference)
                .ok_or("Record publication promise absent from original private Data")?;
            if promise.binding != used.binding
                || promise.origin != accounting.origin
                || promise.height != used.height
                || promise.snapshot != used.snapshot
                || promise.journal != used.journal
                || used.bytes > promise.promised
            {
                return Err(
                    "Record publication promise identity or per-ref capacity differs".into(),
                );
            }
            consumed = consumed
                .checked_add(used.bytes)
                .ok_or("publication consumption overflow")?;
        }
        let remaining = future_promises
            .checked_sub(consumed)
            .ok_or("record consumed a promise absent from private Data")?;
        let external = physical
            .checked_add(remaining)
            .ok_or("private publication usage overflow")?;
        if accounting.origin != self.origin_digest()?
            || accounting.records
                != self
                    .slots
                    .values()
                    .map(|slot| slot.record.digest())
                    .collect::<Result<Vec<_>, _>>()?
        {
            return Err("publication accounting changed during readback".into());
        }
        let mut next = accounting.actual;
        next.charge(external)?;
        if next.bytes < self.capacity.bytes {
            return Err("private publication accounting would refund capacity".into());
        }
        self.publication_promises = promises;
        self.capacity = next;
        self.external_material_bytes = external;
        self.external_physical_bytes = physical;
        self.proof_bytes = self.proof_bytes.max(accounting.proof_bytes);
        Ok(())
    }
}
