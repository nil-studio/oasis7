//! Original-operation index in the *same* fenced Fresh/FirstReserve namespace.
//!
//! Dark only: this module neither signs, persists nor serves originals. A real
//! backend must commit the complete current tuple, operation identity, original
//! signed bytes and spend with ONE protected CAS. A second WAL/CAS is forbidden.
//! A local equality check is not an independent protection or time witness.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::binding::{BftSignerErrorV3, invalid};
use super::state::validate_digest;
use super::{BftFirstReserveSnapshotV1, BftPrivateRetentionAdmissionInputRefV1};

const REQUEST_DOMAIN: &[u8] = b"oasis7.bft.private-retention.original-request.v1\0";
const RECEIPT_DOMAIN: &[u8] = b"oasis7.bft.private-retention.original-receipt.v1\0";
const MAX_ORIGINAL_OPERATIONS: usize = 4096;
const MAX_ORIGINAL_BYTES: usize = 1024 * 1024;
const MAX_ORIGINAL_RETAINED_BYTES: usize = 64 * 1024 * 1024;
const MAX_ORIGINAL_REFS: usize = 16;

/// Hash exact original signed bytes, not a decoded/reencoded receipt. Retain
/// the original byte string beside its digest in the protected transaction.
fn byte_digest(domain: &[u8], original: &[u8]) -> String {
    let mut digest = blake3::Hasher::new();
    digest.update(domain);
    digest.update(&(original.len() as u64).to_be_bytes());
    digest.update(original);
    digest.finalize().to_hex().to_string()
}

/// Original identity and immutable response for an already independently
/// verified operation. This is not a current lease or response authorization.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftRetainedOriginalOperationV1 {
    pub schema_version: u32,
    pub operation_id: String,
    pub source: super::BftPrivateRetentionSourceV1,
    pub ledger_origin_digest: String,
    pub signed_request_bytes: Vec<u8>,
    pub original_request_digest: String,
    pub before_revision: u64,
    pub before_state_digest: String,
    pub after_revision: u64,
    pub after_state_digest: String,
    pub original_lease_generation: u64,
    pub original_lease_token_digest: String,
    pub input_refs: Vec<BftPrivateRetentionAdmissionInputRefV1>,
    pub signed_receipt_bytes: Vec<u8>,
    pub original_receipt_digest: String,
    pub cumulative_reserved_response_bytes: u64,
}

impl std::fmt::Debug for BftRetainedOriginalOperationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BftRetainedOriginalOperationV1")
            .field("operation_id", &self.operation_id)
            .field("original_request_digest", &self.original_request_digest)
            .field("original_receipt_digest", &self.original_receipt_digest)
            .field("original_request_bytes", &self.signed_request_bytes.len())
            .field("original_receipt_bytes", &self.signed_receipt_bytes.len())
            .finish_non_exhaustive()
    }
}

impl BftRetainedOriginalOperationV1 {
    /// This checks shape/byte identity only. Signature verification, original
    /// H/source authority, charge, Ready and Seal remain independent obligations.
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.schema_version != 1
            || self.operation_id.is_empty()
            || self.operation_id.len() > 128
            || !self
                .operation_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-:".contains(&byte))
            || self.signed_request_bytes.is_empty()
            || self.signed_request_bytes.len() > MAX_ORIGINAL_BYTES
            || self.signed_receipt_bytes.is_empty()
            || self.signed_receipt_bytes.len() > MAX_ORIGINAL_BYTES
            || self.input_refs.is_empty()
            || self.input_refs.len() > MAX_ORIGINAL_REFS
            || self.original_lease_generation == 0
            || self.after_revision < self.before_revision
        {
            return Err(invalid("original operation identity or byte capacity invalid"));
        }
        self.source.validate()?;
        for digest in [
            &self.ledger_origin_digest,
            &self.original_request_digest,
            &self.before_state_digest,
            &self.after_state_digest,
            &self.original_lease_token_digest,
            &self.original_receipt_digest,
        ] {
            validate_digest(digest)?;
        }
        if self.original_request_digest != byte_digest(REQUEST_DOMAIN, &self.signed_request_bytes)
            || self.original_receipt_digest != byte_digest(RECEIPT_DOMAIN, &self.signed_receipt_bytes)
        {
            return Err(invalid("original signed byte identity changed"));
        }
        let mut previous: Option<&str> = None;
        for reference in &self.input_refs {
            reference.validate()?;
            if reference.source != self.source
                || reference.key.ledger_origin_digest != self.ledger_origin_digest
                || previous.is_some_and(|before| before >= reference.key.header_id.as_str())
            {
                return Err(invalid("original operation source/ref order or identity differs"));
            }
            previous = Some(&reference.key.header_id);
        }
        Ok(())
    }

    pub fn request_digest_from_original_bytes(original: &[u8]) -> String {
        byte_digest(REQUEST_DOMAIN, original)
    }

    pub fn receipt_digest_from_original_bytes(original: &[u8]) -> String {
        byte_digest(RECEIPT_DOMAIN, original)
    }
}

