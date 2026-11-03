//! Bounded, non-payload timing evidence before the process fixture is removed.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

const MAX_BYTES: u64 = 64 * 1024;
const MAX_ROWS: usize = 128;

pub(super) struct OwnerTiming(PathBuf);

impl OwnerTiming {
    pub(super) fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

fn safe_row(line: &str) -> Option<String> {
    let tail = line.strip_prefix("formal_execution_timing ")?;
    let mut seen = std::collections::BTreeSet::new();
    let fields: Vec<_> = tail
        .split_ascii_whitespace()
        .take_while(|field| !field.starts_with("reason=") && !field.starts_with("payload="))
        .filter_map(|field| {
            let (key, value) = field.split_once('=')?;
            let numeric = matches!(
                key,
                "position" | "stage_ms" | "total_ms" | "objects" | "object_bytes"
            );
            let label = matches!(key, "operation" | "phase" | "event");
            if value.is_empty()
                || value.len() > 64
                || !(numeric || label)
                || !seen.insert(key)
                || !value.bytes().all(|byte| {
                    if numeric {
                        byte.is_ascii_digit()
                    } else {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'_' | b'-' | b'.')
                    }
                })
            {
                return None;
            }
            Some(field)
        })
        .take(8)
        .collect();
    (!fields.is_empty()).then(|| fields.join(" "))
}

impl Drop for OwnerTiming {
    fn drop(&mut self) {
        let Ok(mut file) = File::open(&self.0) else {
            return;
        };
        let Ok(metadata) = file.metadata() else {
            return;
        };
        let offset = metadata.len().saturating_sub(MAX_BYTES);
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.take(MAX_BYTES).read_to_end(&mut bytes).is_err() {
            return;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rows: Vec<_> = text.lines().filter_map(safe_row).collect();
        eprintln!(
            "shipping_owner_timing rows={} truncated={}",
            rows.len().min(MAX_ROWS),
            offset != 0 || rows.len() > MAX_ROWS
        );
        for row in rows.iter().rev().take(MAX_ROWS).rev() {
            eprintln!("shipping_owner_timing {row}");
        }
    }
}

#[test]
fn owner_timing_only_exports_bounded_fixed_fields() {
    assert_eq!(
        safe_row(
            "formal_execution_timing operation=resume position=2 phase=world_restored event=end stage_ms=7 digest=private reason=secret payload=secret"
        ),
        Some("operation=resume position=2 phase=world_restored event=end stage_ms=7".into())
    );
    assert_eq!(safe_row("other reason=secret"), None);
    assert_eq!(
        safe_row(
            "formal_execution_timing operation=resume event=error reason=failed operation=private_goal phase=secret"
        ),
        Some("operation=resume event=error".into())
    );
    assert_eq!(
        safe_row("formal_execution_timing operation=resume operation=private_goal"),
        Some("operation=resume".into())
    );
    assert_eq!(
        safe_row("formal_execution_timing position=secret phase=bad/secret"),
        None
    );
    assert_eq!(
        safe_row(&format!("formal_execution_timing phase={}", "a".repeat(65))),
        None
    );
}
