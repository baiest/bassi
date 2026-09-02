use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use tungstenite::WebSocket;
use tungstenite::stream::MaybeTlsStream;

use nala_overlay::amplitude::amplitude_from_samples;
use nala_overlay::color::{accent_color, glow_color, status_color};
use nala_overlay::config;
use nala_overlay::motion;
use nala_overlay::playback::{self, ClipPlayer};
use nala_overlay::scene::{self, Point3};
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

/// Fraction of the window's half-size reserved as empty margin around the
/// scene, so the core's halo never touches (and gets clipped by) the
/// window edge even at full amplitude.
const SAFE_MARGIN: f32 = 0.14;

/// How much louder amplitude inflates the core's radius, as a fraction of
/// the core's resting radius. At `pulse == 1.0` the core is
/// `1.0 + PULSE_GAIN` times its resting size — high enough that speaking
/// produces an obviously bigger core, not just a few pixels of wobble.
const PULSE_GAIN: f32 = 1.2;

/// The core's resting radius, as a fraction of the scene radius. Kept
/// small on purpose: `CORE_RADIUS_FRACTION * (1.0 + PULSE_GAIN) *
/// HALO_RADIUS_FACTOR` must stay under 1.0, or the halo would clip against
/// `scene_radius`'s margin at full amplitude.
const CORE_RADIUS_FRACTION: f32 = 0.28;

/// The halo's radius, as a multiple of the (amplitude-inflated) core
/// radius.
const HALO_RADIUS_FACTOR: f32 = 1.6;

/// How much amplitude also expands the sphere/ring point cloud, as a
/// fraction of `scene_radius` — smaller than `PULSE_GAIN` since
/// `scene_radius` has less headroom (`SAFE_MARGIN`) to spend than the core
/// does.
const SCENE_PULSE_GAIN: f32 = 0.1;

/// How quickly the displayed amplitude catches up to the real one — small
/// enough to smooth out per-window jitter, large enough to still feel
/// responsive to speech.
const AMPLITUDE_HALF_LIFE: f32 = 0.08;

/// Radians/second the sphere and rings rotate around Y at rest.
const YAW_SPEED: f32 = 0.35;

/// Radians/second the sphere and rings rotate around X at rest — slower
/// than yaw so the tumble doesn't look mechanical.
const PITCH_SPEED: f32 = 0.13;

/// Smallest on-screen radius (in points) a sphere/ring point is drawn at,
/// so far points stay visible instead of anti-aliasing away to nothing.
const MIN_POINT_RADIUS: f32 = 1.0;

/// Largest on-screen radius (in points) a sphere/ring point is drawn at.
const MAX_POINT_RADIUS: f32 = 2.6;

/// How many egui points of scroll map to one point of window size change.
const SCROLL_SENSITIVITY: f32 = 0.5;

/// How often the current window size is written to disk while the user is
/// actively scrolling to resize — anything shorter just wears the disk for
/// no visible benefit.
const SIZE_SAVE_INTERVAL: Duration = Duration::from_millis(500);

struct OverlayApp {
    shared: Arc<Shared>,
    sphere: Vec<Point3>,
    rings: Vec<Vec<Point3>>,
    smoothed_amplitude: f32,
    yaw: f32,
    pitch: f32,
    elapsed: f32,
    size: f32,
    last_size_save: Option<std::time::Instant>,
}

impl OverlayApp {
    fn new(shared: Arc<Shared>, size: f32) -> Self {
        Self {
            shared,
            sphere: scene::sphere_points(scene::SPHERE_POINTS),
            rings: scene::RING_TILTS
                .iter()
                .map(|&tilt| scene::ring_points(scene::RING_POINTS, tilt))
                .collect(),
            smoothed_amplitude: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            elapsed: 0.0,
            size,
            last_size_save: None,
        }
    }

