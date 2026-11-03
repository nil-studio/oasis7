//! New-purpose grant issuance. Neither requests nor hints establish permission.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionReadGrantBodyV1;
use serde::{Deserialize, Serialize};
pub(super) const PATH: &str = "/oasis7/bft/original-admission-input/grant/v1";
pub(super) const DOMAIN: &str = "oasis7.bft.original-admission-input.grant-request.v1";
pub(super) const MAX_FRAME: usize = 64 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrantRequest {
    pub domain: String,
    pub schema_version: u32,
    pub body: BftPrivateRetentionAdmissionReadGrantBodyV1,
    pub subject_signature_hex: String,
}
impl GrantRequest {
    fn preimage(&self) -> Result<Vec<u8>, String> {
        let mut bytes = DOMAIN.as_bytes().to_vec();
        bytes.push(0);
        bytes.extend(
            serde_cbor::to_vec(&(self.schema_version, &self.body))
                .map_err(|_| "admission issuance encoding rejected")?,
        );
        if bytes.len() > MAX_FRAME {
            return Err("admission issuance frame capacity".into());
        }
        Ok(bytes)
    }
    pub fn sign(
        body: BftPrivateRetentionAdmissionReadGrantBodyV1,
        subject: &SigningKey,
    ) -> Result<Self, String> {
        body.validate()
            .map_err(|_| "admission issuance body rejected")?;
        if body.subject_public_key != hex::encode(subject.verifying_key().to_bytes()) {
            return Err("admission issuance subject differs".into());
        }
        let mut request = Self {
            domain: DOMAIN.into(),
            schema_version: 1,
            body,
            subject_signature_hex: String::new(),
        };
        request.subject_signature_hex = hex::encode(subject.sign(&request.preimage()?).to_bytes());
        Ok(request)
    }
    pub fn verify_subject(&self) -> Result<(), String> {
        if self.domain != DOMAIN || self.schema_version != 1 {
            return Err("admission issuance domain/version rejected".into());
        }
        self.body
            .validate()
            .map_err(|_| "admission issuance body rejected")?;
        let key: [u8; 32] = hex::decode(&self.body.subject_public_key)
            .map_err(|_| "admission subject key encoding")?
            .try_into()
            .map_err(|_| "admission subject key length")?;
        if self.subject_signature_hex.len() != 128
            || hex::decode(&self.subject_signature_hex)
                .is_ok_and(|bytes| hex::encode(bytes) != self.subject_signature_hex)
        {
            return Err("admission issuance subject signature encoding".into());
        }
        let signature = Signature::from_slice(
            &hex::decode(&self.subject_signature_hex)
                .map_err(|_| "admission subject signature encoding")?,
        )
        .map_err(|_| "admission subject signature length")?;
        VerifyingKey::from_bytes(&key)
            .map_err(|_| "admission subject key rejected")?
            .verify_strict(&self.preimage()?, &signature)
            .map_err(|_| "admission issuance subject signature rejected".into())
    }
}