/// A single CAS envelope. The existing original Fresh/FirstReserve tuple is
/// never recomputed or rewritten; the operation index extends that same value.
/// The production backend owns a single writer, lease/floor and exact readback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftOriginalRetentionTxnSnapshotV1 {
    pub current: BftFirstReserveSnapshotV1,
    pub original_operations: BTreeMap<String, BftRetainedOriginalOperationV1>,
}

impl BftOriginalRetentionTxnSnapshotV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.validate_with_byte_budget(MAX_ORIGINAL_RETAINED_BYTES)
    }

    fn validate_with_byte_budget(&self, maximum_bytes: usize) -> Result<(), BftSignerErrorV3> {
        if self.original_operations.len() > MAX_ORIGINAL_OPERATIONS {
            return Err(invalid("original operation index capacity exceeded"));
        }
        let mut original_bytes = 0_usize;
        for (id, operation) in &self.original_operations {
            operation.validate()?;
            original_bytes = original_bytes
                .checked_add(operation.signed_request_bytes.len())
                .and_then(|total| total.checked_add(operation.signed_receipt_bytes.len()))
                .ok_or_else(|| invalid("original operation index byte overflow"))?;
            if original_bytes > maximum_bytes {
                return Err(invalid("original operation index total byte limit"));
            }
            if id != &operation.operation_id
                || operation.original_lease_generation > self.current.current.signer.lease_generation
                || (operation.source == self.current.current.marker.source
                    && operation.ledger_origin_digest
                        == self.current.current.marker.ledger_origin_digest
                    && operation.after_revision > self.current.current.signer.state.revision)
                || operation.cumulative_reserved_response_bytes
                    > self.current.current.reserved_response_bytes
            {
                return Err(invalid("protected original operation index differs from current"));
            }
        }
        Ok(())
    }

    /// Historical read identity uses the original request bytes. A newer
    /// current lease does NOT re-sign or replace an older operation result.
    pub fn lookup_original(
        &self,
        operation_id: &str,
        signed_request_bytes: &[u8],
    ) -> Result<Option<&BftRetainedOriginalOperationV1>, BftSignerErrorV3> {
        self.validate()?;
        let Some(stored) = self.original_operations.get(operation_id) else {
            return Ok(None);
        };
        if stored.original_request_digest
            != byte_digest(REQUEST_DOMAIN, signed_request_bytes)
            || stored.signed_request_bytes != signed_request_bytes
        {
            return Err(invalid("original operation identity reused with different bytes"));
        }
        Ok(Some(stored))
    }

    /// Prepare exactly one new operation under the CURRENT expected lease.
    /// Duplicate original identity may only yield the identical old result;
    /// no second debit, journal or signed receipt is permitted on replay.
    pub fn prepare_single_atomic_operation(
        &self,
        next_current: BftFirstReserveSnapshotV1,
        new_operation: BftRetainedOriginalOperationV1,
    ) -> Result<Self, BftSignerErrorV3> {
        self.validate()?;
        new_operation.validate()?;
        if let Some(previous) = self.original_operations.get(&new_operation.operation_id) {
            if previous != &new_operation || next_current != self.current {
                return Err(invalid("original retry attempts to alter protected result"));
            }
            return Ok(self.clone());
        }
        if self.original_operations.len() == MAX_ORIGINAL_OPERATIONS {
            return Err(invalid("original operation index full"));
        }

        let before = &self.current.current.signer;
        let after = &next_current.current.signer;
        if before.lease_generation == 0
            || before.lease_token_digest.is_empty()
            || after.lease_generation != before.lease_generation
            || after.lease_token_digest != before.lease_token_digest
            || after.protected_time_floor_ms < before.protected_time_floor_ms
            || after.state.binding_digest != before.state.binding_digest
            || new_operation.original_lease_generation != before.lease_generation
            || new_operation.original_lease_token_digest != before.lease_token_digest
            || new_operation.before_revision != before.state.revision
            || new_operation.before_state_digest != before.state.state_digest
            || new_operation.after_revision != after.state.revision
            || new_operation.after_state_digest != after.state.state_digest
            || new_operation.source != self.current.current.marker.source
            || new_operation.ledger_origin_digest
                != self.current.current.marker.ledger_origin_digest
            || next_current.current.marker != self.current.current.marker
            || next_current.current.original != self.current.current.original
            || (self.current.reservation.is_some()
                && next_current.reservation != self.current.reservation)
            || next_current.current.reserved_response_bytes
                <= self.current.current.reserved_response_bytes
            || new_operation.cumulative_reserved_response_bytes
                != next_current.current.reserved_response_bytes
        {
            return Err(invalid("original operation whole-tuple CAS transition invalid"));
        }

        let mut next = Self {
            current: next_current,
            original_operations: self.original_operations.clone(),
        };
        next.original_operations
            .insert(new_operation.operation_id.clone(), new_operation);
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{
        BftFreshCoverageSnapshotV1, BftPrivateRetentionCreationMarkerV1,
        BftPrivateRetentionSourceV1, BftSignerBackendSnapshotV3,
        BftSignerStateV3, BftSuccessorWindowArchiveRefV1,
    };

    fn hex_hash(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn source() -> BftPrivateRetentionSourceV1 {
        BftPrivateRetentionSourceV1 {
            binding_digest: hex_hash(1),
            handoff_digest: hex_hash(2),
            material_source_digest: hex_hash(3),
        }
    }

    fn current(revision: u64, reserved: u64) -> BftFirstReserveSnapshotV1 {
        let state: BftSignerStateV3 = serde_json::from_value(serde_json::json!({
            "schema_version": 3,
            "binding_digest": hex_hash(1),
            "revision": revision,
            "history": [],
            "journal": null,
            "completed_journals": [],
            "pending_driver_apply": null,
            "applied_installs": [],
            "state_digest": hex_hash(revision as u8 + 10)
        }))
        .unwrap();
        BftFirstReserveSnapshotV1 {
            current: BftFreshCoverageSnapshotV1 {
                signer: BftSignerBackendSnapshotV3 {
                    state,
                    lease_generation: 7,
                    lease_token_digest: hex_hash(4),
                    protected_time_floor_ms: 100,
                },
                marker: BftPrivateRetentionCreationMarkerV1 {
                    schema_version: 1,
                    creation_identity: hex_hash(5),
                    source: source(),
                    ledger_origin_digest: hex_hash(6),
                    activation_digest: hex_hash(7),
                    initial_state_digest: hex_hash(8),
                    initial_height: 1,
                },
                original: None,
                reserved_response_bytes: reserved,
            },
            reservation: None,
        }
    }

    fn operation() -> BftRetainedOriginalOperationV1 {
        let signed_request_bytes = vec![0x31, 0, 0xff, 0x12];
        let signed_receipt_bytes = vec![0x44, 0, 0xfe, 0x30];
        let original_request_digest =
            BftRetainedOriginalOperationV1::request_digest_from_original_bytes(&signed_request_bytes);
        let original_receipt_digest =
            BftRetainedOriginalOperationV1::receipt_digest_from_original_bytes(&signed_receipt_bytes);
        BftRetainedOriginalOperationV1 {
            schema_version: 1,
            operation_id: "original-reserve-1".into(),
            source: source(),
            ledger_origin_digest: hex_hash(6),
            original_request_digest,
            signed_request_bytes,
            before_revision: 1,
            before_state_digest: hex_hash(11),
            after_revision: 2,
            after_state_digest: hex_hash(12),
            original_lease_generation: 7,
            original_lease_token_digest: hex_hash(4),
            input_refs: vec![BftPrivateRetentionAdmissionInputRefV1 {
                source: source(),
                key: super::super::BftPrivateRetentionKeyV1 {
                    ledger_origin_digest: hex_hash(6),
                    height: 1,
                    header_id: hex_hash(9),
                },
                object: BftSuccessorWindowArchiveRefV1 {
                    digest: hex_hash(10),
                    bytes: 1024,
                },
                admission_digest: hex_hash(11),
            }],
            original_receipt_digest,
            signed_receipt_bytes,
            cumulative_reserved_response_bytes: 100,
        }
    }

    #[test]
    fn exact_single_protected_transaction_retains_original_bytes_and_idempotent_result() {
        let before = BftOriginalRetentionTxnSnapshotV1 {
            current: current(1, 0),
            original_operations: BTreeMap::new(),
        };
        let original = operation();
        let committed = before
            .prepare_single_atomic_operation(current(2, 100), original.clone())
            .unwrap();
        assert!(before.original_operations.is_empty());
        assert_eq!(committed.original_operations.len(), 1);
        assert_eq!(
            committed
                .lookup_original(&original.operation_id, &original.signed_request_bytes)
                .unwrap()
                .unwrap(),
            &original
        );
        assert_eq!(
            committed
                .prepare_single_atomic_operation(committed.current.clone(), original.clone())
                .unwrap(),
            committed
        );
        assert!(
            committed
                .lookup_original(&original.operation_id, b"other original")
                .is_err()
        );
        let mut substituted = original.clone();
        substituted.signed_receipt_bytes[0] ^= 1;
        assert!(committed
            .prepare_single_atomic_operation(committed.current.clone(), substituted)
            .is_err());
        let encoded = serde_cbor::to_vec(&original).unwrap();
        let cold: BftRetainedOriginalOperationV1 = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(cold, original);
        assert_eq!(cold.signed_receipt_bytes, [0x44, 0, 0xfe, 0x30]);
    }

    #[test]
    fn original_whole_tuple_refuses_stale_lease_floor_spend_and_changed_identity() {
        let before = BftOriginalRetentionTxnSnapshotV1 {
            current: current(1, 0),
            original_operations: BTreeMap::new(),
        };
        let original = operation();
        let good = current(2, 100);
        let mut stale = good.clone();
        stale.current.signer.lease_generation = 8;
        assert!(before.prepare_single_atomic_operation(stale, original.clone()).is_err());
        let mut token = good.clone();
        token.current.signer.lease_token_digest = hex_hash(12);
        assert!(before.prepare_single_atomic_operation(token, original.clone()).is_err());
        let mut floor = good.clone();
        floor.current.signer.protected_time_floor_ms = 99;
        assert!(before.prepare_single_atomic_operation(floor, original.clone()).is_err());
        let mut refund = good.clone();
        refund.current.reserved_response_bytes = 0;
        assert!(before.prepare_single_atomic_operation(refund, original.clone()).is_err());
        let mut wrong = original.clone();
        wrong.original_request_digest = hex_hash(15);
        assert!(before.prepare_single_atomic_operation(good.clone(), wrong).is_err());
        let mut crossed = original.clone();
        crossed.original_lease_generation += 1;
        assert!(before.prepare_single_atomic_operation(good.clone(), crossed).is_err());
        let mut changed = good.clone();
        changed.current.marker.creation_identity = hex_hash(16);
        assert!(before.prepare_single_atomic_operation(changed, original).is_err());
    }

    #[test]
    fn cold_history_keeps_original_operation_across_new_lease_and_rejects_corruption() {
        let first = BftOriginalRetentionTxnSnapshotV1 {
            current: current(1, 0),
            original_operations: BTreeMap::new(),
        };
        let original = operation();
        let mut stored = first
            .prepare_single_atomic_operation(current(2, 100), original.clone())
            .unwrap();
        stored.current.current.signer.lease_generation = 8;
        stored.current.current.signer.lease_token_digest = hex_hash(20);
        stored.validate().unwrap();
        // A newer current H/window may reset its local signer revision. The old
        // signed operation belongs to the original source, not the new marker.
        stored.current.current.marker.source.handoff_digest = hex_hash(21);
        stored.current.current.marker.ledger_origin_digest = hex_hash(22);
        stored.current.current.signer.state.revision = 0;
        stored.validate().unwrap();
        assert_eq!(
            stored
                .lookup_original(&original.operation_id, &original.signed_request_bytes)
                .unwrap()
                .unwrap()
                .signed_receipt_bytes,
            original.signed_receipt_bytes
        );
        let mut rollback = stored.clone();
        rollback.current.current.reserved_response_bytes = 99;
        assert!(rollback.validate().is_err());
        let mut rewritten = stored.clone();
        rewritten
            .original_operations
            .get_mut(&original.operation_id)
            .unwrap()
            .signed_receipt_bytes[0] ^= 1;
        assert!(rewritten.validate().is_err());
        let mut bounded = stored.clone();
        let mut another = operation();
        another.operation_id = "original-reserve-2".into();
        bounded
            .original_operations
            .insert(another.operation_id.clone(), another);
        bounded.validate_with_byte_budget(16).unwrap();
        assert!(bounded.validate_with_byte_budget(15).is_err());
        let mut collision = stored.clone();
        collision.original_operations.remove(&original.operation_id);
        collision.original_operations.insert("different".into(), original);
        assert!(collision.validate().is_err());
    }
}
