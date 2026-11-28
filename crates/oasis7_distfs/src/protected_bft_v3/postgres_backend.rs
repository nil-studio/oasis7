//! Explicit PostgreSQL adapter. The caller owns TLS, provisioning and isolation.
//! SQL atomicity alone does not qualify a protected backend or supply time.
use super::binding::invalid;
use super::*;
use postgres::{Client, IsolationLevel};
use std::sync::Mutex;
mod codec;
#[cfg(test)]
mod tests;

/// DDL for an independent provisioner; the runtime never creates or resets rows.
pub const POSTGRES_BFT_SCHEMA_V1: &str = r"-- Apply only through an independent provisioning identity. Runtime never runs DDL.
-- This schema supplies SQL atomicity, not HSM custody or rollback protection.
CREATE TABLE oasis7_bft_v3_namespaces (
    namespace TEXT PRIMARY KEY CHECK (length(namespace) BETWEEN 1 AND 128),
    binding_digest TEXT NOT NULL CHECK (binding_digest ~ '^[0-9a-f]{64}$'),
    body BYTEA NOT NULL CHECK (octet_length(body) BETWEEN 1 AND 536870912)
);
";
const READ: &str = "SELECT binding_digest, body FROM oasis7_bft_v3_namespaces WHERE namespace = $1";

/// A concrete backend over a caller-configured connection, never selected by
/// the production factory. Ambiguous commits/readbacks poison this instance;
/// reconnecting can inspect the durable owner but cannot steal its lease.
pub struct PostgresBftSignerBackendV3 {
    namespace: String,
    binding_digest: String,
    client: Mutex<Client>,
    poisoned: bool,
    #[cfg(test)]
    after_commit: Option<fn(&mut Client) -> Result<(), BftSignerErrorV3>>,
}
impl PostgresBftSignerBackendV3 {
    pub fn new(
        namespace: String,
        binding_digest: String,
        client: Client,
    ) -> Result<Self, BftSignerErrorV3> {
        validate_identity(&namespace, &binding_digest)?;
        Ok(Self {
            namespace,
            binding_digest,
            client: Mutex::new(client),
            poisoned: false,
            #[cfg(test)]
            after_commit: None,
        })
    }

    /// Encodes an already authorized whole namespace for an independent
    /// provisioner. This helper performs no database write or key operation.
    pub fn provisioned_bytes(
        namespace: &str,
        signer: BftSignerBackendSnapshotV3,
        retained: Option<BftOriginalRetentionTxnSnapshotV1>,
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        codec::Stored::new(namespace, signer, retained)?.encode()
    }

