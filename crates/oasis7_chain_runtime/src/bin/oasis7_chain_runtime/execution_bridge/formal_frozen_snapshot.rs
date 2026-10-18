//! One execution's exact original Snapshot; never a persisted or authority cache.
use super::*;

/// The immutable borrows prevent mutation while an expanded Snapshot is reused.
/// Pointer identity deliberately rejects even an equal cloned source. No Clone
/// or serialization implementation may turn this into a cross-operation cache.
pub(crate) struct VerifiedFrozenSnapshot<'a> {
    frozen: &'a ExecutableFormalFrozenV1,
    body: &'a FormalFrozenBodyV1,
    material: &'a ClosedRecord,
    authority: &'a VerifiedAuthorityV1,
    parent: &'a VerifiedFormalCommitV1,
    bytes: Vec<u8>,
}

#[cfg(test)]
std::thread_local! {
    static EXPANSIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static RESTORES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl<'a> VerifiedFrozenSnapshot<'a> {
    pub(crate) fn new(
        frozen: &'a ExecutableFormalFrozenV1,
        authority: &'a VerifiedAuthorityV1,
        parent: &'a VerifiedFormalCommitV1,
    ) -> Result<Self, String> {
        let body = &frozen.input().body;
        let material = frozen.material();
        let bytes = match verify_frozen_material_retaining_snapshot(body, material)
            .map_err(|error| error.to_string())?
        {
            Some(bytes) => bytes,
            // The old schema-1 validator does not expand. This is its original
            // typed Snapshot decode's fresh read, now retained for this call.
            None => material
                .snapshot_bytes()
                .map_err(|error| error.to_string())?,
        };
        if bytes.len() > oasis7_runtime::MAX_SNAPSHOT_DELTA_BYTES
            || blake3_hex(&bytes) != body.pre_state_root
        {
            return Err("formal original Snapshot size or prestate mismatch".into());
        }
        verify_formal_reservation(frozen.input(), authority, parent)
            .map_err(|error| error.to_string())?;
        checked_execution_manifest(frozen, authority, parent)?;
        #[cfg(test)]
        EXPANSIONS.with(|count| count.set(count.get() + 1));
        Ok(Self {
            frozen,
            body,
            material,
            authority,
            parent,
            bytes,
        })
    }

    pub(crate) fn check_binding(
        &self,
        frozen: &ExecutableFormalFrozenV1,
        authority: &VerifiedAuthorityV1,
        parent: &VerifiedFormalCommitV1,
    ) -> Result<(), String> {
        if !std::ptr::eq(self.frozen, frozen)
            || !std::ptr::eq(self.body, &frozen.input().body)
            || !std::ptr::eq(self.material, frozen.material())
            || !std::ptr::eq(self.authority, authority)
            || !std::ptr::eq(self.parent, parent)
        {
            return Err("formal Snapshot token differs from exact original borrow".into());
        }
        Ok(())
    }

    pub(crate) fn bytes_for(&self, material: &ClosedRecord) -> Result<&[u8], String> {
        if !std::ptr::eq(self.material, material) {
            return Err("formal Snapshot token material borrow mismatch".into());
        }
        Ok(&self.bytes)
    }

    /// The rebuilt value is untrusted until every byte and field equals the
    /// immutable source already fully verified by this token's constructor.
    pub(crate) fn verify_exact_rebuild(
        &self,
        body: &FormalFrozenBodyV1,
        material: &ClosedRecord,
    ) -> Result<(), String> {
        if body != self.body || material != self.material {
            return Err("formal frozen exact typed execution closure mismatch".into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn record_restore(&self) {
        RESTORES.with(|count| count.set(count.get() + 1));
    }
}

#[cfg(test)]
pub(super) fn reset_counts() {
    EXPANSIONS.with(|count| count.set(0));
    RESTORES.with(|count| count.set(0));
}

#[cfg(test)]
pub(super) fn counts() -> (usize, usize) {
    (
        EXPANSIONS.with(|count| count.get()),
        RESTORES.with(|count| count.get()),
    )
}
