//! Backend-retained original material source for the independent encrypted
//! admission server. This source supplies no ReadGrant or current authority.
use super::*;
use oasis7_distfs::protected_bft_v3::BftSignerBackendV3;

/// The provider factory explicitly gives its independent input server the same
/// original protected namespace. This is never exported through signer RPCs.
pub struct NodeBftBackendAdmissionInputSourceV1 {
    binding_digest: String,
    backend: Mutex<Box<dyn BftSignerBackendV3>>,
    buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
}
impl NodeBftBackendAdmissionInputSourceV1 {
    pub fn new(
        binding_digest: String,
        backend: Box<dyn BftSignerBackendV3>,
        buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
    ) -> Result<Self, String> {
        if binding_digest.len() != 64
            || !binding_digest
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("backend admission source binding digest rejected".into());
        }
        Ok(Self {
            binding_digest,
            backend: Mutex::new(backend),
            buffers,
        })
    }
}
impl NodeBftAdmissionInputSourceV1 for NodeBftBackendAdmissionInputSourceV1 {
    fn revalidate_original(
        &self,
        original: &NodeBftAdmissionInputBytesV1,
        deadline: Instant,
    ) -> Result<bool, String> {
        original.reference.validate().map_err(|e| e.to_string())?;
        if Instant::now() >= deadline
            || original.reference.source.binding_digest != self.binding_digest
            || !original.bytes.belongs_to(&self.buffers)
        {
            return Err("backend cached original target/deadline/pool rejected".into());
        }
        let input = self
            .backend
            .try_lock()
            .map_err(|_| "backend original input source busy or poisoned")?
            .load_private_retention_input(&self.binding_digest, &original.reference)
            .map_err(|e| e.to_string())?;
        if Instant::now() >= deadline {
            return Err("backend original input deadline expired".into());
        }
        Ok(original
            .retained_owner
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, &input)))
    }
    fn original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<Arc<NodeBftAdmissionInputBytesV1>, String> {
        reference.validate().map_err(|e| e.to_string())?;
        if Instant::now() >= deadline || reference.source.binding_digest != self.binding_digest {
            return Err("backend original input target/deadline rejected".into());
        }
        // No blocking mutex may outlive the inherited absolute grant deadline.
        // Busy ownership is retryable; it is never a missing original or legacy.
        let input = self
            .backend
            .try_lock()
            .map_err(|_| "backend original input source busy or poisoned")?
            .load_private_retention_input(&self.binding_digest, reference)
            .map_err(|e| e.to_string())?;
        if Instant::now() >= deadline {
            return Err("backend original input deadline expired".into());
        }
        reference
            .validate_input(&input)
            .map_err(|e| e.to_string())?;
        // The immutable backend payload is borrowed. Only its one encoded
        // response acquires the existing shared outbound peak reservation.
        let bytes = NodeBftAdmissionInputBytesV1::from_retained_original(&input, &self.buffers)?;
        if Instant::now() >= deadline || bytes.reference != *reference {
            return Err("backend encoded original input target/deadline changed".into());
        }
        Ok(bytes)
    }
}