    fn run<T>(
        &mut self,
        change: impl FnOnce(&mut codec::Stored) -> Result<T, BftSignerErrorV3>,
    ) -> Result<T, BftSignerErrorV3> {
        if self.poisoned {
            return Err(invalid(
                "PG backend poisoned; reconnect and inspect durable owner",
            ));
        }
        // All SQL errors, including known serialization aborts, fail closed.
        // No retry here can repeat a signer action or turn uncertainty into conflict.
        self.poisoned = true;
        let client = self
            .client
            .get_mut()
            .map_err(|_| invalid("PG client lock poisoned"))?;
        let mut tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::Serializable)
            .start()
            .map_err(sql_error)?;
        let row = tx
            .query_opt(&format!("{READ} FOR UPDATE"), &[&self.namespace])
            .map_err(sql_error)?
            .ok_or_else(|| invalid("PG namespace was not independently provisioned"))?;
        let original: Vec<u8> = row.try_get(1).map_err(sql_error)?;
        let mut stored = codec::Stored::decode(&original)?;
        if row.try_get::<_, String>(0).map_err(sql_error)? != self.binding_digest
            || stored.binding_digest != self.binding_digest
            || stored.namespace != self.namespace
        {
            return Err(invalid("PG row namespace/binding differs"));
        }
        let result = change(&mut stored)?;
        let next = stored.encode()?;
        if next != original {
            let changed = tx
                .execute(
                    "UPDATE oasis7_bft_v3_namespaces SET body = $1 \
                 WHERE namespace = $2 AND binding_digest = $3 AND body = $4",
                    &[&next, &self.namespace, &self.binding_digest, &original],
                )
                .map_err(sql_error)?;
            if changed != 1 {
                return Err(invalid(
                    "PG whole namespace CAS did not update exactly one row",
                ));
            }
        }
        tx.commit().map_err(sql_error)?;
        if next != original {
            #[cfg(test)]
            if let Some(fault) = self.after_commit.take() {
                fault(client)?;
            }
            let readback = client
                .query_opt(READ, &[&self.namespace])
                .map_err(sql_error)?;
            let row = readback.ok_or_else(|| invalid("PG committed namespace disappeared"))?;
            if row.try_get::<_, String>(0).map_err(sql_error)? != self.binding_digest
                || row.try_get::<_, Vec<u8>>(1).map_err(sql_error)? != next
            {
                return Err(invalid("PG committed whole namespace readback differs"));
            }
        }
        self.poisoned = false;
        Ok(result)
    }
    fn check_binding(&self, binding: &str) -> Result<(), BftSignerErrorV3> {
        if binding != self.binding_digest {
            return Err(invalid("PG requested binding differs"));
        }
        Ok(())
    }
}
fn sql_error(error: postgres::Error) -> BftSignerErrorV3 {
    invalid(format!("PG namespace operation failed: {error}"))
}
fn validate_identity(namespace: &str, binding: &str) -> Result<(), BftSignerErrorV3> {
    if namespace.is_empty()
        || namespace.len() > 128
        || !namespace
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(invalid("PG namespace identifier invalid"));
    }
    state::validate_digest(binding)
}
fn check_lease(
    signer: &BftSignerBackendSnapshotV3,
    generation: u64,
    token: &str,
) -> Result<(), BftSignerErrorV3> {
    if signer.lease_generation != generation
        || token.is_empty()
        || signer.lease_token_digest != token
    {
        return Err(invalid("PG exact active lease differs"));
    }
    Ok(())
}
// These checks are storage transition guards, not a replacement for the
// authenticated service's independent H, signature and material verification.
fn check_retention_fence(
    expected: &BftFreshCoverageSnapshotV1,
    next: &BftFreshCoverageSnapshotV1,
) -> Result<(), BftSignerErrorV3> {
    let before = &expected.signer;
    let after = &next.signer;
    check_lease(before, after.lease_generation, &after.lease_token_digest)?;
    if before.protected_time_floor_ms != after.protected_time_floor_ms
        || before.state.binding_digest != after.state.binding_digest
        || expected.marker != next.marker
        || next.reserved_response_bytes <= expected.reserved_response_bytes
    {
        return Err(invalid("PG retention fence, floor, marker or spend differs"));
    }
    let head = after.state.private_retention.as_ref()
        .ok_or_else(|| invalid("PG retention requires covered State"))?;
    head.validate_supported_head(&after.state.binding_digest)?;
    if head.source != next.marker.source
        || head.retained_material_bytes
            .and_then(|bytes| bytes.checked_add(next.reserved_response_bytes))
            .is_none_or(|total| total > BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1)
    {
        return Err(invalid("PG retention source or namespace budget differs"));
    }
    Ok(())
}

fn check_receipt_transition(
    before: &BftSignerBackendSnapshotV3,
    after: &BftSignerBackendSnapshotV3,
    body: &BftPrivateRetentionReceiptBodyV1,
) -> Result<(), BftSignerErrorV3> {
    let state = if body.head.schema_version == 3 {
        before.state.with_first_ready_head(body.head.clone())?
    } else {
        before.state.with_original_coverage_head(body.head.clone())?
    };
    if state != after.state
        || body.before_revision != before.state.revision
        || body.before_state_digest != before.state.state_digest
        || body.after_revision != after.state.revision
        || body.after_state_digest != after.state.state_digest
        || body.original_lease_generation != before.lease_generation
    {
        return Err(invalid("PG retention receipt predecessor differs"));
    }
    Ok(())
}

fn check_fresh_transition(
    expected: &BftFreshCoverageSnapshotV1,
    next: &BftFreshCoverageSnapshotV1,
) -> Result<(), BftSignerErrorV3> {
    check_retention_fence(expected, next)?;
    let activation = next.original.as_ref()
        .ok_or_else(|| invalid("PG Fresh original receipt absent"))?;
    if let Some(original) = &expected.original {
        if activation != original || next.signer.state != expected.signer.state {
            return Err(invalid("PG Fresh retry changes original receipt or State"));
        }
    } else {
        if expected.marker.initial_state_digest != expected.signer.state.state_digest {
            return Err(invalid("PG Fresh marker predecessor differs"));
        }
        check_receipt_transition(&expected.signer, &next.signer, &activation.receipt.body)?;
    }
    Ok(())
}

