//! Wire protocol and event types shared between `nala` (the agent, run as a
//! server) and any client that talks to it over a connection — today
//! `voice`, over a synchronous WebSocket. Kept dependency-free of both so
//! neither has to depend on the other just to exchange these types.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Identifies one `process()` call, so every event it emits — LLM calls,
/// tool calls, budget pressure, completion — can be correlated back to the
/// task that produced it (e.g. when writing them out as rows in a CSV keyed
/// by task).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// A process-wide unique id: current time in milliseconds plus a
    /// monotonic counter, so two tasks started in the same millisecond still
    /// get distinct ids. No UUID dependency needed — Nala is single-process,
    /// single-user, so global (cross-process) uniqueness isn't required.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("{millis}-{sequence}"))
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies a single LLM call within a task: `{task_id}-{call_index}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LlmCallId(String);

impl LlmCallId {
    pub fn new(task_id: &TaskId, call_index: u32) -> Self {
        Self(format!("{task_id}-{call_index}"))
    }
}

impl std::fmt::Display for LlmCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The phase the agent loop is currently in, for surfacing progress to the
/// UI (e.g. "Nala is thinking..." vs "Nala is executing..."). Transitions
/// are emitted as `Event::StateChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnState {
    /// The user's input has been received and the turn is starting.
    Receiving,
    /// Generating the up-front step-by-step plan.
    Planning,
    /// Waiting on the LLM to decide the next action.
    Thinking,
    /// Running one or more tool calls the LLM requested.
    Executing,
    /// Mutating tool calls happened this turn and their effect hasn't been
    /// independently confirmed yet.
    Verifying,
    /// Producing the final natural-language answer.
    Responding,
}

/// Where a request originated, for per-client metrics breakdown. Carried on
/// `ClientMessage::Input` and `Event::RequestStarted` — see
/// `apps/voice/src/session.rs` for why it has to travel in the message
/// rather than being inferred from the connection (one shared Nala
/// connection serves every client).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestSource {
    Cli,
    Overlay,
    Android,
    Voice,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    RequestStarted {
        task_id: TaskId,
        prompt: String,
        source: RequestSource,
    },
    StateChanged {
        task_id: TaskId,
        state: TurnState,
    },
    RequestCompleted {
        task_id: TaskId,
        duration: Duration,
        reply: String,
    },
    RequestFailed {
        task_id: TaskId,
        duration: Duration,
        error: String,
    },

    /// The step-by-step plan the assistant generated for this request,
    /// before executing anything, from the user's request plus the
    /// available tools and computer context.
    PlanCreated {
        task_id: TaskId,
        plan: String,
    },

    LlmStarted {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        /// This call's position within the task (1, 2, 3, ...), across
        /// every LLM call the task makes — the main loop, planning, and
        /// compaction summaries all share the same sequence.
        call_index: u32,
        /// Total images attached across the messages sent in this call
        /// (e.g. a screenshot from an earlier tool result), so it's visible
        /// from the outside that an image actually reached the model.
        images: usize,
        provider: String,
        model: String,
    },
    LlmCompleted {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        call_index: u32,
        duration: Duration,
        provider: String,
        model: String,
    },
    /// A single LLM call failed — as opposed to `RequestFailed`, which marks
    /// the whole task giving up. One task can retry several failed calls
    /// (or absorb one in `build_plan`/`compact`, both non-fatal) without the
    /// task itself failing, so the two need separate events.
    LlmFailed {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        call_index: u32,
        duration: Duration,
        error: String,
        provider: String,
        model: String,
    },

    ToolStarted {
        task_id: TaskId,
        /// This tool call's position within the task (1, 2, 3, ...), across
        /// every tool call the task makes.
        tool_call_index: u32,
        name: String,
        arguments: String,
    },
    ToolCompleted {
        task_id: TaskId,
        tool_call_index: u32,
        name: String,
        duration: Duration,
        output: String,
        /// How many images the tool result carried (e.g. a screenshot).
        images: usize,
        arguments: String,
        mutated: bool,
    },

    /// A retryable LLM failure is about to be retried after a backoff
    /// delay.
    Retrying {
        task_id: TaskId,
        attempt: u32,
        error: String,
    },

    /// The turn was stopped because cancellation was requested (e.g.
    /// Ctrl+C) rather than because it finished or hit a limit.
    Cancelled {
        task_id: TaskId,
    },

    /// Real token accounting for a completed LLM call, when the backend
    /// reports it. Distinct from `LlmCompleted`'s `duration` — this is
    /// about context budget, not latency.
    TokensUsed {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        call_index: u32,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    },

    /// The prompt was over budget before a call and had to be trimmed.
    /// `remaining_estimate` is the estimated token count *after* this step,
    /// so repeated pressure events show whether trimming is converging.
    BudgetPressure {
        task_id: TaskId,
        step: BudgetStep,
        remaining_estimate: usize,
    },

    /// Older turns were summarized into a single message to free up budget,
    /// after evicting images/text/whole turns wasn't enough on its own.
    TranscriptCompacted {
        task_id: TaskId,
        turns_compacted: usize,
    },

    /// The model tried to end the turn with a mutating tool call still
    /// unverified. The loop nudges it to check first (see
    /// `agent_loop.rs`'s verification gate) — this event marks the one
    /// case where that nudge is skipped and the answer is let through
    /// anyway, so the turn can't get stuck nagging forever.
    AnsweredUnverified {
        task_id: TaskId,
    },

    /// Sent once, right when a client connects — before any turn — so Nala
    /// is the one greeting, not each client deciding its own opening line.
    /// Not tied to a task: no `task_id`.
    Greeting {
        text: String,
    },
}

