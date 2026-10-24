use super::*;
use crate::controlled_authority::formal::*;
use ed25519_dalek::SigningKey;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().as_bytes())
}

fn hash(n: u8) -> String {
    hex::encode([n; 32])
}

fn trust() -> InitialActivationTrustV1 {
    InitialActivationTrustV1 {
        issuer_public_key: public(1),
        world: FormalWorldIdentityV1 {
            world_id: "durable-full-sync-issuer-test".into(),
            chain_id: "issuer-test-chain".into(),
            genesis_digest: hash(31),
        },
        initial_state_root: hash(32),
        governing_manifest_digest: hash(33),
        recovery_policy_digest: hash(34),
    }
}

fn signed_policy(
    generation: u64,
    issuer: u8,
    manager: u8,
    witness: u8,
) -> SignedFullSyncIssuerPolicyV2 {
    let trust = trust();
    SignedFullSyncIssuerPolicyV2::sign(
        FullSyncIssuerPolicyBodyV2 {
            schema_version: 2,
            root_public_key: public(1),
            activation_trust_digest: full_sync_activation_trust_digest_v2(&trust).unwrap(),
            world: trust.world.clone(),
            authority_epoch: 3,
            allowed_roles: vec![
                FullSyncReadRoleV2::StateSync,
                FullSyncReadRoleV2::FullReadOnly,
            ],
            management_public_key: public(manager),
            issuer_public_key: public(issuer),
            witness_public_key: public(witness),
            signer_generation: generation,
        },
        &key(1),
    )
    .unwrap()
}

fn policy(generation: u64, issuer: u8, manager: u8, witness: u8) -> VerifiedFullSyncIssuerPolicyV2 {
    VerifiedFullSyncIssuerPolicyV2::verify(
        &signed_policy(generation, issuer, manager, witness),
        &trust(),
    )
    .unwrap()
}

fn genesis(policy: &VerifiedFullSyncIssuerPolicyV2) -> FullSyncIssuerAnchorV2 {
    VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![])
        .unwrap()
        .anchor()
        .clone()
}

fn target(node: u8, role: FullSyncReadRoleV2) -> FullSyncIssuerTargetV2 {
    FullSyncIssuerTargetV2 {
        node_public_key: public(node),
        role,
    }
}

fn command(
    state: &VerifiedFullSyncIssuerStateV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    request: u8,
    nonce: u8,
    target: FullSyncIssuerTargetV2,
    operation: FullSyncIssuerOperationV2,
    target_generation: u64,
) -> SignedFullSyncIssuerCommandV2 {
    SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            transition: None,
            target,
            operation,
            expected_head: state.anchor().clone(),
            expected_target_generation: target_generation,
            request_id: hash(request),
            command_nonce: hash(nonce),
        },
        &key(if policy.body().signer_generation == 1 {
            2
        } else {
            9
        }),
    )
    .unwrap()
}

