//! Call-local raw/legacy-pack reads. File identity groups freshly opened aliases;
//! each requested payload is still independently length-checked and hashed.
use super::*;
const OUTPUT: usize = 8 * 1024 * 1024;
#[cfg(all(test, unix))]
thread_local! {
    static COUNTS: std::cell::Cell<(usize, usize, usize)> = const { std::cell::Cell::new((0, 0, 0)) };
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn legacy_fifo_alias_is_rejected_without_blocking_open() {
        const CHILD_ROOT: &str = "OASIS7_TEST_LEGACY_FIFO_ROOT";
        let id = "00".repeat(32);
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let path = std::path::PathBuf::from(root).join(&id);
            assert!(read_durable_batch(&[(path, id, 1)]).is_err());
            return;
        }
        let root = Root::new();
        let path = root.0.join(&id);
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: name is a live NUL-terminated path in our owned test directory.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        struct Probe(std::process::Child);
        impl Drop for Probe {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let test = std::thread::current().name().unwrap().to_owned();
        let mut child = Probe(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &test])
                .env(CHILD_ROOT, &root.0)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "FIFO rejection child failed: {status}");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "FIFO alias open blocked"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    struct Root(std::path::PathBuf);
    impl Root {
        fn new() -> Self {
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "oasis7-legacy-batch-{}",
                crate::controlled_authority::fencing::fresh_challenge().unwrap()
            ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn legacy_pack_retry_shares_header_and_fsync_but_next_call_rechecks_payload() {
        let root = Root::new();
        let mut writes = (0u16..256)
            .map(|index| {
                let mut bytes = vec![17; 512];
                bytes[..2].copy_from_slice(&index.to_be_bytes());
                BusinessIndexNodeWriteV1 {
                    digest: blake3::hash(&bytes).to_hex().to_string(),
                    bytes,
                }
            })
            .collect::<Vec<_>>();
        writes.sort_by(|left, right| left.digest.cmp(&right.digest));
        assert!(!crate::controlled_authority::formal::physical::enabled(&root.0).unwrap());
        super::super::persist(&root.0, &writes).unwrap();
        let first = fs::metadata(root.0.join(&writes[0].digest)).unwrap();
        assert!(writes.iter().all(|write| {
            let metadata = fs::metadata(root.0.join(&write.digest)).unwrap();
            (metadata.dev(), metadata.ino()) == (first.dev(), first.ino())
        }));
        COUNTS.with(|counts| counts.set((0, 0, 0)));
        super::super::persist(&root.0, &writes).unwrap();
        COUNTS.with(|counts| assert_eq!(counts.get(), (1, 1, 1)));
        let path = root.0.join(&writes[0].digest);
        let span = super::super::open(&path, &writes[0].digest)
            .unwrap()
            .unwrap();
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(span.start)).unwrap();
        file.write_all(&[255]).unwrap();
        assert_eq!(file.metadata().unwrap().ino(), first.ino());
        assert!(super::super::persist(&root.0, &writes).is_err());
    }
    #[test]
    fn raw_groups_flush_at_sixteen_and_validate_individual_lengths() {
        let root = Root::new();
        let requests = (0u8..17)
            .map(|index| {
                let bytes = vec![index; 32];
                let id = blake3::hash(&bytes).to_hex().to_string();
                let path = root.0.join(&id);
                fs::write(&path, bytes).unwrap();
                (path, id, 32)
            })
            .collect::<Vec<_>>();
        COUNTS.with(|counts| counts.set((0, 0, 0)));
        assert!(
            read_durable_batch(&requests)
                .unwrap()
                .iter()
                .all(Option::is_some)
        );
        COUNTS.with(|counts| assert_eq!(counts.get(), (17, 17, 16)));
        let mut wrong = requests[..1].to_vec();
        wrong[0].2 = 8;
        assert!(read_durable_batch(&wrong).is_err());
        assert!(read_durable_batch(&vec![requests[0].clone(); 1025]).is_err());
        wrong[0].2 = OUTPUT + 1;
        assert!(read_durable_batch(&wrong).is_err());
        fs::remove_file(&requests[0].0).unwrap();
        std::os::unix::fs::symlink(&requests[1].0, &requests[0].0).unwrap();
        assert!(read_durable_batch(&requests[..1]).is_err());
    }
    #[test]
    fn magic_raw_compatibility_keeps_sparse_length_preflight() {
        let root = Root::new();
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&[0; 24]);
        let id = blake3::hash(&bytes).to_hex().to_string();
        let path = root.0.join(&id);
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            read_durable_batch(&[(path.clone(), id.clone(), bytes.len())]).unwrap(),
            vec![Some(bytes)]
        );
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_LOGICAL)
            .unwrap();
        let error = read_durable_batch(&[(path, id, 8)]).unwrap_err();
        assert!(error.to_string().contains("expected logical length"));
    }
}

