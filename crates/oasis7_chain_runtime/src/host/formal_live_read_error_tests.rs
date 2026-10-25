//! Errors are exercised through signed requests and actual source failures.
use super::*;

struct FileHistoryResolver {
    base: CurrentOnlyResolver,
    path: std::path::PathBuf,
}
impl FormalLiveHistoryResolver for FileHistoryResolver {
    fn origin_publication(&self, _: &FormalReadAnchorRefV1) -> FormalReadResult<FormalReadPublication> {
        // Retained material I/O carries its provenance even when its path
        // happens to contain words previously treated as HTTP categories.
        let _bytes = std::fs::read(&self.path).map_err(|error| {
            FormalReadError::Internal(format!("read {}: {error}", self.path.display()))
        })?;
        FormalReadPublication::new(self.base.anchor.clone(), self.base.world.lock().unwrap().clone())
    }
    fn fixed_publication(&self, commit: &CommitRef) -> FormalReadResult<FormalReadPublication> {
        self.base.fixed_publication(commit)
    }
    fn forward_publication(&self, origin: &VerifiedFormalReadAnchorV1, live: &VerifiedFormalReadAnchorV1, max: usize) -> FormalReadResult<FormalReadPublication> {
        self.base.forward_publication(origin, live, max)
    }
    fn history_page(&self, origin: &VerifiedFormalReadAnchorV1, target: &VerifiedFormalReadAnchorV1, max: usize) -> FormalReadResult<FormalReadHistoryV1> {
        self.base.history_page(origin, target, max)
    }
}

fn assert_signed_error(service: &FormalLiveService, signer: &SigningKey, request: &SignedReadRequest<ReadWorldViewRequest>, status: u16) {
    let reply = service.handle("POST", VIEW_PATH, &serde_json::to_vec(request).unwrap());
    assert_eq!(reply.status, status, "{}", String::from_utf8_lossy(&reply.body));
    let signed: SignedServiceResponse<FormalServiceErrorV1> = serde_json::from_slice(&reply.body).unwrap();
    verify_service_response(VIEW_PATH, &authority::request_digest(VIEW_PATH, request).unwrap(), &signed, &hex::encode(signer.verifying_key().to_bytes())).unwrap();
    assert_eq!(signed.payload.code, "formal_read_rejected");
}

#[test]
fn view_io_source_diagnostics_do_not_control_response_category() {
    let (world, trust, anchor) = fixture_trust();
    let node = live_node(&anchor.head().record().world.world_id);
    for diagnostic in ["missing", "node grant authorized not ready HistoryUnavailable"] {
        let path = std::env::temp_dir().join(format!("oasis7-5071-{}-{diagnostic}", std::process::id()));
        assert!(!path.exists());
        let history = Arc::new(FileHistoryResolver {
            base: CurrentOnlyResolver::new(world.clone(), anchor.clone()), path,
        });
        let (service, _, signer) = live_service(world.clone(), anchor.clone(), trust.clone(), history, Arc::clone(&node), published(world.clone(), anchor.clone()));
        let mut request = read_request(&anchor, &hex::encode([92; 32]), None).request;
        request.formal_anchor.as_mut().unwrap().record_id = "ab".repeat(32);
        let signed = sign_read_request(VIEW_PATH, request, &hex::encode([92; 32])).unwrap();
        assert!(matches!(service.read_view(&signed), Err(FormalReadError::Internal(_))));
        assert_signed_error(&service, &signer, &signed, 400);
    }
    stop_test_node(&node);
}

#[test]
fn view_keeps_unavailable_not_ready_binding_and_integrity_distinct() {
    let (world, trust, anchor) = fixture_trust();
    let node = live_node(&anchor.head().record().world.world_id);
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let publication = published(world.clone(), anchor.clone());
    let (service, state, signer) = live_service(world, anchor.clone(), trust, resolver, Arc::clone(&node), publication.clone());
    let reader = hex::encode([92; 32]);
    let request = read_request(&anchor, &reader, None);
    state.ready.store(false, Ordering::Release);
    assert!(matches!(service.read_view(&request), Err(FormalReadError::NotReady(_))));
    assert_signed_error(&service, &signer, &request, 503);
    state.ready.store(true, Ordering::Release);
    let mut wrong_world = request.request.clone();
    wrong_world.world.world_id = "other-world".into();
    let wrong_world = sign_read_request(VIEW_PATH, wrong_world, &reader).unwrap();
    assert!(matches!(service.read_view(&wrong_world), Err(FormalReadError::RequestRejected(_))));
    assert_signed_error(&service, &signer, &wrong_world, 400);
    let mut missing = request.request.clone();
    let mut commit = service.read_view(&request).unwrap().version.commit;
    commit.execution_block_hash = "cd".repeat(32);
    missing.fixed_commit = Some(commit);
    let missing = sign_read_request(VIEW_PATH, missing, &reader).unwrap();
    assert!(matches!(service.read_view(&missing), Err(FormalReadError::HistoryUnavailable(_))));
    assert_signed_error(&service, &signer, &missing, 410);
    // Corrupt an immutable server cut without changing its verified anchor.
    publication.current.lock().unwrap().as_mut().unwrap().world.step().unwrap();
    assert!(matches!(service.read_view(&request), Err(FormalReadError::Integrity(_))));
    assert_signed_error(&service, &signer, &request, 400);
    stop_test_node(&node);
}

