//! Optional Luna CKM_EDDSA key adapter. Session provisioning and fencing
//! belong to the protected backend that owns this handle.
use std::sync::Mutex;

use cryptoki::mechanism::{
    Mechanism,
    eddsa::{EddsaParams, EddsaSignatureScheme},
};
use cryptoki::object::{Attribute, AttributeType, KeyType, ObjectClass, ObjectHandle};
use cryptoki::session::Session;
use ed25519_dalek::{Signature, VerifyingKey};

use super::binding::{BftSignerErrorV3, BftSigningKeyHandleV3, decode_key, invalid};
use super::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3;

const UPDATE_BYTES: usize = 4096;
const KEY_BINDING_PROBE: &[u8] = b"oasis7.bft.protected-signer.v3.luna-key-binding-probe";

/// An existing HSM-generated, non-extractable Ed25519 key. Opening consumes an
/// authenticated session to the externally qualified Luna partition.
pub struct LunaBftSigningKeyV3 {
    session: Mutex<Option<Session>>,
    key: ObjectHandle,
    public_key: VerifyingKey,
}

impl LunaBftSigningKeyV3 {
    pub const MAX_PREIMAGE_BYTES: usize = BFT_SIGNER_MAX_SIGNING_BYTES_V3;

    pub fn open(
        session: Session,
        key_id: &[u8],
        expected_public_key_hex: &str,
    ) -> Result<Self, BftSignerErrorV3> {
        if key_id.is_empty() {
            return Err(invalid("Luna signing key ID must be provisioned"));
        }
        let public_key = decode_key(expected_public_key_hex)?;
        let mut matches = session
            .iter_objects(&[
                Attribute::Class(ObjectClass::PRIVATE_KEY),
                Attribute::KeyType(KeyType::EC_EDWARDS),
                Attribute::Id(key_id.to_vec()),
            ])
            .map_err(|_| invalid("Luna signing key lookup failed"))?;
        let key = matches
            .next()
            .transpose()
            .map_err(|_| invalid("Luna signing key lookup failed"))?
            .ok_or_else(|| invalid("Luna signing key is not provisioned"))?;
        if matches.next().is_some() {
            return Err(invalid("Luna signing key ID is ambiguous"));
        }
        drop(matches);

        let required = [
            (AttributeType::Token, Attribute::Token(true)),
            (AttributeType::Local, Attribute::Local(true)),
            (AttributeType::Private, Attribute::Private(true)),
            (AttributeType::Sensitive, Attribute::Sensitive(true)),
            (
                AttributeType::AlwaysSensitive,
                Attribute::AlwaysSensitive(true),
            ),
            (AttributeType::Extractable, Attribute::Extractable(false)),
            (
                AttributeType::NeverExtractable,
                Attribute::NeverExtractable(true),
            ),
            (AttributeType::Sign, Attribute::Sign(true)),
        ];
        let types = required.each_ref().map(|(ty, _)| *ty);
        let values = session
            .get_attributes(key, &types)
            .map_err(|_| invalid("Luna signing key attributes unavailable"))?;
        if !required.iter().all(|(_, value)| values.contains(value)) {
            return Err(invalid("Luna signing key protection attributes differ"));
        }

        let handle = Self {
            session: Mutex::new(Some(session)),
            key,
            public_key,
        };
        handle.sign_preimage(KEY_BINDING_PROBE)?;
        Ok(handle)
    }

    fn sign(&self, session: &Session, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        session
            .sign_init(
                &Mechanism::Eddsa(EddsaParams::new(EddsaSignatureScheme::Ed25519)),
                self.key,
            )
            .map_err(|_| invalid("Luna PureEdDSA sign initialization failed"))?;
        for bytes in preimage.chunks(UPDATE_BYTES) {
            session
                .sign_update(bytes)
                .map_err(|_| invalid("Luna PureEdDSA original-byte update failed"))?;
        }
        let bytes: [u8; 64] = session
            .sign_final()
            .map_err(|_| invalid("Luna PureEdDSA sign final failed"))?
            .try_into()
            .map_err(|_| invalid("Luna Ed25519 signature has incorrect length"))?;
        self.public_key
            .verify_strict(preimage, &Signature::from_bytes(&bytes))
            .map_err(|_| {
                invalid("Luna signature differs from provisioned key or original bytes")
            })?;
        Ok(bytes)
    }
}

impl BftSigningKeyHandleV3 for LunaBftSigningKeyV3 {
    fn public_key_hex(&self) -> String {
        hex::encode(self.public_key.to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        if preimage.is_empty() || preimage.len() > Self::MAX_PREIMAGE_BYTES {
            return Err(invalid("Luna Ed25519 preimage exceeds its signing bounds"));
        }
        let mut handle = self
            .session
            .lock()
            .map_err(|_| invalid("Luna signing session is poisoned"))?;
        let session = handle
            .as_ref()
            .ok_or_else(|| invalid("Luna signing session requires protected reopen"))?;
        let result = self.sign(session, preimage);
        if result.is_err() {
            // A partial multipart or lost device response leaves session state
            // ambiguous. A new externally fenced open is required.
            handle.take();
        }
        result
    }
}
