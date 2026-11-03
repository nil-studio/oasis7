//! Confidential originals before reservation, separately authorized from state reads.
use super::*;
use crate::node_bft_v3_runtime::private_buffers::BufferedBytes;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input, BftPrivateRetentionAdmissionReadGrantV1,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBftAdmissionInputAuthorizationV1 {
    pub grant: BftPrivateRetentionAdmissionReadGrantV1,
    pub subject_public_key: String,
    pub signature_hex: String,
}
pub struct NodeBftAdmissionInputReadScopeV1<'a> {
    pub view: NodeBftAdmissionInputReadViewV1<'a>,
    pub reference: InputRef,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub subject_public_key: String,
}
pub trait NodeBftAdmissionInputReadGateV1: Send + Sync {
    /// Reject malformed/foreign issuer identity before provider-state work.
    /// This supplies no fresh witness, current view or release permission.
    fn validate_identity(
        &self,
        _authorization: &NodeBftAdmissionInputAuthorizationV1,
    ) -> Result<(), String> {
        Err("independent admission issuer identity unsupported".into())
    }
    /// Independently issued original-input permission and fresh revocation must
    /// pass before and after the callback; only narrow the transport release.
    fn authorize(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
        scope: &NodeBftAdmissionInputReadScopeV1<'_>,
        make_response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String>;
}
/// This subject signer must obtain the independent issuer's exact nonce/ref
/// permission. A full-state grant or an attestor role supplies no permission.
pub trait NodeBftAdmissionInputReadCredentialsV1: Send + Sync {
    fn authorize(
        &self,
        request: &NodeBftAdmissionInputReadRequestV1,
        deadline: Instant,
    ) -> Result<NodeBftAdmissionInputAuthorizationV1, String>;
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBftAdmissionInputReadRequestV1 {
    pub reference: InputRef,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub nonce: String,
    pub operation: BftCandidateReadOperationV3,
}
impl NodeBftAdmissionInputReadRequestV1 {
    pub fn signing_bytes(
        &self,
        grant: &BftPrivateRetentionAdmissionReadGrantV1,
    ) -> Result<Vec<u8>, String> {
        let mut bytes = b"oasis7.bft.admission-input.subject-read.v1\0".to_vec();
        bytes.extend(super::wire::encode(&(self, grant))?);
        if bytes.len() > super::wire::MAX_FRAME {
            return Err("admission subject request capacity".into());
        }
        Ok(bytes)
    }
}
/// The private constructor binds actual buffer ownership to every cloned reply.
/// Persistent source implementations must acquire the same transport budget.
pub struct NodeBftAdmissionInputBytesV1 {
    pub(crate) reference: InputRef,
    pub(crate) bytes: Arc<BufferedBytes>,
    pub(crate) retained_owner: Option<Arc<Input>>,
}
pub trait NodeBftAdmissionInputSourceV1: Send + Sync {
    /// Publisher-owned bytes already have an explicit clear lifetime; another
    /// strong cache would obstruct the next existing peak publication permit.
    fn retains_encoded_original(&self) -> bool {
        false
    }
    fn original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<Arc<NodeBftAdmissionInputBytesV1>, String>;
    /// Fresh existence/immutable-owner check only, never a read permission.
    /// An error must not fall back to a previously encoded original.
    fn revalidate_original(
        &self,
        _original: &NodeBftAdmissionInputBytesV1,
        _deadline: Instant,
    ) -> Result<bool, String> {
        Err("admission source fresh owner verification unsupported".into())
    }
}
/// One immutable ingress original. It is not durable reservation evidence.
pub struct NodeBftAdmissionInputPublisherV1 {
    buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
    current: Mutex<Option<Arc<NodeBftAdmissionInputBytesV1>>>,
    reserved: AtomicBool,
    recovery_reader:
        Option<Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1>>,
}
impl NodeBftAdmissionInputBytesV1 {
    pub fn from_original(
        input: &Input,
        buffers: &crate::NodeBftPrivateTransportBuffersV1,
    ) -> Result<Arc<Self>, String> {
        let hold = buffers.outbound(2 * super::wire::MAX_PACKAGE)?;
        let (bytes, reference) = input
            .canonical_with_descriptor()
            .map_err(|_| "admission original encoding rejected")?;
        Ok(Arc::new(Self {
            reference,
            bytes: Arc::new(BufferedBytes::new(bytes, hold)?),
            retained_owner: None,
        }))
    }
    pub(super) fn from_retained_original(
        input: &Arc<Input>,
        buffers: &crate::NodeBftPrivateTransportBuffersV1,
    ) -> Result<Arc<Self>, String> {
        let mut original = Self::from_original(input, buffers)?;
        Arc::get_mut(&mut original)
            .ok_or("admission encoded owner unexpectedly shared")?
            .retained_owner = Some(input.clone());
        Ok(original)
    }
}
impl NodeBftAdmissionInputPublisherV1 {
    pub fn new(buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>) -> Self {
        Self {
            buffers,
            current: Mutex::new(None),
            reserved: AtomicBool::new(false),
            recovery_reader: None,
        }
    }
    pub fn with_original_reader(
        mut self,
        reader: Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1>,
    ) -> Self {
        self.recovery_reader = Some(reader);
        self
    }
    /// Reserve before copying Data/dependencies into the detached Input DTO.
    pub fn reserve(self: &Arc<Self>) -> Result<NodeBftAdmissionInputPublicationV1, String> {
        self.reserved
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "admission original already in flight")?;
        let result = (|| {
            if self
                .current
                .lock()
                .map_err(|_| "admission ingress poisoned")?
                .is_some()
            {
                return Err("admission original already in flight".into());
            }
            self.buffers.outbound(2 * super::wire::MAX_PACKAGE)
        })();
        match result {
            Ok(hold) => Ok(NodeBftAdmissionInputPublicationV1 {
                publisher: self.clone(),
                hold: Some(hold),
            }),
            Err(error) => {
                self.reserved.store(false, Ordering::Release);
                Err(error)
            }
        }
    }
    pub fn clear(&self, reference: &InputRef) -> Result<(), String> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| "admission ingress poisoned")?;
        if current
            .as_ref()
            .is_none_or(|old| old.reference != *reference)
        {
            return Err("admission ingress differs from original".into());
        }
        *current = None;
        Ok(())
    }
}
/// An owned ingress resource permit, never signing/read authority. Dropping an
/// unused slot releases backpressure; a published original keeps its byte permit.
pub struct NodeBftAdmissionInputPublicationV1 {
    publisher: Arc<NodeBftAdmissionInputPublisherV1>,
    hold: Option<crate::node_bft_v3_runtime::private_buffers::BufferHold>,
}
impl NodeBftAdmissionInputPublicationV1 {
    pub fn publish(mut self, input: Input) -> Result<InputRef, String> {
        let (bytes, reference) = input
            .canonical_with_descriptor()
            .map_err(|_| "admission original encoding rejected")?;
        drop(input);
        let hold = self
            .hold
            .take()
            .ok_or("admission publication permit missing")?;
        let original = Arc::new(NodeBftAdmissionInputBytesV1 {
            reference: reference.clone(),
            bytes: Arc::new(BufferedBytes::new(bytes, hold)?),
            retained_owner: None,
        });
        let mut current = self
            .publisher
            .current
            .lock()
            .map_err(|_| "admission ingress poisoned")?;
        if current.is_some() {
            return Err("admission original already published".into());
        }
        *current = Some(original);
        Ok(reference)
    }
}
impl Drop for NodeBftAdmissionInputPublicationV1 {
    fn drop(&mut self) {
        self.publisher.reserved.store(false, Ordering::Release);
    }
}
impl NodeBftAdmissionInputSourceV1 for NodeBftAdmissionInputPublisherV1 {
    fn retains_encoded_original(&self) -> bool {
        true
    }
    fn revalidate_original(
        &self,
        original: &NodeBftAdmissionInputBytesV1,
        deadline: Instant,
    ) -> Result<bool, String> {
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        let current = self
            .current
            .lock()
            .map_err(|_| "admission ingress poisoned")?;
        let current = current
            .as_ref()
            .ok_or("admission exact original unavailable")?;
        if current.reference != original.reference {
            return Err("admission exact original unavailable".into());
        }
        Ok(
            Arc::ptr_eq(&current.bytes, &original.bytes)
                && original.bytes.belongs_to(&self.buffers),
        )
    }
    fn original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<Arc<NodeBftAdmissionInputBytesV1>, String> {
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        self.current
            .lock()
            .map_err(|_| "admission ingress poisoned")?
            .as_ref()
            .filter(|original| &original.reference == reference)
            .cloned()
            .ok_or_else(|| "admission exact original unavailable".into())
    }
}

