use reqwest::blocking::Client;

use crate::{
    ports::llm::{Llm, LlmError, LlmResponse, Message},
    ports::tool::ToolDefinition,
};

use wire::{ChatRequest, ChatResponse};

pub struct OllamaLlm {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaLlm {
    pub fn new(base_url: &str, model: &str) -> Result<Self, LlmError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|error| LlmError::RequestFailed(error.to_string()))?;

        Ok(Self {
            client,
            base_url: base_url.to_string(),
            model: model.to_string(),
        })
    }
}

impl Llm for OllamaLlm {
    fn generate(
        &mut self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let request = ChatRequest {
            model: &self.model,
            messages: wire::to_wire_messages(messages),
            tools: wire::to_wire_tools(tools),
            stream: false,
            // Extended "thinking" adds tens of seconds of latency per call
            // on models that support it (e.g. gemma4), which multiplies
            // badly across a tool-call loop. Tool selection doesn't need it.
            think: false,
        };

        let response = self.post_chat(&request)?;

        Ok(wire::to_domain_response(response))
    }
}

impl OllamaLlm {
    fn post_chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(request)
            .send()
            .map_err(|error| LlmError::RequestFailed(error.to_string()))?;

        let status = response.status();

        let body = response
            .text()
            .map_err(|error| LlmError::RequestFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(LlmError::RequestFailed(format!(
                "Ollama returned status {status}\nBody: {body}"
            )));
        }

        serde_json::from_str(&body).map_err(|error| {
            LlmError::InvalidResponse(format!(
                "Could not parse Ollama response: {error}\nBody: {body}"
            ))
        })
    }
}

mod wire {
    use serde::{Deserialize, Serialize};

    use crate::{
        ports::llm::{LlmResponse, Message, ToolCall},
        ports::tool::ToolDefinition,
    };

    #[derive(Serialize)]
    pub struct ChatRequest<'a> {
        pub model: &'a str,
        pub messages: Vec<OllamaMessage>,
        pub tools: Vec<ChatTool<'a>>,
        pub stream: bool,
        pub think: bool,
    }

    #[derive(Serialize)]
    pub struct OllamaMessage {
        role: String,
        content: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<OllamaToolCall>>,

        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,

        #[serde(skip_serializing_if = "Vec::is_empty")]
        images: Vec<String>,
    }

    #[derive(Serialize)]
    struct OllamaToolCall {
        function: OllamaFunctionCall,
    }

    #[derive(Serialize)]
    struct OllamaFunctionCall {
        name: String,
        arguments: serde_json::Value,
    }

    #[derive(Serialize)]
    pub struct ChatTool<'a> {
        r#type: &'a str,
        function: FunctionDefinition<'a>,
    }

    #[derive(Serialize)]
    struct FunctionDefinition<'a> {
        name: &'a str,
        description: &'a str,
        parameters: serde_json::Value,
    }

    #[derive(Deserialize, Debug)]
    pub struct ChatResponse {
        message: ChatMessage,
    }

    #[derive(Deserialize, Debug)]
    struct ChatMessage {
        content: String,
        tool_calls: Option<Vec<ToolCallResponse>>,
    }

    #[derive(Deserialize, Debug)]
    struct ToolCallResponse {
        function: FunctionCallResponse,
    }

    #[derive(Deserialize, Debug)]
    struct FunctionCallResponse {
        name: String,
        arguments: serde_json::Value,
    }

    /// A tool call's `arguments` are stored as a JSON string on the domain
    /// side; if they don't parse as JSON we fall back to an empty object
    /// rather than fail the whole request.
    pub fn to_wire_messages(messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|message| OllamaMessage {
                role: message.role.clone(),
                content: message.content.clone(),
                tool_name: message.tool_name.clone(),
                images: message.images.clone(),
                tool_calls: message.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|call| OllamaToolCall {
                            function: OllamaFunctionCall {
                                name: call.name.clone(),
                                arguments: serde_json::from_str(&call.arguments)
                                    .unwrap_or(serde_json::json!({})),
                            },
                        })
                        .collect()
                }),
            })
            .collect()
    }

    pub fn to_wire_tools<'a>(tools: &[&'a ToolDefinition]) -> Vec<ChatTool<'a>> {
        tools
            .iter()
            .map(|tool| ChatTool {
                r#type: "function",
                function: FunctionDefinition {
                    name: &tool.name,
                    description: &tool.description,
                    parameters: tool.parameters.clone(),
                },
            })
            .collect()
    }

    /// Ollama can return several tool calls in one response; only the first
    /// is used, the rest are dropped.
    pub fn to_domain_response(response: ChatResponse) -> LlmResponse {
        let first_tool_call = response.message.tool_calls.into_iter().flatten().next();

        match first_tool_call {
            Some(tool_call) => LlmResponse::ToolCall(ToolCall {
                name: tool_call.function.name,
                arguments: tool_call.function.arguments.to_string(),
            }),
            None => LlmResponse::Text(response.message.content),
        }
    }
}
