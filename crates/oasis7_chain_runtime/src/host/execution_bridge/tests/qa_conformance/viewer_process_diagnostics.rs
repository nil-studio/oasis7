//! Bounded, fixed-vocabulary evidence only; never relay child output or credentials.
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::Child;

#[derive(Clone, Copy, Debug)]
pub(super) enum Phase {
    Connecting,
    Connected,
    AwaitingHelloAck,
    AwaitingResponse,
    ResponseReceived,
    RejectingSnapshot,
}

pub(super) fn child_status(child: &mut Child) -> String {
    match child.try_wait() {
        Ok(None) => "running_at_observation".into(),
        Ok(Some(status)) => match status.code() {
            Some(code) => format!("exited_code_{code}"),
            None => {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    if let Some(signal) = status.signal() {
                        return format!("exited_signal_{signal}");
                    }
                }
                "exited_without_code".into()
            }
        },
        Err(_) => "status_unavailable".into(),
    }
}

const LOG_LIMIT: u64 = 64 * 1024;

pub(super) fn log_summary(path: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return "unavailable".into();
    };
    let Ok(metadata) = file.metadata() else {
        return "metadata_unavailable".into();
    };
    let bytes = metadata.len();
    // Read only a bounded tail. A missing marker is not evidence of no error.
    if file
        .seek(SeekFrom::Start(bytes.saturating_sub(LOG_LIMIT)))
        .is_err()
    {
        return "seek_unavailable".into();
    }
    let mut tail = Vec::new();
    if file.take(LOG_LIMIT).read_to_end(&mut tail).is_err() {
        return "read_unavailable".into();
    }
    format!(
        "bytes={bytes} tail_truncated={} {}",
        bytes > LOG_LIMIT,
        classify(&tail)
    )
}

fn classify(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    // Presence flags are hints, not authenticated runtime events or root-cause proof.
    // Only fixed booleans leave this function, even for invalid UTF-8 or secret text.
    format!(
        "startup_failure_marker={} session_error_marker={} bind_conflict_marker={} panic_marker={}",
        text.contains("oasis7_viewer_live failed")
            || text.contains("failed to parse viewer live options"),
        text.contains("viewer runtime live server error"),
        text.contains("Address already in use") || text.contains("AddrInUse"),
        text.contains("panicked at")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn natural_exit_is_not_confused_with_cleanup() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 17"])
            .spawn()
            .unwrap();
        child.wait().unwrap();
        assert_eq!(child_status(&mut child), "exited_code_17");
        let mut live = std::process::Command::new("/bin/sleep")
            .arg("10")
            .spawn()
            .unwrap();
        assert_eq!(child_status(&mut live), "running_at_observation");
        live.kill().unwrap();
        live.wait().unwrap();
        assert_eq!(child_status(&mut live), "exited_signal_9");
    }

    #[test]
    fn classifies_without_relaying_private_output() {
        let result =
            classify(b"secret-token /private/path oasis7_viewer_live failed AddrInUse\xff");
        assert_eq!(
            result,
            "startup_failure_marker=true session_error_marker=false bind_conflict_marker=true panic_marker=false"
        );
        assert!(!result.contains("secret-token"));
        assert!(!result.contains("private/path"));
    }

    #[test]
    fn empty_and_unknown_are_not_success() {
        assert_eq!(
            classify(b""),
            classify(b"unrecognized failure credential=hidden")
        );
        assert!(
            classify(b"viewer runtime live server error: secret\npanicked at private")
                .contains("session_error_marker=true")
        );
        assert_eq!(
            log_summary(Path::new("/nonexistent-viewer-log-file")),
            "unavailable"
        );
    }

    #[test]
    fn bounded_tail_reports_truncation_without_raw_content() {
        let path =
            std::env::temp_dir().join(format!("viewer-diagnostic-test-{}", std::process::id()));
        let mut bytes = vec![b'x'; LOG_LIMIT as usize + 1];
        bytes.extend_from_slice(b"panicked at secret");
        std::fs::write(&path, &bytes).unwrap();
        let result = log_summary(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(result.contains("tail_truncated=true"));
        assert!(result.contains("panic_marker=true"));
        assert!(!result.contains("secret"));
    }
}