#[path = "admission_input_backend.rs"]
mod input_backend;
pub use input_backend::NodeBftBackendAdmissionInputSourceV1;

#[path = "admission_input_client.rs"]
mod input_client;
#[path = "admission_input_server.rs"]
mod input_server;
#[path = "admission_input_wire.rs"]
mod input_wire;
pub use input_client::NodeBftLibp2pAdmissionInputReaderV1;
pub use input_server::{NodeBftAdmissionInputServerConfigV1, NodeBftLibp2pAdmissionInputServerV1};

#[cfg(test)]
#[path = "admission_input_tests.rs"]
mod tests;

impl crate::NodeBftAdmissionInputPublicationSlotV1 for NodeBftAdmissionInputPublicationV1 {
    fn publish(self: Box<Self>, input: Input) -> Result<InputRef, String> {
        (*self).publish(input)
    }
}
impl crate::NodeBftAdmissionInputPublicationIoV1 for Arc<NodeBftAdmissionInputPublisherV1> {
    fn read_original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReadV1, String>
    {
        self.recovery_reader
            .as_ref()
            .ok_or("authorized admission recovery reader not configured")?
            .read_original(reference, deadline)
            .map_err(|e| e.to_string())
    }

    fn contains(&self, reference: &InputRef, deadline: Instant) -> Result<bool, String> {
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        let current = self
            .current
            .lock()
            .map_err(|_| "admission ingress poisoned")?;
        Ok(current.as_ref().is_some_and(|v| &v.reference == reference))
    }