fn issue(
    state: &VerifiedFullSyncIssuerStateV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    request: u8,
    target: FullSyncIssuerTargetV2,
    generation: u64,
) -> SignedFullSyncIssuerCommandV2 {
    command(
        state,
        policy,
        request,
        request.wrapping_add(80),
        target,
        FullSyncIssuerOperationV2::Issue {
            not_before_unix_ms: 100,
            expires_unix_ms: 10_000,
        },
        generation,
    )
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = fs::canonicalize(std::env::temp_dir()).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!(
            "o7-full-sync-issuer-{label}-{}-{now}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy)]
enum CommitFault {
    BeforeCas,
    LoseReplyAfterCas,
    BlockLocalLedgerAfterCas,
}

/// Explicitly simulated witness behavior. This fake is not evidence of an
/// independent fault domain or production anti-rollback durability.
struct SimulatedWitness {
    policy: VerifiedFullSyncIssuerPolicyV2,
    key: SigningKey,
    entries: Vec<FullSyncIssuerCommittedEntryV2>,
    fault: Option<CommitFault>,
    /// Advances the independently simulated witness between a loser's fresh
    /// observation and CAS, forcing a genuine stale-head interleaving.
    advance_before_next_cas: Option<SignedFullSyncIssuerCommandV2>,
    local_directory: Option<PathBuf>,
    truncate_response: bool,
}

impl SimulatedWitness {
    fn new(policy: VerifiedFullSyncIssuerPolicyV2) -> Self {
        // Test keys use a repeated seed byte; identify the seed by its public key.
        let seed = (2u8..=20)
            .find(|candidate| public(*candidate) == policy.body().witness_public_key)
            .unwrap();
        Self {
            policy,
            key: key(seed),
            entries: Vec::new(),
            fault: None,
            advance_before_next_cas: None,
            local_directory: None,
            truncate_response: false,
        }
    }

    fn state(&self) -> Result<VerifiedFullSyncIssuerStateV2, FormalError> {
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            self.policy.clone(),
            self.entries.clone(),
        )
    }

    fn store(&mut self, dir: &Path) -> DurableFullSyncIssuerV2 {
        self.try_store(dir).unwrap()
    }

    fn try_store(&mut self, dir: &Path) -> Result<DurableFullSyncIssuerV2, FormalError> {
        DurableFullSyncIssuerV2::open(
            dir,
            self.policy.clone(),
            key((2u8..=20)
                .find(|n| public(*n) == self.policy.body().issuer_public_key)
                .unwrap()),
            genesis(&self.policy),
            self,
        )
    }

    fn issue_for(&self, request: u8, node: u8) -> SignedFullSyncIssuerCommandV2 {
        issue(
            &self.state().unwrap(),
            &self.policy,
            request,
            target(node, FullSyncReadRoleV2::StateSync),
            0,
        )
    }

    fn commit_as_winner(
        &mut self,
        command: SignedFullSyncIssuerCommandV2,
    ) -> Result<(), FormalError> {
        let state = self.state()?;
        let prepared = state.prepare(command)?;
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| FormalError::IssuerUnproven)?;
        let issuer_seed = (2u8..=20)
            .find(|n| public(*n) == self.policy.body().issuer_public_key)
            .ok_or(FormalError::IssuerUnproven)?;
        let query = SignedFullSyncIssuerQueryV2::sign(
            &self.policy,
            state.anchor().clone(),
            [211; 32],
            &key(issuer_seed),
        )?;
        let observation = SignedFullSyncIssuerObservationV2::sign(
            FullSyncIssuerObservationBodyV2 {
                schema_version: 2,
                query,
                current_anchor: state.anchor().clone(),
            },
            &self.key,
        )?;
        let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &self.key)?;
        self.entries
            .push(FullSyncIssuerCommittedEntryV2 { record, receipt });
        Ok(())
    }
}

fn raced_pending(
    label: &str,
    losing_request: u8,
    winning_request: u8,
) -> (TempDir, SimulatedWitness) {
    let pending_dir = TempDir::new(&format!("{label}-pending"));
    let winner_dir = TempDir::new(&format!("{label}-winner"));
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let losing_command = witness.issue_for(losing_request, 5);
    let winning_command = witness.issue_for(winning_request, 6);
    let mut loser = witness.store(pending_dir.path());
    let winner = witness.store(winner_dir.path());
    witness.advance_before_next_cas = Some(winning_command);
    assert!(loser.manage(losing_command, &mut witness).is_err());
    assert_eq!(witness.entries.len(), 1);
    assert!(pending_dir.path().join(PENDING_FILE).is_file());
    drop(loser);
    drop(winner);
    (pending_dir, witness)
}

impl FullSyncIssuerWitnessV2 for SimulatedWitness {
    fn observe(
        &mut self,
        query: &SignedFullSyncIssuerQueryV2,
    ) -> Result<SignedFullSyncIssuerObservationV2, FormalError> {
        query.verify(&self.policy)?;
        SignedFullSyncIssuerObservationV2::sign(
            FullSyncIssuerObservationBodyV2 {
                schema_version: 2,
                query: query.clone(),
                current_anchor: self.state()?.anchor().clone(),
            },
            &self.key,
        )
    }

    fn records_after(
        &mut self,
        after: &FullSyncIssuerAnchorV2,
        through: &FullSyncIssuerAnchorV2,
    ) -> Result<Vec<FullSyncIssuerCommittedEntryV2>, FormalError> {
        let state = self.state()?;
        if after != &genesis(&self.policy) || through != state.anchor() {
            return Err(FormalError::Rollback(
                "simulated witness cursor mismatch".into(),
            ));
        }
        let mut entries = self.entries.clone();
        if self.truncate_response && !entries.is_empty() {
            entries.pop();
        }
        Ok(entries)
    }

