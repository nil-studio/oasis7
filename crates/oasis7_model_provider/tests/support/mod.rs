use oasis7_model_provider::{ModelProviderConfig, OpenAiChatCompletionClient};
use serde_json::{Value, json};
use std::{io::Read, net::TcpStream, time::Duration};

pub fn completion(name: &str) -> String {
    let event = json!({"type":"response.completed","sequence_number":1,"response":{
        "id":"resp_test","object":"response","created_at":1,"completed_at":2,"model":"test-model",
        "output":[{"type":"function_call","call_id":"call_test","name":name,"arguments":"{\"factory\":\"f1\"}"}],
        "status":"completed","parallel_tool_calls":false,
        "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5,"input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}
    }});
    format!("data: {event}\n\n")
}

pub fn client(endpoint: &str) -> OpenAiChatCompletionClient {
    let config = ModelProviderConfig::from_values_for_agent(
        |key| match key {
            "OASIS7_LLM_BASE_URL" => Some(endpoint.into()),
            "OASIS7_LLM_API_KEY" => Some("test-secret".into()),
            "OASIS7_LLM_TIMEOUT_MS" => Some("2000".into()),
            _ => None,
        },
        "agent",
    )
    .unwrap();
    OpenAiChatCompletionClient::from_config(&config).unwrap()
}

pub fn read_request(stream: &mut TcpStream) -> Value {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let header_end = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    while bytes.len() < header_end + length {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
}
