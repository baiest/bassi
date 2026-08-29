use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::application::assistant::{Assistant, PLANNING_INSTRUCTIONS};
use crate::application::assistant::{
    AssistantError, assistant_text_message, assistant_tool_call_message, system_message,
    tool_result_message, user_message,
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

/// How often `call_llm` checks for cancellation while waiting on the
/// background LLM call. Short enough that Ctrl+C feels immediate, long
/// enough not to spin the CPU polling a channel.
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// A retryable `LlmError` is worth trying again (network hiccup, non-2xx
/// status, ...); an `InvalidResponse` means the server answered but the
/// body didn't parse — retrying the identical request won't change that,
/// so it's treated as fatal.
fn is_retryable(error: &LlmError) -> bool {
    matches!(error, LlmError::RequestFailed(_))
}

/// The result of a `call_llm` call: either the LLM call finished (with a
/// success or failure), or cancellation was requested before it did and the
/// caller gave up waiting on it.
enum LlmCallOutcome {
    Completed(Result<LlmResponse, LlmError>),
    Cancelled,
}

impl<L, D, E> Assistant<L, D, E>
where
    L: Llm + Send + 'static,
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
        // calls below (`build_plan`, `self.generate`, ...).
        let tool_definitions: Vec<ToolDefinition> =
            self.registry.definitions().into_iter().cloned().collect();

        self.set_state(TurnState::Planning);
        if let Some(plan) = self.build_plan(&messages, &tool_definitions) {
            self.events.emit(Event::PlanCreated { plan: plan.clone() });
            messages.push(assistant_text_message(format!("Plan:\n{plan}")));
        }

        let mut last_tool_call: Option<ToolCall> = None;
        let mut identical_repeats: usize = 0;
        let mut tool_call_count: usize = 0;
        let mut empty_responses: usize = 0;
        let mut consecutive_tool_failures: usize = 0;

        loop {
            if self.cancel.is_cancelled() {
                return Err(self.cancelled(request_start));
            }

            self.set_state(TurnState::Thinking);
            let response = self.generate_with_retry(&messages, request_start)?;

            if !response.tool_calls.is_empty() {
                self.set_state(TurnState::Executing);

                for tool_call in response.tool_calls {
                    if self.cancel.is_cancelled() {
                        return Err(self.cancelled(request_start));
                    }

                    messages.push(assistant_tool_call_message(tool_call.clone()));

                    if last_tool_call.as_ref() == Some(&tool_call) {
                        identical_repeats += 1;
                    } else {
                        identical_repeats = 1;
                        last_tool_call = Some(tool_call.clone());
                    }

                    if identical_repeats >= self.limits.max_identical_repeats {
                        let limit = self.limits.max_identical_repeats;
                        return Err(self.abort(request_start, "loop detected", |_| {
                            AssistantError::LoopDetected(limit)
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
                    let dispatch_result = self.dispatcher.dispatch(tool_call);
                    let duration = start.elapsed();

                    let outcome = match dispatch_result {
                        Ok(outcome) => {
                            consecutive_tool_failures = 0;
                            outcome
                        }
                        Err(error) => {
                            consecutive_tool_failures += 1;
                            ToolOutcome::from(format!("ERROR: {error}"))
                        }
                    };

                    self.events.emit(Event::ToolCompleted {
                        name: tool_name.clone(),
                        duration,
                        output: outcome.text.clone(),
                        images: outcome.images.len(),
                    });

                    messages.push(tool_result_message(tool_name, outcome));

                    if consecutive_tool_failures >= self.limits.max_consecutive_tool_failures {
                        let limit = self.limits.max_consecutive_tool_failures;
                        return Err(self.abort(
                            request_start,
                            "too many consecutive tool failures",
                            |_| AssistantError::TooManyConsecutiveToolFailures(limit),
                        ));
                    }

                    if tool_call_count >= self.limits.max_tool_calls {
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

    /// Calls the LLM, retrying a retryable failure with exponential backoff
    /// up to `limits.max_llm_retries` times before giving up. A fatal
    /// failure (e.g. a response that doesn't parse) or a cancellation
    /// requested mid-call or mid-backoff returns immediately instead of
    /// retrying.
    fn generate_with_retry(
        &mut self,
        messages: &[Message],
        request_start: Instant,
    ) -> Result<LlmResponse, AssistantError<LlmError, D::Error>> {
        let mut attempt: u32 = 0;

        loop {
            match self.generate(messages) {
                LlmCallOutcome::Cancelled => return Err(self.cancelled(request_start)),
                LlmCallOutcome::Completed(Ok(response)) => return Ok(response),
                LlmCallOutcome::Completed(Err(llm_error)) => {
                    let retryable = is_retryable(&llm_error);

                    if !retryable || attempt >= self.limits.max_llm_retries {
                        return Err(AssistantError::Llm(llm_error));
                    }

                    if self.cancel.is_cancelled() {
                        return Err(self.cancelled(request_start));
                    }

                    self.events.emit(Event::Retrying {
                        attempt: attempt + 1,
                        error: llm_error.to_string(),
                    });

                    let delay = self.limits.retry_base_delay * 2u32.pow(attempt);
                    self.clock.sleep(delay);
                    attempt += 1;
                }
            }
        }
    }

    fn generate(&mut self, messages: &[Message]) -> LlmCallOutcome {
        let tools: Vec<ToolDefinition> = self.registry.definitions().into_iter().cloned().collect();

        let outgoing_images: usize = messages.iter().map(|message| message.images.len()).sum();
        self.events.emit(Event::LlmStarted {
            images: outgoing_images,
        });

        let start = Instant::now();
        let outcome = self.call_llm(messages.to_vec(), tools);
        let duration = start.elapsed();

        match &outcome {
            LlmCallOutcome::Completed(Ok(_)) => {
                self.events.emit(Event::LlmCompleted { duration });
            }
            LlmCallOutcome::Completed(Err(error)) => {
                self.events.emit(Event::RequestFailed {
                    duration,
                    error: error.to_string(),
                });
            }
            // No event here: cancellation is reported once, by whichever
            // caller turns it into `AssistantError::Cancelled` (see
            // `cancelled`), not per LLM call.
            LlmCallOutcome::Cancelled => {}
        }

        outcome
    }

    /// Runs the actual LLM call on a background thread and waits for it
    /// with short polls instead of one blocking call, so cancellation
    /// requested while a call is in flight (a single call can take well
    /// over a minute against a local model) takes effect immediately
    /// instead of only between calls. The background thread is not
    /// interrupted on cancellation — there's no way to abort an in-flight
    /// HTTP request from the outside — it's simply abandoned: it keeps
    /// running until the real request completes or fails, and its result is
    /// dropped since nothing is left waiting on the channel.
    fn call_llm(&mut self, messages: Vec<Message>, tools: Vec<ToolDefinition>) -> LlmCallOutcome {
        let llm = Arc::clone(&self.llm);
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let tool_refs: Vec<&ToolDefinition> = tools.iter().collect();
            let mut llm = match llm.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let result = llm.generate(&messages, &tool_refs);
            let _ = sender.send(result);
        });

        loop {
            match receiver.recv_timeout(CANCEL_POLL_INTERVAL) {
                Ok(result) => return LlmCallOutcome::Completed(result),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.cancel.is_cancelled() {
                        return LlmCallOutcome::Cancelled;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return LlmCallOutcome::Completed(Err(LlmError::RequestFailed(
                        "LLM worker thread stopped without a result".to_string(),
                    )));
                }
            }
        }
    }

    fn set_state(&mut self, state: TurnState) {
        self.events.emit(Event::StateChanged { state });
    }

    fn cancelled(&mut self, request_start: Instant) -> AssistantError<LlmError, D::Error> {
        let duration = request_start.elapsed();
        self.events.emit(Event::Cancelled);
        self.events.emit(Event::RequestFailed {
            duration,
            error: "cancelled".to_string(),
        });
        AssistantError::Cancelled
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
    /// Tools are deliberately withheld from this call (an empty list, not
    /// `tools`) — a text-summarized list is included in the prompt instead
    /// — so a tool-eager model can't skip straight to acting instead of
    /// planning. A failure here (network error, cancellation, etc.) isn't
    /// fatal to the request: it just means proceeding without a plan — a
    /// cancellation requested during planning is still caught a moment
    /// later, at the top of the main loop below.
    pub(crate) fn build_plan(
        &mut self,
        messages: &[Message],
        tools: &[ToolDefinition],
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
        let outcome = self.call_llm(planning_messages, Vec::new());
        let duration = start.elapsed();

        let response = match outcome {
            LlmCallOutcome::Completed(Ok(response)) => response,
            LlmCallOutcome::Completed(Err(_)) | LlmCallOutcome::Cancelled => return None,
        };
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
