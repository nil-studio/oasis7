//! Real bounded TCP Acquire. A private-mint lease retains data fees and its FD.
//! This is not an implementation of the legacy unbounded mutation guard.
use super::*;
use crate::controlled_authority::formal::FormalError;
use crate::controlled_authority::formal::endpoint::{
    BoundedWireErrorV1, BoundedWireWorkV1, encode_bounded_wire_v1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
#[path = "fencing_remote_bounded_sign.rs"]
mod bounded_sign;
pub(in crate::controlled_authority) use bounded_sign::{
    PaidGenesisReceiptSignatureV1, PaidNativeCheckpointBeginV1, PaidNativeCheckpointSignatureV1,
    begin_signed_native_checkpoint_bounded_v1, initialize_protected_registration_bounded_v1,
    sign_genesis_receipt_bounded_v1, sign_native_checkpoint_bounded_v1,
    sign_normal_frozen_receipt_bounded_v1,
};

#[derive(Debug)]
pub enum BoundedAcquireErrorV1 {
    Work(FormalError),
    Provider(FencingError),
}
impl From<FencingError> for BoundedAcquireErrorV1 {
    fn from(e: FencingError) -> Self {
        Self::Provider(e)
    }
}
impl From<FormalError> for BoundedAcquireErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Work(e)
    }
}
impl std::fmt::Display for BoundedAcquireErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(e) => e.fmt(f),
            Self::Provider(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for BoundedAcquireErrorV1 {}
type ResultV<T> = Result<T, BoundedAcquireErrorV1>;
fn layout() -> BoundedAcquireErrorV1 {
    FormalError::Invalid("bounded remote layout overflow".into()).into()
}
fn add(a: usize, b: usize) -> ResultV<usize> {
    a.checked_add(b).ok_or_else(layout)
}
fn mul(a: usize, b: usize) -> ResultV<usize> {
    a.checked_mul(b).ok_or_else(layout)
}
fn debit(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> ResultV<()> {
    ctx.consume_read_bytes(u64::try_from(n).map_err(|_| layout())?)
        .map_err(Into::into)
}
fn step(ctx: &mut FormalBoundedContextV1<'_>) -> ResultV<()> {
    ctx.consume_steps(1).map_err(Into::into)
}
fn reserve(memory: &MemoryBudget, n: usize) -> ResultV<ReservedValue<()>> {
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map(|f| f.retain(()))
        .map_err(|_| FormalError::Invalid("bounded remote memory admission".into()).into())
}
struct PaidWire(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for PaidWire {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
fn wire_error(e: BoundedWireErrorV1) -> BoundedAcquireErrorV1 {
    match e {
        BoundedWireErrorV1::Cancelled => {
            FormalError::Invalid("bounded archive read cancelled".into()).into()
        }
        BoundedWireErrorV1::StepLimit => {
            FormalError::Invalid("bounded archive read steps".into()).into()
        }
        BoundedWireErrorV1::ByteLimit => {
            FormalError::Invalid("bounded archive read bytes".into()).into()
        }
        BoundedWireErrorV1::Encode(e) => invalid(e.to_string()).into(),
        BoundedWireErrorV1::WireLimit => invalid("monotonic oracle frame bound").into(),
        other => FormalError::Invalid(format!("bounded remote wire: {other}")).into(),
    }
}
fn encode_paid<T: Serialize>(
    value: &T,
    max: usize,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidWire> {
    step(ctx)?;
    let (memory, work, stop) = ctx.parts();
    let lease = work.begin_child()?;
    let before = (lease.remaining_steps(), lease.remaining_read_bytes());
    let mut scalar = BoundedWireWorkV1 {
        max_wire_bytes: max,
        remaining_steps: before.0,
        remaining_bytes: before.1,
    };
    let result = encode_bounded_wire_v1(value, memory, &mut scalar, &mut || stop())
        .map(PaidWire)
        .map_err(wire_error);
    let delta = before
        .0
        .checked_sub(scalar.remaining_steps)
        .zip(before.1.checked_sub(scalar.remaining_bytes));
    let debit: ResultV<()> = match delta {
        Some((s, b)) => lease.consume(s, b).map_err(Into::into),
        None => {
            drop(lease);
            return result.and_then(|_| {
                Err(FormalError::Invalid("bounded archive child counters increased".into()).into())
            });
        }
    };
    lease.finish();
    match result {
        Err(e) => Err(e),
        Ok(v) => {
            debit?;
            Ok(v)
        }
    }
}
fn domain_frame<T: Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidWire> {
    // No grammar cap on re-encoding a checked typed body: its actual measured
    // wire must fit the same continuing Work and exact caller memory admission.
    let body = encode_paid(value, usize::MAX, ctx)?;
    let n = add(add(8, domain.len())?, body.as_ref().len())?;
    let fee = ctx
        .memory()
        .reserve(
            add(n, std::mem::size_of::<Vec<u8>>())?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| FormalError::Invalid("bounded remote frame admission".into()))?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() > n {
        return Err(layout());
    }
    for source in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        body.as_ref(),
    ] {
        for chunk in source.chunks(8192) {
            debit(ctx, chunk.len())?;
            bytes.extend_from_slice(chunk);
        }
    }
    Ok(PaidWire(fee.retain(bytes)))
}
fn hash_paid<T: Serialize>(value: &T, ctx: &mut FormalBoundedContextV1<'_>) -> ResultV<String> {
    let wire = encode_paid(value, usize::MAX, ctx)?;
    debit(ctx, wire.as_ref().len())?;
    Ok(hash_encoded(wire.as_ref()))
}
fn sign_paid<T: Serialize>(
    domain: &str,
    value: &T,
    key: &SigningKey,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<String> {
    let shared = RefCell::new(ctx);
    sign_with_preimage(
        key,
        || domain_frame(domain, value, &mut shared.borrow_mut()),
        |n| debit(&mut shared.borrow_mut(), n),
    )
}
fn verify_paid<T: Serialize>(
    domain: &str,
    value: &T,
    signature: &str,
    key: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<()> {
    // The original hex decoder allocates before rejecting a wrong-size signature.
    // Cover its minimum/geometric capacity, key32 and both Vec headers first.
    let _hex = reserve(
        ctx.memory(),
        add(
            add(mul(signature.len() / 2, 2)?.max(8), 32)?,
            2 * std::mem::size_of::<Vec<u8>>(),
        )?,
    )?;
    let shared = RefCell::new(ctx);
    verify_with_preimage(
        signature,
        key,
        || domain_frame(domain, value, &mut shared.borrow_mut()),
        |n| debit(&mut shared.borrow_mut(), n),
    )
}
fn instance_text(instance: &EndpointInstanceBindingV1) -> [&String; 8] {
    [
        &instance.world_id,
        &instance.chain_id,
        &instance.genesis_digest,
        &instance.endpoint_id,
        &instance.endpoint_public_key,
        &instance.instance_id,
        &instance.storage_id,
        &instance.storage_fingerprint,
    ]
}
fn instance_payload(instance: &EndpointInstanceBindingV1) -> ResultV<usize> {
    instance_text(instance)
        .iter()
        .try_fold(0, |n, s| add(n, s.len()))
}
fn checkpoint_payload(checkpoint: &EndpointCheckpointBindingV1) -> ResultV<usize> {
    add(
        checkpoint.commitment.len(),
        checkpoint.head.decision_hash.len(),
    )
}
fn text(s: &str) -> ResultV<String> {
    let mut copy = String::with_capacity(s.len());
    if copy.capacity() > s.len() {
        return Err(layout());
    }
    copy.push_str(s);
    Ok(copy)
}
fn allowed(
    policy: &TrustedFencingPolicyV1,
    resource: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<bool> {
    for candidate in &policy.authorized_resource_bindings {
        debit(ctx, add(candidate.len(), resource.len())?)?;
        if candidate == resource {
            return Ok(true);
        }
    }
    Ok(false)
}
struct Validation<'a, 'b> {
    ctx: &'a mut FormalBoundedContextV1<'b>,
}
impl ReplyValidationOps for Validation<'_, '_> {
    type Error = BoundedAcquireErrorV1;
    fn compare(&mut self, n: usize) -> ResultV<()> {
        debit(self.ctx, n)
    }
    fn verify_reply(
        &mut self,
        reply: &SignedMonotonicOracleReplyV1,
        policy: &TrustedFencingPolicyV1,
    ) -> ResultV<()> {
        verify_paid(
            RESPONSE,
            &reply.body,
            &reply.signature,
            &policy.provider_public_key,
            self.ctx,
        )
    }
    fn resource_allowed(
        &mut self,
        resource: &str,
        policy: &TrustedFencingPolicyV1,
    ) -> ResultV<bool> {
        allowed(policy, resource, self.ctx)
    }
    fn validate_checkpoint(&mut self, v: &EndpointCheckpointBindingV1) -> ResultV<()> {
        debit(self.ctx, checkpoint_payload(v)?)?;
        v.validate().map_err(Into::into)
    }
    fn validate_seal(&mut self, v: &EpochSealBodyV1) -> ResultV<()> {
        // Measuring the original closed typed wire accounts for every nested
        // validation string, including the optional LocalRequestIdentity.
        let encoded = encode_paid(v, usize::MAX, self.ctx)?;
        debit(self.ctx, encoded.as_ref().len())?;
        v.validate().map_err(Into::into)
    }
    fn verify_evidence(
        &mut self,
        e: &MonotonicProviderEvidenceV1,
        i: &EndpointInstanceBindingV1,
        c: &EndpointCheckpointBindingV1,
        p: &TrustedFencingPolicyV1,
    ) -> ResultV<()> {
        let shared = RefCell::new(&mut *self.ctx);
        verify_monotonic_checkpoint_evidence_with(
            e,
            i,
            c,
            p,
            |statement, signature, key| {
                verify_paid(
                    PROVIDER_DOMAIN,
                    statement,
                    signature,
                    key,
                    &mut shared.borrow_mut(),
                )
            },
            |instance| {
                let mut ctx = shared.borrow_mut();
                debit(&mut ctx, instance_payload(instance)?)?;
                instance.validate()?;
                hash_paid(instance, &mut ctx)
            },
            |policy, resource| allowed(policy, resource, &mut shared.borrow_mut()),
        )
    }
    fn seal_digest(&mut self, v: &EpochSealBodyV1) -> ResultV<String> {
        self.validate_seal(v)?;
        hash_paid(v, self.ctx)
    }
}

// All labels reachable from SignedReply's closed 95-String/two-Vec graph.
// These are diagnostic constants, never a replacement serialization schema.
const LABELS: &[&str] = &[
    "SignedMonotonicOracleReplyV1",
    "MonotonicOracleReplyBodyV1",
    "EndpointCheckpointBindingV1",
    "HeadAnchor",
    "FormalHeadRefV1",
    "record_id",
    "CheckpointIntentV1",
    "EpochSealBodyV1",
    "EndpointInstanceBindingV1",
    "SealedTailV1",
    "LocalRequestIdentity",
    "MonotonicProviderEvidenceV1",
    "MonotonicProviderStatementV1",
    "DurableSetProviderStateV1",
    "DurableSetInstallReceiptV1",
    "DurableSetInstallBodyV1",
    "ProtectedEndpointSignatureV1",
    "ProtectedEndpointSignatureBodyV1",
    "schema_version",
    "body",
    "signature",
    "capability",
    "resource_binding",
    "request_digest",
    "challenge",
    "lease",
    "checkpoint",
    "pending",
    "seal",
    "evidence",
    "native_checkpoint_bytes",
    "protected_custody",
    "durable_set",
    "protected_signature",
    "revision",
    "commitment",
    "head",
    "position",
    "decision_hash",
    "qualified",
    "previous",
    "next",
    "checkpoint_bytes",
    "instance",
    "recovery_operation",
    "highest_complete",
    "checkpoint_before",
    "tail",
    "next_epoch_floor",
    "world_id",
    "chain_id",
    "genesis_digest",
    "authority_epoch",
    "endpoint_role",
    "endpoint_id",
    "endpoint_public_key",
    "instance_id",
    "storage_id",
    "storage_fingerprint",
    "kind",
    "request",
    "frozen_input_digest",
    "frozen_certificate_digest",
    "proposal_digest",
    "replica_prepare_digest",
    "primary_receipt_digest",
    "replica_receipt_digest",
    "verified_subject",
    "operation_domain",
    "nonce_scope",
    "request_id",
    "instance_digest",
    "epoch_floor",
    "seal_digest",
    "transition_digest",
    "successor_authority_id",
    "install_receipt",
    "activation_digest",
    "boundary",
    "independent_closure_root",
    "independent_inventory_digest",
    "endpoint_signature_hex",
    "provider_signature_hex",
    "provider_evidence",
    "custody",
    "endpoint_signature",
    "provider_signature",
    "ProviderCapabilityV1",
    "EndpointRole",
    "SealedTailKindV1",
    "ProtectedSignerCustodyV1",
    "ControlledLocalTest",
    "MonotonicExclusiveSignerStorageV1",
    "Primary",
    "Replica",
    "Empty",
    "Frozen",
    "Prepared",
    "Decision",
    "NonexportableLeaseBoundV1",
    "option",
    "a string",
    "a sequence",
    "u8",
    "u32",
    "u64",
    "a boolean",
    "field identifier",
    "variant identifier",
    "invalid type: , expected ",
    "invalid value: , expected ",
    "invalid length , expected ",
    "unknown field ``, expected ",
    "unknown variant ``, expected ",
    "missing field ``",
    "duplicate field ``",
    " at offset ",
    "one of ",
    " or ",
    "struct ",
];
fn read_reply_paid(
    stream: &mut TcpStream,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<ReservedValue<SignedMonotonicOracleReplyV1>> {
    let mut length = [0; 4];
    read_exact_paid(
        stream,
        &mut length,
        "monotonic oracle response header read",
        ctx,
    )?;
    let n = u32::from_be_bytes(length) as usize;
    if n == 0 || n > LIMIT {
        return Err(invalid("monotonic oracle frame bound").into());
    }
    let raw_fee = ctx
        .memory()
        .reserve(
            add(n, std::mem::size_of::<Vec<u8>>())?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| FormalError::Invalid("bounded remote raw admission".into()))?;
    let mut raw = Vec::with_capacity(n);
    if raw.capacity() > n {
        return Err(layout());
    }
    while raw.len() < n {
        let count = (n - raw.len()).min(8192);
        debit(ctx, count)?;
        raw.resize(raw.len() + count, 0);
    }
    read_exact_paid(stream, &mut raw, "monotonic oracle response body read", ctx)?;
    let raw = raw_fee.retain(raw);
    let phase = ctx
        .memory()
        .reserve(decode_phase(n)?, MemoryClass::Ordinary)
        .map_err(|_| FormalError::Invalid("bounded remote reply heap admission".into()))?;
    let mut reader = PaidRead {
        bytes: raw.as_slice(),
        ctx,
        first: None,
    };
    let decoded = serde_cbor::from_reader::<SignedMonotonicOracleReplyV1, _>(&mut reader);
    if let Some(error) = reader.first.take() {
        return Err(error);
    }
    let reply = decoded.map_err(|e| invalid(e.to_string()))?;
    drop(reader);
    drop(raw);
    Ok(phase.retain(reply))
}

fn decode_phase(n: usize) -> ResultV<usize> {
    // Two generic Vec<u8> cautious reservations (serde_core 1.0.229) each <=1MiB;
    // their growth adds <=2n. Strings add <=2n; IoRead scratch<=2n+16KiB.
    // First escaped diagnostic <=12n+2K: parse=2MiB+18n+16KiB+2K.
    // After Deserializer/partial values drop, two error buffers coexist:24n+4K+64.
    // K: six decimal slots*20, 32 delimiters, four short scalar descriptions*26,
    // plus every reachable fixed field/type/enum/template above.
    let k = LABELS
        .iter()
        .try_fold(6 * 20 + 32 + 4 * 26, |n, s| add(n, s.len()))?;
    let parse = add(
        add(add(2 * (1 << 20), mul(n, 18)?)?, 16 * 1024)?,
        mul(k, 2)?,
    )?;
    let bridge = add(add(mul(n, 24)?, mul(k, 4)?)?, 64)?;
    // Typed inline headers/padding are counted once. The Deserializer type
    // includes IoRead's scratch Vec header, offset and lookahead; PaidRead is
    // the separate outer adapter. The bridge String is the second formatted
    // diagnostic while serde::Error still owns its first diagnostic buffer.
    let headers = std::mem::size_of::<SignedMonotonicOracleReplyV1>()
        + std::mem::size_of::<
            serde_cbor::Deserializer<serde_cbor::de::IoRead<&mut PaidRead<'_, '_>>>,
        >()
        + std::mem::size_of::<PaidRead<'_, '_>>()
        + std::mem::size_of::<serde_cbor::Error>()
        + std::mem::size_of::<BoundedAcquireErrorV1>()
        + std::mem::size_of::<String>();
    // Each retained typed value already paid its own inline layout. Only the
    // remaining owner wrappers, FD, opaque identity and actual padding go here.
    let typed = std::mem::size_of::<SignedMonotonicOracleRequestV1>()
        + std::mem::size_of::<SignedMonotonicOracleReplyV1>()
        + std::mem::size_of::<TrustedFencingPolicyV1>();
    let owner_extra = std::mem::size_of::<PaidRemoteLeaseV1>()
        .checked_sub(typed)
        .ok_or_else(layout)?;
    add(add(parse.max(bridge), headers)?, owner_extra)
}
struct PaidRead<'a, 'b> {
    bytes: &'a [u8],
    ctx: &'a mut FormalBoundedContextV1<'b>,
    first: Option<BoundedAcquireErrorV1>,
}
impl Read for PaidRead<'_, '_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.first.is_some() {
            return Err(std::io::ErrorKind::Other.into());
        }
        let n = out.len().min(8192).min(self.bytes.len());
        if let Err(e) = debit(self.ctx, n) {
            self.first = Some(e);
            return Err(std::io::ErrorKind::Other.into());
        }
        out[..n].copy_from_slice(&self.bytes[..n]);
        self.bytes = &self.bytes[n..];
        Ok(n)
    }
}
fn read_exact_paid(
    reader: &mut impl Read,
    bytes: &mut [u8],
    label: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let end = (offset + 8192).min(bytes.len());
        debit(ctx, end - offset)?;
        match reader.read(&mut bytes[offset..end]) {
            Ok(0) => {
                return Err(FencingError::Io(format!(
                    "{label}: {}",
                    std::io::Error::from(std::io::ErrorKind::UnexpectedEof)
                ))
                .into());
            }
            Ok(n) if n <= end - offset => offset += n,
            Ok(_) => return Err(layout()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FencingError::Io(format!("{label}: {e}")).into()),
        }
    }
    Ok(())
}
fn write_all_paid(
    writer: &mut impl Write,
    bytes: &[u8],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let end = (offset + 8192).min(bytes.len());
        debit(ctx, end - offset)?;
        match writer.write(&bytes[offset..end]) {
            Ok(0) => {
                return Err(FencingError::Io(format!(
                    "monotonic oracle request write: {}",
                    std::io::Error::from(std::io::ErrorKind::WriteZero)
                ))
                .into());
            }
            Ok(n) if n <= end - offset => offset += n,
            Ok(_) => return Err(layout()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(
                    FencingError::Io(format!("monotonic oracle request write: {e}")).into(),
                );
            }
        }
    }
    Ok(())
}

pub trait BoundedMonotonicEndpointProvider: Send + Sync {
    fn acquire_bounded(
        &self,
        _instance: &EndpointInstanceBindingV1,
        _initial: &EndpointCheckpointBindingV1,
        _ctx: &mut FormalBoundedContextV1<'_>,
    ) -> ResultV<PaidRemoteLeaseV1> {
        Err(FencingError::FencingUnproven.into())
    }
}
pub struct PaidRemoteLeaseV1 {
    _stream: TcpStream,
    request: ReservedValue<SignedMonotonicOracleRequestV1>,
    reply: ReservedValue<SignedMonotonicOracleReplyV1>,
    policy: ReservedValue<TrustedFencingPolicyV1>,
    identity: WorkIdentityV1,
}
impl std::fmt::Debug for PaidRemoteLeaseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidRemoteLeaseV1").finish_non_exhaustive()
    }
}
impl PaidRemoteLeaseV1 {
    pub fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        &self.reply.body.checkpoint
    }
    pub fn pending(&self) -> Option<&CheckpointIntentV1> {
        self.reply.body.pending.as_ref()
    }
    pub fn seal(&self) -> Option<&EpochSealBodyV1> {
        self.reply.body.seal.as_ref()
    }
    pub fn evidence(&self) -> Option<&MonotonicProviderEvidenceV1> {
        self.reply.body.evidence.as_ref()
    }
    pub(in crate::controlled_authority) fn protected_custody(
        &self,
    ) -> Option<&ProtectedSignerCustodyV1> {
        self.reply.body.protected_custody.as_ref()
    }
    pub fn native_checkpoint_bytes(&self) -> Option<&[u8]> {
        self.reply.body.native_checkpoint_bytes.as_deref()
    }
    pub fn durable_set_state(&self) -> Option<&DurableSetProviderStateV1> {
        self.reply.body.durable_set.as_ref()
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.request.uses_budget(memory)
            && self.reply.uses_budget(memory)
            && self.policy.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.request.reserved_bytes() + self.reply.reserved_bytes() + self.policy.reserved_bytes()
    }
}
pub fn acquire_provider_lease_bounded_v1(
    source: &dyn BoundedMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    initial: &EndpointCheckpointBindingV1,
    pinned: &TrustedFencingPolicyV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidRemoteLeaseV1> {
    step(ctx)?;
    let lease = source.acquire_bounded(instance, initial, ctx)?;
    if !lease.uses_budget(ctx.memory()) || lease.identity != ctx.work().identity() {
        return Err(FormalError::Invalid("bounded remote foreign pool or meter".into()).into());
    }
    debit(
        ctx,
        add(
            instance_payload(instance)?,
            instance_payload(&lease.request.body.instance)?,
        )?,
    )?;
    if &lease.request.body.instance != instance {
        return Err(FencingError::FencingUnproven.into());
    }
    let MonotonicOracleCommandV1::Acquire(actual) = &lease.request.body.command else {
        return Err(FencingError::FencingUnproven.into());
    };
    debit(
        ctx,
        add(checkpoint_payload(initial)?, checkpoint_payload(actual)?)?,
    )?;
    if actual != initial {
        return Err(FencingError::FencingUnproven.into());
    }
    debit(
        ctx,
        add(
            pinned.provider_public_key.len(),
            lease.policy.provider_public_key.len(),
        )?,
    )?;
    if pinned.provider_public_key != lease.policy.provider_public_key
        || pinned.authorized_resource_bindings.len()
            != lease.policy.authorized_resource_bindings.len()
    {
        return Err(FencingError::FencingUnproven.into());
    }
    for (a, b) in pinned
        .authorized_resource_bindings
        .iter()
        .zip(&lease.policy.authorized_resource_bindings)
    {
        debit(ctx, add(a.len(), b.len())?)?;
        if a != b {
            return Err(FencingError::FencingUnproven.into());
        }
    }
    Ok(lease)
}
#[cfg(not(unix))]
impl BoundedMonotonicEndpointProvider for RemoteMonotonicEndpointProvider {}
fn copy_policy_paid(
    source: &TrustedFencingPolicyV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<ReservedValue<TrustedFencingPolicyV1>> {
    let mut policy_bytes = add(
        source.provider_public_key.len(),
        std::mem::size_of::<TrustedFencingPolicyV1>(),
    )?;
    policy_bytes = add(
        policy_bytes,
        mul(
            source.authorized_resource_bindings.len(),
            std::mem::size_of::<String>(),
        )?,
    )?;
    for resource in &source.authorized_resource_bindings {
        step(ctx)?;
        policy_bytes = add(policy_bytes, resource.len())?;
    }
    let policy_fee = ctx
        .memory()
        .reserve(policy_bytes, MemoryClass::Ordinary)
        .map_err(|_| FormalError::Invalid("bounded remote policy admission".into()))?;
    debit(ctx, source.provider_public_key.len())?;
    let public_key = text(&source.provider_public_key)?;
    let mut resources = Vec::with_capacity(source.authorized_resource_bindings.len());
    if resources.capacity() > source.authorized_resource_bindings.len() {
        return Err(layout());
    }
    for resource in &source.authorized_resource_bindings {
        debit(ctx, resource.len())?;
        resources.push(text(resource)?);
    }
    let policy = policy_fee.retain(TrustedFencingPolicyV1 {
        provider_public_key: public_key,
        authorized_resource_bindings: resources,
    });
    Ok(policy)
}
#[cfg(unix)]
impl BoundedMonotonicEndpointProvider for RemoteMonotonicEndpointProvider {
    fn acquire_bounded(
        &self,
        instance: &EndpointInstanceBindingV1,
        initial: &EndpointCheckpointBindingV1,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> ResultV<PaidRemoteLeaseV1> {
        step(ctx)?;
        let request_bytes = add(
            add(instance_payload(instance)?, checkpoint_payload(initial)?)?,
            64 + 64 + 128,
        )?;
        let request_fee = ctx
            .memory()
            .reserve(
                add(
                    request_bytes,
                    std::mem::size_of::<SignedMonotonicOracleRequestV1>(),
                )?,
                MemoryClass::Ordinary,
            )
            .map_err(|_| FormalError::Invalid("bounded remote request admission".into()))?;
        debit(ctx, instance.endpoint_public_key.len())?;
        let auth = hex::encode(self.key.verifying_key().to_bytes());
        if instance.endpoint_public_key == auth {
            return Err(FencingError::FencingUnproven.into());
        }
        step(ctx)?;
        let mut stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_nonblocking(false)
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|e| FencingError::Io(e.to_string()))?;
        let challenge = super::super::fresh_challenge_with_reader(|bytes| {
            step(ctx)?;
            let mut source =
                std::fs::File::open("/dev/urandom").map_err(|e| FencingError::Io(e.to_string()))?;
            let mut offset = 0;
            while offset < bytes.len() {
                debit(ctx, bytes.len() - offset)?;
                match source.read(&mut bytes[offset..]) {
                    Ok(0) => {
                        return Err(BoundedAcquireErrorV1::Provider(FencingError::Io(
                            std::io::Error::from(std::io::ErrorKind::UnexpectedEof).to_string(),
                        )));
                    }
                    Ok(n) => offset += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        return Err(BoundedAcquireErrorV1::Provider(FencingError::Io(
                            e.to_string(),
                        )));
                    }
                }
            }
            debit(ctx, 32)
        })?;
        debit(
            ctx,
            add(instance_payload(instance)?, checkpoint_payload(initial)?)?,
        )?;
        let copied_instance = instance.clone();
        for s in instance_text(&copied_instance) {
            if s.capacity() > s.len() {
                return Err(layout());
            }
        }
        let copied_initial = initial.clone();
        if copied_initial.commitment.capacity() > copied_initial.commitment.len()
            || copied_initial.head.decision_hash.capacity()
                > copied_initial.head.decision_hash.len()
        {
            return Err(layout());
        }
        let body = MonotonicOracleRequestBodyV1 {
            schema_version: 1,
            instance: copied_instance,
            authentication_public_key: auth,
            lease: None,
            challenge,
            command: MonotonicOracleCommandV1::Acquire(copied_initial),
        };
        let _digest_fee = reserve(ctx.memory(), 64 + std::mem::size_of::<String>())?;
        let request_digest = hash_paid(&body, ctx)?;
        let signature = sign_paid(REQUEST, &body, &self.key, ctx)?;
        if signature.capacity() > 128 {
            return Err(layout());
        }
        let request = request_fee.retain(SignedMonotonicOracleRequestV1 { body, signature });
        let encoded = encode_paid(&*request, LIMIT, ctx)?;
        write_all_paid(
            &mut stream,
            &(encoded.as_ref().len() as u32).to_be_bytes(),
            ctx,
        )?;
        write_all_paid(&mut stream, encoded.as_ref(), ctx)?;
        drop(encoded);
        let reply = read_reply_paid(&mut stream, ctx)?;
        validate_reply_with(
            &reply,
            &request_digest,
            &request.body.challenge,
            None,
            instance,
            &self.policy,
            &mut Validation { ctx },
        )?;
        // The complete peak fee deliberately remains with the returned reply.
        // Policy cloning has its own actual-layout admission and lifetime.
        let policy = copy_policy_paid(&self.policy, ctx)?;
        step(ctx)?;
        Ok(PaidRemoteLeaseV1 {
            _stream: stream,
            request,
            reply,
            policy,
            identity: ctx.work().identity(),
        })
    }
}
#[cfg(test)]
#[path = "fencing_remote_bounded_acquire_tests.rs"]
mod tests;