/// Which eviction step fired in a `BudgetPressure` event, in the order the
/// budget fitter tries them — cheapest/least-lossy first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetStep {
    /// Images dropped from tool results older than the most recent few.
    DroppedImages { count: usize },
    /// Long tool-result text truncated to a head/tail excerpt.
    TruncatedText { count: usize },
    /// Whole oldest turns (a tool call and its result) dropped.
    DroppedTurns { count: usize },
}

pub trait EventSink {
    fn emit(&mut self, event: Event);
}

/// A message a client (e.g. `voice`) sends to the agent server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// One turn's worth of user input, already converted to text — the
    /// agent never receives raw audio.
    Input { text: String },
    /// Ask the current turn to stop early, mirroring the local Ctrl+C
    /// behavior.
    Cancel,
}

/// A message the agent server sends back to a client, zero or more times
/// per turn (progress events) followed by exactly one `Reply` or `Error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Progress narration for the turn currently in flight.
    Event(Event),
    /// The turn finished successfully with this text.
    Reply { text: String },
    /// The turn failed; `message` is human-readable, not meant to be parsed.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_input_round_trips_through_json() {
        let message = ClientMessage::Input {
            text: "hola nala".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            ClientMessage::Input { text } => assert_eq!(text, "hola nala"),
            ClientMessage::Cancel => panic!("expected Input"),
        }
    }

    #[test]
    fn client_message_cancel_round_trips_through_json() {
        let json = serde_json::to_string(&ClientMessage::Cancel).unwrap();
        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();

        assert!(matches!(decoded, ClientMessage::Cancel));
    }

    #[test]
    fn a_greeting_event_round_trips_through_json_wrapped_in_a_server_message() {
        let message = ServerMessage::Event(Event::Greeting {
            text: "Hola, en que te puedo ayudar?".to_string(),
        });

        let json = serde_json::to_string(&message).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            ServerMessage::Event(Event::Greeting { text }) => {
                assert_eq!(text, "Hola, en que te puedo ayudar?")
            }
            _ => panic!("expected Event(Greeting)"),
        }
    }

    #[test]
    fn server_message_reply_round_trips_through_json() {
        let message = ServerMessage::Reply {
            text: "listo".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            ServerMessage::Reply { text } => assert_eq!(text, "listo"),
            _ => panic!("expected Reply"),
        }
    }

    #[test]
    fn server_message_error_round_trips_through_json() {
        let json = serde_json::to_string(&ServerMessage::Error {
            message: "boom".to_string(),
        })
        .unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            ServerMessage::Error { message } => assert_eq!(message, "boom"),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn request_started_carries_the_prompt_and_its_source() {
        let task_id = TaskId::new();
        let event = Event::RequestStarted {
            task_id: task_id.clone(),
            prompt: "hola nala".to_string(),
            source: RequestSource::Android,
        };

        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();

        match decoded {
            Event::RequestStarted {
                task_id: decoded_task_id,
                prompt,
                source,
            } => {
                assert_eq!(decoded_task_id, task_id);
                assert_eq!(prompt, "hola nala");
                assert_eq!(source, RequestSource::Android);
            }
            _ => panic!("expected RequestStarted"),
        }
    }

    #[test]
    fn llm_started_carries_the_provider_and_model() {
        let task_id = TaskId::new();
        let event = Event::LlmStarted {
            llm_call_id: LlmCallId::new(&task_id, 1),
            task_id,
            call_index: 1,
            images: 0,
            provider: "ollama".to_string(),
            model: "gemma4:12b".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();

        match decoded {
            Event::LlmStarted {
                provider, model, ..
            } => {
                assert_eq!(provider, "ollama");
                assert_eq!(model, "gemma4:12b");
            }
            _ => panic!("expected LlmStarted"),
        }
    }

    #[test]
    fn tool_completed_carries_its_arguments_and_whether_it_mutated_anything() {
        let task_id = TaskId::new();
        let event = Event::ToolCompleted {
            task_id,
            tool_call_index: 1,
            name: "get_weather".to_string(),
            duration: Duration::from_millis(10),
            output: "sunny".to_string(),
            images: 0,
            arguments: "{\"city\":\"Cali\"}".to_string(),
            mutated: false,
        };

        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();

        match decoded {
            Event::ToolCompleted {
                arguments, mutated, ..
            } => {
                assert_eq!(arguments, "{\"city\":\"Cali\"}");
                assert!(!mutated);
            }
            _ => panic!("expected ToolCompleted"),
        }
    }

    #[test]
    fn request_completed_carries_the_final_reply_text() {
        let task_id = TaskId::new();
        let event = Event::RequestCompleted {
            task_id,
            duration: Duration::from_millis(10),
            reply: "listo".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();

        match decoded {
            Event::RequestCompleted { reply, .. } => assert_eq!(reply, "listo"),
            _ => panic!("expected RequestCompleted"),
        }
    }

    #[test]
    fn server_message_event_with_a_payload_round_trips_through_json() {
        let task_id = TaskId::new();
        let event = Event::LlmStarted {
            llm_call_id: LlmCallId::new(&task_id, 1),
            task_id: task_id.clone(),
            call_index: 1,
            images: 2,
            provider: "ollama".to_string(),
            model: "gemma4:12b".to_string(),
        };

        let json = serde_json::to_string(&ServerMessage::Event(event)).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            ServerMessage::Event(Event::LlmStarted {
                task_id: decoded_task_id,
                call_index,
                images,
                ..
            }) => {
                assert_eq!(decoded_task_id, task_id);
                assert_eq!(call_index, 1);
                assert_eq!(images, 2);
            }
            _ => panic!("expected Event(LlmStarted)"),
        }
    }
}
