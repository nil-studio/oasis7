//! Success-path process timings. Only reconstructed, allowlisted scalar
//! diagnostics leave the child's private stderr file.
use std::{io::Read, path::Path, time::Instant};

const MAX_STDERR_BYTES: usize = 512 * 1024;
const MAX_DIAGNOSTIC_LINES: usize = 128;

struct StartupDiagnostics {
    lines: Vec<String>,
    bytes_read: usize,
    truncated: bool,
}

pub(super) fn report(path: &Path, pid: u32) {
    use std::os::unix::fs::OpenOptionsExt;
    let started = Instant::now();
    let read = || {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(std::io::Error::other(
                "startup diagnostics require a regular file",
            ));
        }
        read_diagnostics(file)
    };
    match read() {
        Ok(diagnostics) => {
            for line in &diagnostics.lines {
                eprintln!("healthy-endpoint-startup pid={pid} {line}");
            }
            eprintln!(
                "healthy-endpoint-startup-summary pid={pid} bytes_read={} safe_lines={} truncated={} read_failed=false elapsed_us={}",
                diagnostics.bytes_read,
                diagnostics.lines.len(),
                diagnostics.truncated,
                started.elapsed().as_micros()
            );
        }
        Err(_) => eprintln!(
            "healthy-endpoint-startup-summary pid={pid} safe_lines=0 truncated=false read_failed=true elapsed_us={}",
            started.elapsed().as_micros()
        ),
    }
}

fn read_diagnostics(reader: impl Read) -> std::io::Result<StartupDiagnostics> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_STDERR_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let bytes_read = bytes.len();
    let mut truncated = bytes_read > MAX_STDERR_BYTES;
    bytes.truncate(MAX_STDERR_BYTES);
    // A bounded read can end in the middle of a counter or a private line.
    // Only whole newline-terminated records can be emitted.
    if let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') {
        bytes.truncate(last_newline + 1);
    } else {
        bytes.clear();
    }
    let mut lines = Vec::new();
    for line in String::from_utf8_lossy(&bytes).lines() {
        let Some(line) = safe_scalar_line(line) else {
            continue;
        };
        if lines.len() == MAX_DIAGNOSTIC_LINES {
            truncated = true;
            break;
        }
        lines.push(line);
    }
    Ok(StartupDiagnostics {
        lines,
        bytes_read,
        truncated,
    })
}

fn safe_scalar_line(line: &str) -> Option<String> {
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    let (&prefix, values) = fields.split_first()?;
    match prefix {
        "healthy-native-bootstrap" => phase_line(
            prefix,
            values,
            &["lineage", "archive_manifest", "management_ready"],
        ),
        "healthy-native-staging" if values.len() == 1 => {
            numeric_line(prefix, values, &["total_us"])
        }
        "healthy-native-staging" => phase_line(
            prefix,
            values,
            &[
                "verify_transfer",
                "copy_closure",
                "bind_boundary",
                "bind_install",
                "persist_manifest",
                "install_request",
            ],
        ),
        "healthy-native-copy-memo" => numeric_line(
            prefix,
            values,
            &[
                "source_reads",
                "source_bytes",
                "hits",
                "seeded",
                "evictions",
                "retained_objects",
                "retained_bytes",
            ],
        ),
        "healthy-native-walk" => numeric_line(
            prefix,
            values,
            &[
                "total_us",
                "entries",
                "entry_bytes",
                "reused",
                "entry_decode_us",
                "material_us",
                "enqueue_us",
                "lookups",
                "lookup_us",
                "directory_sync_us",
                "empty_hash_us",
                "empty_hash_calls",
                "empty_hash_builds",
                "marker_us",
                "read_us",
                "setup_us",
            ],
        ),
        _ => None,
    }
}

fn phase_line(prefix: &str, values: &[&str], phases: &[&str]) -> Option<String> {
    let (&phase, counters) = values.split_first()?;
    let phase = phase.strip_prefix("phase=")?;
    if !phases.contains(&phase) {
        return None;
    }
    numeric_line(
        &format!("{prefix} phase={phase}"),
        counters,
        &["elapsed_us"],
    )
}

fn numeric_line(prefix: &str, values: &[&str], keys: &[&str]) -> Option<String> {
    if values.len() != keys.len() {
        return None;
    }
    let mut output = prefix.to_owned();
    for (value, key) in values.iter().zip(keys) {
        let (actual_key, value) = value.split_once('=')?;
        if actual_key != *key
            || value.is_empty()
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let value = value.parse::<u128>().ok()?;
        output.push_str(&format!(" {key}={value}"));
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_diagnostics_reconstruct_only_known_scalar_metadata() {
        let input = concat!(
            "private-authentication-payload\n",
            "healthy-native-bootstrap phase=lineage elapsed_us=21\n",
            "healthy-native-staging phase=copy_closure elapsed_us=42\n",
            "healthy-native-staging total_us=63\n",
            "healthy-native-copy-memo source_reads=8 source_bytes=80 hits=3 seeded=2 evictions=0 retained_objects=9 retained_bytes=90\n",
            "healthy-native-bootstrap phase=private-key elapsed_us=1\n",
            "healthy-native-staging total_us=1 secret=private-key\n",
            "healthy-native-walk private-key\n",
            "healthy-native-staging total_us=private-key\n",
        );
        let diagnostics = read_diagnostics(input.as_bytes()).unwrap();
        assert_eq!(diagnostics.lines.len(), 4);
        assert!(!diagnostics.lines.join("\n").contains("private"));
        assert!(!diagnostics.truncated);
    }

    #[test]
    fn startup_diagnostics_bound_input_output_and_discard_partial_lines() {
        let mut input = vec![b'x'; MAX_STDERR_BYTES - 1];
        input.push(b'\n');
        input.extend_from_slice(b"healthy-native-staging total_us=9\n");
        let diagnostics = read_diagnostics(input.as_slice()).unwrap();
        assert!(diagnostics.truncated);
        assert_eq!(diagnostics.bytes_read, MAX_STDERR_BYTES + 1);
        assert!(diagnostics.lines.is_empty());

        let input = "healthy-native-staging total_us=9\n".repeat(MAX_DIAGNOSTIC_LINES + 1);
        let diagnostics = read_diagnostics(input.as_bytes()).unwrap();
        assert!(diagnostics.truncated);
        assert_eq!(diagnostics.lines.len(), MAX_DIAGNOSTIC_LINES);
        assert!(
            read_diagnostics(b"healthy-native-staging total_us=9".as_slice())
                .unwrap()
                .lines
                .is_empty()
        );
    }
}
