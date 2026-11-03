use super::*;
pub(super) struct StopNextPrecommit {
    pub(super) snapshot: BackendState,
    pub(super) calls: Arc<std::sync::atomic::AtomicUsize>,
}
impl BftSigningKeyHandleV3 for StopNextPrecommit {
    fn public_key_hex(&self) -> String {
        hex::encode(fixtures::key(120).verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        if bytes
            .windows(b"oasis7.bft.vote.v3.sign".len())
            .any(|w| w == b"oasis7.bft.vote.v3.sign")
            && bytes.windows(b"precommit".len()).any(|w| w == b"precommit")
            && !self.snapshot.lock().unwrap().state.history.is_empty()
        {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(BftSignerErrorV3(
                "injected original T+1 PC interruption".into(),
            ));
        }
        Ok(fixtures::key(120).sign(bytes).to_bytes())
    }
}
pub(super) struct ReadbackOnlyKey(pub(super) Arc<std::sync::atomic::AtomicUsize>);
impl BftSigningKeyHandleV3 for ReadbackOnlyKey {
    fn public_key_hex(&self) -> String {
        hex::encode(fixtures::key(120).verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(fixtures::key(120).sign(bytes).to_bytes())
    }
}
