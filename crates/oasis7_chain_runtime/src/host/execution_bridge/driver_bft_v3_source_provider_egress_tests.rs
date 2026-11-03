//! Provider-to-Node real Noise reads after the Node has stopped serving.
//! The independently keyed issuer and protected backend are software test doubles.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReadV1, BftPrivateRetentionAdmissionInputReaderV1,
    BftPrivateRetentionAdmissionInputRefV1,
};
use std::sync::OnceLock;

#[derive(Default)]
pub(crate) struct DeferredOriginalReader {
    reader: OnceLock<Arc<dyn BftPrivateRetentionAdmissionInputReaderV1>>,
    read_calls: std::sync::atomic::AtomicUsize,
}
impl DeferredOriginalReader {
    pub(crate) fn read_calls(&self) -> usize {
        self.read_calls.load(std::sync::atomic::Ordering::Acquire)
    }
}
impl BftPrivateRetentionAdmissionInputReaderV1 for DeferredOriginalReader {
    fn read_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, BftSignerErrorV3> {
        self.read_calls
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        self.reader
            .get()
            .ok_or_else(|| BftSignerErrorV3("provider egress not configured".into()))?
            .read_original(reference, deadline)
    }
}
pub(crate) struct ProviderEgressFixture {
    _server: NodeBftLibp2pAdmissionInputServerV1,
    released: Arc<std::sync::atomic::AtomicUsize>,
}
impl ProviderEgressFixture {
    pub(crate) fn assert_material_released(&self) {
        assert!(
            self.released.load(std::sync::atomic::Ordering::Acquire) > 0,
            "archive must obtain actual material through independently authorized provider Noise replies"
        );
    }
}
struct ProviderIssuer {
    service: std::sync::Weak<BftSignerServiceV3>,
    requester: String,
    serving: String,
    grants: Mutex<BTreeMap<String, BftPrivateRetentionAdmissionReadGrantV1>>,
    released: Arc<std::sync::atomic::AtomicUsize>,
}
impl NodeBftAdmissionInputReadCredentialsV1 for ProviderIssuer {
    fn authorize(
        &self,
        read: &NodeBftAdmissionInputReadRequestV1,
        deadline: Instant,
    ) -> Result<NodeBftAdmissionInputAuthorizationV1, String> {
        self.service
            .upgrade()
            .ok_or("provider issuer service stopped")?
            .with_observed_original_input_view(deadline, |view| {
                if read.requester_peer_id != self.requester
                    || read.serving_peer_id != self.serving
                    || read.reference.source.binding_digest != view.binding_digest()
                {
                    return Err(BftSignerErrorV3(
                        "provider issuer endpoints/source rejected".into(),
                    ));
                }
                let subject = fixtures::key(104);
                let subject_public_key = hex::encode(subject.verifying_key().to_bytes());
                let mut grants = self
                    .grants
                    .lock()
                    .map_err(|_| BftSignerErrorV3("provider issuer lock".into()))?;
                let grant = if let Some(grant) = grants.get(&read.nonce) {
                    if grant.body.reference != read.reference
                        || grant.body.view_generation != view.generation()
                    {
                        return Err(BftSignerErrorV3(
                            "provider issuer original nonce changed".into(),
                        ));
                    }
                    grant.clone()
                } else {
                    let now = now_ms();
                    let grant = BftPrivateRetentionAdmissionReadGrantV1::sign(
                        BftPrivateRetentionAdmissionReadGrantBodyV1 {
                            schema_version: 1,
                            purpose:
                                BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
                            scope: view.schedule().scope().clone(),
                            reference: read.reference.clone(),
                            subject_public_key: subject_public_key.clone(),
                            requester_peer_id: self.requester.clone(),
                            serving_peer_id: self.serving.clone(),
                            schedule_digest: view.schedule().digest().map_err(BftSignerErrorV3)?,
                            view_generation: view.generation(),
                            request_nonce: read.nonce.clone(),
                            grant_epoch: 1,
                            not_before_unix_ms: now,
                            expires_unix_ms: now + 120_000,
                        },
                        &MemoryTestSignerKey(fixtures::key(103)),
                    )?;
                    grants.insert(read.nonce.clone(), grant.clone());
                    grant
                };
                Ok(NodeBftAdmissionInputAuthorizationV1 {
                    signature_hex: hex::encode(
                        subject
                            .sign(&read.signing_bytes(&grant).map_err(BftSignerErrorV3)?)
                            .to_bytes(),
                    ),
                    subject_public_key,
                    grant,
                })
            })
            .map_err(|e| e.to_string())
    }
}
impl NodeBftAdmissionInputReadGateV1 for ProviderIssuer {
    fn validate_identity(
        &self,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
    ) -> Result<(), String> {
        authorization
            .grant
            .verify_signature(&hex::encode(fixtures::key(103).verifying_key().to_bytes()))
            .map_err(|e| e.to_string())
    }
    fn authorize(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
        scope: &NodeBftAdmissionInputReadScopeV1<'_>,
        make_response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        let check = || -> Result<(), String> {
            self.validate_identity(authorization)?;
            let NodeBftAdmissionInputReadViewV1::Provider(view) = scope.view else {
                return Err("provider issuer requires actual opaque provider view".into());
            };
            let g = &authorization.grant;
            if !view.is_current()
                || g.body.reference != scope.reference
                || g.body.view_generation != view.generation()
                || g.body.scope != *view.schedule().scope()
                || g.body.schedule_digest != view.schedule().digest()?
                || g.body.requester_peer_id != self.requester
                || scope.requester_peer_id != self.requester
                || g.body.serving_peer_id != self.serving
                || scope.serving_peer_id != self.serving
                || transport.peer_id().to_string() != self.requester
                || authorization.subject_public_key
                    != hex::encode(fixtures::key(104).verifying_key().to_bytes())
                || g.body.subject_public_key != authorization.subject_public_key
                || g.body.grant_epoch != 1
                || now_ms() < g.body.not_before_unix_ms
                || now_ms() >= g.body.expires_unix_ms
            {
                return Err("provider issuer exact current grant rejected".into());
            }
            if self
                .grants
                .lock()
                .map_err(|_| "provider issuer originals lock")?
                .get(&g.body.request_nonce)
                != Some(g)
            {
                return Err("provider issuer original grant absent".into());
            }
            Ok(())
        };
        check()?;
        let NodeBftAdmissionInputReadViewV1::Provider(view) = scope.view else {
            return Err("provider original view absent".into());
        };
        let deadline = view.deadline().min(
            Instant::now()
                + Duration::from_millis(
                    authorization
                        .grant
                        .body
                        .expires_unix_ms
                        .saturating_sub(now_ms()),
                ),
        );
        transport
            .restrict_reply_until(deadline)
            .map_err(|_| "provider issuer deadline expired")?;
        let response = make_response(deadline)?;
        check()?;
        self.released
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        Ok(response)
    }
}
impl NoiseFixture {
    pub(in super::super) fn provider_egress(
        &self,
        binding: &BftSignerBindingV3,
        backend: BftPrivateRetentionMemoryBackendV1,
        service: Arc<BftSignerServiceV3>,
        slot: &DeferredOriginalReader,
    ) -> ProviderEgressFixture {
        let provider_network = self._networks[1].clone();
        let node_network = self._networks[0].clone();
        let released = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let issuer = Arc::new(ProviderIssuer {
            service: Arc::downgrade(&service),
            requester: node_network.peer_id().to_string(),
            serving: provider_network.peer_id().to_string(),
            grants: Mutex::new(BTreeMap::new()),
            released: released.clone(),
        });
        let source = Arc::new(
            NodeBftBackendAdmissionInputSourceV1::new(
                binding.digest().unwrap(),
                Box::new(backend),
                self.provider_scope.buffers(),
            )
            .unwrap(),
        );
        let server = NodeBftLibp2pAdmissionInputServerV1::new_for_provider(
            provider_network.clone(),
            NodeBftAdmissionInputServerConfigV1 {
                binding: binding.clone(),
                peers: vec![node_network.peer_id().to_string()],
            },
            service,
            self.provider_scope.clone(),
            source,
            issuer.clone(),
        )
        .unwrap();
        let reader = Arc::new(
            NodeBftLibp2pAdmissionInputReaderV1::new(
                node_network,
                provider_network.peer_id().to_string(),
                issuer,
                self.private.transport_buffers(),
            )
            .unwrap(),
        );
        assert!(
            slot.reader.set(reader).is_ok(),
            "provider reader may only bind once"
        );
        ProviderEgressFixture {
            _server: server,
            released,
        }
    }
}
