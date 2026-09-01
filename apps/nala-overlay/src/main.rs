use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use device_protocol::DeviceState;
use eframe::egui;
use nala_overlay::color::state_color;

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

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let center = rect.center();
                let radius = rect.width().min(rect.height()) / 2.0 - 4.0;
                ui.painter().circle_filled(center, radius, color);

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

        // Repaint on a short interval (not only on input) so a state
        // change delivered on the background thread shows up promptly.
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn main() -> eframe::Result<()> {
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
        Box::new(|_cc| Ok(Box::new(OverlayApp { state }))),
    )
}
