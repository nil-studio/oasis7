//! A borrow of actual authenticated native originals and their namespace pins.
use super::*;
/// Content/lease capability only. The Host must still execute the bounded
/// protocol handshake and bind the business invocation to its real transaction.
pub struct NativeWorkerLeaseV1<'a> {
    executable: &'a executable::VerifiedNativeExecutableV1,
    _material: &'a CandidateExecutionMaterialV1,
}
impl NativeWorkerLeaseV1<'_> {
    pub fn file(&self) -> &std::fs::File {
        self.executable.file()
    }
    pub fn digest(&self) -> [u8; 32] {
        self.executable.digest()
    }
    pub fn byte_len(&self) -> u64 {
        self.executable.byte_len()
    }
}
impl CandidateExecutionMaterialV1 {
    pub(crate) fn native_worker_lease_v1(
        &self,
        memory: &MemoryBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<NativeWorkerLeaseV1<'_>, FormalError> {
        if cancelled() {
            return Err(invalid("native worker lease cancelled"));
        }
        if !self.baseline.uses_budget(memory) || !self.baseline.proof.uses_budget(memory) {
            return Err(invalid("native worker lease pool"));
        }
        let executable = self
            .sealed_executable()
            .ok_or_else(|| invalid("native worker platform unsupported"))?;
        if !executable.platform_compatible()
            || !executable.uses_budget(memory)
            || executable.digest() != self.baseline.proof.native.executable_id()
            || executable.byte_len() == 0
            || executable
                .file()
                .metadata()
                .map_err(|_| invalid("native worker file metadata"))?
                .len()
                != executable.byte_len()
        {
            return Err(invalid("native worker actual executable binding"));
        }
        if cancelled() {
            return Err(invalid("native worker lease cancelled"));
        }
        Ok(NativeWorkerLeaseV1 {
            executable,
            _material: self,
        })
    }
}
