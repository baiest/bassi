use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::application::assistant::{Assistant, PLANNING_INSTRUCTIONS};
use crate::application::assistant::{
    AssistantError, assistant_text_message, assistant_tool_call_message, system_message,
    tool_result_message, user_message,
};
use crate::application::context_budget;
use crate::ports::events::{BudgetStep, Event, EventSink, TurnState};
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

        self.transcript.push(
            user_message(input),
            self.token_counter.as_ref(),
            self.budget.available_tokens(),
        );

        let mut messages = self.build_prompt_messages()?;
        // Cloned rather than borrowed from `self.registry`, so `tools` doesn't
        // keep an immutable borrow of `self` alive across the `&mut self`
        // calls below (`build_plan`, `self.generate`, ...).
        let tool_definitions: Vec<ToolDefinition> =
            self.registry.definitions().into_iter().cloned().collect();

        if self.planning_enabled {
            self.set_state(TurnState::Planning);
            if let Some(plan) = self.build_plan(&messages, &tool_definitions) {
                self.events.emit(Event::PlanCreated { plan: plan.clone() });
                messages.push(assistant_text_message(format!("Plan:\n{plan}")));
            }
        }

        // Everything built so far (persisted history, computer context,
        // plan) is off-limits to the per-turn budget fitter below — only
        // what this turn adds from here (tool calls and their results) is
        // evictable.
        let protected_prefix = messages.len();

        let mut last_tool_call: Option<ToolCall> = None;
        let mut identical_repeats: usize = 0;
        let mut tool_call_count: usize = 0;
        let mut empty_responses: usize = 0;
        let mut consecutive_tool_failures: usize = 0;
        // Set by a mutating tool result, cleared by any subsequent tool
        // call (the model checking its own work) or by the verification
        // gate below letting one unverified answer through.
        let mut unverified_mutation = false;
        let mut verification_gate_used = false;

        loop {
            if self.cancel.is_cancelled() {
                return Err(self.cancelled(request_start));
            }

            self.fit_to_budget(&mut messages, protected_prefix);

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

                    // A mutating call leaves the turn unverified; any call
                    // after it — mutating or not — counts as the model
                    // checking its own work, so it clears the flag rather
                    // than only a read-only one being able to.
                    unverified_mutation = outcome.mutated;

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
                    if unverified_mutation && !verification_gate_used {
                        // Give the model exactly one chance to check its
                        // own work instead of ending the turn on an
                        // unconfirmed mutation. Only one: a model that
                        // genuinely has nothing left to verify (or keeps
                        // misreading the evidence) shouldn't get stuck in a
                        // nagging loop — the second attempt is let through
                        // and marked `AnsweredUnverified` instead.
                        verification_gate_used = true;
                        self.set_state(TurnState::Verifying);
                        messages.push(system_message(
                            "You performed an action that changes state \
                             without confirming the result afterwards. \
                             Verify using the evidence already attached to \
                             that tool's result — a screenshot for a \
                             click/type/key/scroll, or the before/after \
                             system state for a command. That evidence is \
                             already enough in most cases; do NOT take a \
                             new screenshot unless the last action was a \
                             screen interaction and the attached evidence \
                             genuinely does not show the outcome."
                                .to_string(),
                        ));
                        continue;
                    }
                    if unverified_mutation {
                        self.events.emit(Event::AnsweredUnverified);
                    }

                    self.set_state(TurnState::Responding);
                    self.transcript.push(
                        assistant_text_message(text.clone()),
                        self.token_counter.as_ref(),
                        self.budget.available_tokens(),
                    );

                    if let Some(speech) = &self.speech {
                        // The whole answer goes out in a single `say` call:
                        // the Chatterbox backend streams audio back as it's
                        // generated, so playback starts on the first chunk
                        // rather than waiting for the full answer — sentence
                        // splitting here would only add extra round-trips.
                        if let Err(error) = speech.say(&text) {
                            self.events.emit(Event::RequestFailed {
                                duration: request_start.elapsed(),
                                error: error.to_string(),
                            });
                        }
                    }

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

        self.instrumented_call_llm(messages.to_vec(), tools)
    }

    /// The single point where an LLM call is both made and instrumented —
    /// every caller (`generate`, `build_plan`, `compact`) routes through
    /// here, so `LlmStarted`/`LlmCompleted`/`TokensUsed`/`LlmFailed` are
    /// emitted uniformly regardless of which one initiated the call.
    fn instrumented_call_llm(
        &mut self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> LlmCallOutcome {
        let outgoing_images: usize = messages.iter().map(|message| message.images.len()).sum();
        self.events.emit(Event::LlmStarted {
            images: outgoing_images,
        });

        let start = Instant::now();
        let outcome = self.call_llm(messages, tools);
        let duration = start.elapsed();

        match &outcome {
            LlmCallOutcome::Completed(Ok(response)) => {
                self.events.emit(Event::LlmCompleted { duration });
                self.events.emit(Event::TokensUsed {
                    prompt_tokens: response.usage.prompt_tokens,
                    completion_tokens: response.usage.completion_tokens,
                });
            }
            LlmCallOutcome::Completed(Err(error)) => {
                self.events.emit(Event::LlmFailed {
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

        let outcome = self.instrumented_call_llm(planning_messages, Vec::new());

        let response = match outcome {
            LlmCallOutcome::Completed(Ok(response)) => response,
            LlmCallOutcome::Completed(Err(_)) | LlmCallOutcome::Cancelled => return None,
        };

        match response.text {
            Some(plan) if !plan.trim().is_empty() => Some(plan),
            _ => None,
        }
    }

    /// Keeps `messages[protected_prefix..]` within the per-turn token
    /// budget, applying eviction steps in order of increasing cost until it
    /// fits or nothing more can be done:
    /// 1. strip images from tool results older than the most recent few,
    /// 2. truncate long tool-result text to a head/tail excerpt,
    /// 3. summarize the middle of the evictable range into one message
    ///    (`compact`), keeping the most recent messages intact,
    /// 4. as a last resort, drop the oldest evictable messages outright.
    fn fit_to_budget(&mut self, messages: &mut Vec<Message>, protected_prefix: usize) {
        let available = self.budget.available_tokens();

        if self.token_counter.estimate(messages) <= available {
            return;
        }

        let dropped_images = context_budget::evict_images(
            messages,
            protected_prefix,
            self.budget.keep_recent_images,
        );
        if dropped_images > 0 {
            self.events.emit(Event::BudgetPressure {
                step: BudgetStep::DroppedImages {
                    count: dropped_images,
                },
                remaining_estimate: self.token_counter.estimate(messages),
            });
        }
        if self.token_counter.estimate(messages) <= available {
            return;
        }

        let truncated = context_budget::truncate_long_tool_results(
            messages,
            protected_prefix,
            self.budget.truncate_head_chars,
            self.budget.truncate_tail_chars,
        );
        if truncated > 0 {
            self.events.emit(Event::BudgetPressure {
                step: BudgetStep::TruncatedText { count: truncated },
                remaining_estimate: self.token_counter.estimate(messages),
            });
        }
        if self.token_counter.estimate(messages) <= available {
            return;
        }

        if let Some(turns_compacted) = self.compact(messages, protected_prefix) {
            self.events
                .emit(Event::TranscriptCompacted { turns_compacted });
        }
        if self.token_counter.estimate(messages) <= available {
            return;
        }

        let dropped_turns = context_budget::drop_oldest_until_fits(
            messages,
            protected_prefix,
            available,
            self.token_counter.as_ref(),
        );
        if dropped_turns > 0 {
            self.events.emit(Event::BudgetPressure {
                step: BudgetStep::DroppedTurns {
                    count: dropped_turns,
                },
                remaining_estimate: self.token_counter.estimate(messages),
            });
        }
    }

    /// Summarizes `messages[protected_prefix..split]` (everything evictable
    /// except the most recent `budget.keep_recent_uncompacted` messages)
    /// into a single system message, via an extra LLM call. Returns how
    /// many messages were folded into the summary, or `None` if there was
    /// nothing to compact or the summarizing call failed/was cancelled — in
    /// either case the caller falls back to the next eviction step.
    fn compact(&mut self, messages: &mut Vec<Message>, protected_prefix: usize) -> Option<usize> {
        let split = messages
            .len()
            .saturating_sub(self.budget.keep_recent_uncompacted)
            .max(protected_prefix);

        if split <= protected_prefix {
            return None;
        }

        let mut summarize_messages: Vec<Message> = messages[protected_prefix..split].to_vec();
        summarize_messages.push(system_message(
            "Summarize the conversation above in a few sentences, preserving \
             important facts, decisions, and outcomes the assistant will \
             still need. Respond with ONLY the summary as plain text."
                .to_string(),
        ));

        let outcome = self.instrumented_call_llm(summarize_messages, Vec::new());
        let summary = match outcome {
            LlmCallOutcome::Completed(Ok(response)) => response.text?,
            LlmCallOutcome::Completed(Err(_)) | LlmCallOutcome::Cancelled => return None,
        };
        if summary.trim().is_empty() {
            return None;
        }

        let turns_compacted = split - protected_prefix;
        let summary_message =
            system_message(format!("Summary of earlier conversation:\n{summary}"));
        messages.splice(protected_prefix..split, std::iter::once(summary_message));

        Some(turns_compacted)
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
