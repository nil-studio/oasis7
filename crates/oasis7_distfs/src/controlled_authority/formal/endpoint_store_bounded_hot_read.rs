//! Actual disk HOT decoding and original verification. Data, never refresh authority.
use super::super::FormalArchiveReadWorkV1;
use super::paid_read::PaidRead;
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::RefCell,
    mem::{MaybeUninit, size_of},
    ptr::NonNull,
};
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded Hot layout"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("bounded Hot layout"))
}
fn charge(
    work: &FormalArchiveReadWorkV1,
    n: usize,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    work.charge(
        1,
        u64::try_from(n).map_err(|_| invalid("bounded Hot layout"))?,
        stop,
    )
}
// Locked Rust1.96 BTreeMap layout, B=6. The only map is record.artifact_roots
// with 14 enum keys: at most two leaves and one internal node. Three leaves
// plus one internal conservatively include a transient split, even for repeated
// incoming keys. Replacing duplicate values does not grow distinct-key count.
#[repr(C)]
struct LeafLayout {
    parent: Option<NonNull<()>>,
    parent_idx: MaybeUninit<u16>,
    len: u16,
    keys: [MaybeUninit<crate::controlled_authority::replicated_protocol::ArtifactRole>; 11],
    values: [MaybeUninit<String>; 11],
}
#[repr(C)]
struct InternalLayout {
    leaf: LeafLayout,
    edges: [MaybeUninit<NonNull<()>>; 12],
}
fn diagnostic_constant() -> Result<usize, FormalError> {
    const LABELS: &[&str] = &[
        "Hot",
        "NativeHotBodyV1",
        "NativeCheckpointParentV1",
        "CanonicalRecordV1",
        "FormalWorldIdentityV1",
        "FormalHeadRefV1",
        "ProfileHandoffV1",
        "EpochSealBodyV1",
        "EndpointInstanceBindingV1",
        "DurableSetNativeBindingV1",
        "body",
        "signature",
        "version",
        "instance_digest",
        "authority_id",
        "activation_ref",
        "epoch",
        "revision",
        "head",
        "head_node",
        "imported_head_node",
        "import_digest",
        "base_parent",
        "index",
        "pending",
        "decision_head",
        "seal",
        "durable_set",
        "data_closure",
        "record",
        "finalizing_authority_id",
        "schema_version",
        "durable_set_transition_digest",
        "kind",
        "world",
        "position",
        "parent_record_id",
        "original_authority_id",
        "original_epoch",
        "request_digest",
        "payload_digest",
        "frozen_reservation_id",
        "runtime_manifest_digest",
        "governing_manifest_digest",
        "execution_policy_digest",
        "before_state_root",
        "after_state_root",
        "action_root",
        "event_root",
        "result_root",
        "projection_root",
        "artifact_roots",
        "handoff",
        "world_id",
        "chain_id",
        "genesis_digest",
        "record_id",
        "next_authority_digest",
        "next_profile",
        "next_profile_version",
        "valid_from_position",
        "instance",
        "checkpoint_before",
        "recovery_operation",
        "next_epoch_floor",
        "tail",
        "sealed_cut",
        "last_evidence_digest",
        "provider_public_key",
        "resource_binding",
        "transition_digest",
        "independent_inventory_digest",
        "activation_digest",
        "Input",
        "Result",
        "Snapshot",
        "Journal",
        "NonceIndex",
        "EffectOutbox",
        "ExecutionManifest",
        "Wasm",
        "Rules",
        "NativeExecutionEnvironment",
        "IncrementalGenesisConfig",
        "IncrementalRootSet",
        "IncrementalCommitBody",
        "IncrementalHistoryPrefix",
        "InitialActivation",
        "Execution",
        "a string",
        "a byte array",
        "a sequence",
        "a map",
        "a tuple",
        "a boolean",
        "unit",
        "option",
        "an integer",
        "floating point",
        "field identifier",
        "variant identifier",
        "invalid type: ",
        "invalid value: ",
        "invalid length ",
        ", expected ",
        "unknown field ",
        "unknown variant ",
        "duplicate field ",
        "missing field ",
        " at offset ",
        "Scratch buffer too small",
        "EOF while parsing a value",
        "EOF while parsing an array",
        "EOF while parsing a map",
        "length out of range",
        "invalid UTF-8",
        "unassigned type",
        "unexpected code",
        "trailing data",
        "array too short",
        "array too long",
        "recursion limit exceeded",
        "wrong enum format",
        "wrong struct format",
        "formal digest/key requires canonical lowercase hex32",
        "formal signature requires canonical lowercase hex64",
        "formal public key",
        "formal signature",
        "formal signature verification",
    ];
    // Six u64 display slots, fixed delimiters and four short primitive Debug
    // descriptions; input escape expansion belongs to the n-dependent phase.
    LABELS
        .iter()
        .try_fold(6 * 20 + 32 + 4 * 26, |n, s| add(n, s.len()))
}
fn decode_layout(n: usize) -> Result<usize, FormalError> {
    let k = diagnostic_constant()?;
    // Same locked serde_cbor_2 .13 IoRead/error bridge analysis as the paid
    // point readers. Scratch<=2n+16KiB; strings/geometric transient and escaped
    // diagnostic states are bounded by parse14n+16KiB+2K versus bridge24n+4K+64.
    // No Vec DTO/cautious sequence allocation exists in this closed Hot graph.
    let parse = add(add(mul(14, n)?, 16 << 10)?, mul(2, k)?)?;
    let bridge = add(add(mul(24, n)?, mul(4, k)?)?, 64)?;
    let maps = add(
        mul(3, size_of::<LeafLayout>())?,
        size_of::<InternalLayout>(),
    )?;
    let headers = add(
        size_of::<Hot>(),
        add(
            size_of::<PaidRead<'_>>(),
            add(
                size_of::<serde_cbor::de::Deserializer<serde_cbor::de::IoRead<&mut PaidRead<'_>>>>(
                ),
                add(
                    size_of::<serde_cbor::Error>(),
                    add(size_of::<FormalError>(), size_of::<String>())?,
                )?,
            )?,
        )?,
    )?;
    add(add(parse.max(bridge), maps)?, headers)
}
pub(in crate::controlled_authority::formal::endpoint) struct PaidNativeHotV1 {
    hot: ReservedValue<Hot>,
    wire: ReservedValue<Vec<u8>>,
    path_fee: ReservedValue<()>,
}
impl std::fmt::Debug for PaidNativeHotV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidNativeHotV1").finish_non_exhaustive()
    }
}
impl PaidNativeHotV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn body(&self) -> &HotBody {
        &self.hot.body
    }
    pub(in crate::controlled_authority::formal::endpoint) fn bytes(&self) -> &[u8] {
        &self.wire
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        m: &MemoryBudget,
    ) -> bool {
        self.hot.uses_budget(m) && self.wire.uses_budget(m) && self.path_fee.uses_budget(m)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.hot.reserved_bytes() + self.wire.reserved_bytes() + self.path_fee.reserved_bytes()
    }
}
fn read_wire(
    store: &Store,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(ReservedValue<Vec<u8>>, ReservedValue<()>), FormalError> {
    #[cfg(not(unix))]
    return Err(invalid(
        "bounded Hot secure file reads unsupported on this platform",
    ));
    charge(work, 0, stop)?;
    let path_bytes = add(
        add(store.path.as_os_str().as_encoded_bytes().len(), HOT.len())?,
        1,
    )?;
    // PathBuf plus escaped two-buffer IO diagnostic, headers and fixed IO text.
    let path_layout = add(
        add(size_of::<PathBuf>(), path_bytes)?,
        add(
            mul(12, path_bytes)?,
            add(
                2 * size_of::<String>(),
                "bounded Hot file metadata/open/read".len(),
            )?,
        )?,
    )?;
    let path_fee = memory
        .reserve(path_layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded Hot path admission"))?
        .retain(());
    let mut path = PathBuf::with_capacity(path_bytes);
    path.push(&store.path);
    path.push(HOT);
    if path.capacity() > path_bytes {
        return Err(invalid("bounded Hot path capacity"));
    }
    charge(work, 0, stop)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = options.open(&path).map_err(io)?;
    charge(work, 0, stop)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() {
        return Err(invalid("formal storage requires regular file"));
    }
    let n = usize::try_from(metadata.len()).map_err(|_| invalid("formal storage byte limit"))?;
    if n > LIMIT {
        return Err(invalid("formal storage byte limit"));
    }
    let fee = memory
        .reserve(
            add(size_of::<ReservedValue<Vec<u8>>>(), n)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded Hot wire admission"))?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(invalid("bounded Hot wire capacity"));
    }
    while bytes.len() < n {
        let end = add(bytes.len(), 8192)?.min(n);
        charge(work, end - bytes.len(), stop)?;
        bytes.resize(end, 0);
    }
    let mut at = 0;
    while at < n {
        let end = add(at, 8192)?.min(n);
        charge(work, end - at, stop)?;
        match file.read(&mut bytes[at..end]) {
            Ok(0) => return Err(io(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))),
            Ok(got) => at += got,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(io(e)),
        }
    }
    charge(work, 1, stop)?;
    let mut extra = [0];
    loop {
        match file.read(&mut extra) {
            Ok(0) => break,
            Ok(_) => return Err(invalid("formal storage byte limit")),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                charge(work, 1, stop)?;
            }
            Err(e) => return Err(io(e)),
        }
    }
    Ok((fee.retain(bytes), path_fee))
}
struct PaidHotPreimage(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for PaidHotPreimage {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
fn preimage(
    body: &HotBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidHotPreimage, FormalError> {
    // Original crypto::bytes limits encoded value64MiB, independent of raw Hot
    // length (legal nonminimal/unknown-field representations may differ).
    let encoded = super::context_wire::encode_paid(
        body,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "bounded Hot encoding",
    )?;
    let n = add(add(8, DOMAIN.len())?, encoded.len())?;
    let fee = ctx
        .memory()
        .reserve(
            add(size_of::<ReservedValue<Vec<u8>>>(), n)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded Hot preimage admission"))?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(invalid("bounded Hot preimage capacity"));
    }
    for source in [
        &(DOMAIN.len() as u64).to_be_bytes()[..],
        DOMAIN.as_bytes(),
        encoded.as_slice(),
    ] {
        for chunk in source.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            bytes.extend_from_slice(chunk);
        }
    }
    Ok(PaidHotPreimage(fee.retain(bytes)))
}
pub(super) fn read_hot_bounded_v1(
    store: &Store,
    expected_public_key: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNativeHotV1, FormalError> {
    let (wire, path_fee) = read_wire(store, memory, work, stop)?;
    decode_hot_paid_wire(wire, path_fee, expected_public_key, memory, work, stop)
}
pub(super) fn decode_hot_paid_wire(
    wire: ReservedValue<Vec<u8>>,
    path_fee: ReservedValue<()>,
    expected_public_key: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNativeHotV1, FormalError> {
    charge(work, 0, stop)?;
    let extra = size_of::<PaidNativeHotV1>()
        .checked_sub(add(size_of::<Hot>(), size_of::<ReservedValue<Vec<u8>>>())?)
        .ok_or_else(|| invalid("bounded Hot owner layout"))?;
    let fee = memory
        .reserve(
            add(decode_layout(wire.len())?, extra)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded Hot decode admission"))?;
    let mut reader = PaidRead::new(&wire, work, stop);
    let decoded = serde_cbor::from_reader::<Hot, _>(&mut reader);
    if let Some(first) = reader.take_first_error() {
        return Err(first);
    }
    let hot = decoded.map_err(|e| invalid(e.to_string()))?;
    reader.check_cancelled()?;
    drop(reader);
    let crypto_layout = add(
        96 + 2 * size_of::<Vec<u8>>() + 96,
        add(
            size_of::<ed25519_dalek::VerifyingKey>(),
            size_of::<ed25519_dalek::Signature>(),
        )?,
    )?;
    let _crypto = memory
        .reserve(crypto_layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded Hot crypto admission"))?;
    let mut ctx = work.bounded_context(memory, stop);
    let shared = RefCell::new(&mut ctx);
    Store::verify_generated_hot_with(
        &hot.body,
        &hot.signature,
        expected_public_key,
        |body, signature, key| {
            oasis7_formal_protocol::controlled_authority::formal::crypto::verify_with_preimage(
                signature,
                key,
                || preimage(body, &mut shared.borrow_mut()),
                |n| shared.borrow_mut().charge(1, n as u64),
            )
        },
        |binding| {
            enum DurableError {
                Work(FormalError),
                Fencing(fencing::FencingError),
            }
            impl From<fencing::FencingError> for DurableError {
                fn from(e: fencing::FencingError) -> Self {
                    Self::Fencing(e)
                }
            }
            binding
                .validate_with_work(&mut |n| {
                    shared
                        .borrow_mut()
                        .charge(1, n as u64)
                        .map_err(DurableError::Work)
                })
                .map_err(|error| match error {
                    DurableError::Work(e) => e,
                    DurableError::Fencing(e) => fencing_error(e),
                })
        },
    )?;
    drop(shared);
    Ok(PaidNativeHotV1 {
        hot: fee.retain(hot),
        wire,
        path_fee,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_hot_read_tests.rs"]
mod tests;
