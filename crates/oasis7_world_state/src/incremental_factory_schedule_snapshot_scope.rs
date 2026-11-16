//! Private authentic Current cut: only the transaction's actual net writes.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
pub(crate) struct SnapshotScopeV1 {
    digest: [u8; 32],
}
struct Loan<'a> {
    out: &'a mut u64,
    before: u64,
    admitted: usize,
    left: usize,
}
impl Drop for Loan<'_> {
    fn drop(&mut self) {
        *self.out = self
            .before
            .saturating_sub((self.admitted - self.left) as u64);
    }
}
impl SnapshotScopeV1 {
    pub(crate) fn capture<M: MerkleNodeReader>(
        tx: &ExecutionTransaction<'_, M>,
        cells: &mut u64,
        bytes: &mut u64,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<Self, WorldFieldOverlayErrorV1> {
        let before = *cells;
        let admitted = usize::try_from(before).unwrap_or(usize::MAX);
        *cells = 0;
        let mut loan = Loan {
            out: cells,
            before,
            admitted,
            left: admitted,
        };
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.factory-schedule.actual-current-cut.v1");
        for (i, ns) in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Allocator,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Authorization,
            StateNamespaceV1::Effects,
            StateNamespaceV1::Artifacts,
        ]
        .into_iter()
        .enumerate()
        {
            let stop = std::cell::RefCell::new(&mut *cancel);
            tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
                ns,
                &mut loan.left,
                || {
                    if (**stop.borrow_mut())() {
                        Err(ValueTreeError::Cancelled.into())
                    } else {
                        Ok(())
                    }
                },
                |key, before, after| {
                    let n = key.len().checked_add(1 + 8 + 2 + 64).ok_or_else(invalid)?;
                    *bytes = bytes
                        .checked_sub(n as u64)
                        .ok_or(ValueTreeError::ResourceLimited(
                            "factory schedule cut bytes",
                        ))?;
                    hash.update(&[i as u8]);
                    hash.update(&(key.len() as u64).to_be_bytes());
                    hash.update(key);
                    for value in [before, after] {
                        hash.update(&[u8::from(value.is_some())]);
                        if let Some(value) = value {
                            hash.update(&value);
                        }
                    }
                    Ok(())
                },
            )?;
        }
        Ok(Self {
            digest: *hash.finalize().as_bytes(),
        })
    }
    pub(crate) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        cells: &mut u64,
        bytes: &mut u64,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        let current = Self::capture(tx, cells, bytes, cancel)?;
        if self.digest != current.digest {
            return Err(invalid().into());
        }
        Ok(())
    }
}
