//! Existing local, same-authority cold reopen. No initialization or archive import.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;
type ResultV<T> = Result<T, BoundedAcquireErrorV1>;
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded cold reopen layout"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("bounded cold reopen layout"))
}
fn charge(
    work: &FormalArchiveReadWorkV1,
    n: usize,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    work.charge(
        1,
        u64::try_from(n).map_err(|_| invalid("bounded cold reopen layout"))?,
        stop,
    )
}
fn reserve(
    memory: &MemoryBudget,
    n: usize,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, FormalError> {
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded cold reopen admission"))
}
fn hash_encoded<T: Serialize>(
    value: &T,
    domain: Option<&str>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<String>, FormalError> {
    let fee = reserve(memory, add(size_of::<ReservedValue<String>>(), 64)?)?;
    let mut ctx = work.bounded_context(memory, stop);
    let wire = super::context_wire::encode_paid(
        value,
        LIMIT,
        &mut ctx,
        "formal encoding exceeds byte bound",
        "bounded cold reopen encoding",
    )?;
    let mut hash = blake3::Hasher::new();
    if let Some(domain) = domain {
        ctx.charge(1, add(8, domain.len())? as u64)?;
        hash.update(&(domain.len() as u64).to_be_bytes());
        hash.update(domain.as_bytes());
    }
    for chunk in wire.chunks(8192) {
        ctx.charge(1, chunk.len() as u64)?;
        hash.update(chunk);
    }
    ctx.charge(1, 64)?;
    let text = hash.finalize().to_hex().to_string();
    if text.capacity() > 64 {
        return Err(invalid("bounded cold digest capacity"));
    }
    Ok(fee.retain(text))
}
#[cfg(unix)]
fn canonical_directory(
    path: &Path,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PathBuf, FormalError> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    // POSIX realpath's caller-buffer contract requires PATH_MAX bytes. Longer
    // names are explicitly outside this admission slice; std realpath(NULL)'s
    // implementation-owned allocation is never called.
    if bytes.len() >= libc::PATH_MAX as usize {
        return Err(invalid("bounded cold path unsupported length/NUL"));
    }
    charge(work, bytes.len(), stop)?;
    if bytes.contains(&0) {
        return Err(invalid("bounded cold path unsupported length/NUL"));
    }
    let mut input = Vec::with_capacity(bytes.len() + 1);
    if input.capacity() > bytes.len() + 1 {
        return Err(invalid("bounded cold path capacity"));
    }
    input.extend_from_slice(bytes);
    input.push(0);
    let mut output = [0u8; libc::PATH_MAX as usize];
    charge(work, 0, stop)?;
    // SAFETY: input is a terminated, interior-NUL-free byte string; output is
    // an initialized PATH_MAX-sized caller buffer as required by POSIX realpath.
    // Both remain alive and uniquely writable for the synchronous call.
    let result = unsafe { libc::realpath(input.as_ptr().cast(), output.as_mut_ptr().cast()) };
    if result.is_null() {
        return Err(io(std::io::Error::last_os_error()));
    }
    let length = output
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| invalid("bounded cold canonical path terminator"))?;
    charge(work, length, stop)?;
    let mut canonical = PathBuf::with_capacity(length);
    if canonical.capacity() > length {
        return Err(invalid("bounded cold canonical capacity"));
    }
    canonical.push(std::ffi::OsStr::from_bytes(&output[..length]));
    Ok(canonical)
}
impl Store {
    pub(super) fn reopen_existing_bounded(
        path: &Path,
        authority: &VerifiedAuthorityV1,
        role: EndpointRole,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> ResultV<Self> {
        #[cfg(not(unix))]
        {
            let _ = (path, authority, role, provider, policy, memory, work, stop);
            return Err(invalid("bounded cold reopen unsupported platform").into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            charge(work, 0, stop)?;
            let n = path.as_os_str().as_encoded_bytes().len();
            if n >= libc::PATH_MAX as usize {
                return Err(invalid("bounded cold path unsupported length/NUL").into());
            }
            // Retained whole open phase: Store/PathBuf shell, CString input,
            // canonical PathBuf, worst-case lossy UTF8 (3 bytes per source byte),
            // lock path, two escaped diagnostic buffers with geometric growth.
            // The fixed realpath buffer is stack storage, also conservatively paid.
            let cap = libc::PATH_MAX as usize;
            let phase = add(
                size_of::<Store>(),
                add(
                    add(n, 1)?,
                    add(
                        mul(6, cap)?,
                        add(
                            mul(12, add(cap, 64)?)?,
                            add(6 * size_of::<String>() + 3 * size_of::<PathBuf>(), cap)?,
                        )?,
                    )?,
                )?,
            )?;
            let phase = reserve(memory, phase)?.retain(());
            charge(work, 0, stop)?;
            let original = fs::symlink_metadata(path).map_err(io)?;
            if original.file_type().is_symlink() || !original.is_dir() {
                return Err(invalid("endpoint instance directory").into());
            }
            let canonical = canonical_directory(path, work, stop)?;
            charge(work, 0, stop)?;
            let metadata = fs::symlink_metadata(&canonical).map_err(io)?;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata.dev() != original.dev()
                || metadata.ino() != original.ino()
            {
                return Err(invalid("endpoint instance directory changed").into());
            }
            charge(
                work,
                mul(3, canonical.as_os_str().as_encoded_bytes().len())?,
                stop,
            )?;
            let lossy = canonical.to_string_lossy();
            let fingerprint = hash_encoded(
                &(lossy.as_ref(), metadata.dev(), metadata.ino()),
                None,
                memory,
                work,
                stop,
            )?;
            if !super::bounded_transaction::same(
                &policy.provider_public_key,
                &authority.policy().fencing_provider_public_key,
                memory,
                work,
                stop,
            )? || !super::bounded_transaction::same(
                &policy.authorized_resource_bindings,
                &authority.policy().authorized_fencing_resource_bindings,
                memory,
                work,
                stop,
            )? {
                return Err(FormalError::FencingUnproven.into());
            }
            let definition = authority.definition();
            let instance = match role {
                EndpointRole::Primary => &definition.primary.instance,
                EndpointRole::Replica => &definition.replica.instance,
            };
            if !super::bounded_transaction::same(
                &*fingerprint,
                &instance.storage_fingerprint,
                memory,
                work,
                stop,
            )? {
                return Err(FormalError::FencingUnproven.into());
            }
            // The VerifiedAuthority private constructor already proved definition
            // and selected instance constraints. Rehash its original exact wire;
            // do not re-run unmetered definition verification or mint authority.
            if definition.predecessor_authority_id.is_some()
                || authority.durable_set_activation_digest().is_some()
            {
                return Err(invalid("bounded cold recovery/durable transition unsupported").into());
            }
            let shadow_name = super::super::super::storage_shadow::SHADOW_MARKER;
            let shadow_cap = add(
                add(canonical.as_os_str().as_encoded_bytes().len(), 1)?,
                shadow_name.len(),
            )?;
            charge(work, shadow_cap, stop)?;
            let mut shadow = PathBuf::with_capacity(shadow_cap);
            if shadow.capacity() > shadow_cap {
                return Err(invalid("bounded cold shadow path capacity").into());
            }
            shadow.push(&canonical);
            shadow.push(shadow_name);
            charge(work, 0, stop)?;
            match fs::symlink_metadata(&shadow) {
                Ok(_) => {
                    return Err(invalid("formal shadow directory cannot open an endpoint").into());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e).into()),
            }
            let lock_name = ".formal-endpoint.lock";
            let lock_cap = add(
                add(canonical.as_os_str().as_encoded_bytes().len(), 1)?,
                lock_name.len(),
            )?;
            charge(work, lock_cap, stop)?;
            let mut lock_path = PathBuf::with_capacity(lock_cap);
            if lock_path.capacity() > lock_cap {
                return Err(invalid("bounded cold lock path capacity").into());
            }
            lock_path.push(&canonical);
            lock_path.push(lock_name);
            charge(work, 0, stop)?;
            let mut options = OpenOptions::new();
            options
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            let lock = options.open(&lock_path).map_err(io)?;
            charge(work, 0, stop)?;
            let lock_metadata = lock.metadata().map_err(io)?;
            if !lock_metadata.is_file() || lock_metadata.len() > LIMIT as u64 {
                return Err(invalid("formal storage requires regular file").into());
            }
            charge(work, 0, stop)?;
            lock.try_lock()
                .map_err(|_| FormalError::Conflict("formal endpoint locked".into()))?;
            let source = super::bounded_hot_read::read_hot_from_directory_bounded_v1(
                &canonical,
                &instance.endpoint_public_key,
                memory,
                work,
                stop,
            )?;
            let body = source.body();
            validate_cold_scope(body, authority, role, memory, work, stop)?;
            let (checkpoint, checkpoint_fee) =
                super::bounded_transaction::disk_binding(&source, memory, work, stop)?;
            let (body, body_fee) = super::bounded_transaction::copy_body(body, memory, work, stop)?;
            let mut store = Self {
                path: canonical,
                body,
                checkpoint,
                _lock: lock,
                sources: vec![],
                _bounded_cache_fee: Some(body_fee),
                _bounded_checkpoint_fee: Some(checkpoint_fee),
                _bounded_open_fee: Some(phase),
                #[cfg(test)]
                fault: None,
            };
            store.reconcile_pending_update_with_scope(
                provider,
                instance,
                policy,
                memory,
                work,
                stop,
                &super::bounded_transaction::RecoveryScope::Cold { authority, role },
            )?;
            charge(work, 0, stop)?;
            let final_metadata = fs::symlink_metadata(&store.path).map_err(io)?;
            if final_metadata.dev() != metadata.dev()
                || final_metadata.ino() != metadata.ino()
                || !final_metadata.is_dir()
                || final_metadata.file_type().is_symlink()
            {
                return Err(invalid("endpoint instance directory changed").into());
            }
            Ok(store)
        }
    }
}
// Private scope is derived only from the caller's already-verified authority.
// It applies independently to disk, pending intent and protected-current bytes.
pub(super) fn validate_cold_scope(
    body: &HotBody,
    authority: &VerifiedAuthorityV1,
    role: EndpointRole,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    let definition = authority.definition();
    let instance = match role {
        EndpointRole::Primary => &definition.primary.instance,
        EndpointRole::Replica => &definition.replica.instance,
    };
    let authority_id = hash_encoded(
        definition,
        Some(oasis7_formal_protocol::controlled_authority::formal::record::AUTHORITY_ID_DOMAIN_V1),
        memory,
        work,
        stop,
    )?;
    let instance_id = hash_encoded(instance, None, memory, work, stop)?;
    if body.version != 1
        || !super::bounded_transaction::same(
            &body.instance_digest,
            &*instance_id,
            memory,
            work,
            stop,
        )?
        || !super::bounded_transaction::same(
            &body.authority_id,
            &*authority_id,
            memory,
            work,
            stop,
        )?
        || body.epoch != definition.epoch
        || !super::bounded_transaction::same(
            &body.activation_ref.as_str(),
            &authority.activation_ref(),
            memory,
            work,
            stop,
        )?
    {
        return Err(invalid("formal checkpoint instance/authority/activation"));
    }
    charge(work, 4 * size_of::<usize>(), stop)?;
    if definition.predecessor_authority_id.is_some()
        || authority.durable_set_activation_digest().is_some()
        || body.import_digest.is_some()
        || body.imported_head_node.is_some()
        || body.base_parent.is_some()
        || body.durable_set.is_some()
    {
        return Err(invalid(
            "bounded cold imported/recovery/durable checkpoint unsupported",
        ));
    }
    Ok(())
}
// One-way initial registration material. Only this real authority/role producer
// can construct it; the remote RPC cannot accept caller preimages or bodies.
pub(in crate::controlled_authority) struct PaidColdRegistrationV1<'a> {
    value: ReservedValue<fencing::ProtectedEndpointRegistrationV1>,
    instance: &'a fencing::EndpointInstanceBindingV1,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    outer: ReservedValue<()>,
}
impl PaidColdRegistrationV1<'_> {
    pub(in crate::controlled_authority) fn value(
        &self,
    ) -> &fencing::ProtectedEndpointRegistrationV1 {
        &self.value
    }
    pub(in crate::controlled_authority) fn instance(&self) -> &fencing::EndpointInstanceBindingV1 {
        self.instance
    }
    pub(in crate::controlled_authority) fn validate(
        &self,
        ctx:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if self.identity != ctx.work().identity()
            || !self.value.uses_budget(ctx.memory())
            || !self.outer.uses_budget(ctx.memory())
        {
            return Err(invalid("bounded protected registration foreign context"));
        }
        Ok(())
    }
}
pub(super) fn cold_registration<'a>(
    authority: &'a VerifiedAuthorityV1,
    role: EndpointRole,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidColdRegistrationV1<'a>, FormalError> {
    let definition = authority.definition();
    if definition.predecessor_authority_id.is_some()
        || authority.durable_set_activation_digest().is_some()
    {
        return Err(invalid(
            "bounded cold recovery/durable transition unsupported",
        ));
    }
    let instance = match role {
        EndpointRole::Primary => &definition.primary.instance,
        EndpointRole::Replica => &definition.replica.instance,
    };
    let instance_id = hash_encoded(instance, None, memory, work, stop)?;
    let authority_id = hash_encoded(
        definition,
        Some(oasis7_formal_protocol::controlled_authority::formal::record::AUTHORITY_ID_DOMAIN_V1),
        memory,
        work,
        stop,
    )?;
    let genesis = hash_encoded(
        &definition.world,
        Some(oasis7_formal_protocol::controlled_authority::formal::record::GENESIS_HEAD_DOMAIN_V1),
        memory,
        work,
        stop,
    )?;
    let payload = add(192, authority.activation_ref().len())?;
    let body_fee = reserve(memory, add(size_of::<HotBody>(), payload)?)?;
    charge(work, payload, stop)?;
    let body = HotBody {
        version: 1,
        instance_digest: (*instance_id).clone(),
        authority_id: (*authority_id).clone(),
        activation_ref: authority.activation_ref().to_owned(),
        epoch: definition.epoch,
        revision: 0,
        head: FormalHeadRefV1 {
            position: 0,
            record_id: (*genesis).clone(),
        },
        head_node: None,
        imported_head_node: None,
        import_digest: None,
        base_parent: None,
        index: None,
        pending: None,
        decision_head: None,
        seal: None,
        data_closure: None,
        durable_set: None,
    };
    super::bounded_native_draft::visit_body(&body, &mut |s| {
        if s.capacity() > s.len() {
            return Err(invalid("bounded registration body capacity"));
        }
        Ok(())
    })?;
    let body = body_fee.retain(body);
    let mut ctx = work.bounded_context(memory, stop);
    let wire = super::context_wire::encode_paid(
        &*body,
        LIMIT,
        &mut ctx,
        "formal encoding exceeds byte bound",
        "bounded protected registration body",
    )?;
    let frame = add(add(8, DOMAIN.len())?, wire.len())?;
    if frame > 64 * 1024 {
        return Err(invalid("bounded protected registration preimage bound"));
    }
    let layout = add(
        add(size_of::<fencing::ProtectedEndpointRegistrationV1>(), frame)?,
        add(64, authority.activation_ref().len())?,
    )?;
    let fee = reserve(memory, layout)?;
    let mut preimage = Vec::with_capacity(frame);
    if preimage.capacity() > frame {
        return Err(invalid("bounded registration preimage capacity"));
    }
    ctx.charge(1, add(8, DOMAIN.len())? as u64)?;
    preimage.extend_from_slice(&(DOMAIN.len() as u64).to_be_bytes());
    preimage.extend_from_slice(DOMAIN.as_bytes());
    for chunk in wire.chunks(8192) {
        ctx.charge(1, chunk.len() as u64)?;
        preimage.extend_from_slice(chunk);
    }
    ctx.charge(1, add(64, authority.activation_ref().len())? as u64)?;
    let value = fencing::ProtectedEndpointRegistrationV1 {
        authority_id: (*authority_id).clone(),
        activation_ref: authority.activation_ref().to_owned(),
        unsigned_checkpoint_preimage: preimage,
        existing_checkpoint: None,
    };
    if value.authority_id.capacity() > 64
        || value.activation_ref.capacity() > value.activation_ref.len()
    {
        return Err(invalid("bounded registration identity capacity"));
    }
    let outer = reserve(
        memory,
        size_of::<PaidColdRegistrationV1<'a>>()
            .checked_sub(size_of::<fencing::ProtectedEndpointRegistrationV1>())
            .ok_or_else(|| invalid("bounded registration shell"))?,
    )?
    .retain(());
    Ok(PaidColdRegistrationV1 {
        value: fee.retain(value),
        instance,
        identity: ctx.work().identity(),
        outer,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_reopen_tests.rs"]
mod tests;