fn check_reserve_transition(
    expected: &BftFirstReserveSnapshotV1,
    next: &BftFirstReserveSnapshotV1,
) -> Result<(), BftSignerErrorV3> {
    check_retention_fence(&expected.current, &next.current)?;
    let activation = expected.current.original.as_ref()
        .ok_or_else(|| invalid("PG Reserve requires original Fresh receipt"))?;
    if next.current.original != expected.current.original {
        return Err(invalid("PG Reserve changes original Fresh receipt"));
    }
    match (&expected.reservation, &next.reservation) {
        (Some(original), Some(reservation)) if original == reservation
            && expected.current.signer.state == next.current.signer.state => Ok(()),
        (None, Some(reservation)) => {
            let reference = reservation.input.descriptor()?;
            let (ledger, state) = first_reservation::reserve_body(
                &expected.current.signer.state,
                activation,
                &reference,
                &reservation.input,
                expected.current.signer.lease_generation,
            )?;
            if ledger.map != reservation.ledger.map
                || ledger.receipt.body != reservation.ledger.receipt.body
                || state != next.current.signer.state
            {
                return Err(invalid("PG first Reserve exact predecessor differs"));
            }
            check_receipt_transition(
                &expected.current.signer,
                &next.current.signer,
                &reservation.ledger.receipt.body,
            )
        }
        (None, None) if expected.current.signer.state == next.current.signer.state => Ok(()),
        _ => Err(invalid("PG Reserve replaces original input, receipt or State")),
    }
}

fn check_envelope_transition(
    expected: &BftOriginalRetentionTxnSnapshotV1,
    next: &BftOriginalRetentionTxnSnapshotV1,
) -> Result<(), BftSignerErrorV3> {
    expected.validate()?;
    next.validate()?;
    if next == expected {
        return Ok(());
    }
    if expected.original_operations.iter().any(|(id, original)| {
        next.original_operations.get(id) != Some(original)
    }) {
        return Err(invalid("PG original operation history changed"));
    }
    if next.original_operations == expected.original_operations {
        if next.current_retention != expected.current_retention {
            return Err(invalid("PG Ready requires the complete new original operation"));
        }
        return check_reserve_transition(&expected.current, &next.current);
    }
    let mut added = next.original_operations.iter()
        .filter(|(id, _)| !expected.original_operations.contains_key(*id));
    let (_, operation) = added.next().ok_or_else(|| invalid("PG original operation missing"))?;
    if added.next().is_some() {
        return Err(invalid("PG requires exactly one new original operation"));
    }
    if let Some(ready) = &next.current_retention {
        if expected.current_retention.is_some()
            || expected.current.reservation.is_none()
            || next.current.reservation != expected.current.reservation
            || next.current.current.original != expected.current.current.original
        {
            return Err(invalid("PG Ready replaces an original or current selection"));
        }
        check_retention_fence(&expected.current.current, &next.current.current)?;
        let before = &expected.current.current.signer;
        let after = &next.current.current.signer;
        let body = &ready.ledger.receipt.body;
        check_receipt_transition(before, after, body)?;
        if operation.operation_id != format!("first-ready:{}", body.operation_digest)
            || operation.original_lease_token_digest != before.lease_token_digest
            || operation.cumulative_reserved_response_bytes != next.current.current.reserved_response_bytes
        {
            return Err(invalid("PG Ready original operation fence or spend differs"));
        }
        // validate() binds the exact Ready and Reserve record, reference and
        // selection to their original receipts and the full current State.
        Ok(())
    } else {
        if expected.current_retention.is_some() {
            return Err(invalid("PG cannot downgrade Ready"));
        }
        check_reserve_transition(&expected.current, &next.current)?;
        if expected.prepare_single_atomic_operation(next.current.clone(), operation.clone())? != *next {
            return Err(invalid("PG complete original operation differs"));
        }
        Ok(())
    }
}