    fn commit(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV2,
    ) -> Result<SignedFullSyncIssuerReceiptV2, FormalError> {
        observation.verify(&self.policy)?;
        if matches!(self.fault, Some(CommitFault::BeforeCas)) {
            self.fault = None;
            return Err(FormalError::Io(
                "simulated witness failure before CAS".into(),
            ));
        }
        if let Some(winner) = self.advance_before_next_cas.take() {
            self.commit_as_winner(winner)?;
        }
        let state = self.state()?;
        if observation.body.current_anchor != *state.anchor() {
            return Err(FormalError::Conflict(
                "simulated stale CAS observation".into(),
            ));
        }
        let prepared = state.prepare(record.command.clone())?;
        if prepared.record() != Some(record) {
            return Err(FormalError::Conflict("simulated candidate mismatch".into()));
        }
        let receipt = SignedFullSyncIssuerReceiptV2::sign(observation.clone(), record, &self.key)?;
        self.entries.push(FullSyncIssuerCommittedEntryV2 {
            record: record.clone(),
            receipt: receipt.clone(),
        });
        match self.fault.take() {
            Some(CommitFault::LoseReplyAfterCas) => {
                Err(FormalError::Io("simulated committed CAS reply loss".into()))
            }
            Some(CommitFault::BlockLocalLedgerAfterCas) => {
                let directory = self.local_directory.as_ref().ok_or_else(|| {
                    FormalError::Io("simulated local persistence path missing".into())
                })?;
                fs::remove_file(directory.join(LEDGER_FILE)).map_err(|error| {
                    FormalError::Io(format!("simulated local persistence setup failed: {error}"))
                })?;
                fs::create_dir(directory.join(LEDGER_FILE)).map_err(|error| {
                    FormalError::Io(format!("simulated local persistence failure: {error}"))
                })?;
                Ok(receipt)
            }
            _ => Ok(receipt),
        }
    }
}

fn commit_direct(
    state: &VerifiedFullSyncIssuerStateV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    command: SignedFullSyncIssuerCommandV2,
    observation_nonce: u8,
) -> VerifiedFullSyncIssuerDecisionV2 {
    let prepared = state.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query = SignedFullSyncIssuerQueryV2::sign(
        policy,
        state.anchor().clone(),
        [observation_nonce; 32],
        &key((2u8..=20)
            .find(|n| public(*n) == policy.body().issuer_public_key)
            .unwrap()),
    )
    .unwrap();
    let observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: state.anchor().clone(),
        },
        &key((2u8..=20)
            .find(|n| public(*n) == policy.body().witness_public_key)
            .unwrap()),
    )
    .unwrap();
    let receipt = SignedFullSyncIssuerReceiptV2::sign(
        observation,
        &record,
        &key((2u8..=20)
            .find(|n| public(*n) == policy.body().witness_public_key)
            .unwrap()),
    )
    .unwrap();
    prepared.commit(receipt).unwrap()
}

#[test]
fn signed_command_is_durable_idempotent_and_conflicting_identity_is_rejected() {
    let temp = TempDir::new("idempotent");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let command = witness.issue_for(7, 5);
    let mut issuer = witness.store(temp.path());

    let original = issuer.manage(command.clone(), &mut witness).unwrap();
    let duplicate = issuer.manage(command.clone(), &mut witness).unwrap();
    assert_eq!(original, duplicate);
    assert_eq!(witness.entries.len(), 1);

    let mut body = command.body.clone();
    body.operation = FullSyncIssuerOperationV2::Revoke;
    let conflict = SignedFullSyncIssuerCommandV2::sign(body, &key(2)).unwrap();
    assert!(issuer.manage(conflict, &mut witness).is_err());
    assert_eq!(witness.entries.len(), 1);
}

#[test]
fn invalid_or_out_of_scope_command_is_rejected_before_pending_intent() {
    let temp = TempDir::new("invalid-command");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let initial = witness.state().unwrap();
    let mut stale_body = witness.issue_for(11, 5).body;
    stale_body.expected_target_generation = 1;
    let stale = SignedFullSyncIssuerCommandV2::sign(stale_body, &key(2)).unwrap();
    let mut issuer = witness.store(temp.path());
    assert!(issuer.manage(stale, &mut witness).is_err());
    assert!(!temp.path().join(PENDING_FILE).exists());

    let mut outside_body = witness.issue_for(12, 6).body;
    outside_body.world.world_id.push_str("-foreign");
    let outside = SignedFullSyncIssuerCommandV2::sign(outside_body, &key(2)).unwrap();
    assert!(issuer.manage(outside, &mut witness).is_err());
    assert!(!temp.path().join(PENDING_FILE).exists());
    assert_eq!(initial.anchor(), witness.state().unwrap().anchor());
}

