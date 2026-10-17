//! Stream external original native files into the same verified, pinned CAS as
//! the selected cut. They never expand the inline ClosedRecord manifest budget.
use super::*;
impl FormalFullSyncSession {
    pub(super) fn fetch_native_environment(
        &self,
        cut: &FormalFullSyncCutV1,
        material: &ClosedRecord,
        budget: &mut CasBudget,
    ) -> Result<(), String> {
        let Some(environment) = material
            .native_environment()
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        environment.verify(|digest| {
            let declared = environment
                .components
                .iter()
                .flat_map(|component| &component.chunks)
                .find(|chunk| chunk.digest == digest)
                .ok_or("native chunk declaration missing")?;
            let bytes = if let Some(existing) = self.read_cached_object(digest)? {
                existing
            } else {
                self.fetch_object(cut, digest, None, None, Some(declared.byte_len as u64))?
            };
            budget.add(digest, bytes.len() as u64)?;
            self.persist_cas_object(digest, &bytes)?;
            Ok(bytes)
        })
    }
}
