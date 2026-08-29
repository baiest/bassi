use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use windows_sys::Win32::Foundation::{BOOL, FALSE, TRUE};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

use crate::ports::cancellation::CancelSignal;

/// The flag Ctrl+C sets. A `static` because Windows' console control
/// handler is a plain `extern "system" fn` — it can't capture state, so the
/// handler and every `CtrlCCancelSignal` share this one flag. That's fine
/// for nala: one process, one console, one turn cancellable at a time.
static CANCELLED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Reports whether Ctrl+C has been pressed since the signal was armed.
/// Cloning shares the same underlying flag.
#[derive(Clone)]
pub struct CtrlCCancelSignal {
    flag: Arc<AtomicBool>,
}

impl CtrlCCancelSignal {
    /// Registers the process-wide Ctrl+C handler (idempotent: calling this
    /// more than once reuses the same flag instead of registering twice).
    /// Returns `Err` if Windows refuses to install the handler.
    pub fn install() -> std::io::Result<Self> {
        let flag = CANCELLED
            .get_or_init(|| Arc::new(AtomicBool::new(false)))
            .clone();

        let installed = unsafe { SetConsoleCtrlHandler(Some(handle_ctrl_event), TRUE) };

        if installed == FALSE {
            return Err(std::io::Error::last_os_error());
        }

        Ok(Self { flag })
    }

    /// Clears a previously-set cancellation, so the next turn starts fresh.
    pub fn reset(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

impl CancelSignal for CtrlCCancelSignal {
    fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

unsafe extern "system" fn handle_ctrl_event(_ctrl_type: u32) -> BOOL {
    if let Some(flag) = CANCELLED.get() {
        flag.store(true, Ordering::SeqCst);
    }
    // Non-zero tells Windows this handler dealt with the event, so the
    // default action (terminating the process) doesn't run.
    TRUE
}
