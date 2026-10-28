use super::wire::*;
use super::*;
use std::collections::BTreeMap;
use std::time::Duration;
const CACHE_BYTES: usize = 2 * MAX_PACKAGE;
const NONCES_PER_PEER: usize = 1024;
const NONCE_METADATA_BYTES: usize = 1024;
const MAX_NONCE_METADATA_BYTES: usize = 256 * NONCES_PER_PEER * NONCE_METADATA_BYTES;
struct Seen {
    base: String,
    identity: String,
    deadline: Instant,
    manifest: Option<Manifest>,
}
struct Cached {
    bytes: Option<Arc<Vec<u8>>>,
    manifest: Option<Manifest>,
    reserved: usize,
}
#[derive(Default)]
struct Cache {
    seen: BTreeMap<(String, String), Seen>,
    entries: BTreeMap<(String, String), Cached>,
    bytes: usize,
    peer_nonces: BTreeMap<String, usize>,
    nonce_limit: usize,
    metadata_bytes: usize,
    metadata_cap: usize,
}
pub(super) struct Server {
    pub binding: BftSignerBindingV3,
    pub local_peer: String,
    pub state: Arc<NodeBftPeerExecutionStateV3>,
    pub hook: NodeBftSharedExecutionHookV3,
    pub gate: Arc<dyn NodeBftPrivateReadGateV3>,
    pub alive: Arc<AtomicBool>,
    pub timeout: Duration,
    cache: Mutex<Cache>,
    verified_history: Mutex<Option<Arc<NodeBftPeerExecutionViewV3>>>,
}
impl Server {
    pub fn new(
        config: &NodeBftPrivateExchangeConfigV3,
        local_peer: String,
        state: Arc<NodeBftPeerExecutionStateV3>,
        hook: NodeBftSharedExecutionHookV3,
        gate: Arc<dyn NodeBftPrivateReadGateV3>,
        alive: Arc<AtomicBool>,
    ) -> Self {
        Self {
            binding: config.binding.clone(),
            local_peer,
            state,
            hook,
            gate,
            alive,
            timeout: config.operation_timeout,
            cache: Mutex::new(Cache::for_peers(config.peers.iter().cloned())),
            verified_history: Mutex::new(None),
        }
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn restrict_nonce_budget_for_testing(&self, limit: usize) -> Result<(), String> {
        if !(1..=NONCES_PER_PEER).contains(&limit) {
            return Err("invalid narrowed nonce limit".into());
        }
        let mut cache = self.cache.lock().map_err(|_| "private cache poisoned")?;
        if !cache.seen.is_empty() || limit > cache.nonce_limit {
            return Err("test nonce quota can only narrow an unused exchange".into());
        }
        cache.nonce_limit = limit;
        cache.metadata_cap = cache.peer_nonces.len() * limit * NONCE_METADATA_BYTES;
        Ok(())
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn nonce_count_for_testing(&self, peer: &str) -> Result<usize, String> {
        self.cache
            .lock()
            .map_err(|_| "private cache poisoned".to_string())?
            .peer_nonces
            .get(peer)
            .copied()
            .ok_or_else(|| "private nonce peer is not configured".to_string())
    }
    pub fn clear(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.entries.clear();
            c.bytes = 0;
        }
    }
    pub fn handle(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        raw: &[u8],
    ) -> Result<Vec<u8>, String> {
        transport
            .request_context()
            .check_cancelled(|| "private request cancelled".to_string())?;
        let request: Request = decode_frame(raw)?;
        request.validate()?;
        if !self.alive.load(Ordering::Acquire) {
            return Err("private exchange stopped".into());
        }
        let target = &request.target;
        let parent_changed = if target.kind == NodeBftPrivateFetchKindV3::CommittedState {
            "private committed parent advancement"
        } else {
            "private pending parent changed"
        };
        if !self.state.is_serving() {
            return Err(parent_changed.into());
        }
        if target.context
            != oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(
                self.binding.handoff(),
            )
            || target.requester_peer_id != transport.peer_id().to_string()
            || target.serving_peer_id != self.local_peer
        {
            return Err("private request H/Noise endpoints differ".into());
        }
        let view = self
            .state
            .view()?
            .ok_or("private verified peer view unavailable")?;
        // The compact request never supplies membership or execution material.
        // Resolve the original public value from local retained metadata only.
        let value = match target.kind {
            NodeBftPrivateFetchKindV3::CommittedState => view
                .committed_history
                .iter()
                .find(|c| {
                    c.execution_commitment.height == target.height
                        && c.header.block_id().is_ok_and(|id| id == target.header_id)
                })
                .map(|c| BftHeaderValueV3 {
                    header: c.header.clone(),
                    execution_commitment: c.execution_commitment.clone(),
                }),
            NodeBftPrivateFetchKindV3::PendingCandidate => self
                .hook
                .lock()
                .map_err(|_| "private original public lookup hook poisoned")?
                .lookup_bft_v3_public_value(target.height, &target.header_id)?,
        }
        .ok_or("private original public metadata missing")?;
        target.validate_value(&value)?;
        let subject = self.validate_subject(&request, &view, &value)?;
        let scope = NodeBftPrivateReadScopeV3 {
            view: view.clone(),
            value: value.clone(),
            kind: target.kind,
            requester_peer_id: target.requester_peer_id.clone(),
            serving_peer_id: target.serving_peer_id.clone(),
            subject_public_key: subject,
        };
        transport
            .restrict_reply_to_generation(self.state.generation_guard(), view.generation)
            .map_err(|_| parent_changed)?;
        transport
            .restrict_reply_while(self.state.serving_guard())
            .map_err(|_| parent_changed)?;
        transport
            .restrict_reply_while(self.alive.clone())
            .map_err(|_| "private exchange stopped")?;
        let mut response = |authorized_until: Instant| {
            transport
                .request_context()
                .check_cancelled(|| "private request cancelled".to_string())?;
            if !self.state.is_serving()
                || self.state.generation_guard().load(Ordering::Acquire) != view.generation
            {
                return Err(parent_changed.into());
            }
            let (bytes, manifest, deadline) = self.material(&request, &value)?;
            transport
                .restrict_reply_until(authorized_until.min(deadline))
                .map_err(|_| "private permission/original deadline expired")?;
            let chunk_hex = match request.chunk_index {
                Some(index) => {
                    let (offset, len) = manifest.operation().chunk_range(index)?;
                    Some(hex::encode(
                        &bytes[usize::try_from(offset).map_err(|_| "chunk offset")?
                            ..usize::try_from(offset + u64::from(len)).map_err(|_| "chunk end")?],
                    ))
                }
                None => None,
            };
            if !self.state.is_serving()
                || self.state.generation_guard().load(Ordering::Acquire) != view.generation
            {
                return Err(parent_changed.into());
            }
            encode_frame(&Response {
                domain: DOMAIN.into(),
                request_digest: request.request_digest()?,
                manifest,
                chunk_index: request.chunk_index,
                chunk_hex,
            })
        };
        self.gate
            .authorize(transport, &request.authorization, &scope, &mut response)
    }
    fn validate_subject(
        &self,
        request: &Request,
        view: &Arc<NodeBftPeerExecutionViewV3>,
        value: &BftHeaderValueV3,
    ) -> Result<String, String> {
        let t = &request.target;
        match (&request.authorization, t.kind) {
            (
                NodeBftPrivateReadAuthorizationV3::Pending(proof),
                NodeBftPrivateFetchKindV3::PendingCandidate,
            ) => {
                value.validate_for_schedule(&view.schedule)?;
                let v = view
                    .schedule
                    .active_set()
                    .validator(&proof.body.validator_id)
                    .ok_or("pending read requester is not current active validator")?;
                let expected = t.pending_body(
                    value,
                    v.validator_id.clone(),
                    v.public_key_hex.clone(),
                    proof.body.candidate_policy_digest.clone(),
                    proof.body.candidate_grant_generation,
                )?;
                proof.verify_for(&expected, &v.public_key_hex)?;
                Ok(v.public_key_hex.clone())
            }
            (
                NodeBftPrivateReadAuthorizationV3::Committed(auth),
                NodeBftPrivateFetchKindV3::CommittedState,
            ) => {
                self.verify_history(view)?;
                if !view.committed_history.iter().any(|c| {
                    c.header == value.header && c.execution_commitment == value.execution_commitment
                }) {
                    return Err(
                        "committed read is not in the independently verified original prefix"
                            .into(),
                    );
                }
                if auth.subject_public_key != auth.grant.body.node_public_key {
                    return Err("committed read subject differs from V1 grant".into());
                }
                verify_signature(
                    &auth.subject_public_key,
                    &committed_payload(t, &auth.grant, &auth.subject_public_key)?,
                    &auth.signature_hex,
                )?;
                Ok(auth.subject_public_key.clone())
            }
            _ => Err("pending read purpose cannot authorize committed private history".into()),
        }
    }
    // Verify each immutable runtime-published view once, rather than repeat
    // every QC signature for all possible package chunks. Membership and fresh
    // permissions are still checked per request; no peer-supplied view is used.
    fn verify_history(&self, view: &Arc<NodeBftPeerExecutionViewV3>) -> Result<(), String> {
        let mut cached = self
            .verified_history
            .lock()
            .map_err(|_| "private verified history cache poisoned")?;
        if cached.as_ref().is_some_and(|old| Arc::ptr_eq(old, view)) {
            return Ok(());
        }
        let mut schedule = self.binding.initial_schedule().clone();
        if view.committed_history.len() > 64 {
            return Err("private committed history capacity".into());
        }
        for certificate in &view.committed_history {
            schedule = oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(
                &schedule,
                certificate,
            )?
            .next_height_schedule()
            .clone();
        }
        if schedule != view.schedule {
            return Err("private committed prefix differs from verified current schedule".into());
        }
        *cached = Some(view.clone());
        Ok(())
    }
    fn material(
        &self,
        request: &Request,
        value: &BftHeaderValueV3,
    ) -> Result<(Arc<Vec<u8>>, Manifest, Instant), String> {
        let t = &request.target;
        let key = (t.requester_peer_id.clone(), t.nonce.clone());
        let identity = match &request.authorization {
            NodeBftPrivateReadAuthorizationV3::Pending(p) => digest(
                b"pending-read-subject",
                &encode(&(
                    &p.body.validator_public_key_hex,
                    &p.body.candidate_policy_digest,
                    p.body.candidate_grant_generation,
                ))?,
            ),
            NodeBftPrivateReadAuthorizationV3::Committed(p) => digest(
                b"committed-read-subject",
                &encode(&(&p.grant, &p.subject_public_key))?,
            ),
        };
        let base = t.base_digest()?;
        let now = Instant::now();
        let deadline = {
            let mut c = self.cache.lock().map_err(|_| "private cache poisoned")?;
            if let Some(s) = c.seen.get(&key) {
                if s.base != base || s.identity != identity || s.deadline <= now {
                    return Err("private original nonce identity/deadline differs".into());
                }
            } else {
                if !matches!(t.operation, BftCandidateReadOperationV3::Manifest) {
                    return Err("private original manifest is required".into());
                }
                c.admit_nonce(key.clone(), base, identity, now + self.timeout)?;
            }
            let deadline = c
                .seen
                .get(&key)
                .ok_or("private original nonce missing")?
                .deadline;
            if let Some(cached) = c.entries.get(&key) {
                if let (Some(bytes), Some(manifest)) = (&cached.bytes, &cached.manifest) {
                    if !matches!(t.operation, BftCandidateReadOperationV3::Manifest)
                        && t.operation != manifest.operation()
                    {
                        return Err("private package differs from original layout".into());
                    }
                    return Ok((bytes.clone(), manifest.clone(), deadline));
                }
                return Err("private cache build in progress".into());
            }
            if !matches!(t.operation, BftCandidateReadOperationV3::Manifest)
                && c.seen
                    .get(&key)
                    .and_then(|s| s.manifest.as_ref())
                    .is_none_or(|m| m.operation() != t.operation)
            {
                return Err("private package retry differs from original manifest".into());
            }
            c.make_room()?;
            c.bytes += MAX_PACKAGE;
            c.entries.insert(
                key.clone(),
                Cached {
                    bytes: None,
                    manifest: None,
                    reserved: MAX_PACKAGE,
                },
            );
            deadline
        };
        let built: Result<(Arc<Vec<u8>>, Manifest), String> = (|| {
            let package = self
                .hook
                .lock()
                .map_err(|_| "private export hook poisoned")?
                .export_bft_v3_private_candidate(value)?;
            if package.candidate.public_value != *value {
                return Err("private original export value changed".into());
            }
            let bytes = super::package::encode_package(&package)?;
            if Instant::now() >= deadline {
                return Err("private original package export exceeded deadline".into());
            }
            let total_bytes = u64::try_from(bytes.len()).map_err(|_| "private package size")?;
            let total_chunks = u32::try_from(total_bytes.div_ceil(u64::from(CHUNK)))
                .map_err(|_| "private package chunk count")?;
            let manifest = Manifest {
                package_digest: super::package::package_digest(&bytes),
                total_bytes,
                chunk_size: CHUNK,
                total_chunks,
            };
            manifest.operation().validate()?;
            Ok((Arc::new(bytes), manifest))
        })();
        let mut c = self.cache.lock().map_err(|_| "private cache poisoned")?;
        let old = c
            .entries
            .remove(&key)
            .ok_or("private original cache reservation disappeared")?;
        c.bytes -= old.reserved;
        let (bytes, manifest) = built?;
        c.bind_original_manifest(&key, &manifest)?;
        c.bytes += bytes.len();
        c.entries.insert(
            key,
            Cached {
                reserved: bytes.len(),
                bytes: Some(bytes.clone()),
                manifest: Some(manifest.clone()),
            },
        );
        Ok((bytes, manifest, deadline))
    }
}

impl Cache {
    fn for_peers(peers: impl Iterator<Item = String>) -> Self {
        let peer_nonces: BTreeMap<_, _> = peers.map(|p| (p, 0usize)).collect();
        let metadata_cap = peer_nonces
            .len()
            .saturating_mul(NONCES_PER_PEER)
            .saturating_mul(NONCE_METADATA_BYTES)
            .min(MAX_NONCE_METADATA_BYTES);
        Self {
            peer_nonces,
            nonce_limit: NONCES_PER_PEER,
            metadata_cap,
            ..Self::default()
        }
    }
    fn admit_nonce(
        &mut self,
        key: (String, String),
        base: String,
        identity: String,
        deadline: Instant,
    ) -> Result<(), String> {
        // The caller has matched key.0 to the actual Noise identity. One peer
        // may consume only its own reserved share, never another peer's slot.
        if self.seen.contains_key(&key) {
            return Err("private original nonce already admitted".into());
        }
        if key.0.len() > 128 {
            return Err("private peer metadata bound".into());
        }
        hash(&key.1)?;
        hash(&base)?;
        hash(&identity)?;
        let count = self
            .peer_nonces
            .get_mut(&key.0)
            .ok_or("private nonce peer is not configured")?;
        let global_count_cap = self.metadata_cap / NONCE_METADATA_BYTES;
        if *count >= self.nonce_limit
            || self.seen.len() >= global_count_cap
            || NONCE_METADATA_BYTES > self.metadata_cap.saturating_sub(self.metadata_bytes)
        {
            return Err("private Noise peer original-nonce quota exhausted".into());
        }
        *count += 1;
        self.metadata_bytes += NONCE_METADATA_BYTES;
        self.seen.insert(
            key,
            Seen {
                base,
                identity,
                deadline,
                manifest: None,
            },
        );
        Ok(())
    }
    /// Only idle encoded buffers are evicted. Immutable operation metadata and
    /// original deadlines remain, so cache misses can only replay strict
    /// original export and reproduce the first digest/layout exactly.
    fn make_room(&mut self) -> Result<(), String> {
        while self.entries.len() >= 8 || MAX_PACKAGE > CACHE_BYTES.saturating_sub(self.bytes) {
            let idle = self
                .entries
                .iter()
                .filter(|(_, e)| e.bytes.as_ref().is_some_and(|b| Arc::strong_count(b) == 1))
                .min_by_key(|(key, _)| self.seen.get(*key).map(|s| s.deadline))
                .map(|(key, _)| key.clone())
                .ok_or("private global active package/cache capacity")?;
            let old = self
                .entries
                .remove(&idle)
                .ok_or("private idle cache disappeared")?;
            self.bytes -= old.reserved;
        }
        Ok(())
    }
    fn bind_original_manifest(
        &mut self,
        key: &(String, String),
        manifest: &Manifest,
    ) -> Result<(), String> {
        let seen = self
            .seen
            .get_mut(key)
            .ok_or("private original manifest identity missing")?;
        if seen.deadline <= Instant::now() || seen.manifest.as_ref().is_some_and(|m| m != manifest)
        {
            return Err("private strict re-export differs from first manifest/deadline".into());
        }
        if seen.manifest.is_none() {
            seen.manifest = Some(manifest.clone());
        }
        Ok(())
    }
}
#[cfg(test)]
mod cache_tests {
    use super::*;
    fn manifest(n: u8) -> Manifest {
        Manifest {
            package_digest: format!("{n:02x}").repeat(32),
            total_bytes: 1,
            chunk_size: CHUNK,
            total_chunks: 1,
        }
    }
    #[test]
    fn idle_package_eviction_preserves_original_nonce_manifest_and_deadline() {
        let mut c = Cache::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        for n in 0..8u8 {
            let key = ("peer".into(), n.to_string());
            let m = manifest(n);
            c.seen.insert(
                key.clone(),
                Seen {
                    base: "base".into(),
                    identity: "subject".into(),
                    deadline,
                    manifest: Some(m.clone()),
                },
            );
            c.entries.insert(
                key,
                Cached {
                    bytes: Some(Arc::new(vec![n])),
                    manifest: Some(m),
                    reserved: 1,
                },
            );
            c.bytes += 1;
        }
        c.make_room().unwrap();
        assert_eq!(c.entries.len(), 7);
        assert_eq!(c.seen.len(), 8);
        let key = c
            .seen
            .keys()
            .find(|k| !c.entries.contains_key(*k))
            .unwrap()
            .clone();
        let original = c.seen.get(&key).unwrap().manifest.clone().unwrap();
        c.bind_original_manifest(&key, &original).unwrap();
        assert_eq!(c.seen.get(&key).unwrap().deadline, deadline);
        assert!(c.bind_original_manifest(&key, &manifest(99)).is_err());
        assert_eq!(c.seen.get(&key).unwrap().manifest.as_ref(), Some(&original));
        c.seen.get_mut(&key).unwrap().deadline = Instant::now() - Duration::from_secs(1);
        assert!(c.bind_original_manifest(&key, &original).is_err());
    }
    #[test]
    fn noise_peer_nonce_exhaustion_cannot_consume_another_reserved_share() {
        let mut c = Cache::for_peers(["noise-a".to_string(), "noise-b".to_string()].into_iter());
        let deadline = Instant::now() + Duration::from_secs(30);
        for n in 0..NONCES_PER_PEER {
            c.admit_nonce(
                ("noise-a".into(), format!("{n:064x}")),
                "a".repeat(64),
                "b".repeat(64),
                deadline,
            )
            .unwrap();
        }
        assert!(
            c.admit_nonce(
                ("noise-a".into(), format!("{:064x}", NONCES_PER_PEER)),
                "a".repeat(64),
                "b".repeat(64),
                deadline
            )
            .is_err()
        );
        assert!(
            c.admit_nonce(
                ("noise-b".into(), "c".repeat(64)),
                "a".repeat(64),
                "b".repeat(64),
                deadline
            )
            .is_ok()
        );
        assert_eq!(c.peer_nonces["noise-a"], NONCES_PER_PEER);
        assert_eq!(c.peer_nonces["noise-b"], 1);
        assert_eq!(
            c.metadata_bytes,
            (NONCES_PER_PEER + 1) * NONCE_METADATA_BYTES
        );
        assert_eq!(c.metadata_cap, 2 * NONCES_PER_PEER * NONCE_METADATA_BYTES);
        let key = ("noise-a".into(), format!("{:064x}", 0));
        c.seen.get_mut(&key).unwrap().deadline = Instant::now() - Duration::from_secs(1);
        assert!(
            c.admit_nonce(key, "a".repeat(64), "b".repeat(64), deadline)
                .is_err()
        );
        assert_eq!(c.peer_nonces["noise-a"], NONCES_PER_PEER);
    }
    #[test]
    fn expired_active_and_building_reservations_still_consume_capacity() {
        let mut c = Cache::default();
        let held = Arc::new(vec![0]);
        let expired = Instant::now() - Duration::from_secs(1);
        for (key, bytes) in [("active", Some(held.clone())), ("building", None)] {
            let key = ("peer".into(), key.into());
            c.seen.insert(
                key.clone(),
                Seen {
                    base: "b".into(),
                    identity: "i".into(),
                    deadline: expired,
                    manifest: None,
                },
            );
            c.entries.insert(
                key,
                Cached {
                    bytes,
                    manifest: None,
                    reserved: MAX_PACKAGE,
                },
            );
            c.bytes += MAX_PACKAGE;
        }
        assert!(c.make_room().is_err());
        assert_eq!(c.bytes, CACHE_BYTES);
        assert_eq!(c.entries.len(), 2);
        drop(held);
        c.make_room().unwrap();
        assert_eq!(c.bytes, MAX_PACKAGE);
        assert!(c.entries.values().all(|entry| entry.bytes.is_none()));
    }
    #[test]
    fn active_package_buffers_cannot_be_evicted_to_hide_memory() {
        let mut c = Cache::default();
        let held = Arc::new(vec![0]);
        c.bytes = CACHE_BYTES;
        c.entries.insert(
            ("p".into(), "n".into()),
            Cached {
                bytes: Some(held.clone()),
                manifest: Some(manifest(1)),
                reserved: CACHE_BYTES,
            },
        );
        assert!(c.make_room().is_err());
        drop(held);
        c.make_room().unwrap();
        assert!(c.entries.is_empty());
        assert_eq!(c.bytes, 0);
    }
}