impl BftSignerBackendV3 for PostgresBftSignerBackendV3 {
    fn load_provisioned(
        &mut self,
        binding: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.check_binding(binding)?;
        self.run(|stored| Ok(stored.signer().clone()))
    }
    fn acquire_lease(
        &mut self,
        binding: &str,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.check_binding(binding)?;
        state::validate_digest(token)?;
        self.run(|stored| {
            let signer = stored.signer_mut();
            if signer.lease_generation != generation || !signer.lease_token_digest.is_empty() {
                return Ok(BftSignerAcquireOutcomeV3::Conflict(signer.clone()));
            }
            signer.lease_generation = generation
                .checked_add(1)
                .ok_or_else(|| invalid("PG lease generation overflow"))?;
            signer.lease_token_digest = token.into();
            Ok(BftSignerAcquireOutcomeV3::Acquired(signer.clone()))
        })
    }
    fn release_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.run(|stored| {
            let signer = stored.signer_mut();
            check_lease(signer, generation, token)?;
            signer.lease_generation = generation
                .checked_add(1)
                .ok_or_else(|| invalid("PG lease generation overflow"))?;
            signer.lease_token_digest.clear();
            Ok(signer.clone())
        })
    }
    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.run(|stored| {
            check_lease(stored.signer(), generation, token)?;
            Ok(stored.signer().clone())
        })
    }
    fn read_fresh_coverage_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftFreshCoverageSnapshotV1, BftSignerErrorV3> {
        self.run(|stored| {
            check_lease(stored.signer(), generation, token)?;
            Ok(stored.retention()?.current.current.clone())
        })
    }
    fn compare_and_commit_fresh_coverage(
        &mut self,
        expected: &BftFreshCoverageSnapshotV1,
        next: &BftFreshCoverageSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.run(|stored| {
            check_lease(stored.signer(), expected.signer.lease_generation, &expected.signer.lease_token_digest)?;
            let mut whole = stored.retention()?.clone();
            if whole.current.current != *expected {
                return Err(invalid("PG Fresh expected tuple differs"));
            }
            check_fresh_transition(expected, next)?;
            whole.current.current = next.clone();
            stored.replace_retention(whole)
        })
    }
    fn read_first_reserve_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftFirstReserveSnapshotV1, BftSignerErrorV3> {
        self.run(|stored| {
            check_lease(stored.signer(), generation, token)?;
            Ok(stored.retention()?.current.clone())
        })
    }
    fn compare_and_commit_first_reserve(
        &mut self,
        expected: &BftFirstReserveSnapshotV1,
        next: &BftFirstReserveSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.run(|stored| {
            check_lease(stored.signer(), expected.current.signer.lease_generation, &expected.current.signer.lease_token_digest)?;
            let mut whole = stored.retention()?.clone();
            if whole.current != *expected || next.reservation.is_none() {
                return Err(invalid("PG FirstReserve expected tuple differs"));
            }
            check_reserve_transition(expected, next)?;
            whole.current = next.clone();
            stored.replace_retention(whole)
        })
    }
    fn read_original_operation_envelope_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftOriginalRetentionTxnSnapshotV1, BftSignerErrorV3> {
        self.run(|stored| {
            check_lease(stored.signer(), generation, token)?;
            Ok(stored.retention()?.clone())
        })
    }
    fn compare_and_commit_original_operation_envelope(
        &mut self,
        expected: &BftOriginalRetentionTxnSnapshotV1,
        next: &BftOriginalRetentionTxnSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.run(|stored| {
            let signer = &expected.current.current.signer;
            check_lease(stored.signer(), signer.lease_generation, &signer.lease_token_digest)?;
            if stored.retention()? != expected {
                return Err(invalid("PG whole retention expected envelope differs"));
            }
            check_envelope_transition(expected, next)?;
            stored.replace_retention(next.clone())
        })
    }
    fn protected_time_ms(&mut self, _: u64, _: &str) -> Result<i64, BftSignerErrorV3> {
        Err(invalid("PG protected time source is unsupported"))
    }
    fn compare_and_swap(
        &mut self,
        generation: u64,
        token: &str,
        revision: u64,
        digest: &str,
        minimum_event_time_ms: Option<i64>,
        next: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        if minimum_event_time_ms.is_some() {
            return Err(invalid(
                "PG event transition requires unsupported protected time",
            ));
        }
        self.run(|stored| {
            if stored.has_retention() || next.private_retention.is_some() {
                return Err(invalid("PG retention requires whole-envelope transaction"));
            }
            let signer = stored.signer_mut();
            check_lease(signer, generation, token)?;
            if signer.state.revision != revision
                || signer.state.state_digest != digest
                || revision.checked_add(1) != Some(next.revision)
                || next.binding_digest != signer.state.binding_digest
            {
                return Err(invalid("PG state CAS predecessor differs"));
            }
            signer.state = next;
            Ok(signer.clone())
        })
    }
}
