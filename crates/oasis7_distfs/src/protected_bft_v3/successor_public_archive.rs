//! Complete aliases for the exact original public State, never a new trust root.
use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::{BFT_SIGNER_MAX_FRAME_BYTES_V3, encode_bounded};
use super::state::BftSignerStateV3;
use super::successor_window::*;
use oasis7_agent_api::world_action::{
    BootstrapFinalityKeyRotationProofV1, BootstrapFinalityKeyRotationRoleV1,
    FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
};
use oasis7_consensus::tendermint_v3::{BftPublicProposalV3, BftSigningJournalEventV3};
use oasis7_proto::distributed_bft::v3::{BftVotePhaseV3, BftVoteV3};
use std::collections::BTreeMap;

/// The original signed DTO. Consumers must also authenticate and validate S.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BftSuccessorWindowOriginalSignedResultV1 {
    Proposal(Box<BftPublicProposalV3>),
    Vote(Box<BftVoteV3>),
    AdmittedProof(FinalityKeyRotationProofV1),
    BootstrapProof(BootstrapFinalityKeyRotationProofV1),
}
impl BftSuccessorWindowOriginalSignedResultV1 {
    pub fn signature_hex(&self) -> &str {
        match self {
            Self::Proposal(p) => &p.header.signature_hex,
            Self::Vote(v) => &v.signature_hex,
            Self::AdmittedProof(p) => &p.signature_hex,
            Self::BootstrapProof(p) => &p.signature_hex,
        }
    }
}
struct SignedAlias {
    identity: BftSuccessorWindowArchiveIdentityV1,
    result: BftSuccessorWindowOriginalSignedResultV1,
}
fn raw_digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn body_digest<T: serde::Serialize>(body: &T) -> Result<String, BftSignerErrorV3> {
    Ok(raw_digest(&encode_bounded(
        body,
        BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1,
    )?))
}
fn aliases(state: &BftSignerStateV3) -> Result<BTreeMap<String, SignedAlias>, BftSignerErrorV3> {
    let mut values = BTreeMap::<String, SignedAlias>::new();
    let mut insert = |identity: BftSuccessorWindowArchiveIdentityV1, result| {
        let digest = identity.digest()?;
        if let Some(previous) = values.get(&digest) {
            if previous.identity != identity || previous.result != result {
                return Err(invalid("successor signed identity conflict"));
            }
        } else {
            // Reserve one entry for the exact State identity.
            if values.len() >= BFT_SUCCESSOR_WINDOW_MAX_REFS_V1 - 1 {
                return Err(invalid("successor complete signed index capacity"));
            }
            values.insert(digest, SignedAlias { identity, result });
        }
        Ok(())
    };
    for journal in state
        .journal
        .iter()
        .chain(state.completed_journals.iter().flatten())
    {
        for event in &journal.events {
            match event {
                BftSigningJournalEventV3::Proposal {
                    proposal,
                    local: true,
                    ..
                } => {
                    proposal.validate_binding().map_err(invalid)?;
                    proposal.header.verify_signature().map_err(invalid)?;
                    insert(
                        BftSuccessorWindowArchiveIdentityV1::SigningSlot {
                            public_key_hex: proposal.header.public_key_hex.clone(),
                            height: proposal.header.height,
                            round: proposal.header.round,
                            phase: "proposal".into(),
                            preimage_digest: raw_digest(
                                &proposal.header.signing_payload().map_err(invalid)?,
                            ),
                        },
                        BftSuccessorWindowOriginalSignedResultV1::Proposal(proposal.clone()),
                    )?;
                }
                BftSigningJournalEventV3::Vote {
                    vote, local: true, ..
                } => {
                    vote.verify_signature().map_err(invalid)?;
                    insert(
                        BftSuccessorWindowArchiveIdentityV1::SigningSlot {
                            public_key_hex: vote.public_key_hex.clone(),
                            height: vote.height,
                            round: vote.round,
                            phase: match vote.phase {
                                BftVotePhaseV3::Prevote => "prevote",
                                BftVotePhaseV3::Precommit => "precommit",
                            }
                            .into(),
                            preimage_digest: raw_digest(&vote.signing_payload().map_err(invalid)?),
                        },
                        BftSuccessorWindowOriginalSignedResultV1::Vote(vote.clone()),
                    )?;
                }
                _ => {}
            }
        }
    }
    if let Some(archive) = &state.finality_rotation_proofs {
        for record in &archive.records {
            super::rotation::verify_proof(&record.body, &record.proof, record.proof.role)?;
            if record.public_key_hex
                != super::rotation::selected_key(&record.body, record.proof.role)
            {
                return Err(invalid("successor admitted proof key binding"));
            }
            insert(
                BftSuccessorWindowArchiveIdentityV1::FinalityProof {
                    body_digest: body_digest(&record.body)?,
                    role: match record.proof.role {
                        FinalityKeyRotationRoleV1::OldKeyAuthorization => "admitted_old",
                        FinalityKeyRotationRoleV1::NewKeyPossession => "admitted_new",
                    }
                    .into(),
                },
                BftSuccessorWindowOriginalSignedResultV1::AdmittedProof(record.proof.clone()),
            )?;
        }
    }
    if let Some(archive) = &state.bootstrap_finality_rotation_proofs {
        for record in &archive.records {
            super::bootstrap_rotation::verify_proof(
                &record.body,
                &record.proof,
                record.proof.role,
            )?;
            if record.public_key_hex
                != super::bootstrap_rotation::selected_key(&record.body, record.proof.role)
            {
                return Err(invalid("successor bootstrap proof key binding"));
            }
            insert(
                BftSuccessorWindowArchiveIdentityV1::FinalityProof {
                    body_digest: body_digest(&record.body)?,
                    role: match record.proof.role {
                        BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization => "bootstrap_old",
                        BootstrapFinalityKeyRotationRoleV1::NewKeyPossession => "bootstrap_new",
                    }
                    .into(),
                },
                BftSuccessorWindowOriginalSignedResultV1::BootstrapProof(record.proof.clone()),
            )?;
        }
    }
    Ok(values)
}
/// Derive the full bounded set from an authenticated, fully validated original S. Every
/// alias points at S; no remote event or private CanonicalRequest is indexed.
pub fn whole_state_signed_result_entries_v1(
    state: &BftSignerStateV3,
) -> Result<Vec<BftSuccessorWindowArchiveEntryV1>, BftSignerErrorV3> {
    let bytes = encode_bounded(state, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
    let object = BftSuccessorWindowArchiveRefV1 {
        digest: raw_digest(&bytes),
        bytes: bytes.len() as u64,
    };
    let mut entries: BTreeMap<_, _> = aliases(state)?
        .into_keys()
        .map(|id| (id, object.clone()))
        .collect();
    if entries.insert(state.state_digest.clone(), object).is_some() {
        return Err(invalid("successor State and signed identity collision"));
    }
    let entries: Vec<_> = entries
        .into_iter()
        .map(
            |(identity_digest, object)| BftSuccessorWindowArchiveEntryV1 {
                identity_digest,
                object,
            },
        )
        .collect();
    encode_bounded(&entries, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?;
    Ok(entries)
}
/// Opt in a new, uncommitted public manifest; legacy manifests must stay intact.
pub fn index_whole_state_signed_results_v1(
    manifest: &mut BftSuccessorWindowArchiveManifestV1,
    state: &BftSignerStateV3,
) -> Result<(), BftSignerErrorV3> {
    if manifest.kind != BftSuccessorWindowArchiveKindV1::Public {
        return Err(invalid("successor public index on non-public manifest"));
    }
    let bytes = encode_bounded(state, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
    let reference = BftSuccessorWindowArchiveRefV1 {
        digest: raw_digest(&bytes),
        bytes: bytes.len() as u64,
    };
    super::successor_state::validate_whole_state_public_manifest(
        manifest,
        &reference,
        &state.state_digest,
    )?;
    let mut indexed = manifest.clone();
    indexed.public_index = Some(BftSuccessorWindowPublicIndexV1::WholeStateSignedResultsV1);
    indexed.entries = whole_state_signed_result_entries_v1(state)?;
    indexed.validate()?;
    *manifest = indexed;
    Ok(())
}
pub(crate) fn validate_actual_public_manifest(
    manifest: &BftSuccessorWindowArchiveManifestV1,
    state: &BftSignerStateV3,
) -> Result<(), BftSignerErrorV3> {
    let bytes = encode_bounded(state, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
    let reference = BftSuccessorWindowArchiveRefV1 {
        digest: raw_digest(&bytes),
        bytes: bytes.len() as u64,
    };
    super::successor_state::validate_whole_state_public_manifest(
        manifest,
        &reference,
        &state.state_digest,
    )?;
    if manifest.public_index.is_some()
        && manifest.entries != whole_state_signed_result_entries_v1(state)?
    {
        return Err(invalid(
            "successor public index differs from complete original signed results",
        ));
    }
    Ok(())
}
/// Requires authenticated, fully validated original S. Match the complete typed
/// identity. A different preimage in an existing
/// signing slot is a conflict, never a cache miss permitting another signature.
pub fn resolve_whole_state_signed_result_v1(
    state: &BftSignerStateV3,
    identity: &BftSuccessorWindowArchiveIdentityV1,
) -> Result<Option<BftSuccessorWindowOriginalSignedResultV1>, BftSignerErrorV3> {
    identity.validate()?;
    if matches!(
        identity,
        BftSuccessorWindowArchiveIdentityV1::CanonicalRequest { .. }
    ) {
        return Err(invalid(
            "private canonical requests are unsupported by the public signed-result archive",
        ));
    }
    let values = aliases(state)?;
    if let Some(value) = values.get(&identity.digest()?) {
        if value.identity != *identity {
            return Err(invalid("successor signed identity collision"));
        }
        return Ok(Some(value.result.clone()));
    }
    if let BftSuccessorWindowArchiveIdentityV1::SigningSlot {
        public_key_hex,
        height,
        round,
        phase,
        ..
    } = identity
    {
        for value in values.values() {
            if let BftSuccessorWindowArchiveIdentityV1::SigningSlot {
                public_key_hex: k,
                height: h,
                round: r,
                phase: p,
                ..
            } = &value.identity
                && (k, h, r, p) == (public_key_hex, height, round, phase)
            {
                return Err(invalid("successor original signing slot preimage conflict"));
            }
        }
    }
    Ok(None)
}
