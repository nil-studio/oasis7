//! Read-only clone admission over the actual selected decoder's retained fees.
use super::*;
impl SelectedModuleManifestV1 {
    /// Additional JSON ownership is retained separately from the typed manifest
    /// reservation. A consumer cloning this DTO must include BOTH charges.
    pub fn admitted_json_bytes_v1(&self) -> Result<usize, ValueTreeError> {
        self._json.iter().try_fold(0usize, |total, charge| {
            total
                .checked_add(charge.bytes())
                .ok_or(ValueTreeError::ResourceLimited(
                    "selected manifest JSON clone admission",
                ))
        })
    }
}