    /// Applies pending scroll input as a window resize, anchored on the
    /// window's center rather than its top-left corner. Persists the new
    /// size to disk, but no more often than `SIZE_SAVE_INTERVAL` so a burst
    /// of scroll events doesn't hammer the filesystem.
    fn handle_resize(&mut self, ctx: &egui::Context) {
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll == 0.0 {
            return;
        }

        let previous_size = self.size;
        self.size =
            (self.size + scroll * SCROLL_SENSITIVITY).clamp(config::MIN_SIZE, config::MAX_SIZE);
        if self.size == previous_size {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            self.size, self.size,
        )));

        // Growing/shrinking from the center, not the top-left corner, needs
        // the window's current on-screen position — best-effort, since not
        // every platform reports it every frame.
        if let Some(outer_rect) = ctx.input(|i| i.viewport().outer_rect) {
            let delta = (self.size - previous_size) / 2.0;
            let new_pos = outer_rect.min - egui::vec2(delta, delta);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(new_pos));
        }

        let should_save = self
            .last_size_save
            .is_none_or(|last| last.elapsed() >= SIZE_SAVE_INTERVAL);
        if should_save {
            config::save(self.size);
            self.last_size_save = Some(std::time::Instant::now());
        }
    }

    /// Draws one layer of points (sphere or ring), each rotated by the
    /// current yaw/pitch, projected, and depth-sorted so nearer points
    /// paint over farther ones.
    fn draw_points(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        radius: f32,
        color: egui::Color32,
    ) {
        let projected: Vec<_> = self
            .sphere
            .iter()
            .chain(self.rings.iter().flatten())
            .map(|&p| scene::rotate(p, self.yaw, self.pitch))
            .map(|p| scene::project(p, radius, scene::PERSPECTIVE))
            .collect();

        for point in scene::depth_sorted(projected) {
            // Farther points (scale < 1.0) shrink and fade; nearer points
            // (scale > 1.0) grow slightly — this is what reads as "3D"
            // rather than a flat ring of dots.
            let point_radius =
                (MIN_POINT_RADIUS * point.scale).clamp(MIN_POINT_RADIUS, MAX_POINT_RADIUS);
            let alpha = ((point.scale - 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            let point_color = egui::Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                alpha.max(40),
            );
            painter.circle_filled(
                center + egui::vec2(point.pos.0, point.pos.1),
                point_radius,
                point_color,
            );
        }
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let status = *self.shared.status.lock().unwrap();
        let amplitude = *self.shared.amplitude.lock().unwrap();
        let core_color = status_color(status);
        let ring_color = accent_color(status);
        let halo_color = glow_color(status);

        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.elapsed += dt;
        self.smoothed_amplitude =
            motion::smooth(self.smoothed_amplitude, amplitude, dt, AMPLITUDE_HALF_LIFE);
        self.yaw += YAW_SPEED * dt;
        self.pitch = (PITCH_SPEED * self.elapsed).sin() * 0.3;

        self.handle_resize(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let center = rect.center();
                let scene_radius = rect.width().min(rect.height()) / 2.0 * (1.0 - SAFE_MARGIN);

                // In `Idle`, breathe gently instead of sitting frozen; while
                // listening/speaking the real amplitude drives the pulse
                // instead.
                let pulse = if status == Status::Idle {
                    motion::breathe(self.elapsed) * 0.5
                } else {
                    self.smoothed_amplitude
                };
                let core_radius = scene_radius * CORE_RADIUS_FRACTION * (1.0 + pulse * PULSE_GAIN);
                let points_radius = scene_radius * (1.0 + pulse * SCENE_PULSE_GAIN);

                let painter = ui.painter();

                self.draw_points(painter, center, points_radius, ring_color);
                painter.circle_filled(center, core_radius * HALO_RADIUS_FACTOR, halo_color);
                painter.circle_filled(center, core_radius, core_color);

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

        // Continuous repaint, paced by the compositor, instead of a fixed
        // ~30 FPS sleep — combined with `smooth`'s framerate-independent
        // easing, this is what makes the animation read as fluid instead of
        // stepping between amplitude windows.
        ctx.request_repaint();
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

    let size = config::load();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([size, size])
        .with_resizable(false)
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
        Box::new(move |_cc| Ok(Box::new(OverlayApp::new(shared, size)))),
    )
}
