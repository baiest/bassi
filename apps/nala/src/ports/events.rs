use std::time::Duration;

#[derive(Debug)]
pub enum Event {
    RequestStarted,
    RequestCompleted {
        duration: Duration,
    },
    RequestFailed {
        duration: Duration,
        error: String,
    },

    /// The step-by-step plan the assistant generated for this request,
    /// before executing anything, from the user's request plus the
    /// available tools and computer context.
    PlanCreated {
        plan: String,
    },

    LlmStarted {
        /// Total images attached across the messages sent in this call
        /// (e.g. a screenshot from an earlier tool result), so it's visible
        /// from the outside that an image actually reached the model.
        images: usize,
    },
    LlmCompleted {
        duration: Duration,
    },

    ToolStarted {
        name: String,
        arguments: String,
    },
    ToolCompleted {
        name: String,
        duration: Duration,
        output: String,
        /// How many images the tool result carried (e.g. a screenshot).
        images: usize,
    },
}

pub trait EventSink {
    fn emit(&mut self, event: Event);
}
