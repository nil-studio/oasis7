//! Private fixed-origin custody transport. This does not establish host isolation
//! or grant a model access to a signing key, capability or arbitrary HTTP target.
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::http_pop::{
    HttpPopSigningInput, MAX_HTTP_BODY_BYTES, build_http_pop_signing_material,
};
use reqwest::{blocking::Client, redirect::Policy};
use std::{io::Read, time::Duration};
use zeroize::Zeroizing;

const MAX_PRIVATE_REPLY_BYTES: usize = 64 * 1024;

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
            let mut value = reqwest::header::HeaderValue::from_str(&header.value)
                .map_err(|_| "custody proof header unavailable")?;
            value.set_sensitive(true);
            request = request.header(&header.name, value);
        }
        if input.target.method() == "POST" {
            request = request.body(input.body.to_vec());
        }
        let request = request.build().map_err(|_| "custody request unavailable")?;
        if request.url().as_str() != input.target.uri() {
            return Err("custody target normalization rejected".into());
        }
        let response = self
            .client
            .execute(request)
            .map_err(|_| "custody outcome unknown")?;
        Self::read_reply(response)
    }
    /// The only unsigned request is the fixed, closed Enrollment metadata create.
    pub(crate) fn create_enrollment_once(
        &self,
        request: &oasis7_custody_core::client_enrollment::ClientEnrollmentCreateRequestV1,
    ) -> Result<PrivateTransportReply, String> {
        request
            .validate()
            .map_err(|_| "invalid custody enrollment metadata")?;
        let body =
            serde_json::to_vec(request).map_err(|_| "invalid custody enrollment metadata")?;
        if body.len() > MAX_HTTP_BODY_BYTES {
            return Err("custody enrollment metadata exceeds bound".into());
        }
        let response = self
            .client
            .post(format!("{}/v1/client-enrollments", self.origin))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .map_err(|_| "custody enrollment outcome unknown")?;
        Self::read_reply(response)
    }
    fn read_reply(response: reqwest::blocking::Response) -> Result<PrivateTransportReply, String> {
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
