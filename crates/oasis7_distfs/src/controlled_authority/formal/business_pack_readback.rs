//! Independent physical readback with run-local fsync amortization.
use super::*;
/// Registered-local independent readback under the endpoint/storage lease.
/// Hashes every logical object, but syncs a physical pack only once in this run.
/// Markers are ephemeral deduplication; they never authorize logical contents.
#[derive(Default, Clone)]
struct ReadbackTrace {
    reads: u64,
    bytes: u64,
    open_us: u128,
    read_hash_us: u128,
    sync_us: u128,
    syncs: u64,
}
thread_local! {
    static READBACK_TRACE: std::cell::RefCell<Option<ReadbackTrace>> = const { std::cell::RefCell::new(None) };
}
pub(crate) fn begin_readback_trace(enabled: bool) {
    READBACK_TRACE.with(|trace| *trace.borrow_mut() = enabled.then(ReadbackTrace::default));
}
pub(crate) fn finish_readback_trace() {
    READBACK_TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().take() {
            eprintln!("healthy-native-readback reads={} bytes={} open_us={} read_hash_us={} syncs={} sync_us={}",
                trace.reads, trace.bytes, trace.open_us, trace.read_hash_us, trace.syncs, trace.sync_us);
        }
    });
}
pub(crate) fn read_object_durable_once(
    path: &Path,
    id: &str,
    sync_markers: &Path,
) -> Result<Option<Vec<u8>>, FormalError> {
    let _pin = super::super::physical::pin(
        path.parent().ok_or_else(|| invalid("logical CAS parent"))?,
        false,
    )?;
    let selected_path = super::super::physical::selected_path(path, id)?;
    let path = selected_path.as_path();
    if let Some(bytes) = super::super::physical::read_object_durable_once(path, id, sync_markers)? {
        return Ok(Some(bytes));
    }
    let tracing = READBACK_TRACE.with(|trace| trace.borrow().is_some());
    let started = tracing.then(std::time::Instant::now);
    let Some(mut object) = open(path, id)? else {
        return Ok(None);
    };
    #[cfg(unix)]
    {
        let metadata = object.file.metadata().map_err(io)?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(invalid("independent readback registered owner/write mode"));
        }
    }
    let open_us = started
        .map(|started| started.elapsed().as_micros())
        .unwrap_or(0);
    let started = tracing.then(std::time::Instant::now);
    let bytes = read_open_object(&mut object, id)?;
    let read_hash_us = started
        .map(|started| started.elapsed().as_micros())
        .unwrap_or(0);
    READBACK_TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.reads += 1;
            trace.bytes += bytes.len() as u64;
            trace.open_us += open_us;
            trace.read_hash_us += read_hash_us;
        }
    });
    #[cfg(unix)]
    let identity = {
        let metadata = object.file.metadata().map_err(io)?;
        format!("physical-{}-{}", metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let identity = format!("physical-{id}");
    let marker = sync_markers.join(identity);
    // Same-user mutation while the exclusive registered lease is held is outside
    // the storage contract. The descriptor is no-follow, bounded and hash checked.
    let synced = match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => true,
        Ok(_) => return Err(invalid("independent readback sync marker is not regular")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(io(error)),
    };
    if !synced {
        let started = tracing.then(std::time::Instant::now);
        object.file.sync_all().map_err(io)?;
        READBACK_TRACE.with(|trace| {
            if let Some(trace) = trace.borrow_mut().as_mut() {
                trace.syncs += 1;
                trace.sync_us += started
                    .map(|started| started.elapsed().as_micros())
                    .unwrap_or(0);
            }
        });
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        options.open(marker).map_err(io)?;
    }
    Ok(Some(bytes))
}
