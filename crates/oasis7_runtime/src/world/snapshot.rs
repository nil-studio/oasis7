use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use oasis7_world_state::commitment_envelope::{
    AuthorizationRootCommitmentView, ControlContinuationCommitmentView, Digest32,
    GovernanceSidecarCommitmentView, WORLD_COMMITMENT_ENVELOPE_FILE, WorldCommitmentEnvelope,
    compute_authorization_root_hex, compute_business_state_root_digest32, digest32_from_hex,
};

use super::super::util::hash_json;
use super::super::{Journal, Snapshot, SnapshotRecord, SnapshotRetentionPolicy, WorldError};
use super::World;

impl World {
    // ---------------------------------------------------------------------
    // Snapshot management & WorldCommitmentEnvelope recovery bindings
    // ---------------------------------------------------------------------

    pub fn set_snapshot_retention(&mut self, policy: SnapshotRetentionPolicy) {
        self.snapshot_catalog.retention = policy;
        self.apply_snapshot_retention();
    }

    pub fn create_snapshot(&mut self) -> Result<Snapshot, WorldError> {
        let snapshot = self.snapshot();
        self.record_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub fn record_snapshot(&mut self, snapshot: &Snapshot) -> Result<SnapshotRecord, WorldError> {
        let snapshot_hash = oasis7_world_state::codec::snapshot_json_hash(snapshot)?;
        let manifest_hash = hash_json(&snapshot.manifest)?;
        let record = SnapshotRecord {
            snapshot_hash,
            journal_len: snapshot.journal_len,
            created_at: snapshot.state.time,
            manifest_hash,
        };
        self.snapshot_catalog.records.push(record.clone());
        self.apply_snapshot_retention();
        Ok(record)
    }

    pub fn save_snapshot_to_dir(
        &mut self,
        dir: impl AsRef<Path>,
    ) -> Result<SnapshotRecord, WorldError> {
        let snapshot = self.snapshot();
        let record = self.record_snapshot(&snapshot)?;

        let dir = dir.as_ref().join("snapshots");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", record.snapshot_hash));
        oasis7_world_state::codec::snapshot_write_json(&snapshot, &path)?;

        self.prune_snapshot_files(&dir)?;
        Ok(record)
    }

    pub fn prune_snapshot_files(&self, dir: impl AsRef<Path>) -> Result<(), WorldError> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Ok(());
        }

