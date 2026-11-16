use oasis7_agent_api::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    ModelProviderTool, ModelProviderTurn,
    test_support::{
        ConformanceCase, ObservedRequest, Scenario, assert_conformance, assert_legal_case,
        assert_scripted_contract,
    },
};
use oasis7_model_provider::OpenAiChatCompletionClient;
use serde_json::{Value, json};
use std::{
    io::{ErrorKind, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

mod support;

struct HttpCase {
    provider: OpenAiChatCompletionClient,
    requests: Arc<Mutex<Vec<Value>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ConformanceCase for HttpCase {
    type Provider = OpenAiChatCompletionClient;

    fn construct(scenario: Scenario) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let (status, content_type, body) = match scenario {
            Scenario::Legal => (
                "200 OK",
                "text/event-stream",
                support::completion("quote_factory"),
            ),
            Scenario::Unavailable => (
                "503 Service Unavailable",
                "application/json",
                json!({"error":{"message":"temporarily unavailable","type":"server_error"}})
                    .to_string(),
            ),
        };
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        observed
                            .lock()
                            .unwrap()
                            .push(support::read_request(&mut stream));
                        stream
                            .set_write_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("HTTP probe accept failed: {error}"),
                }
            }
        });
        Self {
            provider: support::client(&endpoint),
            requests,
            stopped,
            worker: Some(worker),
        }
    }

    fn provider(&mut self) -> &mut Self::Provider {
        &mut self.provider
    }

    fn observed(&self) -> Vec<ObservedRequest> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|body| {
                let messages = body["input"].as_array().unwrap();
                let prompt = |role: &str| {
                    messages
                        .iter()
                        .find(|message| message["role"] == role)
                        .unwrap()["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                };
                ObservedRequest {
                    system_prompt: body["instructions"].as_str().unwrap().into(),
                    user_prompt: prompt("user"),
                    tools: body["tools"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|tool| ModelProviderTool {
                            name: tool["name"].as_str().unwrap().into(),
                            description: tool["description"].as_str().map(str::to_owned),
                            parameters: tool["parameters"].clone(),
                        })
                        .collect(),
                }
            })
            .collect()
    }
}

impl Drop for HttpCase {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}

struct CorruptedProvider(OpenAiChatCompletionClient);

impl ModelProvider for CorruptedProvider {
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

struct CorruptedCase {
    provider: CorruptedProvider,
    probe: HttpCase,
}

impl ConformanceCase for CorruptedCase {
    type Provider = CorruptedProvider;

    fn construct(scenario: Scenario) -> Self {
        let probe = HttpCase::construct(scenario);
        Self {
            provider: CorruptedProvider(probe.provider.clone()),
            probe,
        }
    }

    fn provider(&mut self) -> &mut Self::Provider {
        &mut self.provider
    }

    fn observed(&self) -> Vec<ObservedRequest> {
        self.probe.observed()
    }
}

#[test]
fn real_adapter_satisfies_shared_caller_snapshot_contract() {
    assert_conformance::<HttpCase>();
}

#[test]
fn fake_satisfies_same_contract_and_detects_parameter_mutation() {
    assert_scripted_contract();
}

#[test]
fn real_adapter_parameter_mutation_fails_same_legal_assertion() {
    assert_legal_case::<HttpCase>();
    let failure = std::panic::catch_unwind(assert_legal_case::<CorruptedCase>)
        .expect_err("the shared legal assertion must reject changed factory arguments");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(message.contains("factory") && message.contains("f2") && message.contains("f1"));
}