#[test]
fn failed_cas_keeps_same_intent_and_reopen_allows_only_original_request() {
    let temp = TempDir::new("pending-retry");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let command = witness.issue_for(13, 5);
    let mut issuer = witness.store(temp.path());
    witness.fault = Some(CommitFault::BeforeCas);
    assert!(issuer.manage(command.clone(), &mut witness).is_err());
    assert!(temp.path().join(PENDING_FILE).is_file());
    assert_eq!(witness.entries.len(), 0);
    assert!(issuer.manage(command.clone(), &mut witness).is_err());
    drop(issuer);

    let other = witness.issue_for(14, 6);
    let mut reopened = witness.store(temp.path());
    assert!(reopened.manage(other, &mut witness).is_err());
    assert!(reopened.manage(command.clone(), &mut witness).is_ok());
    assert_eq!(witness.entries.len(), 1);
    assert!(!temp.path().join(PENDING_FILE).exists());
}

#[test]
fn stale_cas_loser_clears_intent_only_after_reopen_proves_noncommit() {
    let loser_dir = TempDir::new("cas-loser");
    let winner_dir = TempDir::new("cas-winner");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let losing_command = witness.issue_for(131, 5);
    let winning_command = witness.issue_for(132, 6);
    let mut loser = witness.store(loser_dir.path());
    let mut winner = witness.store(winner_dir.path());

    // The fake witness commits a distinct request after the loser's signed
    // observation and before its CAS check. This models two independently
    // locked local stores racing against one monotonic witness.
    witness.advance_before_next_cas = Some(winning_command.clone());
    assert!(loser.manage(losing_command.clone(), &mut witness).is_err());
    assert_eq!(witness.entries.len(), 1);
    assert_eq!(witness.entries[0].record.command, winning_command);
    assert!(loser_dir.path().join(PENDING_FILE).is_file());
    drop(loser);

    // Reopen validates the complete signed log, proves the old predecessor is
    // an exact prefix and that the request ID never committed, then durably
    // refreshes the cache before clearing the stale intent.
    let mut reopened_loser = witness.store(loser_dir.path());
    assert!(!loser_dir.path().join(PENDING_FILE).exists());
    let local: LedgerFile =
        read_optional_bounded(&loser_dir.path().join(LEDGER_FILE), MAX_LEDGER_BYTES)
            .unwrap()
            .unwrap();
    assert_eq!(local.entries, witness.entries);

    // The winner's second independent store sees the exact committed receipt.
    assert_eq!(
        winner.manage(winning_command, &mut witness).unwrap(),
        witness.entries[0]
    );
    // The old loser command still conflicts because its signed expected head
    // is immutable; the adapter never silently rebases or re-signs it.
    assert!(reopened_loser.manage(losing_command, &mut witness).is_err());

    let fresh_command = witness.issue_for(133, 7);
    reopened_loser.manage(fresh_command, &mut witness).unwrap();
    assert_eq!(witness.entries.len(), 2);
    assert_eq!(witness.entries[1].record.command.body.request_id, hash(133));
    assert!(!loser_dir.path().join(PENDING_FILE).exists());
}

#[test]
fn advanced_witness_does_not_clear_pending_with_tampered_candidate() {
    let (temp, mut witness) = raced_pending("tampered-pending-record", 134, 135);
    let mut pending: PendingIntent =
        read_optional_bounded(&temp.path().join(PENDING_FILE), MAX_LEDGER_BYTES)
            .unwrap()
            .unwrap();
    pending.record.next_anchor.state_digest = hash(249);
    fs::write(
        temp.path().join(PENDING_FILE),
        serde_json::to_vec(&pending).unwrap(),
    )
    .unwrap();

    assert!(witness.try_store(temp.path()).is_err());
    assert!(temp.path().join(PENDING_FILE).is_file());
}

#[test]
fn advanced_witness_does_not_clear_pending_with_tampered_management_signature() {
    let (temp, mut witness) = raced_pending("tampered-pending-signature", 136, 137);
    let mut pending: PendingIntent =
        read_optional_bounded(&temp.path().join(PENDING_FILE), MAX_LEDGER_BYTES)
            .unwrap()
            .unwrap();
    pending.command.signature_hex = "00".repeat(64);
    pending.record.command = pending.command.clone();
    fs::write(
        temp.path().join(PENDING_FILE),
        serde_json::to_vec(&pending).unwrap(),
    )
    .unwrap();

    assert!(witness.try_store(temp.path()).is_err());
    assert!(temp.path().join(PENDING_FILE).is_file());
}

