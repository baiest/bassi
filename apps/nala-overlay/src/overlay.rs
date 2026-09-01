use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use device_protocol::DeviceState;
use eframe::egui;

use nala_overlay::animation::{arc_offsets, pulse_scale, ring_sweep};
use nala_overlay::color::state_color;

/// How many points make up the drawn ring — enough for a smooth curve on
/// an 80x80 window without doing more math than the shape needs.
const RING_SEGMENTS: usize = 24;

/// Where the PC daemon's overlay channel lives — not configurable, since
/// the channel itself is loopback-only with no auth (see
/// `pc_daemon::overlay_channel`).
const OVERLAY_ADDR: &str = "127.0.0.1:4183";

/// How long to wait before retrying a dropped or failed connection to the
/// daemon. Fixed rather than a backoff: this is a local, low-cost
/// reconnect (unlike the daemon's own reconnect to Nala over the network),
/// so a simple constant delay is enough.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Runs forever on a background thread, keeping `state` in sync with
/// whatever the daemon's overlay channel broadcasts. The overlay never
/// invokes anything and never sends — it's a pure subscriber, so a
/// connection failure just means retrying, never an error surfaced to the
/// UI beyond staying on the last known state.
fn spawn_state_listener() -> Arc<Mutex<DeviceState>> {
    let state = Arc::new(Mutex::new(DeviceState::Idle));
    let listener_state = Arc::clone(&state);

    thread::spawn(move || {
        loop {
            if let Ok((mut socket, _response)) =
                tungstenite::connect(format!("ws://{OVERLAY_ADDR}"))
            {
                loop {
                    match socket.read() {
                        Ok(tungstenite::Message::Text(text)) => {
                            if let Ok(new_state) = serde_json::from_str::<DeviceState>(&text) {
                                *listener_state.lock().unwrap() = new_state;
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
            thread::sleep(RECONNECT_DELAY);
        }
    });

    state
}

struct OverlayApp {
    state: Arc<Mutex<DeviceState>>,
    // Animation is time-driven (see `animation::pulse_scale`/`ring_sweep`),
    // not frame-count-driven, so it looks the same at any frame rate — this
    // is just the clock it's measured against.
    start: Instant,
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent so only the circle is visible — the window
        // itself has no chrome to blend into.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current = *self.state.lock().unwrap();
        let color = state_color(current);
        let elapsed = self.start.elapsed();

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let center = rect.center();
                // Base radius leaves room for the pulse and the ring, which
                // both draw outside the resting circle, so neither clips
                // against the 80x80 window.
                let base_radius = rect.width().min(rect.height()) / 2.0 - 10.0;
                let radius = base_radius * pulse_scale(current, elapsed);
                ui.painter().circle_filled(center, radius, color);

                if let Some((start, sweep)) = ring_sweep(current, elapsed) {
                    let ring_radius = radius + 6.0;
                    let points: Vec<egui::Pos2> =
                        arc_offsets(start, sweep, ring_radius, RING_SEGMENTS)
                            .into_iter()
                            .map(|(x, y)| center + egui::vec2(x, y))
                            .collect();
                    ui.painter()
                        .add(egui::Shape::line(points, egui::Stroke::new(3.0f32, color)));
                }

                // No decorations means no title bar to drag by — clicking
                // and dragging anywhere on the circle moves the window
                // instead, the usual pattern for borderless egui windows.
                let response = ui.interact(
                    rect,
                    egui::Id::new("overlay_drag_area"),
                    egui::Sense::click_and_drag(),
                );
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            });

        // Repaint on a short interval (not only on input) so the pulse/ring
        // animation and any state change delivered on the background
        // thread both show up smoothly.
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

pub fn run() -> eframe::Result<()> {
    let state = spawn_state_listener();

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
        Box::new(|_cc| {
            Ok(Box::new(OverlayApp {
                state,
                start: Instant::now(),
            }))
        }),
    )
}
