use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use tungstenite::WebSocket;
use tungstenite::stream::MaybeTlsStream;

use nala_overlay::amplitude::amplitude_from_samples;
use nala_overlay::color::status_color;
use nala_overlay::playback::{self, ClipPlayer};
use nala_overlay::status::Status;
use nala_overlay::voice_client::{VoiceConnection, run_clip_loop};

/// Where `voice --serve`'s audio WebSocket lives, overridable with
/// `NALA_VOICE_ADDR` — matches `voice`'s own default.
const DEFAULT_VOICE_ADDR: &str = "127.0.0.1:4181";

/// How long to wait before retrying a dropped or failed connection.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// How often the clip-reading thread polls for the next message. A
/// blocking read here would hold `Shared::connection`'s lock indefinitely
/// (nothing arrives from `voice` until it has a clip to send), starving
/// `record_and_send`'s attempt to lock the same connection to send an
/// utterance — same reasoning as `nala::device_server`'s own
/// `READ_POLL_INTERVAL`.
const READ_POLL_INTERVAL: Duration = Duration::from_millis(100);

type Socket = WebSocket<MaybeTlsStream<std::net::TcpStream>>;

/// Everything the UI thread shares with the recording/networking
/// background threads: the current status, the live amplitude to pulse
/// to, whether the mouse is currently held down over the circle, and the
/// connection to send a finished recording on.
struct Shared {
    // Both `status` and `amplitude` are `Arc`s of their own (not just
    // `Mutex` fields) so `playback::spawn` and the recording thread's
    // closure can hold the exact same cells `OverlayApp` reads from —
    // one status/amplitude pair, whichever of "listening" or "speaking"
    // is currently driving it.
    status: Arc<Mutex<Status>>,
    amplitude: Arc<Mutex<f32>>,
    is_held: AtomicBool,
    connection: Mutex<Option<Arc<Mutex<Socket>>>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(Status::Idle)),
            amplitude: Arc::new(Mutex::new(0.0)),
            is_held: AtomicBool::new(false),
            connection: Mutex::new(None),
        }
    }

    fn set_status(&self, status: Status) {
        *self.status.lock().unwrap() = status;
    }
}

/// Connects to `voice --serve`, retrying forever on failure/drop. Each
/// successful connection gets its own clip-reading thread (pushing every
/// received clip to `player`) and is published to `shared.connection` so
/// the recording side can send an utterance on it.
fn spawn_connection_manager(shared: Arc<Shared>, voice_addr: String, player: Arc<ClipPlayer>) {
    thread::spawn(move || {
        loop {
            if let Ok((mut socket, _response)) = tungstenite::connect(format!("ws://{voice_addr}"))
            {
                if let MaybeTlsStream::Plain(tcp) = socket.get_mut()
                    && let Err(error) = tcp.set_read_timeout(Some(READ_POLL_INTERVAL))
                {
                    eprintln!(
                        "Warning: could not set a read timeout on the voice connection: {error}"
                    );
                }
                let socket = Arc::new(Mutex::new(socket));
                *shared.connection.lock().unwrap() = Some(Arc::clone(&socket));

                // Blocks (on this thread) until the connection closes or
                // errors, pushing every clip that arrives to `player`.
                run_clip_loop_shared(&socket, &player);

                *shared.connection.lock().unwrap() = None;
            }
            thread::sleep(RECONNECT_DELAY);
        }
    });
}

fn run_clip_loop_shared(socket: &Arc<Mutex<Socket>>, player: &Arc<ClipPlayer>) {
    // `Arc<Mutex<Socket>>` itself implements `VoiceConnection` (see
    // voice_client.rs), locking only for the duration of each individual
    // `poll()`/`send_utterance()` call rather than holding it for the
    // whole loop — each `poll()` still returns within `READ_POLL_INTERVAL`
    // (the read timeout set where this socket was connected) instead of
    // blocking indefinitely, so `record_and_send`'s send gets a chance to
    // acquire the lock between polls instead of starving forever. Status
    // while a clip plays is owned by `playback::spawn`, not here.
    let mut connection = Arc::clone(socket);
    run_clip_loop(&mut connection, |clip| player.enqueue(clip));
}

/// Records while `shared.is_held` stays true, updating `shared.amplitude`
/// live, then encodes and sends the result — runs on its own thread so the
/// UI stays responsive while recording.
fn record_and_send(shared: Arc<Shared>) {
    shared.set_status(Status::Listening);

    let recording_amplitude = Arc::clone(&shared);
    let result = stt::record_while(
        || shared.is_held.load(Ordering::Relaxed),
        move |chunk| {
            let samples: Vec<i16> = chunk
                .iter()
                .map(|&sample| (sample * i16::MAX as f32) as i16)
                .collect();
            *recording_amplitude.amplitude.lock().unwrap() = amplitude_from_samples(&samples);
        },
    );
    *shared.amplitude.lock().unwrap() = 0.0;

    let audio = match result {
        Ok(audio) => audio,
        Err(error) => {
            eprintln!("Warning: could not record from the microphone: {error}");
            shared.set_status(Status::Error);
            return;
        }
    };

    shared.set_status(Status::Sending);
    let i16_samples: Vec<i16> = audio
        .samples
        .iter()
        .map(|&sample| (sample * i16::MAX as f32) as i16)
        .collect();
    let wav = voice::wav::encode_wav(&i16_samples, audio.sample_rate, 1);

    let connection = shared.connection.lock().unwrap().clone();
    match connection {
        Some(socket) => {
            if let Err(error) = socket.lock().unwrap().send_utterance(wav) {
                eprintln!("Warning: could not send the recording to voice: {error}");
            }
        }
        None => eprintln!("Warning: not connected to voice — recording dropped."),
    }

    shared.set_status(Status::Idle);
}

struct OverlayApp {
    shared: Arc<Shared>,
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let status = *self.shared.status.lock().unwrap();
        let amplitude = *self.shared.amplitude.lock().unwrap();
        let color = status_color(status);

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let center = rect.center();
                let base_radius = rect.width().min(rect.height()) / 2.0 - 4.0;
                // The circle grows with real amplitude while listening or
                // speaking — 0.0 the rest of the time leaves it at rest.
                let radius = base_radius * (1.0 + amplitude * 0.3);
                ui.painter().circle_filled(center, radius, color);

                let response = ui.interact(
                    rect,
                    egui::Id::new("overlay_interact_area"),
                    egui::Sense::click_and_drag(),
                );
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // A plain click (no drag) starts recording; clicking again
                // stops it and sends what was recorded. Pressing-and-holding
                // is handed off to the OS as a window drag instead (see
                // above), so click-to-toggle is the gesture that's actually
                // reliable here.
                if response.clicked() {
                    let was_held = self.shared.is_held.swap(
                        !self.shared.is_held.load(Ordering::Relaxed),
                        Ordering::Relaxed,
                    );
                    if !was_held {
                        let shared = Arc::clone(&self.shared);
                        thread::spawn(move || record_and_send(shared));
                    }
                }
            });

        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

pub fn run() -> eframe::Result<()> {
    let voice_addr =
        std::env::var("NALA_VOICE_ADDR").unwrap_or_else(|_| DEFAULT_VOICE_ADDR.to_string());

    let shared = Arc::new(Shared::new());
    let player = Arc::new(playback::spawn(
        Arc::clone(&shared.amplitude),
        Arc::clone(&shared.status),
    ));
    spawn_connection_manager(Arc::clone(&shared), voice_addr, Arc::clone(&player));

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([80.0, 80.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Nala Overlay",
        options,
        Box::new(|_cc| Ok(Box::new(OverlayApp { shared }))),
    )
}