struct InvalidPageResolver(CurrentOnlyResolver);
impl FormalLiveHistoryResolver for InvalidPageResolver {
    fn origin_publication(&self, request: &FormalReadAnchorRefV1) -> FormalReadResult<FormalReadPublication> {
        self.0.origin_publication(request)
    }
    fn fixed_publication(&self, commit: &CommitRef) -> FormalReadResult<FormalReadPublication> {
        self.0.fixed_publication(commit)
    }
    fn forward_publication(&self, origin: &VerifiedFormalReadAnchorV1, live: &VerifiedFormalReadAnchorV1, max: usize) -> FormalReadResult<FormalReadPublication> {
        self.0.forward_publication(origin, live, max)
    }
    fn history_page(&self, _: &VerifiedFormalReadAnchorV1, _: &VerifiedFormalReadAnchorV1, _: usize) -> FormalReadResult<FormalReadHistoryV1> {
        Ok(FormalReadHistoryV1 { schema_version: 99, steps: vec![] })
    }
}

#[test]
fn view_invalid_resolver_history_is_integrity_failure() {
    let (origin_world, trust, origin) = fixture_trust();
    let (world, anchor, _) = fixtures::actual_world_successor_fixture(&origin, origin_world.clone());
    let resolver = Arc::new(InvalidPageResolver(CurrentOnlyResolver::new(origin_world, origin.clone())));
    let node = live_node(&anchor.head().record().world.world_id);
    let (service, _, signer) = live_service(world.clone(), anchor.clone(), trust, resolver, Arc::clone(&node), published(world, anchor));
    let request = read_request(&origin, &hex::encode([92; 32]), None);
    assert!(matches!(service.read_view(&request), Err(FormalReadError::Integrity(_))));
    assert_signed_error(&service, &signer, &request, 400);
    stop_test_node(&node);
}

#[test]
fn signature_failure_fallback_stays_plain_and_error_messages_stay_bounded() {
    struct Unencodable;
    impl Serialize for Unencodable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("injected response encoding failure"))
        }
    }
    let signer = fixtures::key(91);
    for reply in [
        signed_reply_with_digest(400, VIEW_PATH, Err("digest failure".into()), &(), &signer),
        signed_reply_with_digest(200, VIEW_PATH, Ok("ab".repeat(32)), &Unencodable, &signer),
    ] {
        assert_eq!(reply.status, 500);
        let plain: FormalServiceErrorV1 = serde_json::from_slice(&reply.body).unwrap();
        assert_eq!(plain.api_version, "oasis7.formal-live-error/v1");
        assert_eq!(plain.code, "signature_failure");
        assert!(serde_json::from_slice::<SignedServiceResponse<FormalServiceErrorV1>>(&reply.body).is_err());
    }
    let digest = "ab".repeat(32);
    let reply = signed_error_with_digest(400, VIEW_PATH, Ok(digest.clone()), "formal_read_rejected", &"界".repeat(MAX_NODE_ERROR_CHARS + 7), &signer);
    let envelope: SignedServiceResponse<FormalServiceErrorV1> = serde_json::from_slice(&reply.body).unwrap();
    assert_eq!(envelope.payload.message.chars().count(), MAX_NODE_ERROR_CHARS);
    verify_service_response(VIEW_PATH, &digest, &envelope, &hex::encode(signer.verifying_key().to_bytes())).unwrap();
    assert!(verify_service_response(LOOKUP_PATH, &digest, &envelope, &hex::encode(signer.verifying_key().to_bytes())).is_err());
}