#[test]
fn advanced_witness_does_not_clear_pending_from_a_different_prefix_fork() {
    let temp = TempDir::new("forked-pending-prefix");
    let configured_policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(configured_policy.clone());
    let mut witness_state = witness.state().unwrap();
    for request in [138, 139] {
        let command = issue(
            &witness_state,
            &configured_policy,
            request,
            target(request - 133, FullSyncReadRoleV2::StateSync),
            0,
        );
        let decision = commit_direct(&witness_state, &configured_policy, command, request);
        witness_state = decision.state().clone();
    }
    witness.entries = witness_state.records().to_vec();
    let issuer = witness.store(temp.path());
    drop(issuer);

    // Construct a valid signed candidate at revision one of a different
    // authenticated fork. The actual witness has advanced to revision two,
    // but the claimed predecessor is not in its complete history.
    let fork_genesis =
        VerifiedFullSyncIssuerStateV2::from_committed_records(configured_policy.clone(), vec![])
            .unwrap();
    let fork_first = issue(
        &fork_genesis,
        &configured_policy,
        140,
        target(7, FullSyncReadRoleV2::StateSync),
        0,
    );
    let fork_state = commit_direct(&fork_genesis, &configured_policy, fork_first, 140)
        .state()
        .clone();
    let command = issue(
        &fork_state,
        &configured_policy,
        141,
        target(8, FullSyncReadRoleV2::StateSync),
        0,
    );
    let record = fork_state
        .prepare(command.clone())
        .unwrap()
        .record()
        .unwrap()
        .clone();
    let pending = PendingIntent {
        schema_version: 2,
        command,
        record,
    };
    fs::write(
        temp.path().join(PENDING_FILE),
        serde_json::to_vec(&pending).unwrap(),
    )
    .unwrap();

    assert!(witness.try_store(temp.path()).is_err());
    assert!(temp.path().join(PENDING_FILE).is_file());
}

#[test]
fn cas_commit_lost_reply_reopens_to_the_original_receipt() {
    let temp = TempDir::new("lost-reply");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let command = witness.issue_for(15, 5);
    let mut issuer = witness.store(temp.path());
    witness.fault = Some(CommitFault::LoseReplyAfterCas);
    assert!(issuer.manage(command.clone(), &mut witness).is_err());
    assert!(temp.path().join(PENDING_FILE).is_file());
    let original_receipt = witness.entries[0].receipt.clone();
    drop(issuer);

    let mut reopened = witness.store(temp.path());
    let reconciled = reopened.manage(command, &mut witness).unwrap();
    assert_eq!(reconciled.receipt, original_receipt);
    assert_eq!(witness.entries.len(), 1);
    assert!(!temp.path().join(PENDING_FILE).exists());
}

#[test]
fn local_persist_failure_after_cas_is_poisoned_and_recovers_from_witness() {
    let temp = TempDir::new("local-persist-failure");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    witness.local_directory = Some(temp.path().to_path_buf());
    let command = witness.issue_for(16, 5);
    let mut issuer = witness.store(temp.path());
    witness.fault = Some(CommitFault::BlockLocalLedgerAfterCas);
    assert!(issuer.manage(command.clone(), &mut witness).is_err());
    assert!(temp.path().join(LEDGER_FILE).is_dir());
    assert!(temp.path().join(PENDING_FILE).is_file());
    drop(issuer);
    fs::remove_dir_all(temp.path().join(LEDGER_FILE)).unwrap();

    let mut reopened = witness.store(temp.path());
    assert!(fs::read(temp.path().join(LEDGER_FILE)).unwrap().len() > 2);
    let replay = reopened.manage(command, &mut witness).unwrap();
    assert_eq!(replay, witness.entries[0]);
}

