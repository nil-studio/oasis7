//! Continuous Common admission for the original business descriptor/codec.
//! All outputs remain DATA; signed-root/actual-seal sources are independent.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    business_index::{
        PaidBusinessIndexErrorV1, PaidBusinessIndexNodeReaderV1, get_business_index_leaf_bounded_v1,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    io::{self, Write},
    mem::size_of,
};
pub struct PaidFormalBusinessErrorV1 {
    error: FormalError,
    protocol: Option<PaidBusinessIndexErrorV1>,
    body: Option<oasis7_world_state::incremental_agent_mutation::BodyMutationError>,
    _phase: Option<MemoryReservation>,
}
impl PaidFormalBusinessErrorV1 {
    pub fn body_error(
        &self,
    ) -> Option<&oasis7_world_state::incremental_agent_mutation::BodyMutationError> {
        self.body.as_ref()
    }
    pub fn error(&self) -> &FormalError {
        self.protocol.as_ref().map_or(&self.error, |p| p.error())
    }
}
impl std::fmt::Debug for PaidFormalBusinessErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(e) = &self.body {
            std::fmt::Debug::fmt(e, f)
        } else {
            std::fmt::Debug::fmt(self.error(), f)
        }
    }
}
impl std::fmt::Display for PaidFormalBusinessErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(e) = &self.body {
            std::fmt::Display::fmt(e, f)
        } else {
            std::fmt::Display::fmt(self.error(), f)
        }
    }
}
impl std::error::Error for PaidFormalBusinessErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Some(e) = &self.body {
            Some(e)
        } else if let Some(e) = &self.protocol {
            Some(e)
        } else {
            Some(&self.error)
        }
    }
}
fn fail(error: FormalError, phase: Option<MemoryReservation>) -> PaidFormalBusinessErrorV1 {
    PaidFormalBusinessErrorV1 {
        error,
        protocol: None,
        body: None,
        _phase: phase,
    }
}
fn protocol(error: PaidBusinessIndexErrorV1) -> PaidFormalBusinessErrorV1 {
    PaidFormalBusinessErrorV1 {
        error: FormalError::Invalid("paid business index".into()),
        protocol: Some(error),
        body: None,
        _phase: None,
    }
}
fn invalid(note: impl Into<String>) -> FormalError {
    FormalError::Invalid(note.into())
}
fn overflow() -> FormalError {
    invalid("paid formal business layout overflow")
}
struct Sink<'a, 'c, W> {
    common: &'a mut FormalBoundedContextV1<'c>,
    out: W,
    count: usize,
    first: Option<FormalError>,
}
impl<W: Write> Write for Sink<'_, '_, W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        for chunk in b.chunks(8192) {
            let next = match self.count.checked_add(chunk.len()) {
                Some(n) => n,
                None => {
                    self.first = Some(overflow());
                    return Err(io::ErrorKind::Other.into());
                }
            };
            if next > MAX_TREE_BYTES {
                self.first = Some(invalid("formal result tree byte bound"));
                return Err(io::ErrorKind::Other.into());
            }
            if let Err(e) = self.common.charge(2, (chunk.len() as u64) * 2) {
                self.first = Some(e);
                return Err(io::ErrorKind::Other.into());
            }
            if let Err(e) = self.out.write_all(chunk) {
                self.first = Some(FormalError::Io(e.to_string()));
                return Err(e);
            }
            self.count = next;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
fn encode<T: Serialize>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, PaidFormalBusinessErrorV1> {
    let mut count = Sink {
        common,
        out: io::sink(),
        count: 0,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut count, value);
    if let Some(e) = count.first {
        return Err(fail(e, None));
    }
    result.map_err(|e| fail(invalid(e.to_string()), None))?;
    let n = count.count;
    let bytes = n
        .checked_add(size_of::<Vec<u8>>())
        .ok_or_else(|| fail(overflow(), None))?;
    let phase = count
        .common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| fail(invalid("formal business output graph admission"), None))?;
    let mut raw = Vec::new();
    if raw.try_reserve_exact(n).is_err() || raw.capacity() > n {
        return Err(fail(
            invalid("formal business output capacity"),
            Some(phase),
        ));
    }
    let mut out = Sink {
        common: count.common,
        out: &mut raw,
        count: 0,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut out, value);
    if let Some(e) = out.first {
        return Err(fail(e, Some(phase)));
    }
    if let Err(e) = result {
        return Err(fail(invalid(e.to_string()), Some(phase)));
    }
    if out.count != n {
        return Err(fail(invalid("formal business output changed"), Some(phase)));
    }
    Ok(phase.retain(raw))
}
pub struct PaidFormalBusinessDescriptorV1 {
    tree: FormalBusinessResultTreeV1,
    core: WorkIdentityV1,
    phase: MemoryReservation,
}
impl PaidFormalBusinessDescriptorV1 {
    pub fn tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.tree
    }
    pub fn metadata(&self) -> &FormalBusinessCommitMetadataV1 {
        &self.tree.metadata
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.phase.uses_budget(memory)
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self.uses_budget(common.memory()) {
            return Err(invalid("formal business descriptor foreign context"));
        }
        Ok(())
    }
    pub fn root_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<String>, PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        paid_root(&self.tree, common)
    }
    pub fn descriptor_cbor_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        encode(&self.tree, common)
    }
    pub fn validate_record_bounded_v1(
        &self,
        record: &CanonicalRecordV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        record_matches(&self.tree, record, common)
    }
    pub fn validate_metadata_leaf_bounded_v1(
        &self,
        reader: &impl PaidBusinessIndexNodeReaderV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        let leaf = get_business_index_leaf_bounded_v1(
            &self.tree.index_root,
            &metadata_key_digest(),
            reader,
            common,
        )
        .map_err(protocol)?
        .ok_or_else(|| {
            fail(
                invalid("formal result predecessor metadata leaf missing"),
                None,
            )
        })?;
        let bytes = leaf
            .leaf()
            .canonical_value
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(65536))
            .ok_or_else(|| fail(overflow(), None))?;
        let phase = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|_| fail(invalid("business metadata leaf graph"), None))?;
        common
            .charge(bytes as u64, leaf.leaf().canonical_value.len() as u64)
            .map_err(|e| fail(e, None))?;
        if leaf.leaf().kind != BusinessIndexLeafKindV1::CommitMetadata {
            return Err(fail(
                invalid("formal result predecessor metadata leaf mismatch"),
                Some(phase),
            ));
        }
        match decode_index_leaf(leaf.leaf()) {
            Ok(TreeLeafV1::CommitMetadata(meta)) if meta == self.tree.metadata => Ok(()),
            Ok(_) => Err(fail(
                invalid("formal result predecessor metadata leaf mismatch"),
                Some(phase),
            )),
            Err(e) => Err(fail(invalid(e), Some(phase))),
        }
    }
}
fn paid_root(
    tree: &FormalBusinessResultTreeV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, PaidFormalBusinessErrorV1> {
    common.consume_steps(1).map_err(|e| fail(e, None))?;
    let bytes = serde_size(tree.metadata(), common)?;
    common
        .charge(bytes as u64, bytes as u64)
        .map_err(|e| fail(e, None))?;
    let mut first = None;
    let mut output = None;
    let result=tree.validate_with_root(|root|match oasis7_formal_protocol::controlled_authority::formal::business_index::canonical_business_index_root_bounded_v1(root,common){Ok(v)=>{output=Some(v);Ok(())},Err(e)=>{first=Some(protocol(e));Err(INDEX_FAILED.into())}});
    if let Some(e) = first {
        return Err(e);
    }
    result.map_err(|e| fail(invalid(e), None))?;
    output.ok_or_else(|| fail(invalid("business root codec output missing"), None))
}
fn serde_size<T: Serialize>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, PaidFormalBusinessErrorV1> {
    let mut out = Sink {
        common,
        out: io::sink(),
        count: 0,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut out, value);
    if let Some(e) = out.first {
        return Err(fail(e, None));
    }
    result.map_err(|e| fail(invalid(e.to_string()), None))?;
    Ok(out.count)
}

fn record_matches(
    tree: &FormalBusinessResultTreeV1,
    record: &CanonicalRecordV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), PaidFormalBusinessErrorV1> {
    let root = paid_root(tree, common)?;
    let m = &tree.metadata;
    let bytes = record.world.world_id.len()
        + record.world.chain_id.len()
        + record.world.genesis_digest.len()
        + record.after_state_root.len()
        + record.governing_manifest_digest.len()
        + 1024;
    common
        .charge(bytes as u64, bytes as u64)
        .map_err(|e| fail(e, None))?;
    if root.as_str() != record.result_root
        || m.world_id != record.world.world_id
        || m.chain_id != record.world.chain_id
        || m.genesis_digest != record.world.genesis_digest
        || m.formal_position != record.position
        || m.after_state_root != record.after_state_root
        || m.governing_manifest_digest != record.governing_manifest_digest
    {
        return Err(fail(
            invalid("formal business descriptor differs from committed World/result root"),
            None,
        ));
    }
    Ok(())
}
pub fn decode_formal_business_descriptor_bounded_v1(
    raw: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidFormalBusinessDescriptorV1, PaidFormalBusinessErrorV1> {
    common.consume_steps(1).map_err(|e| fail(e, None))?;
    if raw.len() > MAX_TREE_BYTES {
        return Err(fail(invalid("formal result tree byte bound"), None));
    }
    let bytes = raw
        .len()
        .checked_mul(128)
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(|| fail(overflow(), None))?;
    let phase = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| fail(invalid("formal business descriptor complete graph"), None))?;
    common
        .charge(bytes as u64, raw.len() as u64)
        .map_err(|e| fail(e, None))?;
    let mut first = None;
    let parsed = FormalBusinessResultTreeV1::decode_cbor_with_root(raw, |root| {
        match oasis7_formal_protocol::controlled_authority::formal::business_index::canonical_business_index_root_bounded_v1(root,common){Ok(_)=>Ok(()),Err(e)=>{first=Some(protocol(e));Err(INDEX_FAILED.into())}}
    });
    if let Some(mut e) = first {
        e._phase = Some(phase);
        return Err(e);
    }
    let tree = match parsed {
        Ok(v) => v,
        Err(e) => return Err(fail(invalid(e), Some(phase))),
    };
    Ok(PaidFormalBusinessDescriptorV1 {
        tree,
        core: common.work().identity(),
        phase,
    })
}

fn body(
    error: oasis7_world_state::incremental_agent_mutation::BodyMutationError,
) -> PaidFormalBusinessErrorV1 {
    PaidFormalBusinessErrorV1 {
        error: invalid("actual Runtime business source"),
        protocol: None,
        body: Some(error),
        _phase: None,
    }
}
/// All source and output graphs stay owned through the caller's package writes.
/// It is a provisional commitment; Qualified publication is independent.
pub struct PaidFormalBusinessResultUpdateV1<'a>{
 source:oasis7_runtime::PaidBorrowedNormalBusinessSourceV1<'a>,
 parent_proof:&'a oasis7_distfs::controlled_authority::formal::endpoint::PaidHostQualifiedParentProofV1,
 tree:FormalBusinessResultTreeV1,
 delta:oasis7_formal_protocol::controlled_authority::formal::business_index::PaidBusinessIndexDeltaV1,
 core:WorkIdentityV1,phase:MemoryReservation,
}
impl PaidFormalBusinessResultUpdateV1<'_> {
    pub fn tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.tree
    }
    pub fn delta(&self) -> &BusinessIndexDeltaV1 {
        self.delta.delta()
    }
    pub fn actual_source(&self) -> &oasis7_runtime::ActualCurrentBusinessDeltaV1 {
        self.source.delta()
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self.phase.uses_budget(common.memory()) {
            return Err(invalid("formal business update foreign Core/pool"));
        }
        self.parent_proof.validate_context(common)?;
        self.delta.validate_context(common)
    }
    pub fn root_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<String>, PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        paid_root(&self.tree, common)
    }
    pub fn descriptor_cbor_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        encode(&self.tree, common)
    }
    pub fn delta_cbor_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        encode(self.delta.delta(), common)
    }
    pub fn validate_record_bounded_v1(
        &self,
        record: &CanonicalRecordV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), PaidFormalBusinessErrorV1> {
        self.validate_context(common).map_err(|e| fail(e, None))?;
        record_matches(&self.tree, record, common)
    }
}
#[path = "formal_result_paid_canonical.rs"]
mod canonical;
struct CurrentFacts<'a, 'b, 'c, 'd> {
    delta: &'a oasis7_runtime::ActualCurrentBusinessDeltaV1,
    state: &'a std::cell::RefCell<BuildState<'b, 'c>>,
    canonical: &'a std::cell::RefCell<&'d mut oasis7_wasm_abi::CanonicalCborWorkV1>,
}
impl kernel::CanonicalOps for CurrentFacts<'_, '_, '_, '_> {
    fn canonical<T: Serialize>(&mut self, value: &T) -> Result<Vec<u8>, String> {
        canonical::encode(value, self.state, self.canonical)
    }
    fn hash_work(&mut self, bytes: usize) -> Result<(), String> {
        let mut s = self.state.borrow_mut();
        match s.common.charge(bytes as u64 + 1, bytes as u64 + 64) {
            Ok(()) => Ok(()),
            Err(e) => {
                if s.first.is_none() {
                    s.first = Some(fail(e, None))
                };
                Err(INDEX_FAILED.into())
            }
        }
    }
}
impl kernel::Facts for CurrentFacts<'_, '_, '_, '_> {
    fn key_digest(&mut self, result: &CanonicalIntentResultV1) -> Result<String, String> {
        // Runtime's private actual-delta producer already checked this original
        // digest against this exact decoded record. Reuse that retained source
        // instead of performing a second unmetered canonical encode.
        let key = self
            .delta
            .results()
            .iter()
            .find(|held| std::ptr::eq(held.result(), result))
            .map(|held| held.key())
            .ok_or_else(|| "formal result candidate is absent from the World ledger".to_owned())?;
        let mut state = self.state.borrow_mut();
        if let Err(e) = state.common.charge(1, key.len() as u64) {
            if state.first.is_none() {
                state.first = Some(fail(e, None))
            }
            return Err(INDEX_FAILED.into());
        }
        Ok(key.to_owned())
    }
    fn has_world_results(&self) -> bool {
        self.delta.before_len() != 0
    }
    fn check_result(&mut self, key: &str, result: &CanonicalIntentResultV1) -> Result<(), String> {
        if self
            .delta
            .results()
            .iter()
            .any(|held| held.key() == key && std::ptr::eq(held.result(), result))
        {
            Ok(())
        } else {
            Err("formal result candidate differs from the World ledger".into())
        }
    }
    fn scope(
        &mut self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<String, String> {
        self.delta
            .results()
            .iter()
            .find(|held| std::ptr::eq(&held.result().request, request))
            .map(|held| held.visibility_scope().to_owned())
            .ok_or_else(|| "formal result candidate is absent from the World ledger".into())
    }
}
struct BuildState<'a, 'c> {
    common: &'a mut FormalBoundedContextV1<'c>,
    first: Option<PaidFormalBusinessErrorV1>,
    leaf_phase: Option<MemoryReservation>,
}
impl BuildState<'_, '_> {
    fn admit_leaf(&mut self, bytes: usize) -> Result<(), PaidFormalBusinessErrorV1> {
        let held = self.leaf_phase.as_ref().map_or(0, MemoryReservation::bytes);
        if bytes > held {
            let mut fee = self
                .common
                .memory()
                .reserve(bytes - held, MemoryClass::Ordinary)
                .map_err(|_| fail(invalid("formal business original leaf decode graph"), None))?;
            match &mut self.leaf_phase {
                Some(held) => held.merge(&mut fee).map_err(|e| fail(invalid(e), None))?,
                None => self.leaf_phase = Some(fee),
            }
        }
        Ok(())
    }
}

struct PaidIndex<'a, 'b, 'c, R> {
    state: &'a std::cell::RefCell<BuildState<'b, 'c>>,
    reader: &'a R,
}
const INDEX_FAILED: &str = "paid formal business index operation failed";
impl<R: PaidBusinessIndexNodeReaderV1> kernel::Index for PaidIndex<'_, '_, '_, R> {
    type Delta=oasis7_formal_protocol::controlled_authority::formal::business_index::PaidBusinessIndexDeltaV1;
    fn empty_root(&mut self) -> Result<BusinessIndexRootV1, String> {
        let mut s = self.state.borrow_mut();
        if s.first.is_some() {
            return Err(INDEX_FAILED.into());
        }
        let root=match oasis7_formal_protocol::controlled_authority::formal::business_index::empty_business_index_root_bounded_v1(s.common){Ok(v)=>v,Err(e)=>{s.first=Some(protocol(e));return Err(INDEX_FAILED.into())}};
        if let Err(e) = s.common.charge(1, 64) {
            s.first = Some(fail(e, None));
            return Err(INDEX_FAILED.into());
        }
        Ok((*root).clone())
    }
    fn validate_tree(&mut self, tree: &FormalBusinessResultTreeV1) -> Result<(), String> {
        let mut s = self.state.borrow_mut();
        if s.first.is_some() {
            return Err(INDEX_FAILED.into());
        }
        match paid_root(tree, s.common) {
            Ok(_) => Ok(()),
            Err(e) => {
                s.first = Some(e);
                Err(INDEX_FAILED.into())
            }
        }
    }
    fn get(
        &mut self,
        base: &BusinessIndexRootV1,
        key: &str,
    ) -> Result<Option<BusinessIndexLeafV1>, String> {
        let mut s = self.state.borrow_mut();
        if s.first.is_some() {
            return Err(INDEX_FAILED.into());
        }
        let value = match get_business_index_leaf_bounded_v1(base, key, self.reader, s.common) {
            Ok(v) => v,
            Err(e) => {
                s.first = Some(protocol(e));
                return Err(INDEX_FAILED.into());
            }
        };
        let Some(value) = value else { return Ok(None) };
        let bytes = match value
            .leaf()
            .canonical_value
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(128 * 1024))
        {
            Some(n) => n,
            None => {
                s.first = Some(fail(overflow(), None));
                return Err(INDEX_FAILED.into());
            }
        };
        if let Err(e) = s.common.charge(bytes as u64, bytes as u64) {
            s.first = Some(fail(e, None));
            return Err(INDEX_FAILED.into());
        }
        if let Err(e) = s.admit_leaf(bytes) {
            s.first = Some(e);
            return Err(INDEX_FAILED.into());
        }
        // This actual-size reusable phase remains held through the shared kernel
        // after this temporary reader owner drops; no rawlen/JSON graph gap.
        Ok(Some(value.leaf().clone()))
    }
    fn build(
        &mut self,
        base: BusinessIndexRootV1,
        updates: Vec<BusinessIndexLeafV1>,
    ) -> Result<Self::Delta, String> {
        let mut s = self.state.borrow_mut();
        if s.first.is_some() {
            return Err(INDEX_FAILED.into());
        }
        match oasis7_formal_protocol::controlled_authority::formal::business_index::build_business_index_v2_bounded_v1(base,updates,self.reader,s.common){Ok(v)=>Ok(v),Err(e)=>{s.first=Some(protocol(e));Err(INDEX_FAILED.into())}}
    }
    fn new_root(value: &Self::Delta) -> &BusinessIndexRootV1 {
        &value.delta().new
    }
}
fn metadata_from_current_source(
    source: &oasis7_runtime::PaidBorrowedNormalBusinessSourceV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<FormalBusinessCommitMetadataV1, PaidFormalBusinessErrorV1> {
    source.validate_context(common).map_err(body)?;
    let parent = source.actual_parent().record();
    let m = source.metadata();
    let world = &parent.world;
    let bytes = world
        .world_id
        .len()
        .checked_add(world.chain_id.len())
        .and_then(|n| n.checked_add(world.genesis_digest.len()))
        .and_then(|n| {
            n.checked_add(
                m.provider_world_id().len()
                    + m.branch_id().len()
                    + m.finality_status().len()
                    + m.finality_block_hash().map_or(0, str::len),
            )
        })
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(|| fail(overflow(), None))?;
    common
        .charge(bytes as u64, bytes as u64)
        .map_err(|e| fail(e, None))?;
    let binding = m
        .runtime_binding_for_state_hash_bounded_v1(source.raw_state_hash(), common, canonical)
        .map_err(body)?;
    let governing = binding.runtime_manifest_hash.to_string();
    let governing = governing
        .strip_prefix("blake3:")
        .ok_or_else(|| fail(invalid("formal result governing digest encoding"), None))?;
    let after = hex::encode(
        source
            .after_roots()
            .digest()
            .map_err(|e| fail(invalid(e.to_string()), None))?,
    );
    let metadata = FormalBusinessCommitMetadataV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        formal_position: parent
            .position
            .checked_add(1)
            .ok_or_else(|| fail(overflow(), None))?,
        world_id: world.world_id.clone(),
        chain_id: world.chain_id.clone(),
        genesis_digest: world.genesis_digest.clone(),
        after_state_root: after,
        provider_world_id: binding.world_id.clone(),
        branch_id: binding.branch_id.clone(),
        finality_epoch: binding.finality_epoch,
        finality_status: binding.finality_status.clone(),
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string)
            .map(|s| s.strip_prefix("blake3:").unwrap_or(&s).to_owned()),
        reorg_generation: binding.reorg_epoch,
        governing_manifest_digest: governing.to_owned(),
        authority_generation: m.authority_generation(),
        permission_generation: m.permission_generation(),
        event_id_era: m.event_id_era(),
        journal_retention_floor: m.journal_retention_floor(),
        journal_high_watermark: m.journal_high_watermark(),
    };
    metadata
        .validate()
        .map_err(|e| fail(invalid(e.to_string()), None))?;
    Ok(metadata)
}
/// Genuine sealed Current plus a paid real accepted descriptor/proof. The
/// descriptor is authenticated against the actual parent's signed result_root;
/// neither a caller DTO nor its claimed root can grant parent qualification.
pub fn build_current_formal_business_result_bounded_v1<'a>(
    source: oasis7_runtime::PaidBorrowedNormalBusinessSourceV1<'a>,
    previous: &PaidFormalBusinessDescriptorV1,
    parent_proof:&'a oasis7_distfs::controlled_authority::formal::endpoint::PaidHostQualifiedParentProofV1,
    reader: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<PaidFormalBusinessResultUpdateV1<'a>, PaidFormalBusinessErrorV1> {
    source.validate_context(common).map_err(body)?;
    previous
        .validate_context(common)
        .map_err(|e| fail(e, None))?;
    parent_proof
        .validate_context(common)
        .map_err(|e| fail(e, None))?;
    common.charge(1, 32).map_err(|e| fail(e, None))?;
    if parent_proof.actual_record_id() != source.actual_parent().verified_record_id() {
        return Err(fail(
            invalid("formal historical cut differs from candidate predecessor"),
            None,
        ));
    }
    previous.validate_record_bounded_v1(source.actual_parent().record(), common)?;
    previous.validate_metadata_leaf_bounded_v1(reader, common)?;
    let layout = source
        .delta()
        .retained_result_layout_bytes_v1()
        .ok_or_else(|| fail(overflow(), None))?;
    let m = source.metadata();
    let w = &source.actual_parent().record().world;
    let metadata_layout = [
        m.provider_world_id(),
        m.branch_id(),
        m.finality_status(),
        m.finality_block_hash().unwrap_or(""),
        m.manifest_hash(),
        w.world_id.as_str(),
        w.chain_id.as_str(),
        w.genesis_digest.as_str(),
        source.raw_state_hash(),
    ]
    .iter()
    .try_fold(0usize, |n, s| n.checked_add(s.len()))
    .and_then(|n| n.checked_mul(32))
    .ok_or_else(|| fail(overflow(), None))?;
    let bytes = layout
        .checked_mul(8)
        .and_then(|n| n.checked_add(metadata_layout))
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(|| fail(overflow(), None))?;
    let mut phase = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| {
            fail(
                invalid("formal Current business complete builder graph"),
                None,
            )
        })?;
    let count = source.delta().results().len();
    let work = layout
        .checked_mul(
            count
                .checked_add(16)
                .ok_or_else(|| fail(overflow(), None))?,
        )
        .and_then(|n| n.checked_add(bytes))
        .ok_or_else(|| fail(overflow(), None))?;
    common
        .charge(work as u64, work as u64)
        .map_err(|e| fail(e, None))?;
    let metadata = metadata_from_current_source(&source, common, canonical)?;
    // The true paid Store proof owns its complete original graph. Its bounded
    // wire traversal pays original cut hashing/clone work before the shared codec.
    let proof_wire = encode(parent_proof.proof(), common)?;
    let n = proof_wire.len();
    let proof_graph = n
        .checked_mul(128)
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(|| fail(overflow(), None))?;
    let mut proof_phase = common
        .memory()
        .reserve(proof_graph, MemoryClass::Ordinary)
        .map_err(|_| fail(invalid("historical cut complete clone graph"), None))?;
    common
        .charge(proof_graph as u64, proof_graph as u64)
        .map_err(|e| fail(e, None))?;
    phase
        .merge(&mut proof_phase)
        .map_err(|e| fail(invalid(e), None))?;
    if &parent_proof.proof().decision.record != source.actual_parent().record() {
        return Err(fail(
            invalid("formal historical cut differs from candidate predecessor"),
            Some(phase),
        ));
    }
    let cut=oasis7_formal_protocol::controlled_authority::formal::business_history::FormalHistoricalCutV1::from_proof(parent_proof.proof()).and_then(|v|v.leaf()).map_err(|e|fail(e,None))?;
    let core = common.work().identity();
    let state = std::cell::RefCell::new(BuildState {
        common,
        first: None,
        leaf_phase: None,
    });
    let canonical = std::cell::RefCell::new(canonical);
    let result = {
        let mut index = PaidIndex {
            state: &state,
            reader,
        };
        let mut facts = CurrentFacts {
            delta: source.delta(),
            state: &state,
            canonical: &canonical,
        };
        kernel::extend_with_index(
            &mut facts,
            metadata,
            Some(previous.tree()),
            source.delta().results().iter().map(|r| r.result()),
            &mut index,
            vec![cut],
        )
    };
    let mut state = state.into_inner();
    if let Some(mut leaf) = state.leaf_phase.take() {
        phase.merge(&mut leaf).map_err(|e| fail(invalid(e), None))?;
    }
    match result {
        Ok((tree, delta)) => {
            if let Some(mut e) = state.first {
                e._phase = Some(phase);
                return Err(e);
            }
            Ok(PaidFormalBusinessResultUpdateV1 {
                source,
                parent_proof,
                tree,
                delta,
                core,
                phase,
            })
        }
        Err(note) => {
            let mut error = state.first.unwrap_or_else(|| fail(invalid(note), None));
            error._phase = Some(phase);
            Err(error)
        }
    }
}

#[path = "formal_result_paid_genesis.rs"]
mod genesis;
pub use genesis::{
    PaidGenesisFormalBusinessResultUpdateV1, build_genesis_formal_business_result_bounded_v1,
};