pub(super) fn read_durable_batch(
    requests: &[(std::path::PathBuf, String, usize)],
) -> Result<Vec<Option<Vec<u8>>>, FormalError> {
    if requests.len() > 1024
        || requests
            .iter()
            .try_fold(0usize, |sum, (_, _, size)| sum.checked_add(*size))
            .is_none_or(|sum| sum > OUTPUT)
    {
        return Err(invalid("legacy batch count/output budget"));
    }
    #[cfg(unix)]
    {
        unix::read_batch(requests)
    }
    #[cfg(not(unix))]
    requests
        .iter()
        .map(|(path, id, length)| read_legacy_durable_expected(path, id, *length))
        .collect()
}

#[cfg(unix)]
mod unix {
    use super::*;
    #[derive(Clone, Copy)]
    enum Layout {
        Raw,
        Pack { count: usize, index: u64 },
    }
    struct Anchor {
        file: File,
        identity: (u64, u64),
        size: u64,
        layout: Layout,
        members: Vec<(usize, String, usize)>,
    }
    fn open_alias(path: &Path) -> Result<Option<File>, FormalError> {
        super::super::super::physical::paths::parents(path)?;
        match fs::symlink_metadata(path) {
            Ok(metadata) => super::super::super::physical::paths::metadata(&metadata, false)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        }
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        };
        let metadata = file.metadata().map_err(io)?;
        super::super::super::physical::paths::metadata(&metadata, false)?;
        if metadata.len() > MAX_PACK {
            return Err(invalid("legacy batch physical byte bound"));
        }
        Ok(Some(file))
    }
    fn layout(file: &mut File, size: u64) -> Result<Layout, FormalError> {
        let mut prefix = vec![0; size.min(MAGIC.len() as u64) as usize];
        file.read_exact(&mut prefix).map_err(io)?;
        if prefix != MAGIC {
            return Ok(Layout::Raw);
        }
        let mut rest = [0u8; 24];
        if file.read_exact(&mut rest).is_err() {
            return Ok(Layout::Raw);
        }
        let version = u32::from_be_bytes(rest[0..4].try_into().unwrap());
        let count = u32::from_be_bytes(rest[4..8].try_into().unwrap()) as usize;
        let index = u64::from_be_bytes(rest[8..16].try_into().unwrap());
        let total = u64::from_be_bytes(rest[16..24].try_into().unwrap());
        let length = (count as u64).checked_mul(INDEX_ENTRY);
        if version != 1
            || count == 0
            || count > MAX_ENTRIES
            || total != size
            || index < HEADER as u64
            || length.and_then(|length| index.checked_add(length)) != Some(total)
        {
            return Ok(Layout::Raw);
        }
        Ok(Layout::Pack { count, index })
    }
    fn locate(anchor: &mut Anchor, id: &str, expected: usize) -> Result<u64, FormalError> {
        if let Layout::Pack { count, index } = anchor.layout {
            let wanted = hex::decode(id).map_err(|_| invalid("legacy batch digest"))?;
            let (mut low, mut high) = (0, count);
            while low < high {
                let middle = low + (high - low) / 2;
                anchor
                    .file
                    .seek(SeekFrom::Start(index + middle as u64 * INDEX_ENTRY))
                    .map_err(io)?;
                let mut row = [0u8; INDEX_ENTRY as usize];
                anchor.file.read_exact(&mut row).map_err(io)?;
                match row[..32].cmp(&wanted) {
                    std::cmp::Ordering::Less => low = middle + 1,
                    std::cmp::Ordering::Greater => high = middle,
                    std::cmp::Ordering::Equal => {
                        let start = u64::from_be_bytes(row[32..40].try_into().unwrap());
                        let length = u32::from_be_bytes(row[40..44].try_into().unwrap()) as u64;
                        if start < HEADER as u64
                            || length == 0
                            || length > MAX_PACK_NODE
                            || start.checked_add(length).is_none_or(|end| end > index)
                        {
                            return Err(invalid("legacy batch pack member span"));
                        }
                        if length != expected as u64 {
                            return Err(invalid("legacy batch expected logical length"));
                        }
                        return Ok(start);
                    }
                }
            }
        }
        // The marker may be original raw bytes, including an apparently valid
        // pack lacking this ID. Check length BEFORE any raw payload scan/alloc.
        if anchor.size != expected as u64 || anchor.size > MAX_LOGICAL {
            return Err(invalid("legacy batch expected logical length"));
        }
        Ok(0)
    }
    fn finish(mut anchor: Anchor, output: &mut [Option<Vec<u8>>]) -> Result<(), FormalError> {
        for (index, id, expected) in std::mem::take(&mut anchor.members) {
            let start = locate(&mut anchor, &id, expected)?;
            let mut bytes = vec![0; expected];
            anchor
                .file
                .seek(SeekFrom::Start(start))
                .and_then(|_| anchor.file.read_exact(&mut bytes))
                .map_err(io)?;
            if blake3::hash(&bytes).to_hex().as_str() != id {
                return Err(invalid("legacy batch logical checksum"));
            }
            output[index] = Some(bytes);
        }
        let after = anchor.file.metadata().map_err(io)?;
        super::super::super::physical::paths::metadata(&after, false)?;
        if after.len() != anchor.size {
            return Err(invalid("legacy batch anchor length changed"));
        }
        anchor.file.sync_all().map_err(io)?;
        #[cfg(test)]
        COUNTS.with(|counts| {
            let (headers, syncs, maximum) = counts.get();
            counts.set((headers, syncs + 1, maximum));
        });
        Ok(())
    }
    pub(super) fn read_batch(
        requests: &[(std::path::PathBuf, String, usize)],
    ) -> Result<Vec<Option<Vec<u8>>>, FormalError> {
        let mut output = vec![None; requests.len()];
        let mut anchors: Vec<Anchor> = Vec::new();
        for (index, (path, id, expected)) in requests.iter().enumerate() {
            hash_hex(id)?;
            // Only this temporary probe can coexist with 16 retained anchors.
            let Some(mut file) = open_alias(path)? else {
                continue;
            };
            let metadata = file.metadata().map_err(io)?;
            let identity = (metadata.dev(), metadata.ino());
            if let Some(anchor) = anchors
                .iter_mut()
                .find(|anchor| anchor.identity == identity)
            {
                if anchor.size != metadata.len() {
                    return Err(invalid("legacy batch alias length changed"));
                }
                anchor.members.push((index, id.clone(), *expected));
                continue;
            }
            if anchors.len() == 16 {
                for anchor in anchors.drain(..) {
                    finish(anchor, &mut output)?;
                }
            }
            let layout = layout(&mut file, metadata.len())?;
            #[cfg(test)]
            COUNTS.with(|counts| {
                let (headers, syncs, maximum) = counts.get();
                counts.set((headers + 1, syncs, maximum.max(anchors.len() + 1)));
            });
            anchors.push(Anchor {
                file,
                identity,
                size: metadata.len(),
                layout,
                members: vec![(index, id.clone(), *expected)],
            });
        }
        for anchor in anchors {
            finish(anchor, &mut output)?;
        }
        Ok(output)
    }
}
