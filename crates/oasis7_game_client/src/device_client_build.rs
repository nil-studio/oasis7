use super::*;
#[cfg(test)]
use oasis7_client_api::world_service as authority;
use oasis7_client_api::world_service::task_factory_quote::{
    TASK_FACTORY_QUOTE_PATH, TaskFactoryQuoteDeniedV1, TaskFactoryQuoteRequestV1,
    TaskFactoryQuoteResponseV1, TaskFactoryQuoteV1,
};

pub enum BuildQuoteOutcome {
    Quote(Box<TaskFactoryQuoteV1>),
    Denied(Box<TaskFactoryQuoteDeniedV1>),
    Original(Value),
}
impl Client {
    pub fn quote_build_outcome(
        &self,
        operation: &str,
        site: &str,
        factory: &str,
        kind: &str,
    ) -> Result<BuildQuoteOutcome, String> {
        if let Some(original) = self.cached_intent(ClientStateKind::Action, operation)? {
            validate_original_build(&original, &self.agent, site, factory, kind)?;
            return Ok(BuildQuoteOutcome::Original(
                json!({"original_operation":self.lookup(&original)?,"quote":null,
                "next":"retain_original_action; capture a new context only after a settled result"}),
            ));
        }
        if let Some(original) = self.cached_intent(ClientStateKind::Outcome, operation)? {
            return Ok(BuildQuoteOutcome::Original(
                json!({"original_operation":self.lookup(&original)?,"quote":null,"next":"retain_original_outcome"}),
            ));
        }
        let (_, quoted) = self.fresh_quote(operation, |captured| {
            self.factory_quote(operation, captured, site, factory, kind)
        })?;
        self.ensure_quote_has_no_write(operation)?;
        Ok(match quoted {
            TaskFactoryQuoteResponseV1::Quote(quote) => BuildQuoteOutcome::Quote(quote),
            TaskFactoryQuoteResponseV1::Denied(denial) => BuildQuoteOutcome::Denied(denial),
        })
    }
    pub fn quote_build(
        &self,
        operation: &str,
        site: &str,
        factory: &str,
        kind: &str,
    ) -> Result<Value, String> {
        match self.quote_build_outcome(operation, site, factory, kind)? {
            BuildQuoteOutcome::Quote(quote) => serde_json::to_value(quote),
            BuildQuoteOutcome::Denied(denial) => serde_json::to_value(denial),
            BuildQuoteOutcome::Original(original) => return Ok(original),
        }
        .map_err(|_| "factory quote encoding failed".into())
    }
    fn factory_quote(
        &self,
        operation: &str,
        captured: &CapturedTaskContextV1,
        site: &str,
        factory: &str,
        kind: &str,
    ) -> Result<TaskFactoryQuoteResponseV1, crate::WorldServiceClientError> {
        let request = self
            .sign(
                TASK_FACTORY_QUOTE_PATH,
                TaskFactoryQuoteRequestV1 {
                    schema_revision: 1,
                    context_proof: self
                        .context_read_proof(operation)
                        .map_err(crate::WorldServiceClientError::Configuration)?,
                    expected_base_binding: captured
                        .current_public_context
                        .captured_base_binding
                        .clone(),
                    site_id: site.to_owned(),
                    factory_id: factory.to_owned(),
                    factory_kind: kind.to_owned(),
                },
            )
            .map_err(crate::WorldServiceClientError::Configuration)?;
        let quoted = self.remote.quote_task_factory_outcome(&request)?;
        if let TaskFactoryQuoteResponseV1::Quote(quote) = &quoted {
            quote
                .quote
                .validate_selection(&self.agent, site, kind, factory)
                .map_err(crate::WorldServiceClientError::Assurance)?;
        }
        Ok(quoted)
    }
    pub fn build(
        &self,
        operation: &str,
        site: &str,
        factory: &str,
        kind: &str,
    ) -> Result<IntentResponse<Value>, String> {
        // Original recovery precedes fresh context/quote reads, including Unknown.
        if let Some(original) = self.cached_intent(ClientStateKind::Action, operation)? {
            validate_original_build(&original, &self.agent, site, factory, kind)?;
            return self.lookup(&original);
        }
        if let Some(original) = self.cached_intent(ClientStateKind::Outcome, operation)? {
            return self.lookup(&original);
        }
        let (mut captured, quoted) = self.fresh_quote(operation, |captured| {
            self.factory_quote(operation, captured, site, factory, kind)
        })?;
        let TaskFactoryQuoteResponseV1::Quote(quoted) = quoted else {
            return Err(
                "factory selection denied; no action submitted; inspect quote-build".into(),
            );
        };
        if quoted.quote.rejection.is_some() {
            return Err("factory quote rejected; no action submitted; inspect quote-build".into());
        }
        captured.current_public_context = quoted.current_public_context;
        self.ensure_quote_has_no_write(operation)?;
        let signed = self.sign(
            "cognition",
            cognition_intent(&captured, quoted.quote.action)?,
        )?;
        self.ensure_quote_has_no_write(operation)?;
        let original = self.cache_intent(
            ClientStateKind::Action,
            operation,
            WorldServicePayloadV1::Cognition(signed),
        )?;
        self.submit_and_poll(&original)
    }
}
fn validate_original_build(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    agent: &str,
    site: &str,
    factory: &str,
    kind: &str,
) -> Result<(), String> {
    let WorldServicePayloadV1::Cognition(signed) = &request.signed_payload else {
        return Err("invalid cached action".into());
    };
    validate_cached_build_action(&signed.request.action, agent, site, factory, kind)
}
fn validate_cached_build_action(
    action: &crate_action::Action,
    agent: &str,
    site: &str,
    factory: &str,
    kind: &str,
) -> Result<(), String> {
    let expected = oasis7_agent_api::world_quote::builtin_factory_spec(kind, factory)
        .ok_or("factory kind requires an exact built-in ID")?;
    match action {
        crate_action::Action::BuildFactory {
            builder_agent_id,
            site_id,
            spec,
        } if builder_agent_id == agent && site_id == site && *spec == expected => Ok(()),
        _ => Err("context already has a different original action; query its result".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn corrupt_original_cache_blocks_build_and_quote_before_any_network_read() {
        use std::os::unix::fs::PermissionsExt;
        struct TestDirectory(PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let temp = TestDirectory(std::fs::canonicalize(std::env::temp_dir()).unwrap().join(
            format!(
                "oasis7-client-build-{}-{}",
                std::process::id(),
                hex::encode(nonce)
            ),
        ));
        std::fs::create_dir(&temp.0).unwrap();
        std::fs::set_permissions(&temp.0, std::fs::Permissions::from_mode(0o700)).unwrap();
        let signer = temp.0.join("signer");
        std::fs::write(&signer, [7u8; 32]).unwrap();
        std::fs::set_permissions(&signer, std::fs::Permissions::from_mode(0o600)).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let state = temp.0.join("state");
        let config = temp.0.join("device.json");
        let service_key = authority::sign_read_request("service-key", (), &hex::encode([9u8; 32]))
            .unwrap()
            .subject_public_key;
        std::fs::write(
            &config,
            serde_json::to_vec(&json!({
                "endpoint":format!("http://{}", listener.local_addr().unwrap()),
                "world_id":"world", "genesis_digest":"genesis", "agent_id":"configured",
                "service_public_key":service_key, "signer_file":signer, "state_directory":state
            }))
            .unwrap(),
        )
        .unwrap();
        let client = Client::open(&config).unwrap();
        let cache_key = blake3::derive_key("oasis7/private-task-client-key/v1", &[7u8; 32]);
        let index_key = blake3::derive_key("oasis7/private-task-client-index/v1", &cache_key);
        let scope =
            oasis7_wasm_abi::encode_canonical_cbor(&(ClientStateKind::Action, "original")).unwrap();
        let record = state.join(format!("client-{}", blake3::keyed_hash(&index_key, &scope)));
        std::fs::write(&record, b"corrupt original").unwrap();
        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            client
                .build("original", "site", "factory", "factory.smelter.mk1")
                .is_err()
        );
        assert!(
            client
                .quote_build("original", "site", "factory", "factory.smelter.mk1")
                .is_err()
        );
        assert_eq!(std::fs::read(&record).unwrap(), b"corrupt original");
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn cached_build_recovery_requires_original_selection_and_canonical_spec() {
        let action = crate_action::Action::BuildFactory {
            builder_agent_id: "configured".into(),
            site_id: "ready-site".into(),
            spec: oasis7_agent_api::world_quote::builtin_factory_spec(
                "factory.smelter.mk1",
                "new-smelter",
            )
            .unwrap(),
        };
        assert!(
            validate_cached_build_action(
                &action,
                "configured",
                "ready-site",
                "new-smelter",
                "factory.smelter.mk1"
            )
            .is_ok()
        );
        for (agent, site, factory, kind) in [
            ("other", "ready-site", "new-smelter", "factory.smelter.mk1"),
            ("configured", "other", "new-smelter", "factory.smelter.mk1"),
            ("configured", "ready-site", "other", "factory.smelter.mk1"),
            (
                "configured",
                "ready-site",
                "new-smelter",
                "factory.assembler.mk1",
            ),
        ] {
            assert!(validate_cached_build_action(&action, agent, site, factory, kind).is_err());
        }
        let mut changed = action.clone();
        let crate_action::Action::BuildFactory { spec, .. } = &mut changed else {
            unreachable!()
        };
        spec.base_power_draw += 1;
        assert!(
            validate_cached_build_action(
                &changed,
                "configured",
                "ready-site",
                "new-smelter",
                "factory.smelter.mk1"
            )
            .is_err()
        );
        let other = crate_action::Action::MoveAgent {
            agent_id: "configured".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        };
        assert!(
            validate_cached_build_action(
                &other,
                "configured",
                "ready-site",
                "new-smelter",
                "factory.smelter.mk1"
            )
            .is_err()
        );
    }
}
