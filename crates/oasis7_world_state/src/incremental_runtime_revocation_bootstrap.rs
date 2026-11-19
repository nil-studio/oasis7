//! Fixed sparse storage publication for the two Bootstrap revocation changes.
//! This grants no issuer, finality, agent, or Bootstrap business admission.
//! Runtime must retain its genuine checked source through publication.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1, write_semantic_key_v1},
    incremental_typed_record_codec::{Codec, prepare_typed_record_body_v1},
    incremental_typed_record_diff::{ChangeDecoder, point_typed_column_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_record_root::RecordRootRefV1,
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::{self, Write};

pub enum BootstrapRevocationChangeV1<'a> {
    AgentIdentity {
        agent_id: &'a str,
        identity: &'a crate::CapabilityAgentIdentity,
    },
    Authority {
        record: &'a crate::CapabilityAuthorityRecord,
        proof: &'a crate::CapabilityAuthorityFinalityProof,
    },
}
pub enum BootstrapRevocationWriteKindV1 {
    AgentIdentity,
    Authority,
}
pub struct BootstrapRevocationWriteReceiptV1 {
    before: StateRootV1,
    after: StateRootV1,
    key: String,
    kind: BootstrapRevocationWriteKindV1,
    guard: PreparedGuard,
}
impl BootstrapRevocationWriteReceiptV1 {
    pub fn before_body(&self) -> &StateRootV1 {
        &self.before
    }
    pub fn after_body(&self) -> &StateRootV1 {
        &self.after
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn kind(&self) -> &BootstrapRevocationWriteKindV1 {
        &self.kind
    }
    pub fn validate_transaction<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_transaction_identity(&self.guard)?;
        Ok(())
    }
}
struct Count(usize);
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(b.len())
            .ok_or_else(|| io::Error::other("bootstrap semantic key size"))?;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn key_bytes<K: SemanticKeyV1 + ?Sized>(
    key: &K,
    max: usize,
    write: &mut MemberRecordWriteContext<'_, impl ValueNodeWriter, impl FnMut() -> bool>,
) -> Result<ReservedValue<Vec<u8>>, MemberRecordError> {
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let work = max
        .checked_add(128)
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(invalid)?;
    write.key_work.bytes_remaining = write
        .key_work
        .bytes_remaining
        .checked_sub(work as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "bootstrap key encoding work",
        ))?;
    let mut count = Count(0);
    write_semantic_key_v1(key, &mut count).map_err(|_| invalid())?;
    let fee = write
        .memory
        .reserve(
            count
                .0
                .checked_add(std::mem::size_of::<Vec<u8>>())
                .ok_or_else(invalid)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(count.0);
    if bytes.capacity() > count.0 {
        return Err(invalid().into());
    }
    write_semantic_key_v1(key, &mut bytes).map_err(|_| invalid())?;
    if bytes.len() != count.0 || bytes.capacity() > count.0 {
        return Err(invalid().into());
    }
    Ok(fee.retain(bytes))
}
enum Payload<'a> {
    Scalar(MemberPayloadRefV1<'a>),
    Tree(RecordRootRefV1),
}
struct Edit<'a> {
    first: ReservedValue<Vec<u8>>,
    second: Option<ReservedValue<Vec<u8>>>,
    expected: Option<[u8; 32]>,
    payload: Payload<'a>,
}
struct Map {
    root: StateRootV1,
    count: u64,
    length_id: [u8; 32],
    added: u64,
    name: &'static str,
}
fn point<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    name: &str,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>, ValueTreeError> {
    let profile = profile();
    let trees_remaining = work.max_retained_trees;
    point_typed_column_v1(
        &profile,
        body,
        name,
        &mut ChangeDecoder {
            ctx: read,
            scan,
            work,
            profile,
            trees_remaining,
            tree_depth: 0,
        },
        8192,
    )
}
fn value_id<
    K: SemanticKeyV1 + ?Sized,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    key: &K,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    let profile = profile();
    let mut records = read.records();
    let key = records.key(profile.field, WorldFieldRecordKindV1::Entry, key)?;
    crate::incremental_record_tree::lookup(root, &key.0, &mut records, profile.labels)
}
fn map<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    name: &'static str,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<Map, ValueTreeError> {
    let row = point(body, name, read, scan, work)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    let length = point(root, "_length", read, scan, work)?;
    let ColumnCellV1::U64(count) = &length.cell else {
        return Err(invalid());
    };
    let count = *count;
    if count.checked_add(2) != Some(root.leaf_count) {
        return Err(invalid());
    }
    let length_id = value_id(root, "_length", read)?.ok_or_else(invalid)?;
    Ok(Map {
        root: root.clone(),
        count,
        length_id,
        added: 0,
        name,
    })
}
fn member<
    'a,
    K: SemanticKeyV1 + ?Sized,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
