//! Shared acceptance path for a newly planned profile2 recovery.
//!
//! The writer and both endpoint operations use their actual service clients.
//! The issuer uses its production durable store. Its separately persisted
//! witness is a software simulation, not physical nonrollback evidence.
use ed25519_dalek::{Signature, SigningKey};
use oasis7_distfs::controlled_authority::{
    fencing::{EpochSealReceiptV1, TrustedFencingPolicyV1, verify_epoch_seal},
    formal::{
        endpoint::{FileFormalArchiveSourceV1, FormalArchiveSourceV1, FormalRequestStateV1},
        issuer::*,
        transport::RemoteFormalEndpoint,
        writer_signing::RemoteWriterCustodyClientV1,
        *,
    },
    replicated_protocol::ClosedRecord,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use crate::formal_live_runtime::tests::protected_process_tests::oracle_support::private_directory;

/// Inventory-derived copy budget for a software fixture, independent of its
/// deployment quota. Logical reads resolve packed objects; inline originals
/// in retained Qualified entries are included once by their actual hash.
pub(crate) fn recovery_copy_budget(directory: &Path) -> Result<u64, String> {
    let source = FileFormalArchiveSourceV1::new(directory.to_path_buf()).map_err(message)?;
    let mut objects = BTreeMap::<String, u64>::new();
    for id in source.logical_ids().map_err(message)? {
        let bytes = source
            .read_object(&id)
            .map_err(message)?
            .ok_or("recovery copy inventory lost an original object")?;
        insert_original(&mut objects, &id, &bytes)?;
        if let Ok(FormalRequestStateV1::Qualified { candidate, .. }) =
            serde_cbor::from_slice::<FormalRequestStateV1>(&bytes)
        {
            for material in
                std::iter::once(&candidate.material).chain(candidate.frozen_material.as_ref())
            {
                for object in &material.objects {
                    insert_original(&mut objects, &object.content_hash, &object.bytes)?;
                }
            }
        }
    }
    objects.values().try_fold(64 * 1024u64, |total, bytes| {
        total
            .checked_add(*bytes)
            .ok_or_else(|| "recovery copy inventory byte sum overflow".into())
    })
}

fn insert_original(
    objects: &mut BTreeMap<String, u64>,
    id: &str,
    bytes: &[u8],
) -> Result<(), String> {
    if blake3::hash(bytes).to_hex().as_str() != id {
        return Err("recovery copy inventory object digest mismatch".into());
    }
    let length = u64::try_from(bytes.len()).map_err(message)?;
    if objects
        .insert(id.to_owned(), length)
        .is_some_and(|old| old != length)
    {
        return Err("recovery copy inventory original length changed".into());
    }
    Ok(())
}

pub(crate) struct PlanInputs<'a> {
    pub directory: &'a Path,
    pub original: &'a VerifiedFormalReadAnchorV1,
    pub trust: &'a InitialActivationTrustV1,
    pub operation_id: String,
    pub successor: AuthorityDefinitionV1,
    pub fixed: &'a RecoveryTailWitnessV1,
    pub fixed_material: Option<&'a ClosedRecord>,
    pub pending: &'a RecoveryPendingHandoffV1,
    pub issuer_key: &'a SigningKey,
    pub witness_key: &'a SigningKey,
    pub minimum: IssuerAnchorV1,
    pub authorization: RecoveryPlanAuthorityV1,
}

/// Installing this plan closes normal writer admission. The caller must finish
/// its exact allowlisted old-epoch continuation before calling `finish`.
pub(crate) struct PreparedRecovery {
    pub plan: SignedRecoveryPlanV1,
    pub pending: RecoveryPendingHandoffV1,
    pub writer_ack: SignedWriterRecoveryPlanAckV1,
    original: VerifiedFormalReadAnchorV1,
    trust: InitialActivationTrustV1,
    issuer_directory: PathBuf,
    witness_path: PathBuf,
    issuer_key: SigningKey,
    witness_key: SigningKey,
    minimum: IssuerAnchorV1,
}

pub(crate) struct CompletedRecovery {
    pub permission: RecoveryPermissionBootstrapV1,
    pub verified: VerifiedRecoveryBootstrapV1,
    pub seals: [EpochSealReceiptV1; 2],
    pub drained: RecoveryDrainedProofV1,
}