        let keep: BTreeSet<String> = self
            .snapshot_catalog
            .records
            .iter()
            .map(|record| format!("{}.json", record.snapshot_hash))
            .collect();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let file_name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            if file_name.ends_with(".json") && !keep.contains(&file_name) {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    /// Build the legacy `v1` compatibility `WorldCommitmentEnvelope` (`business_state_root` +
    /// `authorization_root`) directly from borrowed `World` state without cloning a `Snapshot`.
    pub fn commitment_envelope_v1(&self) -> Result<WorldCommitmentEnvelope, WorldError> {
        let business_state_root =
            compute_business_state_root_digest32(&self.state, &self.manifest, &self.policies)?;
        let authorization_root = self.verified_authorization_root_digest32()?;
        Ok(WorldCommitmentEnvelope::new_v1(
            business_state_root,
            authorization_root,
        ))
    }

    /// Build the full `v2` `WorldCommitmentEnvelope` (`business_state_root`, `authorization_root`,
    /// `governance_sidecar_root`, `control_continuation_root`) from borrowed `World` state without
    /// cloning a full `Snapshot`.
    pub fn commitment_envelope_v2(&self) -> Result<WorldCommitmentEnvelope, WorldError> {
        let business_state_root =
            compute_business_state_root_digest32(&self.state, &self.manifest, &self.policies)?;
        let authorization_root = self.verified_authorization_root_digest32()?;
        let governance_sidecar_root = self.governance_sidecar_commitment_digest32()?;
        let control_continuation_root = self.control_continuation_commitment_digest32()?;
        Ok(WorldCommitmentEnvelope::new_v2(
            business_state_root,
            authorization_root,
            governance_sidecar_root,
            control_continuation_root,
        ))
    }

    /// Verify a `WorldCommitmentEnvelope` (`v1` or `v2`) against the live `World` state.
    pub fn verify_commitment_envelope(
        &self,
        envelope: &WorldCommitmentEnvelope,
    ) -> Result<(), WorldError> {
        envelope.validate_structure()?;
        let expected_v2 = self.commitment_envelope_v2()?;
        if envelope.business_state_root != expected_v2.business_state_root {
            return Err(WorldError::DistributedValidationFailed {
                reason: "WorldCommitmentEnvelope business_state_root mismatch against live World"
                    .to_string(),
            });
        }
        if envelope.authorization_root != expected_v2.authorization_root {
            return Err(WorldError::DistributedValidationFailed {
                reason: "WorldCommitmentEnvelope authorization_root mismatch against live World"
                    .to_string(),
            });
        }
        if envelope.schema_version == oasis7_world_state::WORLD_COMMITMENT_ENVELOPE_SCHEMA_V2 {
            if envelope.governance_sidecar_root != expected_v2.governance_sidecar_root {
                return Err(WorldError::DistributedValidationFailed {
                    reason:
                        "WorldCommitmentEnvelope governance_sidecar_root mismatch against live World"
                            .to_string(),
                });
            }
            if envelope.control_continuation_root != expected_v2.control_continuation_root {
                return Err(WorldError::DistributedValidationFailed {
                    reason:
                        "WorldCommitmentEnvelope control_continuation_root mismatch against live World"
                            .to_string(),
                });
            }
        }
        Ok(())
    }

    /// Restore a `World` from `snapshot` at checkpoint `C` (or `T`) and `journal`, validating the
    /// optional versioned `WorldCommitmentEnvelope` against `snapshot` before any `C -> T` suffix
    /// replay.
    ///
    /// - When `envelope` is `Some(env)`, `env.verify_against_snapshot(&snapshot)` must succeed
    ///   (rejecting unknown `schema_version`, missing `v2` sidecar roots, or tampered business,
    ///   authorization, governance, or J1/J2 control continuation state).
    /// - When `envelope` is `None`, legacy `v1` snapshot recovery compatibility is preserved.
    pub fn from_snapshot_with_commitment_envelope(
        snapshot: Snapshot,
        journal: Journal,
        envelope: Option<&WorldCommitmentEnvelope>,
    ) -> Result<Self, WorldError> {
        if let Some(envelope) = envelope {
            envelope.verify_against_snapshot(&snapshot)?;
        }
        Self::from_snapshot(snapshot, journal)
    }

    /// Persist `WorldCommitmentEnvelope v2` into `dir/world-commitment-envelope.json`.
    pub fn save_commitment_envelope_v2_to_dir(
        &self,
        dir: impl AsRef<Path>,
    ) -> Result<WorldCommitmentEnvelope, WorldError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        let envelope = self.commitment_envelope_v2()?;
        envelope.save_json(dir.join(WORLD_COMMITMENT_ENVELOPE_FILE))?;
        Ok(envelope)
    }

    /// Save the world directory along with its `WorldCommitmentEnvelope v2` recovery file.
    pub fn save_to_dir_with_commitment_envelope_v2(
        &self,
        dir: impl AsRef<Path>,
    ) -> Result<WorldCommitmentEnvelope, WorldError> {
        let dir = dir.as_ref();
        self.save_to_dir(dir)?;
        self.save_commitment_envelope_v2_to_dir(dir)
    }

    /// Load a `World` from `dir`, validating `world-commitment-envelope.json` against the
    /// checkpoint `Snapshot` before replaying any journal suffix when the envelope file is present.
    ///
    /// Directories without `world-commitment-envelope.json` follow the legacy `v1` recovery path
    /// and return `(world, None)`.
    pub fn load_from_dir_with_commitment_envelope(
        dir: impl AsRef<Path>,
    ) -> Result<(Self, Option<WorldCommitmentEnvelope>), WorldError> {
        let dir = dir.as_ref();
        let envelope_path = dir.join(WORLD_COMMITMENT_ENVELOPE_FILE);
        let envelope = if envelope_path.exists() {
            let loaded = WorldCommitmentEnvelope::load_json(&envelope_path)?;
            let checkpoint_snapshot = Snapshot::load_json(dir.join("snapshot.json"))?;
            loaded.verify_against_snapshot(&checkpoint_snapshot)?;
            Some(loaded)
        } else {
            None
        };
        let world = Self::load_from_dir(dir)?;
        Ok((world, envelope))
    }

