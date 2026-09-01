use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use device_protocol::{CapabilityDefinition, DeviceState, ErrorCode, NalaMessage, Outcome};

use crate::ports::device::RemoteDevice;

/// How long `invoke` waits for the matching `Result` before giving up and
/// reporting a timeout — bounded so a device that stops answering (crashed,
/// unplugged, wedged) never hangs a turn forever.
const DEFAULT_INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// The outbound half of a device connection: sending it a `NalaMessage`.
/// A trait (rather than a concrete socket type) so `WsDevice` can be tested
/// against an in-memory fake, and so the real implementation can be a
/// cheaply-cloned handle shared between the connection's reading thread and
/// every `WsDevice` clone that wants to invoke a capability on it.
pub trait DeviceSink: Clone + Send {
    fn send(&self, message: &NalaMessage) -> Result<(), String>;
}

/// Requests waiting on a `Result` from the device, keyed by `request_id`.
/// Shared between every `WsDevice` clone (so any of them can register a
/// wait) and the connection's reading thread (which delivers results into
/// it as they arrive over the wire).
#[derive(Default)]
struct Pending {
    waiters: HashMap<String, mpsc::Sender<Outcome>>,
}

impl Pending {
    /// Routes `outcome` to whichever `invoke` call is waiting on
    /// `request_id`. A `request_id` with no matching waiter — already timed
    /// out, or never ours to begin with — is silently dropped rather than
    /// treated as an error: a device replying late or replying twice must
    /// never crash or misroute to some *other* in-flight call.
    fn deliver(&mut self, request_id: &str, outcome: Outcome) {
        if let Some(sender) = self.waiters.remove(request_id) {
            let _ = sender.send(outcome);
        }
    }
}

/// A `RemoteDevice` backed by a real (or faked) connection. Cheap to clone —
/// every clone shares the same underlying sink and pending-request table —
/// so each turn-client's `Assistant` can hold its own handle to the same
/// physical device.
pub struct WsDevice<S: DeviceSink> {
    name: String,
    capabilities: Vec<CapabilityDefinition>,
    sink: S,
    pending: Arc<Mutex<Pending>>,
    next_request_id: Arc<AtomicU64>,
    timeout: Duration,
}

impl<S: DeviceSink> Clone for WsDevice<S> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            capabilities: self.capabilities.clone(),
            sink: self.sink.clone(),
            pending: Arc::clone(&self.pending),
            next_request_id: Arc::clone(&self.next_request_id),
            timeout: self.timeout,
        }
    }
}

impl<S: DeviceSink> WsDevice<S> {
    pub fn new(name: String, capabilities: Vec<CapabilityDefinition>, sink: S) -> Self {
        Self::with_timeout(name, capabilities, sink, DEFAULT_INVOKE_TIMEOUT)
    }

    pub fn with_timeout(
        name: String,
        capabilities: Vec<CapabilityDefinition>,
        sink: S,
        timeout: Duration,
    ) -> Self {
        Self {
            name,
            capabilities,
            sink,
            pending: Arc::new(Mutex::new(Pending::default())),
            next_request_id: Arc::new(AtomicU64::new(0)),
            timeout,
        }
    }

    /// Called by the connection's reading thread when a `Result` arrives
    /// over the wire, to hand it to whichever `invoke` call is waiting on
    /// it (if any — see `Pending::deliver`).
    pub fn deliver_result(&self, request_id: &str, outcome: Outcome) {
        self.pending.lock().unwrap().deliver(request_id, outcome);
    }
}

impl<S: DeviceSink> RemoteDevice for WsDevice<S> {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &[CapabilityDefinition] {
        &self.capabilities
    }

