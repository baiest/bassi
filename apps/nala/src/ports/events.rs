use std::time::Duration;

#[derive(Debug)]
pub enum Event {
    RequestStarted,
    RequestCompleted { duration: Duration },
    RequestFailed { duration: Duration, error: String },

    LlmStarted,
    LlmCompleted { duration: Duration },

    ToolStarted { name: String, arguments: String },
    ToolCompleted { name: String, duration: Duration },
}

pub trait EventSink {
    fn emit(&mut self, event: Event);
}