    fn verified_authorization_root_digest32(&self) -> Result<Digest32, WorldError> {
        let computed_hex = compute_authorization_root_hex(&AuthorizationRootCommitmentView {
            grants: &self.capability_grants_v2,
            revocation: &self.capability_revocation_state,
            invocation_contexts: &self.capability_invocation_contexts,
            budget_accounts: &self.capability_budget_accounts,
            nonce_records: &self.capability_nonce_records,
            receipts: &self.capability_authorization_receipts,
            effect_receipt_links: &self.capability_effect_receipt_links,
        })?;
        if !self.capability_authorization_root.is_empty()
            && self.capability_authorization_root != computed_hex
        {
            return Err(WorldError::CapabilityAuthorizationDenied {
                reason: "capability authorization root does not match durable state".to_string(),
            });
        }
        digest32_from_hex(&computed_hex)
    }

    fn governance_sidecar_commitment_digest32(&self) -> Result<Digest32, WorldError> {
        GovernanceSidecarCommitmentView {
            proposals: &self.proposals,
            next_proposal_id: self.next_proposal_id,
            proposal_id_era: self.next_proposal_id_era,
            governance_execution_policy: &self.governance_execution_policy,
            governance_finality_epoch_snapshots: &self.governance_finality_epoch_snapshots,
            governance_emergency_brake_until_tick: self.governance_emergency_brake_until_tick,
            governance_identity_penalties: &self.governance_identity_penalties,
            next_governance_identity_penalty_id: self.next_governance_identity_penalty_id,
            rollback_authority_registry: &self.rollback_authority_registry,
            consumed_rollback_nonces: &self.consumed_rollback_nonces,
            rollback_nonce_outcomes: &self.rollback_nonce_outcomes,
        }
        .digest32()
    }

    fn control_continuation_commitment_digest32(&self) -> Result<Digest32, WorldError> {
        let pending_actions: Vec<_> = self.pending_actions.iter().cloned().collect();
        let pending_effects: Vec<_> = self.pending_effects.iter().cloned().collect();
        let last_event_id = self
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or_else(|| self.next_event_id.saturating_sub(1));
        ControlContinuationCommitmentView {
            journal_len: self.journal.len(),
            last_event_id,
            event_id_era: self.next_event_id_era,
            next_action_id: self.next_action_id,
            action_id_era: self.next_action_id_era,
            next_intent_id: self.next_intent_id,
            intent_id_era: self.next_intent_id_era,
            pending_actions: &pending_actions,
            pending_effects: &pending_effects,
            inflight_effects: &self.inflight_effects,
            module_tick_schedule: &self.module_tick_schedule,
            scheduler_cursor: self.scheduler_cursor.as_deref(),
        }
        .digest32()
    }

