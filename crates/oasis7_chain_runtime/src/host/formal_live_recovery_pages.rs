//! Bounded numeric private files; each page is independently verified from C0.
use oasis7::world_service::formal_read::FormalReadHistoryPageV1;
use std::path::{Path, PathBuf};
pub(super) struct RecoveryPages {
    directory: PathBuf,
    next: u64,
    exhausted: bool,
}
impl RecoveryPages {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        super::super::super::controlled_live_config::require_private_owned(path, true)?;
        Ok(Self {
            directory: path.into(),
            next: 0,
            exhausted: false,
        })
    }
}
impl Iterator for RecoveryPages {
    type Item = Result<FormalReadHistoryPageV1, String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }
        let path = self.directory.join(format!("{:020}.cbor", self.next));
        let result = (|| {
            super::super::super::controlled_live_config::require_private_owned(&path, false)?;
            let bytes = super::super::super::controlled_history_cli::read_bounded(
                &path,
                u64::try_from(oasis7_client_api::world_service::MAX_FORMAL_PROOF_BYTES)
                    .map_err(|_| "recovery page byte bound exceeds u64")?,
            )?;
            let page = FormalReadHistoryPageV1::decode_bytes(&bytes)?;
            self.next = self
                .next
                .checked_add(1)
                .ok_or("recovery page position overflow")?;
            Ok(page)
        })();
        if result.is_err() {
            self.exhausted = true;
        }
        Some(result)
    }
}
