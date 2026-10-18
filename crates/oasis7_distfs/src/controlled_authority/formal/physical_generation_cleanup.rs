//! Each call removes only a verified batch from an unselected generation.
use super::*;
fn known(path: &Path, relative: &Path, metadata: &fs::Metadata) -> Result<(), FormalError> {
    let parts = relative
        .components()
        .map(|part| {
            part.as_os_str()
                .to_str()
                .ok_or_else(|| invalid("old generation nonUTF8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let first = *parts
        .first()
        .ok_or_else(|| invalid("old generation relative path"))?;
    let catalog_staging =
        parts.len() >= 2 && first == catalog::CATALOG && parts[1] == ".physical-staging-v1";
    if metadata.is_dir() {
        if catalog_staging && parts.len() == 2 {
            return Ok(());
        }
        if parts.len() != 1
            || ![
                catalog::CATALOG,
                FRAMES,
                SELECTORS,
                ".business-pack-staging-v1",
                ".physical-staging-v1",
                ".retained-logical-v1",
            ]
            .contains(&first)
        {
            return Err(invalid("old generation unknown directory"));
        }
        return Ok(());
    }
    if catalog_staging && parts.len() == 3 {
        if !metadata.is_file() {
            return Err(invalid("old catalog staging regular file"));
        }
        hash_hex(parts[2])?;
        read(path, META_LIMIT)?;
        return Ok(());
    }
    if !metadata.is_file() || parts.len() > 2 {
        return Err(invalid("old generation entry type/depth"));
    }
    let name = *parts.last().ok_or_else(|| invalid("old generation name"))?;
    if parts.len() == 1 {
        if name.len() == 64 {
            hash_hex(name)?;
            crate::controlled_authority::formal::business_pack::read_object_unpinned(path, name)?
                .ok_or_else(|| invalid("old generation raw object missing"))?;
        } else if [GUARD, PUBLICATION_GUARD].contains(&name) {
            if !read(path, 1)?.is_empty() {
                return Err(invalid("old generation guard contents"));
            }
        } else if name == ENABLED {
            if read(path, 64)? != b"oasis7.formal.physical.v1\n" {
                return Err(invalid("old generation capability"));
            }
        } else if name == MANIFEST {
            let value: Manifest = serde_cbor::from_slice(&read(path, META_LIMIT)?).map_err(io)?;
            if value.version != 3 {
                return Err(invalid("old generation manifest version"));
            }
            catalog::validate(&value.catalogs)?;
        } else {
            return Err(invalid("old generation unknown file"));
        }
    } else if [catalog::CATALOG, FRAMES, SELECTORS, ".physical-staging-v1"].contains(&first) {
        hash_hex(name)?;
        let bound = if first == FRAMES {
            FRAME
        } else {
            LIMIT.max(META_LIMIT)
        };
        let bytes = read(path, bound)?;
        if first == SELECTORS {
            decode_recipe(&bytes)?;
        } else if first == FRAMES || first == catalog::CATALOG {
            if blake3::hash(&bytes).to_hex().as_str() != name {
                return Err(invalid("old generation stored checksum"));
            }
            if first == catalog::CATALOG {
                catalog::validate_file(path, name)?;
            }
        }
    } else if first == ".business-pack-staging-v1" {
        let digest = name
            .strip_prefix("pack-")
            .ok_or_else(|| invalid("old pack staging name"))?;
        hash_hex(digest)?;
        if metadata.len() > 80 * 1024 * 1024 {
            return Err(invalid("old pack staging byte budget"));
        }
    } else if first == ".retained-logical-v1" {
        if name == "head.cbor" || name.starts_with(".head-") {
            if let Some(nonce) = name.strip_prefix(".head-") {
                hash_hex(nonce)?;
            }
            let _: serde_cbor::Value = serde_cbor::from_slice(&read(path, 1024)?).map_err(io)?;
        } else {
            let number = name
                .strip_suffix(".bin")
                .ok_or_else(|| invalid("old retention file"))?;
            if number.len() != 20 || !number.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("old retention segment name"));
            }
            read(path, 65536)?;
        }
    } else {
        return Err(invalid("old generation unknown namespace"));
    }
    Ok(())
}
fn gather(
    root: &Path,
    dir: &Path,
    batch: &mut Vec<(PathBuf, bool)>,
    size: &mut u64,
) -> Result<bool, FormalError> {
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(io)?;
        let relative = path.strip_prefix(root).map_err(io)?;
        known(&path, relative, &metadata)?;
        if metadata.is_dir() {
            if !gather(root, &path, batch, size)? {
                return Ok(false);
            }
            if fs::read_dir(&path).map_err(io)?.next().is_none() {
                batch.push((path, true));
            }
        } else {
            if !batch.is_empty() && size.saturating_add(metadata.len()) > BATCH as u64 {
                return Ok(false);
            }
            *size = size
                .checked_add(metadata.len())
                .ok_or_else(|| invalid("old generation cleanup bytes overflow"))?;
            batch.push((path, false));
        }
        if batch.len() >= 1024 || *size >= BATCH as u64 {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn old_generation_batch(root: &Path) -> Result<bool, FormalError> {
    match fs::symlink_metadata(root) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(io(e)),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(invalid("old generation regular directory"));
        }
        Ok(_) => {}
    }
    let mut batch = Vec::new();
    let mut size = 0u64;
    gather(root, root, &mut batch, &mut size)?;
    // Validate the entire bounded batch before unlinking any of it. A failure
    // leaves the selected target and all as-yet-unretired old entries intact.
    for (path, directory) in batch {
        if directory {
            fs::remove_dir(&path).map_err(io)?;
        } else {
            fs::remove_file(&path).map_err(io)?;
        }
        sync(
            path.parent()
                .ok_or_else(|| invalid("old generation parent"))?,
        )?;
    }
    if fs::read_dir(root).map_err(io)?.next().is_none() {
        fs::remove_dir(root).map_err(io)?;
        sync(
            root.parent()
                .ok_or_else(|| invalid("old generation parent"))?,
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}