>(
    map: &mut Map,
    key: &K,
    key_limit: usize,
    payload: Payload<'a>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
) -> Result<Edit<'a>, WorldFieldOverlayErrorV1> {
    let expected = value_id(&map.root, key, read)?;
    if expected.is_none() {
        map.added = map.added.checked_add(1).ok_or_else(invalid)?;
    }
    Ok(Edit {
        first: key_bytes(map.name, map.name.len(), write)?,
        second: Some(key_bytes(key, key_limit, write)?),
        expected,
        payload,
    })
}
fn tree<T: Codec, W: ValueNodeWriter + ValueNodeReader + StatePageWriter, D: FnMut() -> bool>(
    value: &T,
    id: &str,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut RevocationWriteWorkV1,
) -> Result<RecordRootRefV1, WorldFieldOverlayErrorV1> {
    let root = prepare_typed_record_body_v1(value, &profile(), write, work)?;
    let owner = hash_semantic_key_v1(
        &("entry", id),
        write.memory,
        MemoryClass::Ordinary,
        write.key_work,
        &mut write.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    Ok(
        RecordRootRefV1::new(profile().schema, owner.digest, (*root).clone())
            .map_err(|_| invalid())?,
    )
}
fn length<'a, W: ValueNodeWriter, D: FnMut() -> bool>(
    map: &Map,
    write: &mut MemberRecordWriteContext<'_, W, D>,
) -> Result<Option<Edit<'a>>, WorldFieldOverlayErrorV1> {
    if map.added == 0 {
        return Ok(None);
    }
    let count = map.count.checked_add(map.added).ok_or_else(invalid)?;
    Ok(Some(Edit {
        first: key_bytes(map.name, map.name.len(), write)?,
        second: Some(key_bytes("_length", 7, write)?),
        expected: Some(map.length_id),
        payload: Payload::Scalar(MemberPayloadRefV1::U64(count)),
    }))
}
#[allow(clippy::too_many_arguments)]
pub fn publish_current_bootstrap_revocation_v1<M, R, E, C, P, V, ER, CR, W, D>(
    expected_body: &StateRootV1,
    change: BootstrapRevocationChangeV1<'_>,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &mut RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    write_work: &mut RevocationWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<BootstrapRevocationWriteReceiptV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    D: FnMut() -> bool,
{
    let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        overlay.transaction.validate_guard(guard, live)?;
        current.check(overlay.transaction, expected_body, write.memory)?;
        // Authenticate the actual outer body too; a stale private token never
        // permits overwriting another same-transaction revocation update.
        let actual = {
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            current_identity::current_body(
                overlay.transaction,
                &mut records,
                &mut overlay.take_state_error,
            )?
        };
        if &actual != expected_body {
            return Err(invalid().into());
        }
        let (count, key, kind) = match &change {
            BootstrapRevocationChangeV1::AgentIdentity { agent_id, .. } => {
                (2, *agent_id, BootstrapRevocationWriteKindV1::AgentIdentity)
            }
            BootstrapRevocationChangeV1::Authority { record, .. } => (
                record
                    .revoked_grant_ids
                    .len()
                    .checked_add(record.superseded_by.len())
                    .and_then(|n| n.checked_add(9))
                    .ok_or_else(invalid)?,
                record.issuer_id.as_str(),
                BootstrapRevocationWriteKindV1::Authority,
            ),
        };
        let bytes = count
            .checked_mul(
                std::mem::size_of::<Edit<'_>>()
                    + std::mem::size_of::<RuntimeRevocationPathEditV1<'_>>()
                    + 3 * std::mem::size_of::<&[u8]>()
                    + 2048,
            )
            .ok_or_else(invalid)?;
        let _scratch = write
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let receipt_fee = write
            .memory
            .reserve(
                std::mem::size_of::<BootstrapRevocationWriteReceiptV1>() + key.len() + 1536,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        write.key_work.bytes_remaining = write
            .key_work
            .bytes_remaining
            .checked_sub(key.len() as u64 + 128)
            .ok_or(ValueTreeError::ResourceLimited("bootstrap receipt copy"))?;
        let key = key.to_owned();
        let before = current.body().clone();
        let mut edits = Vec::with_capacity(count);
        if edits.capacity() > count {
            return Err(invalid().into());
        }
        match change {
            BootstrapRevocationChangeV1::AgentIdentity { agent_id, identity } => {
                let mut identities = map(current.body(), "agent_identities", read, scan, work)?;
                let after = tree(identity, agent_id, write, write_work)?;
                edits.push(member(
                    &mut identities,
                    &("entry", agent_id),
                    agent_id.len() + 64,
                    Payload::Tree(after),
                    read,
                    write,
                )?);
                if let Some(edit) = length(&identities, write)? {
                    edits.push(edit)
                }
            }
            BootstrapRevocationChangeV1::Authority { record, proof } => {
                let mut records = map(current.body(), "authority_records", read, scan, work)?;
                let mut proofs = map(
                    current.body(),
                    "authority_finality_proofs",
                    read,
                    scan,
                    work,
                )?;
                let mut revoked = map(current.body(), "revoked_grant_ids", read, scan, work)?;
                let mut superseded = map(current.body(), "superseded_by", read, scan, work)?;
                let after = tree(record, &record.issuer_id, write, write_work)?;
                edits.push(member(
                    &mut records,
                    &("entry", record.issuer_id.as_str()),
                    record.issuer_id.len() + 64,
                    Payload::Tree(after),
                    read,
                    write,
                )?);
                let after = tree(proof, &record.issuer_id, write, write_work)?;
                edits.push(member(
                    &mut proofs,
                    &("entry", record.issuer_id.as_str()),
                    record.issuer_id.len() + 64,
                    Payload::Tree(after),
                    read,
                    write,
                )?);
                for id in &record.revoked_grant_ids {
                    write_work.spend(&mut write.cancelled)?;
                    edits.push(member(
                        &mut revoked,
                        &("entry", id.as_str()),
                        id.len() + 64,
                        Payload::Scalar(MemberPayloadRefV1::Bool(true)),
                        read,
                        write,
                    )?);
                }
                for (id, value) in &record.superseded_by {
                    write_work.spend(&mut write.cancelled)?;
                    edits.push(member(
                        &mut superseded,
                        &("entry", id.as_str()),
                        id.len() + 64,
                        Payload::Scalar(MemberPayloadRefV1::Text(value)),
                        read,
                        write,
                    )?);
                }
                for value in [&records, &proofs, &revoked, &superseded] {
                    if let Some(edit) = length(value, write)? {
                        edits.push(edit)
                    }
                }
                let epoch = point(current.body(), "epoch", read, scan, work)?;
                let ColumnCellV1::U64(epoch) = &epoch.cell else {
                    return Err(invalid().into());
                };
                let epoch = *epoch;
                edits.push(Edit {
                    first: key_bytes("epoch", 5, write)?,
                    second: None,
                    expected: value_id(current.body(), "epoch", read)?,
                    payload: Payload::Scalar(MemberPayloadRefV1::U64(
                        epoch.max(record.revocation_epoch),
                    )),
                });
                edits.push(Edit {
                    first: key_bytes("finalized_receipt_id", 20, write)?,
                    second: None,
                    expected: value_id(current.body(), "finalized_receipt_id", read)?,
                    payload: Payload::Scalar(MemberPayloadRefV1::Text(
                        &record.finalized_receipt_id,
                    )),
                });
            }
        }
        let preparation_steps = edits.len().checked_mul(2).ok_or_else(invalid)?;
        write_work.cells_remaining = write_work
            .cells_remaining
            .checked_sub(u64::try_from(preparation_steps).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "bootstrap path preparation",
            ))?;
        if (write.cancelled)() || (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let paths: Vec<[&[u8]; 2]> = edits
            .iter()
            .map(|edit| {
                [
                    edit.first.as_slice(),
                    edit.second.as_ref().map_or(&[][..], |key| key.as_slice()),
                ]
            })
            .collect();
        let typed: Vec<RuntimeRevocationPathEditV1<'_>> = edits
            .iter()
            .zip(&paths)
            .map(|(edit, path)| RuntimeRevocationPathEditV1 {
                path: if edit.second.is_some() {
                    path
                } else {
                    &path[..1]
                },
                expected: edit.expected,
                after: Some(match &edit.payload {
                    Payload::Scalar(value) => *value,
                    Payload::Tree(root) => MemberPayloadRefV1::RecordRoot(root),
                }),
            })
            .collect();
        let fresh = overlay.transaction.guard();
        patch_runtime_revocation_overlay_v1(
            qualification,
            current,
            &typed,
            overlay,
            read,
            scan,
            work,
            write,
            write_work,
            &fresh,
            live,
        )?;
        Ok(receipt_fee.retain(BootstrapRevocationWriteReceiptV1 {
            before,
            after: current.body().clone(),
            key,
            kind,
            guard: overlay.transaction.guard(),
        }))
    }));
    match attempted {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                overlay.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            Err(error)
        }
        Err(panic) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                overlay.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(panic)
        }
    }
}
