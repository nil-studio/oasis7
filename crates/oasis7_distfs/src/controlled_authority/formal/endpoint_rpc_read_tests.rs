use super::*;
use crate::controlled_authority::formal::coordinator::FormalDurabilityEndpoint;
use crate::controlled_authority::formal::crypto::invalid;
use ed25519_dalek::Signer;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

#[path = "endpoint_rpc_read_test_server.rs"]
mod owned_server;
use owned_server::{OwnedRpcServer, RPC_TIMEOUT};

struct CountingProvider {
    inner: Arc<dyn fencing::MonotonicEndpointProvider>,
    acquisitions: Arc<AtomicUsize>,
}

impl fencing::MonotonicEndpointProvider for CountingProvider {
    fn initialize_protected(
        &self,
        instance: &fencing::EndpointInstanceBindingV1,
        registration: fencing::ProtectedEndpointRegistrationV1,
    ) -> Result<Box<dyn fencing::MonotonicEndpointGuard>, fencing::FencingError> {
        self.inner.initialize_protected(instance, registration)
    }

    fn acquire(
        &self,
        instance: &fencing::EndpointInstanceBindingV1,
        checkpoint: &fencing::EndpointCheckpointBindingV1,
    ) -> Result<Box<dyn fencing::MonotonicEndpointGuard>, fencing::FencingError> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        self.inner.acquire(instance, checkpoint)
    }
}

fn signed_rpc(
    address: SocketAddr,
    authority: &VerifiedAuthorityV1,
    minimum: FormalHeadRefV1,
    command: super::super::super::super::transport::Command,
) -> Result<super::super::super::super::transport::Response, FormalError> {
    use super::super::super::super::transport::{Request, RequestBody};
    use super::super::super::super::{crypto, transport};

    let instance = &authority.definition().primary.instance;
    let body = RequestBody {
        archive_read_grant: None,
        schema_version: 1,
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id()?,
        instance_digest: instance
            .digest()
            .map_err(super::super::super::super::endpoint::fencing_error)?,
        challenge: fencing::fresh_challenge()
            .map_err(super::super::super::super::endpoint::fencing_error)?,
        minimum,
        command,
    };
    let request_digest = crypto::hash("oasis7.formal.endpoint.rpc.request_digest.v1", &body)?;
    let request = Request {
        signature: hex::encode(
            key(4)
                .sign(&crypto::bytes(transport::REQUEST_DOMAIN, &body)?)
                .to_bytes(),
        ),
        body,
    };
    let bytes = serde_cbor::to_vec(&request).map_err(|error| invalid(error.to_string()))?;
    let mut stream = TcpStream::connect_timeout(&address, RPC_TIMEOUT)
        .map_err(|error| invalid(error.to_string()))?;
    stream
        .set_read_timeout(Some(RPC_TIMEOUT))
        .map_err(|error| invalid(error.to_string()))?;
    stream
        .set_write_timeout(Some(RPC_TIMEOUT))
        .map_err(|error| invalid(error.to_string()))?;
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|error| invalid(error.to_string()))?;
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| invalid(error.to_string()))?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > 64 * 1024 * 1024 {
        return Err(invalid("formal RPC test response frame bound"));
    }
    let mut bytes = vec![0; length];
    stream
        .read_exact(&mut bytes)
        .map_err(|error| invalid(error.to_string()))?;
    let response: transport::Response =
        serde_cbor::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?;
    if response.body.request_digest != request_digest
        || response.body.challenge != request.body.challenge
    {
        return Err(invalid("formal RPC test response request binding"));
    }
    let response_digest = crypto::hash(
        "oasis7.formal.endpoint.rpc.response_body.v1",
        &response.body,
    )?;
    crypto::verify(
        transport::RESPONSE_DOMAIN,
        &response_digest,
        &response.signature,
        &instance.endpoint_public_key,
    )?;
    super::super::super::native_rpc::verify_formal_rpc_checkpoint(
        &response.body,
        authority,
        EndpointRole::Primary,
    )?;
    let protected = response
        .protected_signature
        .as_ref()
        .ok_or(FormalError::FencingUnproven)?;
    let signing = fencing::ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance
            .digest()
            .map_err(super::super::super::super::endpoint::fencing_error)?,
        checkpoint: response.body.checkpoint.clone(),
        challenge: protected.body.challenge.clone(),
        purpose: fencing::ProtectedEndpointSignaturePurposeV1::RpcResponse,
        preimage: crypto::bytes(transport::RESPONSE_DOMAIN, &response_digest)?,
    };
    let signature = fencing::verify_protected_endpoint_signature(
        protected,
        &signing,
        instance,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(super::super::super::super::endpoint::fencing_error)?;
    if signature != response.signature {
        return Err(FormalError::FencingUnproven);
    }
    Ok(response)
}