    fn reserve(&self) -> Result<Box<dyn crate::NodeBftAdmissionInputPublicationSlotV1>, String> {
        Ok(Box::new(NodeBftAdmissionInputPublisherV1::reserve(self)?))
    }
    fn clear(&self, reference: &InputRef) -> Result<(), String> {
        NodeBftAdmissionInputPublisherV1::clear(self, reference)
    }
}

#[path = "admission_input_hint.rs"]
mod input_hint;
pub use input_hint::NodeBftAdmissionInputCurrentViewV1;

#[path = "admission_input_view.rs"]
mod input_view;
pub use input_view::NodeBftAdmissionInputReadViewV1;

#[cfg(test)]
#[path = "admission_input_codec_tests.rs"]
mod codec_tests;

#[path = "admission_input_batch_wire.rs"]
mod batch_wire;
#[path = "admission_input_batch.rs"]
mod batch;
pub use batch::*;
#[path = "admission_input_batch_hint.rs"]
mod batch_hint;
#[path = "admission_input_batch_client.rs"]
mod batch_client;
pub use batch_client::NodeBftLibp2pAdmissionInputReaderV2;
#[path = "admission_input_batch_server.rs"]
mod batch_server;
pub use batch_server::{NodeBftAdmissionInputServerConfigV2, NodeBftLibp2pAdmissionInputServerV2};
