//! Manual qualification against an isolated Luna partition and a non-production key.
//! Supply the original framed bytes from an isolated run with
//! OASIS7_LUNA_TEST_PREIMAGE_FILE. Use OASIS7_LUNA_TEST_PREIMAGE_BYTES for a
//! separate synthetic multipart length check.
use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use cryptoki::session::UserType;
use cryptoki::types::AuthPin;
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_distfs::protected_bft_v3::{BftSigningKeyHandleV3, LunaBftSigningKeyV3};
use std::{fs::File, io::Read};

fn main() {
    let module = std::env::var_os("OASIS7_LUNA_TEST_MODULE").expect("qualified PKCS#11 module");
    let serial = std::env::var("OASIS7_LUNA_TEST_SERIAL").expect("qualified token serial");
    let key_id = hex::decode(std::env::var("OASIS7_LUNA_TEST_KEY_ID").expect("test key ID"))
        .expect("hex test key ID");
    let public_key = std::env::var("OASIS7_LUNA_TEST_PUBLIC_KEY").expect("test key public key");
    let pin_path = std::env::var_os("OASIS7_LUNA_TEST_PIN_FILE").expect("private PIN file");
    let (mut original, input_kind) = match (
        std::env::var_os("OASIS7_LUNA_TEST_PREIMAGE_FILE"),
        std::env::var_os("OASIS7_LUNA_TEST_PREIMAGE_BYTES"),
    ) {
        (Some(path), None) => {
            let mut bytes = Vec::new();
            File::open(path)
                .expect("open original framed signing bytes")
                .take(LunaBftSigningKeyV3::MAX_PREIMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .expect("read original framed signing bytes");
            (bytes, "original framed")
        }
        (None, Some(length)) => {
            let length = length
                .to_str()
                .expect("UTF-8 qualification size")
                .parse::<usize>()
                .expect("valid qualification size");
            assert!((4097..=LunaBftSigningKeyV3::MAX_PREIMAGE_BYTES).contains(&length));
            (vec![0xA5; length], "synthetic")
        }
        _ => panic!("set exactly one Luna preimage file or synthetic size"),
    };
    let preimage_len = original.len();
    assert!((4097..=LunaBftSigningKeyV3::MAX_PREIMAGE_BYTES).contains(&preimage_len));

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
    let signature = Signature::from_bytes(&key.sign_preimage(&original).unwrap());
    let raw_key = hex::decode(public_key).expect("hex test key public key");
    let verifying_key = VerifyingKey::from_bytes(&raw_key.try_into().expect("32-byte public key"))
        .expect("valid Ed25519 public key");
    assert!(verifying_key.verify_strict(&original, &signature).is_ok());
    original[preimage_len - 1] ^= 1;
    assert!(verifying_key.verify_strict(&original, &signature).is_err());
    println!("qualified PureEdDSA multipart signing for {preimage_len} {input_kind} bytes");
}