    fn invoke(&mut self, capability: &str, arguments: &str) -> Outcome {
        let request_id = self
            .next_request_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();

        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .unwrap()
            .waiters
            .insert(request_id.clone(), sender);

        let message = NalaMessage::Invoke {
            request_id: request_id.clone(),
            capability: capability.to_string(),
            arguments: arguments.to_string(),
        };

        if let Err(error) = self.sink.send(&message) {
            self.pending.lock().unwrap().waiters.remove(&request_id);
            return Outcome::Err {
                code: ErrorCode::Failed,
                message: format!("could not reach device '{}': {error}", self.name),
            };
        }

        match receiver.recv_timeout(self.timeout) {
            Ok(outcome) => outcome,
            Err(_) => {
                self.pending.lock().unwrap().waiters.remove(&request_id);
                Outcome::Err {
                    code: ErrorCode::Timeout,
                    message: format!("device '{}' did not respond in time", self.name),
                }
            }
        }
    }

    fn push_state(&self, state: DeviceState) {
        // Best-effort: nothing waits on this, and a device that can't be
        // reached right now will get the *next* state push anyway — no
        // retry or error reporting needed for a fire-and-forget notice.
        let _ = self.sink.send(&NalaMessage::State { state });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[derive(Clone, Default)]
    struct FakeSink {
        sent: Arc<Mutex<Vec<NalaMessage>>>,
    }

    impl FakeSink {
        fn sent(&self) -> Vec<NalaMessage> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl DeviceSink for FakeSink {
        fn send(&self, message: &NalaMessage) -> Result<(), String> {
            self.sent.lock().unwrap().push(message.clone());
            Ok(())
        }
    }

    fn wait_for_one_sent_message(sink: &FakeSink) -> NalaMessage {
        for _ in 0..1000 {
            if let Some(message) = sink.sent().into_iter().next() {
                return message;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("no message was sent within the deadline");
    }

    #[test]
    fn an_invocation_that_never_answers_times_out() {
        let sink = FakeSink::default();
        let mut device =
            WsDevice::with_timeout("pc".to_string(), vec![], sink, Duration::from_millis(20));

        let outcome = device.invoke("open_app", "{}");

        assert!(matches!(
            outcome,
            Outcome::Err {
                code: ErrorCode::Timeout,
                ..
            }
        ));
    }

    #[test]
    fn a_result_with_an_unknown_request_id_is_ignored_not_matched() {
        let sink = FakeSink::default();
        let device = WsDevice::with_timeout(
            "pc".to_string(),
            vec![],
            sink.clone(),
            Duration::from_secs(5),
        );

        let mut invoking_device = device.clone();
        let handle = thread::spawn(move || invoking_device.invoke("open_app", "{}"));

        let sent = wait_for_one_sent_message(&sink);
        let real_request_id = match sent {
            NalaMessage::Invoke { request_id, .. } => request_id,
            other => panic!("expected Invoke, got {other:?}"),
        };

        // A result for some other request (already timed out, or never
        // ours) must not be mistaken for this one.
        device.deliver_result(
            "not-the-real-id",
            Outcome::Ok {
                text: "wrong".to_string(),
                mutated: false,
            },
        );
        device.deliver_result(
            &real_request_id,
            Outcome::Ok {
                text: "right".to_string(),
                mutated: true,
            },
        );

        let outcome = handle.join().expect("invoke thread should not panic");
        match outcome {
            Outcome::Ok { text, mutated } => {
                assert_eq!(text, "right");
                assert!(mutated);
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn a_sink_failure_becomes_a_failed_outcome_not_a_panic() {
        #[derive(Clone)]
        struct AlwaysFailsSink;
        impl DeviceSink for AlwaysFailsSink {
            fn send(&self, _message: &NalaMessage) -> Result<(), String> {
                Err("connection reset".to_string())
            }
        }

        let mut device = WsDevice::new("pc".to_string(), vec![], AlwaysFailsSink);

        let outcome = device.invoke("open_app", "{}");

        assert!(matches!(
            outcome,
            Outcome::Err {
                code: ErrorCode::Failed,
                ..
            }
        ));
    }
}
