//! Bounded deployment representation; the reconstructed policy owns authority.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const INLINE: &str = "DEV_TEST_POLICY";
const MANIFEST: &str = "DEV_TEST_POLICY_MANIFEST";
const PART_BYTES: usize = 4096;
const MAX_PARTS: usize = 16;
const MAX_BYTES: usize = 60 * 1024;
const ERROR: &str = "configuration_unavailable";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u8,
    parts: usize,
    blake3: String,
}

fn part_name(index: usize) -> String {
    format!("DEV_TEST_POLICY_PART_{index:02}")
}

pub(crate) fn load(mut secret: impl FnMut(&str) -> Option<String>) -> Result<String, &'static str> {
    match (secret(INLINE), secret(MANIFEST)) {
        (Some(raw), None) if !raw.is_empty() && raw.len() <= 5 * 1024 => Ok(raw),
        (None, Some(raw)) if raw.len() <= 256 => {
            let manifest: Manifest = serde_json::from_str(&raw).map_err(|_| ERROR)?;
            if manifest.version != 1
                || !(1..=MAX_PARTS).contains(&manifest.parts)
                || manifest.blake3.len() != 64
                || !manifest
                    .blake3
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(ERROR);
            }
            let mut policy = String::new();
            for index in 1..=manifest.parts {
                let part = secret(&part_name(index)).ok_or(ERROR)?;
                if part.is_empty()
                    || part.len() > PART_BYTES
                    || policy.len() + part.len() > MAX_BYTES
                {
                    return Err(ERROR);
                }
                policy.push_str(&part);
            }
            if blake3::hash(policy.as_bytes()).to_hex().as_str() != manifest.blake3 {
                return Err(ERROR);
            }
            Ok(policy)
        }
        _ => Err(ERROR),
    }
}

pub(crate) fn encode(raw: &str) -> Result<BTreeMap<String, Option<String>>, &'static str> {
    if raw.is_empty() || raw.len() > MAX_BYTES {
        return Err(ERROR);
    }
    // Nulls let one bulk update remove the other representation and stale parts.
    let mut bindings = BTreeMap::from([(INLINE.into(), None), (MANIFEST.into(), None)]);
    for index in 1..=MAX_PARTS {
        bindings.insert(part_name(index), None);
    }
    if raw.len() <= PART_BYTES {
        bindings.insert(INLINE.into(), Some(raw.into()));
    } else {
        let mut tail = raw;
        let mut count = 0;
        while !tail.is_empty() {
            let mut end = PART_BYTES.min(tail.len());
            while !tail.is_char_boundary(end) {
                end -= 1;
            }
            count += 1;
            if count > MAX_PARTS {
                return Err(ERROR);
            }
            bindings.insert(part_name(count), Some(tail[..end].into()));
            tail = &tail[end..];
        }
        bindings.insert(
            MANIFEST.into(),
            Some(
                serde_json::to_string(&Manifest {
                    version: 1,
                    parts: count,
                    blake3: blake3::hash(raw.as_bytes()).to_hex().to_string(),
                })
                .map_err(|_| ERROR)?,
            ),
        );
    }
    Ok(bindings)
}

#[cfg(test)]
mod tests;
