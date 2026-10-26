//! Historical Root facts are verified independently of live business authority.
use super::*;
use oasis7_custody_core::native_history::NativeHistoryEntryV1;

/// Deployment-owned public verification material, never deserialized from a caller.
/// No source freshness, signing seed, KEK, or mutable business approval is carried here.
#[derive(Clone)]
pub struct NativeHistoryVerificationContext {
    pub native_root_key_id: String,
    pub native_root_public_key: String,
    pub http_origin: String,
}
impl NativeHistoryVerificationContext {
    fn as_core(&self) -> oasis7_custody_core::native_history::NativeHistoryVerificationContext<'_> {
        oasis7_custody_core::native_history::NativeHistoryVerificationContext {
            native_root_key_id: &self.native_root_key_id,
            native_root_public_key: &self.native_root_public_key,
            http_origin: &self.http_origin,
        }
    }
}
impl OwnerProvisionApprovalStore {
    pub fn native_history_entry(&self, digest: &str) -> Result<NativeHistoryEntryV1> {
        self.snapshot()?
            .state
            .native_history()
            .get(digest)
            .cloned()
            .ok_or_else(rejected)
    }
    pub async fn archive_native_history(
        &self,
        entry: NativeHistoryEntryV1,
        verification: &NativeHistoryVerificationContext,
    ) -> Result<NativeHistoryEntryV1> {
        let digest = entry.digest().map_err(|_| rejected())?;
        let checks = self
            .snapshot()?
            .state
            .native_history_archive_checks(&entry, &verification.as_core())
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        let mut archived = None;
        // tighten validates known Account inventory without requiring an unfrozen
        // business lane. Archival facts cannot reactivate any authority.
        self.tighten("native_history_archived", |state| {
            let checks = state
                .native_history_archive_checks(&entry, &verification.as_core())
                .map_err(|_| rejected())?;
            cache.verify(&checks).map_err(|_| rejected())?;
            let original = state
                .archive_native_history(entry.clone(), &verification.as_core(), &mut cache)
                .map_err(|_| rejected())?;
            if original.digest().map_err(|_| rejected())? != digest {
                return Err(rejected());
            }
            archived = Some(original);
            Ok(())
        })?;
        archived.ok_or_else(rejected)
    }
}
