//! Descriptor-pinned completed source history. Never acquires a writer lock or
//! repairs a WAL while the actual Node owns the source.
use super::super::super::super::formal_ledger::{OriginContract, RecordContract};
use super::*;

impl InstallationRootV1 {
    pub(in super::super) fn read_source_history(
        &self,
        deadline: Instant,
    ) -> Result<(storage::Origin, Vec<storage::Record>), String> {
        live(deadline)?;
        self.verify_directories()?;
        let names = entry_names(&self.prefix)?;
        let mut heights = std::collections::BTreeSet::new();
        for name in &names {
            let name = name.to_str().ok_or("source ledger filename encoding")?;
            if matches!(name, "writer.lock" | "origin.cbor")
                || name == storage::NODE_PRIVATE_DIRECTORY
            {
                continue;
            }
            let height = name
                .strip_prefix("record-")
                .and_then(|n| n.strip_suffix(".cbor"))
                .filter(|n| n.len() == 20 && n.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|n| n.parse::<u64>().ok())
                .ok_or("source ledger has unresolved WAL or unknown original")?;
            heights.insert(height);
        }
        if heights.len() > 64 {
            return Err("source ledger history capacity".into());
        }
        let read = |name: &str, limit: usize| -> Result<(File, Vec<u8>), String> {
            live(deadline)?;
            let mut file = open_child(&self.prefix, name, false)?;
            let metadata = file.metadata().map_err(err)?;
            // SAFETY: geteuid has no arguments or memory access.
            if !metadata.is_file()
                || metadata.nlink() != 1
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.len() > limit as u64
            {
                return Err("source ledger original file ownership/type/capacity".into());
            }
            let mut bytes = Vec::new();
            (&mut file)
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(err)?;
            if bytes.len() > limit {
                return Err("source ledger file capacity".into());
            }
            same_file(&file, &open_child(&self.prefix, name, false)?)?;
            Ok((file, bytes))
        };
        let (origin_file, origin_bytes) = read("origin.cbor", MAX_METADATA)?;
        let origin: storage::Origin = decode(&origin_bytes, MAX_METADATA)?;
        origin.validate()?;
        let _carriers = self.verify_origin_carriers(&origin, deadline)?;
        let mut records = Vec::new();
        let mut originals = vec![("origin.cbor".to_owned(), origin_file, origin_bytes)];
        let mut previous = origin.digest()?;
        for height in heights {
            let expected = origin
                .first_height()
                .checked_add(records.len() as u64)
                .ok_or("source ledger height overflow")?;
            if height != expected {
                return Err("source ledger history gap".into());
            }
            let name = format!("record-{height:020}.cbor");
            let (file, bytes) = read(&name, super::super::super::super::formal_ledger::MAX_RECORD)?;
            let record: storage::Record = decode(
                &bytes,
                super::super::super::super::formal_ledger::MAX_RECORD,
            )?;
            record.validate(&origin)?;
            if record.height() != height || record.parent_record_digest() != previous {
                return Err("source ledger original history chain differs".into());
            }
            previous = record.digest()?;
            originals.push((name, file, bytes));
            records.push(record);
        }
        // Hold every descriptor and compare the immutable bytes again; neither
        // an atomic replacement nor an in-place rewrite can become authority.
        for (name, file, original) in originals {
            let (current, bytes) = read(&name, original.len())?;
            same_file(&file, &current)?;
            if bytes != original {
                return Err("source ledger changed during readback".into());
            }
        }
        if entry_names(&self.prefix)?
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            != names.into_iter().collect()
        {
            return Err("source ledger inventory changed during readback".into());
        }
        self.verify_origin_carriers(&origin, deadline)?;
        live(deadline)?;
        Ok((origin, records))
    }
}