#[test]
fn local_fsync_error_after_cas_keeps_intent_and_reconciles_after_reopen() {
    let temp = TempDir::new("local-fsync-error");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let command = witness.issue_for(162, 5);
    let mut issuer = witness.store(temp.path());
    issuer.fail_next_local_sync.set(true);

    assert!(issuer.manage(command.clone(), &mut witness).is_err());
    assert_eq!(witness.entries.len(), 1);
    assert!(temp.path().join(PENDING_FILE).is_file());
    let local: LedgerFile = read_optional_bounded(&temp.path().join(LEDGER_FILE), MAX_LEDGER_BYTES)
        .unwrap()
        .unwrap();
    assert!(local.entries.is_empty());
    drop(issuer);

    let mut reopened = witness.store(temp.path());
    assert_eq!(
        reopened.manage(command, &mut witness).unwrap(),
        witness.entries[0]
    );
    let local: LedgerFile = read_optional_bounded(&temp.path().join(LEDGER_FILE), MAX_LEDGER_BYTES)
        .unwrap()
        .unwrap();
    assert_eq!(local.entries, witness.entries);
    assert!(!temp.path().join(PENDING_FILE).exists());
}

#[test]
fn projected_json_capacity_is_rejected_before_intent_or_witness_cas() {
    let temp = TempDir::new("capacity-preflight");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let command = witness.issue_for(161, 5);
    let mut issuer = witness.store(temp.path());
    issuer.ledger_byte_limit = 256;

    assert!(issuer.manage(command, &mut witness).is_err());
    assert!(witness.entries.is_empty());
    assert!(!temp.path().join(PENDING_FILE).exists());
}

#[test]
fn valid_old_backup_is_repaired_from_current_witness_and_revocation_is_retained() {
    let temp = TempDir::new("backup-recovery");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let target = target(5, FullSyncReadRoleV2::StateSync);
    let issue = witness.issue_for(17, 5);
    let mut issuer = witness.store(temp.path());
    issuer.manage(issue.clone(), &mut witness).unwrap();
    let backup = fs::read(temp.path().join(LEDGER_FILE)).unwrap();

    let state = witness.state().unwrap();
    let revoke = command(
        &state,
        &policy,
        18,
        98,
        target.clone(),
        FullSyncIssuerOperationV2::Revoke,
        1,
    );
    issuer.manage(revoke.clone(), &mut witness).unwrap();
    drop(issuer);
    fs::write(temp.path().join(LEDGER_FILE), backup).unwrap();

    let mut reopened = witness.store(temp.path());
    let local: LedgerFile = read_optional_bounded(&temp.path().join(LEDGER_FILE), MAX_LEDGER_BYTES)
        .unwrap()
        .unwrap();
    let recovered =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), local.entries)
            .unwrap();
    assert_eq!(recovered.anchor(), witness.state().unwrap().anchor());
    assert!(!recovered.target_state(&target).unwrap().active);
    assert_eq!(
        reopened.manage(revoke, &mut witness).unwrap(),
        witness.entries[1]
    );
}

#[test]
fn corrupt_truncated_and_forked_local_or_remote_history_fails_closed() {
    let corrupt_dir = TempDir::new("corrupt-local");
    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let command = witness.issue_for(19, 5);
    let mut issuer = witness.store(corrupt_dir.path());
    issuer.manage(command, &mut witness).unwrap();
    drop(issuer);
    fs::write(corrupt_dir.path().join(LEDGER_FILE), b"{truncated").unwrap();
    let before = fs::read(corrupt_dir.path().join(LEDGER_FILE)).unwrap();
    assert!(witness.try_store(corrupt_dir.path()).is_err());
    assert_eq!(
        fs::read(corrupt_dir.path().join(LEDGER_FILE)).unwrap(),
        before
    );

    let remote_dir = TempDir::new("truncated-remote");
    let mut truncated = SimulatedWitness::new(policy.clone());
    let command = truncated.issue_for(20, 6);
    let mut issuer = truncated.store(remote_dir.path());
    issuer.manage(command, &mut truncated).unwrap();
    drop(issuer);
    truncated.truncate_response = true;
    let before = fs::read(remote_dir.path().join(LEDGER_FILE)).unwrap();
    assert!(truncated.try_store(remote_dir.path()).is_err());
    assert_eq!(
        fs::read(remote_dir.path().join(LEDGER_FILE)).unwrap(),
        before
    );

    let fork_dir = TempDir::new("forked-local");
    let mut left = SimulatedWitness::new(policy.clone());
    let left_command = left.issue_for(21, 7);
    let mut issuer = left.store(fork_dir.path());
    issuer.manage(left_command, &mut left).unwrap();
    drop(issuer);
    let local_before = fs::read(fork_dir.path().join(LEDGER_FILE)).unwrap();
    let mut right = SimulatedWitness::new(policy);
    let right_command = right.issue_for(22, 8);
    // Construct a valid alternative first record from the same genesis.
    let state = right.state().unwrap();
    let decision = commit_direct(&state, &right.policy, right_command, 23);
    right.entries.push(decision.entry().clone());
    assert!(right.try_store(fork_dir.path()).is_err());
    assert_eq!(
        fs::read(fork_dir.path().join(LEDGER_FILE)).unwrap(),
        local_before
    );
}

