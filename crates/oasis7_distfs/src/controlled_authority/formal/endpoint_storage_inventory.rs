//! Local read-only retained-record inventory; no new RPC or serving authority.
use super::super::super::physical;
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[derive(Debug, Serialize, Deserialize)]
pub struct FormalStorageLogicalObject {
    pub digest: String,
    pub length: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FormalStorageLogicalInventoryPage {
    /// Append-only retention records, including legitimate duplicate digests.
    pub objects: Vec<FormalStorageLogicalObject>,
    pub next: Option<String>,
    pub complete: bool,
    pub namespace_digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    namespace: String,
    position: physical::roots::PageCursor,
}
fn inventory_io(error: impl std::fmt::Display) -> FormalError {
    FormalError::Io(error.to_string())
}
impl FileFormalEndpoint {
    fn inventory_namespace(&mut self) -> Result<String, FormalError> {
        if self.physical_maintenance.inventory_session.is_none() {
            self.physical_maintenance.inventory_session =
                Some(super::super::super::super::fencing::fresh_challenge().map_err(inventory_io)?);
        }
        let root = self.store.path.join("formal-objects-v1");
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);
        let file = options
            .open(root.join(".retained-logical-v1/head.cbor"))
            .map_err(inventory_io)?;
        physical::paths::metadata(&file.metadata().map_err(inventory_io)?, false)?;
        let mut registry = Vec::new();
        file.take(1025)
            .read_to_end(&mut registry)
            .map_err(inventory_io)?;
        if registry.len() > 1024 {
            return Err(invalid("inventory retention head bound"));
        }
        let metadata = std::fs::symlink_metadata(&root).map_err(inventory_io)?;
        #[cfg(unix)]
        let identity = (metadata.dev(), metadata.ino());
        #[cfg(not(unix))]
        let identity = (0u64, metadata.len());
        let namespace = (
            self.physical_maintenance.inventory_session.as_ref(),
            identity,
            self.store.checkpoint_bytes()?,
            registry,
            &self.public_key,
        );
        Ok(
            blake3::hash(&serde_cbor::to_vec(&namespace).map_err(inventory_io)?)
                .to_hex()
                .to_string(),
        )
    }
    /// Every page is bound to this live endpoint owner and exact retained head.
    /// Each record receives fresh original-byte checksum verification. This is
    /// a namespace receipt, not proof that unknown objects grant world authority.
    pub fn storage_logical_inventory_page(
        &mut self,
        cursor: Option<&str>,
    ) -> Result<FormalStorageLogicalInventoryPage, FormalError> {
        let _authority = self.guard()?;
        let namespace = self.inventory_namespace()?;
        let root = self.store.path.join("formal-objects-v1");
        let _pin = physical::pin(&root, false)?
            .ok_or_else(|| invalid("inventory physical fence missing"))?;
        let position = if let Some(cursor) = cursor {
            if cursor.len() > 4096 {
                return Err(invalid("inventory cursor bound"));
            }
            let bytes = hex::decode(cursor).map_err(inventory_io)?;
            let cursor: Cursor = serde_cbor::from_slice(&bytes).map_err(inventory_io)?;
            if cursor.version != 1 || cursor.namespace != namespace {
                return Err(invalid("inventory namespace/session changed"));
            }
            cursor.position
        } else {
            physical::roots::PageCursor::start()
        };
        let (ids, _, _) = physical::roots::page(&root, &position, 1024)?;
        let mut objects = Vec::new();
        let mut used = 0u64;
        for id in ids {
            let path = root.join(&id);
            let length = super::super::super::business_pack::logical_length(&path, &id)?
                .ok_or_else(|| invalid("inventory retained object missing"))?;
            if length > 64 * 1024 * 1024 {
                return Err(invalid("inventory single object bound"));
            }
            if !objects.is_empty() && used.saturating_add(length) > 8 * 1024 * 1024 {
                break;
            }
            let bytes = super::super::super::business_pack::read_object(&path, &id)?
                .ok_or_else(|| invalid("inventory retained object missing"))?;
            if bytes.len() as u64 != length || blake3::hash(&bytes).to_hex().as_str() != id {
                return Err(invalid("inventory exact original checksum/length"));
            }
            used += length;
            objects.push(FormalStorageLogicalObject { digest: id, length });
            if used >= 8 * 1024 * 1024 {
                break;
            }
        }
        let (_, next, complete) = physical::roots::page(&root, &position, objects.len().max(1))?;
        if self.inventory_namespace()? != namespace {
            return Err(invalid("inventory namespace changed during page"));
        }
        let next = if complete {
            None
        } else {
            Some(hex::encode(
                serde_cbor::to_vec(&Cursor {
                    version: 1,
                    namespace: namespace.clone(),
                    position: next,
                })
                .map_err(inventory_io)?,
            ))
        };
        Ok(FormalStorageLogicalInventoryPage {
            objects,
            next,
            complete,
            namespace_digest: namespace,
        })
    }
}
