//! Shared original immutable staging I/O, beneath concrete paid producers.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::formal::physical;
use oasis7_proto::memory_budget::MemoryBudget;
use std::io::Write;
pub(super) const TEMP_NAME_CAP: usize = ".formal-stage-".len() + 10 + 1 + 20;
pub(super) fn write_payload_bounded(
    writer: &mut impl Write,
    bytes: &[u8],
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    let mut offset = 0;
    while offset < bytes.len() {
        let end = (offset + 8192).min(bytes.len());
        work.consume_read_bytes((end - offset) as u64, cancelled)?;
        match writer.write(&bytes[offset..end]) {
            Ok(0) => return Err(io(std::io::Error::from(std::io::ErrorKind::WriteZero))),
            Ok(n) if n <= end - offset => offset += n,
            Ok(_) => return Err(invalid("bounded stage writer exceeded supplied slice")),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(error)),
        }
    }
    Ok(())
}
pub(super) fn join_paid(base: &Path, name: &str, capacity: usize) -> Result<PathBuf, FormalError> {
    let mut path = PathBuf::with_capacity(capacity);
    if path.capacity() > capacity {
        return Err(invalid("bounded stage path capacity"));
    }
    path.push(base);
    path.push(name);
    if path.capacity() > capacity {
        return Err(invalid("bounded stage path growth"));
    }
    Ok(path)
}

pub(super) fn stage_wire_with_layout(
    guard: &physical::PublicationGuard,
    wire: &[u8],
    max_wire: usize,
    child_cap: usize,
    temp_name_cap: usize,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<String, FormalError> {
    work.consume_steps(1, cancelled)?;
    let directory = guard.initialized_root()?;
    let expected = directory
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(1 + 64usize.max(TEMP_NAME_CAP))
        .ok_or_else(|| invalid("bounded stage child layout"))?;
    // The caller may have prepaid a larger capacity: pushing a component
    // onto an equivalent trailing-separator directory can omit one byte.
    // Reject insufficient admission, not safe unused reserved capacity.
    if child_cap < expected
        || temp_name_cap != TEMP_NAME_CAP
        || wire.len() > max_wire
        || max_wire > LIMIT
    {
        return Err(invalid("bounded stage core layout"));
    }
    work.consume_read_bytes(wire.len() as u64, cancelled)?;
    let id = blake3::hash(wire).to_hex().to_string();
    if id.capacity() > 64 {
        return Err(invalid("bounded stage digest capacity"));
    }
    let target = join_paid(&directory, &id, child_cap)?;
    let existing = super::super::archive_bounded_read::read_local_archive_bounded_v1(
        &directory, &id, max_wire, memory, work, cancelled,
    )?;
    if let Some(existing) = existing {
        if existing.as_slice().len() != wire.len() {
            return Err(invalid("formal immutable object changed"));
        }
        for (old, new) in existing.as_slice().chunks(8192).zip(wire.chunks(8192)) {
            work.consume_read_bytes(old.len() as u64, cancelled)?;
            if old != new {
                return Err(invalid("formal immutable object changed"));
            }
        }
    } else {
        let tick = TEMP
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| invalid("bounded stage temporary counter overflow"))?;
        let mut name = String::with_capacity(temp_name_cap);
        if name.capacity() > temp_name_cap {
            return Err(invalid("bounded stage temporary capacity"));
        }
        // Fixed u32/u64 decimal formatting needs at most the prepaid 10/20
        // digits, so this write cannot grow the admitted String.
        std::fmt::Write::write_fmt(
            &mut name,
            format_args!(".formal-stage-{}-{tick}", std::process::id()),
        )
        .map_err(|_| invalid("bounded stage temporary formatting"))?;
        if name.capacity() > temp_name_cap {
            return Err(invalid("bounded stage temporary growth"));
        }
        let temp = join_paid(&directory, &name, child_cap)?;
        physical::paths::parents_with_step(&target, &mut || work.consume_steps(1, cancelled))?;
        physical::paths::directory_with_step(&directory, &mut || work.consume_steps(1, cancelled))?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        work.consume_steps(1, cancelled)?;
        let mut file = options.open(&temp).map_err(io)?;
        write_payload_bounded(&mut file, wire, work, cancelled)?;
        work.consume_steps(1, cancelled)?;
        file.sync_all().map_err(io)?;
        work.consume_steps(1, cancelled)?;
        fs::hard_link(&temp, &target).map_err(io)?;
        work.consume_steps(1, cancelled)?;
        fs::remove_file(&temp).map_err(io)?;
        physical::paths::sync_with_step(&directory, &mut || work.consume_steps(1, cancelled))?;
    }
    work.consume_steps(1, cancelled)?;
    Ok(id)
}

#[cfg(test)]
#[path = "endpoint_store_stage_core_tests.rs"]
mod tests;
