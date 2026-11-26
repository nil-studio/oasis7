//! Standalone v2 OriginalAdmissionInput request + subject proof contract.
//! No handler registration, candidate-read promotion, current witness, material
//! release, transfer budget, response codec, or runtime execution authority.
use super::wire::{
    CHUNK, MAX_FRAME, decode_frame, digest, encode, encode_frame, hash, verify_signature,
};
use oasis7_distfs::protected_bft_v3::{
    BftAdmissionInputTransferPlanV2, BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionReadGrantV1,
};
use oasis7_proto::distributed_bft::v2::BftScopeV2;
use serde::{Deserialize, Serialize};

pub(super) const PROTOCOL_V2: &str = "/oasis7/bft/admission-input/2.0.0";
const DOMAIN_V2: &str = "oasis7.bft.admission-input.exchange.v2";
const SUBJECT_V2: &[u8] = b"oasis7.bft.admission-input.subject-batch.v2\0";
const MAX_PARTS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AdmissionBatchOperationV2 {
    Manifest,
    Batch { offset_bytes: u64, part_count: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionBatchReadV2 {
    pub reference: InputRef,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub nonce: String,
    pub operation: AdmissionBatchOperationV2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionInputPartRangeV2 {
    pub index: u32,
    pub offset: u64,
    pub length: u32,
}

impl AdmissionBatchReadV2 {
    pub fn ranges(&self) -> Result<Vec<AdmissionInputPartRangeV2>, String> {
        let AdmissionBatchOperationV2::Batch {
            offset_bytes,
            part_count,
        } = self.operation
        else {
            return Ok(Vec::new());
        };
        if part_count == 0
            || part_count > MAX_PARTS
            || !offset_bytes.is_multiple_of(u64::from(CHUNK))
            || offset_bytes >= self.reference.object.bytes
        {
            return Err("original admission batch offset/count invalid".into());
        }
        let first = u32::try_from(offset_bytes / u64::from(CHUNK))
            .map_err(|_| "original admission batch offset conversion")?;
        let mut out = Vec::with_capacity(usize::from(part_count));
        for delta in 0..u32::from(part_count) {
            let index = first
                .checked_add(delta)
                .ok_or("original admission batch index overflow")?;
            let offset = u64::from(index) * u64::from(CHUNK);
            let remaining = self
                .reference
                .object
                .bytes
                .checked_sub(offset)
                .ok_or("original admission batch range beyond object")?;
            let length = remaining.min(u64::from(CHUNK)) as u32;
            if length == 0 {
                return Err("original admission batch empty part".into());
            }
            out.push(AdmissionInputPartRangeV2 {
                index,
                offset,
                length,
            });
        }
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), String> {
        BftAdmissionInputTransferPlanV2::for_original(&self.reference)
            .map_err(|_| "original admission reference/plan invalid")?;
        hash(&self.nonce)?;
        let requester = self
            .requester_peer_id
            .parse::<libp2p::PeerId>()
            .map_err(|_| "original admission requester peer invalid")?;
        let serving = self
            .serving_peer_id
            .parse::<libp2p::PeerId>()
            .map_err(|_| "original admission serving peer invalid")?;
        if requester == serving {
            return Err("original admission peer self-read forbidden".into());
        }
        self.ranges()?;
        Ok(())
    }

    fn bind_grant(&self, grant: &BftPrivateRetentionAdmissionReadGrantV1) -> Result<(), String> {
        self.validate()?;
        grant
            .body
            .validate()
            .map_err(|_| "original admission malformed dedicated grant")?;
        hash(&grant.issuer_public_key)?;
        signature_shape(&grant.signature_hex)?;
        if grant.body.reference != self.reference
            || grant.body.requester_peer_id != self.requester_peer_id
            || grant.body.serving_peer_id != self.serving_peer_id
            || grant.body.request_nonce != self.nonce
        {
            return Err("original admission dedicated grant mismatch".into());
        }
        Ok(())
    }

    pub fn signing_bytes(
        &self,
        grant: &BftPrivateRetentionAdmissionReadGrantV1,
    ) -> Result<Vec<u8>, String> {
        self.bind_grant(grant)?;
        let encoded = encode(&(self, grant))?;
        if encoded.len() > MAX_FRAME - SUBJECT_V2.len() {
            return Err("original admission subject signed payload too large".into());
        }
        let mut result = SUBJECT_V2.to_vec();
        result.extend(encoded);
        Ok(result)
    }
}

fn signature_shape(signature: &str) -> Result<(), String> {
    if signature.len() != 128
        || !signature
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("original admission signature encoding".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionBatchAuthorizationV2 {
    pub grant: BftPrivateRetentionAdmissionReadGrantV1,
    pub subject_public_key: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionBatchRequestV2 {
    pub domain: String,
    pub read: AdmissionBatchReadV2,
    pub authorization: AdmissionBatchAuthorizationV2,
}

/// The owner must supply this from separately authenticated Noise endpoints,
/// independently verified issuer policy/time and protected current view.
#[derive(Clone, Debug)]
pub struct AdmissionInputBoundContextV2 {
    pub trusted_issuer_public_key: String,
    pub scope: BftScopeV2,
    pub schedule_digest: String,
    pub view_generation: u64,
    pub grant_epoch: u64,
    pub protected_earliest_unix_ms: u64,
    pub protected_latest_unix_ms: u64,
    pub authenticated_requester_peer_id: String,
    pub authenticated_serving_peer_id: String,
}

/// Not deserializable: this is a verified original request/subject, *not* fresh
/// issuer revocation, source ownership, responder budget or material grant.
#[derive(Clone)]
pub struct VerifiedAdmissionBatchSubjectV2 {
    read: AdmissionBatchReadV2,
    grant: BftPrivateRetentionAdmissionReadGrantV1,
    subject_public_key: String,
}

impl VerifiedAdmissionBatchSubjectV2 {
    pub fn read(&self) -> &AdmissionBatchReadV2 {
        &self.read
    }

    pub fn grant(&self) -> &BftPrivateRetentionAdmissionReadGrantV1 {
        &self.grant
    }

    pub fn subject_public_key(&self) -> &str {
        &self.subject_public_key
    }

    pub fn authorizes_part(&self, index: u32) -> bool {
        self.read
            .ranges()
            .is_ok_and(|ranges| ranges.iter().any(|part| part.index == index))
    }
}

impl AdmissionBatchRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != DOMAIN_V2 {
            return Err("original admission request domain invalid".into());
        }
        self.read.bind_grant(&self.authorization.grant)?;
        hash(&self.authorization.subject_public_key)?;
        signature_shape(&self.authorization.signature_hex)?;
        if self.authorization.subject_public_key != self.authorization.grant.body.subject_public_key
        {
            return Err("original admission subject differs from dedicated grant".into());
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode_frame(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let request: Self = decode_frame(bytes)?;
        request.validate()?;
        Ok(request)
    }

    pub fn original_attempt_digest(&self) -> Result<String, String> {
        Ok(digest(
            b"oasis7.bft.admission-input.original.v2",
            &encode(&(
                &self.read.reference,
                &self.read.requester_peer_id,
                &self.read.serving_peer_id,
                &self.read.nonce,
            ))?,
        ))
    }

    pub fn request_digest(&self) -> Result<String, String> {
        Ok(digest(
            b"oasis7.bft.admission-input.request.v2",
            &self.encode()?,
        ))
    }

    /// The gate must also independently attest current issuer/fresh witness
    /// and source possession. Merely holding this typed proof grants no read.
    pub fn verify_bound_subject(
        &self,
        context: &AdmissionInputBoundContextV2,
    ) -> Result<VerifiedAdmissionBatchSubjectV2, String> {
        self.validate()?;
        self.authorization
            .grant
            .verify_signature(&context.trusted_issuer_public_key)
            .map_err(|_| "original admission issuer signature rejected")?;
        verify_signature(
            &self.authorization.subject_public_key,
            &self.read.signing_bytes(&self.authorization.grant)?,
            &self.authorization.signature_hex,
        )?;
        let body = &self.authorization.grant.body;
        if body.scope != context.scope
            || body.schedule_digest != context.schedule_digest
            || body.view_generation != context.view_generation
            || body.grant_epoch != context.grant_epoch
            || self.read.requester_peer_id != context.authenticated_requester_peer_id
            || self.read.serving_peer_id != context.authenticated_serving_peer_id
            || context.protected_earliest_unix_ms > context.protected_latest_unix_ms
            || context.protected_earliest_unix_ms < body.not_before_unix_ms
            || context.protected_latest_unix_ms >= body.expires_unix_ms
        {
            return Err("original admission scope/view/time/Noise binding invalid".into());
        }
        Ok(VerifiedAdmissionBatchSubjectV2 {
            read: self.read.clone(),
            grant: self.authorization.grant.clone(),
            subject_public_key: self.authorization.subject_public_key.clone(),
        })
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use oasis7_distfs::protected_bft_v3::{
        BftPrivateRetentionAdmissionReadGrantBodyV1, BftPrivateRetentionAdmissionReadPurposeV1,
        BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1, BftSignerErrorV3,
        BftSigningKeyHandleV3, BftSuccessorWindowArchiveRefV1,
    };

    struct TestIssuer(SigningKey);
    impl BftSigningKeyHandleV3 for TestIssuer {
        fn public_key_hex(&self) -> String {
            hex::encode(self.0.verifying_key().to_bytes())
        }
        fn sign_preimage(&self, payload: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
            Ok(self.0.sign(payload).to_bytes())
        }
    }

    fn fixture(
        operation: AdmissionBatchOperationV2,
    ) -> (
        AdmissionBatchRequestV2,
        AdmissionInputBoundContextV2,
        SigningKey,
    ) {
        fixture_with_original(operation, 4 * u64::from(CHUNK) + 7, "66".repeat(32))
    }

    /// Shared signed fixture for the distinct original response codec tests.
    pub(super) fn fixture_with_original(
        operation: AdmissionBatchOperationV2,
        original_bytes: u64,
        original_digest: String,
    ) -> (
        AdmissionBatchRequestV2,
        AdmissionInputBoundContextV2,
        SigningKey,
    ) {
        let subject = SigningKey::from_bytes(&[32; 32]);
        let issuer = TestIssuer(SigningKey::from_bytes(&[71; 32]));
        let input = InputRef {
            source: BftPrivateRetentionSourceV1 {
                binding_digest: "11".repeat(32),
                handoff_digest: "22".repeat(32),
                material_source_digest: "33".repeat(32),
            },
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: "44".repeat(32),
                height: 32,
                header_id: "55".repeat(32),
            },
            object: BftSuccessorWindowArchiveRefV1 {
                digest: original_digest,
                bytes: original_bytes,
            },
            admission_digest: "77".repeat(32),
        };
        let read = AdmissionBatchReadV2 {
            reference: input.clone(),
            requester_peer_id: libp2p::PeerId::random().to_string(),
            serving_peer_id: libp2p::PeerId::random().to_string(),
            nonce: "99".repeat(32),
            operation,
        };
        let scope = BftScopeV2 {
            world_id: "wire-original".into(),
            chain_id: "admission-chain".into(),
            genesis_hash: "aa".repeat(32),
        };
        let subject_public_key = hex::encode(subject.verifying_key().to_bytes());
        let grant = BftPrivateRetentionAdmissionReadGrantV1::sign(
            BftPrivateRetentionAdmissionReadGrantBodyV1 {
                schema_version: 1,
                purpose: BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
                scope: scope.clone(),
                reference: input,
                subject_public_key: subject_public_key.clone(),
                requester_peer_id: read.requester_peer_id.clone(),
                serving_peer_id: read.serving_peer_id.clone(),
                schedule_digest: "bb".repeat(32),
                view_generation: 2,
                request_nonce: read.nonce.clone(),
                grant_epoch: 3,
                not_before_unix_ms: 1_000,
                expires_unix_ms: 12_000,
            },
            &issuer,
        )
        .unwrap();
        let signature_hex = hex::encode(
            subject
                .sign(&read.signing_bytes(&grant).unwrap())
                .to_bytes(),
        );
        let context = AdmissionInputBoundContextV2 {
            trusted_issuer_public_key: issuer.public_key_hex(),
            scope,
            schedule_digest: "bb".repeat(32),
            view_generation: 2,
            grant_epoch: 3,
            protected_earliest_unix_ms: 2_000,
            protected_latest_unix_ms: 2_100,
            authenticated_requester_peer_id: read.requester_peer_id.clone(),
            authenticated_serving_peer_id: read.serving_peer_id.clone(),
        };
        (
            AdmissionBatchRequestV2 {
                domain: DOMAIN_V2.into(),
                read,
                authorization: AdmissionBatchAuthorizationV2 {
                    grant,
                    subject_public_key,
                    signature_hex,
                },
            },
            context,
            subject,
        )
    }

    #[test]
    fn manifest_and_batch_request_verified_only_for_exact_original_context() {
        for operation in [
            AdmissionBatchOperationV2::Manifest,
            AdmissionBatchOperationV2::Batch {
                offset_bytes: 0,
                part_count: 3,
            },
            AdmissionBatchOperationV2::Batch {
                offset_bytes: 3 * u64::from(CHUNK),
                part_count: 2,
            },
        ] {
            let (request, context, _) = fixture(operation);
            assert_eq!(
                AdmissionBatchRequestV2::decode(&request.encode().unwrap()).unwrap(),
                request
            );
            let proof = request.verify_bound_subject(&context).unwrap();
            assert_eq!(proof.read(), &request.read);
            assert_eq!(proof.grant(), &request.authorization.grant);
            assert_eq!(
                proof.subject_public_key(),
                request.authorization.subject_public_key
            );
            if matches!(operation, AdmissionBatchOperationV2::Manifest) {
                assert!(!proof.authorizes_part(0));
            } else {
                let indexes = request.read.ranges().unwrap();
                for part in &indexes {
                    assert!(proof.authorizes_part(part.index));
                }
                assert!(!proof.authorizes_part(500));
            }
        }
    }

    #[test]
    fn original_subject_signature_binds_operation_phase_and_original_bytes() {
        let (request, context, _) = fixture(AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        });
        for operation in [
            AdmissionBatchOperationV2::Manifest,
            AdmissionBatchOperationV2::Batch {
                offset_bytes: u64::from(CHUNK),
                part_count: 3,
            },
            AdmissionBatchOperationV2::Batch {
                offset_bytes: 0,
                part_count: 2,
            },
        ] {
            let mut changed = request.clone();
            changed.read.operation = operation;
            assert!(changed.verify_bound_subject(&context).is_err());
        }
        let mut changed = request.clone();
        changed.read.reference.key.height += 1;
        assert!(changed.verify_bound_subject(&context).is_err());
        let mut changed = request.clone();
        changed.read.nonce = "ab".repeat(32);
        assert!(changed.verify_bound_subject(&context).is_err());
        let mut changed = request.clone();
        std::mem::swap(
            &mut changed.read.requester_peer_id,
            &mut changed.read.serving_peer_id,
        );
        assert!(changed.verify_bound_subject(&context).is_err());
        let mut changed = request.clone();
        changed.authorization.subject_public_key =
            hex::encode(SigningKey::from_bytes(&[33; 32]).verifying_key().to_bytes());
        assert!(changed.verify_bound_subject(&context).is_err());
        let mut changed = request.clone();
        changed.domain = "oasis7.bft.private-candidate.exchange.v2".into();
        assert!(changed.verify_bound_subject(&context).is_err());
    }

    #[test]
    fn original_issuer_view_noise_and_time_are_independent_obligations() {
        let (request, context, _) = fixture(AdmissionBatchOperationV2::Manifest);
        let mut foreign = context.clone();
        foreign.trusted_issuer_public_key = "ff".repeat(32);
        assert!(request.verify_bound_subject(&foreign).is_err());
        let mut foreign = context.clone();
        foreign.scope.world_id = "other".into();
        assert!(request.verify_bound_subject(&foreign).is_err());
        let mut foreign = context.clone();
        foreign.grant_epoch += 1;
        assert!(request.verify_bound_subject(&foreign).is_err());
        let mut foreign = context.clone();
        foreign.view_generation += 1;
        assert!(request.verify_bound_subject(&foreign).is_err());
        let mut foreign = context.clone();
        foreign.authenticated_serving_peer_id = libp2p::PeerId::random().to_string();
        assert!(request.verify_bound_subject(&foreign).is_err());
        let mut foreign = context.clone();
        foreign.protected_latest_unix_ms = 12_000;
        assert!(request.verify_bound_subject(&foreign).is_err());
        let mut foreign = context.clone();
        foreign.protected_earliest_unix_ms = 999;
        assert!(request.verify_bound_subject(&foreign).is_err());
    }

    #[test]
    fn rejects_noncanonical_wrong_phase_or_range_and_old_domain() {
        let (request, context, _) = fixture(AdmissionBatchOperationV2::Manifest);
        let bytes = request.encode().unwrap();
        let mut noncanonical = vec![0xb8, 3];
        if bytes.first() == Some(&0xa3) {
            noncanonical.extend_from_slice(&bytes[1..]);
            assert!(AdmissionBatchRequestV2::decode(&noncanonical).is_err());
        }
        let mut invalid = request.clone();
        invalid.read.operation = AdmissionBatchOperationV2::Batch {
            offset_bytes: 1,
            part_count: 2,
        };
        assert!(invalid.validate().is_err());
        invalid.read.operation = AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 4,
        };
        assert!(invalid.validate().is_err());
        invalid.read.operation = AdmissionBatchOperationV2::Batch {
            offset_bytes: 5 * u64::from(CHUNK),
            part_count: 1,
        };
        assert!(invalid.validate().is_err());
        let mut old = request.clone();
        old.domain = "oasis7.bft.admission-input.exchange.v1".into();
        assert!(old.validate().is_err());
        assert!(AdmissionBatchRequestV2::decode(&vec![0xff; MAX_FRAME + 1]).is_err());
        assert!(request.verify_bound_subject(&context).is_ok());
    }
}