#[test]
fn signed_read_rpcs_use_one_guard_and_still_verify_minimum_and_sealed_reads() {
    if crate::test_process::run_isolated(
        concat!(
            module_path!(),
            "::signed_read_rpcs_use_one_guard_and_still_verify_minimum_and_sealed_reads"
        ),
        Duration::from_secs(180),
    ) {
        return;
    }
    let fixture = Arc::new(Fixture::new());
    let primary_oracle = Arc::new(Oracle::start(&fixture, EndpointRole::Primary));
    let replica_oracle = Arc::new(Oracle::start(&fixture, EndpointRole::Replica));
    let open = |role, oracle: &Oracle| {
        FileFormalEndpoint::open_protected(
            if role == EndpointRole::Primary {
                &fixture.pdir
            } else {
                &fixture.rdir
            },
            FormalEndpointAuthorizationV1 {
                authority: fixture.authority.clone(),
                original: fixture.authority.clone(),
                recovery: None,
                parent: None,
            },
            role,
            oracle.provider(&fixture),
            None,
        )
        .unwrap()
    };
    let mut coordinator = FormalCoordinator::new(
        open(EndpointRole::Primary, &primary_oracle),
        open(EndpointRole::Replica, &replica_oracle),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let first = coordinator.submit(fixture.initial.clone()).unwrap();
    let first_commit = verify_formal_qualified(&first, &fixture.authority, None, None).unwrap();
    let (input, material) = fixture.frozen(&first_commit, 33);
    let frozen = coordinator.freeze(input, material).unwrap();
    let second = coordinator.submit(fixture.candidate(&frozen)).unwrap();
    let second_commit =
        verify_formal_qualified(&second, &fixture.authority, Some(&first_commit), None).unwrap();
    drop(coordinator);

    let acquisitions = Arc::new(AtomicUsize::new(0));
    let counting_provider = Arc::new(CountingProvider {
        inner: primary_oracle.provider(&fixture),
        acquisitions: Arc::clone(&acquisitions),
    });
    let endpoint = FileFormalEndpoint::open_protected(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        counting_provider,
        None,
    )
    .unwrap();
    // Immutable path reads do not confer qualification on this descriptor.
    use crate::controlled_authority::formal::business_index::{
        BusinessIndexDeltaV1, BusinessIndexLeafKindV1, BusinessIndexLeafV1, BusinessIndexRootV1,
        metadata_key_digest,
    };
    let business_key = h(7);
    let mut updates = vec![
        BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: vec![1],
        },
        BusinessIndexLeafV1 {
            key_digest: business_key.clone(),
            kind: BusinessIndexLeafKindV1::Outcome,
            canonical_value: vec![2],
        },
    ];
    updates.sort_by(|a, b| a.key_digest.cmp(&b.key_digest));
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        updates,
        &endpoint.store,
    )
    .unwrap();
    endpoint
        .store
        .persist_business_nodes(&delta.node_writes)
        .unwrap();
    let business_root = delta.new;
    acquisitions.store(0, Ordering::SeqCst);
    let head_node_id = endpoint
        .store
        .body
        .head_node
        .as_deref()
        .expect("qualified head has a history node");
    let head_node: super::super::super::store::HistoryNode =
        endpoint.store.get(head_node_id).unwrap();
    let object_id = head_node.entry;
    let object_bytes = endpoint.store.raw_object(&object_id).unwrap().unwrap();
    assert_eq!(blake3::hash(&object_bytes).to_hex().as_str(), object_id);

    let mut server = OwnedRpcServer::start(
        endpoint,
        (
            primary_oracle.clone(),
            replica_oracle.clone(),
            fixture.clone(),
        ),
    )
    .unwrap();
    let address = server.address;

    let mut client = super::super::super::super::transport::RemoteFormalEndpoint::new(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Primary,
        key(4),
        first_commit.head(),
    )
    .unwrap();
    assert_eq!(
        server.call("head", || client.head()).unwrap(),
        second_commit.head()
    );
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);

    let chunk_len = object_bytes.len().min(32);
    let chunk = server
        .call("archive_chunk", || {
            client.archive_chunk(&object_id, 0, chunk_len as u32)
        })
        .unwrap()
        .unwrap();
    assert_eq!(chunk.bytes, object_bytes[..chunk_len]);
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);

    let response = server
        .call("signed_archive_object", || {
            signed_rpc(
                address,
                &fixture.authority,
                first_commit.head(),
                super::super::super::super::transport::Command::ArchiveObject(object_id.clone()),
            )
        })
        .unwrap();
    assert_eq!(response.body.minimum, first_commit.head());
    assert_eq!(response.body.head, second_commit.head());
    assert_eq!(response.body.ancestry.len(), 1);
    assert!(matches!(
        &response.body.result,
        Ok(super::super::super::super::transport::Value::ArchiveObject(Some(bytes)))
            if bytes == &object_bytes
    ));
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);

    let path = server
        .call("business_index_path", || {
            client.business_index_path(&business_root, &business_key)
        })
        .unwrap();
    assert_eq!(path.nodes.len(), 257);
    assert_eq!(
        path.verify(&business_root, &business_key)
            .unwrap()
            .unwrap()
            .key_digest,
        business_key
    );
    assert_eq!(
        acquisitions.swap(0, Ordering::SeqCst),
        1,
        "one guarded signed RPC for the whole cold path"
    );
    let absent = h(8);
    let absence = server
        .call("business_index_absent", || {
            client.business_index_path(&business_root, &absent)
        })
        .unwrap();
    assert!(absence.verify(&business_root, &absent).unwrap().is_none());
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);

    let wrong_minimum = FormalHeadRefV1 {
        position: second_commit.head().position,
        record_id: h(244),
    };
    assert_ne!(wrong_minimum, second_commit.head());
    let mut forked_client = super::super::super::super::transport::RemoteFormalEndpoint::new(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Primary,
        key(4),
        wrong_minimum,
    )
    .unwrap();
    assert!(
        server
            .call("wrong_minimum_head", || forked_client.head())
            .is_err()
    );
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);
    assert_eq!(
        server.call("head", || client.head()).unwrap(),
        second_commit.head()
    );
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);
    assert!(
        server
            .diagnostics
            .errors
            .lock()
            .unwrap()
            .iter()
            .any(|error| matches!(error, FormalError::Rollback(_)))
    );

    let checkpoint = server.call("checkpoint", || client.checkpoint()).unwrap();
    server
        .call("seal_epoch", || client.seal_epoch(h(245), 2, checkpoint))
        .unwrap();
    acquisitions.store(0, Ordering::SeqCst);
    assert_eq!(
        server.call("head", || client.head()).unwrap(),
        second_commit.head()
    );
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);
    let sealed_path = server
        .call("sealed_business_index_path", || {
            client.business_index_path(&business_root, &business_key)
        })
        .unwrap();
    assert_eq!(sealed_path, path);
    assert_eq!(acquisitions.swap(0, Ordering::SeqCst), 1);

    server.finish().unwrap();
    drop(primary_oracle);
    drop(replica_oracle);
}

