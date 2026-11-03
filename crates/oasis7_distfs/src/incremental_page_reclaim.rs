//! Conservative physical sweep: every recipe is a root, including provisional pages.
use super::*;
use std::{collections::BTreeSet, io::Write};

pub(super) const RETIRE: &str = "orphan-retire";
const INTENT: &[u8] = b"oasis7.iws.orphan-frame-retirement.v1\n";

pub(super) fn check_publication(root: &Path) -> Result<()> {
    if files::exists(&root.join(RETIRE))? {
        return Err(PageStoreError::ResourceLimited(
            "physical retirement pending",
        ));
    }
    Ok(())
}

/// Memory admission bounds all inventories and the frame mark set. Enumeration
/// and reference counters are spent continuously, including failed attempts.
pub struct PageReclaimWork {
    pub max_entries: usize,
    pub entries_remaining: u64,
    pub references_remaining: u64,
    pub object_logical: oasis7_formal_protocol::incremental_value::ValueIoBudget,
    pub max_object_bytes: u64,
}
#[derive(Debug, Default)]
pub struct PageReclaimReport {
    pub verified_pages: u64,
    pub verified_objects: u64,
    pub unlinked_frames: u64,
    /// Logical stored member bytes, not freed filesystem blocks/shared extents.
    pub unlinked_member_bytes: u64,
    pub directory_synced: bool,
}
#[derive(Debug)]
pub enum PageReclaimFailure {
    NotStarted(PageStoreError),
    /// Retirement may be durable even when no unlink has completed. New puts
    /// remain fenced until a subsequent complete revalidation finishes.
    Incomplete {
        report: PageReclaimReport,
        cause: PageStoreError,
    },
    /// All safe frame removals finished. Marker removal/root sync or its final
    /// cancellation failed; the marker may be absent and puts may be allowed.
    Finalization {
        report: PageReclaimReport,
        cause: PageStoreError,
    },
}
fn spend(value: &mut u64, message: &'static str) -> Result<()> {
    *value = value
        .checked_sub(1)
        .ok_or(PageStoreError::ResourceLimited(message))?;
    Ok(())
}
fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn names(root: &Path, work: &mut PageReclaimWork, cancelled: &AtomicBool) -> Result<Vec<[u8; 32]>> {
    let mut result = Vec::with_capacity(work.max_entries);
    for entry in fs::read_dir(root)? {
        check_cancelled(cancelled)?;
        spend(&mut work.entries_remaining, "physical inventory entries")?;
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or(PageStoreError::Corrupt("physical inventory name"))?;
        if name.len() != 64 || result.len() == work.max_entries {
            return Err(PageStoreError::ResourceLimited(
                "physical inventory bound/name",
            ));
        }
        let mut id = [0; 32];
        hex::decode_to_slice(name, &mut id)
            .map_err(|_| PageStoreError::Corrupt("physical inventory digest"))?;
        if hex::encode(id) != name || !fs::symlink_metadata(entry.path())?.file_type().is_file() {
            return Err(PageStoreError::Corrupt("physical inventory type/digest"));
        }
        result.push(id);
    }
    result.sort_unstable();
    Ok(result)
}
fn scope(root: &Path, work: &mut PageReclaimWork, cancelled: &AtomicBool) -> Result<()> {
    for entry in fs::read_dir(root)? {
        check_cancelled(cancelled)?;
        spend(&mut work.entries_remaining, "physical inventory entries")?;
        let entry = entry?;
        if !matches!(
            entry.file_name().to_str(),
            Some(
                "format"
                    | "lease"
                    | "frames"
                    | "recipes"
                    | "staging"
                    | "objects"
                    | "objects-format"
                    | RETIRE
            )
        ) {
            return Err(PageStoreError::Corrupt("unknown physical store member"));
        }
    }
    if fs::read_dir(root.join("staging"))?
        .next()
        .transpose()?
        .is_some()
    {
        return Err(PageStoreError::ResourceLimited(
            "physical staging owner unresolved",
        ));
    }
    Ok(())
}
fn retirement(
    root: &Path,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
    fenced: &mut bool,
) -> Result<()> {
    let path = root.join(RETIRE);
    if files::exists(&path)? {
        *fenced = true;
        let bytes = files::read(&path, INTENT.len(), io, cancelled)?;
        if !INTENT.starts_with(&bytes) {
            return Err(PageStoreError::Corrupt("physical retirement domain"));
        }
    }
    // A crash during initialization leaves a recognized prefix at this exact
    // fixed pathname. Puts fence on existence, not on successfully decoding it.
    io.write(INTENT.len())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(all(test, unix))]
    if FAIL_RETIRE_OPEN.with(|flag| flag.replace(false)) {
        return Err(io::Error::from_raw_os_error(libc::ENOSPC).into());
    }
    let mut file = options.open(&path)?;
    *fenced = true;
    file.write_all(INTENT)?;
    file.sync_all()?;
    files::sync(root)
}
/// Requires quiescence of this page store's shared leases. Never retires a
/// logical page, recipe, staging job or a necessary independent replica.
pub fn reclaim_orphan_page_frames(
    root: &Path,
    memory: &MemoryBudget,
    work: &mut PageReclaimWork,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
) -> std::result::Result<PageReclaimReport, PageReclaimFailure> {
    let mut report = PageReclaimReport {
        directory_synced: true,
        ..Default::default()
    };
    let preflight = (|| -> Result<_> {
        require_platform()?;
        check_cancelled(cancelled)?;
        let charge = admit_path(root, memory, MemoryClass::Ordinary)?;
        files::validate_store_io(root, io, cancelled)?;
        let lease = files::open_regular(&root.join("lease"))?;
        lease.try_lock().map_err(|error| match error {
            fs::TryLockError::WouldBlock => {
                PageStoreError::ResourceLimited("physical retirement lease busy")
            }
            fs::TryLockError::Error(error) => PageStoreError::Io(error),
        })?;
        files::validate_lease(root, &lease)?;
        let metadata_bytes = work
            .max_entries
            .checked_mul(256)
            .and_then(|n| n.checked_add(RAW_WORKSPACE))
            .ok_or(PageStoreError::ResourceLimited(
                "physical inventory admission",
            ))?;
        let admission = memory.reserve(metadata_bytes, MemoryClass::Ordinary)?;
        scope(root, work, cancelled)?;
        let store = IncrementalPageStore {
            root: root.to_owned(),
            memory: memory.clone(),
            compression: PageCompression::Raw,
            _lease: lease,
            _charge: charge,
        };
        let object_directory = present(&root.join("objects"))?;
        let object_capability = present(&root.join("objects-format"))?;
        if object_directory != object_capability {
            return Err(PageStoreError::Corrupt(
                "partial original object capability",
            ));
        }
        if object_directory {
            let objects = objects::IncrementalObjectStore::open(
                &store,
                MemoryClass::Ordinary,
                io,
                cancelled,
            )?;
            for id in names(&root.join("objects"), work, cancelled)? {
                objects.read_object_into(
                    &id,
                    work.max_object_bytes,
                    &mut std::io::sink(),
                    &mut objects::ObjectIo {
                        physical: io,
                        logical: &mut work.object_logical,
                        class: MemoryClass::Ordinary,
                        cancelled,
                    },
                )?;
                report.verified_objects += 1;
            }
        }
        let mut marked = BTreeSet::new();
        for id in names(&root.join("recipes"), work, cancelled)? {
            // Reuse the real reader: verify original bytes, frames, pack
            // members, lengths and hashes rather than trusting recipe edges.
            drop(store.read_page(
                &id,
                VALUE_MAX_NODE_BYTES,
                MemoryClass::Ordinary,
                io,
                cancelled,
            )?);
            let bytes = files::read(
                &root.join("recipes").join(hex::encode(id)),
                recipe::MAX_RECIPE_BYTES,
                io,
                cancelled,
            )?;
            let (_, frames) = recipe::decode(&id, &bytes)?;
            for frame in frames {
                spend(&mut work.references_remaining, "physical frame references")?;
                if !marked.contains(&frame.stored) && marked.len() == work.max_entries {
                    return Err(PageStoreError::ResourceLimited("physical frame marks"));
                }
                marked.insert(frame.stored);
            }
            report.verified_pages += 1;
        }
        let mut orphans = Vec::with_capacity(work.max_entries);
        for id in names(&root.join("frames"), work, cancelled)? {
            if marked.contains(&id) {
                continue;
            }
            let bytes = files::read(
                &root.join("frames").join(hex::encode(id)),
                frame_codec::FRAME_BYTES,
                io,
                cancelled,
            )?;
            if bytes.is_empty() || blake3::hash(&bytes).as_bytes() != &id {
                return Err(PageStoreError::Corrupt("unrecognized orphan frame"));
            }
            orphans.push((id, bytes.len() as u64));
        }
        // Admission for the marker, all removals/syncs and final marker removal
        // occurs before retirement; no budget exhaustion starts a partial sweep.
        let resuming = files::exists(&root.join(RETIRE))?;
        let writes = (orphans.len() as u64)
            .checked_mul(2)
            .and_then(|n| n.checked_add(3 + u64::from(resuming)))
            .ok_or(PageStoreError::ResourceLimited(
                "physical retirement writes",
            ))?;
        let needs_retirement = !orphans.is_empty() || resuming;
        if needs_retirement
            && (io.writes_remaining < writes || io.write_bytes_remaining < INTENT.len() as u64)
        {
            return Err(PageStoreError::ResourceLimited(
                "physical retirement writes",
            ));
        }
        check_cancelled(cancelled)?;
        Ok((store, admission, orphans, needs_retirement, resuming))
    })()
    .map_err(PageReclaimFailure::NotStarted)?;
    let (store, _admission, orphans, needs_retirement, resuming) = preflight;
    if !needs_retirement {
        return Ok(report);
    }
    let mut fenced = resuming;
    report.directory_synced = !resuming;
    let sweep = (|| -> Result<()> {
        retirement(root, io, cancelled, &mut fenced)?;
        if resuming {
            report.directory_synced = false;
            io.write(0)?;
            files::sync(&root.join("frames"))?;
            report.directory_synced = true;
        }
        for (id, bytes) in orphans {
            check_cancelled(cancelled)?;
            io.write(0)?;
            fs::remove_file(root.join("frames").join(hex::encode(id)))?;
            report.unlinked_frames += 1;
            report.unlinked_member_bytes += bytes;
            report.directory_synced = false;
            io.write(0)?;
            files::sync(&root.join("frames"))?;
            report.directory_synced = true;
        }
        check_cancelled(cancelled)?;
        Ok(())
    })();
    if let Err(cause) = sweep {
        return Err(if fenced {
            PageReclaimFailure::Incomplete { report, cause }
        } else {
            PageReclaimFailure::NotStarted(cause)
        });
    }
    let finalization = (|| -> Result<()> {
        io.write(0)?;
        fs::remove_file(root.join(RETIRE))?;
        report.directory_synced = false;
        io.write(0)?;
        files::sync(root)?;
        report.directory_synced = true;
        check_cancelled(cancelled)
    })();
    drop(store);
    match finalization {
        Ok(()) => Ok(report),
        Err(cause) => Err(PageReclaimFailure::Finalization { report, cause }),
    }
}

#[cfg(all(test, unix))]
#[path = "incremental_page_reclaim_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
thread_local! { static FAIL_RETIRE_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
