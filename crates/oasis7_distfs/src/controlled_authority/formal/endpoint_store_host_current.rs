//! Actual disk/CPP and complete-reference gate for the private Host source.
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;

impl Store {
    pub(in crate::controlled_authority::formal::endpoint) fn validate_host_current_bounded(
        &self,
        public_key: &str,
        commit: &VerifiedFormalCommitV1,
        local_reference: &str,
        memory: &MemoryBudget,
        work: &super::FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let disk = self.read_hot_bounded(public_key, memory, work, stop)?;
        let (binding, _binding_fee) = bounded_transaction::disk_binding(&disk, memory, work, stop)?;
        if !bounded_transaction::same(disk.body(), &self.body, memory, work, stop)?
            || !bounded_transaction::same(&binding, &self.checkpoint, memory, work, stop)?
        {
            return Err(FormalError::Conflict(
                "Host actual disk/cache checkpoint changed".into(),
            ));
        }
        work.charge(1, 64, stop)?;
        let record = blake3::Hash::from_bytes(commit.verified_record_id()).to_hex();
        work.charge(
            1,
            (disk.body().head.record_id.len() + local_reference.len()) as u64,
            stop,
        )?;
        if disk.body().head.position != 1
            || disk.body().head.record_id != record.as_str()
            || disk.body().decision_head.is_some()
            || disk.body().pending.is_some()
            || disk.body().seal.is_some()
            || disk.body().data_closure.as_deref() != Some(local_reference)
        {
            return Err(invalid("Host exact durable GEN head/closure required"));
        }
        bounded_native_refs::validate_recovery_refs(self, disk.body(), memory, work, stop)
    }
}
