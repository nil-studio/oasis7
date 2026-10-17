use super::super::native_environment::{
    EndpointVerificationJob, NativeExecutionEnvironmentV1, VerificationJob,
};
use super::*;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
impl FileFormalEndpoint {
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
                },
            );
        }
        let directory = self.store.path.join("formal-objects-v1");
        let job = self
            .native_environment_jobs
            .get_mut(&id)
            .ok_or_else(|| invalid("native verification job"))?;
        let job = &mut job.job;
        job.advance(|hash| {
            super::super::business_pack::read_object_durable(&directory.join(hash), hash)?
                .ok_or_else(|| invalid("native local durable chunk missing"))
        })?;
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
    pub(super) fn take_native_environment(
        &mut self,
        material: &ClosedRecord,
    ) -> Result<Option<EndpointVerificationJob>, FormalError> {
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
