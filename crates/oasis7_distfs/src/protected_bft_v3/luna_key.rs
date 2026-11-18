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
        if preimage.is_empty() || preimage.len() > BFT_SIGNER_MAX_SIGNING_BYTES_V3 {
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

#[cfg(test)]
mod qualification {
    use super::*;
    use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
    use cryptoki::session::UserType;
    use cryptoki::types::AuthPin;

    #[test]
    #[ignore = "requires an isolated Luna partition and provisioned qualification key"]
    fn luna_signs_original_ed25519_bytes_with_nonextractable_key() {
        let module = std::env::var_os("OASIS7_LUNA_TEST_MODULE").expect("qualified PKCS#11 module");
        let serial = std::env::var("OASIS7_LUNA_TEST_SERIAL").expect("qualified token serial");
        let key_id = hex::decode(std::env::var("OASIS7_LUNA_TEST_KEY_ID").expect("test key ID"))
            .expect("hex test key ID");
        let public_key = std::env::var("OASIS7_LUNA_TEST_PUBLIC_KEY").expect("test key public key");
        let pin_path = std::env::var_os("OASIS7_LUNA_TEST_PIN_FILE").expect("private PIN file");
        let preimage_len: usize = std::env::var("OASIS7_LUNA_TEST_PREIMAGE_BYTES")
            .expect("qualified original-byte signing size")
            .parse()
            .expect("valid qualification size");
        assert!((UPDATE_BYTES + 1..=BFT_SIGNER_MAX_SIGNING_BYTES_V3).contains(&preimage_len));

        let library = Pkcs11::new(module).expect("load qualified PKCS#11 module");
        library
            .initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK))
            .expect("initialize Luna module");
        let eligible: Vec<_> = library
            .get_slots_with_token()
            .expect("enumerate tokens")
            .into_iter()
            .filter(|slot| {
                library
                    .get_token_info(*slot)
                    .is_ok_and(|token| token.serial_number().trim() == serial)
            })
            .collect();
        let [slot] = eligible.as_slice() else {
            panic!("expected one provisioned Luna token with the configured serial");
        };
        let session = library.open_ro_session(*slot).expect("open Luna session");
        let mut pin = std::fs::read_to_string(pin_path).expect("read private PIN file");
        pin.truncate(pin.trim_end_matches(['\r', '\n']).len());
        let pin = AuthPin::new(pin.into_boxed_str());
        session
            .login(UserType::User, Some(&pin))
            .expect("authenticate isolated Luna session");
        drop(pin);

        let key = LunaBftSigningKeyV3::open(session, &key_id, &public_key)
            .expect("bind hardware key against provisioned Ed25519 public key");
        let mut original = vec![0xA5; preimage_len];
        let signature = Signature::from_bytes(&key.sign_preimage(&original).unwrap());
        let verifying_key = decode_key(&public_key).unwrap();
        assert!(verifying_key.verify_strict(&original, &signature).is_ok());
        original[preimage_len - 1] ^= 1;
        assert!(verifying_key.verify_strict(&original, &signature).is_err());
    }
}