#[test]
fn stale_issuer_and_wrong_key_fail_before_state_mutation() {
    let temp = TempDir::new("wrong-issuer-key");
    let initial_policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(initial_policy.clone());
    let result = DurableFullSyncIssuerV2::open(
        temp.path().join("must-not-exist").as_path(),
        initial_policy.clone(),
        key(8),
        genesis(&initial_policy),
        &mut witness,
    );
    assert!(result.is_err());
    assert!(!temp.path().join("must-not-exist").exists());

    let first = witness.issue_for(24, 5);
    let initial = witness.state().unwrap();
    let committed = commit_direct(&initial, &initial_policy, first, 25);
    let next_policy = policy(2, 8, 9, 4);
    let transition = FullSyncIssuerPolicyTransitionV2 {
        previous_policy_digest: initial_policy.policy_digest().into(),
        previous_signer_generation: 1,
        new_policy_digest: next_policy.policy_digest().into(),
        new_signer_generation: 2,
    };
    let next_command = SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: next_policy.body().world.clone(),
            authority_epoch: next_policy.body().authority_epoch,
            scope_digest: next_policy.scope_digest().into(),
            policy_digest: next_policy.policy_digest().into(),
            signer_generation: 2,
            transition: Some(transition),
            target: target(6, FullSyncReadRoleV2::StateSync),
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 200,
                expires_unix_ms: 20_000,
            },
            expected_head: committed.state().anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(26),
            command_nonce: hash(27),
        },
        &key(9),
    )
    .unwrap();
    let transition_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        next_policy.clone(),
        committed.state().records().to_vec(),
    )
    .unwrap();
    let second = commit_direct(&transition_state, &next_policy, next_command, 28);
    let mut current_witness = SimulatedWitness::new(next_policy.clone());
    current_witness.entries = second.state().records().to_vec();
    let stale_dir = TempDir::new("stale-issuer");
    let stale_witness_policy = initial_policy.clone();
    let stale_adapter = DurableFullSyncIssuerV2::open(
        stale_dir.path(),
        stale_witness_policy.clone(),
        key(3),
        genesis(&stale_witness_policy),
        &mut current_witness,
    );
    assert!(stale_adapter.is_err());
}

#[test]
fn root_policy_and_private_store_boundaries_are_enforced() {
    let temp = TempDir::new("root-policy");
    let mut wrong_root_body = signed_policy(1, 3, 2, 4).body;
    wrong_root_body.root_public_key = public(7);
    let wrong_root = SignedFullSyncIssuerPolicyV2::sign(wrong_root_body, &key(7)).unwrap();
    assert!(VerifiedFullSyncIssuerPolicyV2::verify(&wrong_root, &trust()).is_err());

    let policy = policy(1, 3, 2, 4);
    let mut witness = SimulatedWitness::new(policy.clone());
    let issuer = witness.store(temp.path());
    let second = DurableFullSyncIssuerV2::open(
        temp.path(),
        policy.clone(),
        key(3),
        genesis(&policy),
        &mut witness,
    );
    assert!(matches!(second, Err(FormalError::Conflict(_))));
    drop(issuer);

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
        let metadata = fs::metadata(temp.path()).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        let lock = fs::metadata(temp.path().join(LOCK_FILE)).unwrap();
        assert_eq!(lock.permissions().mode() & 0o777, 0o600);
        // SAFETY: geteuid reads the current process identity.
        let uid = unsafe { libc::geteuid() };
        assert_eq!(lock.uid(), uid);

        let link = temp.path().with_extension("symlink");
        symlink(temp.path(), &link).unwrap();
        let mut other_witness = SimulatedWitness::new(policy.clone());
        assert!(
            DurableFullSyncIssuerV2::open(
                &link,
                policy.clone(),
                key(3),
                genesis(&policy),
                &mut other_witness,
            )
            .is_err()
        );
        let _ = fs::remove_file(link);
    }
}
