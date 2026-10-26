//! AES-GCM envelope primitives. No production HTTP or RPC caller is installed.
use js_sys::{Array, Reflect, Uint8Array};
use oasis7_custody_core::DecimalU64;
use oasis7_custody_core::envelope::{EnvelopeContextV1, KeyEnvelopeV1, MAX_ENVELOPE_VALUE_BYTES};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AesGcmParams, Crypto, CryptoKey};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvelopeRejected;

/// Secret output deliberately has no Debug, Clone, or serialization implementation.
pub struct SecretBytes(Zeroizing<Vec<u8>>);
impl SecretBytes {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// Explicitly erase our JS raw-key copy after import, including rejected imports.
struct RawKey(Uint8Array);
impl Drop for RawKey {
    fn drop(&mut self) {
        self.0.fill(0, 0, self.0.length());
    }
}

fn crypto() -> Result<Crypto, EnvelopeRejected> {
    Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| EnvelopeRejected)?
        .dyn_into()
        .map_err(|_| EnvelopeRejected)
}
fn random<const N: usize>() -> Result<[u8; N], EnvelopeRejected> {
    let mut bytes = [0; N];
    crypto()?
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|_| EnvelopeRejected)?;
    Ok(bytes)
}
fn distinct_nonce(value: &[u8; 12]) -> Result<[u8; 12], EnvelopeRejected> {
    for _ in 0..4 {
        let nonce = random::<12>()?;
        if nonce != *value {
            return Ok(nonce);
        }
    }
    Err(EnvelopeRejected)
}
async fn aes(
    key: &[u8; 32],
    nonce: &[u8],
    aad: &[u8],
    data: &[u8],
    encrypt: bool,
) -> Result<Zeroizing<Vec<u8>>, EnvelopeRejected> {
    let subtle = crypto()?.subtle();
    let raw = RawKey(Uint8Array::from(key.as_slice()));
    let usages = Array::new();
    usages.push(&JsValue::from_str(if encrypt {
        "encrypt"
    } else {
        "decrypt"
    }));
    let imported = subtle
        .import_key_with_str("raw", raw.0.as_ref(), "AES-GCM", false, usages.as_ref())
        .map_err(|_| EnvelopeRejected)?;
    let imported: CryptoKey = JsFuture::from(imported)
        .await
        .map_err(|_| EnvelopeRejected)?
        .dyn_into()
        .map_err(|_| EnvelopeRejected)?;
    drop(raw);
    let iv = Uint8Array::from(nonce);
    let params = AesGcmParams::new("AES-GCM", iv.as_ref());
    params.set_additional_data_u8_array(&Uint8Array::from(aad));
    params.set_tag_length(128);
    let result = if encrypt {
        subtle.encrypt_with_object_and_u8_array(params.as_ref(), &imported, data)
    } else {
        subtle.decrypt_with_object_and_u8_array(params.as_ref(), &imported, data)
    }
    .map_err(|_| EnvelopeRejected)?;
    let result = JsFuture::from(result).await.map_err(|_| EnvelopeRejected)?;
    let result = Uint8Array::new(&result);
    let bytes = Zeroizing::new(result.to_vec());
    result.fill(0, 0, result.length());
    Ok(bytes)
}

