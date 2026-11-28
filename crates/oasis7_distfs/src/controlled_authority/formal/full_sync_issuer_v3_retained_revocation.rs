//! Local copies of externally issued, signed cutoff receipts. No local phase,
//! file or cache is a protected witness or permission to stop another owner.
use super::*;
use std::collections::BTreeSet;

pub(super) const FENCES_FILE: &str = "full-sync-revocation-fences-v1.json";
const MAX_RECEIPT_BYTES: usize = 16 * 1024;

// One bounded file for the entire issuer, not a new unbounded file per revoke.
// Strings preserve the exact incoming UTF-8 receipt bytes, including whitespace.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedFences {
    schema_version: u32,
    receipts: Vec<String>,
}

fn decode(bytes: &[u8]) -> Result<SignedFullSyncRevocationFenceV1, FormalError> {
    if bytes.is_empty() || bytes.len() > MAX_RECEIPT_BYTES {
        return Err(invalid("original revocation receipt byte bound"));
    }
    serde_json::from_slice(bytes).map_err(|error| invalid(error.to_string()))
}

fn load(directory: &Path) -> Result<RetainedFences, FormalError> {
    let retained: RetainedFences =
        read_optional_bounded(&directory.join(FENCES_FILE), MAX_LEDGER_BYTES)?.unwrap_or(
            RetainedFences {
                schema_version: 1,
                receipts: Vec::new(),
            },
        );
    if retained.schema_version != 1 {
        return Err(invalid("original revocation receipt cache schema"));
    }
    let mut identities = BTreeSet::new();
    for bytes in &retained.receipts {
        let receipt = decode(bytes.as_bytes())?;
        if !identities.insert((
            receipt.body.revocation_record_digest,
            receipt.body.original_lease,
        )) {
            return Err(FormalError::Conflict(
                "duplicate original revocation receipt identity".into(),
            ));
        }
    }
    Ok(retained)
}

pub(super) fn read_inventory(
    directory: &Path,
    state: &VerifiedFullSyncIssuerStateV3,
    original: &FullSyncRevocationInventoryV1,
) -> Result<FullSyncRevocationInventoryV1, FormalError> {
    original.verify_inventory(state)?;
    let mut inventory = original.clone();
    for bytes in load(directory)?.receipts {
        let receipt = decode(bytes.as_bytes())?;
        if receipt.body.revocation_record_digest == inventory.revocation_record_digest {
            inventory.record_after_history_verification(state, receipt)?;
        }
    }
    Ok(inventory)
}

impl DurableFullSyncIssuerV3 {
    /// Retain one already-issued protected fence for subsequent management
    /// queries. Management authorization belongs to the caller. This does not
    /// sign a fence, change witness state, or claim protection for local storage.
    /// Unknown local writes require reopening and retrying these same bytes.
    pub fn retain_revocation_fence(
        &mut self,
        revoke_record_digest: &str,
        original_receipt_bytes: &[u8],
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncRevocationSummaryV1, FormalError> {
        let receipt = decode(original_receipt_bytes)?;
        if receipt.body.revocation_record_digest != revoke_record_digest {
            return Err(FormalError::Conflict(
                "retained fence differs from original revocation".into(),
            ));
        }
        let state = self.synchronize_v3(witness)?;
        let original =
            FullSyncRevocationInventoryV1::from_verified_history(&state, revoke_record_digest)?;
        let mut inventory = read_inventory(&self.directory, &state, &original)?;
        inventory.record_after_history_verification(&state, receipt.clone())?;
        let mut retained = load(&self.directory)?;
        let mut identical = false;
        for bytes in &retained.receipts {
            let existing = decode(bytes.as_bytes())?;
            if existing.body.revocation_record_digest == revoke_record_digest
                && existing.body.original_lease == receipt.body.original_lease
            {
                if bytes.as_bytes() != original_receipt_bytes {
                    return Err(FormalError::Conflict(
                        "original revocation receipt bytes cannot be replaced".into(),
                    ));
                }
                identical = true;
            }
        }
        if !identical {
            retained.receipts.push(
                String::from_utf8(original_receipt_bytes.to_vec())
                    .map_err(|error| invalid(error.to_string()))?,
            );
            let bytes = serialize_bounded(&retained, MAX_LEDGER_BYTES)?;
            let result = atomic_bounded(
                &self.directory,
                &self.directory.join(FENCES_FILE),
                &bytes,
                MAX_LEDGER_BYTES,
            )
            .and_then(|()| {
                if load(&self.directory)? != retained {
                    return Err(FormalError::Rollback(
                        "original revocation receipt exact readback mismatch".into(),
                    ));
                }
                Ok(())
            });
            if let Err(error) = result {
                self.poisoned = true;
                return Err(error);
            }
        }
        // Reconcile the current witness again after persistence. A concurrent
        // reissue or unavailable witness must not return a stale effective result.
        self.read_revocation_status(revoke_record_digest, &original, witness)
    }
}
