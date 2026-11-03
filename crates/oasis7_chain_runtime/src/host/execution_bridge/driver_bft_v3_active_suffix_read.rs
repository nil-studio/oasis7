//! Descriptor-pinned authenticated committed prefix; never acquires a writer lock.
use super::super::super::super::formal_ledger::{OriginContract, RecordContract};
use super::super::window_storage::{WindowOrigin, WindowRecord};
use super::*;

impl InstallationRootV1 {
    pub(in super::super) fn read_active_prefix(
        &self,
        base: &super::super::successor_verifier::VerifiedPrivateSuccessorBaseV1,
        origin: &WindowOrigin,
        history: &[FormalBftCertifiedHeaderV3],
        deadline: Instant,
    ) -> Result<Vec<(WindowRecord, bool)>, String> {
        live(deadline)?;
        self.verify_directories()?;
        if history.len() > 64 {
            return Err("active signed suffix capacity".into());
        }
        let original = base.original.as_ref().ok_or("active original missing")?;
        let id = original
            .request
            .intent
            .body
            .checkpoint
            .digest()
            .map_err(err)?;
        let windows = open_child(&self.root, "successor-active-v1", true)?;
        let segment = open_child(&windows, &id, true)?;
        let prefix = open_child(&segment, super::super::window_storage::DIRECTORY, true)?;
        private_directory(&windows)?;
        private_directory(&segment)?;
        private_directory(&prefix)?;
        let names = entry_names(&prefix)?;
        if names.len() > 3 * 64 + 5 {
            return Err("active suffix directory capacity".into());
        }
        for name in &names {
            let name = name.to_str().ok_or("active suffix non-UTF8 filename")?;
            if name == "writer.lock"
                || name == "origin.cbor"
                || name == storage::NODE_PRIVATE_DIRECTORY
            {
                continue;
            }
            let target = files::stage_target(name).unwrap_or(name);
            let height = ["record-", "intent-", "reservation-"]
                .iter()
                .find_map(|prefix| target.strip_prefix(prefix))
                .and_then(|suffix| suffix.strip_suffix(".cbor"))
                .filter(|digits| digits.len() == 20 && digits.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|digits| digits.parse::<u64>().ok())
                .ok_or("active suffix unknown immutable slot file")?;
            if height < origin.first_height()
                || height
                    .checked_sub(origin.first_height())
                    .is_none_or(|offset| offset >= 64)
            {
                return Err("active suffix slot height capacity".into());
            }
            if files::stage_target(name).is_some() && target.starts_with("record-") {
                return Err("active suffix unsupported record staging target".into());
            }
        }

        let mut result = Vec::new();
        let mut held = Vec::new();
        let mut previous = origin.digest()?;
        let mut total = 0usize;
        for (index, certificate) in history.iter().enumerate() {
            live(deadline)?;
            let height = origin
                .first_height()
                .checked_add(index as u64)
                .ok_or("active suffix height overflow")?;
            let targets = [
                format!("record-{height:020}.cbor"),
                format!("intent-{height:020}.cbor"),
                format!("reservation-{height:020}.cbor"),
            ];
            let mut values: [Option<WindowRecord>; 5] = Default::default();
            let mut files_by_kind: [Option<File>; 5] = Default::default();
            for raw_name in &names {
                let name = raw_name.to_str().ok_or("active suffix non-UTF8 filename")?;
                let staged = files::stage_target(name);
                let target = staged.unwrap_or(name);
                let Some(kind) = targets.iter().position(|candidate| candidate == target) else {
                    continue;
                };
                let slot = match (kind, staged.is_some()) {
                    (0, false) => 0,
                    (1, false) => 1,
                    (1, true) => 2,
                    (2, false) => 3,
                    (2, true) => 4,
                    _ => return Err("active suffix unsupported record staging target".into()),
                };
                let mut file = open_child(&prefix, name, false)?;
                let metadata = file.metadata().map_err(err)?;
                // SAFETY: geteuid has no arguments or memory access.
                if !metadata.is_file()
                    || metadata.nlink() == 0
                    || metadata.nlink() > 2
                    || metadata.uid() != unsafe { libc::geteuid() }
                    || metadata.mode() & 0o077 != 0
                    || metadata.len() > super::super::super::super::formal_ledger::MAX_RECORD as u64
                {
                    return Err("active record is not a bounded private original".into());
                }
                let mut bytes = Vec::new();
                (&mut file)
                    .take(super::super::super::super::formal_ledger::MAX_RECORD as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(err)?;
                total = total
                    .checked_add(bytes.len())
                    .filter(|n| *n <= super::super::successor_archive::MAX_CLOSURE)
                    .ok_or("active suffix record material capacity")?;
                let value: WindowRecord = decode(
                    &bytes,
                    super::super::super::super::formal_ledger::MAX_RECORD,
                )?;
                if values[slot].as_ref().is_some_and(|old| old != &value) {
                    return Err("active suffix staged identities conflict".into());
                }
                values[slot] = Some(value);
                files_by_kind[slot] = Some(file.try_clone().map_err(err)?);
                held.push((name.to_owned(), file, blake3_hex(&bytes), bytes.len()));
            }
            let record = super::super::super::super::formal_ledger::select_original_slot(
                values[0].as_ref(),
                values[1].as_ref(),
                values[2].as_ref(),
                values[3].as_ref(),
                values[4].as_ref(),
            )?
            .clone();
            if let (Some(record_file), Some(intent_file)) = (&files_by_kind[0], &files_by_kind[1]) {
                same_file(record_file, intent_file)?;
                if record_file.metadata().map_err(err)?.nlink() != 2 {
                    return Err("active record/intent pair has foreign links".into());
                }
                for file in files_by_kind[2..].iter().flatten() {
                    if file.metadata().map_err(err)?.nlink() != 1 {
                        return Err("active suffix stage has foreign links".into());
                    }
                }
            } else {
                for file in files_by_kind.iter().flatten() {
                    if file.metadata().map_err(err)?.nlink() != 1 {
                        return Err("active suffix original has foreign links".into());
                    }
                }
            }
            super::super::super::super::formal_ledger::validate_record_chain(
                origin,
                &record,
                index,
                height,
                Some(&previous),
            )?;
            if record.input().certificate != *certificate {
                return Err(
                    "active suffix record differs from authenticated signed history".into(),
                );
            }
            previous = record.digest()?;
            result.push((record, values[3].is_some() || values[4].is_some()));
        }
        let after_names = entry_names(&prefix)?;
        let mut before_names = names.clone();
        let mut after_names = after_names;
        before_names.sort();
        after_names.sort();
        if before_names != after_names {
            return Err("active suffix directory changed during read".into());
        }
        for (name, file, digest, length) in held {
            verify_original_readback(&prefix, &name, &file, &digest, length)?;
        }
        same_directory(
            &windows,
            &open_child(&self.root, "successor-active-v1", true)?,
        )?;
        same_directory(&segment, &open_child(&windows, &id, true)?)?;
        same_directory(
            &prefix,
            &open_child(&segment, super::super::window_storage::DIRECTORY, true)?,
        )?;
        let mut final_names = entry_names(&prefix)?;
        final_names.sort();
        if final_names != before_names {
            return Err("active suffix inventory changed during original recheck".into());
        }
        self.verify_directories()?;
        live(deadline)?;
        Ok(result)
    }
}

fn verify_original_readback(
    directory: &File,
    name: &str,
    original: &File,
    digest: &str,
    length: usize,
) -> Result<(), String> {
    let mut current = open_child(directory, name, false)?;
    same_file(original, &current)?;
    let mut bytes = Vec::new();
    (&mut current)
        .take(length as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() != length || blake3_hex(&bytes) != digest {
        return Err("active suffix original changed during readback".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn suffix_readback_rejects_rewrite_replacement_and_symlink() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "oasis7-active-read-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("record.cbor");
        std::fs::write(&path, b"original").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let directory = open_absolute_directory(&root).unwrap();
        let original = open_child(&directory, "record.cbor", false).unwrap();
        let digest = blake3_hex(b"original");
        verify_original_readback(&directory, "record.cbor", &original, &digest, 8).unwrap();
        std::fs::write(&path, b"rewritten").unwrap();
        assert!(
            verify_original_readback(&directory, "record.cbor", &original, &digest, 8).is_err()
        );
        std::fs::write(&path, b"original").unwrap();
        std::fs::rename(&path, root.join("old.cbor")).unwrap();
        std::fs::write(&path, b"original").unwrap();
        assert!(
            verify_original_readback(&directory, "record.cbor", &original, &digest, 8).is_err()
        );
        std::fs::remove_file(&path).unwrap();
        symlink(root.join("old.cbor"), &path).unwrap();
        assert!(
            verify_original_readback(&directory, "record.cbor", &original, &digest, 8).is_err()
        );
    }
}