impl PreparedRecovery {
    pub(crate) fn prepare(
        input: PlanInputs<'_>,
        writer: &RemoteWriterCustodyClientV1,
    ) -> Result<Self, String> {
        private_directory(input.directory)?;
        let authority = input.original.authority();
        let intent = RecoveryCertificateBodyV1 {
            schema_version: 1,
            world: authority.definition().world.clone(),
            operation_id: input.operation_id,
            predecessor_authority_id: authority.authority_id().map_err(message)?,
            predecessor_epoch: authority.definition().epoch,
            sealed_cut: input.original.head().head(),
            tail: recovery_tail_from_witness(input.fixed).map_err(message)?,
            // `new` replaces this with the explicit unsealed plan marker.
            fencing_evidence_digest: "00".repeat(32),
            successor: input.successor,
            first_normal_position: None,
            retained_context: None,
        };
        let checked = VerifiedRecoveryPlanV1::verify(
            RecoveryPlanBodyV1::new(
                intent,
                authority,
                input.original.head(),
                input.fixed,
                input.pending,
                input.authorization,
            )
            .map_err(message)?,
            authority,
            input.original.head(),
            input.fixed,
            input.pending,
        )
        .map_err(message)?;
        let issuer_directory = input.directory.join("issuer");
        let open = || {
            RecoveryIssuer::open(
                &issuer_directory,
                authority.policy().clone(),
                input.issuer_key.clone(),
                input.minimum.clone(),
            )
            .map_err(message)
        };
        let mut issuer = open()?;
        let plan = issuer.prepare_plan(&checked).map_err(message)?;
        drop(issuer);
        // Cold reconciliation must return the original signed plan, before
        // either actual endpoint is permitted to prepare or seal.
        let mut issuer = open()?;
        if issuer.prepare_plan(&checked).map_err(message)? != plan {
            return Err("cold issuer changed the original recovery plan".into());
        }
        drop(issuer);
        let writer_ack = writer
            .prepare_recovery_plan(
                &plan,
                input.fixed,
                input.fixed_material,
                input.pending,
                authority,
            )
            .map_err(message)?;
        Ok(Self {
            plan,
            pending: input.pending.clone(),
            writer_ack,
            original: input.original.clone(),
            trust: input.trust.clone(),
            issuer_directory,
            witness_path: input.directory.join("independent-software-witness.json"),
            issuer_key: input.issuer_key.clone(),
            witness_key: input.witness_key.clone(),
            minimum: input.minimum,
        })
    }

    /// The surviving endpoint can install first. A lost peer must be repaired
    /// using the real protected-provider grant before this same operation is
    /// sent to its replacement; no synthetic peer image is accepted here.
    pub(crate) fn install_on(&self, endpoint: &mut RemoteFormalEndpoint) -> Result<(), String> {
        let checkpoint = endpoint.checkpoint().map_err(message)?;
        endpoint
            .prepare_recovery_plan(
                self.plan.clone(),
                self.pending.clone(),
                self.writer_ack.clone(),
                checkpoint,
            )
            .map_err(message)?;
        if endpoint.recovery_pending_handoff().map_err(message)? != self.pending {
            return Err("endpoint changed the complete original pending handoff".into());
        }
        Ok(())
    }

