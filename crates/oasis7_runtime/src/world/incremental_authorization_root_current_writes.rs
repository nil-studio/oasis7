//! Closed top-level pending-write correspondence. A registration is only
//! retained after the selected source's actual Current join has succeeded.
use super::*;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_world_state::{
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_semantic_key::{
        SEMANTIC_KEY_DOMAIN, SemanticKeyBudget, SemanticKeyV1, hash_semantic_key_v1,
    },
    incremental_world_field_key::{
        WORLD_FIELD_KEY_MAX_BYTES, WorldFieldKeyV1, WorldFieldRecordKindV1,
    },
};
fn marker(
    key: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<CanonicalKeyV1, SourceError> {
    if key.len() > WORLD_FIELD_KEY_MAX_BYTES {
        return Err(invalid("authorization pending key bound").into());
    }
    let (header, n) = cbor::header(3, (key.len() * 2) as u64);
    let mut raw = [0u8; WORLD_FIELD_KEY_MAX_BYTES * 2 + 9];
    raw[..n].copy_from_slice(&header[..n]);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, b) in key.iter().enumerate() {
        raw[n + i * 2] = HEX[(b >> 4) as usize];
        raw[n + i * 2 + 1] = HEX[(b & 15) as usize];
    }
    Ok(CanonicalKeyV1::Text(tree::bytes(
        &raw[..n + key.len() * 2],
        common,
        MemoryClass::Ordinary,
    )?))
}
fn fingerprint(after: Option<[u8; 32]>) -> [u8; 33] {
    let mut raw = [0u8; 33];
    if let Some(hash) = after {
        raw[0] = 1;
        raw[1..].copy_from_slice(&hash);
    }
    raw
}
impl CurrentAuthorizationProjectionV1<'_> {
    pub(super) fn register_joined_key<M: MerkleNodeReader, K: SemanticKeyV1 + ?Sized>(
        &mut self,
        descriptor: FieldDescriptor,
        key: &K,
        key_bytes: usize,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        let bytes = SEMANTIC_KEY_DOMAIN
            .len()
            .checked_add(key_bytes)
            .ok_or_else(limited)?;
        common.charge(1, bytes as u64)?;
        let mut budget = SemanticKeyBudget {
            bytes_remaining: bytes as u64,
        };
        let semantic = {
            let (memory, _, stop) = common.parts();
            hash_semantic_key_v1(key, memory, MemoryClass::Ordinary, &mut budget, stop)
                .map_err(SourceError::Semantic)?
        };
        if semantic.encoded_bytes != bytes as u64 {
            return Err(invalid("authorization pending semantic source encoding").into());
        }
        let physical = WorldFieldKeyV1::new(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            Some(semantic.digest),
        )
        .map_err(|e| SourceError::from(ValueTreeError::Invalid(e)))?;
        let FieldDestination::State(namespace) = descriptor.destination else {
            return Err(invalid("authorization pending destination").into());
        };
        let common = std::cell::RefCell::new(common);
        let mut joined = None;
        tx.visit_pending_writes::<SourceError>(
            namespace,
            remaining,
            || {
                common.borrow_mut().consume_steps(1)?;
                Ok(())
            },
            |actual, _, after| {
                common
                    .borrow_mut()
                    .consume_read_bytes(actual.len() as u64)?;
                if actual == physical.as_bytes() {
                    joined = Some(after);
                }
                Ok(())
            },
        )?;
        let Some(after) = joined else {
            return Ok(());
        };
        let mut borrowed_common = common.borrow_mut();
        let common = &mut **borrowed_common;
        let key = marker(physical.as_bytes(), common)?;
        let value = tree::value(
            CanonicalValueV1::Atom(tree::bytes(
                &fingerprint(after),
                common,
                MemoryClass::Ordinary,
            )?),
            common,
            MemoryClass::Ordinary,
        )?;
        let (registered, _) =
            tree::put(&self.registered, key, value, common, MemoryClass::Ordinary)?;
        self.registered = Some(registered);
        Ok(())
    }
    pub(super) fn register_record<M: MerkleNodeReader>(
        &mut self,
        field: &str,
        key: &str,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        let (name, namespace) = match field {
            "grants" => ("capability_grants_v2", StateNamespaceV1::Authorization),
            "receipts" => (
                "capability_authorization_receipts",
                StateNamespaceV1::Authorization,
            ),
            "invocation_contexts" => (
                "capability_invocation_contexts",
                StateNamespaceV1::Authorization,
            ),
            "nonce_records" => ("capability_nonce_records", StateNamespaceV1::Authorization),
            "budget_accounts" => (
                "capability_budget_accounts",
                StateNamespaceV1::Authorization,
            ),
            "effect_receipt_links" => {
                ("capability_effect_receipt_links", StateNamespaceV1::Effects)
            }
            _ => return Err(invalid("authorization fixed registered source").into()),
        };
        let descriptor = FieldDescriptor {
            owner: FieldOwner::Runtime,
            name,
            shape: FieldShape::Map,
            destination: FieldDestination::State(namespace),
        };
        self.register_joined_key(
            descriptor,
            key,
            9usize.checked_add(key.len()).ok_or_else(limited)?,
            tx,
            remaining,
            common,
        )
    }
    pub(super) fn register_revocation<M: MerkleNodeReader>(
        &mut self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        self.register_joined_key(
            FieldDescriptor {
                owner: FieldOwner::Runtime,
                name: "capability_revocation_state",
                shape: FieldShape::Value,
                destination: FieldDestination::State(StateNamespaceV1::Authorization),
            },
            &(),
            1,
            tx,
            remaining,
            common,
        )
    }
}