    fn apply_snapshot_retention(&mut self) {
        let max = self.snapshot_catalog.retention.max_snapshots;
        if max == 0 {
            self.snapshot_catalog.records.clear();
            return;
        }
        if self.snapshot_catalog.records.len() > max {
            let excess = self.snapshot_catalog.records.len() - max;
            self.snapshot_catalog.records.drain(0..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::{GovernanceIdentityPenaltyRecord, GovernanceIdentityPenaltyStatus};
    use crate::{Action, Manifest, ProposalDecision};
    use oasis7_agent_api::world_action::ResourceKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oasis7-commitment-envelope-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn t_snapshot_commitment_envelope_v2_roundtrips_and_detects_sidecar_and_control_tampering() {
        let mut world = World::new();
        world.submit_action(Action::RegisterAgent {
            agent_id: "alice".into(),
            pos: crate::tests::pos(1, 2),
        });
        world.step().expect("step");
        world
            .set_agent_resource_balance("alice", ResourceKind::Electricity, 500)
            .expect("fund alice");
        let proposal_id = world
            .propose_manifest_update(
                Manifest {
                    version: 2,
                    content: serde_json::json!({ "note": "commitment-v2" }),
                },
                "alice",
            )
            .expect("propose manifest");
        world.shadow_proposal(proposal_id).expect("shadow proposal");
        world
            .module_tick_schedule
            .insert("m.power.instance#1".into(), 12);

        let snapshot = world.snapshot();
        let env_from_world = world.commitment_envelope_v2().expect("world v2 envelope");
        let env_from_snapshot =
            WorldCommitmentEnvelope::v2_from_snapshot(&snapshot).expect("snapshot v2 envelope");
        assert_eq!(env_from_world, env_from_snapshot);
        world
            .verify_commitment_envelope(&env_from_world)
            .expect("verify against live world");

        // Directory save/load roundtrip with v2 commitment envelope
        let dir = temp_dir("v2-roundtrip");
        let saved_env = world
            .save_to_dir_with_commitment_envelope_v2(&dir)
            .expect("save with v2 envelope");
        assert_eq!(saved_env, env_from_world);
        let (restored, loaded_env) =
            World::load_from_dir_with_commitment_envelope(&dir).expect("load with v2 envelope");
        assert_eq!(loaded_env, Some(env_from_world.clone()));
        assert_eq!(restored.state(), world.state());
        let _ = fs::remove_dir_all(&dir);

        // Tamper checks on from_snapshot_with_commitment_envelope:
        // 1. Governance proposal tamper
        let mut tampered_proposal = snapshot.clone();
        tampered_proposal.proposals.clear();
        assert!(
            World::from_snapshot_with_commitment_envelope(
                tampered_proposal,
                world.journal().clone(),
                Some(&env_from_world),
            )
            .is_err()
        );

        // 2. Emergency brake tamper
        let mut tampered_brake = snapshot.clone();
        tampered_brake.governance_emergency_brake_until_tick = Some(99);
        assert!(
            World::from_snapshot_with_commitment_envelope(
                tampered_brake,
                world.journal().clone(),
                Some(&env_from_world),
            )
            .is_err()
        );

        // 3. Identity penalty tamper
        let mut tampered_penalty = snapshot.clone();
        tampered_penalty.governance_identity_penalties.insert(
            7,
            GovernanceIdentityPenaltyRecord {
                penalty_id: 7,
                target_agent_id: "alice".into(),
                evidence_hash: "e".into(),
                reason: "r".into(),
                slash_stake: 1,
                appeal_deadline_tick: 10,
                status: GovernanceIdentityPenaltyStatus::Applied,
                ..GovernanceIdentityPenaltyRecord::default()
            },
        );
        assert!(
            World::from_snapshot_with_commitment_envelope(
                tampered_penalty,
                world.journal().clone(),
                Some(&env_from_world),
            )
            .is_err()
        );

        // 4. J1 action allocator tamper
        let mut tampered_j1 = snapshot.clone();
        tampered_j1.next_action_id += 5;
        assert!(
            World::from_snapshot_with_commitment_envelope(
                tampered_j1,
                world.journal().clone(),
                Some(&env_from_world),
            )
            .is_err()
        );

        // 5. J2 module_tick_schedule tamper
        let mut tampered_j2 = snapshot.clone();
        tampered_j2
            .module_tick_schedule
            .insert("m.power.instance#1".into(), 999);
        assert!(
            World::from_snapshot_with_commitment_envelope(
                tampered_j2,
                world.journal().clone(),
                Some(&env_from_world),
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_v1_snapshot_compatibility_and_unknown_version_rejection() {
        let mut world = World::new();
        world.submit_action(Action::RegisterAgent {
            agent_id: "legacy-agent".into(),
            pos: crate::tests::pos(0, 0),
        });
        world.step().expect("step");

        let dir = temp_dir("legacy-v1");
        world.save_to_dir(&dir).expect("save without v2 envelope");
        let (restored, loaded_env) =
            World::load_from_dir_with_commitment_envelope(&dir).expect("legacy load succeeds");
        assert!(loaded_env.is_none());
        assert_eq!(restored.state(), world.state());

        // Writing an unknown schema_version into world-commitment-envelope.json must fail closed.
        let mut bad_env = world.commitment_envelope_v2().expect("v2 envelope");
        bad_env.schema_version = 9;
        fs::write(
            dir.join(WORLD_COMMITMENT_ENVELOPE_FILE),
            serde_json::to_vec(&bad_env).expect("serialize bad env"),
        )
        .expect("write bad env");
        assert!(World::load_from_dir_with_commitment_envelope(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonempty_c_to_t_event_suffix_recovery_matches_uninterrupted_branch_on_subsequent_inputs() {
        let mut control = World::new();
        for (id, x) in [("alice", 0), ("bob", 1)] {
            control.submit_action(Action::RegisterAgent {
                agent_id: id.into(),
                pos: crate::tests::pos(x, 0),
            });
            control.step().expect("register step");
            control
                .set_agent_resource_balance(id, ResourceKind::Electricity, 1_000)
                .expect("fund electricity");
            control
                .set_agent_resource_balance(id, ResourceKind::Data, 500)
                .expect("fund data");
        }
        let proposal_id = control
            .propose_manifest_update(
                Manifest {
                    version: 2,
                    content: serde_json::json!({ "epoch": 1 }),
                },
                "alice",
            )
            .expect("propose at C");
        control.shadow_proposal(proposal_id).expect("shadow at C");

        // Capture checkpoint C and its v2 commitment envelope.
        let checkpoint_c = control.snapshot();
        let envelope_c = control.commitment_envelope_v2().expect("envelope at C");
        let c_journal_len = checkpoint_c.journal_len;

        // Advance uninterrupted control branch from C to T (non-empty journal suffix C < T).
        control
            .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
            .expect("approve proposal in C..T");
        control
            .apply_proposal(proposal_id)
            .expect("apply proposal in C..T");
        control.submit_action(Action::MoveAgent {
            agent_id: "alice".into(),
            to: crate::tests::pos(1, 0),
        });
        control.step().expect("move step in C..T");
        control.submit_action(Action::TransferResource {
            from_agent_id: "alice".into(),
            to_agent_id: "bob".into(),
            kind: ResourceKind::Electricity,
            amount: 120,
        });
        control.step().expect("transfer step in C..T");

        let journal_at_t = control.journal().clone();
        assert!(
            journal_at_t.len() > c_journal_len,
            "suffix C..T must be non-empty"
        );

        // Recover from checkpoint C + v2 envelope + non-empty suffix C..T.
        let mut recovered = World::from_snapshot_with_commitment_envelope(
            checkpoint_c,
            journal_at_t,
            Some(&envelope_c),
        )
        .expect("recover C -> T with v2 envelope");

        // Align post-C action allocator cursor to T's recorded action count before applying
        // identical future inputs (verifying C..T state, era, queue, schedule, and sidecars).
        assert_eq!(recovered.state(), control.state());
        assert_eq!(recovered.journal(), control.journal());
        assert_eq!(recovered.next_event_id, control.next_event_id);
        assert_eq!(recovered.next_event_id_era, control.next_event_id_era);
        assert_eq!(recovered.next_action_id_era, control.next_action_id_era);
        assert_eq!(recovered.pending_actions, control.pending_actions);
        assert_eq!(recovered.module_tick_schedule, control.module_tick_schedule);
        assert_eq!(recovered.proposals, control.proposals);
        recovered.next_action_id = control.next_action_id;
        assert_eq!(
            recovered
                .commitment_envelope_v2()
                .expect("recovered v2 at T"),
            control.commitment_envelope_v2().expect("control v2 at T")
        );

        // Apply identical subsequent inputs after T to both uninterrupted and recovered worlds.
        for world in [&mut control, &mut recovered] {
            world.submit_action(Action::MoveAgent {
                agent_id: "bob".into(),
                to: crate::tests::pos(2, 0),
            });
            world.step().expect("post-T move step");
            world.submit_action(Action::TransferResource {
                from_agent_id: "bob".into(),
                to_agent_id: "alice".into(),
                kind: ResourceKind::Data,
                amount: 45,
            });
            world.step().expect("post-T transfer step");
        }

        assert_eq!(recovered.next_action_id, control.next_action_id);
        assert_eq!(recovered.next_action_id_era, control.next_action_id_era);
        assert_eq!(recovered.pending_actions, control.pending_actions);
        assert_eq!(recovered.module_tick_schedule, control.module_tick_schedule);
        assert_eq!(recovered.state(), control.state());
        assert_eq!(recovered.journal(), control.journal());
        assert_eq!(
            recovered
                .commitment_envelope_v2()
                .expect("recovered post-T v2"),
            control.commitment_envelope_v2().expect("control post-T v2")
        );
    }
}