pub async fn seal(
    context: &EnvelopeContextV1,
    wrapping_key_version: DecimalU64,
    kek: &[u8; 32],
    plaintext: &[u8],
) -> Result<KeyEnvelopeV1, EnvelopeRejected> {
    context.validate().map_err(|_| EnvelopeRejected)?;
    if wrapping_key_version.0 == 0
        || plaintext.is_empty()
        || plaintext.len() > MAX_ENVELOPE_VALUE_BYTES
    {
        return Err(EnvelopeRejected);
    }
    let dek = Zeroizing::new(random::<32>()?);
    let value_nonce = random::<12>()?;
    let wrap_nonce = distinct_nonce(&value_nonce)?;
    let ciphertext = aes(
        &dek,
        &value_nonce,
        &context.value_aad().map_err(|_| EnvelopeRejected)?,
        plaintext,
        true,
    )
    .await?;
    let wrapped_dek = aes(
        kek,
        &wrap_nonce,
        &context
            .wrap_aad(wrapping_key_version)
            .map_err(|_| EnvelopeRejected)?,
        dek.as_slice(),
        true,
    )
    .await?;
    let envelope = KeyEnvelopeV1 {
        context: context.clone(),
        ciphertext: hex::encode(&*ciphertext),
        value_nonce: hex::encode(value_nonce),
        wrapped_dek: hex::encode(&*wrapped_dek),
        wrap_nonce: hex::encode(wrap_nonce),
        wrapping_key_version,
    };
    envelope
        .validate_for(context)
        .map_err(|_| EnvelopeRejected)?;
    Ok(envelope)
}

async fn unwrap_dek(
    envelope: &KeyEnvelopeV1,
    expected: &EnvelopeContextV1,
    kek: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, EnvelopeRejected> {
    envelope
        .validate_for(expected)
        .map_err(|_| EnvelopeRejected)?;
    let bytes = aes(
        kek,
        &hex::decode(&envelope.wrap_nonce).map_err(|_| EnvelopeRejected)?,
        &envelope.wrap_aad().map_err(|_| EnvelopeRejected)?,
        &hex::decode(&envelope.wrapped_dek).map_err(|_| EnvelopeRejected)?,
        false,
    )
    .await?;
    let mut dek = Zeroizing::new([0; 32]);
    if bytes.len() != dek.len() {
        return Err(EnvelopeRejected);
    }
    dek.copy_from_slice(&bytes);
    Ok(dek)
}

pub async fn open(
    envelope: &KeyEnvelopeV1,
    expected: &EnvelopeContextV1,
    kek: &[u8; 32],
) -> Result<SecretBytes, EnvelopeRejected> {
    let dek = unwrap_dek(envelope, expected, kek).await?;
    let plaintext = aes(
        &dek,
        &hex::decode(&envelope.value_nonce).map_err(|_| EnvelopeRejected)?,
        &envelope.value_aad().map_err(|_| EnvelopeRejected)?,
        &hex::decode(&envelope.ciphertext).map_err(|_| EnvelopeRejected)?,
        false,
    )
    .await?;
    if plaintext.is_empty() || plaintext.len() > MAX_ENVELOPE_VALUE_BYTES {
        return Err(EnvelopeRejected);
    }
    Ok(SecretBytes(plaintext))
}

pub async fn rewrap(
    envelope: &KeyEnvelopeV1,
    expected: &EnvelopeContextV1,
    old_kek: &[u8; 32],
    new_wrapping_key_version: DecimalU64,
    new_kek: &[u8; 32],
) -> Result<KeyEnvelopeV1, EnvelopeRejected> {
    if new_wrapping_key_version.0 <= envelope.wrapping_key_version.0 {
        return Err(EnvelopeRejected);
    }
    let dek = unwrap_dek(envelope, expected, old_kek).await?;
    // Authenticate the unchanged value as well: never bless a corrupt stored value on rotation.
    let _verified_value = open(envelope, expected, old_kek).await?;
    let value_nonce: [u8; 12] = hex::decode(&envelope.value_nonce)
        .map_err(|_| EnvelopeRejected)?
        .try_into()
        .map_err(|_| EnvelopeRejected)?;
    let nonce = distinct_nonce(&value_nonce)?;
    let wrapped = aes(
        new_kek,
        &nonce,
        &expected
            .wrap_aad(new_wrapping_key_version)
            .map_err(|_| EnvelopeRejected)?,
        dek.as_slice(),
        true,
    )
    .await?;
    let mut result = envelope.clone();
    result.wrapped_dek = hex::encode(&*wrapped);
    result.wrap_nonce = hex::encode(nonce);
    result.wrapping_key_version = new_wrapping_key_version;
    result
        .validate_for(expected)
        .map_err(|_| EnvelopeRejected)?;
    Ok(result)
}
