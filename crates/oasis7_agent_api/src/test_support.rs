//! Explicitly enabled test support for the single-call CallerSnapshot contract.
use crate::{
    BudgetContractV1, Digest32, ModelProvider, ModelProviderError, ModelProviderRequest,
    ModelProviderResponse, ModelProviderTool, ModelProviderTrace, ModelProviderTurn,
    ModelProviderUsage,
};
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

/// A scripted step may derive its response from the exact request, for consumers
/// whose request identity is constructed by the system under test.
pub type Step = Box<
    dyn FnOnce(&ModelProviderRequest) -> Result<ModelProviderResponse, ModelProviderError> + Send,
>;
#[derive(Clone, Default)]
pub struct RequestProbe(Arc<Mutex<Vec<ModelProviderRequest>>>);
impl RequestProbe {
    pub fn requests(&self) -> Vec<ModelProviderRequest> {
        self.0.lock().unwrap().clone()
    }
}
pub struct ScriptedProvider {
    steps: VecDeque<Step>,
    probe: RequestProbe,
}
impl ScriptedProvider {
    pub fn new(steps: Vec<Step>) -> (Self, RequestProbe) {
        let probe = RequestProbe::default();
        (
            Self {
                steps: steps.into(),
                probe: probe.clone(),
            },
            probe,
        )
    }
}
impl ModelProvider for ScriptedProvider {
    fn provider_id(&self) -> &str {
        "scripted"
    }
    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        self.probe.0.lock().unwrap().push(request.clone());
        self.steps.pop_front().ok_or_else(|| {
            ModelProviderError::new(
                "script_exhausted",
                "scripted provider has no remaining step",
                false,
            )
        })?(request)
    }
}
pub fn fixed_step(result: Result<ModelProviderResponse, ModelProviderError>) -> Step {
    Box::new(move |_| result)
}

pub fn request() -> ModelProviderRequest {
    ModelProviderRequest {
        request_digest: Digest32::from(
            "blake3:0000000000000000000000000000000000000000000000000000000000000000",
        ),
        model: "test-model".into(),
        system_prompt: "fixed system prompt".into(),
        user_prompt: "fixed user prompt".into(),
        tools: vec![ModelProviderTool {
            name: "quote_factory".into(),
            description: Some("Quote a factory".into()),
            parameters: json!({"type":"object","properties":{"factory":{"type":"string"}},"required":["factory"],"additionalProperties":false}),
        }],
        budget: BudgetContractV1 {
            max_latency_ms: 2000,
            max_repair_attempts: 0,
            max_model_calls: 1,
            max_tool_calls: 1,
        },
    }
}
pub fn legal_response() -> ModelProviderResponse {
    ModelProviderResponse {
        request_digest: request().request_digest,
        turns: vec![ModelProviderTurn::ToolCall {
            name: "quote_factory".into(),
            arguments: json!({"factory":"f1"}),
        }],
        output: String::new(),
        model: None,
        usage: ModelProviderUsage {
            total_tokens: Some(5),
            ..Default::default()
        },
        trace: ModelProviderTrace::default(),
    }
}
/// Independent oracle shared by the fake and each real-adapter probe.
pub fn assert_legal(response: &ModelProviderResponse, sent: &ObservedRequest) {
    let expected = request();
    assert_eq!(response.request_digest, expected.request_digest);
    assert_eq!(
        response.turns,
        vec![ModelProviderTurn::ToolCall {
            name: "quote_factory".into(),
            arguments: json!({"factory":"f1"})
        }]
    );
    assert_eq!(response.usage.total_tokens, Some(5));
    assert_eq!(sent.system_prompt, expected.system_prompt);
    assert_eq!(sent.user_prompt, expected.user_prompt);
    assert_eq!(sent.tools, expected.tools);
}
#[derive(Clone, Copy)]
pub enum Scenario {
    Legal,
    Unavailable,
}
/// Implementors construct an isolated provider and probe for each scenario.
/// `observed` reports the original semantic request seen by the fake or HTTP
/// endpoint; implementations must not synthesize it from the expected request.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedRequest {
    pub system_prompt: String,
    pub user_prompt: String,
    pub tools: Vec<ModelProviderTool>,
}
impl From<&ModelProviderRequest> for ObservedRequest {
    fn from(value: &ModelProviderRequest) -> Self {
        Self {
            system_prompt: value.system_prompt.clone(),
            user_prompt: value.user_prompt.clone(),
            tools: value.tools.clone(),
        }
    }
}
pub trait ConformanceCase {
    type Provider: ModelProvider;
    fn construct(scenario: Scenario) -> Self
    where
        Self: Sized;
    fn provider(&mut self) -> &mut Self::Provider;
    fn observed(&self) -> Vec<ObservedRequest>;
}
/// Execute the legal scenario, including checked invocation and probe observation.
pub fn assert_legal_case<C: ConformanceCase>() {
    let mut case = C::construct(Scenario::Legal);
    let response = case.provider().complete_checked(&request()).unwrap();
    let observed = case.observed();
    assert_eq!(observed.len(), 1);
    assert_legal(&response, &observed[0]);
}
/// Run exactly the same checked-contract assertions against any implementation.
pub fn assert_conformance<C: ConformanceCase>() {
    assert_legal_case::<C>();
    for (index, code) in [
        (0, "invalid_request_digest"),
        (1, "invalid_model"),
        (2, "invalid_tool_schema"),
    ] {
        let mut case = C::construct(Scenario::Legal);
        let mut input = request();
        match index {
            0 => input.request_digest = Digest32::from("bad"),
            1 => input.model.clear(),
            _ => input.tools[0].name.clear(),
        }
        let error = case.provider().complete_checked(&input).unwrap_err();
        assert_eq!(error.code(), code);
        assert!(!error.retryable);
        assert!(case.observed().is_empty());
    }
    let mut case = C::construct(Scenario::Unavailable);
    let error = case.provider().complete_checked(&request()).unwrap_err();
    assert_eq!(error.code(), "provider_unavailable");
    assert!(error.retryable);
    // Borrow the real implementation directly; the wrapper changes only the
    // returned response before complete_checked validates it.
    for (alteration, code) in [
        (1, "response_identity_mismatch"),
        (2, "empty_model_response"),
    ] {
        let mut case = C::construct(Scenario::Legal);
        let mut wrapped = BorrowedAlter {
            inner: case.provider(),
            alteration,
        };
        let error = wrapped.complete_checked(&request()).unwrap_err();
        assert_eq!(error.code(), code);
        assert!(!error.retryable);
    }
}
struct BorrowedAlter<'a, P> {
    inner: &'a mut P,
    alteration: u8,
}
impl<P: ModelProvider> ModelProvider for BorrowedAlter<'_, P> {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }
    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        let mut response = self.inner.complete(request)?;
        match self.alteration {
            1 => {
                response.request_digest = Digest32::from(
                    "blake3:1111111111111111111111111111111111111111111111111111111111111111",
                )
            }
            2 => response.turns.clear(),
            _ => {}
        }
        Ok(response)
    }
}

