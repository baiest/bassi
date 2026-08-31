use nala::adapters::llm::ollama::OllamaLlm;
use nala::ports::llm::{Llm, LlmError, Message};

use crate::http_stub::HttpStub;

fn user_message(content: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
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
    assert!(request_body.contains(r#""think":false"#));
}

#[test]
fn returns_text_response() {
    let stub = HttpStub::start(
        200,
        r#"{"message":{"content":"chrome opened","tool_calls":null}}"#,
    );
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[user_message("hello")], &[]).unwrap();

    assert_eq!(result.text.as_deref(), Some("chrome opened"));
    assert!(result.tool_calls.is_empty());
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

    let result = llm.generate(&[user_message("open chrome")], &[]).unwrap();

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].name, "execute_command");
    assert!(result.tool_calls[0].arguments.contains("start chrome"));
}

#[test]
fn returns_every_tool_call_when_many() {
    let stub = HttpStub::start(
        200,
        r#"{"message":{"content":"","tool_calls":[
            {"function":{"name":"first","arguments":{}}},
            {"function":{"name":"second","arguments":{}}}
        ]}}"#,
    );
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[], &[]).unwrap();

    let names: Vec<&str> = result
        .tool_calls
        .iter()
        .map(|tool_call| tool_call.name.as_str())
        .collect();
    assert_eq!(names, vec!["first", "second"]);
}

#[test]
fn fails_on_non_success_status() {
    let stub = HttpStub::start(500, "internal error");
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[], &[]);

    assert!(matches!(result, Err(LlmError::RequestFailed(_))));
}

#[test]
fn fails_with_model_not_found_on_404() {
    let stub = HttpStub::start(404, "model not found");
    let mut llm = OllamaLlm::new(&stub.base_url, "missing-model").unwrap();

    let result = llm.generate(&[], &[]);

    assert!(matches!(result, Err(LlmError::ModelNotFound(model)) if model == "missing-model"));
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
        images: Vec::new(),
    };

    let result = llm.generate(&[tool_message], &[]);

    assert!(result.is_ok());

    let request_body = stub.received_body();
    assert!(request_body.contains(r#""tool_name":"execute_command""#));
}

#[test]
fn sends_images_when_the_message_carries_them() {
    let stub = HttpStub::start(200, r#"{"message":{"content":"hi","tool_calls":null}}"#);
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let tool_message = Message {
        role: "tool".to_string(),
        content: "here is the screen".to_string(),
        tool_calls: None,
        tool_name: Some("screenshot".to_string()),
        images: vec!["YmFzZTY0ZGF0YQ==".to_string()],
    };

    let result = llm.generate(&[tool_message], &[]);

    assert!(result.is_ok());

    let request_body = stub.received_body();
    assert!(request_body.contains(r#""images":["YmFzZTY0ZGF0YQ=="]"#));
}

#[test]
fn omits_the_images_field_when_the_message_has_none() {
    let stub = HttpStub::start(200, r#"{"message":{"content":"hi","tool_calls":null}}"#);
    let mut llm = OllamaLlm::new(&stub.base_url, "test-model").unwrap();

    let result = llm.generate(&[user_message("hello")], &[]);

    assert!(result.is_ok());

    let request_body = stub.received_body();
    assert!(!request_body.contains(r#""images""#));
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
