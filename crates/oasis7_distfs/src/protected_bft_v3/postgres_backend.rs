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