/// Exercise the shared oracle with the reusable fake, including a controlled
/// mutation of a successful underlying response. Consumers can run this from
/// their own test targets, where dependency unit tests are not executed.
pub fn assert_scripted_contract() {
    struct FakeCase {
        provider: ScriptedProvider,
        probe: RequestProbe,
    }
    impl ConformanceCase for FakeCase {
        type Provider = ScriptedProvider;
        fn construct(scenario: Scenario) -> Self {
            let result = match scenario {
                Scenario::Legal => Ok(legal_response()),
                Scenario::Unavailable => Err(ModelProviderError::new(
                    "provider_unavailable",
                    "offline",
                    true,
                )),
            };
            let (provider, probe) = ScriptedProvider::new(vec![fixed_step(result)]);
            Self { provider, probe }
        }
        fn provider(&mut self) -> &mut Self::Provider {
            &mut self.provider
        }
        fn observed(&self) -> Vec<ObservedRequest> {
            self.probe
                .requests()
                .iter()
                .map(ObservedRequest::from)
                .collect()
        }
    }
    struct Mutated(ScriptedProvider);
    impl ModelProvider for Mutated {
        fn provider_id(&self) -> &str {
            self.0.provider_id()
        }
        fn complete(
            &mut self,
            request: &ModelProviderRequest,
        ) -> Result<ModelProviderResponse, ModelProviderError> {
            let mut response = self.0.complete(request)?;
            if let ModelProviderTurn::ToolCall { arguments, .. } = &mut response.turns[0] {
                *arguments = json!({"factory":"f2"});
            }
            Ok(response)
        }
    }
    struct BrokenCase {
        provider: Mutated,
        probe: RequestProbe,
    }
    impl ConformanceCase for BrokenCase {
        type Provider = Mutated;
        fn construct(_: Scenario) -> Self {
            let (provider, probe) = ScriptedProvider::new(vec![fixed_step(Ok(legal_response()))]);
            Self {
                provider: Mutated(provider),
                probe,
            }
        }
        fn provider(&mut self) -> &mut Self::Provider {
            &mut self.provider
        }
        fn observed(&self) -> Vec<ObservedRequest> {
            self.probe
                .requests()
                .iter()
                .map(ObservedRequest::from)
                .collect()
        }
    }
    assert_conformance::<FakeCase>();
    let failure = std::panic::catch_unwind(assert_legal_case::<BrokenCase>)
        .expect_err("the shared legal assertion must reject the altered factory");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(message.contains("factory") && message.contains("f2") && message.contains("f1"));
    let (mut a, probe_a) = ScriptedProvider::new(vec![fixed_step(Ok(legal_response()))]);
    let (mut b, probe_b) = ScriptedProvider::new(vec![fixed_step(Err(ModelProviderError::new(
        "x", "x", false,
    )))]);
    assert!(a.complete_checked(&request()).is_ok());
    assert_eq!(
        a.complete_checked(&request()).unwrap_err().code(),
        "script_exhausted"
    );
    assert_eq!(b.complete_checked(&request()).unwrap_err().code(), "x");
    assert_eq!(probe_a.requests().len(), 2);
    assert_eq!(probe_b.requests().len(), 1);
}

#[cfg(test)]
mod tests {
    #[test]
    fn scripted_provider_satisfies_shared_contract_and_detects_mutation() {
        super::assert_scripted_contract();
    }
}