impl CurrentAuthorizationProjectionV1<'_> {
    pub(super) fn register_authorization_root<M: MerkleNodeReader>(
        &mut self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        self.register_joined_key(oasis7_world_state::incremental_runtime_authorization_root::authorization_root_descriptor_v1(),
            &(),1,tx,remaining,common)
    }
    pub(super) fn validate_registered_writes<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        let common = std::cell::RefCell::new(common);
        for namespace in [StateNamespaceV1::Authorization, StateNamespaceV1::Effects] {
            tx.visit_pending_writes::<SourceError>(
                namespace,
                remaining,
                || {
                    common.borrow_mut().consume_steps(1)?;
                    Ok(())
                },
                |physical, _, after| {
                    let mut common = common.borrow_mut();
                    common.consume_read_bytes(physical.len() as u64)?;
                    let Ok(parsed) = WorldFieldKeyV1::from_bytes(physical) else {
                        return Ok(());
                    };
                    if parsed.owner != FieldOwner::Runtime {
                        return Ok(());
                    }
                    let included = matches!(
                        parsed.name,
                        "capability_grants_v2"
                            | "capability_authorization_receipts"
                            | "capability_invocation_contexts"
                            | "capability_nonce_records"
                            | "capability_budget_accounts"
                            | "capability_effect_receipt_links"
                            | "capability_revocation_state"
                            | "capability_authorization_root"
                    );
                    if !included {
                        return Ok(());
                    }
                    if parsed.namespace != namespace || parsed.kind != WorldFieldRecordKindV1::Entry
                    {
                        return Err(
                            invalid("authorization unregistered source metadata mutation").into(),
                        );
                    }
                    let key = marker(physical, &mut common)?;
                    let registered = tree::find(&self.registered, &key, &mut common)?
                        .ok_or_else(|| invalid("authorization unregistered source write"))?;
                    let CanonicalValueV1::Atom(bytes) = &**registered else {
                        return Err(invalid("authorization source registration domain").into());
                    };
                    common.consume_read_bytes(33)?;
                    if bytes.as_slice() != fingerprint(after) {
                        return Err(
                            invalid("authorization source changed after registration").into()
                        );
                    }
                    Ok(())
                },
            )?;
        }
        common.borrow_mut().consume_steps(0)?;
        Ok(())
    }
}
