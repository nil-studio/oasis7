//! One bounded logical read unit. No descriptor, frame or inode evidence is
//! reused by a later call. The caller retains the root retirement pin.
use super::*;
const OUTPUT: usize = 8 * 1024 * 1024;
const COUNT: usize = 1024;
#[cfg(test)]
thread_local! {
    static COUNTS: std::cell::Cell<(usize, usize, usize)> = const { std::cell::Cell::new((0, 0, 0)) };
}

/// Paths have already been resolved under the caller's live root pin. `None`
/// means no physical selector; the caller must use the ordinary legacy reader.
pub(crate) fn read_durable_batch(
    root: &Path,
    requests: &[(PathBuf, String, usize)],
) -> Result<Vec<Option<Vec<u8>>>, FormalError> {
    if requests.len() > COUNT
        || requests
            .iter()
            .try_fold(0usize, |sum, (_, _, size)| sum.checked_add(*size))
            .is_none_or(|sum| sum > OUTPUT)
    {
        return Err(invalid("physical batch count/output budget"));
    }
    let _pin = pin(root, false)?;
    #[cfg(unix)]
    {
        unix::read_batch(requests)
    }
    #[cfg(not(unix))]
    requests
        .iter()
        .map(|(path, id, expected)| {
            let Some((_, recipe)) = selector(path, id)? else {
                return Ok(None);
            };
            let index = recipe
                .objects
                .binary_search_by(|object| object.id.as_str().cmp(id))
                .map_err(|_| invalid("physical batch selector member absent"))?;
            if recipe.objects[index].length != *expected as u64 {
                return Err(invalid("physical batch logical length differs"));
            }
            read_object(path, id, true)
        })
        .collect()
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{Seek, SeekFrom};
    struct Member {
        index: usize,
        id: String,
        start: u64,
        length: usize,
    }
    struct Anchor {
        root: PathBuf,
        file: File,
        identity: (u64, u64),
        size: u64,
        recipe: std::sync::Arc<Recipe>,
        members: Vec<Member>,
    }
    fn open_selector(path: &Path, id: &str) -> Result<Option<(PathBuf, File)>, FormalError> {
        hash_hex(id)?;
        let root = path
            .parent()
            .ok_or_else(|| invalid("physical batch root"))?;
        let selected = root.join(SELECTORS).join(id);
        paths::parents(path)?;
        match fs::symlink_metadata(&selected) {
            Ok(metadata) => paths::metadata(&metadata, false)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        }
        paths::parents(&selected)?;
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&selected)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        };
        let metadata = file.metadata().map_err(io)?;
        paths::metadata(&metadata, false)?;
        if metadata.len() > META_LIMIT as u64 {
            return Err(invalid("physical batch selector size"));
        }
        match fs::symlink_metadata(path) {
            Ok(alias) => {
                paths::metadata(&alias, false)?;
                if (alias.dev(), alias.ino()) != (metadata.dev(), metadata.ino()) {
                    return Err(invalid("physical batch ambiguous canonical alias"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
        Ok(Some((root.to_path_buf(), file)))
    }
    fn member(
        anchor: &mut Anchor,
        index: usize,
        id: &str,
        length: usize,
    ) -> Result<(), FormalError> {
        let entry = anchor
            .recipe
            .objects
            .binary_search_by(|entry| entry.id.as_str().cmp(id))
            .map_err(|_| invalid("physical batch selector member absent"))?;
        let object = &anchor.recipe.objects[entry];
        if object.length != length as u64 {
            return Err(invalid("physical batch logical length differs"));
        }
        anchor.members.push(Member {
            index,
            id: id.into(),
            start: object.start,
            length,
        });
        Ok(())
    }
    fn finish(anchor: Anchor, output: &mut [Option<Vec<u8>>]) -> Result<(), FormalError> {
        for member in &anchor.members {
            output[member.index] = Some(Vec::with_capacity(member.length));
        }
        let mut position = 0u64;
        for frame in &anchor.recipe.frames {
            let next = position + u64::from(frame.length);
            if anchor
                .members
                .iter()
                .any(|member| member.start < next && member.start + member.length as u64 > position)
            {
                // At most one decoded 64 KiB frame is retained at a time.
                #[cfg(test)]
                COUNTS.with(|counts| {
                    let (anchors, frames, maximum) = counts.get();
                    counts.set((anchors, frames + 1, maximum));
                });
                let bytes = decode_frame(&anchor.root, frame)?;
                for member in &anchor.members {
                    let start = member.start.max(position);
                    let end = (member.start + member.length as u64).min(next);
                    if start < end {
                        output[member.index]
                            .as_mut()
                            .ok_or_else(|| invalid("physical batch output missing"))?
                            .extend_from_slice(
                                &bytes[(start - position) as usize..(end - position) as usize],
                            );
                    }
                }
            }
            position = next;
        }
        for member in &anchor.members {
            let bytes = output[member.index]
                .as_ref()
                .ok_or_else(|| invalid("physical batch output missing"))?;
            if bytes.len() != member.length || blake3::hash(bytes).to_hex().as_str() != member.id {
                return Err(invalid("physical batch logical coverage/hash mismatch"));
            }
        }
        // Sync the retained descriptor itself and all its dependency names.
        anchor.file.sync_all().map_err(io)?;
        for frame in &anchor.recipe.frames {
            sync(&anchor.root.join(FRAMES).join(&frame.stored))?;
        }
        sync(&anchor.root.join(FRAMES))?;
        sync(&anchor.root.join(SELECTORS))?;
        sync(&anchor.root)?;
        Ok(())
    }
    pub(super) fn read_batch(
        requests: &[(PathBuf, String, usize)],
    ) -> Result<Vec<Option<Vec<u8>>>, FormalError> {
        let mut output = vec![None; requests.len()];
        let mut anchors: Vec<Anchor> = Vec::new();
        let mut metadata_bytes = 0u64;
        for (index, (path, id, length)) in requests.iter().enumerate() {
            // At most 16 retained anchors plus this one temporary alias probe
            // are open. A matching probe is dropped immediately; a new anchor
            // is inserted only after a full group flush when already at 16.
            let Some((root, mut file)) = open_selector(path, id)? else {
                continue;
            };
            let metadata = file.metadata().map_err(io)?;
            let identity = (metadata.dev(), metadata.ino());
            if let Some(anchor) = anchors
                .iter_mut()
                .find(|anchor| anchor.root == root && anchor.identity == identity)
            {
                if anchor.size != metadata.len() {
                    return Err(invalid("physical batch alias length changed"));
                }
                member(anchor, index, id, *length)?;
                continue;
            }
            if anchors.len() == 16 || metadata_bytes + metadata.len() > META_LIMIT as u64 {
                for anchor in anchors.drain(..) {
                    finish(anchor, &mut output)?;
                }
                metadata_bytes = 0;
            }
            file.seek(SeekFrom::Start(0)).map_err(io)?;
            let mut bytes = Vec::new();
            (&mut file)
                .take(META_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(io)?;
            let after = file.metadata().map_err(io)?;
            paths::metadata(&after, false)?;
            if after.len() != metadata.len() || bytes.len() as u64 != metadata.len() {
                return Err(invalid("physical batch anchor changed during read"));
            }
            let recipe = recipe::decode_recipe_cached(&bytes)?;
            #[cfg(test)]
            COUNTS.with(|counts| {
                let (reads, frames, maximum) = counts.get();
                counts.set((reads + 1, frames, maximum.max(anchors.len() + 1)));
            });
            metadata_bytes += metadata.len();
            let mut anchor = Anchor {
                root,
                file,
                identity,
                size: metadata.len(),
                recipe,
                members: Vec::new(),
            };
            member(&mut anchor, index, id, *length)?;
            anchors.push(anchor);
        }
        for anchor in anchors {
            finish(anchor, &mut output)?;
        }
        Ok(output)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::tests::{Fixture, object};
    use super::*;
    #[test]
    fn physical_fifo_selector_is_rejected_without_blocking_open() {
        const CHILD_ROOT: &str = "OASIS7_TEST_PHYSICAL_FIFO_ROOT";
        let id = "00".repeat(32);
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let root = PathBuf::from(root);
            assert!(read_durable_batch(&root, &[(root.join(&id), id, 1)]).is_err());
            return;
        }
        let fixture = Fixture::new();
        let path = fixture.0.join(SELECTORS).join(&id);
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
                .env(CHILD_ROOT, &fixture.0)
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
                "FIFO selector open blocked"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    fn requests(root: &Path, objects: &[(String, Vec<u8>)]) -> Vec<(PathBuf, String, usize)> {
        objects
            .iter()
            .map(|(id, bytes)| (root.join(id), id.clone(), bytes.len()))
            .collect()
    }
    #[test]
    fn shared_recipe_batch_reads_each_frame_once_and_rechecks_same_inode_next_call() {
        let fixture = Fixture::new();
        let mut objects = (0..128)
            .map(|index| object(format!("{index:05}{}", "x".repeat(1024)).into_bytes()))
            .collect::<Vec<_>>();
        objects.sort_by(|left, right| left.0.cmp(&right.0));
        assert!(persist(&fixture.0, &objects).unwrap());
        let input = requests(&fixture.0, &objects);
        let _pin = pin(&fixture.0, false).unwrap();
        COUNTS.with(|counts| counts.set((0, 0, 0)));
        let result = read_durable_batch(&fixture.0, &input).unwrap();
        for (actual, (_, expected)) in result.iter().zip(&objects) {
            assert_eq!(actual.as_ref(), Some(expected));
        }
        let (_, recipe) = selector(&input[0].0, &input[0].1).unwrap().unwrap();
        COUNTS.with(|counts| assert_eq!(counts.get(), (1, recipe.frames.len(), 1)));
        let writes = objects
            .iter()
            .map(
                |(digest, bytes)| super::super::super::business_index::BusinessIndexNodeWriteV1 {
                    digest: digest.clone(),
                    bytes: bytes.clone(),
                },
            )
            .collect::<Vec<_>>();
        COUNTS.with(|counts| counts.set((0, 0, 0)));
        super::super::super::business_pack::persist(&fixture.0, &writes).unwrap();
        COUNTS.with(|counts| assert_eq!(counts.get(), (1, recipe.frames.len(), 1)));
        let selected = fixture.0.join(SELECTORS).join(&objects[0].0);
        let original = fs::read(&selected).unwrap();
        let inode = fs::metadata(&selected).unwrap().ino();
        let mut corrupt = original.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        fs::write(&selected, corrupt).unwrap();
        assert_eq!(fs::metadata(&selected).unwrap().ino(), inode);
        assert!(read_durable_batch(&fixture.0, &input).is_err());
        fs::write(&selected, original).unwrap();
        let frame = fixture.0.join(FRAMES).join(&recipe.frames[0].stored);
        fs::write(frame, b"damaged frame").unwrap();
        assert!(read_durable_batch(&fixture.0, &input).is_err());
    }
    #[test]
    fn batch_flushes_anchor_groups_and_rejects_alias_and_output_budget_violations() {
        let fixture = Fixture::new();
        let objects = (0..17)
            .map(|index| object(vec![index; 8192]))
            .collect::<Vec<_>>();
        for object in &objects {
            assert!(persist(&fixture.0, std::slice::from_ref(object)).unwrap());
        }
        let input = requests(&fixture.0, &objects);
        let _pin = pin(&fixture.0, false).unwrap();
        COUNTS.with(|counts| counts.set((0, 0, 0)));
        assert!(
            read_durable_batch(&fixture.0, &input)
                .unwrap()
                .iter()
                .all(Option::is_some)
        );
        COUNTS.with(|counts| assert_eq!(counts.get().2, 16));
        let mut wrong_length = input[..1].to_vec();
        wrong_length[0].2 += 1;
        assert!(read_durable_batch(&fixture.0, &wrong_length).is_err());
        assert!(read_durable_batch(&fixture.0, &vec![input[0].clone(); 1025]).is_err());
        wrong_length[0].2 = OUTPUT + 1;
        assert!(read_durable_batch(&fixture.0, &wrong_length).is_err());
        let alias = fixture.0.join(SELECTORS).join(&objects[0].0);
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(fixture.0.join(SELECTORS).join(&objects[1].0), alias).unwrap();
        assert!(read_durable_batch(&fixture.0, &input).is_err());
    }
    #[test]
    fn raw_fallback_checks_declared_length_before_reading_payload() {
        let fixture = Fixture::new();
        let bytes = b"raw original";
        let id = blake3::hash(bytes).to_hex().to_string();
        let path = fixture.0.join(&id);
        create(&path, bytes).unwrap();
        let _pin = pin(&fixture.0, false).unwrap();
        assert_eq!(
            read_durable_batch(&fixture.0, &[(path.clone(), id.clone(), bytes.len())]).unwrap(),
            vec![None]
        );
        assert_eq!(
            super::super::super::business_pack::read_legacy_durable_expected(
                &path,
                &id,
                bytes.len()
            )
            .unwrap(),
            Some(bytes.to_vec())
        );
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(LIMIT as u64).unwrap();
        assert!(
            super::super::super::business_pack::read_legacy_durable_expected(
                &path,
                &id,
                bytes.len()
            )
            .is_err()
        );
    }
    #[test]
    fn raw_magic_prefix_remains_readable_but_wrong_sparse_length_rejects_before_hash_scan() {
        let fixture = Fixture::new();
        let mut bytes = b"oasis7.formal.business-cas.pack.v1\0".to_vec();
        bytes.extend_from_slice(&[0; 24]); // Invalid pack header, valid raw bytes.
        let id = blake3::hash(&bytes).to_hex().to_string();
        let path = fixture.0.join(&id);
        create(&path, &bytes).unwrap();
        let _pin = pin(&fixture.0, false).unwrap();
        assert_eq!(
            super::super::super::business_pack::read_legacy_durable_expected(
                &path,
                &id,
                bytes.len()
            )
            .unwrap(),
            Some(bytes.clone())
        );
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(LIMIT as u64)
            .unwrap();
        let error = super::super::super::business_pack::read_legacy_durable_expected(&path, &id, 8)
            .unwrap_err();
        assert!(
            error.to_string().contains("expected logical byte length"),
            "must reject before the ambiguous-magic fallback scans payload: {error}"
        );
    }
}
