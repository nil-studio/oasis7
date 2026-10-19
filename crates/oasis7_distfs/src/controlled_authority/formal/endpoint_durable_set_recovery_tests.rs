//! Real management RPCs around native activation failures. The provider fixture
//! owns the committed state and lease; server memory is deliberately discarded.
use super::*;
use crate::controlled_authority::formal::transport::{
    DurableSetCandidateServerV1, RemoteDurableSetCandidateV1,
};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) fn verify_management_recovery(
    candidate: DurableSetCandidateV1,
    path: &Path,
    provider: Arc<InstallProvider>,
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    primary: DurableSetInstallReceiptV1,
    replica: DurableSetInstallReceiptV1,
) -> FileFormalEndpoint {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let failures = Arc::new(AtomicUsize::new(0));
    let server_stop = stop.clone();
    let server_failures = failures.clone();
    let server_provider = provider.clone();
    let server_sealed = sealed.clone();
    let path = path.to_path_buf();
    let server = std::thread::spawn(move || {
        let mut server = DurableSetCandidateServerV1::new(
            candidate,
            server_sealed,
            EndpointRole::Primary,
            Duration::from_secs(3),
        )
        .unwrap();
        let mut active = None;
        while !server_stop.load(Ordering::Acquire) {
            let result = server.serve_one_recovering(
                &listener,
                &path,
                server_provider.clone(),
                64 * 1024 * 1024,
            );
            if server_stop.load(Ordering::Acquire) {
                break;
            }
            match result {
                Ok(Some(endpoint)) => {
                    assert!(active.is_none(), "only one ordinary owner may be returned");
                    active = Some(endpoint);
                }
                Ok(None) => {}
                Err(_) => {
                    server_failures.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        active
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut client = RemoteDurableSetCandidateV1::new(
            address,
            Duration::from_secs(3),
            sealed.clone(),
            EndpointRole::Primary,
            key(4),
        )
        .unwrap();
        provider.fail_activation_once(false);
        assert!(client.activate(primary.clone(), replica.clone()).is_err());
        assert_eq!(provider.activation_calls.load(Ordering::SeqCst), 1);
        // This next authenticated RPC must reopen Installed, release the failed
        // owner/lease, and retry the SAME activation. Its reply is then lost.
        provider.fail_activation_once(true);
        assert!(client.activate(primary.clone(), replica.clone()).is_err());
        assert_eq!(provider.activation_calls.load(Ordering::SeqCst), 2);
        let queries = provider.query_calls.load(Ordering::SeqCst);
        // A different but correctly signed dual pair must fail BEFORE recovery
        // queries. Merely checking the caller's signature is insufficient.
        let mut other_primary = primary.clone();
        let mut other_replica = replica.clone();
        for (receipt, signer) in [(&mut other_primary, 5), (&mut other_replica, 12)] {
            receipt.body.independent_inventory_digest = h(195);
            receipt.endpoint_signature_hex = signed(
                durable_set_install_endpoint_signing_bytes(&receipt.body).unwrap(),
                signer,
            );
            receipt.provider_signature_hex = signed(
                durable_set_install_provider_signing_bytes(&receipt.body).unwrap(),
                3,
            );
        }
        verify_durable_set_activation(
            sealed.transition(),
            [
                &sealed.evidence().primary_seal,
                &sealed.evidence().replica_seal,
            ],
            [&other_primary, &other_replica],
        )
        .unwrap();
        assert!(client.activate(other_primary, other_replica).is_err());
        assert_eq!(provider.query_calls.load(Ordering::SeqCst), queries);
        assert_eq!(provider.activation_calls.load(Ordering::SeqCst), 2);
        // Rollback while querying Active remains a failed RPC. It must not
        // fall back to Installed and issue a replacement activation.
        provider.fail_query_once();
        assert!(client.query_install().is_err());
        assert_eq!(provider.query_calls.load(Ordering::SeqCst), queries + 1);
        assert_eq!(provider.activation_calls.load(Ordering::SeqCst), 2);
        // Fresh query now proves Active. No second provider activation is needed
        // and the original dual receipts, boundary and owner are preserved.
        assert_eq!(client.query_install().unwrap(), primary);
        assert_eq!(provider.activation_calls.load(Ordering::SeqCst), 2);
        assert!(provider.query_calls.load(Ordering::SeqCst) > queries);
        assert_eq!(
            client.activate(primary.clone(), replica.clone()).unwrap(),
            primary
        );
        assert_eq!(client.install().unwrap(), primary);
        assert_eq!(failures.load(Ordering::SeqCst), 4);
    }));
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    let active = server.join().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    active.expect("fresh protected active reopen must return the real ordinary owner")
}
