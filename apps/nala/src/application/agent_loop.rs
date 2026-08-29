use std::time::Instant;

use crate::application::assistant::{Assistant, PLANNING_INSTRUCTIONS};
use crate::application::assistant::{
    AssistantError, MAX_IDENTICAL_REPEATS, MAX_TOOL_CALLS, assistant_text_message,
    assistant_tool_call_message, system_message, tool_result_message, user_message,
};
use crate::ports::events::{Event, EventSink, TurnState};
use crate::ports::llm::{Llm, LlmError, LlmResponse, Message, ToolCall};
use crate::ports::tool::ToolDefinition;
use crate::ports::tool_dispatcher::{ToolDispatcher, ToolOutcome};

/// How many turns in a row the model may answer with neither text nor a
/// tool call before the turn is aborted. A model occasionally emits an
/// empty message; a few retries with a nudge usually recovers it, but this
/// bounds how long the loop waits on a model that's stuck saying nothing.
const MAX_EMPTY_RESPONSES: usize = 3;

impl<L, D, E> Assistant<L, D, E>
where
    L: Llm,
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    E: EventSink,
{
    pub fn process(&mut self, input: &str) -> Result<String, AssistantError<LlmError, D::Error>> {
        let request_start = Instant::now();

        self.events.emit(Event::RequestStarted);
        self.set_state(TurnState::Receiving);

        self.transcript.push(user_message(input));

        let mut messages = self.build_prompt_messages()?;
        // Cloned rather than borrowed from `self.registry`, so `tools` doesn't
        // keep an immutable borrow of `self` alive across the `&mut self`
        // calls below (`build_plan`, `self.llm.generate`, ...).
        let tool_definitions: Vec<ToolDefinition> =
            self.registry.definitions().into_iter().cloned().collect();
        let tools: Vec<&ToolDefinition> = tool_definitions.iter().collect();

        self.set_state(TurnState::Planning);
        if let Some(plan) = self.build_plan(&messages, &tools) {
            self.events.emit(Event::PlanCreated { plan: plan.clone() });
            messages.push(assistant_text_message(format!("Plan:\n{plan}")));
        }

        let mut last_tool_call: Option<ToolCall> = None;
        let mut identical_repeats: usize = 0;
        let mut tool_call_count: usize = 0;
        let mut empty_responses: usize = 0;

        loop {
            self.set_state(TurnState::Thinking);
            let response = self.generate(&messages)?;

            if !response.tool_calls.is_empty() {
                self.set_state(TurnState::Executing);

                for tool_call in response.tool_calls {
                    messages.push(assistant_tool_call_message(tool_call.clone()));

                    if last_tool_call.as_ref() == Some(&tool_call) {
                        identical_repeats += 1;
                    } else {
                        identical_repeats = 1;
                        last_tool_call = Some(tool_call.clone());
                    }

                    if identical_repeats >= MAX_IDENTICAL_REPEATS {
                        return Err(self.abort(request_start, "loop detected", |_| {
                            AssistantError::LoopDetected
                        }));
                    }
                    tool_call_count += 1;
                    empty_responses = 0;

                    let tool_name = tool_call.name.clone();
                    let tool_args = tool_call.arguments.clone();

                    self.events.emit(Event::ToolStarted {
                        name: tool_name.clone(),
                        arguments: tool_args,
                    });

                    let start = Instant::now();
                    let outcome = handle_tool_call(&mut self.dispatcher, tool_call);
                    let duration = start.elapsed();

                    self.events.emit(Event::ToolCompleted {
                        name: tool_name.clone(),
                        duration,
                        output: outcome.text.clone(),
                        images: outcome.images.len(),
                    });

                    messages.push(tool_result_message(tool_name, outcome));

                    if tool_call_count >= MAX_TOOL_CALLS {
                        return Err(self.abort(request_start, "tool call limit exceeded", |_| {
                            AssistantError::ToolCallLimitExceeded
                        }));
                    }
                }

                continue;
            }

            match response.text {
                Some(text) if !text.trim().is_empty() => {
                    self.set_state(TurnState::Responding);
                    self.transcript.push(assistant_text_message(text.clone()));

                    let duration = request_start.elapsed();
                    self.events.emit(Event::RequestCompleted { duration });

                    break Ok(text);
                }
                _ => {
                    empty_responses += 1;
                    if empty_responses >= MAX_EMPTY_RESPONSES {
                        return Err(self.abort(request_start, "empty response", |_| {
                            AssistantError::EmptyResponse
                        }));
                    }
                    messages.push(system_message(
                        "Your last response had no text and no tool call. \
                         Either call a tool or answer the user's request in \
                         natural language."
                            .to_string(),
                    ));
                }
            }
        }
    }

    fn generate(
        &mut self,
        messages: &[Message],
    ) -> Result<LlmResponse, AssistantError<LlmError, D::Error>> {
        let tool_definitions: Vec<ToolDefinition> =
            self.registry.definitions().into_iter().cloned().collect();
        let tools: Vec<&ToolDefinition> = tool_definitions.iter().collect();

        let outgoing_images: usize = messages.iter().map(|message| message.images.len()).sum();
        self.events.emit(Event::LlmStarted {
            images: outgoing_images,
        });

        let start = Instant::now();
        let response = self.llm.generate(messages, &tools);
        let duration = start.elapsed();

        match response {
            Ok(response) => {
                self.events.emit(Event::LlmCompleted { duration });
                Ok(response)
            }
            Err(error) => {
                self.events.emit(Event::RequestFailed {
                    duration,
                    error: error.to_string(),
                });
                Err(AssistantError::Llm(error))
            }
        }
    }

    fn set_state(&mut self, state: TurnState) {
        self.events.emit(Event::StateChanged { state });
    }

    fn abort<F>(
        &mut self,
        request_start: Instant,
        reason: &str,
        error: F,
    ) -> AssistantError<LlmError, D::Error>
    where
        F: FnOnce(&str) -> AssistantError<LlmError, D::Error>,
    {
        let duration = request_start.elapsed();
        self.events.emit(Event::RequestFailed {
            duration,
            error: reason.to_string(),
        });
        error(reason)
    }

    /// Asks the model for a short step-by-step plan before it does anything,
    /// so a multi-step task (open an app, find a specific item, act on it)
    /// has an explicit target to follow instead of being decided one tool
    /// call at a time with no larger goal in view.
    ///
    /// Tools are deliberately withheld from this call (an empty slice, not
    /// `tools`) — a text-summarized list is included in the prompt instead
    /// — so a tool-eager model can't skip straight to acting instead of
    /// planning. A failure here (network error, etc.) isn't fatal to the
    /// request: it just means proceeding without a plan.
    pub(crate) fn build_plan(
        &mut self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Option<String> {
        let tool_summary: String = tools
            .iter()
            .map(|tool| format!("- {}: {}", tool.name, tool.description))
            .collect::<Vec<_>>()
            .join("\n");

        let mut planning_messages = messages.to_vec();
        planning_messages.push(system_message(format!(
            "Available tools:\n{tool_summary}\n\n{PLANNING_INSTRUCTIONS}"
        )));

        self.events.emit(Event::LlmStarted { images: 0 });
        let start = Instant::now();
        let response = self.llm.generate(&planning_messages, &[]);
        let duration = start.elapsed();

        let response = response.ok()?;
        self.events.emit(Event::LlmCompleted { duration });

        match response.text {
            Some(plan) if !plan.trim().is_empty() => Some(plan),
            _ => None,
        }
    }

    /// Context can change between turns, so it is fetched fresh each time
    /// and never persisted in the transcript.
    pub(crate) fn build_prompt_messages(
        &mut self,
    ) -> Result<Vec<Message>, AssistantError<LlmError, D::Error>> {
        let context = self
            .dispatcher
            .get_context()
            .map_err(AssistantError::Tool)?;

        let mut messages = self.transcript.snapshot();
        messages.push(system_message(format!(
            "Computer context:\n{context}\n\nUse this context when generating commands."
        )));

        Ok(messages)
    }
}

fn handle_tool_call<D>(dispatcher: &mut D, tool_call: ToolCall) -> ToolOutcome
where
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
{
    match dispatcher.dispatch(tool_call) {
        Ok(outcome) => outcome,
        Err(error) => ToolOutcome::from(format!("ERROR: {error}")),
    }
}