    /// `drained_anchor` must come from the actual qualified old-epoch execution
    /// or, for an empty fixed set, the unchanged original authenticated cut.
    pub(crate) fn finish(
        &self,
        drained_anchor: &VerifiedFormalReadAnchorV1,
        writer: &RemoteWriterCustodyClientV1,
        primary: &mut RemoteFormalEndpoint,
        replica: &mut RemoteFormalEndpoint,
    ) -> Result<CompletedRecovery, String> {
        let authority = self.original.authority();
        if drained_anchor.authority().authority_id().map_err(message)?
            != authority.authority_id().map_err(message)?
            || drained_anchor.initial_certificate() != self.original.initial_certificate()
        {
            return Err(
                "drained anchor changed the original epoch or initial authorization".into(),
            );
        }
        let drained = primary.recovery_drained_proof().map_err(message)?;
        if replica.recovery_drained_proof().map_err(message)? != drained
            || drained.verify(&self.plan, authority).map_err(message)?
                != drained_anchor.head().head()
            || &drained.qualified.decision.record != drained_anchor.head().record()
        {
            return Err(
                "actual dual drained originals differ from the verified execution cut".into(),
            );
        }
        for endpoint in [&mut *primary, &mut *replica] {
            if endpoint.recovery_pending_handoff().map_err(message)? != self.pending {
                return Err("drain lost or changed the original pending handoff".into());
            }
        }
        let retained = writer
            .complete_drain(&self.plan, &drained, authority)
            .map_err(message)?;
        if retained != drained {
            return Err("writer custody changed the actual drained original".into());
        }
        let writer_seal = writer
            .seal_epoch(&self.plan, &drained, authority)
            .map_err(message)?;
        let primary_checkpoint = primary.checkpoint().map_err(message)?;
        let primary_seal = primary
            .seal_planned_epoch(writer_seal.clone(), primary_checkpoint)
            .map_err(message)?;
        let replica_checkpoint = replica.checkpoint().map_err(message)?;
        let replica_seal = replica
            .seal_planned_epoch(writer_seal, replica_checkpoint)
            .map_err(message)?;
        let policy = TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        };
        let p = verify_epoch_seal(&primary_seal, &policy).map_err(message)?;
        let r = verify_epoch_seal(&replica_seal, &policy).map_err(message)?;
        let mut body = self.plan.body.intent.clone();
        body.schema_version = 3;
        body.sealed_cut = drained_anchor.head().head();
        body.tail = RecoveryTailV1::Empty;
        body.retained_context = None;
        body.fencing_evidence_digest = recovery_fencing_digest([&p, &r]).map_err(message)?;
        let intent = VerifiedRecoveryIntent::verify(
            body,
            authority,
            drained_anchor.head(),
            [&p, &r],
            &RecoveryTailWitnessV1::Empty,
        )
        .map_err(message)?;
        let mut issuer = RecoveryIssuer::open(
            &self.issuer_directory,
            authority.policy().clone(),
            self.issuer_key.clone(),
            self.minimum.clone(),
        )
        .map_err(message)?;
        let mut witness = SoftwareWitness {
            path: self.witness_path.clone(),
            policy: authority.policy().clone(),
            key: self.witness_key.clone(),
            original_floor: self.minimum.clone(),
        };
        let (certificate, issuer_witness, _) =
            issuer.issue(&intent, &mut witness).map_err(message)?;
        let checkpoint = drained_anchor.checkpoint();
        let seals = [primary_seal, replica_seal];
        let permission = RecoveryPermissionBootstrapV1 {
            predecessor_checkpoint_digest: checkpoint.digest().map_err(message)?,
            predecessor_checkpoint: checkpoint,
            predecessor_context_version: None,
            certificate,
            fencing: FencingBootstrapV1::Dual {
                primary: Box::new(seals[0].clone()),
                replica: Box::new(seals[1].clone()),
            },
            tail: RecoveryTailWitnessV1::Empty,
            issuer_witness,
            minimum_issuer_anchor: self.minimum.clone(),
            prior_permission: None,
        };
        let verified = permission.verify_at_minimum(
            &self.trust,
            self.original.initial_certificate(),
            &self.minimum,
        )?;
        Ok(CompletedRecovery {
            permission,
            verified,
            seals,
            drained,
        })
    }
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessState {
    certificate: SignedRecoveryCertificateV1,
    anchor: IssuerAnchorV1,
}

/// One immutable grant for this fixture's independently stored predecessor.
/// This implements the real issuer challenge protocol but models its external
/// monotonic guarantee in software only.
struct SoftwareWitness {
    path: PathBuf,
    policy: RecoveryPolicyV1,
    key: SigningKey,
    original_floor: IssuerAnchorV1,
}

