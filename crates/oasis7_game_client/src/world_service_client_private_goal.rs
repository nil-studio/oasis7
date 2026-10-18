//! Confidential goal transport uses its own no-proxy/no-redirect client.
//! Plaintext loopback requires all local listening processes to be trusted: the
//! service response signature does not authenticate the peer before body delivery.
//! Use authenticated TLS transport for hosts with untrusted local users.
use super::*;
use oasis7_client_api::world_service::{
    private_goal::{PreparedGoalReceiptV1, SignedGoalPrepareV1},
    private_goal_read::*,
};
impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    #[cfg(test)]
    fn private_connection(&self) -> Result<reqwest::blocking::Client, WorldServiceClientError> {
        self.reader.transport.private_connection()
    }
    pub(super) fn private_scope(
        &self,
        world: &WorldIdentity,
        agent: &str,
    ) -> Result<(), WorldServiceClientError> {
        if world != &self.reader.config.expected_world
            || self.reader.config.scope_id != format!("agent:{agent}")
        {
            return Err(WorldServiceClientError::Configuration(
                "private goal world or agent differs from configuration".into(),
            ));
        }
        Ok(())
    }
    fn private_call<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &Q,
    ) -> Result<SignedServiceResponse<R>, WorldServiceClientError> {
        self.private_call_bounded(path, request, 64 * 1024)
    }
    pub(super) fn private_call_bounded<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &Q,
        maximum: usize,
    ) -> Result<SignedServiceResponse<R>, WorldServiceClientError> {
        self.reader
            .transport
            .private_signed_response(path, request, maximum)
    }
    /// Retain and verify the exact original proof. Does not sign or submit a public intent.
    pub fn prepare_goal(
        &self,
        original: &SignedGoalPrepareV1,
    ) -> Result<PreparedGoalReceiptV1, WorldServiceClientError> {
        original
            .verify()
            .map_err(|_| WorldServiceClientError::Assurance("invalid private goal proof".into()))?;
        self.private_scope(
            &WorldIdentity {
                world_id: original.request.world_id.clone(),
                genesis_digest: original.request.genesis_digest.clone(),
            },
            &original.request.agent_id,
        )?;
        let response = self.private_call(GOAL_PREPARE_PATH, original)?;
        verify_prepare_response(
            original,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(|_| {
            WorldServiceClientError::Assurance("private prepare response not verified".into())
        })?;
        Ok(response.payload)
    }
    /// Owner-authorized full goal subset. This does not provide full TaskSnapshot/Context.
    pub fn read_current_goal(
        &self,
        request: &SignedReadRequest<CurrentGoalRequest>,
    ) -> Result<CurrentGoalResponse, WorldServiceClientError> {
        verify_current_request(request)
            .map_err(|_| WorldServiceClientError::Assurance("invalid current goal proof".into()))?;
        self.private_scope(&request.request.world, &request.request.agent_id)?;
        let response = self.private_call(GOAL_CURRENT_PATH, request)?;
        verify_current_response(
            request,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(|_| {
            WorldServiceClientError::Assurance("current private goal response not verified".into())
        })?;
        Ok(response.payload)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    type RemoteWorldServiceClient = crate::world_service_client::RemoteWorldServiceClient<
        crate::world_service_test_profile::TestProfile,
    >;
    use std::io::Read;
    use std::time::Duration;
    #[test]
    fn private_transport_rejects_remote_plaintext_and_names_before_network() {
        let private = hex::encode([7; 32]);
        let key = authority::sign_read_request("fixture", (), &private)
            .unwrap()
            .subject_public_key;
        for endpoint in [
            "http://192.0.2.1",
            "http://localhost",
            "http://127.0.0.1.evil.invalid",
        ] {
            let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
                endpoint: endpoint.into(),
                trusted_service_public_key: key.clone(),
                expected_world: WorldIdentity {
                    world_id: "w".into(),
                    genesis_digest: "g".into(),
                },
                scope_id: "agent:a".into(),
                read_private_key_hex: private.clone(),
                timeout: Duration::from_millis(50),
                formal_read_trust: None,
                max_response_bytes: 65536,
            })
            .unwrap();
            assert!(matches!(
                client.private_connection(),
                Err(WorldServiceClientError::Configuration(_))
            ));
        }
    }
    fn config(endpoint: String, key: String, bound: usize) -> WorldServiceClientConfig {
        WorldServiceClientConfig {
            endpoint,
            trusted_service_public_key: key,
            expected_world: WorldIdentity {
                world_id: "w".into(),
                genesis_digest: "g".into(),
            },
            scope_id: "agent:a".into(),
            read_private_key_hex: hex::encode([7; 32]),
            timeout: Duration::from_secs(5),
            formal_read_trust: None,
            max_response_bytes: bound,
        }
    }
    #[test]
    fn private_trust_canonical_and_weak_keys_rejected_before_sending() {
        let key = authority::sign_read_request("fixture", (), &hex::encode([7; 32]))
            .unwrap()
            .subject_public_key;
        for bad in [key.to_uppercase(), hex::encode([0; 32])] {
            let client =
                RemoteWorldServiceClient::new(config("http://127.0.0.1:1".into(), bad, 65536))
                    .unwrap();
            assert!(matches!(
                client.private_connection(),
                Err(WorldServiceClientError::Configuration(_))
            ));
        }
    }
    #[test]
    fn private_response_size_cap_even_with_usize_max_configuration() {
        use std::io::Write;
        let key = authority::sign_read_request("fixture", (), &hex::encode([7; 32]))
            .unwrap()
            .subject_public_key;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = RemoteWorldServiceClient::new(config(
            format!("http://{}", listener.local_addr().unwrap()),
            key,
            usize::MAX,
        ))
        .unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut b = [0; 1];
                stream.read_exact(&mut b).unwrap();
                request.push(b[0]);
            }
            let header = std::str::from_utf8(&request).unwrap();
            let length = header
                .split("\r\n")
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse::<usize>()
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let body = vec![b'x'; 65537];
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });
        let result = client.private_call::<_, Value>(GOAL_PREPARE_PATH, &());
        assert!(
            matches!(result,Err(WorldServiceClientError::Assurance(ref reason)) if reason=="private response exceeds bound")
        );
        server.join().unwrap();
    }
}
