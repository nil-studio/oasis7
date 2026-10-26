use super::{
    BlobStore, LocalCasStore, PIN_SCOPES_DIR, PinFile, read_json_from_path, validate_hash,
    validate_pin_path_component, write_json_atomic,
};
use oasis7_proto::world_error::WorldError;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

impl LocalCasStore {
    fn load_pins(&self) -> Result<PinFile, WorldError> {
        if !self.pins_path.exists() {
            return Ok(PinFile::default());
        }
        read_json_from_path(&self.pins_path)
    }

    fn save_pins(&self, pins: &PinFile) -> Result<(), WorldError> {
        self.ensure_dirs()?;
        write_json_atomic(pins, &self.pins_path)
    }

    pub(super) fn pin_scope_dir(&self, scope: &str) -> Result<PathBuf, WorldError> {
        validate_pin_path_component(scope, "pin scope")?;
        Ok(self.root.join(PIN_SCOPES_DIR).join(scope))
    }

    pub(super) fn pin_scope_shard_path(
        &self,
        scope: &str,
        shard: &str,
    ) -> Result<PathBuf, WorldError> {
        validate_pin_path_component(shard, "pin scope shard")?;
        Ok(self.pin_scope_dir(scope)?.join(format!("{shard}.json")))
    }

    fn load_scoped_pins(&self) -> Result<BTreeSet<String>, WorldError> {
        let scopes_root = self.root.join(PIN_SCOPES_DIR);
        let mut pins = BTreeSet::new();
        if !scopes_root.exists() {
            return Ok(pins);
        }
        for scope_entry in fs::read_dir(scopes_root)? {
            let scope_entry = scope_entry?;
            if !scope_entry.file_type()?.is_dir() {
                continue;
            }
            for shard_entry in fs::read_dir(scope_entry.path())? {
                let shard_entry = shard_entry?;
                if !shard_entry.file_type()?.is_file()
                    || shard_entry.path().extension().and_then(|ext| ext.to_str()) != Some("json")
                {
                    continue;
                }
                let shard: PinFile = read_json_from_path(shard_entry.path().as_path())?;
                for pin in shard.pins {
                    validate_hash(pin.as_str())?;
                    pins.insert(pin);
                }
            }
        }
        Ok(pins)
    }

    pub(super) fn load_effective_pins(&self) -> Result<BTreeSet<String>, WorldError> {
        let mut pins = self.load_pins()?.pins;
        pins.extend(self.load_scoped_pins()?);
        Ok(pins)
    }

    pub fn pin(&self, content_hash: &str) -> Result<(), WorldError> {
        let _retention = self.lock_retention()?;
        validate_hash(content_hash)?;
        if !self.has(content_hash)? {
            return Err(WorldError::BlobNotFound {
                content_hash: content_hash.to_string(),
            });
        }
        let mut pins = self.load_pins()?;
        pins.pins.insert(content_hash.to_string());
        self.save_pins(&pins)
    }

    pub fn unpin(&self, content_hash: &str) -> Result<bool, WorldError> {
        let _retention = self.lock_retention()?;
        validate_hash(content_hash)?;
        let mut pins = self.load_pins()?;
        let removed = pins.pins.remove(content_hash);
        self.save_pins(&pins)?;
        Ok(removed)
    }

    pub fn list_pins(&self) -> Result<Vec<String>, WorldError> {
        Ok(self.load_pins()?.pins.into_iter().collect())
    }

    pub fn is_pinned(&self, content_hash: &str) -> Result<bool, WorldError> {
        validate_hash(content_hash)?;
        Ok(self.load_pins()?.pins.contains(content_hash))
    }

    pub fn list_effective_pins(&self) -> Result<Vec<String>, WorldError> {
        Ok(self.load_effective_pins()?.into_iter().collect())
    }

    pub fn is_effectively_pinned(&self, content_hash: &str) -> Result<bool, WorldError> {
        validate_hash(content_hash)?;
        Ok(self.load_effective_pins()?.contains(content_hash))
    }

    /// Atomically replaces the legacy global pin set in one write.
    pub fn replace_pins(&self, pins: &BTreeSet<String>) -> Result<(), WorldError> {
        let _retention = self.lock_retention()?;
        for pin in pins {
            validate_hash(pin.as_str())?;
            if !self.has(pin.as_str())? {
                return Err(WorldError::BlobNotFound {
                    content_hash: pin.clone(),
                });
            }
        }
        self.save_pins(&PinFile { pins: pins.clone() })
    }

    /// Atomically publishes one independently replaceable pin shard.
    pub fn replace_pin_scope_shard(
        &self,
        scope: &str,
        shard: &str,
        pins: &BTreeSet<String>,
    ) -> Result<(), WorldError> {
        let _retention = self.lock_retention()?;
        for pin in pins {
            validate_hash(pin.as_str())?;
            if !self.has(pin.as_str())? {
                return Err(WorldError::BlobNotFound {
                    content_hash: pin.clone(),
                });
            }
        }
        let path = self.pin_scope_shard_path(scope, shard)?;
        let parent = path
            .parent()
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: format!("pin scope shard path missing parent: {}", path.display()),
            })?;
        fs::create_dir_all(parent)?;
        write_json_atomic(&PinFile { pins: pins.clone() }, path.as_path())
    }

    pub fn remove_pin_scope_shard(&self, scope: &str, shard: &str) -> Result<bool, WorldError> {
        let _retention = self.lock_retention()?;
        let path = self.pin_scope_shard_path(scope, shard)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    /// Lists the published JSON shard ids in one pin scope.
    pub fn list_pin_scope_shards(&self, scope: &str) -> Result<Vec<String>, WorldError> {
        let path = self.pin_scope_dir(scope)?;
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut shards = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry.path().extension().and_then(|ext| ext.to_str()) != Some("json")
            {
                continue;
            }
            if let Some(shard) = entry.path().file_stem().and_then(|stem| stem.to_str()) {
                shards.push(shard.to_string());
            }
        }
        shards.sort_unstable();
        Ok(shards)
    }

    pub fn clear_pin_scope(&self, scope: &str) -> Result<(), WorldError> {
        let _retention = self.lock_retention()?;
        let path = self.pin_scope_dir(scope)?;
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}