#[test]
fn signed_read_rpc_fixture_joins_server_when_client_panics() {
    if crate::test_process::run_isolated(
        concat!(
            module_path!(),
            "::signed_read_rpc_fixture_joins_server_when_client_panics"
        ),
        Duration::from_secs(180),
    ) {
        return;
    }
    let fixture = Arc::new(Fixture::new());
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let diagnostics = Arc::new(Mutex::new(None));
    let observed = diagnostics.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let owned = OwnedRpcServer::start(endpoint, fixture.clone()).unwrap();
        *observed.lock().unwrap() = Some(owned.diagnostics.clone());
        // Exercise a real connection with an incomplete frame, then unwind at
        // the client. Its socket closes before the owned server guard drops.
        let mut partial_client = TcpStream::connect_timeout(&owned.address, RPC_TIMEOUT).unwrap();
        partial_client.set_write_timeout(Some(RPC_TIMEOUT)).unwrap();
        partial_client.write_all(&[0]).unwrap();
        panic!("injected client assertion failure");
    }));
    assert!(outcome.is_err());
    let diagnostics = diagnostics.lock().unwrap().take().unwrap();
    assert!(
        diagnostics.joined.load(Ordering::Acquire),
        "{}",
        diagnostics.report()
    );
    // The server's actual endpoint lock is released, not just a stop flag. The
    // same owned storage can reopen immediately while the fixture still lives.
    drop(fixture.endpoint(EndpointRole::Primary));
    assert!(fixture.pdir.exists());
}
