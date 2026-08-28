use nala::adapters::llm::ollama::OllamaLlm;
use nala::ports::llm::{Llm, LlmError, LlmResponse, Message};

use crate::http_stub::HttpStub;

fn user_message(content: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_name: None,
    }
}

#[test]
fn sends_model_and_messages_in_request() {
    let stub = HttpStub::start(200, r#"{"message":{"content":"hi","tool_calls":null}}"#);
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[user_message("hello")], &[]);

    assert!(result.is_ok());

    let request_body = stub.received_body();
    assert!(request_body.contains(r#""model":"test-model""#));
    assert!(request_body.contains(r#""content":"hello""#));
    assert!(request_body.contains(r#""stream":false"#));
}

#[test]
fn returns_text_response() {
    let stub = HttpStub::start(
        200,
        r#"{"message":{"content":"chrome opened","tool_calls":null}}"#,
    );
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[user_message("hello")], &[]);

    assert!(matches!(result, Ok(LlmResponse::Text(text)) if text == "chrome opened"));
}

#[test]
fn returns_tool_call_from_response() {
    let stub = HttpStub::start(
        200,
        r#"{"message":{"content":"","tool_calls":[
            {"function":{"name":"execute_command","arguments":{"command":"start chrome"}}}
        ]}}"#,
    );
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[user_message("open chrome")], &[]);

    match result {
        Ok(LlmResponse::ToolCall(tool_call)) => {
            assert_eq!(tool_call.name, "execute_command");
            assert!(tool_call.arguments.contains("start chrome"));
        }
        other => panic!("expected a tool call, got {other:?}"),
    }
}

#[test]
fn returns_first_tool_call_when_many() {
    let stub = HttpStub::start(
        200,
        r#"{"message":{"content":"","tool_calls":[
            {"function":{"name":"first","arguments":{}}},
            {"function":{"name":"second","arguments":{}}}
        ]}}"#,
    );
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[], &[]);

    assert!(matches!(result, Ok(LlmResponse::ToolCall(tool_call)) if tool_call.name == "first"));
}

#[test]
fn fails_on_non_success_status() {
    let stub = HttpStub::start(500, "internal error");
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[], &[]);

    assert!(matches!(result, Err(LlmError::RequestFailed(_))));
}

#[test]
fn fails_on_malformed_json() {
    let stub = HttpStub::start(200, "not json");
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[], &[]);

    assert!(matches!(result, Err(LlmError::InvalidResponse(_))));
}

#[test]
fn sends_tool_name_for_tool_result_messages() {
    let stub = HttpStub::start(200, r#"{"message":{"content":"hi","tool_calls":null}}"#);
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let tool_message = Message {
        role: "tool".to_string(),
        content: "chrome opened".to_string(),
        tool_calls: None,
        tool_name: Some("execute_command".to_string()),
    };

    let result = llm.generate(&[tool_message], &[]);

    assert!(result.is_ok());

    let request_body = stub.received_body();
    assert!(request_body.contains(r#""tool_name":"execute_command""#));
}

/// Exercises the adapter against a real, locally running Ollama server.
/// Not run by default: needs `ollama serve` with the `qwen3:8b` model pulled.
#[test]
#[ignore]
fn generates_response() {
    let mut llm = OllamaLlm::new("http://localhost:11434", "qwen3:8b").unwrap();

    let result = llm.generate(&[user_message("hello")], &[]);

    assert!(result.is_ok());
}
