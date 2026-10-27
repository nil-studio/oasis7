//! Private fixed-origin custody transport. This does not establish host isolation
//! or grant a model access to a signing key, capability or arbitrary HTTP target.
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::http_pop::{
    HttpPopSigningInput, MAX_HTTP_BODY_BYTES, build_http_pop_signing_material,
};
use reqwest::{blocking::Client, redirect::Policy};
use std::{io::Read, time::Duration};
use zeroize::Zeroizing;

const MAX_PRIVATE_REPLY_BYTES: usize = 4 * 1024 * 1024 + 64 * 1024;

pub(crate) struct CustodyTransport {
    origin: String,
    client: Client,
}
pub(crate) struct PrivateTransportReply {
    pub status: u16,
    body: Zeroizing<Vec<u8>>,
}
impl PrivateTransportReply {
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}
impl core::fmt::Debug for PrivateTransportReply {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PrivateTransportReply")
            .field("status", &self.status)
            .field("body", &"<private>")
            .finish()
    }
}
impl CustodyTransport {
    pub(crate) fn new(origin: &str) -> Result<Self, String> {
        // Validate the same origin rules used to construct every signed target.
        oasis7_custody_core::http_pop::CanonicalHttpTarget::new(
            origin,
            oasis7_custody_core::http_pop::HttpPopRoute::ProofChallenges,
        )
        .map_err(|_| "invalid fixed custody origin")?;
        let client = Client::builder()
            .https_only(true)
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "custody HTTPS transport unavailable")?;
        Ok(Self {
            origin: origin.into(),
            client,
        })
    }

    /// The trusted host must persist the exact typed business original and ID
    /// before calling this method. An error is unknown; this method never retries.
    pub(crate) fn send_once(
        &self,
        input: &HttpPopSigningInput<'_>,
        key: &SigningKey,
    ) -> Result<PrivateTransportReply, String> {
        if input.target.origin() != self.origin || input.body.len() > MAX_HTTP_BODY_BYTES {
            return Err("custody request does not match fixed transport".into());
        }
        let material = build_http_pop_signing_material(input)
            .map_err(|_| "invalid fixed custody proof request")?;
        let signature = key.sign(material.signature_base()).to_bytes();
        let mut request = match input.target.method() {
            "POST" => self.client.post(input.target.uri()),
            "GET" if input.body.is_empty() => self.client.get(input.target.uri()),
            _ => return Err("custody method unavailable".into()),
        };
        for header in material.signed_headers(&signature) {
            request = request.header(&header.name, &header.value);
        }
        if input.target.method() == "POST" {
            request = request.body(input.body.to_vec());
        }
        let response = request.send().map_err(|_| "custody outcome unknown")?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|n| n > MAX_PRIVATE_REPLY_BYTES as u64)
        {
            return Err("context_too_large; custody outcome unknown".into());
        }
        let mut body = Zeroizing::new(Vec::new());
        response
            .take(MAX_PRIVATE_REPLY_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| "custody reply incomplete; outcome unknown")?;
        if body.len() > MAX_PRIVATE_REPLY_BYTES {
            return Err("context_too_large; custody outcome unknown".into());
        }
        Ok(PrivateTransportReply { status, body })
    }
}
