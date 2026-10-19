use super::super::native_environment::{
    EndpointVerificationJob, NativeExecutionEnvironmentV1, VerificationJob,
};
use super::*;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
impl FileFormalEndpoint {
    /// Expire only volatile verification evidence, never retained chunks/history.
    /// The job map has at most two entries; this path performs no storage I/O.
    pub(crate) fn reap_idle_native_environment_verifications(
        &mut self,
        now: std::time::Instant,
    ) -> usize {
        let before = self.native_environment_jobs.len();
        self.native_environment_jobs.retain(|_, entry| {
            now.saturating_duration_since(entry.last_activity) < std::time::Duration::from_secs(300)
        });
        let removed = before - self.native_environment_jobs.len();
        if removed != 0 {
            eprintln!("formal_endpoint_native_verification_idle_expired jobs={removed}");
        }
        removed
    }

    pub fn stage_native_environment(
        &mut self,
        descriptor: &NativeExecutionEnvironmentV1,
        chunks: &[NativeEnvironmentChunkBytesV1],
    ) -> Result<(), FormalError> {
        let _guard = self.guard()?;
        descriptor.validate().map_err(invalid)?;
        if chunks.is_empty() || chunks.len() > 8 {
            return Err(invalid("native stage batch count"));
        }
        let mut total = 0usize;
        let mut objects = Vec::new();
        for object in chunks {
            let declared = descriptor
                .components
                .iter()
                .flat_map(|part| &part.chunks)
                .find(|chunk| chunk.digest == object.digest)
                .ok_or_else(|| invalid("native undeclared chunk"))?;
            if object.bytes.len() != declared.byte_len as usize
                || blake3::hash(&object.bytes).to_hex().as_str() != object.digest
            {
                return Err(invalid("native stage chunk length/hash"));
            }
            total = total
                .checked_add(object.bytes.len())
                .ok_or_else(|| invalid("native stage overflow"))?;
            objects.push(super::super::business_index::BusinessIndexNodeWriteV1 {
                digest: object.digest.clone(),
                bytes: object.bytes.clone(),
            });
        }
        if total > 8 * 1024 * 1024 {
            return Err(invalid("native stage byte budget"));
        }
        // Restaging explicitly invalidates previous readback evidence and releases
        // its pin, so an interrupted upload can restart without a stale lease.
        self.native_environment_jobs.clear();
        objects.sort_by(|a, b| a.digest.cmp(&b.digest));
        objects.dedup_by(|a, b| a.digest == b.digest);
        let directory = self.store.path.join("formal-objects-v1");
        super::super::business_pack::persist(&directory, &objects)?;
        // Publication registers every external chunk under the same physical ledger.
        Ok(())
    }
    pub fn advance_native_environment_verification(
        &mut self,
        descriptor: &NativeExecutionEnvironmentV1,
    ) -> Result<NativeEnvironmentVerificationProgressV1, FormalError> {
        let _guard = self.guard()?;
        self.reap_idle_native_environment_verifications(std::time::Instant::now());
        let id = descriptor.digest().map_err(invalid)?;
        if !self.native_environment_jobs.contains_key(&id) {
            if self.native_environment_jobs.len() >= 2 {
                self.native_environment_jobs.clear();
            }
            let pin = crate::controlled_authority::formal::physical::pin(
                &self.store.path.join("formal-objects-v1"),
                false,
            )?
            .ok_or_else(|| invalid("native verification requires physical live pin"))?;
            self.native_environment_jobs.insert(
                id.clone(),
                EndpointVerificationJob {
                    job: VerificationJob::new(descriptor.clone()),
                    _pin: pin,
                    last_activity: std::time::Instant::now(),
                },
            );
        }
        let directory = self.store.path.join("formal-objects-v1");
        let job = self
            .native_environment_jobs
            .get_mut(&id)
            .ok_or_else(|| invalid("native verification job"))?;
        job.last_activity = std::time::Instant::now();
        let result = job.job.advance(|hash| {
            super::super::business_pack::read_object_durable(&directory.join(hash), hash)?
                .ok_or_else(|| invalid("native local durable chunk missing"))
        });
        // Measure idleness from the end of the call, including an unsuccessful
        // bounded read, rather than counting its own active I/O as idle time.
        job.last_activity = std::time::Instant::now();
        result?;
        let job = &job.job;
        Ok(NativeEnvironmentVerificationProgressV1 {
            complete: job.complete(),
            verified_chunks: job
                .descriptor
                .components
                .iter()
                .take(job.component)
                .map(|component| component.chunks.len() as u64)
                .sum::<u64>()
                + job.chunk as u64,
            descriptor_digest: id,
            endpoint_instance: self.instance().clone(),
        })
    }
    /// Idempotently release ephemeral verification evidence. Original descriptor
    /// and chunk bytes remain retained; a later Prepare must verify them afresh.
    pub fn release_native_environment_verification(
        &mut self,
        descriptor_digest: &str,
    ) -> Result<(), FormalError> {
        let _guard = self.guard()?;
        hash_hex(descriptor_digest)?;
        self.native_environment_jobs.remove(descriptor_digest);
        Ok(())
    }

    pub(super) fn take_native_environment(
        &mut self,
        material: &ClosedRecord,
    ) -> Result<Option<EndpointVerificationJob>, FormalError> {
        self.reap_idle_native_environment_verifications(std::time::Instant::now());
        if let Some(descriptor) = material
            .native_environment()
            .map_err(|e| invalid(e.to_string()))?
        {
            let id = descriptor.digest().map_err(invalid)?;
            if !self
                .native_environment_jobs
                .get(&id)
                .is_some_and(|entry| entry.job.complete())
            {
                return Err(invalid(
                    "native environment local durable verification incomplete; advance verification first",
                ));
            }
            return Ok(self.native_environment_jobs.remove(&id));
        }
        Ok(None)
    }
}
