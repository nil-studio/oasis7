//! Opt-in operator evidence only; never changes admission or execution authority.
use oasis7::runtime::World;
use oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1;

pub(super) fn observe(
    stage: &'static str,
    request_commitment: &str,
    captured: &RuntimeCognitionBaseBindingV1,
    world: &World,
    correlation: Option<&oasis7::world_service::RequestCorrelation>,
) {
    if std::env::var("OASIS7_COGNITION_AUTHORITY_DIAGNOSTIC").as_deref() != Ok("1") {
        return;
    }
    let current_tick = world.state().time;
    let action_bytes = correlation.and_then(|value| serde_json::to_vec(value).ok());
    let value = evidence(
        stage,
        request_commitment,
        captured,
        current_tick,
        action_bytes.as_deref(),
    );
    if let Some(sender) = writer() {
        let _ = enqueue(sender, format!("cognition_authority_diagnostic {value}"));
    }
}

fn spawn_writer<W: std::io::Write + Send + 'static>(
    mut sink: W,
) -> Option<std::sync::mpsc::SyncSender<String>> {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<String>(16);
    // Never join: even a permanently blocked sink must not delay shutdown.
    std::thread::Builder::new()
        .name("cognition-diagnostic".into())
        .spawn(move || {
            for line in receiver {
                if writeln!(sink, "{line}").is_err() {
                    break;
                }
            }
        })
        .ok()
        .map(|_| sender)
}

fn writer() -> Option<&'static std::sync::mpsc::SyncSender<String>> {
    static WRITER: std::sync::OnceLock<Option<std::sync::mpsc::SyncSender<String>>> =
        std::sync::OnceLock::new();
    WRITER
        .get_or_init(|| {
            #[cfg(unix)]
            {
                use std::os::fd::AsFd;
                // Duplicate the fd without holding Rust's global stderr lock.
                let owned = std::io::stderr().as_fd().try_clone_to_owned().ok()?;
                spawn_writer(std::fs::File::from(owned))
            }
            #[cfg(not(unix))]
            {
                None
            }
        })
        .as_ref()
}

fn enqueue(sender: &std::sync::mpsc::SyncSender<String>, line: String) -> bool {
    sender.try_send(line).is_ok()
}

fn evidence(
    stage: &str,
    request_commitment: &str,
    captured: &RuntimeCognitionBaseBindingV1,
    current_tick: u64,
    action_bytes: Option<&[u8]>,
) -> serde_json::Value {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"oasis7/operator-cognition-authority/v1\0");
    hasher.update(request_commitment.as_bytes());
    serde_json::json!({
        "stage": stage,
        "context_digest": hasher.finalize().to_hex().to_string(),
        "action_digest": action_bytes.map(|bytes| {
            let mut action = blake3::Hasher::new();
            action.update(b"oasis7/operator-cognition-action/v1\0");
            action.update(bytes);
            action.finalize().to_hex().to_string()
        }),
        "captured_tick": captured.base_tick,
        "current_tick": current_tick,
        "base_hash_equal": null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_links_stages_without_disclosing_authority_values() {
        let binding = RuntimeCognitionBaseBindingV1 {
            world_id: "private-world".into(),
            branch_id: "private-branch".into(),
            finality_epoch: 0,
            finality_block_hash: None,
            finality_status: "pending".into(),
            base_tick: 7,
            base_world_hash: "private-hash".into(),
            reorg_epoch: 0,
            runtime_manifest_hash: "private-manifest".into(),
        };
        let a = evidence("delivery", "private-request", &binding, 7, None);
        let b = evidence(
            "execution",
            "private-request",
            &binding,
            8,
            Some(b"nonce-and-payload"),
        );
        assert_eq!(a["context_digest"], b["context_digest"]);
        assert!(a["action_digest"].is_null());
        let admitted = evidence(
            "admission",
            "private-request",
            &binding,
            7,
            Some(b"nonce-and-payload"),
        );
        let different = evidence(
            "execution",
            "private-request",
            &binding,
            7,
            Some(b"same-nonce-different-payload"),
        );
        assert_eq!(admitted["action_digest"], b["action_digest"]);
        assert_ne!(different["action_digest"], b["action_digest"]);
        assert!(a["base_hash_equal"].is_null());
        assert!(b["base_hash_equal"].is_null());
        assert_eq!(b["current_tick"], 8);
        assert!(!a.to_string().contains("private"));
        assert!(!b.to_string().contains("private"));
    }

    #[test]
    fn blocked_writer_cannot_block_producer_and_failed_sink_disconnects() {
        struct BlockingSink {
            entered: std::sync::mpsc::SyncSender<()>,
            release: std::sync::mpsc::Receiver<()>,
        }
        impl std::io::Write for BlockingSink {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                let _ = self.entered.try_send(());
                let _ = self.release.recv();
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (entered, observed) = std::sync::mpsc::sync_channel(1);
        let (release, wait) = std::sync::mpsc::channel();
        let sender = spawn_writer(BlockingSink {
            entered,
            release: wait,
        })
        .unwrap();
        assert!(enqueue(&sender, "first".into()));
        observed
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let (done, completion) = std::sync::mpsc::channel();
        let producer = sender.clone();
        std::thread::spawn(move || {
            let accepted = (0..64)
                .filter(|_| enqueue(&producer, "queued".into()))
                .count();
            done.send(accepted).unwrap();
        });
        // Release even on failure, so a failed regression leaves no blocked writer.
        let result = completion.recv_timeout(std::time::Duration::from_secs(2));
        release.send(()).unwrap();
        assert_eq!(result.unwrap(), 16);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut disconnected = false;
        while std::time::Instant::now() < deadline {
            if matches!(
                sender.try_send("after-close".into()),
                Err(std::sync::mpsc::TrySendError::Disconnected(_))
            ) {
                disconnected = true;
                break;
            }
            std::thread::yield_now();
        }
        assert!(disconnected, "failed sink must drop its receiver");
        assert!(!enqueue(&sender, "disconnected".into()));
    }
}
