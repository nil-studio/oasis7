use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::super::util::{hash_json, write_json_to_path};
use super::super::{Snapshot, SnapshotRecord, SnapshotRetentionPolicy, WorldError};
use super::World;

impl World {
    // ---------------------------------------------------------------------
    // Snapshot management
    // ---------------------------------------------------------------------

    pub fn set_snapshot_retention(&mut self, policy: SnapshotRetentionPolicy) {
        self.snapshot_catalog.retention = policy;
        self.apply_snapshot_retention();
    }

    pub fn create_snapshot(&mut self) -> Result<Snapshot, WorldError> {
        let snapshot = self.snapshot();
        self.record_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub fn record_snapshot(&mut self, snapshot: &Snapshot) -> Result<SnapshotRecord, WorldError> {
        let snapshot_hash = hash_json(snapshot)?;
        let manifest_hash = hash_json(&snapshot.manifest)?;
        let record = SnapshotRecord {
            snapshot_hash,
            journal_len: snapshot.journal_len,
            created_at: snapshot.state.time,
            manifest_hash,
        };
        self.snapshot_catalog.records.push(record.clone());
        self.apply_snapshot_retention();
        Ok(record)
    }

    pub fn save_snapshot_to_dir(
        &mut self,
        dir: impl AsRef<Path>,
    ) -> Result<SnapshotRecord, WorldError> {
        let snapshot = self.snapshot();
        let record = self.record_snapshot(&snapshot)?;

        let dir = dir.as_ref().join("snapshots");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", record.snapshot_hash));
        write_json_to_path(&snapshot, &path)?;

        self.prune_snapshot_files(&dir)?;
        Ok(record)
    }

    pub fn prune_snapshot_files(&self, dir: impl AsRef<Path>) -> Result<(), WorldError> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Ok(());
        }

        let keep: BTreeSet<String> = self
            .snapshot_catalog
            .records
            .iter()
            .map(|record| format!("{}.json", record.snapshot_hash))
            .collect();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let file_name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            if file_name.ends_with(".json") && !keep.contains(&file_name) {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    fn apply_snapshot_retention(&mut self) {
        let max = self.snapshot_catalog.retention.max_snapshots;
        if max == 0 {
            self.snapshot_catalog.records.clear();
            return;
        }
        if self.snapshot_catalog.records.len() > max {
            let excess = self.snapshot_catalog.records.len() - max;
            self.snapshot_catalog.records.drain(0..excess);
        }
    }
}
