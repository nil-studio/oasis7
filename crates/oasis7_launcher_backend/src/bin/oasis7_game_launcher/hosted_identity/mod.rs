//! Hosted account authority. Account authentication never allocates a world slot.
mod config;
mod credentials;
mod email;
mod entry_executor_control;
mod entry_owner_control;
mod machine_registration;
pub mod model;
mod model_session;
mod native_key;
mod participant_entry;
mod recovery;
mod service;
#[cfg(test)]
mod session_tests;
pub(super) mod snapshot_export;
pub mod store;
pub mod webauthn;
mod world;
pub(super) use config::IdentityConfig;
pub(super) use service::{IdentityReply, IdentityService, header};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use store::StoreError;
fn random_token() -> String {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("operating system random source unavailable");
    URL_SAFE_NO_PAD.encode(bytes)
}
fn digest(secret: &[u8; 32], purpose: &str, value: &str) -> String {
    let mut material = Vec::with_capacity(purpose.len() + value.len() + 1);
    material.extend_from_slice(purpose.as_bytes());
    material.push(0);
    material.extend_from_slice(value.as_bytes());
    blake3::keyed_hash(secret, &material).to_hex().to_string()
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn reject(code: &str) -> StoreError {
    StoreError::Rejected(code.to_owned())
}

struct ClockGuard {
    instant: std::time::Instant,
    utc: u64,
    healthy: bool,
}
impl ClockGuard {
    fn new() -> Self {
        Self {
            instant: std::time::Instant::now(),
            utc: now_ms(),
            healthy: true,
        }
    }
    fn observe(&mut self, now: u64) -> bool {
        let expected = self
            .utc
            .saturating_add(self.instant.elapsed().as_millis() as u64);
        if now.abs_diff(expected) > 5000 {
            self.healthy = false;
        }
        self.healthy
    }
}
pub(super) fn run_maintenance(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    runtime: std::sync::Arc<super::static_http::StaticHttpRuntimeConfig>,
) {
    while !stop.load(std::sync::atomic::Ordering::Acquire) {
        if let Ok(mut identity) = runtime.identity.lock()
            && let Some(service) = identity.as_mut()
        {
            let now = now_ms();
            if service.clock.observe(now) {
                let _ = service.retry_recovery_notifications(now);
                let _ = service.store.cleanup_expired(now);
            } else {
                service.email.clock_healthy = false;
            }
        }
        if let Some(client) = &runtime.model_client {
            let _ = client.manager.sweep();
        }
        for _ in 0..50 {
            if stop.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