impl SoftwareWitness {
    fn read(&self) -> Result<Option<WitnessState>, FormalError> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| FormalError::Invalid(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(FormalError::Io(error.to_string())),
        }
    }

    fn check_request(
        &self,
        issuer: &str,
        binding: &str,
        minimum: &IssuerAnchorV1,
        current: &IssuerAnchorV1,
    ) -> Result<(), FormalError> {
        if issuer != self.policy.recovery_issuer_public_key
            || binding != policy_binding(&self.policy)?
            || hex::encode(self.key.verifying_key().to_bytes())
                != self.policy.issuer_anchor_public_key
            || (minimum != &self.original_floor && minimum != current)
        {
            return Err(FormalError::IssuerUnproven);
        }
        Ok(())
    }

    fn persist(&self, state: &WitnessState) -> Result<(), FormalError> {
        let bytes =
            serde_json::to_vec(state).map_err(|error| FormalError::Invalid(error.to_string()))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&self.path)
            .map_err(|error| FormalError::Io(error.to_string()))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| FormalError::Io(error.to_string()))?;
        fs::File::open(self.path.parent().ok_or_else(|| {
            FormalError::Invalid("software witness has no independent directory".into())
        })?)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| FormalError::Io(error.to_string()))
    }
}

impl IssuanceWitness for SoftwareWitness {
    fn query_predecessor(
        &mut self,
        query: &SignedIssuerGrantQueryV1,
    ) -> Result<SignedIssuerGrantQueryResultV1, FormalError> {
        query.verify()?;
        let state = self.read()?;
        let current = state
            .as_ref()
            .map_or(&self.original_floor, |state| &state.anchor);
        self.check_request(
            &query.body.issuer_public_key,
            &query.body.policy_binding,
            &query.body.minimum_anchor,
            current,
        )?;
        if state.as_ref().is_some_and(|state| {
            state.certificate.body.predecessor_authority_id != query.body.predecessor_authority_id
        }) {
            return Err(FormalError::Conflict(
                "software witness directory belongs to another predecessor".into(),
            ));
        }
        SignedIssuerGrantQueryResultV1::sign(
            IssuerGrantQueryResultBodyV1 {
                schema_version: 1,
                capability: IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
                query: query.body.clone(),
                current_anchor: current.clone(),
                grant: state.map(|state| ExistingIssuerGrantV1 {
                    grant_revision: state.anchor.revision,
                    certificate: state.certificate,
                }),
            },
            &self.key,
        )
    }

    fn authorize_or_query(
        &mut self,
        request: &SignedIssuanceRequestV1,
        certificate: &SignedRecoveryCertificateV1,
    ) -> Result<SignedIssuanceWitnessV1, FormalError> {
        request.verify()?;
        let state = self.read()?;
        let current = state
            .as_ref()
            .map_or(&self.original_floor, |state| &state.anchor);
        self.check_request(
            &request.body.issuer_public_key,
            &request.body.policy_binding,
            &request.body.minimum_anchor,
            current,
        )?;
        if request.body.certificate_digest != recovery_certificate_digest(certificate)?
            || request.body.predecessor_authority_id != certificate.body.predecessor_authority_id
        {
            return Err(FormalError::IssuerUnproven);
        }
        ed25519_dalek::VerifyingKey::from_bytes(&canonical::<32>(
            &self.policy.recovery_issuer_public_key,
        )?)
        .map_err(|_| FormalError::IssuerUnproven)?
        .verify_strict(
            &recovery_signing_bytes(&certificate.body)?,
            &Signature::from_bytes(&canonical::<64>(&certificate.signature_hex)?),
        )
        .map_err(|_| FormalError::IssuerUnproven)?;
        let state = if let Some(state) = state {
            if &state.certificate != certificate {
                return Err(FormalError::Conflict(
                    "software witness retains another original successor".into(),
                ));
            }
            state
        } else {
            let revision =
                self.original_floor.revision.checked_add(1).ok_or_else(|| {
                    FormalError::Invalid("software witness revision overflow".into())
                })?;
            let state = WitnessState {
                certificate: certificate.clone(),
                anchor: IssuerAnchorV1 {
                    revision,
                    digest: blake3::hash(
                        &serde_cbor::to_vec(&(&self.original_floor, certificate))
                            .map_err(|error| FormalError::Invalid(error.to_string()))?,
                    )
                    .to_hex()
                    .to_string(),
                },
            };
            self.persist(&state)?;
            state
        };
        SignedIssuanceWitnessV1::sign(
            IssuanceWitnessBodyV1 {
                schema_version: 1,
                capability: IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
                request: request.body.clone(),
                grant_revision: state.anchor.revision,
                current_anchor: state.anchor,
            },
            &self.key,
        )
    }
}
